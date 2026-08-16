//! L_builtin `lseek` subcommand: reposition file offset.
//!
//! Usage: `L_builtin lseek [-v VAR] fd offset [whence]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListView, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, getopts};
use std::os::raw::{c_char, c_int};

const ENAME: &str = "L_builtin lseek";

const CMD: CmdDesc = CmdDesc::new(
    c"lseek",
    c"[-v var] fd offset [whence]",
    c"\
Adjust the file offset of file descriptor FD to OFFSET bytes
according to WHENCE.

WHENCE can be one of:
  0 or SET  Seek from the beginning (default)
  1 or CUR  Seek from the current position
  2 or END  Seek from the end

If -v VAR is provided, the new offset is stored in VAR.

Exit Status:
Returns success unless an error occurs during lseek or variable binding.
",
);

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn lseek_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let mut var: *mut c_char = std::ptr::null_mut();
    let args = getopts!(
        list,
        [],
        [ v => |v: crate::bash_api::Cpnt<'_>| var = v.as_ptr().cast() ]
    );

    let view = unsafe { WordListView::from_raw(args) };
    let mut iter = view.iter();

    // Get fd
    let fd = match iter.next() {
        Some(fd_cptr) => {
            let fd_bytes = unsafe { fd_cptr.as_bytes() };
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
            beprintln!(ENAME, b": missing fd argument");
            return EX_USAGE;
        }
    };

    // Get offset
    let offset = match iter.next() {
        Some(offset_cptr) => {
            let offset_bytes = unsafe { offset_cptr.as_bytes() };
            match std::str::from_utf8(offset_bytes) {
                Ok(s) => match s.parse::<i64>() {
                    Ok(offset) => offset,
                    Err(_) => {
                        beprintln!(ENAME, b": invalid offset: ", offset_bytes);
                        return EX_USAGE;
                    }
                },
                Err(_) => {
                    beprintln!(ENAME, b": invalid offset encoding");
                    return EX_USAGE;
                }
            }
        }
        None => {
            beprintln!(ENAME, b": missing offset argument");
            return EX_USAGE;
        }
    };

    // Get whence (optional, defaults to SEEK_SET)
    let mut whence: c_int = libc::SEEK_SET;
    if let Some(whence_cptr) = iter.next() {
        let whence_bytes = unsafe { whence_cptr.as_bytes() };
        let whence_str = match std::str::from_utf8(whence_bytes) {
            Ok(s) => s,
            Err(_) => {
                beprintln!(ENAME, b": invalid whence encoding");
                return EX_USAGE;
            }
        };

        whence = match whence_str {
            "SET" | "0" => libc::SEEK_SET,
            "CUR" | "1" => libc::SEEK_CUR,
            "END" | "2" => libc::SEEK_END,
            _ => {
                beprintln!(ENAME, b": invalid whence: ", whence_bytes);
                return EX_USAGE;
            }
        };
    }

    // Call lseek
    let result = unsafe { libc::lseek(fd, offset, whence) };
    if result == -1 {
        beprintln!(ENAME, b": lseek error: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }

    // If -v VAR was provided, store the result
    if !var.is_null() {
        let var_ptr = var;
        let result_str = crate::shared::I64Str::new(result);

        unsafe {
            if crate::bash_api::bind_variable(var_ptr, result_str.as_ptr(), 0).is_null() {
                beprintln!(ENAME, b": cannot bind variable");
                return EXECUTION_FAILURE;
            }
        }
    }

    EXECUTION_SUCCESS
}
