//! L_builtin `epoll` subcommand group: scalable I/O event notification via epoll(7).
//!
//! `epoll` is the Linux-specific scalable readiness mechanism. Unlike `poll`/
//! `ppoll` (which scan a full fd set on every call), an epoll instance tracks its
//! watched fds in the kernel, so `epoll wait` reports only the ready fds in O(1)
//! per ready fd. The fds it produces compose with the rest of the fd subcommands
//! (`send`/`recv`/`accept`/`close`, `timerfd`, `eventfd`, `signalfd`).
//!
//! `epoll wait` reports readiness into a **sparse indexed array** keyed by the fd:
//! `ready[FD]` holds the decoded event tokens (e.g. `r`, `rw`). This matches the
//! format used by `poll`/`ppoll`, so a consumer loop can treat the two
//! interchangeably.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::os::raw::c_char;
use std::os::raw::c_int;

use cmdargs_derive::CmdArgs;

use crate::bash_api::{
    array_insert, arrayind_t, l_prepare_indexed_array, EXECUTION_FAILURE, WORD_LIST,
};
use crate::cmdargs::{BashVar, Duration};
use crate::l_builtin_error;
use crate::shared::ensure_high_fd;
use crate::subcmd::{CmdDesc, CmdResult, SubCommandCallerArgs, SubcommandFn};

/// Translate an event-token string (`r`/`w`/`p`/`t`) into an `epoll_event.events`
/// bitmask. Empty or unknown tokens default to `EPOLLIN`. Mirrors `poll`'s token
/// notation so the two can be used uniformly.
fn parse_events(s: Option<&str>) -> u32 {
    let s = s.unwrap_or("r");
    if s.is_empty() {
        return libc::EPOLLIN as u32;
    }
    let mut ev: u32 = 0;
    for ch in s.bytes() {
        match ch {
            b'r' => ev |= libc::EPOLLIN as u32,
            b'w' => ev |= libc::EPOLLOUT as u32,
            b'p' => ev |= libc::EPOLLPRI as u32,
            b't' => ev |= libc::EPOLLET as u32,
            _ => {}
        }
    }
    if ev == 0 {
        libc::EPOLLIN as u32
    } else {
        ev
    }
}

/// Decode an `epoll_event.events` bitmask back into event tokens for array output.
fn format_events(ev: u32) -> String {
    let mut s = String::new();
    if ev & (libc::EPOLLIN as u32) != 0 {
        s.push('r');
    }
    if ev & (libc::EPOLLOUT as u32) != 0 {
        s.push('w');
    }
    if ev & (libc::EPOLLPRI as u32) != 0 {
        s.push('p');
    }
    if ev & (libc::EPOLLHUP as u32) != 0 {
        s.push('h');
    }
    if ev & (libc::EPOLLERR as u32) != 0 {
        s.push('e');
    }
    if ev & (libc::EPOLLET as u32) != 0 {
        s.push('t');
    }
    if s.is_empty() {
        s.push_str("(none)");
    }
    s
}

fn duration_to_ms(d: Duration) -> c_int {
    let ms = (d.as_secs_f64() * 1000.0).round() as c_int;
    ms.max(0)
}

/// Maximum events collected per `epoll_wait` call. Ready fds beyond this are
/// drained in extra non-blocking calls so none are dropped.
const EPOLL_MAX_EVENTS: c_int = 256;

/// Populate a sparse indexed bash array `var` with `(fd, event-tokens)` pairs.
/// The fd is the array index, so `var[FD]` yields the event string for that fd.
/// Existing indexed arrays are flushed first. A scalar or associative variable is
/// automatically converted into an indexed array (and the att_invisible flag that
/// `local` sets on unset locals is cleared) so no prior `local -a` is needed.
unsafe fn populate_ready_array(var: &BashVar, ready: &[(c_int, String)]) -> CmdResult {
    let name = var.as_ptr();
    let a = l_prepare_indexed_array(name);
    if a.is_null() {
        return Err(l_builtin_error!(b"epoll wait: cannot create array ", name));
    }
    for (fd, ev) in ready {
        let mut buf = ev.as_bytes().to_vec();
        buf.push(0);
        array_insert(a, *fd as arrayind_t, buf.as_ptr() as *mut c_char);
    }
    Ok(())
}

