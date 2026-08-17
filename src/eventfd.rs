//! L_builtin `eventfd` subcommand: create an eventfd(2) counter fd.
//!
//! Usage: `L_builtin eventfd [-n] [-s] [-c] VAR [INITVAL]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{bind_variable, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::CmdDesc;
use crate::subcmd_getopts;
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
  -c   EFD_CLOEXEC (toggle; on by default)

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

unsafe fn store_fd(var: *mut c_char, fd: c_int) -> bool {
    let s = crate::shared::I64Str::new(fd as i64);
    if unsafe { bind_variable(var, s.as_ptr(), 0) }.is_null() {
        l_builtin_error!(b"cannot bind variable");
        return false;
    }
    true
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn eventfd_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut nonblock = false;
    let mut semaphore = false;
    let mut cloexec = true;
    let (var, initval) = subcmd_getopts!(
        CMD,
        list,
        flags: [
            n => || nonblock = true,
            s => || semaphore = true,
            c => || cloexec = !cloexec,
        ],
        required: [VAR],
        optional: [INITVAL],
    );

    let initval: u32 = match initval {
        Some(c) => match unsafe { c.as_str() }.ok().and_then(|s| s.parse().ok()) {
            Some(v) => v,
            None => {
        l_builtin_error!(b"invalid INITVAL");
                return EX_USAGE;
            }
        },
        None => 0,
    };

    let mut flags = 0;
    if nonblock {
        flags |= libc::EFD_NONBLOCK;
    }
    if semaphore {
        flags |= libc::EFD_SEMAPHORE;
    }
    if cloexec {
        flags |= libc::EFD_CLOEXEC;
    }

    let fd = unsafe { libc::eventfd(initval, flags) };
    if fd < 0 {
        l_builtin_error!(b"eventfd: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }
    let var_ptr = var.as_ptr().cast::<c_char>();
    if !unsafe { store_fd(var_ptr, fd) } {
        unsafe { libc::close(fd) };
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}
