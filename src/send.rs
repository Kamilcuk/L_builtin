//! L_builtin `send` subcommand: send bytes over a socket.
//!
//! Usage: `L_builtin send [-f format] [-v SENT_VAR] FD DATA`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::CmdDesc;
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
    if hex.len() % 2 != 0 {
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

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[derive(CmdArgs)]
struct SendArgs {
    #[opt('f')]
    f_var: Option<&'static CStr>,
    #[opt('v')]
    var: Option<&'static CStr>,
    #[positional]
    fd: *const c_char,
    #[positional]
    data: *const c_char,
}

#[no_mangle]
pub unsafe extern "C" fn send_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let args = match SendArgs::parse(list) {
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
    let data_c = crate::bash_api::Cpnt::new(args.data as *mut c_char);

    // Get format (optional, defaults to "raw")
    let format = if !f_var.is_null() {
        match cptr_to_str(f_var) {
            Ok(s) => s,
            Err(_) => {
                l_builtin_error!(b"invalid format encoding");
                return EX_USAGE;
            }
        }
    } else {
        "raw"
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

    // Get data
    let data = unsafe { data_c.as_bytes() };

    // Prepare data to send
    let send_data: Vec<u8> = if format == "hex" {
        match hex_decode(data) {
            Some(d) => d,
            None => {
                l_builtin_error!(b"invalid hex string");
                return EXECUTION_FAILURE;
            }
        }
    } else if format == "raw" {
        data.to_vec()
    } else {
        l_builtin_error!(b"invalid format (must be raw or hex): ", format);
        return EX_USAGE;
    };

    // Send data
    let sent = unsafe {
        libc::send(
            fd,
            send_data.as_ptr() as *const libc::c_void,
            send_data.len(),
            0,
        )
    };
    if sent < 0 {
        l_builtin_error!(b"send failed: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }

    // If -v SENT_VAR is provided, store the result
    if !var.is_null() {
        let var_ptr = var;
        let sent_str = crate::shared::SizeTStr::from_usize(sent as usize);
        if unsafe { crate::bash_api::bind_variable(var_ptr, sent_str.as_ptr(), 0) }.is_null() {
            l_builtin_error!(b"cannot bind variable");
            return EXECUTION_FAILURE;
        }
    }

    EXECUTION_SUCCESS
}
