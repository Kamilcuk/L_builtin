//! Top-level L_builtin dispatch (Rust entry point called by bash)
//!
//! Bash calls `L_builtin_builtin` directly. This function reads the
//! subcommand name (first word), prints help on `-h`/`--help`, looks the
//! subcommand up in a dispatch table, and calls the appropriate handler with
//! the word list advanced past the subcommand name.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use getargs::Opt::{Long, Short};

use crate::bash_api::{
    c_int, l_execute_word_list, this_command_name, WordListView, EX_USAGE, WORD_LIST,
};
use crate::bprintln;
use crate::shared::{capture_into_variable, getargs_unexpected};
use crate::{beprintln, return_on_err};
use std::ffi::c_char;
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
    fn fflush(stream: *mut core::ffi::c_void) -> c_int;
    pub static L_builtin_doc: [*const c_char; 0];
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
unsafe fn l_builtin_print_help() {
    let mut p = L_builtin_doc.as_ptr();
    while !(*p).is_null() {
        bprintln!(*p);
        p = p.add(1);
    }
}

unsafe fn l_builtin_print_usage() {
    let cmd_name = this_command_name;
    if !cmd_name.is_null() && *cmd_name != 0 {
        bprintln!(cmd_name, b": usage:");
    }
    let doc_array = L_builtin_doc.as_ptr();
    let short_doc_ptr = *doc_array.add(2);
    if !short_doc_ptr.is_null() {
        bprintln!(short_doc_ptr);
    }
}
unsafe fn l_builtin_unknown_subcommand(name: &[u8]) {
    let cmd_name = this_command_name;
    if !cmd_name.is_null() && *cmd_name != 0 {
        beprintln!(cmd_name, b": unknown subcommand:", name);
    } else {
        beprintln!(b"unknown subcommand:", name);
    }
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
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn capture_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut args = WordListView::from_raw(list).into_iter();
    // var is a slice from bash WORD_LIST; use the original C string pointer.
    // l_word_desc_string returns a NUL-terminated C string.
    let var_ptr = args.current_cpnt();
    if var_ptr.is_null() {
        beprintln!(b"L_builtin capture: usage: L_builtin capture VAR <command> [args...]");
        return EX_USAGE;
    };
    args.advance();
    if args.current_cpnt().is_null() {
        beprintln!(b"L_builtin capture: missing command");
        return EX_USAGE;
    }
    // We need the var pointer from the first element. Since we already advanced
    // the iterator, we need to get it from the original list.
    // Actually, let's use find_variable with the name directly.
    capture_into_variable("L_builtin capture", var_ptr, || unsafe {
        l_execute_word_list(args.head)
    })
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn L_builtin_builtin(list: *mut WORD_LIST) -> c_int {
    let mut list = unsafe { WordListView::from_raw(list) }.into_iter();
    let mut opts = getargs::Options::new(&mut list);
    if let Some(opt) = return_on_err!("L_builtin", opts.next_opt(), EX_USAGE) {
        match opt {
            Short(b'h') | Long(b"help") => {
                unsafe { l_builtin_print_help() };
                return 0;
            }
            _ => return getargs_unexpected("L_builtin", opt),
        }
    }
    let val = match opts.next_positional() {
        Some(val) => val,
        None => {
            unsafe { l_builtin_print_usage() };
            return EX_USAGE;
        }
    };
    // Find the handler for this subcommand name (table is sorted at compile
    // time, so binary search applies).
    let subcommand = match SUBCOMMAND_TABLE.binary_search_by(|(n, _)| n.cmp(&val)) {
        Ok(i) => SUBCOMMAND_TABLE[i].1,
        Err(_) => {
            unsafe { l_builtin_unknown_subcommand(val) };
            return EX_USAGE;
        }
    };
    // Flush C stdio before the handler so buffered bash/C output
    // cannot be reordered against direct fd writes from Rust.
    unsafe { fflush(std::ptr::null_mut()) };
    let ret = unsafe { subcommand(list.head) };
    // Flush both layers after the handler: C stdio (C/Lua handlers)
    // and Rust's stdout buffer (never flushed at exit in a cdylib).
    unsafe { fflush(std::ptr::null_mut()) };
    let _ = std::io::stdout().flush();
    ret
}
