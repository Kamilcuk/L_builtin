//! L_builtin `sleep` subcommand: high-precision sub-second sleep.
//!
//! Usage: `L_builtin sleep [-i] SECONDS`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EX_USAGE, WORD_LIST};
use crate::cmdargs::Duration;
use crate::l_builtin_error;
use crate::l_builtin_usage_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;

const CMD: CmdDesc = CmdDesc::new(
    c"sleep",
    c"[-i] SECONDS",
    c"\
Sleep for the specified number of SECONDS. SECONDS can be a duration string
(e.g. `1s`, `500ms`, `1h30m`) or a floating-point number to request
sub-second/microsecond-level precision.

If -i is provided, the sleep will not automatically retry on signal interruption
(EINTR). Instead, it will fail with an error. By default, sleep retries on EINTR.

Exit Status:
  Returns success unless sleep fails.
",
);

#[derive(CmdArgs)]
struct SleepArgs {
    #[flag('i')]
    interruptible: bool,
    #[positional]
    seconds: Duration,
}

pub unsafe fn sleep_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = SleepArgs::parse(list)?;
    let seconds = args.seconds.as_secs_f64();

    if seconds < 0.0 {
        return Err(l_builtin_usage_error!(b"invalid sleep duration"));
    }

    let mut ts = args.seconds.as_timespec();

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
            if args.interruptible {
                return Err(l_builtin_error!(b"sleep failed: Interrupted system call"));
            }
            // Interrupted by signal, continue sleeping with remaining time
            ts = rem;
            continue;
        }
        return Err(l_builtin_error!(b"sleep failed: ", err));
    }

    Ok(())
}
