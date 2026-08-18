//! L_builtin `recv` subcommand: receive bytes from a socket.
//!
//! Usage: `L_builtin recv [-f format] [-v RECV_VAR] [-n] [-i] FD SIZE`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::cmdargs::{BashVar, CStr};
use crate::l_builtin_error;
use crate::subcmd::CmdDesc;
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

const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

/// Hex-encode `data` into a NUL-terminated byte vector. It is passed straight
/// to the bash interface, which only needs a zero-terminated string, so the
/// bytes are written directly into the `Vec<u8>` (hex nibble lookup) instead
/// of building a `String`/`format!` per byte.
fn hex_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2 + 1);
    for byte in data {
        out.push(HEX_CHARS[(byte >> 4) as usize]); // high nibble
        out.push(HEX_CHARS[(byte & 0x0f) as usize]); // low nibble
    }
    out.push(0);
    out
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[derive(CmdArgs)]
struct RecvArgs {
    #[flag('n')]
    non_blocking: bool,
    #[flag('i')]
    interruptible: bool,
    #[opt('f')]
    f_var: Option<&'static CStr>,
    #[opt('v')]
    var: Option<BashVar>,
    #[positional]
    fd: c_int,
    #[positional]
    size: usize,
}

#[no_mangle]
pub unsafe extern "C" fn recv_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();

    let args = match RecvArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };

    let f_var = args
        .f_var
        .map_or(std::ptr::null_mut(), |c| c.as_ptr() as *mut c_char);
    let var = args
        .var
        .map_or(std::ptr::null_mut(), |c| c.as_ptr() as *mut c_char);
    let fd_c = crate::bash_api::Cpnt::new(args.fd as *mut c_char);

    // Get format (optional, defaults to "raw")
    #[derive(Copy, Clone)]
    enum Format {
        Raw,
        Hex,
    }

    let format = if !f_var.is_null() {
        match crate::shared::cstr_to_str(f_var) {
            Some("hex") => Format::Hex,
            Some("raw") | None => Format::Raw,
            Some(_) => {
                l_builtin_error!(b"invalid format (must be raw or hex)");
                return EX_USAGE;
            }
        }
    } else {
        Format::Raw
    };

    // Get fd
    let fd = {
        let fd_bytes = unsafe { fd_c.as_bytes() };
        match std::str::from_utf8(fd_bytes) {
            Ok(s) => match s.parse::<c_int>() {
                Ok(fd) => fd,
                Err(_) => {
                    l_builtin_error!(b"invalid fd: ", fd_bytes);
                    return EX_USAGE;
                }
            },
            Err(_) => {
                l_builtin_error!(b"invalid fd encoding");
                return EX_USAGE;
            }
        }
    };

    // Allocate buffer
    let mut buf = vec![0u8; args.size + 1];

    // Receive data
    let mut flags = 0;
    if args.non_blocking {
        flags |= libc::MSG_DONTWAIT;
    }

    let received;
    loop {
        let result =
            unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, args.size, flags) };
        if result < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                if args.interruptible {
                    l_builtin_error!(b"recv failed: Interrupted system call");
                    return EXECUTION_FAILURE;
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
            return EXECUTION_FAILURE;
        }
        received = result as usize;
        break;
    }

    buf[received] = 0; // null terminate

    // If -v RECV_VAR is provided, store the result
    if !var.is_null() {
        let var_ptr = var;
        match format {
            Format::Hex => {
                // hex format - NUL-terminated byte vector, no C-string type
                let out = hex_encode(&buf[..received]);
                if unsafe { crate::bash_api::bind_variable(var_ptr, out.as_ptr().cast(), 0) }
                    .is_null()
                {
                    l_builtin_error!(b"cannot bind variable");
                    return EXECUTION_FAILURE;
                }
            }
            Format::Raw => {
                // raw format - buffer is already null-terminated, use directly
                let out_ptr = buf.as_ptr() as *const c_char;
                if unsafe { crate::bash_api::bind_variable(var_ptr, out_ptr, 0) }.is_null() {
                    l_builtin_error!(b"cannot bind variable");
                    return EXECUTION_FAILURE;
                }
            }
        }
    }

    EXECUTION_SUCCESS
}
