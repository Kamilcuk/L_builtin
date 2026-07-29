use std::io::{self, Write};
use std::fmt;

/// Helper trait to convert supported types into raw byte slices without allocation.
pub trait AsByteSlice {
    fn as_byte_slice(&self) -> &[u8];
}

impl AsByteSlice for [u8] {
    #[inline]
    fn as_byte_slice(&self) -> &[u8] {
        self
    }
}

impl<const N: usize> AsByteSlice for [u8; N] {
    #[inline]
    fn as_byte_slice(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsByteSlice for str {
    #[inline]
    fn as_byte_slice(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsByteSlice for String {
    #[inline]
    fn as_byte_slice(&self) -> &[u8] {
        self.as_bytes()
    }
}

use std::io::Write;

macro_rules! beprintln {
    ($($arg:tt)*) => {{
        let mut stderr = ::std::io::stderr().lock();
        let _ = ::std::fmt::write(&mut StderrByteWriter(&mut stderr), format_args!($($arg)*));
        let _ = stderr.write_all(b"\n");
    }};
}

// Helper struct to bridge std::fmt::Write to std::io::Write directly
struct StderrByteWriter<'a>(&'a mut ::std::io::StderrLock<'static>);

impl<'a> ::std::fmt::Write for StderrByteWriter<'a> {
    fn write_str(&mut self, s: &str) -> ::std::fmt::Result {
        self.0.write_all(s.as_bytes()).map_err(|_| ::std::fmt::Error)
    }
}

pub struct DisplayBytes<'a>(pub &'a [u8]);

impl<'a> fmt::Display for DisplayBytes<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Writes raw bytes directly to the formatter output stream
        for &byte in self.0 {
            f.write_char(byte as char)?;
        }
        Ok(())
    }
}
