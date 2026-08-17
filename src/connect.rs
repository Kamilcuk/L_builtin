//! L_builtin `connect` subcommand: establish a TCP connection.
//!
//! Usage: `L_builtin connect CLIENTFD_VAR IP PORT`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::l_builtin_error;
use crate::{subcmd_getopts};
use std::os::fd::{AsRawFd, IntoRawFd};
use std::os::raw::c_int;

use crate::subcmd::CmdDesc;

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
    let (clientfd_var, ip, port) = subcmd_getopts!(
        CMD,
        list,
        required: [CLIENTFD_VAR, IP, PORT],
    );

    let ip_str = unsafe { ip.as_str().unwrap_or("0.0.0.0") };
    let port_str = unsafe { port.as_str().unwrap_or("0") };
    let port_num: u16 = match port_str.parse() {
        Ok(p) => p,
        Err(_) => {
            l_builtin_error!(b"invalid port: ", port_str.as_bytes());
            return EX_USAGE;
        }
    };

    let stream = match std::net::TcpStream::connect((ip_str, port_num)) {
        Ok(s) => s,
        Err(e) => {
            l_builtin_error!(b"connect failed: ", e);
            return EXECUTION_FAILURE;
        }
    };

    let sfd_str = crate::shared::SizeTStr::from_usize(stream.as_raw_fd() as usize);
    if unsafe { crate::bash_api::bind_variable(clientfd_var.as_ptr(), sfd_str.as_ptr(), 0) }
        .is_null()
    {
        l_builtin_error!(b"cannot bind variable");
        return EXECUTION_FAILURE;
    }

    let _ = stream.into_raw_fd();

    EXECUTION_SUCCESS
}
