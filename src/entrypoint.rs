//! Top-level L_builtin dispatch (Rust entry point called by bash)
//!
//! Bash calls `l_entrypoint` directly. This function reads the
//! subcommand name (first word), prints help on `-h`/`--help`, looks the
//! subcommand up in a dispatch table, and calls the appropriate handler with
//! the word list advanced past the subcommand name.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{
    builtin_error, c_char, c_int, this_cmd_name, WordListView, EX_USAGE, WORD_LIST,
};
use crate::intlookup::IntLookup128;
use crate::shared::{capture_into_variable, flush_stdout_buffers};
use crate::subcmd::{CmdDesc, SubcommandFn, SubcommandGuard};
use crate::{beprintln, bprintln, getopts, intlookup, variadic};

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

// C subcommands have no Rust doc constants, so give each a CmdDesc wrapper that
// enters the subcommand context (name + docs) before delegating to the C hander.
const POLL_CMD: CmdDesc = CmdDesc::new(
    c"poll",
    c"[-t TIMEOUT] [-v ARRAY_VAR] [-i] [FD[:EVENTS] ...]",
    c"\
Wait for file descriptors to become ready using poll(2). EVENTS can be 'r',
'w', or 'p'. Results are stored in the indexed array ARRAY_VAR as
FD:REVENTS ('r', 'w', 'p', 'h', 'e', or 'n').

If -i is provided, poll will not automatically retry on signal interruption
(EINTR); by default it retries.

Exit Status:
Returns success if poll succeeds, even on timeout; failure on system errors.
",
);
unsafe extern "C" fn poll_enter(list: *mut WORD_LIST) -> c_int {
    POLL_CMD.enter();
    unsafe { poll_subcommand(list) }
}

#[cfg(feature = "ppoll")]
const PPOLL_CMD: CmdDesc = CmdDesc::new(
    c"ppoll",
    c"[-t TIMEOUT] [-v ARRAY_VAR] [-u SIGSPEC] [-i] [FD[:EVENTS] ...]",
    c"\
Wait for file descriptors and unblock signals atomically using ppoll(2).
Results are stored in the indexed array ARRAY_VAR as FD:REVENTS.

Use -u SIGSPEC to temporarily unblock specified signals during ppoll; use
-u 'ALL' (case-insensitive) to unblock all signals.

If -i is provided, ppoll will not automatically retry on EINTR.
",
);
#[cfg(feature = "ppoll")]
unsafe extern "C" fn ppoll_enter(list: *mut WORD_LIST) -> c_int {
    PPOLL_CMD.enter();
    unsafe { ppoll_subcommand(list) }
}

const SIGMASK_CMD: CmdDesc = CmdDesc::new(
    c"sigmask",
    c"[-s sigspec] [-u sigspec] [sigspec ...]",
    c"\
Block or unblock signals in the shell process. Without options, prints the
current signal mask. -s blocks, -u unblocks. Use 'ALL' (case-insensitive)
with -s or -u to block or unblock all signals. Positional sigspecs are
always blocked.

Exit Status:
Returns success unless an invalid signal is given or a system error occurs.
",
);
unsafe extern "C" fn sigmask_enter(list: *mut WORD_LIST) -> c_int {
    SIGMASK_CMD.enter();
    unsafe { sigmask_subcommand(list) }
}

const SIGUNMASK_CMD: CmdDesc = CmdDesc::new(
    c"sigunmask",
    c"-s sigspec cmd [args...]",
    c"\
Temporarily unblock the specified signal and execute the command. Use
'ALL' (case-insensitive) with -s to unblock all signals. If the signal was
pending, the trap runs and the command is skipped. The command can be any
shell command (builtin, function, or external).

Exit Status:
Returns the command's status, or 128+signum if a signal was caught.
",
);
unsafe extern "C" fn sigunmask_enter(list: *mut WORD_LIST) -> c_int {
    SIGUNMASK_CMD.enter();
    unsafe { sigunmask_subcommand(list) }
}

