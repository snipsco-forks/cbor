use std::cmp;
use std::io::{self, Read as StdRead};

use error::{Result, Error, ErrorCode};

use sealingslice::SealingSlice;

/// Trait used by the deserializer for iterating over input.
///
/// This trait is sealed and cannot be implemented for types outside of `serde_cbor`.
pub trait Read<'de>: private::Sealed {
    #[doc(hidden)]
    fn next(&mut self) -> io::Result<Option<u8>>;
    #[doc(hidden)]
    fn peek(&mut self) -> io::Result<Option<u8>>;

    #[doc(hidden)]
    /// Read n bytes from the input.
    ///
    /// Implementations that can are asked to return a slice with a Long lifetime that outlives the
    /// decoder, but others (eg. ones that need to allocate the data into a temporary buffer) can
    /// return it with a Short lifetime that just lives for the time of read's mutable borrow of
    /// the reader.
    ///
    /// This may, as a side effect, clear the reader's scratch buffer (as the provided
    /// implementation does).
    ///
    /// A more appropriate lifetime setup for this (that would allow the Deserializer::convert_str
    /// to stay a function) would be something like `fn read<'a, 'r: 'a>(&'a mut 'r immut self, ...) -> ...
    /// EitherLifetime<'r, 'de>>`, which borrows self mutably for the duration of the function and
    /// downgrates that reference to an immutable one that outlives the result (protecting the
    /// scratch buffer from changes), but alas, that can't be expressed (yet?).
    fn read<'a>(
        &'a mut self,
        n: usize,
    ) -> Result<EitherLifetime<'a, 'de>> {
        self.clear_buffer();
        self.read_to_buffer(n)?;

        Ok(self.view_buffer())
    }

    #[doc(hidden)]
    fn clear_buffer(&mut self);

    #[doc(hidden)]
    /// Append n bytes from the reader to the reader's scratch buffer (without clearing it)
    fn read_to_buffer(&mut self, n: usize) -> Result<()>;

    #[doc(hidden)]
    fn view_buffer<'a>(&'a self) -> EitherLifetime<'a, 'de>;

    #[doc(hidden)]
    fn read_into(&mut self, buf: &mut [u8]) -> Result<()>;

    #[doc(hidden)]
    fn discard(&mut self);

    #[doc(hidden)]
    fn offset(&self) -> u64;
}

