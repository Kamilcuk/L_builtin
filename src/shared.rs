//! Shared utilities for L_builtin Rust implementation

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicU64, Ordering};

use memmap2::MmapMut;

use crate::bash_api::{
    bind_variable, find_variable, l_readonly_p, Cpnt, EXECUTION_FAILURE, EXECUTION_SUCCESS,
};
use crate::beprintln;
use crate::l_builtin_error;

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

pub const fn max_str_len_for_bits(bits: usize) -> usize {
    // ceil(bits * log10(2)) + 1 (sign) + 1 (null terminator)
    ((bits * 30103) / 100000) + 3
}

pub struct IntStr<const N: usize> {
    buf: [u8; N],
    start: usize,
}

impl<const N: usize> IntStr<N> {
    pub fn as_ptr(&self) -> *const c_char {
        &self.buf[self.start] as *const u8 as *const c_char
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[self.start..N - 1]
    }
}

// Constructor for i64
impl<const N: usize> IntStr<N> {
    pub fn new_i64(val: i64) -> Self {
        let mut buf = [0u8; N];
        let mut n = val;
        let is_negative = n < 0;
        if is_negative {
            n = -n;
        }
        let mut i = N;
        i -= 1;
        buf[i] = 0; // null terminator
        if n == 0 {
            i -= 1;
            buf[i] = b'0';
        } else {
            while n > 0 && i > 0 {
                i -= 1;
                buf[i] = b'0' + (n % 10) as u8;
                n /= 10;
            }
        }
        if is_negative && i > 0 {
            i -= 1;
            buf[i] = b'-';
        }
        Self { buf, start: i }
    }
}

// Constructor for u64
impl<const N: usize> IntStr<N> {
    pub fn new_u64(val: u64) -> Self {
        let mut buf = [0u8; N];
        let mut n = val;
        let mut i = N;
        i -= 1;
        buf[i] = 0; // null terminator
        if n == 0 {
            i -= 1;
            buf[i] = b'0';
        } else {
            while n > 0 && i > 0 {
                i -= 1;
                buf[i] = b'0' + (n % 10) as u8;
                n /= 10;
            }
        }
        Self { buf, start: i }
    }
}

// Constructor for usize
impl<const N: usize> IntStr<N> {
    pub fn new_usize(val: usize) -> Self {
        let mut buf = [0u8; N];
        let mut n = val;
        let mut i = N;
        i -= 1;
        buf[i] = 0; // null terminator
        if n == 0 {
            i -= 1;
            buf[i] = b'0';
        } else {
            while n > 0 && i > 0 {
                i -= 1;
                buf[i] = b'0' + (n % 10) as u8;
                n /= 10;
            }
        }
        Self { buf, start: i }
    }
}

// Newtype wrappers to create distinct types (not just aliases)
pub struct SizeTStr(IntStr<{ max_str_len_for_bits(size_of::<usize>() * 8) }>);
pub struct I64Str(IntStr<{ max_str_len_for_bits(64) }>);
pub struct U64Str(IntStr<{ max_str_len_for_bits(64) }>);

impl SizeTStr {
    pub fn from_usize(val: usize) -> Self {
        Self(IntStr::new_usize(val))
    }
    pub fn as_ptr(&self) -> *const c_char {
        self.0.as_ptr()
    }
}

impl I64Str {
    pub fn new(val: i64) -> Self {
        Self(IntStr::new_i64(val))
    }
    pub fn as_ptr(&self) -> *const c_char {
        self.0.as_ptr()
    }
}

impl U64Str {
    pub fn new(val: u64) -> Self {
        Self(IntStr::new_u64(val))
    }
    pub fn as_ptr(&self) -> *const c_char {
        self.0.as_ptr()
    }
}

////////////////////////////////////
// C string helpers - no allocations, work with raw pointers from bash
////////////////////////////////////

/// Convert a raw C string pointer to &str (borrowing, no allocation)
pub fn cstr_to_str(ptr: *const c_char) -> Option<&'static str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr).to_str().ok() }
}

/// Convert a raw C string pointer to &[u8] (borrowing, no allocation)
pub fn cstr_to_bytes(ptr: *const c_char) -> Option<&'static [u8]> {
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(ptr).to_bytes() })
}

/// Parse an integer from a raw C string pointer
pub fn parse_int<T: std::str::FromStr>(ptr: *const c_char) -> Option<T> {
    cstr_to_str(ptr)?.parse().ok()
}

