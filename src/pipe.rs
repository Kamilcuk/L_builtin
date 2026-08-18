//! L_builtin `pipe` subcommand: create a pipe.
//!
//! Usage: `L_builtin pipe ARRAY`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, EXECUTION_SUCCESS, WORD_LIST};
use crate::l_builtin_error;
use crate::subcmd::CmdDesc;
use cmdargs_derive::CmdArgs;
use std::ffi::c_char;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"pipe",
    c"ARRAY",
    c"\
Create a new pipe and store the file descriptors in the indexed
array ARRAY. ARRAY[0] is the read end, ARRAY[1] is the write end.

Exit Status:
Returns success unless the pipe cannot be created or ARRAY is invalid.
",
);

/// L_builtin `pipe ARRAY`
#[derive(CmdArgs)]
struct PipeArgs {
    #[positional]
    array_name: *const c_char,
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn pipe_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();

    let args = match PipeArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };

    // Create pipe
    let mut fds: [c_int; 2] = [0, 0];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } < 0 {
        l_builtin_error!(b"pipe: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }

    // Check if variable exists and is an array
    let mut var = unsafe { crate::bash_api::find_variable(args.array_name) };
    if !var.is_null() {
        let is_array = unsafe { crate::bash_api::l_array_p(var) };
        if is_array == 0 {
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            l_builtin_error!(b"not an indexed array");
            return EXECUTION_FAILURE;
        }
    }

    // Create array variable if it doesn't exist
    if var.is_null() {
        var = unsafe { crate::bash_api::make_new_array_variable(args.array_name) };
        if var.is_null() {
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            l_builtin_error!(b"cannot create array variable");
            return EXECUTION_FAILURE;
        }
    }

    let array = unsafe { crate::bash_api::l_array_cell(var) };
    unsafe { crate::bash_api::array_flush(array) };

    // Insert read fd (index 0)
    let read_fd = crate::shared::I64Str::new(fds[0] as i64);
    unsafe { crate::bash_api::array_insert(array, 0, read_fd.as_ptr().cast_mut()) };

    // Insert write fd (index 1)
    let write_fd = crate::shared::I64Str::new(fds[1] as i64);
    unsafe { crate::bash_api::array_insert(array, 1, write_fd.as_ptr().cast_mut()) };

    EXECUTION_SUCCESS
}
