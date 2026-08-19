//! L_builtin `eventfd` subcommand group: create an eventfd(2) counter fd and
//! read/write data to it through `create`, `write`, `read`, and `apply` subcommands.
//!
//! An eventfd is an unsigned 64-bit counter accessed as a file descriptor:
//! `write` adds a value to the counter (the value is carried as 8 bytes in
//! native byte order); `read` returns the current counter and resets it to 0
//! (or, with the `-s`/EFD_SEMAPHORE flag, returns 1 and decrements by 1). The
//! resulting file descriptor is stored directly in a shell variable.
//!
//! Usage:
//!   `L_builtin eventfd create [-n] [-s] [-C] VAR [INITVAL]`
//!   `L_builtin eventfd write FD [VALUE]`
//!   `L_builtin eventfd read FD [VAR]`
//!   `L_builtin eventfd apply FD [VALUE]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::os::raw::c_int;

use cmdargs_derive::CmdArgs;

use crate::bash_api::{EXECUTION_FAILURE, WORD_LIST};
use crate::bprintln;
use crate::cmdargs::BashVar;
use crate::l_builtin_error;
use crate::shared::ensure_high_fd;
use crate::subcmd::{CmdDesc, CmdResult, SubCommandCallerArgs, SubcommandFn};

/// `L_builtin eventfd create [-n] [-s] [-C] VAR [INITVAL]`
#[derive(CmdArgs)]
struct EventfdCreateArgs {
    /// EFD_NONBLOCK.
    #[flag('n')]
    nonblock: bool,
    /// EFD_SEMAPHORE (read returns 1 instead of the counter).
    #[flag('s')]
    semaphore: bool,
    /// Do *not* set EFD_CLOEXEC (it is set by default).
    #[flag('C')]
    nocloexec: bool,
    /// Shell variable receiving the file descriptor.
    #[positional]
    var: BashVar,
    /// Initial counter value (default 0).
    #[optional(default = 0u32)]
    initval: u32,
}

pub unsafe fn eventfd_create_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EVENTFD_CREATE_CMD.enter();
    let args = EventfdCreateArgs::parse(list)?;
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
    let fd_int = ensure_high_fd(fd).map_err(|e| {
        l_builtin_error!(b"eventfd: fd dup failed: ", e);
        EXECUTION_FAILURE
    })?;
    args.var.set_int(fd_int as i64)?;
    Ok(())
}

/// `L_builtin eventfd write FD [VALUE]`
#[derive(CmdArgs)]
struct EventfdWriteArgs {
    /// File descriptor of the eventfd.
    #[positional]
    fd: c_int,
    /// Counter value to add (8-byte native-endian u64; default 1).
    #[optional(default = 1u64)]
    value: u64,
}

