//! L_builtin `write` subcommand: write bytes to a file descriptor.
//!
//! Usage: `L_builtin write [-f format] [-v WRITTEN_VAR] [-n] FD DATA`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::WORD_LIST;
use crate::io_common::{Format, hex_decode, parse_format};
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"write",
    c"[-f format] [-v WRITTEN_VAR] [-n] FD DATA",
    c"\
Write DATA to the file descriptor FD via write(2). Works on any fd
(pipes, files, sockets, etc.), not just sockets.

Supported formats (-f):
  raw   Write DATA as raw bytes (default)
  hex   Decode DATA from hex representation first, then write

By default, write loops until all bytes are transmitted, retrying on short
writes and interrupted system calls (EINTR). If -n is provided, only a
single write(2) call is made and the result (which may be a short write)
is returned immediately.

If -v WRITTEN_VAR is provided, the number of bytes written is stored in
WRITTEN_VAR.

Exit Status:
Returns success unless write fails or variable binding fails.
",
);

#[derive(CmdArgs)]
struct WriteArgs {
    #[opt('f', default=Format::Raw)]
    #[parse(parse_format)]
    format: Format,
    #[flag('n')]
    non_blocking: bool,
    #[opt('v')]
    var: Option<BashVar>,
    #[positional]
    fd: c_int,
    #[positional]
    data: &'static [u8],
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn write_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = WriteArgs::parse(list)?;
    let hex_bytes;
    let data = match args.format {
        Format::Hex => match hex_decode(args.data) {
            Some(d) => {
                hex_bytes = d;
                &hex_bytes
            }
            None => return Err(l_builtin_error!(b"invalid hex string: ", args.data)),
        },
        Format::Raw => args.data,
    };

    let mut total_written: usize = 0;
    loop {
        let written = unsafe {
            libc::write(
                args.fd,
                data[total_written..].as_ptr().cast(),
                data.len() - total_written,
            )
        };
        if written < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(l_builtin_error!(b"write failed: ", err));
        }
        total_written += written as usize;
        if args.non_blocking || total_written == data.len() {
            break;
        }
    }

    if let Some(ret) = args.var {
        ret.set_int(total_written as i64)?;
    }
    Ok(())
}
