//! Global allocator routing every Rust allocation through bash's allocator.
//!
//! The `.so` is dlopen'd into the bash process, so `sh_xmalloc`/`sh_xrealloc`/
//! `sh_xfree` resolve to whichever allocator that bash was built with: glibc
//! malloc normally, or bash's internal allocator under `USING_BASH_MALLOC`.
//! Using them here keeps Rust and bash on a single heap on every build,
//! rather than only on builds where both happen to bottom out in glibc.
//! Sharing a heap removes the allocator-mismatch hazard, not the ownership
//! rule. The FFI boundary stays copy-based.
//!
//! Failure policy: allocation failure and over-aligned requests both abort.
//! `sh_xmalloc` already kills the shell via `allocerr()` on OOM, so returning
//! null from `alloc()` would only trade one abort for a less informative
//! one; the null checks below are belt-and-braces for a build where
//! `sh_xmalloc` might return.
//!
//! Alignment: bash 5.0's internal malloc (USING_BASH_MALLOC) may not provide
//! 16-byte alignment required by SSE2 instructions. We ensure alignment by
//! over-allocating and adjusting the pointer, storing the original pointer
//! in the preceding bytes for deallocation.

use std::alloc::{GlobalAlloc, Layout};

/// Alignment guaranteed by `sh_xmalloc`, matching what C's `malloc` guarantees:
/// suitable for any fundamental type. 16 bytes on x86-64.
const MAX_ALIGN: usize = std::mem::align_of::<libc::max_align_t>();

/// Report a fatal allocator condition and abort.
#[cold]
#[inline(never)]
fn fatal(msg: &str) -> ! {
    unsafe {
        libc::write(2, msg.as_ptr().cast(), msg.len());
        libc::abort()
    }
}

/// Allocate `size + align` bytes, align (raw + 1) to `align`,
/// store the offset in the byte before the aligned pointer.
unsafe fn alloc_aligned(raw: *mut u8, size: usize, align: usize) -> *mut u8 {
    let total_size = size + align;
    let raw: *mut u8 = crate::bash_api::l_xrealloc(raw.cast(), total_size).cast();
    if raw.is_null() {
        fatal("L_builtin: out of memory\n");
    }
    // Align (raw + 1) so aligned - 1 >= raw
    let addr = raw.add(1) as usize;
    let aligned_addr = (addr + align - 1) & !(align - 1);
    let aligned_ptr = aligned_addr as *mut u8;
    let offset = aligned_addr - (raw.add(1) as usize);
    // Store offset in the byte before aligned pointer (valid since aligned_ptr >= raw + 1)
    *aligned_ptr.sub(1) = offset as u8;
    aligned_ptr
}

/// Recover raw from an aligned pointer returned by alloc_aligned.
unsafe fn raw_from_aligned(ptr: *mut u8) -> *mut u8 {
    let offset = *ptr.sub(1) as usize;
    debug_assert!(offset < MAX_ALIGN);
    ptr.sub(1).sub(offset)
}

pub struct BashAllocator;

unsafe impl GlobalAlloc for BashAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.align() > MAX_ALIGN {
            fatal("L_builtin: allocation alignment exceeds max_align_t\n");
        }
        alloc_aligned(std::ptr::null_mut(), layout.size(), layout.align())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let raw = raw_from_aligned(ptr);
        crate::bash_api::l_xfree(raw.cast());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        debug_assert!(layout.align() <= MAX_ALIGN);
        let raw = raw_from_aligned(ptr);
        alloc_aligned(raw, new_size, layout.align())
    }
}
