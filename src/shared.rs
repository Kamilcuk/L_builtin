//! Shared utilities for L_builtin Rust implementation

use std::ffi::CStr;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::raw::{c_char, c_int};

use memmap2::MmapMut;

use crate::bash_api::{bind_variable, find_variable, l_readonly_p};
use crate::beprintln;
use crate::bprint_bytes::BDisplay;

/// Bind `value` to the shell variable `name`.
///
/// Both `name` and `value` must be NUL-terminated C strings (as returned by
/// bash's C API). No copying or allocation is performed.
/// Fails on readonly variables or a failed bind.
///
/// # Safety
/// `name` and `value` must be valid pointers to NUL-terminated C strings.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub(crate) unsafe fn bind_shell_variable(
    name: *const c_char,
    value: *const c_char,
) -> Result<(), String> {
    unsafe {
        debug_assert!(!name.is_null(), "name is null");
        debug_assert!(!value.is_null(), "value is null");
        let var = find_variable(name);
        if !var.is_null() && l_readonly_p(var) != 0 {
            return Err(format!(
                "{}: readonly variable",
                CStr::from_ptr(name).to_string_lossy()
            ));
        }
        if bind_variable(name, value, 0).is_null() {
            return Err(format!(
                "failed to set variable: {}",
                CStr::from_ptr(name).to_string_lossy()
            ));
        }
    }
    Ok(())
}

/// Unwrap a `getargs::Result`, printing `"{ename}: {error}"` to stderr and
/// returning `code` on failure.
///
/// `ename` is the subcommand's name (e.g. its `ENAME` const); `expr` is the
/// result being unwrapped; `code` is the exit code returned on error (e.g.
/// `EX_USAGE`). The `return` exits the caller's function, so this is only
/// valid inside a `-> c_int` subcommand handler.
#[macro_export]
macro_rules! return_on_err {
    ($ename:expr, $expr:expr, $code:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => {
                beprintln!($ename, ": ", e);
                return $code;
            }
        }
    };
}

#[macro_export]
macro_rules! return_on_err2 {
    ($ename:expr, $prefix:expr, $expr:expr, $code:expr) => {
        match $expr.inspect_err(|e| {
            $crate::beprintln!($ename, concat!(": ", $prefix, ": "), e);
        }) {
            Ok(val) => val,
            Err(_) => return $code,
        }
    };
}

////////////////////////////////////

struct RedirectStdout {
    saved_stdout: File,
}

impl RedirectStdout {
    pub fn new(target: &File) -> io::Result<Self> {
        flush_stdout_buffers();
        let saved_fd = unsafe { libc::fcntl(1, libc::F_DUPFD_CLOEXEC, 256) };
        if saved_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let res = unsafe { libc::dup2(target.as_raw_fd(), 1) };
        if res < 0 {
            unsafe {
                libc::close(saved_fd);
            }
            return Err(io::Error::last_os_error());
        }
        let saved_stdout = unsafe { File::from_raw_fd(saved_fd) };
        Ok(Self { saved_stdout })
    }
}

impl Drop for RedirectStdout {
    fn drop(&mut self) {
        flush_stdout_buffers();
        unsafe {
            libc::dup2(self.saved_stdout.as_raw_fd(), 1);
        }
    }
}

pub(crate) fn flush_stdout_buffers() {
    let _ = io::stdout().flush();
    unsafe { libc::fflush(std::ptr::null_mut()) };
}

////////////////////////////////////
pub(crate) struct Memfd {
    file: File,
}

