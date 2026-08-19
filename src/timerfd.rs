//! L_builtin `timerfd` subcommand group: create and arm a timerfd.
//!
//! Usage:
//!   `L_builtin timerfd create [-c CLOCK] [-s SEC] [-i SEC] [-n] [-v FD_VAR]`
//!   `L_builtin timerfd set FD [-c CLOCK] [-s SEC] [-i SEC]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CStr;
use std::os::raw::c_int;

use cmdargs_derive::CmdArgs;

use crate::bash_api::{EXECUTION_FAILURE, EX_USAGE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::l_builtin_error;
use crate::shared::ensure_high_fd;
use crate::subcmd::{CmdDesc, CmdResult, SubCommandCallerArgs, SubcommandFn};

const TIMERFD_CMD: CmdDesc = CmdDesc::new(
    c"timerfd",
    c"create [-c CLOCK] [-s SEC] [-i SEC] [-n] FD_VAR | set FD [-s SEC] [-i SEC] [-c CLOCK]",
    c"\
Create a timerfd(2) and arm it, or modify an existing timerfd's settings.

Subcommands:
  create [-c CLOCK] [-s SEC] [-i SEC] [-n] FD_VAR
                        Create a timerfd(2) and store its file descriptor in
                        the shell variable FD_VAR. The fd becomes readable when
                        the timer expires, so it can be polled together with
                        other fds - see also the `poll`/`ppoll` subcommands.
                        -c     CLOCK (CLOCK_REALTIME or CLOCK_MONOTONIC;
                               default CLOCK_MONOTONIC)
                        -s     Initial expiry in (possibly fractional) seconds;
                               default 0 = do not arm
                        -i     Periodic interval in (possibly fractional) seconds;
                               default 0
                        -n     TFD_NONBLOCK

  set FD [-s SEC] [-i SEC] [-c CLOCK]
                        Read the current timer settings with timerfd_gettime,
                        change -s (initial expiry) and/or -i (interval) as
                        given, then re-arm with timerfd_settime. At least one
                        of -s/-i is required. CLOCK is accepted for
                        compatibility but must match the fd's clock.

Exit Status:
  Returns success unless timerfd_create/timerfd_settime fails or the variable
  cannot be bound.
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

/// `L_builtin timerfd create [-c CLOCK] [-s SEC] [-i SEC] [-n] FD_VAR`
#[derive(CmdArgs)]
struct TimerfdCreateArgs {
    #[flag('n')]
    nonblock: bool,
    #[opt('c')]
    clock: Option<*const c_char>,
    #[opt('s')]
    initial: Option<f64>,
    #[opt('i')]
    interval: Option<f64>,
    /// Shell variable receiving the file descriptor.
    #[positional]
    fd_var: BashVar,
}

pub unsafe fn timerfd_create_subcommand(list: *mut WORD_LIST) -> CmdResult {
    TIMERFD_CREATE_CMD.enter();
    let args = TimerfdCreateArgs::parse(list)?;

    let mut clock: libc::clockid_t = libc::CLOCK_MONOTONIC;
    if let Some(p) = args.clock {
        match parse_clock(unsafe { CStr::from_ptr(p).to_str().ok() }) {
            Some(c) => clock = c,
            None => {
                l_builtin_error!(b"invalid clock");
                return Err(EXECUTION_FAILURE);
            }
        }
    }
    let initial: f64 = args.initial.unwrap_or(0.0);
    let interval: f64 = args.interval.unwrap_or(0.0);

    let mut flags = libc::TFD_CLOEXEC;
    if args.nonblock {
        flags |= libc::TFD_NONBLOCK;
    }

    let fd = unsafe { libc::timerfd_create(clock, flags) };
    if fd < 0 {
        l_builtin_error!(b"timerfd_create: ", std::io::Error::last_os_error());
        return Err(EXECUTION_FAILURE);
    }

    let fd_int = ensure_high_fd(fd).map_err(|e| {
        l_builtin_error!(b"timerfd_create: fd dup failed: ", e);
        EXECUTION_FAILURE
    })?;

    if initial > 0.0 || interval > 0.0 {
        let spec = libc::itimerspec {
            it_interval: to_timespec(interval),
            it_value: to_timespec(initial),
        };
        if unsafe { libc::timerfd_settime(fd_int, 0, &spec, std::ptr::null_mut()) } < 0 {
            l_builtin_error!(b"timerfd_settime: ", std::io::Error::last_os_error());
            unsafe { libc::close(fd_int) };
            return Err(EXECUTION_FAILURE);
        }
    }

    args.fd_var.set_int(fd_int as i64)?;
    Ok(())
}

/// `L_builtin timerfd set FD [-s SEC] [-i SEC] [-c CLOCK]`
#[derive(CmdArgs)]
struct TimerfdSetArgs {
    /// File descriptor of the timerfd.
    #[positional]
    fd: c_int,
    #[opt('s')]
    initial: Option<f64>,
    #[opt('i')]
    interval: Option<f64>,
    #[opt('c')]
    clock: Option<*const c_char>,
}

pub unsafe fn timerfd_set_subcommand(list: *mut WORD_LIST) -> CmdResult {
    TIMERFD_SET_CMD.enter();
    let args = TimerfdSetArgs::parse(list)?;

    if args.initial.is_none() && args.interval.is_none() {
        l_builtin_error!(b"at least one of -s or -i is required");
        return Err(EX_USAGE);
    }

    let mut cur: libc::itimerspec = unsafe { std::mem::zeroed() };
    if unsafe { libc::timerfd_gettime(args.fd, &mut cur) } < 0 {
        l_builtin_error!(b"timerfd_gettime: ", std::io::Error::last_os_error());
        return Err(EXECUTION_FAILURE);
    }

    let new_spec = libc::itimerspec {
        it_interval: args.interval.map(to_timespec).unwrap_or(cur.it_interval),
        it_value: args.initial.map(to_timespec).unwrap_or(cur.it_value),
    };

    if unsafe { libc::timerfd_settime(args.fd, 0, &new_spec, std::ptr::null_mut()) } < 0 {
        l_builtin_error!(b"timerfd_settime: ", std::io::Error::last_os_error());
        return Err(EXECUTION_FAILURE);
    }

    Ok(())
}

const TIMERFD_CREATE_CMD: CmdDesc = CmdDesc::new(
    c"create",
    c"create [-c CLOCK] [-s SEC] [-i SEC] [-n] [-v FD_VAR]",
    c"\
Create a timerfd(2) and store its file descriptor in FD_VAR (or print it if
-v is omitted). The fd becomes readable when the timer expires, so it can be
polled together with other fds - see also the `poll`/`ppoll` subcommands.

Options:
  -c     CLOCK (CLOCK_REALTIME or CLOCK_MONOTONIC; default CLOCK_MONOTONIC)
  -s     Initial expiry in (possibly fractional) seconds; default 0 = do not arm
  -i     Periodic interval in (possibly fractional) seconds; default 0
  -n     TFD_NONBLOCK

Examples:
   L_builtin timerfd create -n -s 0.5 tf
   L_builtin timerfd create -s 1.0 -i 0.25 tf
",
);

const TIMERFD_SET_CMD: CmdDesc = CmdDesc::new(
    c"set",
    c"set FD [-s SEC] [-i SEC] [-c CLOCK]",
    c"\
Read the current timer settings on FD with timerfd_gettime, change -s
(initial expiry) and/or -i (periodic interval) as given, then re-arm with
timerfd_settime. At least one of -s/-i is required. CLOCK is accepted for
compatibility but must match the fd's clock.

Examples:
  L_builtin timerfd set \"$tf\" -s 0.1 -i 0.1
  L_builtin timerfd set \"$tf\" -s 0    # disarm
",
);

const TIMERFD_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("create", timerfd_create_subcommand),
    ("set", timerfd_set_subcommand),
];

const TIMERFD_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 2> =
    crate::intlookup!(&TIMERFD_SUBCOMMANDS);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn timerfd_subcommand(list: *mut WORD_LIST) -> CmdResult {
    TIMERFD_CMD.enter();
    let args = SubCommandCallerArgs::parse(list)?;
    let caller = args.handler(TIMERFD_TABLE)?;
    caller.call()
}