/// `L_builtin epoll create FD_VAR`
#[derive(CmdArgs)]
struct EpollCreateArgs {
    /// Shell variable receiving the epoll file descriptor.
    #[positional]
    fd_var: BashVar,
}

pub unsafe fn epoll_create_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EPOLL_CREATE_CMD.enter();
    let args = EpollCreateArgs::parse(list)?;
    let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
    if fd < 0 {
        return Err(l_builtin_error!(
            b"epoll create: ",
            std::io::Error::last_os_error()
        ));
    }
    let fd_int = ensure_high_fd(fd).map_err(|e| {
        l_builtin_error!(b"epoll create: fd dup failed: ", e);
        EXECUTION_FAILURE
    })?;
    args.fd_var.set_int(fd_int as i64)?;
    Ok(())
}

/// `L_builtin epoll add EPOLLFD FD [events]` and `mod EPOLLFD FD [events]`
#[derive(CmdArgs)]
struct EpollCtlArgs {
    /// The epoll instance file descriptor.
    #[positional]
    epfd: c_int,
    /// The file descriptor to register/modify.
    #[positional]
    fd: c_int,
    /// Event tokens (default `r`): r/EPOLLIN, w/EPOLLOUT, p/EPOLLPRI, t/EPOLLET.
    #[optional]
    events: Option<&'static str>,
}