impl Memfd {
    pub fn new() -> io::Result<Self> {
        let fd = unsafe { libc::memfd_create(c"L_capture".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let memfd = unsafe { File::from_raw_fd(fd) };
        Ok(Self { file: memfd })
    }
}

pub(crate) fn trim_trailing_newlines_in_zero_terminated_array_place(bytes: &mut [u8]) {
    debug_assert!(
        !bytes.is_empty() && bytes.last() == Some(&0),
        "array must be non-empty and null-terminated, found: {:?}",
        bytes
    );
    let orig_len = bytes.len() - 1;
    let mut i = orig_len;
    while i > 0 {
        if bytes[i - 1] == b'\n' {
            i -= 1;
            if i > 0 && bytes[i - 1] == b'\r' {
                i -= 1;
            }
        } else {
            break;
        }
    }
    if i < orig_len {
        bytes[i] = b'\0';
    }
}

////////////////////////////////////
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub(crate) fn capture_into_variable(
    ename: &str,
    var: *const c_char,
    trimnewlines: bool,
    f: impl FnOnce() -> c_int,
) -> c_int {
    let mut memfd = return_on_err2!(ename, "cannot capture stdout", Memfd::new(), 1);
    let result = {
        let _guard = return_on_err2!(
            ename,
            "cannot redirect stdout",
            RedirectStdout::new(&memfd.file),
            1
        );
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
    };
    let ret = return_on_err2!(ename, "captured command panicked", result, 1);
    return_on_err2!(ename, "couldn't write to memfd", memfd.file.write(b"\0"), 1);
    let mut mmap = return_on_err2!(
        ename,
        "couldn't mmap",
        unsafe { MmapMut::map_mut(&memfd.file) },
        1
    );
    if trimnewlines {
        trim_trailing_newlines_in_zero_terminated_array_place(&mut mmap)
    }
    let res = unsafe { bind_shell_variable(var, mmap.as_ptr().cast()) };
    return_on_err!(ename, res, 1);
    ret
}

////////////////////////////////////

/// Lexicographic `a < b` for byte slices, usable in const context.
pub(crate) const fn bytes_lt(a: &[u8], b: &[u8]) -> bool {
    let min = if a.len() < b.len() { a.len() } else { b.len() };
    let mut i = 0;
    while i < min {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
        i += 1;
    }
    a.len() < b.len()
}

/// Sort an array of `(byte-key, value)` pairs by key at compile time.
///
/// Insertion sort; runs entirely in const evaluation, so the resulting
/// table is stored pre-sorted in the binary and can be binary-searched at
/// runtime.
pub(crate) const fn sort_by_byte_key<T: Copy, const N: usize>(
    mut arr: [(&'static [u8], T); N],
) -> [(&'static [u8], T); N] {
    let mut i = 1;
    while i < N {
        let item = arr[i];
        let mut j = i;
        while j > 0 && bytes_lt(item.0, arr[j - 1].0) {
            arr[j] = arr[j - 1];
            j -= 1;
        }
        arr[j] = item;
        i += 1;
    }
    arr
}

impl BDisplay for getargs::Error<&[u8]> {
    fn bwrite<W: Write>(&self, w: &mut W) {
        match self {
            getargs::Error::RequiresValue(opt) => {
                w.write_all(b"option requires a value: ").unwrap();
                opt.bwrite(w);
            }
            getargs::Error::DoesNotRequireValue(opt) => {
                w.write_all(b"option does not require a value: ").unwrap();
                opt.bwrite(w);
            }
            &_ => {}
        }
    }
}

impl BDisplay for getargs::Opt<&[u8]> {
    fn bwrite<W: Write>(&self, _w: &mut W) {}
}

pub(crate) fn getargs_unexpected(
    ENAME: &(impl BDisplay + ?Sized),
    arg: getargs::Opt<&[u8]>,
) -> c_int {
    match arg {
        getargs::Opt::Short(c) => {
            beprintln!(ENAME, b": unknown option -", c);
            2
        }
        getargs::Opt::Long(l) => {
            beprintln!(ENAME, b": unknown option --", l);
            2
        }
    }
}

////////////////////////////////////////////

pub(crate) struct CByteStr(Vec<u8>);

impl CByteStr {
    #[inline]
    pub(crate) fn new(bytes: &[u8]) -> Self {
        debug_assert!(
            bytes.last() != Some(&0),
            "input slice already ends with a null byte; unexpected double null-termination"
        );
        let mut buf = Vec::with_capacity(bytes.len() + 1);
        buf.extend_from_slice(bytes);
        buf.push(0);
        Self(buf)
    }

    #[inline]
    pub(crate) fn as_ptr(&self) -> *const c_char {
        self.0.as_ptr().cast()
    }
}

/// Returns a `*const c_char` pointer for a slice that comes from a
/// NUL-terminated C string. The slice itself excludes the trailing NUL
/// (as returned by `CStr::to_bytes()`), but the underlying C string is
/// guaranteed to be NUL-terminated, so the NUL is at `bytes.as_ptr().add(bytes.len())`.
#[inline]
pub(crate) fn from_after_null_terminated(bytes: &[u8]) -> *const c_char {
    debug_assert!(
        !bytes.is_empty(),
        "input slice is empty; cannot verify null terminator"
    );
    // The slice comes from CStr::to_bytes() which excludes the NUL.
    // The underlying C string is NUL-terminated — verify the byte one past the slice.
    unsafe {
        debug_assert!(
            *bytes.as_ptr().add(bytes.len()) == 0,
            "expected NUL byte at index {} (one past slice end)",
            bytes.len()
        );
    }
    bytes.as_ptr().cast()
}
