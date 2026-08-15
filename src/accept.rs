//! L_builtin `accept` subcommand: accept a network connection.
//!
//! Usage: `L_builtin accept CLIENTFD_VAR ADDR_VAR LISTENFD`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{this_cmd_name, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, bufwrite, getopts, parse_positionals, shared};
use std::os::raw::c_int;

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

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn accept_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let rest = getopts!(list, [], []);
    let (clientfd_var, addr_var, fd_cptr) =
        parse_positionals!(rest, [CLIENTFD_VAR, ADDR_VAR, LISTENFD]);

    let fd_bytes = unsafe { fd_cptr.as_bytes() };
    let Some(listenfd) = shared::parse_bytes::<c_int>(fd_bytes) else {
        beprintln!(this_cmd_name(), b": invalid listenfd: ", fd_bytes);
        return EX_USAGE;
    };

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

    if unsafe { crate::bash_api::bind_variable(clientfd_var.as_ptr(), clientfd_str.as_ptr(), 0) }
        .is_null()
    {
        unsafe {
            libc::close(clientfd);
        }
        beprintln!(this_cmd_name(), b": cannot bind variable");
        return EXECUTION_FAILURE;
    }

    // Bind addr variable - use stack buffer
    if unsafe { crate::bash_api::bind_variable(addr_var.as_ptr(), addr_buf.as_ptr().cast(), 0) }
        .is_null()
    {
        beprintln!(this_cmd_name(), b": cannot bind variable");
        return EXECUTION_FAILURE;
    }

    EXECUTION_SUCCESS
}
