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