// Dispatch table: a plain map of subcommand name -> extern "C" handler.
const SUBCOMMAND_ENTRIES: &[(&str, SubcommandFn)] = &[
    ("lseek", crate::lseek::lseek_subcommand),
    ("poll", poll_enter),
    #[cfg(feature = "ppoll")]
    ("ppoll", ppoll_enter),
    ("sigmask", sigmask_enter),
    ("sigunmask", sigunmask_enter),
    ("pipe", crate::pipe::pipe_subcommand),
    ("listen", crate::listen::listen_subcommand),
    ("accept", crate::accept::accept_subcommand),
    ("connect", crate::connect::connect_subcommand),
    ("shutdown", crate::shutdown::shutdown_subcommand),
    ("send", crate::send::send_subcommand),
    ("recv", crate::recv::recv_subcommand),
    ("sleep", crate::sleep::sleep_subcommand),
    ("core", crate::cmd_core::l_core_subcommand),
    ("lua", crate::cmd_lua::l_lua_subcommand),
    ("ext", l_cmd_ext),
    ("eventfd", crate::eventfd::eventfd_subcommand),
    ("memfd", crate::memfd::memfd_subcommand),
    ("timerfd", crate::timerfd::timerfd_subcommand),
    ("signalfd", crate::signalfd::signalfd_subcommand),
    ("splice", crate::splice::splice_subcommand),
    #[cfg(not(feature = "bash_lt_4_3"))]
    ("capture", l_capture_subcommand),
];

const fn extract_first<const N: usize>(a: &[(&'static str, SubcommandFn)]) -> [&'static str; N] {
    let mut names = [""; N];
    let mut i = 0;
    while i < N {
        names[i] = a[i].0;
        i += 1;
    }
    names
}

/// Extract just the subcommand names for usage printing.
const SUBCOMMAND_NAMES: &[&str] =
    &extract_first::<{ SUBCOMMAND_ENTRIES.len() }>(SUBCOMMAND_ENTRIES);

const SUBCOMMAND_TABLE: IntLookup128<SubcommandFn, { SUBCOMMAND_ENTRIES.len() }> =
    intlookup!(&SUBCOMMAND_ENTRIES);

fn l_builtin_print_usage() {
    let cmd_name = this_cmd_name();
    // Print usage line
    bprintln!(
        cmd_name,
        b": usage: ",
        cmd_name,
        " [-v VAR] <subcommand> [options] [args]"
    );
    bprintln!(b"");
    bprintln!(b"Available subcommands:");
    // Print each subcommand name
    for name in SUBCOMMAND_NAMES {
        bprintln!(b"  ", name);
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
const CAPTURE_CMD: crate::subcmd::CmdDesc = crate::subcmd::CmdDesc::new(
    c"capture",
    c"VAR <command> [args...]",
    c"\
Run <command> with its stdout captured into the shell variable VAR
(trailing newlines stripped, like $(...)). The command runs through the
shell, so external commands, functions, builtins and L_builtin subcommands
all work uniformly.
",
);

/// # Safety
#[cfg(not(feature = "bash_lt_4_3"))]
pub unsafe extern "C" fn l_capture_subcommand(list: *mut WORD_LIST) -> c_int {
    CAPTURE_CMD.enter();
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
    let mut var: *mut c_char = std::ptr::null_mut();
    let args = getopts!(
        list,
        [],
        [ v => |v| var = v.as_ptr().cast() ]
    );
    let view = unsafe { WordListView::from_raw(args) };
    let mut list = view.into_iter();
    let first_word = match list.next() {
        Some(first_word) => first_word,
        None => {
            variadic!(builtin_error, c"missing subcommand");
            l_builtin_print_usage();
            return EX_USAGE;
        }
    };
    let first = unsafe { first_word.to_bytes() };
    // Find the subcommand for this name using intlookup's packed table.
    let subcommand = match SUBCOMMAND_TABLE.lookup(first) {
        Some(f) => f,
        None => {
            // beprintln!(this_cmd_name(), b": unknown subcommand:", first);
            variadic!(
                builtin_error,
                c"unknown subcommand: %s",
                first_word.as_ptr(),
            );
            l_builtin_print_usage();
            return EX_USAGE;
        }
    };
    // Construct the guard before dispatching so current_builtin's doc pointers
    // (set by the subcommand's CmdDesc::enter) are restored when l_entrypoint
    // returns.
    let _guard = SubcommandGuard::new();
    // Flush before the handler so buffered bash/C output cannot be reordered
    // against direct fd writes from Rust.
    flush_stdout_buffers();
    let ret = if !var.is_null() {
        // -v VAR was provided: capture subcommand stdout into VAR
        capture_into_variable("L_builtin", var, true, || unsafe {
            subcommand(list.as_ptr())
        })
    } else {
        unsafe { subcommand(list.as_ptr()) }
    };
    flush_stdout_buffers();
    ret
}
