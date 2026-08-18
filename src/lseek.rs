//! L_builtin `lseek` subcommand: reposition file offset.
//!
//! Usage: `L_builtin lseek [-v VAR] fd offset [whence]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::ffi::c_char;

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

/// `L_builtin lseek [-v VAR] fd offset [whence]`
#[derive(CmdArgs)]
struct LseekArgs {
    /// Store the resulting offset into shell variable VAR.
    #[opt('v')]
    var: Option<BashVar>,

    /// File descriptor to seek on.
    #[positional]
    fd: c_int,

    /// Offset in bytes.
    #[positional]
    offset: i64,

    /// Seek whence: SET/CUR/END or 0/1/2 (default SET).
    #[optional(default = libc::SEEK_SET)]
    #[parse(|cptr| match unsafe { cptr.as_str() } {
        Ok("SET") | Ok("0") => Ok(libc::SEEK_SET),
        Ok("CUR") | Ok("1") => Ok(libc::SEEK_CUR),
        Ok("END") | Ok("2") => Ok(libc::SEEK_END),
        Ok(s) => Err(format!("invalid whence: {s}")),
        Err(e) => Err(e.to_string()),
    })]
    whence: c_int,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn lseek_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = LseekArgs::parse(list)?;
    let result = libc::lseek(args.fd, args.offset, args.whence);
    if result == -1 {
        l_builtin_error!(b"lseek error: ", std::io::Error::last_os_error());
        return Err(EXECUTION_FAILURE);
    }
    if let Some(var) = args.var {
        var.set_int(result)?;
    }
    Ok(())
}
