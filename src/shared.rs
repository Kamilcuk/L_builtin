//! Shared utilities for L_builtin Rust implementation

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::raw::{c_char, c_int};

use memmap2::MmapMut;

use crate::bash_api::{bind_variable, EXECUTION_FAILURE, EXECUTION_SUCCESS};
use crate::cmdargs::BashVar;
use crate::l_builtin_error;
use crate::subcmd::CmdResult;

/// Bind `value` to the shell variable `var`, returning `EXECUTION_SUCCESS` on
/// success or `EXECUTION_FAILURE` if the bind failed (e.g. a readonly variable).
///
/// # Safety
/// `var` and `value` must be valid pointers to NUL-terminated C strings.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub(crate) unsafe fn bind_variable_check(
    var: *const c_char,
    value: *const c_char,
    flags: c_int,
) -> c_int {
    if bind_variable(var, value, flags).is_null() {
        EXECUTION_FAILURE
    } else {
        EXECUTION_SUCCESS
    }
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
    _ename: &str,
    var: BashVar,
    trimnewlines: bool,
    f: impl FnOnce() -> CmdResult,
) -> CmdResult {
    let mut memfd = Memfd::new().map_err(|_e| l_builtin_error!("cannot capture stdout"))?;
    let result;
    {
        let _guard = RedirectStdout::new(&memfd.file)
            .map_err(|e| l_builtin_error!("cannot redirect stdout: ", e))?;
        result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
            .map_err(|e| l_builtin_error!("captured command panicked: ", e))?;
    }
    memfd
        .file
        .write(b"\0")
        .map_err(|e| l_builtin_error!("couldn't write to memfd: ", e))?;
    let mut mmap = unsafe { MmapMut::map_mut(&memfd.file) }
        .map_err(|e| l_builtin_error!("could not mmap:", e))?;
    if trimnewlines {
        trim_trailing_newlines_in_zero_terminated_array_place(&mut mmap)
    }
    var.set(mmap.as_ptr().cast())?;
    result
}

/// Format a string into a stack buffer and return the buffer.
///
/// `$size` is the total buffer size in bytes; the last byte is reserved for the
/// null terminator. Returns `[u8; $size]`.
///
/// # Example
/// ```ignore
/// let buf = bufwrite!(48, "{}:{}", ip, port);
/// let addr_ptr = buf.as_ptr() as *const c_char;
/// ```
#[macro_export]
macro_rules! bufwrite {
    ($size:expr, $($arg:tt)*) => {{
        let mut buf = [0u8; $size];
        let mut cursor = ::std::io::Cursor::new(&mut buf[..$size - 1]);
        let _ = ::std::io::Write::write_fmt(&mut cursor, ::core::format_args!($($arg)*));
        let pos = cursor.position() as usize;
        buf[pos] = 0;
        buf
    }};
}

/// Absolute `CLOCK_REALTIME` timespec `secs` seconds from now, for
/// `pthread_mutex_timedlock` / `sem_timedwait`.
pub(crate) fn timespec_from_now(secs: f64) -> libc::timespec {
    let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
    unsafe {
        libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts);
    }
    let now = ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9;
    let abs = now + secs;
    ts.tv_sec = abs.floor() as libc::time_t;
    ts.tv_nsec = ((abs - ts.tv_sec as f64) * 1e9).round() as libc::c_long;
    ts
}
