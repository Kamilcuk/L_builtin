//! Subcommand dispatch helpers: temporarily replace `current_builtin`'s doc
//! pointers while a subcommand runs and restore them afterwards.

#![allow(non_upper_case_globals)]

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

use crate::bash_api::{builtin, current_builtin, WORD_LIST};

/// A constant description of one L_builtin subcommand: its name plus the
/// short (usage line) and long (full help) documentation. Build with
/// [`CmdDesc::new`] in a `const` (using `c"..."` literals, which are
/// NUL-terminated at compile time) and call [`CmdDesc::enter`] at the top of
/// the subcommand to enter the subcommand context.
pub struct CmdDesc {
    /// Subcommand name, shown after `L_builtin` in `this_command_name`.
    pub name: &'static CStr,
    /// Usage line, printed after `<this_command_name>: usage:`.
    pub short_doc: &'static CStr,
    /// Full help text (description, options, exit status).
    pub long_doc: &'static CStr,
}

/// Constant check that a C string ends with the NUL terminator (the pieces of
/// a `CmdDesc` must be NUL-terminated: C code does `strlen()` on them).
const fn cstr_is_nul_terminated(c: &CStr) -> bool {
    match c.to_bytes_with_nul().last() {
        Some(&0) => true,
        _ => false,
    }
}

impl CmdDesc {
    /// Build a constant subcommand description. All three arguments are
    /// NUL-terminated C strings baked at compile time (no runtime work). In
    /// debug builds the NUL termination is asserted here at compile time.
    pub const fn new(
        name: &'static CStr,
        short_doc: &'static CStr,
        long_doc: &'static CStr,
    ) -> Self {
        debug_assert!(cstr_is_nul_terminated(name));
        debug_assert!(cstr_is_nul_terminated(short_doc));
        debug_assert!(cstr_is_nul_terminated(long_doc));
        Self {
            name,
            short_doc,
            long_doc,
        }
    }

    /// Enter the subcommand context: `l_enter_subcommand` appends `" name"` to
    /// `this_command_name` and points `current_builtin` at these docs, so that
    /// `-h` (via `l_builtin_usage_long`) shows this subcommand's help.
    pub fn enter(&self) {
        unsafe {
            crate::bash_api::l_enter_subcommand(
                self.name.as_ptr().cast_mut(),
                self.short_doc.as_ptr().cast_mut(),
                self.long_doc.as_ptr().cast_mut(),
            )
        };
    }
}

/// RAII guard: constructed with [`SubcommandGuard::new`] before calling
/// [`CmdDesc::enter`] to remember the current `current_builtin` and its doc
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