//! L_builtin `sleep` subcommand: high-precision sub-second sleep.
//!
//! Usage: `L_builtin sleep [-i] SECONDS`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{this_cmd_name, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, subcmd_getopts};
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"sleep",
    c"[-i] SECONDS",
    c"\
Sleep for the specified number of SECONDS. SECONDS can be a floating-point
number to request sub-second/microsecond-level precision.

If -i is provided, the sleep will not automatically retry on signal interruption
(EINTR). Instead, it will fail with an error. By default, sleep retries on EINTR.

Exit Status:
Returns success unless sleep fails.
",
);

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn sleep_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut interruptible = false;
    let (seconds_cstr,) = subcmd_getopts!(
        CMD,
        list,
        flags: [ i => || interruptible = true ],
        required: [SECONDS],
    );
    let seconds_str = match seconds_cstr.as_str() {
        Ok(s) => s,
        Err(_) => {
            beprintln!("L_builtin: invalid UTF-8 argument");
            return EX_USAGE;
        }
    };
    let seconds: f64 = match seconds_str.parse() {
        Ok(f) => f,
        Err(_) => {
            beprintln!("L_builtin: invalid number: {}", seconds_str);
            return EX_USAGE;
        }
    };

    if seconds < 0.0 {
        beprintln!(this_cmd_name(), b": invalid sleep duration");
        return EX_USAGE;
    }

    let mut ts = libc::timespec {
        tv_sec: seconds as libc::time_t,
        tv_nsec: ((seconds.fract() * 1e9) as libc::c_long),
    };

    loop {
        let mut rem = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let result = unsafe { libc::nanosleep(&ts, &mut rem) };
        if result == 0 {
            break;
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EINTR) {
            if interruptible {
                beprintln!(this_cmd_name(), b": sleep failed: Interrupted system call");
                return EXECUTION_FAILURE;
            }
            // Interrupted by signal, continue sleeping with remaining time
            ts = rem;
            continue;
        }
        beprintln!(this_cmd_name(), b": sleep failed: ", err);
        return EXECUTION_FAILURE;
    }

    EXECUTION_SUCCESS
}