pub enum EitherLifetime<'short, 'long> {
    Short(&'short [u8]),
    Long(&'long [u8]),
}

mod private {
    pub trait Sealed {}
}

/// CBOR input source that reads from a std::io input stream.
pub struct IoRead<R>
where
    R: io::Read,
{
    reader: OffsetReader<R>,
    scratch: Vec<u8>,
    ch: Option<u8>,
}

impl<R> IoRead<R>
where
    R: io::Read,
{
    /// Creates a new CBOR input source to read from a std::io input stream.
    pub fn new(reader: R) -> IoRead<R> {
        IoRead {
            reader: OffsetReader {
                reader,
                offset: 0,
            },
            scratch: vec![],
            ch: None,
        }
    }

    #[inline]
    fn next_inner(&mut self) -> io::Result<Option<u8>> {
        let mut buf = [0; 1];
        loop {
            match self.reader.read(&mut buf) {
                Ok(0) => return Ok(None),
                Ok(_) => return Ok(Some(buf[0])),
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }
}

impl<R> private::Sealed for IoRead<R>
where
    R: io::Read,
{
}

impl<'de, R> Read<'de> for IoRead<R>
where
    R: io::Read,
{
    #[inline]
    fn next(&mut self) -> io::Result<Option<u8>> {
        match self.ch.take() {
            Some(ch) => Ok(Some(ch)),
            None => self.next_inner(),
        }
    }

    #[inline]
    fn peek(&mut self) -> io::Result<Option<u8>> {
        match self.ch {
            Some(ch) => Ok(Some(ch)),
            None => {
                self.ch = self.next_inner()?;
                Ok(self.ch)
            }
        }
    }

    fn read_to_buffer(&mut self, mut n: usize) -> Result<()> {
        // defend against malicious input pretending to be huge strings by limiting growth
        self.scratch.reserve(cmp::min(n, 16 * 1024));

        if n == 0 {
            return Ok(())
        }

        if let Some(ch) = self.ch.take() {
            self.scratch.push(ch);
            n -= 1;
        }

        // n == 0 is OK here and needs no further special treatment

        let transfer_result = {
            // Prepare for take() (which consumes its reader) by creating a reference adaptor
            // that'll only live in this block
            let reference = self.reader.by_ref();
            // Append the first n bytes of the reader to the scratch vector (or up to
            // an error or EOF indicated by a shorter read)
            let mut taken = reference.take(n as u64);
            taken.read_to_end(&mut self.scratch)
        };

        match transfer_result {
            Ok(r) if r == n => Ok(()),
            Ok(_) => Err(Error::syntax(
                    ErrorCode::EofWhileParsingValue,
                    self.offset(),
                )),
            Err(e) => Err(Error::io(e)),
        }
    }

    fn clear_buffer(&mut self) {
        self.scratch.clear();
    }

    fn view_buffer<'a>(&'a self) -> EitherLifetime<'a, 'de> {
        EitherLifetime::Short(&self.scratch)
    }

    fn read_into(&mut self, buf: &mut [u8]) -> Result<()> {
        self.reader.read_exact(buf).map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                Error::syntax(ErrorCode::EofWhileParsingValue, self.offset())
            } else {
                Error::io(e)
            }
        })
    }

    #[inline]
    fn discard(&mut self) {
        self.ch = None;
    }

    fn offset(&self) -> u64 {
        self.reader.offset
    }
}

struct OffsetReader<R> {
    reader: R,
    offset: u64,
}

impl<R> io::Read for OffsetReader<R>
where
    R: io::Read,
{
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let r = self.reader.read(buf);
        if let Ok(count) = r {
            self.offset += count as u64;
        }
        r
    }
}

/// A CBOR input source that reads from a slice of bytes.
pub struct SliceRead<'a> {
    slice: &'a [u8],
    scratch: Vec<u8>,
    index: usize,
}

impl<'a> SliceRead<'a> {
    /// Creates a CBOR input source to read from a slice of bytes.
    pub fn new(slice: &'a [u8]) -> SliceRead<'a> {
        SliceRead {
            slice,
            scratch: vec![],
            index: 0,
        }
    }

    fn end(&self, n: usize) -> Result<usize> {
        match self.index.checked_add(n) {
            Some(end) if end <= self.slice.len() => Ok(end),
            _ => {
                Err(Error::syntax(
                    ErrorCode::EofWhileParsingValue,
                    self.slice.len() as u64,
                ))
            }
        }
    }
}

impl<'a> private::Sealed for SliceRead<'a> {}

impl<'a> Read<'a> for SliceRead<'a> {
    #[inline]
    fn next(&mut self) -> io::Result<Option<u8>> {
        Ok(if self.index < self.slice.len() {
            let ch = self.slice[self.index];
            self.index += 1;
            Some(ch)
        } else {
            None
        })
    }

    #[inline]
    fn peek(&mut self) -> io::Result<Option<u8>> {
        Ok(if self.index < self.slice.len() {
            Some(self.slice[self.index])
        } else {
            None
        })
    }

    fn clear_buffer(&mut self) {
        self.scratch.clear();
    }

    fn read_to_buffer(&mut self, n: usize) -> Result<()> {
        let end = self.end(n)?;
        let slice = &self.slice[self.index..end];
        self.scratch.extend_from_slice(slice);
        self.index = end;

        Ok(())
    }

