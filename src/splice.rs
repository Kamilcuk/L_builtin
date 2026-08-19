//! L_builtin `splice` subcommand: zero-copy move of data between two file
//! descriptors.
//!
//! Usage: `L_builtin splice [-v BYTES_VAR] FD_IN FD_OUT LEN [FLAGS]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EX_USAGE, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
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

fn parse_flags(cpnt: Cpnt) -> Result<libc::c_uint, String> {
    let mut flags = 0u32;
    let s = unsafe { cpnt.as_str() }.map_err(|e| e.to_string())?;
    for tok in s.split(',') {
        match tok.trim().to_ascii_lowercase().as_str() {
            "move" => flags |= libc::SPLICE_F_MOVE,
            "nonblock" => flags |= libc::SPLICE_F_NONBLOCK,
            "more" => flags |= libc::SPLICE_F_MORE,
            "gift" => flags |= libc::SPLICE_F_GIFT,
            _ => return Err(format!("invalid flag: {tok}")),
        };
    }
    Ok(flags)
}

#[derive(CmdArgs)]
struct SpliceArgs {
    /// Store the number of bytes moved into shell variable BYTES_VAR.
    #[opt('v')]
    var: Option<BashVar>,
    /// Source file descriptor.
    #[positional]
    fd_in: c_int,
    /// Destination file descriptor.
    #[positional]
    fd_out: c_int,
    /// Maximum number of bytes to move.
    #[optional(default=usize::MAX)]
    len: usize,
    /// Optional splice flags (comma-separated).
    #[optional(default=0 as libc::c_uint)]
    #[parse(parse_flags)]
    flags: libc::c_uint,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn splice_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = SpliceArgs::parse(list)?;
    let moved = unsafe {
        libc::splice(
            args.fd_in,
            std::ptr::null_mut(),
            args.fd_out,
            std::ptr::null_mut(),
            args.len,
            args.flags,
        )
    };
    if moved < 0 {
        return Err(l_builtin_error!(b"splice: ", std::io::Error::last_os_error()));
    }
    if let Some(var) = args.var {
        var.set_int(moved)?;
    }
    Ok(())
}
