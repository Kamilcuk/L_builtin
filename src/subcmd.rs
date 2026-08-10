//! Subcommand dispatch helpers: temporarily replace `current_builtin`'s doc
//! pointers while a subcommand runs and restore them afterwards.

#![allow(non_upper_case_globals)]

use std::os::raw::{c_char, c_int};

use crate::bash_api::{builtin, current_builtin, WORD_LIST};

/// RAII guard: constructed with [`SubcommandGuard::new`] before calling
/// [`crate::bash_api::l_enter_subcommand`] to remember the current
/// `current_builtin` and its doc pointers; on drop restores them on the same
/// struct.
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
