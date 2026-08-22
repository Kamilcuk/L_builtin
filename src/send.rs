//! L_builtin `send` subcommand: send bytes over a socket.
//!
//! Usage: `L_builtin send [-f format] [-v SENT_VAR] [-n] FD DATA`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::WORD_LIST;
use crate::io_common::{hex_decode, parse_format, Format};
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"send",
    c"[-f format] [-v SENT_VAR] [-n] FD DATA",
    c"\
Transmit raw or encoded data over the socket file descriptor FD.
Supported formats (-f):
  raw   Transmit DATA as raw characters (default)
  hex   Transmit DATA after decoding from hex representation

By default, send loops until all bytes are transmitted, retrying on short
writes and interrupted system calls (EINTR). If -n is provided, only a
single send(2) call is made and the result (which may be a short write)
is returned immediately.

If -v SENT_VAR is provided, the number of bytes successfully transmitted
is stored in SENT_VAR.

Exit Status:
Returns success unless send fails or variable binding fails.
",
);

#[derive(CmdArgs)]
struct SendArgs {
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
pub unsafe fn send_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = SendArgs::parse(list)?;
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

    let mut total_sent: usize = 0;
    loop {
        let sent = unsafe {
            libc::send(
                args.fd,
                data[total_sent..].as_ptr().cast(),
                data.len() - total_sent,
                0,
            )
        };
        if sent < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(l_builtin_error!(b"send failed: ", err));
        }
        total_sent += sent as usize;
        if args.non_blocking || total_sent == data.len() {
            break;
        }
    }

    if let Some(ret) = args.var {
        ret.set_int(total_sent as i64)?;
    }
    Ok(())
}
