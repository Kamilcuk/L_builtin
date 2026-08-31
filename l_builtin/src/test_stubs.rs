//! Test-only stand-ins for the bash C allocator symbols.
//!
//! The crate's global allocator (`BashAllocator`) routes every Rust allocation
//! through `l_xrealloc`/`l_xfree`, which are normally provided by bash at load
//! time. Under `cargo test` there is no bash process, so we define them here,
//! delegating to the system libc allocator. This module is compiled only for
//! `cargo test` (`#[cfg(test)]`); the real `.so` build gets these symbols from
//! bash and must NOT include these definitions.

#![allow(dead_code)]

use std::os::raw::c_void;

#[no_mangle]
pub unsafe extern "C" fn l_xrealloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    libc::realloc(ptr, size)
}

#[no_mangle]
pub unsafe extern "C" fn l_xfree(ptr: *mut c_void) {
    libc::free(ptr)
}
