//! L_builtin `memfd` subcommand: create an anonymous memory-backed file
//! descriptor with memfd_create(2) and bind its fd to a shell variable.
//!
//! Usage: `L_builtin memfd VAR [NAME]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::intstr::ToIntStr;
use crate::l_builtin_error;
use crate::shared::ensure_high_fd;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"memfd",
    c"[-C] VAR [NAME]",
    c"\
Create an anonymous memory-backed file (memfd_create(2)) and store its file
descriptor in the shell variable VAR. The fd is a regular file-like object
living in RAM; its name appears in /proc/self/fd. NAME, if given, names the
memfd (otherwise a default name is used).

The fd is close-on-exec by default; -C clears it so the fd is inherited by
child processes.

Exit Status:
  Returns success unless memfd_create fails or the variable cannot be bound.

Examples:
  L_builtin memfd MYFD
  L_builtin memfd MYFD mydata
  L_builtin memfd FD
  echo data >&\"$FD\"
  cat <&\"$FD\"
  exec {FD}>&-
",
);

#[derive(CmdArgs)]
struct MemfdArgs {
    #[flag('C')]
    nocloexec: bool,
    /// Shell variable to bind the memfd fd to.
    #[positional]
    var: BashVar,
    /// Name for the memfd (default: L_builtin_memfd).
    #[optional(default = c"L_builtin_memfd".as_ptr())]
    name: *const c_char,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn memfd_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = MemfdArgs::parse(list)?;
    let flags: libc::c_uint = libc::MFD_NOEXEC_SEAL;
    let fd = unsafe { libc::memfd_create(args.name, flags) };
    if fd < 0 {
        l_builtin_error!(b"memfd_create: ", std::io::Error::last_os_error());
        return Err(EXECUTION_FAILURE);
    }
    let fd_int = ensure_high_fd(fd, !args.nocloexec).map_err(|e| {
        l_builtin_error!(b"memfd_create: fd dup failed: ", e);
        EXECUTION_FAILURE
    })?;
    if let Err(e) = args.var.set(fd_int.to_intstr().as_ptr()) {
        return Err(e);
    }
    Ok(())
}
