//! L_builtin `shutdown` subcommand: semi-close a network socket.
//!
//! Usage: `L_builtin shutdown FD [how]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListView, EX_USAGE, EXECUTION_SUCCESS, EXECUTION_FAILURE, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{bash_getopt, beprintln};
use std::os::raw::c_int;

const ENAME: &str = "L_builtin shutdown";

const CMD: CmdDesc = CmdDesc::new(
    c"shutdown",
    c"FD [how]",
    c"\
Close parts or all of a full-duplex connection on network socket FD.
how can be one of:
  RD or 0    Further receptions will be disallowed
  WR or 1    Further transmissions will be disallowed
  RDWR or 2  Further receptions and transmissions will be disallowed (default)

Exit Status:
Returns success unless shutdown fails.
",
);

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn shutdown_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let (_, args) = bash_getopt!(list, [], []);

    let view = unsafe { WordListView::from_raw(args) };
    let mut iter = view.iter();

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

    // Get how (optional, defaults to SHUT_RDWR)
    let mut how: c_int = libc::SHUT_RDWR;
    if let Some(how_cptr) = iter.next() {
        let how_bytes = unsafe { how_cptr.to_bytes() };
        let how_str = match std::str::from_utf8(how_bytes) {
            Ok(s) => s,
            Err(_) => {
                beprintln!(ENAME, b": invalid shutdown mode encoding");
                return EX_USAGE;
            }
        };

        how = match how_str {
            "RD" | "0" => libc::SHUT_RD,
            "WR" | "1" => libc::SHUT_WR,
            "RDWR" | "2" => libc::SHUT_RDWR,
            _ => {
                beprintln!(ENAME, b": invalid shutdown mode: ", how_bytes);
                return EX_USAGE;
            }
        };
    }

    // Call shutdown
    if unsafe { libc::shutdown(fd, how) } < 0 {
        beprintln!(ENAME, b": shutdown failed: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }

    EXECUTION_SUCCESS
}