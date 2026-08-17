//! L_builtin `splice` subcommand: zero-copy move of data between two file
//! descriptors.
//!
//! Usage: `L_builtin splice [-v BYTES_VAR] FD_IN FD_OUT LEN [FLAGS]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::l_builtin_error;
use crate::{bprintln, subcmd_getopts};
use std::os::raw::{c_char, c_int};

const CMD: CmdDesc = CmdDesc::new(
    c"splice",
    c"[-v BYTES_VAR] FD_IN FD_OUT LEN [FLAGS]",
    c"\
Move up to LEN bytes from FD_IN to FD_OUT without copying them through
userspace (splice(2)). At least one fd must be a pipe. The number of bytes
moved is stored in BYTES_VAR (or printed if -v is omitted).

FLAGS combos:
  move   SPLICE_F_MOVE
  nonblock  SPLICE_F_NONBLOCK
  more   SPLICE_F_MORE
  gift   SPLICE_F_GIFT

Exit Status:
Returns success unless splice fails.

Examples:
  // Splice 1024 bytes from fd 3 (pipe) to fd 4 (pipe), print bytes moved
  L_builtin splice 3 4 1024

  // Splice with nonblock flag, store bytes moved in MOVED
  L_builtin splice -v MOVED 3 4 4096 nonblock

  // Splice with multiple flags (comma-separated)
  L_builtin splice 3 4 8192 move,more

  // Typical use: zero-copy pipe-to-pipe transfer
  // (assuming fd 3 is readable pipe, fd 4 is writable pipe)
  L_builtin splice 3 4 65536

  // Copy file to pipe (fd 3=file, fd 4=pipe) - requires splice support
  L_builtin splice 3 4 1048576
",
);

fn parse_flags(s: &str) -> Option<libc::c_uint> {
    let mut flags = 0u32;
    for tok in s.split(',') {
        match tok.trim().to_ascii_lowercase().as_str() {
            "move" => flags |= libc::SPLICE_F_MOVE,
            "nonblock" => flags |= libc::SPLICE_F_NONBLOCK,
            "more" => flags |= libc::SPLICE_F_MORE,
            "gift" => flags |= libc::SPLICE_F_GIFT,
            _ => return None,
        }
    }
    Some(flags)
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn splice_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut var: *mut c_char = std::ptr::null_mut();
    let (fd_in, fd_out, len, flags) = subcmd_getopts!(
        CMD,
        list,
        options: [ v => |v| var = v.as_ptr().cast() ],
        required: [fd_in, fd_out, len],
        optional: [flags],
    );

    let (fd_in, fd_out, len) = {
        let a = match unsafe { fd_in.as_str() }.ok().and_then(|s| s.parse().ok()) {
            Some(v) => v,
            None => {
                l_builtin_error!(b"invalid FD_IN");
                return EX_USAGE;
            }
        };
        let b = match unsafe { fd_out.as_str() }.ok().and_then(|s| s.parse().ok()) {
            Some(v) => v,
            None => {
                l_builtin_error!(b"invalid FD_OUT");
                return EX_USAGE;
            }
        };
        let c = match unsafe { len.as_str() }.ok().and_then(|s| s.parse().ok()) {
            Some(v) => v,
            None => {
                l_builtin_error!(b"invalid LEN");
                return EX_USAGE;
            }
        };
        (a, b, c)
    };

    let flags = match flags {
        Some(c) => match unsafe { c.as_str() } {
            Ok(s) => match parse_flags(s) {
                Some(f) => f,
                None => {
                    l_builtin_error!(b"invalid FLAGS");
                    return EX_USAGE;
                }
            },
            Err(_) => {
                l_builtin_error!(b"invalid FLAGS encoding");
                return EX_USAGE;
            }
        },
        None => 0,
    };

    let moved = unsafe {
        libc::splice(
            fd_in,
            std::ptr::null_mut(),
            fd_out,
            std::ptr::null_mut(),
            len,
            flags,
        )
    };
    if moved < 0 {
        l_builtin_error!(b"splice: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }

    if var.is_null() {
        bprintln!(moved as i64);
    } else {
        let s = crate::shared::SizeTStr::from_usize(moved as usize);
        if unsafe { crate::bash_api::bind_variable(var, s.as_ptr(), 0) }.is_null() {
            l_builtin_error!(b"cannot bind variable");
            return EXECUTION_FAILURE;
        }
    }
    EXECUTION_SUCCESS
}
