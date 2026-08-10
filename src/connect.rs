//! L_builtin `connect` subcommand: establish a TCP connection.
//!
//! Usage: `L_builtin connect CLIENTFD_VAR IP PORT`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListView, EX_USAGE, EXECUTION_SUCCESS, EXECUTION_FAILURE, WORD_LIST};
use crate::{beprintln, getopts};
use std::os::fd::{AsRawFd, IntoRawFd};
use std::os::raw::c_int;

use crate::subcmd::CmdDesc;

const ENAME: &str = "L_builtin connect";

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
#[no_mangle]
pub unsafe extern "C" fn connect_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let args = getopts!(list, [], []);

    let view = unsafe { WordListView::from_raw(args) };
    let mut iter = view.iter();

    // Get clientfd_var, IP, and PORT
    let Some(clientfd_var) = iter.next() else {
        beprintln!(ENAME, b": missing CLIENTFD_VAR argument");
        return EX_USAGE;
    };
    let Some(ip) = iter.next() else {
        beprintln!(ENAME, b": missing IP argument");
        return EX_USAGE;
    };
    let Some(port) = iter.next() else {
        beprintln!(ENAME, b": missing PORT argument");
        return EX_USAGE;
    };

    let ip_str = unsafe { ip.to_str().unwrap_or("0.0.0.0") };
    let port_str = unsafe { port.to_str().unwrap_or("0") };
    let port_num: u16 = match port_str.parse() {
        Ok(p) => p,
        Err(_) => {
            beprintln!(ENAME, b": invalid port: ", port_str.as_bytes());
            return EX_USAGE;
        }
    };

    let stream = match std::net::TcpStream::connect((ip_str, port_num)) {
        Ok(s) => s,
        Err(e) => {
            beprintln!(ENAME, b": connect failed: ", e);
            return EXECUTION_FAILURE;
        }
    };

    let sfd_str = crate::shared::SizeTStr::from_usize(stream.as_raw_fd() as usize);
    if unsafe { crate::bash_api::bind_variable(clientfd_var.as_ptr(), sfd_str.as_ptr(), 0) }.is_null() {
        beprintln!(ENAME, b": cannot bind variable");
        return EXECUTION_FAILURE;
    }

    let _ = stream.into_raw_fd();

    EXECUTION_SUCCESS
}