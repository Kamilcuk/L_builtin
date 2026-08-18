//! L_builtin `close` subcommand: close a file descriptor.
//!
//! Usage: `L_builtin close FD`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EX_USAGE, WORD_LIST};
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
    let r = unsafe { libc::close(args.fd) };
    if r < 0 {
        return Err(l_builtin_error!(
            b"close: ",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
