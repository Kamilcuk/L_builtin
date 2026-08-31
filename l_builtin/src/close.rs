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
    c"FD...",
    c"\
Close the file descriptor(s) FD... (close(2)).

Exit Status:
   Returns success unless any FD is invalid or close(2) fails.

Examples:
   L_builtin close 3
   L_builtin close $MYFD
   L_builtin close 3 4 5
   L_builtin close $FD1 $FD2 $FD3
",
);

/// # Safety
///
// Safe when called from bash with a valid WORD_LIST pointer.
#[derive(CmdArgs)]
struct CloseArgs {
    #[positional]
    fd: c_int,
    #[rest]
    fds: Vec<c_int>,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn close_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = CloseArgs::parse(list)?;
    let mut ret = Ok(());
    // Close all fds: the first one plus any additional ones
    let all_fds = std::iter::once(args.fd).chain(args.fds.into_iter());
    for fd in all_fds {
        let r = unsafe { libc::close(fd) };
        if r < 0 {
            ret = Err(l_builtin_error!(
                b"close fd ",
                fd,
                b": ",
                std::io::Error::last_os_error()
            ));
            break;
        }
    }
    ret
}
