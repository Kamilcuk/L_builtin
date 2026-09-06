//! Shared utilities for L_builtin Rust implementation

use crate::bash_api::{bind_variable, EXECUTION_FAILURE, EXECUTION_SUCCESS};
use std::fs::File;
use std::io::{self};
use std::os::fd::{FromRawFd, RawFd};
use std::os::raw::{c_char, c_int};

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
    if bind_variable(var, value.cast_mut(), flags).is_null() {
        EXECUTION_FAILURE
    } else {
        EXECUTION_SUCCESS
    }
}

////////////////////////////////////

/// Minimum file descriptor number for internally-created fds.
///
/// Bash redirects stdin (fd 0) from `/dev/null` for asynchronous commands
/// (commands launched with `&`), which silently reclaims fd 0. If a memfd or
/// shm_open fd lands on 0/1/2 it is lost in the forked child. Ensuring every
/// internal fd is >= this value keeps them out of the standard range.
pub(crate) const L_FD_MIN: RawFd = 80;

/// Ensure `fd` is >= [`L_FD_MIN`] by duplicating it when necessary.
/// `cloexec` selects `F_DUPFD_CLOEXEC` (true) or `F_DUPFD` (false). Takes
/// ownership of `fd`: on success the original fd is closed and the new high fd
/// is returned; on error the original fd is also closed and the error is
/// returned. If the fd is already high enough it is returned unchanged.
pub(crate) fn ensure_high_fd(fd: RawFd, cloexec: bool) -> io::Result<RawFd> {
    if fd >= L_FD_MIN {
        return Ok(fd);
    }
    let cmd = if cloexec {
        libc::F_DUPFD_CLOEXEC
    } else {
        libc::F_DUPFD
    };
    let new_fd = unsafe { libc::fcntl(fd, cmd, L_FD_MIN) };
    unsafe { libc::close(fd) };
    if new_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(new_fd)
}

////////////////////////////////////

pub(crate) struct Memfd {
    pub file: File,
}

impl Memfd {
    pub fn new() -> io::Result<Self> {
        let fd = unsafe { libc::memfd_create(c"L_capture".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let new_fd = ensure_high_fd(fd, true)?;
        Ok(Self {
            file: unsafe { File::from_raw_fd(new_fd) },
        })
    }
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
