//! L_builtin `signalfd` subcommand: create a signalfd(2) that delivers a set
//! of signals as a readable file descriptor.
//!
//! Usage: `L_builtin signalfd [-n] [-b] [-v FD_VAR] [SIGNAL...]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListIterCpnt, EX_USAGE, WORD_LIST};
use crate::bprintln;
use crate::cmdargs::BashVar;
use crate::l_builtin_error;
use crate::shared::ensure_high_fd;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"signalfd",
    c"[-n] [-b] [-v FD_VAR] [SIGNAL...]",
    c"\
Create a signalfd(2) and store its file descriptor in FD_VAR (or print it if
-v is omitted). The fd becomes readable whenever one of the listed SIGNALs is
pending, so signals can be polled as an fd - see also the `poll` subcommand.

SIGNAL names (SIGTERM, INT, HUP, ...) or numbers are accepted. If none are
given, the fd covers every signal.

Options:
  -n     SFD_NONBLOCK
  -b     Also block (sigprocmask) the listed signals so they are consumed
         by reads from the fd instead of running their default action
  -v     Store the resulting fd in the variable FD_VAR

Exit Status:
Returns success unless signalfd fails or the variable cannot be bound.
",
);

fn parse_signal(s: &str) -> Option<c_int> {
    let base = s.strip_prefix("SIG").unwrap_or(s);
    let up = base.to_ascii_uppercase();
    Some(match up.as_str() {
        "HUP" => libc::SIGHUP,
        "INT" => libc::SIGINT,
        "QUIT" => libc::SIGQUIT,
        "ILL" => libc::SIGILL,
        "TRAP" => libc::SIGTRAP,
        "ABRT" => libc::SIGABRT,
        "BUS" => libc::SIGBUS,
        "FPE" => libc::SIGFPE,
        "KILL" => libc::SIGKILL,
        "USR1" => libc::SIGUSR1,
        "SEGV" => libc::SIGSEGV,
        "USR2" => libc::SIGUSR2,
        "PIPE" => libc::SIGPIPE,
        "ALRM" => libc::SIGALRM,
        "TERM" => libc::SIGTERM,
        "CHLD" => libc::SIGCHLD,
        "CONT" => libc::SIGCONT,
        "STOP" => libc::SIGSTOP,
        "TSTP" => libc::SIGTSTP,
        "TTIN" => libc::SIGTTIN,
        "TTOU" => libc::SIGTTOU,
        "URG" => libc::SIGURG,
        "XCPU" => libc::SIGXCPU,
        "XFSZ" => libc::SIGXFSZ,
        "VTALRM" => libc::SIGVTALRM,
        "PROF" => libc::SIGPROF,
        "WINCH" => libc::SIGWINCH,
        "IO" => libc::SIGIO,
        "SYS" => libc::SIGSYS,
        _ => return s.parse::<c_int>().ok(),
    })
}

#[derive(CmdArgs)]
struct SignalfdArgs {
    #[flag('n')]
    nonblock: bool,
    #[flag('b')]
    block: bool,
    #[opt('v')]
    fd_var: Option<BashVar>,
    #[rest]
    signals: WordListIterCpnt<'static>,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn signalfd_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = SignalfdArgs::parse(list)?;

    // Build the signal set (all signals if none listed).
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe { libc::sigemptyset(&mut set) };
    if args.signals.as_ptr().is_null() {
        unsafe { libc::sigfillset(&mut set) };
    } else {
        for sig in args.signals {
            let name = match unsafe { sig.as_str() } {
                Ok(s) => s,
                Err(_) => {
                    l_builtin_error!(b"invalid signal encoding");
                    return Err(EX_USAGE);
                }
            };
            match parse_signal(name) {
                Some(n) => {
                    unsafe { libc::sigaddset(&mut set, n) };
                }
                None => {
                    l_builtin_error!(b"unknown signal: ", name);
                    return Err(EX_USAGE);
                }
            }
        }
    }

    // Optionally block the signals so reads from the fd consume them.
    if args.block {
        if unsafe { libc::sigprocmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) } < 0 {
            return Err(l_builtin_error!(
                b"sigprocmask: ",
                std::io::Error::last_os_error()
            ));
        }
    }

    let mut flags = libc::SFD_CLOEXEC;
    if args.nonblock {
        flags |= libc::SFD_NONBLOCK;
    }

    let fd = unsafe { libc::signalfd(-1, &set, flags) };
    if fd < 0 {
        return Err(l_builtin_error!(
            b"signalfd: ",
            std::io::Error::last_os_error()
        ));
    }
    let fd = ensure_high_fd(fd).map_err(|e| l_builtin_error!(b"signalfd: fd dup failed: ", e))?;
    match args.fd_var {
        Some(v) => {
            if let Err(e) = v.set_int(fd as i64) {
                unsafe { libc::close(fd) };
                return Err(e);
            }
        }
        None => bprintln!(fd as i64),
    }
    Ok(())
}
