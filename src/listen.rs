//! L_builtin `listen` subcommand: create a listening TCP socket.
//!
//! Usage: `L_builtin listen [-p PORT_VAR] LISTENFD_VAR [IP] [PORT]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EX_USAGE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::subcmd::{CmdDesc, CmdResult};
use crate::{l_builtin_error, l_builtin_usage_error};
use cmdargs_derive::CmdArgs;
use std::os::fd::{AsRawFd, IntoRawFd};

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
    port_var: Option<BashVar>,
    /// Variable to store the resulting listening socket fd in.
    #[positional]
    listenfd_var: BashVar,
    /// IP to bind to (defaults to 127.0.0.1).
    #[optional(default = "127.0.0.1")]
    ip: &'static str,
    /// Port to bind to (defaults to 0).
    #[optional(default = 0)]
    port: u16,
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
pub unsafe fn listen_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = ListenArgs::parse(list)?;
    if args.port == 0 && args.port_var.is_none() {
        return Err(l_builtin_usage_error!(
            b"-p PORT_VAR option is required when port is 0"
        ));
    }
    let listener = std::net::TcpListener::bind((args.ip, args.port))
        .map_err(|e| l_builtin_error!(b"bind failed: ", e))?;
    // If -p PORT_VAR is provided, get the actual bound port
    if let Some(ref port_var) = args.port_var {
        let port_num = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        port_var.set_int(port_num)?;
    }
    args.listenfd_var.set_int(listener.as_raw_fd())?;
    // Convert to raw fd (don't close the socket)
    let _ = listener.into_raw_fd();
    Ok(())
}
