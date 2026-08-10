//! Top-level L_builtin dispatch (Rust entry point called by bash)
//!
//! Bash calls `l_entrypoint` directly. This function reads the
//! subcommand name (first word), prints help on `-h`/`--help`, looks the
//! subcommand up in a dispatch table, and calls the appropriate handler with
//! the word list advanced past the subcommand name.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{c_int, this_command_name, WordListView, EX_USAGE, WORD_LIST};
use crate::intlookup::IntLookup128;
use crate::shared::{capture_into_variable, flush_stdout_buffers};
use crate::subcmd::{Subcommand, SubcommandGuard};
use crate::{bash_getopt, beprintln, bprintln, intlookup};
use std::ffi::c_char;

#[cfg(not(feature = "bash_lt_4_3"))]
use crate::bash_api::l_execute_command_string;

// C subcommand handlers (compiled into the same .so)
extern "C" {
    fn poll_subcommand(list: *mut WORD_LIST) -> c_int;
    #[cfg(feature = "ppoll")]
    fn ppoll_subcommand(list: *mut WORD_LIST) -> c_int;
    fn sigmask_subcommand(list: *mut WORD_LIST) -> c_int;
    fn sigunmask_subcommand(list: *mut WORD_LIST) -> c_int;
    fn l_cmd_ext(list: *mut WORD_LIST) -> c_int;
    #[link_name = "L_builtin_doc"]
    static L_BUILTIN_DOC: [*const c_char; 0];
}

const SUBCOMMAND_ENTRIES: &[(&str, Subcommand)] = &[
    (
        "lseek",
        Subcommand {
            short_doc: b"Reposition file offset\0",
            long_doc: b"\0",
            func: crate::lseek::lseek_subcommand,
        },
    ),
    (
        "poll",
        Subcommand {
            short_doc: b"Wait for file descriptors to become ready\0",
            long_doc: b"\0",
            func: poll_subcommand,
        },
    ),
    #[cfg(feature = "ppoll")]
    (
        "ppoll",
        Subcommand {
            short_doc: b"Wait for FDs and unblock signals atomically\0",
            long_doc: b"\0",
            func: ppoll_subcommand,
        },
    ),
    (
        "sigmask",
        Subcommand {
            short_doc: b"Block or unblock signals\0",
            long_doc: b"\0",
            func: sigmask_subcommand,
        },
    ),
    (
        "sigunmask",
        Subcommand {
            short_doc: b"Unblock signals and run a command\0",
            long_doc: b"\0",
            func: sigunmask_subcommand,
        },
    ),
    (
        "pipe",
        Subcommand {
            short_doc: b"Create a pipe\0",
            long_doc: b"\0",
            func: crate::pipe::pipe_subcommand,
        },
    ),
    (
        "listen",
        Subcommand {
            short_doc: b"Create a listening TCP socket\0",
            long_doc: b"\0",
            func: crate::listen::listen_subcommand,
        },
    ),
    (
        "accept",
        Subcommand {
            short_doc: b"Accept a network connection\0",
            long_doc: b"\0",
            func: crate::accept::accept_subcommand,
        },
    ),
    (
        "connect",
        Subcommand {
            short_doc: b"Establish a TCP connection\0",
            long_doc: b"\0",
            func: crate::connect::connect_subcommand,
        },
    ),
    (
        "shutdown",
        Subcommand {
            short_doc: b"Semi-close a network socket\0",
            long_doc: b"\0",
            func: crate::shutdown::shutdown_subcommand,
        },
    ),
    (
        "send",
        Subcommand {
            short_doc: b"Send bytes over a socket\0",
            long_doc: b"\0",
            func: crate::send::send_subcommand,
        },
    ),
    (
        "recv",
        Subcommand {
            short_doc: b"Receive bytes from a socket\0",
            long_doc: b"\0",
            func: crate::recv::recv_subcommand,
        },
    ),
    (
        "sleep",
        Subcommand {
            short_doc: b"High-precision sub-second sleep\0",
            long_doc: b"\0",
            func: crate::sleep::sleep_subcommand,
        },
    ),
    (
        "core",
        Subcommand {
            short_doc: b"Core utilities (ls, stat) via Rust/uutils\0",
            long_doc: b"\0",
            func: crate::cmd_core::l_core_subcommand,
        },
    ),
    (
        "lua",
        Subcommand {
            short_doc: b"Execute LuaJIT script\0",
            long_doc: b"\0",
            func: crate::cmd_lua::l_lua_subcommand,
        },
    ),
    (
        "ext",
        Subcommand {
            short_doc: b"External command helpers\0",
            long_doc: b"\0",
            func: l_cmd_ext,
        },
    ),
    #[cfg(not(feature = "bash_lt_4_3"))]
    (
        "capture",
        Subcommand {
            short_doc: b"Run a command with stdout captured into a variable\0",
            long_doc: b"\0",
            func: l_capture_subcommand,
        },
    ),
];

