//! L_builtin `recv` subcommand: receive bytes from a socket.
//!
//! Usage: `L_builtin recv [-f format] [-v RECV_VAR] [-n] [-i] FD SIZE`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EX_USAGE, WORD_LIST};
use crate::cmdargs::{BashVar, Cpnt};
use crate::subcmd::{CmdDesc, CmdResult};
use crate::l_builtin_error;
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

// Get format (optional, defaults to "raw")
#[derive(Copy, Clone)]
enum Format {
    Raw,
    Hex,
}

fn parse_format(cptr: Cpnt) -> Result<Format, String> {
    match unsafe { cptr.as_str() } {
        Ok("hex") => Ok(Format::Hex),
        Ok("raw") => Ok(Format::Raw),
        Ok(s) => Err(format!("invalid format, must be 'raw' or 'hex': {s}")),
        Err(e) => Err(e.to_string()),
    }
}

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
        let result =
            unsafe { libc::recv(args.fd, buf.as_mut_ptr() as *mut libc::c_void, args.size, flags) };
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
