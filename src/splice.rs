//! L_builtin `splice` subcommand: zero-copy move of data between two file
//! descriptors.
//!
//! Usage: `L_builtin splice [-v BYTES_VAR] FD_IN FD_OUT LEN [FLAGS]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EX_USAGE, WORD_LIST};
use crate::bprintln;
use crate::intstr::ToIntStr;
use crate::l_builtin_error;
use crate::shared::bind_variable_check;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::ffi::CStr;
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

#[derive(CmdArgs)]
struct SpliceArgs {
    /// Store the number of bytes moved into shell variable BYTES_VAR.
    #[opt('v')]
    var: Option<*const c_char>,

    /// Source file descriptor.
    #[positional]
    fd_in: c_int,

    /// Destination file descriptor.
    #[positional]
    fd_out: c_int,

    /// Maximum number of bytes to move.
    #[positional]
    len: usize,

    /// Optional splice flags (comma-separated).
    #[optional]
    flags: Option<&'static CStr>,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn splice_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();

    let args = SpliceArgs::parse(list)?;

    let var = args.var.map_or(std::ptr::null_mut(), |p| p as *mut c_char);
    let fd_in = args.fd_in;
    let fd_out = args.fd_out;
    let len = args.len;

    let flags = match args.flags {
        Some(c) => match c.to_str() {
            Ok(s) => match parse_flags(s) {
                Some(f) => f,
                None => {
                    l_builtin_error!(b"invalid FLAGS");
                    return Err(EX_USAGE);
                }
            },
            Err(_) => {
                l_builtin_error!(b"invalid FLAGS encoding");
                return Err(EX_USAGE);
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
        return Err(EXECUTION_FAILURE);
    }

    if var.is_null() {
        bprintln!(moved as i64);
    } else {
        let moved_int: i64 = moved as i64;
        if bind_variable_check(var, moved_int.to_intstr().as_ptr(), 0) != 0 {
            return Err(EXECUTION_FAILURE);
        }
    }
    Ok(())
}
