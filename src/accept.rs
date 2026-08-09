//! L_builtin `accept` subcommand: accept a network connection.
//!
//! Usage: `L_builtin accept CLIENTFD_VAR ADDR_VAR LISTENFD`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListView, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::{bash_getopt, beprintln, bufwrite, shared};
use std::os::raw::c_int;

const ENAME: &str = "L_builtin accept";

fn print_accept_help() {
    let doc = b"\
L_builtin accept CLIENTFD_VAR ADDR_VAR LISTENFD

Accept an incoming connection on the listening socket file descriptor LISTENFD.
The new socket file descriptor for the client is stored in CLIENTFD_VAR.
The client's address (IP:PORT) is stored in ADDR_VAR.

Exit Status:
Returns success unless accept fails or variable binding fails.
";
    beprintln!(doc);
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn accept_subcommand(list: *mut WORD_LIST) -> c_int {
    let (_, args) = bash_getopt!(list, print_accept_help, [], []);

    let view = unsafe { WordListView::from_raw(args) };
    let mut iter = view.iter();

    // Get clientfd_var and addr_var
    let Some(clientfd_var) = iter.next() else {
        beprintln!(ENAME, b": missing CLIENTFD_VAR argument");
        return EX_USAGE;
    };
    let Some(addr_var) = iter.next() else {
        beprintln!(ENAME, b": missing ADDR_VAR argument");
        return EX_USAGE;
    };

    // Get listenfd
    let Some(fd_cptr) = iter.next() else {
        beprintln!(ENAME, b": missing LISTENFD argument");
        return EX_USAGE;
    };
    let fd_bytes = unsafe { fd_cptr.to_bytes() };
    let Some(listenfd) = shared::parse_bytes::<c_int>(fd_bytes) else {
        beprintln!(ENAME, b": invalid listenfd: ", fd_bytes);
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
        beprintln!(ENAME, b": accept failed: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }

    // Format address
    let addr_ptr = if addr.ss_family == libc::AF_INET as libc::sa_family_t {
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
        bufwrite!(16, "unknown:0")
    };

    // Bind clientfd variable - use the raw C string pointer
    let clientfd_str = crate::shared::SizeTStr::from_usize(clientfd as usize);

    if unsafe { crate::bash_api::bind_variable(clientfd_var.as_ptr(), clientfd_str.as_ptr(), 0) }
        .is_null()
    {
        unsafe {
            libc::close(clientfd);
        }
        beprintln!(ENAME, b": cannot bind variable");
        return EXECUTION_FAILURE;
    }

    // Bind addr variable - use stack buffer
    if unsafe { crate::bash_api::bind_variable(addr_var.as_ptr(), addr_ptr, 0) }.is_null() {
        beprintln!(ENAME, b": cannot bind variable");
        return EXECUTION_FAILURE;
    }

    EXECUTION_SUCCESS
}

