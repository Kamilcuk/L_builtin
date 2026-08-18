//! L_builtin `close` subcommand: close a file descriptor.
//!
//! Usage: `L_builtin close FD`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EX_USAGE, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"close",
    c"FD",
    c"\
Close the file descriptor FD (close(2)).

Exit Status:
  Returns success unless FD is invalid or close(2) fails.

Examples:
  // Close fd 3
  L_builtin close 3

  // Close the fd held in MYFD
  L_builtin close $MYFD
",
);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[derive(CmdArgs)]
struct CloseArgs {
    #[positional]
    fd: c_int,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn close_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();

    let args = CloseArgs::parse(list)?;

    let fd_c = crate::bash_api::Cpnt::new(args.fd as *mut c_char);

    let fd: c_int = match unsafe { fd_c.as_str() }.ok().and_then(|s| s.parse().ok()) {
        Some(v) if v >= 0 => v,
        _ => {
            l_builtin_error!(b"invalid fd");
            return Err(EX_USAGE);
        }
    };

    let r = unsafe { libc::close(fd) };
    if r < 0 {
        l_builtin_error!(b"close: ", std::io::Error::last_os_error());
        return Err(EXECUTION_FAILURE);
    }
    Ok(())
}
