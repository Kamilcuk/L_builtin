//! L_builtin `recv` subcommand: receive bytes from a socket.
//!
//! Usage: `L_builtin recv [-f format] [-v RECV_VAR] [-n] [-i] FD SIZE`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListView, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, getopts};
use std::os::raw::{c_char, c_int};

const ENAME: &str = "L_builtin recv";

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
#[no_mangle]
pub unsafe extern "C" fn recv_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let mut non_blocking = false;
    let mut interruptible = false;
    let mut f_var: *mut c_char = std::ptr::null_mut();
    let mut var: *mut c_char = std::ptr::null_mut();
    let args = getopts!(
        list,
        [ n => || non_blocking = true,
          i => || interruptible = true ],
        [ f => |f: crate::bash_api::Cpnt<'_>| f_var = f.as_ptr().cast(),
          v => |v: crate::bash_api::Cpnt<'_>| var = v.as_ptr().cast() ]
    );

    let view = unsafe { WordListView::from_raw(args) };
    let mut iter = view.iter();

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
                beprintln!(ENAME, b": invalid format (must be raw or hex)");
                return EX_USAGE;
            }
        }
    } else {
        Format::Raw
    };

    // Get fd
    let fd = match iter.next() {
        Some(fd_cptr) => {
            let fd_bytes = unsafe { fd_cptr.to_bytes() };
            match std::str::from_utf8(fd_bytes) {
                Ok(s) => match s.parse::<c_int>() {
                    Ok(fd) => fd,
                    Err(_) => {
                        beprintln!(ENAME, b": invalid fd: ", fd_bytes);
                        return EX_USAGE;
                    }
                },
                Err(_) => {
                    beprintln!(ENAME, b": invalid fd encoding");
                    return EX_USAGE;
                }
            }
        }
        None => {
            beprintln!(ENAME, b": missing FD argument");
            return EX_USAGE;
        }
    };

    // Get size
    let size = match iter.next() {
        Some(size_cptr) => {
            let size_bytes = unsafe { size_cptr.to_bytes() };
            match std::str::from_utf8(size_bytes) {
                Ok(s) => match s.parse::<usize>() {
                    Ok(size) => size,
                    Err(_) => {
                        beprintln!(ENAME, b": invalid size: ", size_bytes);
                        return EX_USAGE;
                    }
                },
                Err(_) => {
                    beprintln!(ENAME, b": invalid size encoding");
                    return EX_USAGE;
                }
            }
        }
        None => {
            beprintln!(ENAME, b": missing SIZE argument");
            return EX_USAGE;
        }
    };

    // Allocate buffer
    let mut buf = vec![0u8; size + 1];

    // Receive data
    let mut flags = 0;
    if non_blocking {
        flags |= libc::MSG_DONTWAIT;
    }

    let received;
    loop {
        let result = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, size, flags) };
        if result < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                if interruptible {
                    beprintln!(ENAME, b": recv failed: Interrupted system call");
                    return EXECUTION_FAILURE;
                }
                // Interrupted by signal, retry
                continue;
            }
            if non_blocking
                && (err.raw_os_error() == Some(libc::EAGAIN)
                    || err.raw_os_error() == Some(libc::EWOULDBLOCK))
            {
                received = 0;
                break;
            }
            beprintln!(ENAME, b": recv failed: ", err);
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
                    beprintln!(ENAME, b": cannot bind variable");
                    return EXECUTION_FAILURE;
                }
            }
            Format::Raw => {
                // raw format - buffer is already null-terminated, use directly
                let out_ptr = buf.as_ptr() as *const c_char;
                if unsafe { crate::bash_api::bind_variable(var_ptr, out_ptr, 0) }.is_null() {
                    beprintln!(ENAME, b": cannot bind variable");
                    return EXECUTION_FAILURE;
                }
            }
        }
    }

    EXECUTION_SUCCESS
}
