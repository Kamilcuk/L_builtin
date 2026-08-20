//! L_builtin `read` subcommand: read bytes from a file descriptor.
//!
//! Usage: `L_builtin read [-f format] [-v READ_VAR] [-n] [-i] FD SIZE`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{EXECUTION_FAILURE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::io_common::{Format, hex_encode, parse_format};
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;
use std::os::raw::c_int;

const CMD: CmdDesc = CmdDesc::new(
    c"read",
    c"[-f format] [-v READ_VAR] [-n] [-i] FD SIZE",
    c"\
Read up to SIZE bytes from the file descriptor FD via read(2). Works on any fd
(pipes, files, sockets, etc.), not just sockets.

Supported formats (-f):
  raw   Store raw bytes directly into READ_VAR (null-byte unsafe) (default)
  hex   Store read bytes as hexadecimal string into READ_VAR (null-byte safe)

If -n is provided, the fd is temporarily set non-blocking for this call.
If no data is currently available, it returns success with an empty value.

If -i is provided, the read does not retry on signal interruption (EINTR)
and instead fails. By default, read retries on EINTR.

Exit Status:
Returns success unless read fails or variable binding fails.
",
);

#[derive(CmdArgs)]
struct ReadArgs {
    #[flag('n')]
    non_blocking: bool,
    #[flag('i')]
    interruptible: bool,
    #[opt('f', default=Format::Raw)]
    #[parse(parse_format)]
    format: Format,
    #[opt('v')]
    var: Option<BashVar>,
    #[positional]
    fd: c_int,
    #[positional]
    size: usize,
}

/// RAII guard that temporarily sets O_NONBLOCK on an fd and restores the
/// original file status flags when dropped. A value of -1 means no changes
/// were made (non-blocking mode not requested).
struct NonblockGuard {
    fd: c_int,
    old_flags: c_int,
}

impl NonblockGuard {
    /// # Safety
    ///
    /// Caller must ensure `fd` is valid. Returns `Ok(guard)` on success,
    /// or an `Err` with the io error if fcntl fails.
    unsafe fn set(fd: c_int) -> Result<Self, std::io::Error> {
        let old_flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if old_flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, old_flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { fd, old_flags })
    }
}

impl Drop for NonblockGuard {
    fn drop(&mut self) {
        if self.old_flags >= 0 {
            unsafe { libc::fcntl(self.fd, libc::F_SETFL, self.old_flags) };
        }
    }
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn read_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = ReadArgs::parse(list)?;
    let mut buf = vec![0u8; args.size + 1];

    let _guard = if args.non_blocking {
        match NonblockGuard::set(args.fd) {
            Ok(g) => Some(g),
            Err(e) => return Err(l_builtin_error!(b"read: fcntl: ", e)),
        }
    } else {
        None
    };

    let received;
    loop {
        let result = unsafe {
            libc::read(
                args.fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                args.size,
            )
        };
        if result < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                if args.interruptible {
                    l_builtin_error!(b"read: Interrupted system call");
                    return Err(EXECUTION_FAILURE);
                }
                continue;
            }
            if args.non_blocking
                && (err.raw_os_error() == Some(libc::EAGAIN)
                    || err.raw_os_error() == Some(libc::EWOULDBLOCK))
            {
                received = 0;
                break;
            }
            return Err(l_builtin_error!(b"read failed: ", err));
        }
        received = result as usize;
        break;
    }

    buf[received] = 0;
    if let Some(var) = args.var {
        match args.format {
            Format::Hex => {
                let out = hex_encode(&buf[..received]);
                var.set(out.as_ptr().cast())?;
            }
            Format::Raw => {
                var.set(buf.as_ptr().cast())?;
            }
        }
    }
    Ok(())
}
