//! Top-level L_builtin dispatch (Rust entry point called by bash)
//!
//! Bash calls `L_builtin_builtin` directly. This function reads the
//! subcommand name (first word), prints help on `-h`/`--help`, looks the
//! subcommand up in a dispatch table, and calls the appropriate handler with
//! the word list advanced past the subcommand name.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{
    EX_USAGE, WORD_LIST, c_char, c_int, l_word_desc_string, l_word_list_next, l_word_list_word,
};

use std::ffi::CStr;
use std::io::Write;

// C subcommand handlers (compiled into the same .so)
extern "C" {
    fn lseek_subcommand(list: *mut WORD_LIST) -> c_int;
    fn poll_subcommand(list: *mut WORD_LIST) -> c_int;
    #[cfg(feature = "ppoll")]
    fn ppoll_subcommand(list: *mut WORD_LIST) -> c_int;
    fn sigmask_subcommand(list: *mut WORD_LIST) -> c_int;
    fn sigunmask_subcommand(list: *mut WORD_LIST) -> c_int;
    fn pipe_subcommand(list: *mut WORD_LIST) -> c_int;
    fn listen_subcommand(list: *mut WORD_LIST) -> c_int;
    fn accept_subcommand(list: *mut WORD_LIST) -> c_int;
    fn connect_subcommand(list: *mut WORD_LIST) -> c_int;
    fn shutdown_subcommand(list: *mut WORD_LIST) -> c_int;
    fn send_subcommand(list: *mut WORD_LIST) -> c_int;
    fn recv_subcommand(list: *mut WORD_LIST) -> c_int;
    fn sleep_subcommand(list: *mut WORD_LIST) -> c_int;
}

// Rust subcommand handlers are defined in this crate (cmd_core.rs / cmd_lua.rs)
// and referenced directly below.

// Small C helpers that need access to bash globals (this_command_name, docs).
extern "C" {
    fn l_builtin_print_usage();
    fn l_builtin_print_help();
    fn l_builtin_unknown_subcommand(name: *const c_char);
    /// glibc fflush; called with NULL to flush all C stdio output streams.
    fn fflush(stream: *mut core::ffi::c_void) -> c_int;
}

type SubcommandFn = unsafe extern "C" fn(*mut WORD_LIST) -> c_int;

/// Dispatch table: subcommand name -> handler.
///
/// Sorted by name at compile time (`sort_by_byte_key` is a const fn), so
/// lookups can use binary search.
const SUBCOMMAND_TABLE: &[(&[u8], SubcommandFn)] = &crate::shared::sort_by_byte_key([
    (b"lseek" as &[u8], lseek_subcommand as SubcommandFn),
    (b"poll", poll_subcommand),
    #[cfg(feature = "ppoll")]
    (b"ppoll", ppoll_subcommand),
    (b"sigmask", sigmask_subcommand),
    (b"sigunmask", sigunmask_subcommand),
    (b"pipe", pipe_subcommand),
    (b"listen", listen_subcommand),
    (b"accept", accept_subcommand),
    (b"connect", connect_subcommand),
    (b"shutdown", shutdown_subcommand),
    (b"send", send_subcommand),
    (b"recv", recv_subcommand),
    (b"sleep", sleep_subcommand),
    (b"core", crate::cmd_core::l_core_subcommand),
    (b"lua", crate::cmd_lua::l_lua_subcommand),
]);

#[no_mangle]
pub extern "C" fn L_builtin_builtin(list: *mut WORD_LIST) -> c_int {
    if list.is_null() {
        unsafe { l_builtin_print_usage() };
        return EX_USAGE;
    }

    // Read the subcommand name (first word) using C shims.
    let word_desc = unsafe { l_word_list_word(list) };
    if word_desc.is_null() {
        unsafe { l_builtin_print_usage() };
        return EX_USAGE;
    }
    let str_ptr = unsafe { l_word_desc_string(word_desc) };
    if str_ptr.is_null() {
        unsafe { l_builtin_print_usage() };
        return EX_USAGE;
    }
    let name = unsafe { CStr::from_ptr(str_ptr).to_bytes() };

    if name == b"-h" || name == b"--help" {
        unsafe { l_builtin_print_help() };
        return EX_USAGE;
    }

    // Find the handler for this subcommand name (table is sorted at compile
    // time, so binary search applies).
    let handler = SUBCOMMAND_TABLE
        .binary_search_by(|(n, _)| n.cmp(&name))
        .ok()
        .map(|i| SUBCOMMAND_TABLE[i].1);

    match handler {
        Some(f) => {
            // Advance past the subcommand name; handlers expect the list to
            // start at the first real argument.
            let next = unsafe { l_word_list_next(list) };
            // Flush C stdio before the handler so buffered bash/C output
            // cannot be reordered against direct fd writes from Rust.
            unsafe { fflush(std::ptr::null_mut()) };
            let ret = unsafe { f(next) };
            // Flush both layers after the handler: C stdio (C/Lua handlers)
            // and Rust's stdout buffer (never flushed at exit in a cdylib).
            unsafe { fflush(std::ptr::null_mut()) };
            let _ = std::io::stdout().flush();
            ret
        }
        None => {
            unsafe { l_builtin_unknown_subcommand(str_ptr) };
            EX_USAGE
        }
    }
}