const SUBCOMMAND_TABLE: IntLookup128<Subcommand, { SUBCOMMAND_ENTRIES.len() }> =
    intlookup!(&SUBCOMMAND_ENTRIES);

unsafe fn l_builtin_print_help() {
    let mut p = L_BUILTIN_DOC.as_ptr();
    while !(*p).is_null() {
        bprintln!(*p);
        p = p.add(1);
    }
}

unsafe fn l_builtin_print_usage() {
    let cmd_name = this_command_name;
    if !cmd_name.is_null() && *cmd_name != 0 {
        bprintln!(cmd_name, ": usage:");
    }
    let doc_array = L_BUILTIN_DOC.as_ptr();
    let short_doc_ptr = *doc_array.add(2);
    if !short_doc_ptr.is_null() {
        bprintln!(short_doc_ptr);
    }
}
unsafe fn l_builtin_unknown_subcommand(name: &[u8]) {
    let cmd_name = this_command_name;
    if !cmd_name.is_null() && *cmd_name != 0 {
        beprintln!(cmd_name, ": unknown subcommand:", name);
    } else {
        beprintln!("unknown subcommand:", name);
    }
}

#[cfg(not(feature = "bash_lt_4_3"))]
fn build_eval_command<'a>(args: impl Iterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut buf = Vec::new();
    for (i, word) in args.enumerate() {
        if i > 0 {
            buf.push(b' ');
        }
        buf.push(b'\'');
        for &b in word {
            if b == b'\'' {
                buf.extend_from_slice(b"'\\''");
            } else {
                buf.push(b);
            }
        }
        buf.push(b'\'');
    }
    buf.push(b'\0');
    buf
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
#[cfg(not(feature = "bash_lt_4_3"))]
pub unsafe extern "C" fn l_capture_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut args = WordListView::from_raw(list).into_iter();
    // var is a slice from bash WORD_LIST; use the original C string pointer.
    // The WORD_DESC.word field (direct field access on the generated layout)
    // is a NUL-terminated C string.
    let var_ptr = match args.next() {
        None => {
            beprintln!(b"L_builtin capture: usage: L_builtin capture VAR <command> [args...]");
            return EX_USAGE;
        }
        Some(v) => v,
    };
    if args.current().is_none() {
        beprintln!(b"L_builtin capture: missing command");
        return EX_USAGE;
    }
    l_capture_output(var_ptr.as_ptr().cast(), args.as_ptr())
}

#[cfg(not(feature = "bash_lt_4_3"))]
#[no_mangle]
pub unsafe extern "C" fn l_capture_output(var: *const c_char, list: *mut WORD_LIST) -> c_int {
    let args = WordListView::from_raw(list).into_iter();
    let cmd = build_eval_command(args.map(|c| unsafe { c.to_bytes() }));
    assert!(!cmd.is_empty());
    capture_into_variable("L_builtin capture", var, false, || unsafe {
        l_execute_command_string(cmd.as_ptr().cast())
    })
}

/// Top-level L_builtin entry point called by bash via L_builtin_struct.function
#[unsafe(no_mangle)]
pub unsafe extern "C" fn l_entrypoint(list: *mut WORD_LIST) -> c_int {
    // Parse top-level options (-v VAR) before dispatching to subcommand
    let (opts, args) = bash_getopt!(list, l_builtin_print_help, [h], [v]);
    let view = unsafe { WordListView::from_raw(args) };
    let mut list = view.into_iter();
    let first = match list.next() {
        Some(first) => first.to_bytes(),
        None => {
            unsafe { l_builtin_print_usage() };
            return EX_USAGE;
        }
    };
    // Find the subcommand for this name using intlookup's packed table.
    let subcommand = match SUBCOMMAND_TABLE.lookup(first) {
        Some(s) => s,
        None => {
            unsafe { l_builtin_unknown_subcommand(first) };
            return EX_USAGE;
        }
    };
    // Construct the guard before dispatching so current_builtin's doc pointers
    // are restored when l_entrypoint returns.
    let _guard = SubcommandGuard::new();
    // Flush before the handler so buffered bash/C output cannot be reordered
    // against direct fd writes from Rust.
    flush_stdout_buffers();
    let ret = if let Some(var) = opts.v {
        // -v VAR was provided: capture subcommand stdout into VAR
        capture_into_variable("L_builtin", var, true, || unsafe {
            subcommand.call(first, list.as_ptr())
        })
    } else {
        unsafe { subcommand.call(first, list.as_ptr()) }
    };
    flush_stdout_buffers();
    ret
}
