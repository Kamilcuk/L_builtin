//! L_builtin `fcntl` subcommand group: manipulate file descriptor properties
//! via fcntl(2).
//!
//! The flag name->value lookup tables (`l_open_flags`, `l_fd_flags`) live in C
//! (cmd_fcntl.c) and are exposed to Rust through bindgen-generated bindings.
//! The `parse_open_flags` / `parse_fd_flags` #[parse] converters traverse those
//! tables at parse time to turn comma-separated flag names into numeric
//! bitmasks.
//!
//! Usage:
//!   `L_builtin fcntl getfl [-v VAR] FD`
//!   `L_builtin fcntl setfl FD FLAGS`
//!   `L_builtin fcntl getfd [-v VAR] FD`
//!   `L_builtin fcntl setfd FD FLAGS`
//!   `L_builtin fcntl dup [-v VAR] [-c] FD [START]`

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CStr;
use std::os::raw::c_int;

use cmdargs_derive::CmdArgs;

use crate::bash_api::{l_fd_flags, l_flag_entry_t, l_open_flags, Cpnt, WORD_LIST};
use crate::bprintln;
use crate::cmdargs::BashVar;
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult, SubCommandCallerArgs, SubcommandFn};

/////////////////////////////////////////////////////////////////////////////
// Flag-table traversal helpers
///////////////////////////////////////////////////////////////////////////

/// Iterator over a sentinel-terminated `l_flag_entry_t` C table.  Each step
/// copies out one entry and advances the pointer; iteration stops at the
/// terminating entry whose `name` is null.
struct FlagTableIter {
    ptr: *const l_flag_entry_t,
}

impl FlagTableIter {
    /// # Safety
    /// `base` must point to a valid `l_flag_entry_t` array that is terminated
    /// by a null-`name` entry and that outlives the iterator.
    unsafe fn new(base: *const l_flag_entry_t) -> Self {
        Self { ptr: base }
    }
}

impl Iterator for FlagTableIter {
    type Item = l_flag_entry_t;
    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let entry = *self.ptr;
            if entry.name.is_null() {
                return None;
            }
            self.ptr = self.ptr.add(1);
            Some(entry)
        }
    }
}

/// Linear-scan a sentinel-terminated `l_flag_entry_t` C table for `name`,
/// returning the matching numeric flag value (or `None`).
unsafe fn lookup_flag_in_table(name: &str, base: *const l_flag_entry_t) -> Option<c_int> {
    for entry in unsafe { FlagTableIter::new(base) } {
        let entry_name = CStr::from_ptr(entry.name);
        if entry_name.to_bytes() == name.as_bytes() {
            return Some(entry.flag);
        }
    }
    None
}

/// Traverse a sentinel-terminated `l_flag_entry_t` C table, collecting the
/// names of all flags whose bit is set in `flags`.
/// Entries with a zero-value flag (e.g. `O_RDONLY == 0`) are skipped because
/// `flags & 0` is always true and would match spuriously.
unsafe fn format_flags(flags: c_int, base: *const l_flag_entry_t) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for entry in unsafe { FlagTableIter::new(base) } {
        if entry.flag != 0 && (flags & entry.flag) != 0 {
            if let Ok(n) = CStr::from_ptr(entry.name).to_str() {
                parts.push(n);
            }
        }
    }
    if parts.is_empty() {
        // No named flags matched; fall back to the raw value.
        format!("{}", flags)
    } else {
        parts.join(",")
    }
}

/// Print every entry of a sentinel-terminated `l_flag_entry_t` C table, one per
/// line, as `LABEL: NAME=VALUE`.  Unlike `format_flags`, zero-value entries
/// (e.g. `O_RDONLY == 0`) are listed too, since this enumerates the table.
unsafe fn list_flag_table(label: &str, base: *const l_flag_entry_t) {
    for entry in unsafe { FlagTableIter::new(base) } {
        let name = CStr::from_ptr(entry.name);
        let name = name.to_str().unwrap_or("<invalid>");
        bprintln!(format!("{}: {}={}", label, name, entry.flag));
    }
}

/// Parse a comma-separated list of open(2) flag names into a bitmask by
/// traversing the `l_open_flags` C array.  Used via `#[parse(parse_open_flags)]`
fn parse_open_flags(cptr: Cpnt) -> Result<c_int, String> {
    let s = unsafe { cptr.as_str() }.map_err(|e| e.to_string())?;
    let mut flags: c_int = 0;
    for tok in s.split(',') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let resolved = unsafe { lookup_flag_in_table(tok, l_open_flags.as_ptr()) };
        match resolved {
            Some(v) => flags |= v,
            None => return Err(format!("unknown open flag: {tok}")),
        }
    }
    Ok(flags)
}

