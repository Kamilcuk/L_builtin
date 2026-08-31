//! L_builtin `accept` subcommand: accept a network connection.
//!
//! Usage: `L_builtin accept CLIENTFD_VAR ADDR_VAR LISTENFD`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EX_RETRYFAIL, EX_USAGE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::shared::ensure_high_fd;
use crate::subcmd::{CmdDesc, CmdResult};
use crate::{bufwrite, l_builtin_error};
use cmdargs_derive::CmdArgs;
use std::os::raw::{c_char, c_int};

const CMD: CmdDesc = CmdDesc::new(
    c"accept",
    c"[-C] [-t MS] CLIENTFD_VAR ADDR_VAR LISTENFD",
    c"\
Accept an incoming connection on the listening socket file descriptor LISTENFD.
The new socket file descriptor for the client is stored in CLIENTFD_VAR.
The client's address (IP:PORT) is stored in ADDR_VAR.

By default the accepted fd is close-on-exec. With -C the close-on-exec flag is
cleared so the fd is inherited by child processes (e.g. a forked handler).

Exit Status:
Returns success unless accept fails or variable binding fails.

Examples:
  L_builtin listen -p PORT LFD 127.0.0.1 0
  (
    exec {cli}<>/dev/tcp/127.0.0.1/\"$PORT\"
    printf 'ping' >&\"$cli\"
    read -r r <&\"$cli\"
    echo \"server said $r\"
  ) &
  L_builtin accept CFD ADDR \"$LFD\"
  L_builtin recv -v msg \"$CFD\" 4
  echo \"client $ADDR said: $msg\"
  L_builtin send \"$CFD\" 'pong'
  exec {CFD}>&- {LFD}>&-
",
);

#[derive(CmdArgs)]
struct AcceptArgs {
    #[opt('t')]
    timeout_ms: Option<i32>,
    #[flag('C')]
    clear_cloexec: bool,
    #[positional]
    clientfd_var: BashVar,
    #[positional]
    addr_var: BashVar,
    #[positional]
    listenfd: c_int,
}

fn addr_to_ip_port(addr: libc::sockaddr_storage) -> [u8; 48] {
    if addr.ss_family == libc::AF_INET as libc::sa_family_t {
        let s = &addr as *const _ as *const libc::sockaddr_in;
        let ip = unsafe { std::net::Ipv4Addr::from((*s).sin_addr.s_addr.to_ne_bytes()) };
        let port = u16::from_be(unsafe { (*s).sin_port });
        bufwrite!(48, "{ip}:{port}")
    } else if addr.ss_family == libc::AF_INET6 as libc::sa_family_t {
        let s = &addr as *const _ as *const libc::sockaddr_in6;
        let ip = unsafe { std::net::Ipv6Addr::from((*s).sin6_addr.s6_addr) };
        let port = u16::from_be(unsafe { (*s).sin6_port });
        bufwrite!(48, "{ip}:{port}")
    } else {
        bufwrite!(48, "unknown:0")
    }
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn accept_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = AcceptArgs::parse(list)?;
    // Handle timeout using poll if specified
    if let Some(timeout) = args.timeout_ms {
        let mut pfd = libc::pollfd {
            fd: args.listenfd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ret = libc::poll(&mut pfd, 1, timeout);
        if ret < 0 {
            return Err(l_builtin_error!(
                ": poll failed: ",
                std::io::Error::last_os_error()
            ));
        } else if ret == 0 {
            l_builtin_error!(b": accept timed out");
            return Err(EX_RETRYFAIL);
        }
    }
    // Call accept
    let mut addr: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut addrlen = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    let clientfd = unsafe {
        libc::accept(
            args.listenfd,
            &mut addr as *mut _ as *mut libc::sockaddr,
            &mut addrlen,
        )
    };
    if clientfd < 0 {
        return Err(l_builtin_error!(
            ": accept failed: ",
            std::io::Error::last_os_error()
        ));
    }
    let clientfd = ensure_high_fd(clientfd, !args.clear_cloexec).map_err(|e| {
        l_builtin_error!(b": accept: fd dup failed: ", e);
        EXECUTION_FAILURE
    })?;
    // Format address
    let addr_buf = addr_to_ip_port(addr);
    // Bind clientfd variable - use the raw C string pointer
    args.clientfd_var.set_int(clientfd)?;
    // Bind addr variable - use stack buffer
    args.addr_var.set(addr_buf.as_ptr().cast())?;
    Ok(())
}
