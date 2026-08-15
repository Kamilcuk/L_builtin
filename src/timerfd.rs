//! L_builtin `timerfd` subcommand: create and arm a timerfd.
//!
//! Usage: `L_builtin timerfd [-c CLOCK] [-s SEC] [-i SEC] [-n] [-v FD_VAR]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, bprintln, getopts, parse_positionals};
use std::os::raw::{c_char, c_int};

const ENAME: &str = "L_builtin timerfd";

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

unsafe fn store_fd(var: *mut c_char, fd: c_int) -> bool {
    if var.is_null() {
        bprintln!(fd as i64);
        return true;
    }
    let s = crate::shared::I64Str::new(fd as i64);
    if unsafe { crate::bash_api::bind_variable(var, s.as_ptr(), 0) }.is_null() {
        beprintln!(ENAME, b": cannot bind variable");
        return false;
    }
    true
}

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
#[no_mangle]
pub unsafe extern "C" fn timerfd_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let mut nonblock = false;
    let mut clock: libc::clockid_t = libc::CLOCK_MONOTONIC;
    let mut initial: f64 = 0.0;
    let mut interval: f64 = 0.0;
    let mut fd_var: *mut c_char = std::ptr::null_mut();
    let rest = getopts!(
        list,
        [ n => || nonblock = true ],
        [ c => |v: crate::bash_api::Cpnt<'_>| {
            if let Some(c) = parse_clock(unsafe { v.as_str() }.ok()) {
                clock = c;
            } else {
                beprintln!(ENAME, b": invalid clock");
            }
        },
          s => |v: crate::bash_api::Cpnt<'_>| initial = unsafe {
                v.as_str().ok().and_then(|s| s.parse().ok()).unwrap_or(0.0)
            },
          i => |v: crate::bash_api::Cpnt<'_>| interval = unsafe {
                v.as_str().ok().and_then(|s| s.parse().ok()).unwrap_or(0.0)
            },
          v => |v: crate::bash_api::Cpnt<'_>| fd_var = v.as_ptr().cast() ]
    );
    let _ = parse_positionals!(rest, []);

    let mut flags = libc::TFD_CLOEXEC;
    if nonblock {
        flags |= libc::TFD_NONBLOCK;
    }

    let fd = unsafe { libc::timerfd_create(clock, flags) };
    if fd < 0 {
        beprintln!(
            ENAME,
            b": timerfd_create: ",
            std::io::Error::last_os_error()
        );
        return EXECUTION_FAILURE;
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
            beprintln!(
                ENAME,
                b": timerfd_settime: ",
                std::io::Error::last_os_error()
            );
            unsafe { libc::close(fd) };
            return EXECUTION_FAILURE;
        }
    }

    if !unsafe { store_fd(fd_var, fd) } {
        unsafe { libc::close(fd) };
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}
