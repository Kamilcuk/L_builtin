use std::any::Any;
use std::ffi::c_char;
use std::ffi::CStr;
use std::ffi::{OsStr, OsString};
use std::io;
use std::io::Write;

/// Trait for types that can display themselves as raw bytes to any Write.
/// Implement this for your types to enable `bwriteln!(stream, value)`.
pub(crate) trait BDisplay {
    /// Write raw bytes to the given writer (without newline).
    fn bwrite<W: Write>(&self, w: &mut W);
}

// === Byte slices and arrays ===
impl BDisplay for [u8] {
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(self).ok();
    }
}
impl<const N: usize> BDisplay for [u8; N] {
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(self.as_slice()).ok();
    }
}
impl BDisplay for Vec<u8> {
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(self.as_slice()).ok();
    }
}

// === Strings (as UTF-8 bytes) ===
impl BDisplay for str {
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(self.as_bytes()).ok();
    }
}

impl BDisplay for String {
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(self.as_bytes()).ok();
    }
}

// === CStr ===
impl BDisplay for CStr {
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(self.to_bytes()).ok();
    }
}

// === References ===
impl<T: BDisplay + ?Sized> BDisplay for &T {
    fn bwrite<W: Write>(&self, w: &mut W) {
        (**self).bwrite(w)
    }
}
impl<T: BDisplay + ?Sized> BDisplay for &mut T {
    fn bwrite<W: Write>(&self, w: &mut W) {
        (**self).bwrite(w)
    }
}

// === C string pointers: print the STRING CONTENTS, not the pointer address ===
impl BDisplay for *const c_char {
    fn bwrite<W: Write>(&self, w: &mut W) {
        if self.is_null() {
            w.write_all(b"(null)").ok();
            return;
        }
        unsafe {
            w.write_all(CStr::from_ptr(*self).to_bytes()).ok();
        }
    }
}
impl BDisplay for *mut c_char {
    fn bwrite<W: Write>(&self, w: &mut W) {
        let ptr = *self as *const c_char;
        if ptr.is_null() {
            w.write_all(b"(null)").ok();
        } else {
            unsafe {
                w.write_all(CStr::from_ptr(ptr).to_bytes()).ok();
            }
        }
    }
}

impl BDisplay for OsString {
    fn bwrite<W: Write>(&self, w: &mut W) {
        self.as_os_str().bwrite(w);
    }
}

impl BDisplay for OsStr {
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(self.to_string_lossy().as_bytes()).ok();
    }
}

impl BDisplay for u8 {
    #[inline]
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(&[*self]).ok();
    }
}

impl BDisplay for char {
    #[inline]
    fn bwrite<W: Write>(&self, w: &mut W) {
        let mut buf = [0u8; 4];
        let bytes = self.encode_utf8(&mut buf).as_bytes();
        w.write_all(bytes).ok();
    }
}

impl BDisplay for io::Error {
    fn bwrite<W: Write>(&self, w: &mut W) {
        w.write_all(self.to_string().as_bytes()).ok();
    }
}

impl BDisplay for Box<dyn Any + Send> {
    fn bwrite<W: Write>(&self, writer: &mut W) {
        let msg = if let Some(s) = self.downcast_ref::<&str>() {
            *s
        } else if let Some(s) = self.downcast_ref::<String>() {
            s.as_str()
        } else {
            "unknown panic payload"
        };
        writer.write_all(msg.as_bytes()).ok();
    }
}

////////////////////////////////////////////

#[allow(dead_code)]
pub(crate) struct ViaDisplay<'a, T: ?Sized>(pub &'a T);

impl<'a, T: core::fmt::Display + ?Sized> BDisplay for ViaDisplay<'a, T> {
    fn bwrite<W: Write + ?Sized>(&self, writer: &mut W) {
        struct FmtAdapter<'w, W: Write + ?Sized>(&'w mut W);

        impl<'w, W: Write + ?Sized> core::fmt::Write for FmtAdapter<'w, W> {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                self.0.write_all(s.as_bytes()).ok();
                Ok(())
            }
        }

        let mut adapter = FmtAdapter(writer);
        let _ = core::fmt::write(&mut adapter, format_args!("{}", self.0));
    }
}

macro_rules! impl_bdisplay_uint {
    ($($t:ty),*) => {
        $(
            impl BDisplay for $t {
                #[inline]
                fn bwrite<W: Write>(&self, w: &mut W) {
                    let mut buf = [0u8; 39];
                    let mut n = *self;
                    let mut i = buf.len();

                    if n == 0 {
                        w.write_all(b"0").ok();
                        return;
                    }

                    while n > 0 {
                        i -= 1;
                        buf[i] = b'0' + (n % 10) as u8;
                        n /= 10;
                    }

                    w.write_all(&buf[i..]).ok();
                }
            }
        )*
    };
}

