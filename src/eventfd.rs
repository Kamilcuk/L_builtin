//! L_builtin `eventfd` subcommand: create an eventfd(2) counter fd.
//!
//! Usage: `L_builtin eventfd [-n] [-s] [-c] VAR [INITVAL]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EX_USAGE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::intstr::ToIntStr;
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::{c_char, c_int};

const CMD: CmdDesc = CmdDesc::new(
    c"eventfd",
    c"[-n] [-s] [-c] VAR [INITVAL]",
    c"\
Create an eventfd(2) counting file descriptor and store its file descriptor
in the shell variable VAR.

Options:
  -n   EFD_NONBLOCK
  -s   EFD_SEMAPHORE
  -C   no EFD_CLOEXEC, is set by default

VAR is the variable the resulting fd is bound to (required). INITVAL
initializes the counter (default 0) and is optional.

Exit Status:
  Returns success unless the eventfd cannot be created or the variable cannot be bound.

Examples:
  // Create eventfd with default flags (CLOEXEC), counter=0, store fd in MYFD
  L_builtin eventfd MYFD

  // Create non-blocking eventfd, store fd in MYFD
  L_builtin eventfd -n MYFD

  // Create semaphore-style eventfd with initial value 5
  L_builtin eventfd -s MYFD 5

  // Create eventfd without CLOEXEC, initial value 100
  L_builtin eventfd -c MYFD 100
",
);

#[derive(CmdArgs)]
struct EventfdArgs {
    #[flag('n')]
    nonblock: bool,
    #[flag('s')]
    semaphore: bool,
    #[flag('C')]
    nocloexec: bool,
    #[positional]
    var: BashVar,
    #[optional(default = 0u32)]
    initval: u32,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn eventfd_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = EventfdArgs::parse(list)?;
    let mut flags = 0;
    if args.nonblock {
        flags |= libc::EFD_NONBLOCK;
    }
    if args.semaphore {
        flags |= libc::EFD_SEMAPHORE;
    }
    if !args.nocloexec {
        flags |= libc::EFD_CLOEXEC;
    }
    let fd = unsafe { libc::eventfd(args.initval, flags) };
    if fd < 0 {
        return Err(l_builtin_error!(
            b"eventfd: ",
            std::io::Error::last_os_error()
        ));
    }
    if let Err(e) = args.var.set(fd.to_intstr().as_ptr()) {
        unsafe { libc::close(fd) };
        return Err(e);
    }
    Ok(())
}