pub unsafe fn eventfd_write_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EVENTFD_WRITE_CMD.enter();
    let args = EventfdWriteArgs::parse(list)?;
    let buf = args.value.to_ne_bytes();
    let r = unsafe { libc::write(args.fd, buf.as_ptr() as *const libc::c_void, buf.len()) };
    if r < 0 {
        return Err(l_builtin_error!(
            b"eventfd write: ",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// `L_builtin eventfd read FD [VAR]`
#[derive(CmdArgs)]
struct EventfdReadArgs {
    /// File descriptor of the eventfd.
    #[positional]
    fd: c_int,
    /// Shell variable receiving the counter value (if omitted, print it).
    #[optional]
    var: Option<BashVar>,
}

pub unsafe fn eventfd_read_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EVENTFD_READ_CMD.enter();
    let args = EventfdReadArgs::parse(list)?;
    let mut buf = [0u8; 8];
    let r = unsafe { libc::read(args.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
    if r < 0 {
        return Err(l_builtin_error!(
            b"eventfd read: ",
            std::io::Error::last_os_error()
        ));
    }
    if r as usize != buf.len() {
        return Err(l_builtin_error!(b"eventfd read: short read"));
    }
    let value = u64::from_ne_bytes(buf);
    if let Some(var) = &args.var {
        var.set_int(value)?;
    } else {
        bprintln!(value);
    }
    Ok(())
}

/// `L_builtin eventfd apply FD [VALUE]`
#[derive(CmdArgs)]
struct EventfdApplyArgs {
    /// File descriptor of the eventfd.
    #[positional]
    fd: c_int,
    /// Counter value to set (default 0).
    #[optional(default = 0u64)]
    value: u64,
}

pub unsafe fn eventfd_apply_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EVENTFD_APPLY_CMD.enter();
    let args = EventfdApplyArgs::parse(list)?;
    // Consume any pending counter value so the fd is readable/reset, without
    // blocking on a zero counter.
    let mut pfd = libc::pollfd {
        fd: args.fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let r = unsafe { libc::poll(&mut pfd, 1, 0) };
    if r > 0 && (pfd.revents & libc::POLLIN) != 0 {
        let mut buf = [0u8; 8];
        unsafe { libc::read(args.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
    }
    if args.value > 0 {
        let val = args.value.to_ne_bytes();
        let w = unsafe { libc::write(args.fd, val.as_ptr() as *const libc::c_void, val.len()) };
        if w < 0 {
            return Err(l_builtin_error!(
                b"eventfd apply: ",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

const EVENTFD_CMD: CmdDesc = CmdDesc::new(
    c"eventfd",
    c"create [-n] [-s] [-C] VAR [INITVAL] | write FD [VALUE] | read FD [VAR] | apply FD [VALUE]",
    c"\
Create an eventfd(2) counting file descriptor and read/write its 64-bit counter.

Subcommands:
  create [-n] [-s] [-C] VAR [INITVAL]
                        Create an eventfd(2) and store its file descriptor in the
                        shell variable VAR. EFD_CLOEXEC is set by default; -C
                        clears it. INITVAL initializes the counter (default 0).
  write FD [VALUE]      Write VALUE (a 64-bit unsigned integer, default 1) into
                        the eventfd FD, adding it to the counter. VALUE is carried
                        as 8 bytes in native byte order.
  read FD [VAR]         Read the eventfd FD counter (an 8-byte native-endian
                         u64), resetting it to 0. If the counter was 0, a blocking
                         fd blocks; create the fd with -n (EFD_NONBLOCK) for
                         non-blocking operation (read then fails with EAGAIN).
                         Without EFD_SEMAPHORE read returns the full counter; with
                         it read returns 1 and decrements by 1. If VAR is given the
                         counter value is stored there, otherwise it is printed.
  apply FD [VALUE]      Set the counter to exactly VALUE (default 0). Consumes any
                         pending counter (read-and-reset) first, then writes VALUE.
                         Does not block even if the counter is 0.

The file descriptor is a real OS descriptor (as with the `close`, `lseek`,
`timerfd` and `signalfd` subcommands), so it can be polled through the `poll`/
`ppoll` subcommands and closed with `close`.

Exit Status:
  Returns success unless eventfd(2) fails or the variable cannot be bound.

Examples:
  L_builtin eventfd create -n ev          # counter=0, non-blocking, fd in $ev
  L_builtin eventfd write \"$ev\" 5        # counter += 5
  L_builtin eventfd read \"$ev\" val       # val=5, counter reset to 0
  L_builtin eventfd write \"$ev\" 1        # counter += 1
  L_builtin eventfd read \"$ev\"           # prints 1
  L_builtin close \"$ev\"
",
);

const EVENTFD_CREATE_CMD: CmdDesc = CmdDesc::new(
    c"create",
    c"create [-n] [-s] [-C] VAR [INITVAL]",
    c"\
Create an eventfd(2) and store its file descriptor in the shell variable VAR.

Options:
  -n   EFD_NONBLOCK (reads/writes do not block).
  -s   EFD_SEMAPHORE: read returns 1 instead of the counter value.
  -C   Do not set EFD_CLOEXEC (it is set by default).

INITVAL initializes the 64-bit counter (default 0).

Examples:
  L_builtin eventfd create ev
  L_builtin eventfd create -n ev 5
  L_builtin eventfd create -s -n ev
  L_builtin eventfd create -C ev 100
",
);

const EVENTFD_WRITE_CMD: CmdDesc = CmdDesc::new(
    c"write",
    c"write FD [VALUE]",
    c"\
Write VALUE into the eventfd FD, adding it to the 64-bit counter.

VALUE is a 64-bit unsigned integer carried as 8 bytes in native byte order
(default 1). A successful write adds VALUE to the counter. Writing a non-zero
value that would overflow the counter blocks (or, for a non-blocking fd, fails
with EAGAIN); writing the value 2**64-1 (0xFFFFFFFFFFFFFFFF) when the counter is
non-zero fails with EINVAL.

Examples:
  L_builtin eventfd write \"$ev\"
  L_builtin eventfd write \"$ev\" 42
",
);

const EVENTFD_READ_CMD: CmdDesc = CmdDesc::new(
    c"read",
    c"read FD [VAR]",
    c"\
Read the 64-bit counter from the eventfd FD, resetting it to 0.

Without EFD_SEMAPHORE, read returns the whole counter value and resets it to 0.
With EFD_SEMAPHORE (created via 'create -s'), read returns 1 and decrements the
counter by 1. If the counter is 0, a blocking fd blocks until it becomes
non-zero; create the fd with -n (EFD_NONBLOCK) for non-blocking operation (read
then fails with EAGAIN).

If VAR is given, the counter value is stored in the shell variable VAR as an
integer. Otherwise it is printed to stdout.

Examples:
  L_builtin eventfd read \"$ev\" val
  L_builtin eventfd read \"$ev\"
",
);

const EVENTFD_APPLY_CMD: CmdDesc = CmdDesc::new(
    c"apply",
    c"apply FD [VALUE]",
    c"\
Set the eventfd FD counter to exactly VALUE (default 0).

Unlike 'write' (which adds VALUE to the counter), 'apply' first consumes any
pending counter value (read-and-reset), then writes VALUE so the counter is set
to a known state. For VALUE=0 this simply drains the counter.

Non-blocking is not required: a zero timeout poll is used to check readability,
so 'apply' never blocks even on a blocking fd with a zero counter.

Examples:
  L_builtin eventfd apply \"$ev\" 5           # counter = 5
  L_builtin eventfd apply \"$ev\"             # counter = 0 (drain)
",
);

const EVENTFD_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("create", eventfd_create_subcommand),
    ("write", eventfd_write_subcommand),
    ("read", eventfd_read_subcommand),
    ("apply", eventfd_apply_subcommand),
];

const EVENTFD_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 4> =
    crate::intlookup!(&EVENTFD_SUBCOMMANDS);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn eventfd_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EVENTFD_CMD.enter();
    let args = SubCommandCallerArgs::parse(list)?;
    let caller = args.handler(EVENTFD_TABLE)?;
    caller.call()
}
