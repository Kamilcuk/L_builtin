//! L_builtin `shutdown` subcommand: semi-close a network socket.
//!
//! Usage: `L_builtin shutdown FD [how]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use cmdargs_derive::CmdArgs;

use crate::bash_api::{this_cmd_name, EXECUTION_FAILURE, EX_USAGE, WORD_LIST};
use crate::beprintln;
use crate::cmdargs::Cpnt;
use crate::subcmd::{CmdDesc, CmdResult};
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"shutdown",
    c"FD [how]",
    c"\
Close parts or all of a full-duplex connection on network socket FD.
how can be one of:
  RD or 0    Further receptions will be disallowed
  WR or 1    Further transmissions will be disallowed
  RDWR or 2  Further receptions and transmissions will be disallowed (default)

Exit Status:
Returns success unless shutdown fails.
",
);

fn parse_how(cptr: Cpnt) -> Result<c_int, String> {
    match unsafe { cptr.as_str() } {
        Ok("RD") | Ok("0") => Ok(libc::SHUT_RD),
        Ok("WR") | Ok("1") => Ok(libc::SHUT_WR),
        Ok("RDWR") | Ok("2") => Ok(libc::SHUT_RDWR),
        Ok(_) => Err(format!(
            "invalid how, must be one of: RD WR RDWR 0 1 2: {cptr}"
        )),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(CmdArgs)]
struct ShutdownArgs {
    #[positional]
    fd: c_int,
    #[optional(default=libc::SHUT_RDWR)]
    #[parse(parse_how)]
    how: c_int,
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
pub unsafe fn shutdown_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = ShutdownArgs::parse(list)?;
    // Call shutdown
    if unsafe { libc::shutdown(args.fd, args.how) } < 0 {
        beprintln!(
            this_cmd_name(),
            b": shutdown(",
            args.fd,
            ", ",
            args.how,
            ") failed: ",
            std::io::Error::last_os_error()
        );
        return Err(EXECUTION_FAILURE);
    }
    Ok(())
}
