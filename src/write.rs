#[cfg(feature = "std")]
use std::io;

use error;

/// This is a trait similar to std::io::Write, but with potientially different errors.
///
/// It depends on core::fmt::Write to allow easy implementation of
/// serde::ser::Serializer::collect_string.
pub trait Write: core::fmt::Write {
    type Error: Into<error::Error>;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error>;
}

impl<'a, W> Write for &'a mut W where
    W: Write,
{
    type Error = W::Error;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        (*self).write_all(buf)
    }
}

#[cfg(feature = "std")]
// FIXME want to implement for all io::Write
impl Write for Vec<u8>
{
    type Error = io::Error;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        (self as &mut io::Write).write_all(buf)
    }
}

/// A WindowedInfinity represents an infinite writable space. A small section of it is mapped to a
/// &mut [u8] to which writes are forwarded; writes to the area outside are silently discsarded. It
/// implements Write and can thus be used as a target for CBOR serialization.
pub struct WindowedInfinity<'a> {
    view: &'a mut [u8],
    cursor: isize,
}

impl<'a> WindowedInfinity<'a> {
    /// Create a new infinity with the window passed as view. The cursor parameter indicates where
    /// (in the index space of the view) the infinity's write operations should start, and is
    /// typically either 0 or negative.
    pub fn new(view: &'a mut [u8], cursor: isize) -> Self {
        WindowedInfinity { view, cursor }
    }
}

impl<'a> Write for WindowedInfinity<'a>
{
    type Error = ();

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        let start = self.cursor;
        self.cursor += buf.len() as isize;
        let end = self.cursor;

        if end <= 0 {
            // Not in view yet
            return Ok(());
        }

        if start >= self.view.len() as isize {
            // Already out of view
            return Ok(());
        }

        let (fronttrim, start) = if start < 0 {
            (-start, 0)
        } else {
            (0, start)
        };
        let buf = &buf[fronttrim as usize..];

        let overshoot = start + buf.len() as isize - self.view.len() as isize;
        let (tailtrim, end) = if overshoot > 0 {
            (overshoot, end - overshoot)
        } else {
            (0, end)
        };
        let buf = &buf[..buf.len() - tailtrim as usize];
        self.view[start as usize..end as usize].copy_from_slice(buf);

        Ok(())
    }
}

impl<'a> core::fmt::Write for WindowedInfinity<'a>
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_all(s.as_bytes()).map_err(|_| core::fmt::Error)
    }
}
