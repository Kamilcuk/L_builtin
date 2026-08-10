//! Subcommand dispatch helpers: append the subcommand name to
//! `this_command_name` and temporarily replace `current_builtin`'s docs while
//! a subcommand runs.

#![allow(non_upper_case_globals)]

use std::os::raw::{c_char, c_int};

use crate::bash_api::{
    builtin, current_builtin, l_xfree, l_xmalloc, l_xrealloc, this_command_name, WORD_LIST,
};

/// Backing storage for `current_builtin->long_doc`: a NULL-terminated array
/// of C strings.
static mut LONG_DOC_ARRAY: [*mut c_char; 2] = [std::ptr::null_mut(); 2];

/// Owned NUL-terminated copies for doc strings passed without a trailing NUL.
/// The previous copy is freed on the next `enter_subcommand` call, so these
/// pointers stay valid for the duration of the current subcommand.
static mut OWNED_SHORT_DOC: *mut c_char = std::ptr::null_mut();
static mut OWNED_LONG_DOC: *mut c_char = std::ptr::null_mut();

/// Return a stable NUL-terminated C string for `bytes`: the borrowed slice
/// pointer when it already ends with `\0`, otherwise an owned copy kept in
/// `*owned` (freed on the next call).
unsafe fn doc_cstr(bytes: &[u8], owned: *mut *mut c_char) -> *mut c_char {
    if bytes.last() == Some(&0) {
        return bytes.as_ptr() as *mut c_char;
    }
    unsafe {
        if !owned.is_null() {
            l_xfree((*owned).cast());
        }
        let buf = l_xmalloc(bytes.len() + 1) as *mut c_char;
        std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), buf, bytes.len());
        *buf.add(bytes.len()) = 0;
        *owned = buf;
        buf
    }
}

/// Enter a subcommand context: append `name` to `this_command_name` and
/// replace `current_builtin`'s `short_doc` (a single NUL-terminated C string)
/// and `long_doc` (`short_doc`-style text wrapped as a NULL-terminated array
/// of C strings). NUL terminators are added if missing in the slices.
///
/// Call [`SubcommandGuard::new`] first so the previous doc pointers and struct
/// are remembered and restored on drop; bash unwinds `this_command_name` back
/// to its pre-invocation value after the builtin returns, so it needs no
/// restore.
///
/// # Safety
/// `current_builtin` must be non-NULL (i.e. called while bash is executing a
/// builtin).
pub unsafe fn enter_subcommand(name: &[u8], short_doc: &[u8], long_doc: &[u8]) {
    // this_command_name += " name"
    let old_len = if this_command_name.is_null() {
        0
    } else {
        unsafe { libc::strlen(this_command_name) }
    };
    let new_len = old_len + 1 + name.len() + 1; // separator + name + NUL
    let buf = unsafe { l_xrealloc(this_command_name.cast(), new_len) } as *mut c_char;
    let mut off = old_len;
    if old_len > 0 {
        unsafe { *buf.add(off) = b' ' as c_char };
        off += 1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(name.as_ptr().cast::<c_char>(), buf.add(off), name.len());
        *buf.add(off + name.len()) = 0;
    }
    unsafe { this_command_name = buf };

    // Replace current_builtin's doc pointers.
    let short_doc = unsafe { doc_cstr(short_doc, &raw mut OWNED_SHORT_DOC) };
    let long_doc = unsafe { doc_cstr(long_doc, &raw mut OWNED_LONG_DOC) };
    unsafe {
        LONG_DOC_ARRAY = [long_doc, std::ptr::null_mut()];
        (*current_builtin).short_doc = short_doc;
        (*current_builtin).long_doc = std::ptr::addr_of!(LONG_DOC_ARRAY).cast();
    }
}

/// RAII guard: constructed with [`SubcommandGuard::new`] before calling
/// [`enter_subcommand`] to remember the current `current_builtin` and its doc
/// pointers; on drop restores them on the same struct.
pub struct SubcommandGuard {
    saved_builtin: *mut builtin,
    saved_short_doc: *const c_char,
    saved_long_doc: *const *mut c_char,
}

impl SubcommandGuard {
    /// Save the current `current_builtin` pointer and its `short_doc`/
    /// `long_doc`, so they can be restored on drop.
    pub fn new() -> Self {
        unsafe {
            debug_assert!(!current_builtin.is_null(), "current_builtin is null");
            SubcommandGuard {
                saved_builtin: current_builtin,
                saved_short_doc: (*current_builtin).short_doc,
                saved_long_doc: (*current_builtin).long_doc,
            }
        }
    }
}

impl Drop for SubcommandGuard {
    fn drop(&mut self) {
        unsafe {
            (*self.saved_builtin).short_doc = self.saved_short_doc;
            (*self.saved_builtin).long_doc = self.saved_long_doc;
        }
    }
}

/// Function implementing a subcommand.
pub type SubcommandFn = unsafe extern "C" fn(*mut WORD_LIST) -> c_int;

/// A subcommand of the main builtin: the doc strings shown while it runs
/// (`short_doc` usage line, `long_doc` full help) and the function that
/// implements it.
#[derive(Debug, Clone, Copy)]
pub struct Subcommand {
    pub short_doc: &'static [u8],
    pub long_doc: &'static [u8],
    pub func: SubcommandFn,
}

impl Subcommand {
    /// Run the subcommand: append `name` to `this_command_name` and replace
    /// `current_builtin`'s docs with this subcommand's, then invoke `func`
    /// with `list`.
    ///
    /// The caller (e.g. the dispatch code) is responsible for constructing a
    /// [`SubcommandGuard`] beforehand so the previous docs are restored.
    ///
    /// # Safety
    /// `list` must be a valid `WORD_LIST` (bash is executing a builtin).
    pub unsafe fn call(&self, name: &[u8], list: *mut WORD_LIST) -> c_int {
        unsafe { enter_subcommand(name, self.short_doc, self.long_doc) };
        unsafe { (self.func)(list) }
    }
}


