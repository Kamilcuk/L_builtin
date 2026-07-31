//! Global allocator routing every Rust allocation through bash's allocator.
//!
//! The `.so` is dlopen'd into the bash process, so `xmalloc`/`xrealloc`/
//! `xfree` resolve to whichever allocator that bash was built with: glibc
//! malloc normally, or bash's internal allocator under `USING_BASH_MALLOC`.
//! Using them here keeps Rust and bash on a single heap on every build,
//! rather than only on builds where both happen to bottom out in glibc.
//!
//! This does NOT make it legal for bash to free Rust-owned memory or vice
//! versa: `GlobalAlloc` still requires that each allocation be released
//! through the same interface (and with the same `Layout`) it came from.
//! Sharing a heap removes the allocator-mismatch hazard, not the ownership
//! rule. The FFI boundary stays copy-based.
//!
//! Failure policy: allocation failure and over-aligned requests both abort.
//! `xmalloc` already kills the shell via `allocerr()` on OOM, so returning
//! null from `alloc()` would only trade one abort for a less informative
//! one; the null checks below are belt-and-braces for a build where
//! `xmalloc` might return.

use std::alloc::{GlobalAlloc, Layout};
use std::os::raw::c_void;

extern "C" {
    /// Bash's checked malloc. Calls `allocerr()` (which exits the shell) on
    /// failure, so it does not return null in practice.
    fn xmalloc(bytes: usize) -> *mut c_void;
    /// Bash's checked realloc. Same failure behaviour as `xmalloc`.
    fn xrealloc(ptr: *mut c_void, bytes: usize) -> *mut c_void;
    /// Bash's free. Tolerates null.
    fn xfree(ptr: *mut c_void);
}

/// Alignment guaranteed by `xmalloc`, matching what C's `malloc` guarantees:
/// suitable for any fundamental type. 16 bytes on x86-64.
const MAX_ALIGN: usize = std::mem::align_of::<libc::max_align_t>();

/// Report a fatal allocator condition and abort.
///
/// Writes straight to fd 2 with `write(2)` and aborts via `libc::abort`.
/// Nothing here allocates, formats, or unwinds -- all three are unavailable
/// or unsafe on this path (we may be inside a failing allocation, and the
/// crate is built with `panic = "abort"`).
#[cold]
#[inline(never)]
fn fatal(msg: &str) -> ! {
    unsafe {
        libc::write(2, msg.as_ptr().cast(), msg.len());
        libc::abort()
    }
}

pub struct BashAllocator;

unsafe impl GlobalAlloc for BashAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.align() > MAX_ALIGN {
            fatal("L_builtin: allocation alignment exceeds max_align_t\n");
        }
        let ptr = xmalloc(layout.size());
        if ptr.is_null() {
            fatal("L_builtin: out of memory\n");
        }
        ptr.cast()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        xfree(ptr.cast());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Alignment is unchanged by realloc, and `alloc` already rejected
        // anything above MAX_ALIGN, so xrealloc's guarantee still holds.
        debug_assert!(layout.align() <= MAX_ALIGN);
        let ptr = xrealloc(ptr.cast(), new_size);
        if ptr.is_null() {
            fatal("L_builtin: out of memory\n");
        }
        ptr.cast()
    }
}
