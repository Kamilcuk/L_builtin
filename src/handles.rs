//! Per-kind opaque handle registries for the synchronization primitives.
//!
//! Each primitive (barrier, mutex, semaphore) owns its own `HandleRegistry`,
//! so handle ids never need a kind tag to resolve.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::io;
use std::os::raw::{c_char, c_int};

use libc;

use crate::bash_api::{bind_variable, EXECUTION_FAILURE, EXECUTION_SUCCESS};
use crate::l_builtin_error;

/// One registry entry: the mapped base pointer and, for a named object, the
/// name to unlink on `destroy`.
pub(crate) struct HandleEntry {
    pub ptr: *mut u8,
    pub name: Option<CString>,
}

/// A registry mapping opaque integer handles to their backing data. Each
/// synchronization primitive owns its own instance, so handles never need a
/// kind tag to resolve.
pub(crate) struct HandleRegistry {
    inner: RefCell<RegistryInner>,
}

struct RegistryInner {
    map: HashMap<u64, HandleEntry>,
    next: u64,
}

impl HandleRegistry {
    pub(crate) fn new() -> Self {
        HandleRegistry {
            inner: RefCell::new(RegistryInner {
                map: HashMap::new(),
                next: 1,
            }),
        }
    }

    /// Store a mapping under a fresh handle id and return that id.
    pub(crate) fn store(&self, ptr: *mut u8, name: Option<CString>) -> u64 {
        let mut inner = self.inner.borrow_mut();
        let id = inner.next;
        inner.next += 1;
        inner.map.insert(id, HandleEntry { ptr, name });
        id
    }

    /// Resolve a handle id to its base pointer (or `None` if unknown).
    pub(crate) fn lookup(&self, id: u64) -> Option<*mut u8> {
        self.inner.borrow().map.get(&id).map(|e| e.ptr)
    }

    /// Remove a registry entry, returning its pointer + optional name.
    pub(crate) fn take(&self, id: u64) -> Option<HandleEntry> {
        self.inner.borrow_mut().map.remove(&id)
    }

    /// Invoke `f` once per registered handle, passing `(id, ptr)`.
    pub(crate) fn for_each(&self, mut f: impl FnMut(u64, *mut u8)) {
        for (&id, entry) in self.inner.borrow().map.iter() {
            f(id, entry.ptr);
        }
    }
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
            io::Error::last_os_error()
        ));
    }
    if create {
        if unsafe { libc::ftruncate(fd, size as libc::off_t) } != 0 {
            unsafe { libc::close(fd) };
            return Err(format!("ftruncate failed: {}", io::Error::last_os_error()));
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