    #[inline]
    fn read<'b>(&'b mut self, n: usize) -> Result<EitherLifetime<'b, 'a>> {
        let end = self.end(n)?;
        let slice = &self.slice[self.index..end];
        self.index = end;
        Ok(EitherLifetime::Long(slice))
    }

    fn view_buffer<'b>(&'b self) -> EitherLifetime<'b, 'a> {
        EitherLifetime::Short(&self.scratch)
    }

    #[inline]
    fn read_into(&mut self, buf: &mut [u8]) -> Result<()> {
        let end = self.end(buf.len())?;
        buf.copy_from_slice(&self.slice[self.index..end]);
        self.index = end;
        Ok(())
    }

    #[inline]
    fn discard(&mut self) {
        self.index += 1;
    }

    fn offset(&self) -> u64 {
        self.index as u64
    }
}

/// A CBOR input source that reads from a slice of bytes, and can move data around internally to
/// reassemble indefinite strings without the need of an allocated scratch buffer.
///
/// This is implemented using the sealingslice crate, which exposes this "interior immutability" in
/// a safe way.
pub struct MutSliceRead<'a> {
    /// A complete view of the reader's data.
    slice: SealingSlice<'a, u8>,
    /// Read cursor position in the mutable part of the slice
    index: usize,
    /// Length of the immutable part when clear() was last called.
    buffer_start: usize,
}

impl<'a> MutSliceRead<'a> {
    /// Creates a CBOR input source to read from a slice of bytes.
    pub fn new(slice: &'a mut [u8]) -> MutSliceRead<'a> {
        MutSliceRead {
            slice: SealingSlice::new(slice),
            index: 0,
            buffer_start: 0,
        }
    }

    fn end(&mut self, n: usize) -> Result<usize> {
        let mutlen = self.slice.mutable().len();
        match self.index.checked_add(n) {
            Some(end) if end <= mutlen => Ok(end),
            _ => {
                Err(Error::syntax(
                    ErrorCode::EofWhileParsingValue,
                    (self.slice.sealed().len() + mutlen) as u64,
                ))
            }
        }
    }
}

impl<'a> private::Sealed for MutSliceRead<'a> {}

impl<'a> Read<'a> for MutSliceRead<'a> {
    #[inline]
    fn next(&mut self) -> io::Result<Option<u8>> {
        let next = self.peek();
        if let Ok(Some(_)) = next {
            self.index += 1;
        };
        next
    }

    #[inline]
    fn peek(&mut self) -> io::Result<Option<u8>> {
        Ok(self.slice.mutable().get(self.index).cloned())
    }

    fn clear_buffer<'b>(&'b mut self) {
        self.slice.seal(self.index);
        self.buffer_start = self.slice.sealed().len();
        self.index = 0;
    }

    fn read_to_buffer(&mut self, n: usize) -> Result<()> {
        let end = self.end(n)?;
        self.slice.mutable()[..end].rotate_left(self.index);
        self.slice.seal(n);

        // self.index stays the same -- index was some bytes ahead of the seal before, and didn't
        // move relative to it.

        Ok(())
    }

    #[inline]
    fn read<'b>(&'b mut self, n: usize) -> Result<EitherLifetime<'b, 'a>> {
        self.clear_buffer();
        self.read_to_buffer(n)?;
        Ok(self.view_buffer())
    }

    fn view_buffer<'b>(&'b self) -> EitherLifetime<'b, 'a> {
        EitherLifetime::Long(&self.slice.sealed()[self.buffer_start..])
    }

    #[inline]
    fn read_into(&mut self, buf: &mut [u8]) -> Result<()> {
        let end = self.end(buf.len())?;
        buf.copy_from_slice(&self.slice.mutable()[self.index..end]);
        self.index = end;
        Ok(())
    }

    #[inline]
    fn discard(&mut self) {
        self.index += 1;
    }

    fn offset(&self) -> u64 {
        (self.slice.sealed().len() + self.index) as u64
    }
}
