//! L_builtin `memfd` subcommand: create an anonymous memory-backed file
//! descriptor with memfd_create(2) and bind its fd to a shell variable.
//!
//! Usage: `L_builtin memfd VAR [NAME]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{bind_variable, EXECUTION_FAILURE, EXECUTION_SUCCESS, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::CmdDesc;
use crate::subcmd_getopts;
use std::ffi::c_char;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"memfd",
    c"VAR [NAME]",
    c"\
Create an anonymous memory-backed file (memfd_create(2)) and store its file
descriptor in the shell variable VAR. The fd is a regular file-like object
living in RAM; its name appears in /proc/self/fd. NAME, if given, names the
memfd (otherwise a default name is used). The memfd is created with
MFD_CLOEXEC | MFD_NOEXEC_SEAL.

Exit Status:
  Returns success unless memfd_create fails or the variable cannot be bound.

Examples:
  // Create memfd with default name, store fd in MYFD
  L_builtin memfd MYFD

  // Create memfd named mydata, store fd in MYFD
  L_builtin memfd MYFD mydata

  // Use memfd as temporary in-RAM storage
  L_builtin memfd FD
  echo data >&$FD
  cat <&$FD
",
);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn memfd_subcommand(list: *mut WORD_LIST) -> c_int {
    let (var, name) = subcmd_getopts!(
        CMD,
        list,
        required: [VAR],
        optional: [NAME],
    );

    let name_ptr: *const c_char = match name {
        // The WORD_LIST word is already a NUL-terminated C string.
        Some(c) => c.as_ptr() as *const c_char,
        None => c"L_builtin_memfd".as_ptr(),
    };

    let flags: libc::c_uint = libc::MFD_CLOEXEC | libc::MFD_NOEXEC_SEAL;

    let fd = unsafe { libc::memfd_create(name_ptr, flags) };
    if fd < 0 {
        l_builtin_error!(b"memfd_create: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }
    let var_ptr = var.as_ptr().cast::<c_char>();
    let s = crate::shared::I64Str::new(fd as i64);
    if unsafe { bind_variable(var_ptr, s.as_ptr(), 0) }.is_null() {
        l_builtin_error!(b"cannot bind variable '", unsafe { var.as_bytes() }, b"'");
        unsafe { libc::close(fd) };
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}
