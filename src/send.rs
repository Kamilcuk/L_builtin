//! L_builtin `send` subcommand: send bytes over a socket.
//!
//! Usage: `L_builtin send [-f format] [-v SENT_VAR] FD DATA`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EX_USAGE, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

const CMD: CmdDesc = CmdDesc::new(
    c"send",
    c"[-f format] [-v SENT_VAR] FD DATA",
    c"\
Transmit raw or encoded data over the socket file descriptor FD.
Supported formats (-f):
  raw   Transmit DATA as raw characters (default)
  hex   Transmit DATA after decoding from hex representation

If -v SENT_VAR is provided, the number of bytes successfully transmitted
is stored in SENT_VAR.

Exit Status:
Returns success unless send fails or variable binding fails.
",
);

fn hex_decode(hex: &[u8]) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = match std::str::from_utf8(&hex[i..i + 2]) {
            Ok(s) => u8::from_str_radix(s, 16).ok()?,
            Err(_) => return None,
        };
        out.push(byte);
    }
    Some(out)
}

fn cptr_to_str(ptr: *mut std::os::raw::c_char) -> Result<&'static str, ()> {
    if ptr.is_null() {
        return Err(());
    }
    unsafe { CStr::from_ptr(ptr).to_str().map_err(|_| ()) }
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
struct SendArgs {
    #[opt('f', default=Format::Raw)]
    #[parse(parse_format)]
    format: Format,
    #[opt('v')]
    var: Option<BashVar>,
    #[positional]
    fd: c_int,
    #[positional]
    data: &'static [u8],
}

pub unsafe fn send_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = SendArgs::parse(list)?;
    let hex_bytes;
    let data = match args.format {
        Format::Hex => match hex_decode(args.data) {
            Some(d) => {
                hex_bytes = d;
                hex_bytes.as_slice()
            }
            None => return Err(l_builtin_error!(b"invalid hex string: ", args.data)),
        },
        Format::Raw => args.data,
    };
    // Send data
    let sent = unsafe { libc::send(args.fd, data.as_ptr().cast(), data.len(), 0) };
    if sent < 0 {
        return Err(l_builtin_error!(
            b"send failed: ",
            std::io::Error::last_os_error()
        ));
    }
    // If -v SENT_VAR is provided, store the result
    if let Some(ret) = args.var {
        ret.set_int(sent)?;
    }
    Ok(())
}
