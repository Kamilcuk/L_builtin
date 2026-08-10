//! L_builtin `eventfd` subcommand: create an eventfd(2) counter fd.
//!
//! Usage: `L_builtin eventfd [-n] [-s] [-c] [-v FD_VAR] [INITVAL]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{this_cmd_name, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, bprintln, getopts, parse_positionals};
use std::os::raw::{c_char, c_int};

const CMD: CmdDesc = CmdDesc::new(
    c"eventfd",
    c"[-n] [-s] [-c] [-v FD_VAR] [INITVAL]",
    c"\
Create an eventfd(2) counting file descriptor and store it in FD_VAR
(or print it if -v is omitted).

Options:
  -n   EFD_NONBLOCK
  -s   EFD_SEMAPHORE
  -c   EFD_CLOEXEC (toggle; on by default)
  -v   Store the resulting fd in the variable FD_VAR

INITVAL initializes the counter (default 0).

Exit Status:
Returns success unless the eventfd cannot be created or the variable cannot be bound.

Examples:
  // Create eventfd with default flags (CLOEXEC), counter=0, print fd
  L_builtin eventfd
  // Output: 3

  // Create non-blocking eventfd, store fd in MYFD
  L_builtin eventfd -n -v MYFD
  echo $$MYFD

  // Create semaphore-style eventfd with initial value 5
  L_builtin eventfd -s 5

  // Create eventfd without CLOEXEC, initial value 100
  L_builtin eventfd -c 100
",
);

/// Bind `fd` into the shell variable `var`, or print it to stdout when `var`
/// is NULL. Returns `false` on failure.
unsafe fn store_fd(var: *mut c_char, fd: c_int) -> bool {
    if var.is_null() {
        bprintln!(fd as i64);
        return true;
    }
    let s = crate::shared::I64Str::new(fd as i64);
    if unsafe { crate::bash_api::bind_variable(var, s.as_ptr(), 0) }.is_null() {
        beprintln!(this_cmd_name(), b": cannot bind variable");
        return false;
    }
    true
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn eventfd_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let mut nonblock = false;
    let mut semaphore = false;
    let mut cloexec = true;
    let mut fd_var: *mut c_char = std::ptr::null_mut();
    let rest = getopts!(
        list,
        [ n => || nonblock = true,
          s => || semaphore = true,
          c => || cloexec = !cloexec ],
        [ v => |v| fd_var = v.as_ptr().cast() ]
    );
    let (initval,) = parse_positionals!(rest, [], [initval]);
    let initval: u32 = match initval {
        Some(c) => match unsafe { c.to_str() }.ok().and_then(|s| s.parse().ok()) {
            Some(v) => v,
            None => {
                beprintln!(this_cmd_name(), b": invalid INITVAL");
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
        beprintln!(
            this_cmd_name(),
            b": eventfd: ",
            std::io::Error::last_os_error()
        );
        return EXECUTION_FAILURE;
    }
    if !unsafe { store_fd(fd_var, fd) } {
        unsafe { libc::close(fd) };
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}
