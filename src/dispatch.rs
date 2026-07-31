//! Top-level L_builtin dispatch (Rust entry point called by bash)
//!
//! Bash calls `L_builtin_builtin` directly. This function reads the
//! subcommand name (first word), prints help on `-h`/`--help`, looks the
//! subcommand up in a dispatch table, and calls the appropriate handler with
//! the word list advanced past the subcommand name.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::{c_char, CStr};
use std::io::{stdout, stderr, Write};
use crate::bash_api::{
    c_int, l_execute_word_list, l_word_desc_string, l_word_list_next, l_word_list_word,
    EX_USAGE, WORD_LIST,     this_command_name
};
use crate::shared::capture_into_variable;

use std::ffi::{OsStr};
use std::os::unix::ffi::OsStrExt;

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
    fn fflush(stream: *mut core::ffi::c_void) -> c_int;
    pub static L_builtin_doc: *const *const c_char;
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
    (b"capture", capture_subcommand),
]);

/// Iterates through a NULL-terminated array of C string pointers (`*const *const c_char`)
/// and streams each string directly to stdout, followed by a newline.
///
/// # Safety
///
/// `arr` must be null or point to a readable, NULL-terminated array of valid C string pointers.
pub unsafe fn print_arr(mut arr: *const *const c_char) {
    if arr.is_null() {
        return;
    }

    let mut handle = stdout().lock();
    while !(*arr).is_null() {
        let cstr = CStr::from_ptr(*arr);
        let _ = handle.write_all(cstr.to_bytes());
        let _ = handle.write_all(b"\n");
        arr = arr.add(1);
    }
}

/// Equivalent to `l_builtin_print_usage` using `L_builtin_doc[2]` for `short_doc`.
pub unsafe fn l_builtin_print_usage() {
    let mut err = stderr().lock();

    if !this_command_name.is_null() && *this_command_name != 0 {
        let name = CStr::from_ptr(this_command_name).to_bytes();
        let _ = err.write_all(name);
        let _ = err.write_all(b": usage: ");
    }

    if !L_builtin_doc.is_null() {
        let short_doc_ptr = *L_builtin_doc.add(2);
        if !short_doc_ptr.is_null() {
            let doc = CStr::from_ptr(short_doc_ptr).to_bytes();
            let _ = err.write_all(doc);
        }
    }
    let _ = err.write_all(b"\n");
}

/// Equivalent to `l_builtin_print_help` in C.
pub unsafe fn l_builtin_print_help() {
    print_arr(L_builtin_doc);
}

/// Equivalent to `l_builtin_unknown_subcommand` in C.
pub unsafe fn l_builtin_unknown_subcommand(name: *const c_char) {
    let mut err = stderr().lock();

    if !this_command_name.is_null() && *this_command_name != 0 {
        let cmd = CStr::from_ptr(this_command_name).to_bytes();
        let _ = err.write_all(cmd);
        let _ = err.write_all(b": ");
    }

    let _ = err.write_all(b"unknown subcommand: ");
    if !name.is_null() {
        let sub = CStr::from_ptr(name).to_bytes();
        let _ = err.write_all(sub);
    }
    let _ = err.write_all(b"\n");
}

/// `capture VAR <command> [args...]`: run the command with stdout captured
/// into VAR (memfd redirection).
///
/// The command is always executed through the shell, so external commands,
/// shell functions, builtins, and L_builtin subcommands all work uniformly.
/// Words are single-quoted before being joined, so arguments reach the
/// command verbatim (no re-splitting or globbing).
///
/// Lives in the dispatch table with the same C ABI as every other handler;
/// `list` starts at VAR (the dispatcher already consumed the word `capture`).
#[no_mangle]
pub extern "C" fn capture_subcommand(list: *mut WORD_LIST) -> c_int {
    let Some(var_ptr) = first_word(list) else {
        eprintln!("L_builtin capture: usage: L_builtin capture VAR <command> [args...]");
        return EX_USAGE;
    };
    let var = OsStr::from_bytes(unsafe { CStr::from_ptr(var_ptr).to_bytes() });
    let cmd_list = unsafe { l_word_list_next(list) };
    if first_word(cmd_list).is_none() {
        eprintln!("L_builtin capture: missing command");
        return EX_USAGE;
    }
    capture_into_variable(
        "L_builtin capture",
        var,
        || unsafe { l_execute_word_list(cmd_list) },
    )
}

/// Read the first word of `list` as a C string pointer, or None if any
/// pointer on the way is null.
fn first_word(list: *mut WORD_LIST) -> Option<*const c_char> {
    if list.is_null() {
        return None;
    }
    let word_desc = unsafe { l_word_list_word(list) };
    if word_desc.is_null() {
        return None;
    }
    let str_ptr = unsafe { l_word_desc_string(word_desc) };
    (!str_ptr.is_null()).then_some(str_ptr)
}

#[no_mangle]
pub extern "C" fn L_builtin_builtin(list: *mut WORD_LIST) -> c_int {
    let Some(str_ptr) = first_word(list) else {
        unsafe { l_builtin_print_usage() };
        return EX_USAGE;
    };
    let name = unsafe { CStr::from_ptr(str_ptr).to_bytes() };

    if name == b"-h" || name == b"--help" {
        unsafe { l_builtin_print_help() };
        return EX_USAGE;
    }

    // Find the handler for this subcommand name (table is sorted at compile
    // time, so binary search applies).
    match SUBCOMMAND_TABLE
        .binary_search_by(|(n, _)| n.cmp(&name))
        .ok()
        .map(|i| SUBCOMMAND_TABLE[i].1)
    {
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
