//! Shared utilities for L_builtin Rust implementation

use std::ffi::{CString, OsStr};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::raw::c_int;
use std::os::unix::ffi::OsStrExt;

use crate::bash_api::{bind_variable, find_variable, l_readonly_p};

/// Bind `value` to the shell variable `name`.
///
/// Takes `value` by value so the `CString` conversion reuses its allocation.
/// Fails on embedded NUL bytes, readonly variables, or a failed bind.
pub fn bind_shell_variable(name: &OsStr, value: Vec<u8>) -> Result<(), String> {
    let c_name = CString::new(name.as_bytes())
        .map_err(|_| "variable name contains NUL byte".to_string())?;
    let c_value =
        CString::new(value).map_err(|_| "value contains NUL byte".to_string())?;
    unsafe {
        let var = find_variable(c_name.as_ptr());
        if !var.is_null() && l_readonly_p(var) != 0 {
            return Err(format!("{}: readonly variable", name.to_string_lossy()));
        }
        if bind_variable(c_name.as_ptr(), c_value.as_ptr(), 0).is_null() {
            return Err(format!("failed to set variable: {}", name.to_string_lossy()));
        }
    }
    Ok(())
}

/// RAII redirection of fd 1 into a memfd (constructor/destructor pattern).
///
/// `begin()` drains userspace stdout buffers, saves fd 1, and points it at a
/// fresh memfd. Dropping the guard drains buffers again and restores fd 1 —
/// on the success path, on errors, and after a caught panic alike.
/// `finish()` additionally returns the captured bytes.
pub struct StdoutCapture {
    /// Dup of the original fd 1 (cloexec, >= 10 to stay out of the
    /// script-visible range). Restored and closed on drop.
    saved: OwnedFd,
    /// Capture target; `Some` until `finish()` takes it.
    memfd: Option<File>,
}

impl StdoutCapture {
    pub fn begin() -> io::Result<Self> {
        // Pending buffered output must reach the *real* stdout, not the capture.
        flush_stdout_buffers();

        let fd = unsafe { libc::memfd_create(c"L_capture".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let memfd = unsafe { File::from_raw_fd(fd) };

        let fd = unsafe { libc::fcntl(1, libc::F_DUPFD_CLOEXEC, 10) };
        if fd < 0 {
            // EBADF: stdout was closed (e.g. `L_builtin ... >&-`).
            return Err(io::Error::last_os_error());
        }
        let saved = unsafe { OwnedFd::from_raw_fd(fd) };

        if unsafe { libc::dup2(memfd.as_raw_fd(), 1) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { saved, memfd: Some(memfd) })
    }

    /// Restore fd 1 and return everything captured.
    pub fn finish(mut self) -> io::Result<Vec<u8>> {
        let mut file = self.memfd.take().expect("memfd taken only by finish()");
        // Drop drains buffers into the memfd and restores fd 1; the taken
        // File keeps the memfd itself alive for reading.
        drop(self);

        file.seek(SeekFrom::Start(0))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

impl Drop for StdoutCapture {
    fn drop(&mut self) {
        // Drain what ran under the capture, then put the original fd 1 back.
        flush_stdout_buffers();
        unsafe { libc::dup2(self.saved.as_raw_fd(), 1) };
    }
}

/// Drain both userspace stdout buffers (Rust's LineWriter and C stdio) down
/// to whatever fd 1 currently is.
pub fn flush_stdout_buffers() {
    let _ = io::stdout().flush();
    unsafe { libc::fflush(std::ptr::null_mut()) };
}

/// Run `f` with stdout captured, then bind the output (trailing newlines
/// stripped, matching `$(...)` / `${ ...; }` semantics) to shell variable
/// `var`. Returns `f`'s exit code, or 1 on capture/bind/panic errors.
///
/// `ename` prefixes error messages. Panics in `f` are caught: fd 1 must be
/// restored, and a panic crossing the extern "C" boundary would abort bash.
pub fn capture_into_variable(
    ename: &str,
    var: &OsStr,
    f: impl FnOnce() -> c_int,
) -> c_int {
    let capture = match StdoutCapture::begin() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{ename}: cannot capture stdout: {e}");
            return 1;
        }
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    // Restores fd 1 on every path, including the panic path below.
    let output = capture.finish();

    let ret = match result {
        Ok(r) => r,
        Err(_) => {
            eprintln!("{ename}: captured command panicked");
            return 1;
        }
    };
    let mut output = match output {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{ename}: failed to read captured output: {e}");
            return 1;
        }
    };

    // Strip trailing newlines, matching `$(...)` semantics.
    while output.last() == Some(&b'\n') {
        output.pop();
    }

    if let Err(e) = bind_shell_variable(var, output) {
        eprintln!("{ename}: {e}");
        return 1;
    }
    ret
}

/// Lexicographic `a < b` for byte slices, usable in const context.
pub const fn bytes_lt(a: &[u8], b: &[u8]) -> bool {
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
pub const fn sort_by_byte_key<T: Copy, const N: usize>(
    mut arr: [(&'static [u8], T); N],
) -> [(&'static [u8], T); N] {
    let mut i = 1;
    while i < N {
        let mut j = i;
        while j > 0 && bytes_lt(arr[j].0, arr[j - 1].0) {
            let tmp = arr[j];
            arr[j] = arr[j - 1];
            arr[j - 1] = tmp;
            j -= 1;
        }
        i += 1;
    }
    arr
}

/// Unwrap a `lexopt::Result`, printing `"{ename}: {error}"` to stderr and
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
                eprintln!("{}: {e}", $ename);
                return $code;
            }
        }
    };
}
