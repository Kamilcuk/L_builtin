//! L_builtin `listen` subcommand: create a listening TCP socket.
//!
//! Usage: `L_builtin listen [-p PORT_VAR] LISTENFD_VAR [IP] [PORT]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::CmdDesc;
use cmdargs_derive::CmdArgs;
use std::ffi::CStr;
use std::os::fd::IntoRawFd;
use std::os::raw::{c_char, c_int};

const CMD: CmdDesc = CmdDesc::new(
    c"listen",
    c"[-p PORT_VAR] LISTENFD_VAR [IP] [PORT]",
    c"\
Create a new socket, bind it to IP and PORT, listen for incoming
connections, and store the resulting socket file descriptor in the
variable LISTENFD_VAR.

If IP is omitted, it defaults to 127.0.0.1.
If PORT is omitted, it defaults to 0 (ephemeral port allocation).

If -p PORT_VAR is provided, the actual bound port (useful when passing 0
for ephemeral port allocation) is stored in PORT_VAR.

Exit Status:
Returns success unless socket/bind/listen fails or variable binding fails.
",
);

#[derive(CmdArgs)]
struct ListenArgs {
    /// Store the actual bound port into shell variable PORT_VAR.
    #[opt('p')]
    port_var: Option<*const c_char>,

    /// Variable to store the resulting listening socket fd in.
    #[positional]
    listenfd_var: *const c_char,

    /// IP to bind to (defaults to 127.0.0.1).
    #[optional(default = c"127.0.0.1")]
    ip: &'static CStr,

    /// Port to bind to (defaults to 0).
    #[optional(default = 0)]
    port: u32,
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn listen_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();

    let args = match ListenArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };

    let port_var = args.port_var.map(|p| p as *mut c_char);

    let ip_str = match args.ip.to_str() {
        Ok(s) => s,
        Err(_) => {
            l_builtin_error!(b"invalid IP");
            return EX_USAGE;
        }
    };
    let port: u16 = args.port as u16;

    if port == 0 && port_var.is_none() {
        l_builtin_error!(b"-p PORT_VAR option is required when port is 0");
        return EX_USAGE;
    }

    let listener = match std::net::TcpListener::bind((ip_str, port)) {
        Ok(l) => l,
        Err(e) => {
            l_builtin_error!(b"bind failed: ", e);
            return EXECUTION_FAILURE;
        }
    };

    // If -p PORT_VAR is provided, get the actual bound port
    if let Some(port_var) = port_var {
        let port_num = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        let port_str = crate::shared::I64Str::new(port_num as i64);
        if unsafe { crate::bash_api::bind_variable(port_var, port_str.as_ptr(), 0) }.is_null() {
            l_builtin_error!(b"cannot bind port variable");
            return EXECUTION_FAILURE;
        }
    }

    // Convert to raw fd (don't close the socket)
    let sfd = listener.into_raw_fd();

    // Bind the listenfd variable - use the raw C string pointer
    let sfd_str = crate::shared::I64Str::new(sfd as i64);

    if unsafe { crate::bash_api::bind_variable(args.listenfd_var, sfd_str.as_ptr(), 0) }.is_null() {
        unsafe {
            libc::close(sfd);
        }
        l_builtin_error!(b"cannot bind variable");
        return EXECUTION_FAILURE;
    }

    EXECUTION_SUCCESS
}
