//! L_builtin `timerfd` subcommand: create and arm a timerfd.
//!
//! Usage: `L_builtin timerfd [-c CLOCK] [-s SEC] [-i SEC] [-n] [-v FD_VAR]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, WORD_LIST};
use crate::bprintln;
use crate::cmdargs::BashVar;
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

const CMD: CmdDesc = CmdDesc::new(
    c"timerfd",
    c"[-c CLOCK] [-s SEC] [-i SEC] [-n] [-v FD_VAR]",
    c"\
Create a timerfd(2) and store its file descriptor in FD_VAR (or print it if
-v is omitted). The fd becomes readable when the timer expires, so it can be
polled together with other fds - see also the `poll`/`ppoll` subcommands.

Options:
  -c     CLOCK (CLOCK_REALTIME or CLOCK_MONOTONIC; default CLOCK_MONOTONIC)
  -s     Initial expiry in (possibly fractional) seconds; default 0 = do not arm
  -i     Periodic interval in (possibly fractional) seconds; default 0
  -n     TFD_NONBLOCK
  -v     Store the resulting fd in the variable FD_VAR

Exit Status:
Returns success unless timerfd_create fails or the variable cannot be bound.
",
);

fn parse_clock(s: Option<&str>) -> Option<libc::clockid_t> {
    let c = match s.map(str::to_ascii_uppercase).as_deref() {
        None | Some("MONOTONIC" | "CLOCK_MONOTONIC") => libc::CLOCK_MONOTONIC,
        Some("REALTIME" | "CLOCK_REALTIME") => libc::CLOCK_REALTIME,
        _ => return None,
    };
    Some(c)
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[derive(CmdArgs)]
struct TimerfdArgs {
    #[flag('n')]
    nonblock: bool,
    #[opt('c')]
    clock: Option<*const c_char>,
    #[opt('s')]
    initial: Option<f64>,
    #[opt('i')]
    interval: Option<f64>,
    #[opt('v')]
    fd_var: Option<BashVar>,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn timerfd_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();

    let args = TimerfdArgs::parse(list)?;

    let nonblock = args.nonblock;
    let mut clock: libc::clockid_t = libc::CLOCK_MONOTONIC;
    if let Some(p) = args.clock {
        match parse_clock(unsafe { CStr::from_ptr(p).to_str().ok() }) {
            Some(c) => clock = c,
            None => {
                l_builtin_error!(b"invalid clock");
            }
        }
    }
    let initial: f64 = args.initial.unwrap_or(0.0);
    let interval: f64 = args.interval.unwrap_or(0.0);
    let fd_var = args.fd_var;

    let mut flags = libc::TFD_CLOEXEC;
    if nonblock {
        flags |= libc::TFD_NONBLOCK;
    }

    let fd = unsafe { libc::timerfd_create(clock, flags) };
    if fd < 0 {
        l_builtin_error!(b"timerfd_create: ", std::io::Error::last_os_error());
        return Err(EXECUTION_FAILURE);
    }

    fn to_timespec(secs: f64) -> libc::timespec {
        let (s, n) = if secs > 0.0 {
            (
                secs.trunc() as libc::time_t,
                ((secs.fract()) * 1e9) as libc::c_long,
            )
        } else {
            (0, 0)
        };
        libc::timespec {
            tv_sec: s,
            tv_nsec: n,
        }
    }

    if initial > 0.0 || interval > 0.0 {
        let spec = libc::itimerspec {
            it_interval: to_timespec(interval),
            it_value: to_timespec(initial),
        };
        if unsafe { libc::timerfd_settime(fd, 0, &spec, std::ptr::null_mut()) } < 0 {
            l_builtin_error!(b"timerfd_settime: ", std::io::Error::last_os_error());
            unsafe { libc::close(fd) };
            return Err(EXECUTION_FAILURE);
        }
    }

    if let Some(var) = &fd_var {
        var.set_int(fd as i64)?;
    } else {
        bprintln!(fd as i64);
    }
    Ok(())
}
