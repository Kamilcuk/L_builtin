//! L_builtin `accept` subcommand: accept a network connection.
//!
//! Usage: `L_builtin accept CLIENTFD_VAR ADDR_VAR LISTENFD`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::subcmd::CmdDesc;
use crate::cmdargs::BashVar;
use crate::bash_api::{
    this_cmd_name, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST,
};
use crate::{beprintln, bufwrite};
use cmdargs_derive::CmdArgs;
use std::os::raw::{c_char, c_int};

const CMD: CmdDesc = CmdDesc::new(
    c"accept",
    c"CLIENTFD_VAR ADDR_VAR LISTENFD",
    c"\
Accept an incoming connection on the listening socket file descriptor LISTENFD.
The new socket file descriptor for the client is stored in CLIENTFD_VAR.
The client's address (IP:PORT) is stored in ADDR_VAR.

Exit Status:
Returns success unless accept fails or variable binding fails.
",
);

#[derive(CmdArgs)]
struct AcceptArgs {
    #[positional]
    clientfd_var: BashVar,
    #[positional]
    addr_var: BashVar,
    #[positional]
    listenfd: c_int,
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn accept_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let args = match AcceptArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };

    let listenfd = args.listenfd;

    // Call accept
    let mut addr: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut addrlen = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    let clientfd = unsafe {
        libc::accept(
            listenfd,
            &mut addr as *mut _ as *mut libc::sockaddr,
            &mut addrlen,
        )
    };
    if clientfd < 0 {
        beprintln!(
            this_cmd_name(),
            b": accept failed: ",
            std::io::Error::last_os_error()
        );
        return EXECUTION_FAILURE;
    }

    // Format address
    let addr_buf = if addr.ss_family == libc::AF_INET as libc::sa_family_t {
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
    };

    // Bind clientfd variable - use the raw C string pointer
    let clientfd_str = crate::shared::SizeTStr::from_usize(clientfd as usize);

    if let Err(e) = args.clientfd_var.set(clientfd_str.as_ptr()) {
        return e;
    }

    // Bind addr variable - use stack buffer
    if let Err(e) = args.addr_var.set(addr_buf.as_ptr().cast()) {
        return e;
    }

    EXECUTION_SUCCESS
}
