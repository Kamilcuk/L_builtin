//! L_builtin `pipe` subcommand: create a pipe.
//!
//! Usage: `L_builtin pipe ARRAY`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::intstr::ToIntStr;
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::c_int;

struct PipeGuard([c_int; 2]);

impl Drop for PipeGuard {
    fn drop(&mut self) {
        unsafe {
            if self.0[0] >= 0 {
                libc::close(self.0[0]);
            }
            if self.0[1] >= 0 {
                libc::close(self.0[1]);
            }
        }
    }
}

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
    array_name: BashVar,
}

/// # Safety
///
/// Safe when called from bash with valid WORD_LIST pointer.
pub unsafe fn pipe_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = PipeArgs::parse(list)?;
    // Create pipe
    let mut fds = PipeGuard([0, 0]);
    if unsafe { libc::pipe(fds.0.as_mut_ptr()) } < 0 {
        l_builtin_error!(b"pipe: ", std::io::Error::last_os_error());
        return Err(EXECUTION_FAILURE);
    }
    // Check if variable exists and is an array
    let mut var = unsafe { crate::bash_api::find_variable(args.array_name.as_ptr()) };
    if !var.is_null() {
        let is_array = unsafe { crate::bash_api::l_array_p(var) };
        if is_array == 0 {
            l_builtin_error!(b"not an indexed array", args.array_name.as_ptr());
            return Err(EXECUTION_FAILURE);
        }
    }

    // Create array variable if it doesn't exist
    if var.is_null() {
        var = unsafe { crate::bash_api::make_new_array_variable(args.array_name.as_ptr()) };
        if var.is_null() {
            l_builtin_error!(b"cannot create array variable", args.array_name.as_ptr());
            return Err(EXECUTION_FAILURE);
        }
    }

    let array = unsafe { crate::bash_api::l_array_cell(var) };
    unsafe { crate::bash_api::array_flush(array) };

    // Insert read fd (index 0)
    let read_fd: i64 = fds.0[0] as i64;
    unsafe { crate::bash_api::array_insert(array, 0, read_fd.to_intstr().as_ptr().cast_mut()) };

    // Insert write fd (index 1)
    let write_fd: i64 = fds.0[1] as i64;
    unsafe { crate::bash_api::array_insert(array, 1, write_fd.to_intstr().as_ptr().cast_mut()) };

    // Prevent guard from closing the file descriptors since ownership is handed off to bash array
    std::mem::forget(fds);

    Ok(())
}
