//! Top-level L_builtin dispatch (Rust entry point called by bash)
//!
//! Bash calls `l_entrypoint` directly. This function reads the
//! subcommand name (first word), prints help on `-h`/`--help`, looks the
//! subcommand up in a dispatch table, and calls the appropriate handler with
//! the word list advanced past the subcommand name.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use cmdargs_derive::CmdArgs;

use crate::bash_api::{c_char, c_int, this_cmd_name, WordListView, EX_USAGE, WORD_LIST};
use crate::cmdargs::WordListIterCpnt;
use crate::shared::{capture_into_variable, flush_stdout_buffers};
#[cfg(not(feature = "bash_lt_4_3"))]
use crate::subcmd::cint_to_cmd_result;
use crate::subcmd::{cmd_result_to_cint, CmdResult, SubcommandFn, SubcommandGuard};
use crate::{bprintln, intlookup, l_builtin_usage_error};

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

macro_rules! c_wrap {
    ($f:expr) => {
        |list| {
            let ret = unsafe { $f(list) };
            if ret == 0 {
                ::core::result::Result::Ok(())
            } else {
                ::core::result::Result::Err(ret)
            }
        }
    };
}

// Dispatch table: a plain map of subcommand name -> extern "C" handler.
const SUBCOMMAND_ENTRIES: &[(&str, SubcommandFn)] = &[
    ("lseek", crate::lseek::lseek_subcommand),
    ("poll", c_wrap!(poll_subcommand)),
    #[cfg(feature = "ppoll")]
    ("ppoll", c_wrap!(ppoll_subcommand)),
    ("sigmask", c_wrap!(sigmask_subcommand)),
    ("sigunmask", c_wrap!(sigunmask_subcommand)),
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
    ("ext", c_wrap!(l_cmd_ext)),
    ("eventfd", crate::eventfd::eventfd_subcommand),
    ("memfd", crate::memfd::memfd_subcommand),
    ("timerfd", crate::timerfd::timerfd_subcommand),
    ("signalfd", crate::signalfd::signalfd_subcommand),
    ("flock", crate::flock::flock_subcommand),
    ("close", crate::close::close_subcommand),
    ("splice", crate::splice::splice_subcommand),
    ("shm", crate::cmd_shm::shm_subcommand),
    ("fcntl", crate::cmd_fcntl::fcntl_subcommand),
    ("barrier", crate::cmd_barrier::barrier_subcommand),
    ("mutex", crate::cmd_mutex::mutex_subcommand),
    ("semaphore", crate::cmd_semaphore::semaphore_subcommand),
    #[cfg(not(feature = "bash_lt_4_3"))]
    ("capture", l_capture_subcommand),
    #[cfg(feature = "dev")]
    ("unittest", crate::unittest::l_unittest_subcommand),
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

const SUBCOMMAND_TABLE: crate::intlookup::U128::IntLookup<
    SubcommandFn,
    { SUBCOMMAND_ENTRIES.len() },
> = intlookup!(&SUBCOMMAND_ENTRIES);

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
        buf.reserve(word.len() + 2);
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

#[derive(CmdArgs)]
struct CaptureArgs {
    #[positional]
    var: BashVar,
    #[positional]
    command: &'static [u8],
    #[rest]
    args: WordListIterCpnt<'static>,
}

/// # Safety
#[cfg(not(feature = "bash_lt_4_3"))]
pub unsafe fn l_capture_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CAPTURE_CMD.enter();
    let args = CaptureArgs::parse(list)?;
    let cmd = build_eval_command(
        std::iter::once(args.command).chain(args.args.map(|c| unsafe { c.as_bytes() })),
    );
    assert!(!cmd.is_empty());
    capture_into_variable("L_builtin capture", args.var, false, || {
        cint_to_cmd_result(l_execute_command_string(cmd.as_ptr().cast()))
    })
}

#[derive(CmdArgs)]
struct EntrypointArgs {
    #[opt('v')]
    var: Option<BashVar>,
    #[rest]
    rest: WordListIterCpnt<'static>,
}

/// Top-level L_builtin entry point called by bash via L_builtin_struct.function
#[unsafe(no_mangle)]
pub unsafe extern "C" fn l_entrypoint(list: *mut WORD_LIST) -> c_int {
    flush_stdout_buffers();
    let ret = cmd_result_to_cint(entrypoint(list));
    flush_stdout_buffers();
    ret
}

pub unsafe fn entrypoint(list: *mut WORD_LIST) -> CmdResult {
    let args = EntrypointArgs::parse(list)?;
    let mut list = args.rest;
    let first_word = match list.next() {
        Some(first_word) => first_word,
        None => return Err(l_builtin_usage_error!("missing subcommand")),
    };
    let first = unsafe { first_word.as_bytes() };
    // Find the subcommand for this name using intlookup's packed table.
    let subcommand = match SUBCOMMAND_TABLE.lookup(first) {
        Some(f) => f,
        None => return Err(l_builtin_usage_error!("unknown subcommand: ", first_word)),
    };
    // Construct the guard before dispatching so current_builtin's doc pointers
    // (set by the subcommand's CmdDesc::enter) are restored when l_entrypoint
    // returns.
    let _guard = SubcommandGuard::new();
    // Flush before the handler so buffered bash/C output cannot be reordered
    // against direct fd writes from Rust.
    if let Some(ret) = args.var {
        // -v VAR was provided: capture subcommand stdout into VAR
        capture_into_variable("L_builtin", ret, true, || unsafe {
            subcommand(list.as_ptr())
        })
    } else {
        unsafe { subcommand(list.as_ptr()) }
    }
}
