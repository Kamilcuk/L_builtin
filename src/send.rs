//! L_builtin `send` subcommand: send bytes over a socket.
//!
//! Usage: `L_builtin send [-f format] [-v SENT_VAR] FD DATA`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListView, EX_USAGE, EXECUTION_SUCCESS, EXECUTION_FAILURE, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, getopts};
use std::os::raw::{c_char, c_int};
use std::ffi::CStr;

const ENAME: &str = "L_builtin send";

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
        let byte = match std::str::from_utf8(&hex[i..i+2]) {
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
    unsafe {
        CStr::from_ptr(ptr)
            .to_str()
            .map_err(|_| ())
    }
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn send_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let mut f_var: *mut c_char = std::ptr::null_mut();
    let mut var: *mut c_char = std::ptr::null_mut();
    let args = getopts!(
        list,
        [],
        [ f => |f: crate::bash_api::Cpnt<'_>| f_var = f.as_ptr().cast(),
          v => |v: crate::bash_api::Cpnt<'_>| var = v.as_ptr().cast() ]
    );

    let view = unsafe { WordListView::from_raw(args) };
    let mut iter = view.iter();

    // Get format (optional, defaults to "raw")
    let format = if !f_var.is_null() {
        match cptr_to_str(f_var) {
            Ok(s) => s,
            Err(_) => {
                beprintln!(ENAME, b": invalid format encoding");
                return EX_USAGE;
            }
        }
    } else {
        "raw"
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

    // Get data
    let data = match iter.next() {
        Some(data_cptr) => {
            let data_bytes = unsafe { data_cptr.to_bytes() };
            data_bytes
        }
        None => {
            beprintln!(ENAME, b": missing DATA argument");
            return EX_USAGE;
        }
    };

    // Prepare data to send
    let send_data: Vec<u8> = if format == "hex" {
        match hex_decode(data) {
            Some(d) => d,
            None => {
                beprintln!(ENAME, b": invalid hex string");
                return EXECUTION_FAILURE;
            }
        }
    } else if format == "raw" {
        data.to_vec()
    } else {
        beprintln!(ENAME, b": invalid format (must be raw or hex): ", format);
        return EX_USAGE;
    };

    // Send data
    let sent = unsafe { libc::send(fd, send_data.as_ptr() as *const libc::c_void, send_data.len(), 0) };
    if sent < 0 {
        beprintln!(ENAME, b": send failed: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }

    // If -v SENT_VAR is provided, store the result
    if !var.is_null() {
        let var_ptr = var;
        let sent_str = crate::shared::SizeTStr::from_usize(sent as usize);
        if unsafe { crate::bash_api::bind_variable(var_ptr, sent_str.as_ptr(), 0) }.is_null() {
            beprintln!(ENAME, b": cannot bind variable");
            return EXECUTION_FAILURE;
        }
    }

    EXECUTION_SUCCESS
}