/// Parse a comma-separated list of file-descriptor flag names into a bitmask
/// by traversing the `l_fd_flags` C array.  Used via `#[parse(parse_fd_flags)]`.
fn parse_fd_flags(cptr: Cpnt) -> Result<c_int, String> {
    let s = unsafe { cptr.as_str() }.map_err(|e| e.to_string())?;
    let mut flags: c_int = 0;
    for tok in s.split(',') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let resolved = unsafe { lookup_flag_in_table(tok, l_fd_flags.as_ptr()) };
        match resolved {
            Some(v) => flags |= v,
            None => return Err(format!("unknown fd flag: {tok}")),
        }
    }
    Ok(flags)
}

///////////////////////////////////////////////////////////////////////////
// Subcommand argument structs
///////////////////////////////////////////////////////////////////////////

/// `L_builtin fcntl getfl [-v VAR] FD`
#[derive(CmdArgs)]
struct FcntlGetflArgs {
    /// Shell variable receiving the file status flags (if omitted, decoded
    /// flag names are printed).
    #[opt('v')]
    var: Option<BashVar>,
    /// File descriptor to query.
    #[positional]
    fd: c_int,
}

/// `L_builtin fcntl setfl FD FLAGS`
#[derive(CmdArgs)]
struct FcntlSetflArgs {
    /// File descriptor to modify.
    #[positional]
    fd: c_int,
    /// Comma-separated open(2) flag names (e.g. `nonblock,append`).
    #[positional]
    #[parse(parse_open_flags)]
    flags: c_int,
}

/// `L_builtin fcntl getfd [-v VAR] FD`
#[derive(CmdArgs)]
struct FcntlGetfdArgs {
    /// Shell variable receiving the file descriptor flags (if omitted, decoded
    /// flag names are printed).
    #[opt('v')]
    var: Option<BashVar>,
    /// File descriptor to query.
    #[positional]
    fd: c_int,
}

/// `L_builtin fcntl setfd FD FLAGS`
#[derive(CmdArgs)]
struct FcntlSetfdArgs {
    /// File descriptor to modify.
    #[positional]
    fd: c_int,
    /// Comma-separated fd flag names (e.g. `cloexec` or empty string to clear).
    #[positional]
    #[parse(parse_fd_flags)]
    flags: c_int,
}

/// `L_builtin fcntl dup [-v VAR] [-c] FD [START]`
#[derive(CmdArgs)]
struct FcntlDupArgs {
    /// Shell variable receiving the new file descriptor (if omitted, it is
    /// printed).
    #[opt('v')]
    var: Option<BashVar>,
    /// Use F_DUPFD_CLOEXEC (close-on-exec) instead of F_DUPFD.
    #[flag('c')]
    cloexec: bool,
    /// File descriptor to duplicate.
    #[positional]
    fd: c_int,
    /// Minimum file descriptor to allocate (default 0).
    #[optional(default = 0)]
    start: c_int,
}

/// `L_builtin fcntl list [open|fd]`
#[derive(CmdArgs)]
struct FcntlListArgs {
    /// Which table to list: `open` (file status flags) or `fd` (descriptor
    /// flags). Omit to list both.
    #[optional]
    which: Option<&'static str>,
}

///////////////////////////////////////////////////////////////////////////
// Subcommand handlers
///////////////////////////////////////////////////////////////////////////

pub unsafe fn fcntl_getfl_subcommand(list: *mut WORD_LIST) -> CmdResult {
    FCNTL_GETFL_CMD.enter();
    let args = FcntlGetflArgs::parse(list)?;
    let flags = libc::fcntl(args.fd, libc::F_GETFL);
    if flags < 0 {
        return Err(l_builtin_error!(
            b"fcntl getfl: ",
            std::io::Error::last_os_error()
        ));
    }
    if let Some(var) = &args.var {
        var.set_int(flags)?;
    } else {
        let decoded = format!(
            "{} (0x{:x})",
            format_flags(flags, l_open_flags.as_ptr()),
            flags
        );
        bprintln!(decoded.as_bytes());
    }
    Ok(())
}

