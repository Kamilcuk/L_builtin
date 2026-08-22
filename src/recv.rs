//! L_builtin `recv` subcommand: receive bytes from a socket.
//!
//! Usage: `L_builtin recv [-f format] [-v RECV_VAR] [-n] [-i] FD SIZE`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EX_USAGE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::io_common::{hex_encode, parse_format, Format};
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"recv",
    c"[-f format] [-v RECV_VAR] [-n] [-i] FD SIZE",
    c"\
Receive up to SIZE bytes from the socket file descriptor FD.
Supported formats (-f):
  raw   Store raw bytes directly into RECV_VAR (null-byte unsafe) (default)
  hex   Store received bytes as hexadecimal string into RECV_VAR (null-byte safe)

If -n is provided, the recv call will be non-blocking. If no data is currently
available, it will return success immediately with an empty string.

If -i is provided, the recv call will not automatically retry on signal interruption
(EINTR). Instead, it will fail with an error. By default, recv retries on EINTR.

Exit Status:
Returns success unless recv fails or variable binding fails.
",
);

#[derive(CmdArgs)]
struct RecvArgs {
    #[flag('n')]
    non_blocking: bool,
    #[flag('i')]
    interruptible: bool,
    #[opt('f', default=Format::Raw)]
    #[parse(parse_format)]
    format: Format,
    #[opt('v')]
    var: Option<BashVar>,
    #[positional]
    fd: c_int,
    #[positional]
    size: usize,
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
pub unsafe fn recv_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = RecvArgs::parse(list)?;
    // Allocate buffer
    let mut buf = vec![0u8; args.size + 1];
    // Receive data
    let mut flags = 0;
    if args.non_blocking {
        flags |= libc::MSG_DONTWAIT;
    }
    let received;
    loop {
        let result = unsafe {
            libc::recv(
                args.fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                args.size,
                flags,
            )
        };
        if result < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                if args.interruptible {
                    l_builtin_error!(b"recv failed: Interrupted system call");
                    return Err(EXECUTION_FAILURE);
                }
                // Interrupted by signal, retry
                continue;
            }
            if args.non_blocking
                && (err.raw_os_error() == Some(libc::EAGAIN)
                    || err.raw_os_error() == Some(libc::EWOULDBLOCK))
            {
                received = 0;
                break;
            }
            l_builtin_error!(b"recv failed: ", err);
            return Err(EXECUTION_FAILURE);
        }
        received = result as usize;
        break;
    }
    buf[received] = 0; // null terminate
                       // If -v RECV_VAR is provided, store the result
    if let Some(var) = args.var {
        match args.format {
            Format::Hex => {
                // hex format - NUL-terminated byte vector, no C-string type
                let out = hex_encode(&buf[..received]);
                var.set(out.as_ptr().cast())?;
            }
            Format::Raw => {
                // raw format - buffer is already null-terminated, use directly
                var.set(buf.as_ptr().cast())?;
            }
        }
    }
    Ok(())
}