pub unsafe fn epoll_add_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EPOLL_ADD_CMD.enter();
    let args = EpollCtlArgs::parse(list)?;
    let mut ev: libc::epoll_event = unsafe { std::mem::zeroed() };
    ev.events = parse_events(args.events);
    ev.u64 = args.fd as u64;
    if unsafe { libc::epoll_ctl(args.epfd, libc::EPOLL_CTL_ADD, args.fd, &mut ev) } < 0 {
        return Err(l_builtin_error!(
            b"epoll add: ",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub unsafe fn epoll_mod_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EPOLL_MOD_CMD.enter();
    let args = EpollCtlArgs::parse(list)?;
    let mut ev: libc::epoll_event = unsafe { std::mem::zeroed() };
    ev.events = parse_events(args.events);
    ev.u64 = args.fd as u64;
    if unsafe { libc::epoll_ctl(args.epfd, libc::EPOLL_CTL_MOD, args.fd, &mut ev) } < 0 {
        return Err(l_builtin_error!(
            b"epoll mod: ",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// `L_builtin epoll del EPOLLFD FD` - strict: takes only two positionals so a
/// stray argument is reported as "too many arguments" rather than silently
/// ignored.
#[derive(CmdArgs)]
struct EpollDelArgs {
    #[positional]
    epfd: c_int,
    #[positional]
    fd: c_int,
}

pub unsafe fn epoll_del_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EPOLL_DEL_CMD.enter();
    let args = EpollDelArgs::parse(list)?;
    if unsafe {
        libc::epoll_ctl(
            args.epfd,
            libc::EPOLL_CTL_DEL,
            args.fd,
            std::ptr::null_mut(),
        )
    } < 0
    {
        return Err(l_builtin_error!(
            b"epoll del: ",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

const EPOLL_CMD: CmdDesc = CmdDesc::new(
    c"epoll",
     c"create FD_VAR | add EPOLLFD FD [r|w|p|t] | mod EPOLLFD FD [r|w|p|t] | del EPOLLFD FD | wait [-t SECS] [-v ARR] EPOLLFD",
    c"\
Scalable I/O event notification via epoll(7), the Linux-specific readiness
mechanism that scales O(1) per ready fd (unlike poll/ppoll, which scan the full
fd set). The fds it produces compose with the rest of the fd subcommands
(send/recv/accept/close, timerfd, eventfd, signalfd).

Subcommands:
  create FD_VAR             Create an epoll instance and store its fd in FD_VAR.
                              The fd is close-on-exec.
  add EPOLLFD FD [events]   Register FD on EPOLLFD (EPOLL_CTL_ADD). EVENTS defaults
                             to r; see EVENTS tokens below.
  mod EPOLLFD FD [events]   Change FD's event mask on EPOLLFD (EPOLL_CTL_MOD).
  del EPOLLFD FD            Stop watching FD on EPOLLFD (EPOLL_CTL_DEL).
  wait [-t SECS] [-v ARR] EPOLLFD
                            Block until fds on EPOLLFD are ready. With -v ARR, every
                             ready fd is stored as a sparse array entry ARR[FD]=
                             events. -t SECS sets a timeout (durations like
                             '1.5', '500ms' accepted); without -t it blocks
                             forever.

EVENTS tokens (add/mod):  r EPOLLIN | w EPOLLOUT | p EPOLLPRI | t EPOLLET
                          (edge-triggered; combine, e.g. 'rw', 'rt').
Readiness tokens (wait -> ARR[FD]): r w p | h EPOLLHUP | e EPOLLERR | t EPOLLET

The fd is just an integer bash variable; there is no handle registry.

Examples:
   # Watch a pipe and a timerfd, then decode readiness per fd
   L_builtin epoll create ep
   L_builtin pipe in
   L_builtin timerfd t 500ms
   printf 'hello' >&\"${in[1]}\" &
   L_builtin epoll add $ep ${in[0]} r
   L_builtin epoll add $ep $t r
   L_builtin epoll wait -v ready $ep
   for fd in \"${!ready[@]}\"; do
       rev=${ready[fd]}
       echo \"fd $fd ready: $rev\"
       [[ $rev == *r* ]] && echo \"  fd $fd is readable\"
       [[ $rev == *w* ]] && echo \"  fd $fd is writable\"
       [[ $rev == *h* ]] && echo \"  fd $fd hung up\"
       [[ $rev == *e* ]] && echo \"  fd $fd errored\"
   done
   exec {in[0]}<&- {in[1]}>&- {t}<&- {ep}<&-
",
);

const EPOLL_CREATE_CMD: CmdDesc = CmdDesc::new(
    c"create",
    c"create FD_VAR",
    c"\
Create an epoll instance (epoll_create1(2)) and store its file descriptor in
the shell variable FD_VAR. The fd is close-on-exec (EPOLL_CLOEXEC). The fd
becomes readable (POLLIN) when any watched fd is ready, so it can be polled
together with other fds (see poll/ppoll).

Examples:
   L_builtin epoll create ep
",
);

const EPOLL_ADD_CMD: CmdDesc = CmdDesc::new(
    c"add",
    c"add EPOLLFD FD [events]",
    c"\
Register FD on the epoll instance EPOLLFD via epoll_ctl(2) EPOLL_CTL_ADD.

EVENTS defaults to 'r' (EPOLLIN). Tokens: r/EPOLLIN, w/EPOLLOUT, p/EPOLLPRI,
t/EPOLLET (edge-triggered). Combine them, e.g. 'rw' or 'rt'.

Examples:
   L_builtin epoll add $ep 3          # watch fd 3 for reads (default)
   L_builtin epoll add $ep 3 w        # watch fd 3 for writes
   L_builtin epoll add $ep 3 rt       # edge-triggered read on fd 3
",
);

const EPOLL_MOD_CMD: CmdDesc = CmdDesc::new(
    c"mod",
    c"mod EPOLLFD FD [events]",
    c"\
Change the event mask of FD on EPOLLFD via epoll_ctl(2) EPOLL_CTL_MOD.

EVENTS defaults to 'r'. See `add` for the token meaning.

Examples:
   L_builtin epoll mod $ep 3 rw       # now also watch fd 3 for writes
",
);

const EPOLL_DEL_CMD: CmdDesc = CmdDesc::new(
    c"del",
    c"del EPOLLFD FD",
    c"\
Stop watching FD on EPOLLFD via epoll_ctl(2) EPOLL_CTL_DEL. Takes no event
argument; a trailing token is rejected as 'too many arguments'.

Examples:
   L_builtin epoll del $ep 3
",
);

const EPOLL_WAIT_CMD: CmdDesc = CmdDesc::new(
    c"wait",
    c"wait [-t SECS] [-v ARR] EPOLLFD",
    c"\
Block until one or more fds registered on EPOLLFD are ready (epoll_wait(2)).

With -v ARR, every ready fd is stored as a sparse array entry ARR[FD]=events,
where events is a token string (r/w/p/h/e/t). The fd is the array index, so
'${!ARR[@]}' lists the ready fds and ARR[$fd] gives their readiness.

-t SECS sets a timeout (duration strings like '1.5', '500ms' are accepted);
without -t it blocks forever. Returns success on readiness or timeout (0 ready
fds), failure only on error. Without -v no array is populated.

Examples:
   L_builtin epoll wait -v ready $ep
   for fd in \"${!ready[@]}\"; do echo \"fd $fd: ${ready[$fd]}\"; done
   L_builtin epoll wait -t 2.5 -v r $ep   # timeout after 2.5s
",
);

const EPOLL_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("create", epoll_create_subcommand),
    ("add", epoll_add_subcommand),
    ("mod", epoll_mod_subcommand),
    ("del", epoll_del_subcommand),
    ("wait", epoll_wait_subcommand),
];

const EPOLL_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 5> =
    crate::intlookup!(&EPOLL_SUBCOMMANDS);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn epoll_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EPOLL_CMD.enter();
    let args = SubCommandCallerArgs::parse(list)?;
    let caller = args.handler(EPOLL_TABLE)?;
    caller.call()
}

/// `L_builtin epoll wait [-t SECS] [-v ARR] EPOLLFD`
#[derive(CmdArgs)]
struct EpollWaitArgs {
    /// Timeout as a duration string or seconds; default blocks forever.
    #[opt('t')]
    timeout: Option<Duration>,
    /// Sparse indexed array (keyed by fd) receiving event tokens per ready fd.
    #[opt('v')]
    var: Option<BashVar>,
    /// The epoll instance file descriptor.
    #[positional]
    epfd: c_int,
}

pub unsafe fn epoll_wait_subcommand(list: *mut WORD_LIST) -> CmdResult {
    EPOLL_WAIT_CMD.enter();
    let args = EpollWaitArgs::parse(list)?;
    let timeout_ms = match &args.timeout {
        Some(d) => duration_to_ms(*d),
        None => -1,
    };
    let mut events: Vec<libc::epoll_event> =
        vec![unsafe { std::mem::zeroed() }; EPOLL_MAX_EVENTS as usize];
    let maxev = EPOLL_MAX_EVENTS;
    let mut ready: Vec<(c_int, String)> = Vec::new();
    let mut n = unsafe { libc::epoll_wait(args.epfd, events.as_mut_ptr(), maxev, timeout_ms) };
    if n < 0 {
        return Err(l_builtin_error!(
            b"epoll_wait: ",
            std::io::Error::last_os_error()
        ));
    }
    loop {
        for i in 0..n as usize {
            ready.push((events[i].u64 as c_int, format_events(events[i].events)));
        }
        if n < maxev {
            break;
        }
        // More events may be pending: drain with a non-blocking call.
        n = unsafe { libc::epoll_wait(args.epfd, events.as_mut_ptr(), maxev, 0) };
        if n <= 0 {
            break;
        }
    }
    if let Some(var) = &args.var {
        populate_ready_array(var, &ready)?;
    }
    Ok(())
}