pub unsafe fn fcntl_setfl_subcommand(list: *mut WORD_LIST) -> CmdResult {
    FCNTL_SETFL_CMD.enter();
    let args = FcntlSetflArgs::parse(list)?;
    let r = libc::fcntl(args.fd, libc::F_SETFL, args.flags);
    if r < 0 {
        return Err(l_builtin_error!(
            b"fcntl setfl: ",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub unsafe fn fcntl_getfd_subcommand(list: *mut WORD_LIST) -> CmdResult {
    FCNTL_GETFD_CMD.enter();
    let args = FcntlGetfdArgs::parse(list)?;
    let flags = libc::fcntl(args.fd, libc::F_GETFD);
    if flags < 0 {
        return Err(l_builtin_error!(
            b"fcntl getfd: ",
            std::io::Error::last_os_error()
        ));
    }
    if let Some(var) = &args.var {
        var.set_int(flags)?;
    } else {
        let decoded = format!(
            "{} (0x{:x})",
            format_flags(flags, l_fd_flags.as_ptr()),
            flags
        );
        bprintln!(decoded.as_bytes());
    }
    Ok(())
}

pub unsafe fn fcntl_setfd_subcommand(list: *mut WORD_LIST) -> CmdResult {
    FCNTL_SETFD_CMD.enter();
    let args = FcntlSetfdArgs::parse(list)?;
    let r = libc::fcntl(args.fd, libc::F_SETFD, args.flags);
    if r < 0 {
        return Err(l_builtin_error!(
            b"fcntl setfd: ",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub unsafe fn fcntl_dup_subcommand(list: *mut WORD_LIST) -> CmdResult {
    FCNTL_DUP_CMD.enter();
    let args = FcntlDupArgs::parse(list)?;
    let cmd = if args.cloexec {
        libc::F_DUPFD_CLOEXEC
    } else {
        libc::F_DUPFD
    };
    let new_fd = libc::fcntl(args.fd, cmd, args.start);
    if new_fd < 0 {
        return Err(l_builtin_error!(
            b"fcntl dup: ",
            std::io::Error::last_os_error()
        ));
    }
    if let Some(var) = &args.var {
        var.set_int(new_fd)?;
    } else {
        bprintln!(new_fd);
    }
    Ok(())
}

pub unsafe fn fcntl_list_subcommand(list: *mut WORD_LIST) -> CmdResult {
    FCNTL_LIST_CMD.enter();
    let args = FcntlListArgs::parse(list)?;
    match args.which {
        None => {
            list_flag_table("open", l_open_flags.as_ptr());
            list_flag_table("fd", l_fd_flags.as_ptr());
        }
        Some("open") => list_flag_table("open", l_open_flags.as_ptr()),
        Some("fd") => list_flag_table("fd", l_fd_flags.as_ptr()),
        Some(other) => {
            return Err(l_builtin_error!(b"fcntl list: unknown table: ", other));
        }
    }
    Ok(())
}

///////////////////////////////////////////////////////////////////////////
// Command descriptors
///////////////////////////////////////////////////////////////////////////

const FCNTL_CMD: CmdDesc = CmdDesc::new(
    c"fcntl",
    c"getfl [-v VAR] FD | setfl FD FLAGS | getfd [-v VAR] FD | setfd FD FLAGS | dup [-v VAR] [-c] FD [START] | list [open|fd]",
    c"\
Manipulate file descriptor properties via fcntl(2).

Subcommands:
  getfl [-v VAR] FD        Get file status flags (F_GETFL).  Without -v the
                           decoded flag names (e.g. 'nonblock,append') and the
                           raw value are printed.
  setfl FD FLAGS           Set file status flags (F_SETFL).  FLAGS is a
                           comma-separated list of open(2) flag names, e.g.
                           'nonblock,append' or an empty string to clear all
                           status flags.
  getfd [-v VAR] FD        Get file descriptor flags (F_GETFD).
  setfd FD FLAGS           Set file descriptor flags (F_SETFD).  FLAGS is a
                           comma-separated list of fd flag names (e.g.
                           'cloexec'), or an empty string to clear.
  dup [-v VAR] [-c] FD [START]
                           Duplicate FD via F_DUPFD.  START is the minimum fd
                           (default 0).  With -c, F_DUPFD_CLOEXEC is used
                           instead (close-on-exec is set on the new fd).
  list [open|fd]           Enumerate the internal fcntl flag lookup tables.
                           `list open` prints the open(2) status flags,
                           `list fd` the descriptor flags, and plain `list`
                           prints both.  Each line is `TABLE: NAME=VALUE`.

The file descriptor can be any open fd (as with the `close`, `lseek`,
`timerfd` and `signalfd` subcommands).

Exit Status:
  Returns success unless fcntl(2) fails, an unknown flag name is given, or
  the variable cannot be bound.

Examples:
  L_builtin fcntl getfl 3
  L_builtin fcntl setfl 3 nonblock,append
  L_builtin fcntl setfl 3 ''      # clear all status flags
  L_builtin fcntl getfd 3
  L_builtin fcntl setfd 3 cloexec
  L_builtin fcntl dup 3
  L_builtin fcntl dup -c 3 256    # new fd >= 256 with close-on-exec
  L_builtin fcntl getfl -v result 3
  L_builtin fcntl list
  L_builtin fcntl list open
",
);

const FCNTL_GETFL_CMD: CmdDesc = CmdDesc::new(
    c"getfl",
    c"getfl [-v VAR] FD",
    c"\
Read the file status flags of FD via fcntl(2) F_GETFL.

Without -v, the decoded flag names and raw integer value are printed to stdout.
With -v VAR, the raw integer value is stored in the shell variable VAR.

Examples:
  L_builtin fcntl getfl 3
  L_builtin fcntl getfl -v flags 3
",
);

const FCNTL_SETFL_CMD: CmdDesc = CmdDesc::new(
    c"setfl",
    c"setfl FD FLAGS",
    c"\
Set the file status flags of FD via fcntl(2) F_SETFL.

FLAGS is a comma-separated list of open(2) flag names.  Any combination of
the following is accepted (availability depends on the platform):
  rdonly, wronly, rdwr, creat, excl, noctty, trunc, append, nonblock,
  ndelay, sync, dsync, rsync, async, direct, directory, nofollow, noatime,
  cloexec, path, tmpfile, largefile

An empty string clears all status flags.

Examples:
  L_builtin fcntl setfl 3 nonblock,append
  L_builtin fcntl setfl 3 ''
",
);

const FCNTL_GETFD_CMD: CmdDesc = CmdDesc::new(
    c"getfd",
    c"getfd [-v VAR] FD",
    c"\
Read the file descriptor flags of FD via fcntl(2) F_GETFD.

Without -v, the decoded flag names and raw integer value are printed.
With -v VAR, the raw integer value is stored in VAR.

Examples:
  L_builtin fcntl getfd 3
  L_builtin fcntl getfd -v flags 3
",
);

const FCNTL_SETFD_CMD: CmdDesc = CmdDesc::new(
    c"setfd",
    c"setfd FD FLAGS",
    c"\
Set the file descriptor flags of FD via fcntl(2) F_SETFD.

FLAGS is a comma-separated list of fd flag names.  Currently the only
supported flag is 'cloexec' (FD_CLOEXEC).  An empty string clears all
fd flags.

Examples:
  L_builtin fcntl setfd 3 cloexec
  L_builtin fcntl setfd 3 ''
",
);

const FCNTL_DUP_CMD: CmdDesc = CmdDesc::new(
    c"dup",
    c"dup [-v VAR] [-c] FD [START]",
    c"\
Duplicate FD via fcntl(2) F_DUPFD (or F_DUPFD_CLOEXEC with -c).

START specifies the minimum file descriptor to allocate (default 0).
Without -v, the new fd is printed; with -v VAR it is stored in VAR.

Options:
  -c   Use F_DUPFD_CLOEXEC instead of F_DUPFD (the new fd has close-on-exec
       set).
  -v   Store the result in VAR instead of printing.

Examples:
  L_builtin fcntl dup 3
  L_builtin fcntl dup -c 3
  L_builtin fcntl dup -v newfd 3 256
",
);

const FCNTL_LIST_CMD: CmdDesc = CmdDesc::new(
    c"list",
    c"list [open|fd]",
    c"\
Enumerate the internal fcntl flag lookup tables used to translate flag names
to numeric values.

Without an argument, both the open(2) status flag table and the file
descriptor flag table are printed.  With `open` or `fd`, only that table is
printed.  Each output line is `TABLE: NAME=VALUE`, where VALUE is the numeric
flag (a value may be 0, e.g. O_RDONLY).

Examples:
  L_builtin fcntl list
  L_builtin fcntl list open
  L_builtin fcntl list fd
",
);

///////////////////////////////////////////////////////////////////////////
// Dispatch table
///////////////////////////////////////////////////////////////////////////

const FCNTL_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("getfl", fcntl_getfl_subcommand),
    ("setfl", fcntl_setfl_subcommand),
    ("getfd", fcntl_getfd_subcommand),
    ("setfd", fcntl_setfd_subcommand),
    ("dup", fcntl_dup_subcommand),
    ("list", fcntl_list_subcommand),
];

const FCNTL_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 6> =
    crate::intlookup!(&FCNTL_SUBCOMMANDS);

///////////////////////////////////////////////////////////////////////////
// Entry point
///////////////////////////////////////////////////////////////////////////

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn fcntl_subcommand(list: *mut WORD_LIST) -> CmdResult {
    FCNTL_CMD.enter();
    let args = SubCommandCallerArgs::parse(list)?;
    let caller = args.handler(FCNTL_TABLE)?;
    caller.call()
}
