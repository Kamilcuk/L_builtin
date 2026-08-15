//! L_builtin `memfd` subcommand: create an anonymous memory-backed file
//! descriptor with memfd_create(2).
//!
//! Usage: `L_builtin memfd [-x] [-v FD_VAR] [NAME]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, bprintln, getopts, parse_positionals};
use std::os::raw::{c_char, c_int};

const ENAME: &str = "L_builtin memfd";

const CMD: CmdDesc = CmdDesc::new(
    c"memfd",
    c"[-x] [-v FD_VAR] [NAME]",
    c"\
Create an anonymous memory-backed file (memfd_create(2)) and store its file
descriptor in FD_VAR (or print it if -v is omitted). The fd is a regular
file-like object living in RAM; its name appears in /proc/self/fd.

Options:
  -x   MFD_EXEC (fd may be MFD-executed; default is MFD_CLOEXEC | MFD_NOEXEC_SEAL)
  -v   Store the resulting fd in the variable FD_VAR

Exit Status:
Returns success unless memfd_create fails or the variable cannot be bound.

Examples:
  // Create memfd with default name, default flags (CLOEXEC | NOEXEC_SEAL), print fd
  L_builtin memfd
  // Output: 3

  // Create memfd with custom name, store fd in MYFD
  L_builtin memfd -v MYFD mydata

  // Create memfd with MFD_EXEC flag (allows fexecve)
  L_builtin memfd -x myexec

  // Use memfd as temporary in-RAM storage
  L_builtin memfd -v FD
  echo data >&FD
  cat <&FD
",
);

unsafe fn store_fd(var: *mut c_char, fd: c_int) -> bool {
    if var.is_null() {
        bprintln!(fd as i64);
        return true;
    }
    let s = crate::shared::I64Str::new(fd as i64);
    if unsafe { crate::bash_api::bind_variable(var, s.as_ptr(), 0) }.is_null() {
        beprintln!(ENAME, b": cannot bind variable");
        return false;
    }
    true
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn memfd_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let mut exec = false;
    let mut fd_var: *mut c_char = std::ptr::null_mut();
    let rest = getopts!(
        list,
        [ x => || exec = true ],
        [ v => |v: crate::bash_api::Cpnt<'_>| fd_var = v.as_ptr().cast() ]
    );
    let (name,) = parse_positionals!(rest, [], [name]);

    let name: Vec<u8> = match name {
        Some(c) => {
            let b = unsafe { c.as_bytes() };
            let mut v = Vec::with_capacity(b.len() + 1);
            v.extend_from_slice(b);
            v.push(0);
            v
        }
        None => b"L_builtin_memfd\0".to_vec(),
    };

    let flags: libc::c_uint = if exec {
        libc::MFD_EXEC
    } else {
        libc::MFD_CLOEXEC | libc::MFD_NOEXEC_SEAL
    };

    let fd = unsafe { libc::memfd_create(name.as_ptr().cast(), flags) };
    if fd < 0 {
        beprintln!(ENAME, b": memfd_create: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }
    if !unsafe { store_fd(fd_var, fd) } {
        unsafe { libc::close(fd) };
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}