macro_rules! impl_bdisplay_sint {
    ($($t:ty),*) => {
        $(
            impl BDisplay for $t {
                #[inline]
                fn bwrite<W: Write>(&self, w: &mut W) {
                    let mut buf = [0u8; 40];
                    let mut n = *self;
                    let is_negative = n < 0;
                    let mut i = buf.len();

                    if n == 0 {
                        w.write_all(b"0").ok();
                        return;
                    }

                    while n != 0 {
                        i -= 1;
                        let digit = (n % 10).unsigned_abs() as u8;
                        buf[i] = b'0' + digit;
                        n /= 10;
                    }

                    if is_negative {
                        i -= 1;
                        buf[i] = b'-';
                    }

                    w.write_all(&buf[i..]).ok();
                }
            }
        )*
    };
}

impl_bdisplay_uint!(u16, u32, u64, u128, usize);
impl_bdisplay_sint!(i8, i16, i32, i64, i128, isize);

// === Macros ===

/// Write raw bytes to a writer without a trailing newline.
#[macro_export]
macro_rules! bwrite {
    ($w:expr) => {};
    ($w:expr, $first:expr $(, $rest:expr)*) => {{
        let first = &$first;
        $crate::bprint_bytes::BDisplay::bwrite(first, &mut $w);
        $crate::bwrite!($w $(, $rest)*);
    }};
}

/// Write raw bytes + newline to a writer.
#[macro_export]
macro_rules! bwriteln {
    ($($tt:tt)*) => {
        $crate::bwrite!($($tt)*, b"\n");
    };
}

/// Formats arguments using `BDisplay` and returns a `Vec<u8>`.
#[macro_export]
macro_rules! bformat {
    ($($arg:expr),* $(,)?) => {{
        let mut buf = Vec::new();
        $crate::bwrite!(buf, $($arg),*);
        buf
    }};
}

/// Formats arguments using `BDisplay` and returns a `String`.
/// Panics if the generated bytes are not valid UTF-8.
#[macro_export]
macro_rules! bformatstring {
    ($($arg:expr),* $(,)?) => {{
        let bytes = $crate::bformat!($($arg),*);
        ::std::string::String::from_utf8(bytes).expect("bformat_string produced invalid UTF-8")
    }};
}

/// Formats arguments using `BDisplay` and returns a `Vec<u8>`.
#[macro_export]
macro_rules! bformatln {
    ($($arg:expr),* $(,)?) => {{
        let mut buf = $crate::alloc::vec::Vec::new();
        $crate::bwriteln!(buf, $($arg),*);
        buf
    }};
}

/// Internal helper macro to lock a standard stream and write.
#[macro_export]
#[doc(hidden)]
macro_rules! bprint_impl {
    ($target:expr) => {};
    ($target:expr, $($tt:tt)+) => {{
        let mut w = $target.lock();
        $crate::bwrite!(w, $($tt)+);
    }};
}

/// Print to stdout without a trailing newline.
#[macro_export]
macro_rules! bprint {
    ($($tt:tt)*) => {
        $crate::bprint_impl!(::std::io::stdout(), $($tt)*);
    };
}

/// Print to stderr without a trailing newline.
#[macro_export]
macro_rules! beprint {
    ($($tt:tt)*) => {
        $crate::bprint_impl!(::std::io::stderr(), $($tt)*);
    };
}

/// Print to stdout with a trailing newline.
#[macro_export]
macro_rules! bprintln {
    ($($tt:tt)*) => {
        $crate::bprint_impl!(::std::io::stdout(), $($tt)*, b"\n");
    };
}

/// Print to stderr with a trailing newline.
#[macro_export]
macro_rules! beprintln {
    ($($tt:tt)*) => {
        $crate::bprint_impl!(::std::io::stderr(), $($tt)*, b"\n");
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::io::Cursor;

    #[test]
    fn test_basic() {
        bprintln!(b"hello");
        bprintln!(&[1, 2, 3]);
        bprintln!(vec![4, 5, 6]);
        bprintln!("hello");
        bprintln!(String::from("world"));
    }
    #[test]
    fn test_cstr() {
        let s = CString::new("hello").unwrap();
        bprintln!(s.as_ptr());
    }
    #[test]
    fn test_multi() {
        bprintln!(b"one", b"two", b"three");
    }
    #[test]
    fn test_stderr() {
        beprintln!(b"error");
    }
    #[test]
    fn test_bwriteln() {
        let mut buf = Cursor::new(Vec::new());
        bwriteln!(buf, b"hello");
        bwriteln!(buf, "world");
        assert_eq!(buf.into_inner(), b"hello\nworld\n");
    }
}
