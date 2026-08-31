//! L_builtin `connect` subcommand: establish a TCP connection.
//!
//! Usage: `L_builtin connect CLIENTFD_VAR IP PORT`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EX_USAGE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::intstr::ToIntStr;
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::ffi::CStr;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"connect",
    c"CLIENTFD_VAR IP PORT",
    c"\
Establish an outgoing connection to IP on PORT, and store the resulting
socket file descriptor in CLIENTFD_VAR.

Exit Status:
Returns success unless connection fails or variable binding fails.
",
);

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[derive(CmdArgs)]
struct ConnectArgs {
    #[positional]
    clientfd_var: BashVar,
    #[positional]
    ip: &'static CStr,
    #[positional]
    port: u32,
}

pub unsafe fn connect_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();

    let args = ConnectArgs::parse(list)?;

    let ip_str = match args.ip.to_str() {
        Ok(s) => s,
        Err(_) => {
            l_builtin_error!(b"invalid IP");
            return Err(EX_USAGE);
        }
    };
    let port_num: u16 = args.port as u16;

    let stream = match std::net::TcpStream::connect((ip_str, port_num)) {
        Ok(s) => s,
        Err(e) => {
            l_builtin_error!(b"connect failed: ", e);
            return Err(EXECUTION_FAILURE);
        }
    };

    let sfd_int: i64 = stream.as_raw_fd() as i64;
    if let Err(e) = args.clientfd_var.set(sfd_int.to_intstr().as_ptr()) {
        return Err(e);
    }

    let _ = stream.into_raw_fd();

    Ok(())
}