/// Create a null-terminated C string from a raw buffer using a stack buffer
pub fn bytes_to_cstr<'a, const N: usize>(bytes: &[u8], buf: &'a mut [u8; N]) -> *const c_char {
    let len = bytes.len().min(N - 1);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf[len] = 0;
    buf.as_ptr() as *const c_char
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

////////////////////////////////////
// Zero-allocation integer parsing from ASCII bytes

/// Trait for primitive integers that can be parsed from raw ASCII bytes.
pub trait FromAsciiBytes: Sized {
    fn parse_ascii(bytes: &[u8]) -> Option<Self>;
}

macro_rules! impl_from_ascii_signed {
    ($($t:ty),*) => {
        $(
            impl FromAsciiBytes for $t {
                fn parse_ascii(bytes: &[u8]) -> Option<Self> {
                    if bytes.is_empty() {
                        return None;
                    }
                    let (is_neg, digits) = match bytes {
                        [b'-', rest @ ..] if !rest.is_empty() => (true, rest),
                        [b'+', rest @ ..] if !rest.is_empty() => (false, rest),
                        _ => (false, bytes),
                    };

                    let mut acc: $t = 0;
                    for &b in digits {
                        if !b.is_ascii_digit() {
                            return None;
                        }
                        let digit = (b - b'0') as $t;
                        acc = acc.checked_mul(10)?.checked_add(digit)?;
                    }

                    if is_neg {
                        acc.checked_neg()
                    } else {
                        Some(acc)
                    }
                }
            }
        )*
    };
}

macro_rules! impl_from_ascii_unsigned {
    ($($t:ty),*) => {
        $(
            impl FromAsciiBytes for $t {
                fn parse_ascii(bytes: &[u8]) -> Option<Self> {
                    if bytes.is_empty() {
                        return None;
                    }
                    let digits = match bytes {
                        [b'+', rest @ ..] if !rest.is_empty() => rest,
                        _ => bytes,
                    };

                    let mut acc: $t = 0;
                    for &b in digits {
                        if !b.is_ascii_digit() {
                            return None;
                        }
                        let digit = (b - b'0') as $t;
                        acc = acc.checked_mul(10)?.checked_add(digit)?;
                    }
                    Some(acc)
                }
            }
        )*
    };
}

impl_from_ascii_signed!(i8, i16, i32, i64, isize);
impl_from_ascii_unsigned!(u8, u16, u32, u64, usize);

/// Parse `&[u8]` into any type implementing `FromAsciiBytes`.
#[inline]
pub fn parse_bytes<T: FromAsciiBytes>(bytes: &[u8]) -> Option<T> {
    T::parse_ascii(bytes)
}

////////////////////////////////////
// Shared-memory mapping + opaque handle registry
//
// Used by the `barrier`, `mutex` and `semaphore` subcommands. Each primitive
// lives in shared memory (anonymous, shared across forked processes, or a named
// shared-memory object via `shm_open`) and is referenced from bash by an opaque
// integer handle. The handle maps to `(mmap pointer, optional shm name)` in a
// per-kind registry; only `create`/`open` assign a handle into a shell variable,
// every other subcommand resolves the integer value directly.
////////////////////////////////////

/// Handle kind tags so the three registries never collide.
#[allow(dead_code)]
pub(crate) const HANDLE_KIND_BARRIER: u8 = 1;
#[allow(dead_code)]
pub(crate) const HANDLE_KIND_MUTEX: u8 = 2;
#[allow(dead_code)]
pub(crate) const HANDLE_KIND_SEMAPHORE: u8 = 3;

/// One registry entry: the mapped base pointer and, for a named object, the
/// name to unlink on `destroy`.
pub(crate) struct HandleEntry {
    pub ptr: *mut u8,
    pub name: Option<CString>,
}

thread_local! {
    static HANDLES: RefCell<HashMap<(u8, u64), HandleEntry>> = RefCell::new(HashMap::new());
}

/// Monotonic, process-global generator for opaque integer handles.
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Store a mapping under a fresh handle id and return that id.
pub(crate) fn store_handle(kind: u8, ptr: *mut u8, name: Option<CString>) -> u64 {
    let id = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    HANDLES.with(|m| m.borrow_mut().insert((kind, id), HandleEntry { ptr, name }));
    id
}

/// Resolve a handle id to its base pointer (or `None` if unknown).
pub(crate) fn lookup_handle(kind: u8, id: u64) -> Option<*mut u8> {
    HANDLES.with(|m| m.borrow().get(&(kind, id)).map(|e| e.ptr))
}

/// Remove a registry entry, returning its pointer + optional name.
pub(crate) fn take_handle(kind: u8, id: u64) -> Option<HandleEntry> {
    HANDLES.with(|m| m.borrow_mut().remove(&(kind, id)))
}

/// Map `size` bytes of anonymous shared memory (shared across forked processes).
pub(crate) fn map_anonymous(size: usize) -> Result<*mut u8, String> {
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_ANONYMOUS | libc::MAP_SHARED,
            -1,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        return Err("mmap failed".into());
    }
    Ok(ptr as *mut u8)
}

/// Map `size` bytes backed by a named shared-memory object.
///
/// `create` chooses `O_CREAT` (and `ftruncate`s to `size`); otherwise the object
/// must already exist.
pub(crate) fn map_named(name: &CStr, size: usize, create: bool) -> Result<*mut u8, String> {
    let flags = if create {
        libc::O_CREAT | libc::O_RDWR
    } else {
        libc::O_RDWR
    };
    let fd = unsafe { libc::shm_open(name.as_ptr(), flags, 0o600) };
    if fd < 0 {
        return Err(format!(
            "shm_open {} failed: {}",
            name.to_str().unwrap_or("?"),
            std::io::Error::last_os_error()
        ));
    }
    if create {
        if unsafe { libc::ftruncate(fd, size as libc::off_t) } != 0 {
            unsafe { libc::close(fd) };
            return Err(format!(
                "ftruncate failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    unsafe { libc::close(fd) };
    if ptr == libc::MAP_FAILED {
        return Err("mmap failed".into());
    }
    Ok(ptr as *mut u8)
}

/// Unmap a previously mapped region.
pub(crate) fn unmap(ptr: *mut u8, size: usize) {
    unsafe { libc::munmap(ptr as *mut libc::c_void, size) };
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

/// Bind opaque integer handle `id` into the shell variable named by `var`.
pub(crate) fn bind_handle(var: &Cpnt, id: u64) -> c_int {
    let val = CString::new(id.to_string()).unwrap_or_default();
    if unsafe { bind_variable(var.as_ptr() as *const c_char, val.as_ptr(), 0) }.is_null() {
        l_builtin_error!(b"failed to bind variable");
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}