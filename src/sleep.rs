//! L_builtin `sleep` subcommand: high-precision sub-second sleep.
//!
//! Usage: `L_builtin sleep [-i] SECONDS`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListView, EX_USAGE, EXECUTION_SUCCESS, EXECUTION_FAILURE, WORD_LIST};
use crate::{bash_getopt, beprintln};
use std::os::raw::c_int;

const ENAME: &str = "L_builtin sleep";

fn print_sleep_help() {
    let doc = b"\
L_builtin sleep [-i] SECONDS

Sleep for the specified number of SECONDS. SECONDS can be a floating-point
number to request sub-second/microsecond-level precision.

If -i is provided, the sleep will not automatically retry on signal interruption
(EINTR). Instead, it will fail with an error. By default, sleep retries on EINTR.

Exit Status:
Returns success unless sleep fails.
";
    beprintln!(doc);
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn sleep_subcommand(list: *mut WORD_LIST) -> c_int {
    let (opts, args) = bash_getopt!(list, print_sleep_help, [i], []);

    let view = unsafe { WordListView::from_raw(args) };
    let mut iter = view.iter();

    let interruptible = opts.i;

    // Get seconds
    let seconds = match iter.next() {
        Some(sec_cptr) => {
            let sec_bytes = unsafe { sec_cptr.to_bytes() };
            match std::str::from_utf8(sec_bytes) {
                Ok(s) => match s.parse::<f64>() {
                    Ok(sec) => sec,
                    Err(_) => {
                        beprintln!(ENAME, b": invalid sleep duration: ", sec_bytes);
                        return EX_USAGE;
                    }
                },
                Err(_) => {
                    beprintln!(ENAME, b": invalid sleep duration encoding");
                    return EX_USAGE;
                }
            }
        }
        None => {
            beprintln!(ENAME, b": missing SECONDS argument");
            return EX_USAGE;
        }
    };

    if seconds < 0.0 {
        beprintln!(ENAME, b": invalid sleep duration");
        return EX_USAGE;
    }

    let mut ts = libc::timespec {
        tv_sec: seconds as libc::time_t,
        tv_nsec: ((seconds.fract() * 1e9) as libc::c_long),
    };

    loop {
        let mut rem = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        let result = unsafe { libc::nanosleep(&ts, &mut rem) };
        if result == 0 {
            break;
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EINTR) {
            if interruptible {
                beprintln!(ENAME, b": sleep failed: Interrupted system call");
                return EXECUTION_FAILURE;
            }
            // Interrupted by signal, continue sleeping with remaining time
            ts = rem;
            continue;
        }
        beprintln!(ENAME, b": sleep failed: ", err);
        return EXECUTION_FAILURE;
    }

    EXECUTION_SUCCESS
}