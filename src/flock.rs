//! L_builtin `flock` subcommand: apply flock(2) to an existing file descriptor.
//!
//! Usage: `L_builtin flock [-x|-e] [-s] [-u] [-n] FD`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::CmdDesc;
use crate::subcmd_getopts;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"flock",
    c"[-x|-e] [-s] [-u] [-n] FD",
    c"\
Apply flock(2) to an existing file descriptor FD (fd-only: the fd must
already be open, e.g. a memfd created with `L_builtin memfd VAR`).

Options:
  -x, -e   LOCK_EX (exclusive lock)
  -s       LOCK_SH (shared lock)
  -u       LOCK_UN (unlock)
  -n       LOCK_NB: non-blocking; fail immediately instead of waiting

Exactly one of -x/-e, -s, or -u selects the operation (default -x when
none is given). -n may be combined with -x/-s/-e.

Exit Status:
  Returns success unless the fd is invalid, the operation is unknown, or
  flock(2) fails (a non-blocking lock that would block returns failure).

Examples:
  // Exclusive-lock fd 3 (blocks until acquired)
  L_builtin flock -x 3

  // Non-blocking shared lock on the fd held in MYFD
  L_builtin flock -n -s $MYFD

  // Release the lock on fd 3
  L_builtin flock -u 3
",
);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn flock_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut exclusive = false;
    let mut shared = false;
    let mut unlock = false;
    let mut nonblock = false;
    let (fd_c,) = subcmd_getopts!(
        CMD,
        list,
        flags: [
            x => || exclusive = true,
            e => || exclusive = true,
            s => || shared = true,
            u => || unlock = true,
            n => || nonblock = true,
        ],
        required: [FD],
    );

    let fd: c_int = match unsafe { fd_c.as_str() }.ok().and_then(|s| s.parse().ok()) {
        Some(v) if v >= 0 => v,
        _ => {
        l_builtin_error!(b"invalid fd");
            return EX_USAGE;
        }
    };

    let mut op: c_int = 0;
    let mut chosen = 0;
    if exclusive {
        op |= libc::LOCK_EX;
        chosen += 1;
    }
    if shared {
        op |= libc::LOCK_SH;
        chosen += 1;
    }
    if unlock {
        op |= libc::LOCK_UN;
        chosen += 1;
    }
    if chosen > 1 {
        l_builtin_error!(b"-x/-s/-u are mutually exclusive");
        return EX_USAGE;
    }
    if chosen == 0 {
        op |= libc::LOCK_EX;
    }
    if nonblock {
        op |= libc::LOCK_NB;
    }

    let r = unsafe { libc::flock(fd, op) };
    if r < 0 {
        l_builtin_error!(b"flock: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}
