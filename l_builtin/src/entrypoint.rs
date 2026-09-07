//! Top-level L_builtin dispatch (Rust entry point called by bash)
//!
//! Bash calls `l_entrypoint` directly. This function reads the
//! subcommand name (first word), prints help on `-h`/`--help`, looks the
//! subcommand up in a dispatch table, and calls the appropriate handler with
//! the word list advanced past the subcommand name.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{
    l_enter_subcommand, this_cmd_name, Builtin, WordListView, BUILTIN_ENABLED, WORD_LIST,
};
use crate::cmdargs::BashVar;
use crate::cmdargs::WordListIterCpnt;
use crate::l_builtin_error;
use crate::shared::Memfd;
use crate::subcmd::{cmd_result_to_cint, CmdResult, SubcommandFn, SubcommandGuard};
use crate::{bprintln, l_builtin_usage_error};
use cmdargs_derive::CmdArgs;
use llib::nolock::SyncPtr;
use memmap2::MmapMut;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::raw::{c_char, c_int};

pub(crate) fn trim_trailing_newlines_in_zero_terminated_array_place(bytes: &mut [u8]) {
    debug_assert!(
        !bytes.is_empty() && bytes.last() == Some(&0),
        "array must be non-empty and null-terminated, found: {:?}",
        bytes
    );
    let orig_len = bytes.len() - 1;
    let mut i = orig_len;
    while i > 0 {
        if bytes[i - 1] == b'\n' {
            i -= 1;
            if i > 0 && bytes[i - 1] == b'\r' {
                i -= 1;
            }
        } else {
            break;
        }
    }
    if i < orig_len {
        bytes[i] = b'\0';
    }
}

struct RedirectStdout {
    saved_stdout: File,
}

impl RedirectStdout {
    pub fn new(target: &File) -> io::Result<Self> {
        flush_stdout_buffers();
        let saved_fd = unsafe { libc::fcntl(1, libc::F_DUPFD_CLOEXEC, 256) };
        if saved_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let res = unsafe { libc::dup2(target.as_raw_fd(), 1) };
        if res < 0 {
            unsafe {
                libc::close(saved_fd);
            }
            return Err(io::Error::last_os_error());
        }
        let saved_stdout = unsafe { File::from_raw_fd(saved_fd) };
        Ok(Self { saved_stdout })
    }
}

impl Drop for RedirectStdout {
    fn drop(&mut self) {
        flush_stdout_buffers();
        unsafe {
            libc::dup2(self.saved_stdout.as_raw_fd(), 1);
        }
    }
}

pub(crate) fn flush_stdout_buffers() {
    let _ = io::stdout().flush();
    unsafe { libc::fflush(std::ptr::null_mut()) };
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub(crate) fn capture_into_variable(
    var: BashVar,
    trimnewlines: bool,
    f: impl FnOnce() -> CmdResult,
) -> CmdResult {
    let mut memfd = Memfd::new().map_err(|_e| l_builtin_error!("cannot capture stdout"))?;
    let result;
    {
        let _guard = RedirectStdout::new(&memfd.file)
            .map_err(|e| l_builtin_error!("cannot redirect stdout: ", e))?;
        result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
            .map_err(|e| l_builtin_error!("captured command panicked: ", e))?;
    }
    memfd
        .file
        .write(b"\0")
        .map_err(|e| l_builtin_error!("couldn't write to memfd: ", e))?;
    let mut mmap = unsafe { MmapMut::map_mut(&memfd.file) }
        .map_err(|e| l_builtin_error!("could not mmap:", e))?;
    if trimnewlines {
        trim_trailing_newlines_in_zero_terminated_array_place(&mut mmap)
    }
    var.set(mmap.as_ptr().cast())?;
    result
}

macro_rules! c_wrap {
    ($f:ident) => {
        |list| {
            let ret = unsafe { $crate::bash_api::$f(list) };
            if ret == 0 {
                ::core::result::Result::Ok(())
            } else {
                ::core::result::Result::Err(ret)
            }
        }
    };
}

///////////////////////////////////////////////////////////////

static L_BUILTIN_DOC: &[SyncPtr<*const c_char>] = &llib::doc_array!(
    c"L_lib helper builtins.",
    c"",
    c"L_builtin [-v VAR] <subcommand> [options] [args]",
    c"",
    c"Options:",
    c"  -v VAR   Capture stdout of the subcommand into shell variable VAR",
    c"           (trailing newlines stripped, like $(...))",
    c"",
    c"Available subcommands:",
    c"",
    c"Network:",
    c"  listen       Create a listening TCP socket",
    c"  accept       Accept a network connection",
    c"  connect      Establish a TCP connection",
    c"  send         Send bytes over a socket",
    c"  recv         Receive bytes from a socket",
    c"  shutdown     Semi-close a network socket",
    c"",
    c"File descriptors:",
    c"  pipe         Create a pipe",
    c"  eventfd      Create an eventfd counter",
    c"  memfd        Create an anonymous memory-backed file",
    c"  timerfd      Create a timer as a file descriptor",
    c"  signalfd     Deliver signals as a file descriptor",
    c"  splice       Zero-copy move between two file descriptors",
    c"  lseek        Reposition file offset",
    c"  read         Read bytes from a file descriptor",
    c"  write        Write bytes to a file descriptor",
    c"  fcntl        Manipulate file descriptor properties",
    c"  flock        Acquire or release an advisory file lock",
    c"  close        Close a file descriptor",
    c"  epoll        Wait for file descriptor events (epoll)",
    c"  poll         Wait for file descriptors to become ready",
    #[cfg(feature = "ppoll")]
    c"  ppoll        Wait for FDs and unblock signals atomically",
    c"",
    c"Signals:",
    c"  sig          Inspect or modify the shell's signal mask",
    c"",
    c"Synchronization:",
    c"  barrier      Process-shared barrier synchronization",
    c"  mutex        Process-shared mutual-exclusion lock",
    c"  semaphore    Process-shared counting semaphore",
    c"  shm          Shared-memory variables backed by a database",
    c"",
    c"Variables:",
    c"  replace      In-place regex substitution on a bash variable",
    c"  sedvar       Run a sed script over a bash variable, in place",
    c"",
    c"Utilities:",
    c"  sleep        High-precision sub-second sleep",
    c"  core         Core utilities via Rust/uutils",
    c"  lua          Execute LuaJIT script",
    c"  ext          Builtins from bash examples/loadables/ directory",
    #[cfg(not(feature = "bash_lt_4_3"))]
    c"  run          Run a command. For use with -v VAR.",
    c"  version      Print build and bash version information",
    #[cfg(feature = "dev")]
    c"  unittest     Run internal unittests. Only on dev build.",
    c"",
    c"Use 'L_builtin <subcommand> --help' for more information.",
);

// Dispatch table: a plain map of subcommand name -> extern "C" handler.
// Ordered by functional group (network, fd ops, signals, sync, variables,
// utilities) for readability; the lookup table is hash-based so order has
// no runtime effect.
const SUBCOMMAND_ENTRIES: &[(&str, SubcommandFn)] = &[
    // Network
    ("listen", crate::listen::listen_subcommand),
    ("accept", crate::accept::accept_subcommand),
    ("connect", crate::connect::connect_subcommand),
    ("send", crate::send::send_subcommand),
    ("recv", crate::recv::recv_subcommand),
    ("shutdown", crate::shutdown::shutdown_subcommand),
    // FD ops
    ("pipe", crate::pipe::pipe_subcommand),
    ("eventfd", crate::eventfd::eventfd_subcommand),
    ("memfd", crate::memfd::memfd_subcommand),
    ("timerfd", crate::timerfd::timerfd_subcommand),
    ("signalfd", crate::signalfd::signalfd_subcommand),
    ("splice", crate::splice::splice_subcommand),
    ("lseek", crate::lseek::lseek_subcommand),
    ("read", crate::read::read_subcommand),
    ("write", crate::write::write_subcommand),
    ("fcntl", crate::cmd_fcntl::fcntl_subcommand),
    ("flock", crate::flock::flock_subcommand),
    ("close", crate::close::close_subcommand),
    ("epoll", crate::cmd_epoll::epoll_subcommand),
    ("poll", c_wrap!(l_poll_subcommand)),
    #[cfg(feature = "ppoll")]
    ("ppoll", c_wrap!(l_ppoll_subcommand)),
    // Signals
    ("sig", crate::cmd_sig::sig_subcommand),
    // Sync (incl. shared-memory variables)
    ("barrier", crate::cmd_barrier::barrier_subcommand),
    ("mutex", crate::cmd_mutex::mutex_subcommand),
    ("semaphore", crate::cmd_semaphore::semaphore_subcommand),
    ("shm", crate::cmd_shm::shm_subcommand),
    // Variables
    ("replace", crate::cmd_replace::replace_subcommand),
    ("sedvar", crate::cmd_sedvar::sedvar_subcommand),
    // Utilities
    ("sleep", crate::sleep::sleep_subcommand),
    ("core", crate::cmd_core::l_core_subcommand),
    ("lua", crate::cmd_lua::l_lua_subcommand),
    ("ext", c_wrap!(l_cmd_ext)),
    #[cfg(not(feature = "bash_lt_4_3"))]
    ("run", crate::cmd_run::l_run_subcommand),
    ("version", crate::cmd_version::version_subcommand),
    #[cfg(feature = "dev")]
    ("unittest", crate::unittest::l_unittest_subcommand),
];

#[no_mangle]
pub static mut L_builtin_struct: Builtin = Builtin {
    name: c"L_builtin".as_ptr().cast_mut(),
    function: Some(l_entrypoint),
    flags: BUILTIN_ENABLED as i32,
    long_doc: L_BUILTIN_DOC.as_ptr().cast_mut().cast(),
    short_doc: c"L_builtin <subcommand> [options] [args]"
        .as_ptr()
        .cast_mut(),
    handle: std::ptr::null_mut(),
};

#[no_mangle]
pub static L_builtin_impl: SyncPtr<*mut Builtin> = SyncPtr(&raw mut L_builtin_struct);

///////////////////////////////////////////////////////////////

const fn extract_first<const N: usize>(a: &[(&'static str, SubcommandFn)]) -> [&'static str; N] {
    let mut names = [""; N];
    let mut i = 0;
    while i < N {
        names[i] = a[i].0;
        i += 1;
    }
    names
}

/// Extract just the subcommand names for usage printing.
const SUBCOMMAND_NAMES: &[&str] =
    &extract_first::<{ SUBCOMMAND_ENTRIES.len() }>(SUBCOMMAND_ENTRIES);

const SUBCOMMAND_TABLE: llib::intlookup::U128::IntLookup<
    SubcommandFn,
    { SUBCOMMAND_ENTRIES.len() },
> = llib::intlookup!(&SUBCOMMAND_ENTRIES);

fn l_builtin_print_usage() {
    let cmd_name = this_cmd_name();
    // Print usage line
    bprintln!(
        cmd_name,
        b": usage: ",
        cmd_name,
        " [-v VAR] <subcommand> [options] [args]"
    );
    bprintln!(b"");
    bprintln!(b"Available subcommands:");
    // Print each subcommand name
    for name in SUBCOMMAND_NAMES {
        bprintln!(b"  ", name);
    }
}

#[derive(CmdArgs)]
struct EntrypointArgs {
    #[opt('v')]
    var: Option<BashVar>,
    #[rest]
    rest: WordListIterCpnt<'static>,
}

/// Top-level L_builtin entry point called by bash via L_builtin_struct.function
unsafe extern "C" fn l_entrypoint(list: *mut WORD_LIST) -> c_int {
    flush_stdout_buffers();
    let ret = cmd_result_to_cint(entrypoint(list));
    flush_stdout_buffers();
    ret
}

pub unsafe fn entrypoint(list: *mut WORD_LIST) -> CmdResult {
    l_enter_subcommand(
        std::ptr::null(),
        L_builtin_struct.short_doc.cast(),
        L_builtin_struct.long_doc.cast(),
    );
    let args = EntrypointArgs::parse(list)?;
    let mut list = args.rest;
    let first_word = match list.next() {
        Some(first_word) => first_word,
        None => return Err(l_builtin_usage_error!("missing subcommand")),
    };
    let first = unsafe { first_word.as_bytes() };
    // Find the subcommand for this name using intlookup's packed table.
    let subcommand = match SUBCOMMAND_TABLE.lookup(first) {
        Some(f) => f,
        None => return Err(l_builtin_usage_error!("unknown subcommand: ", first)),
    };
    // Construct the guard before dispatching so current_builtin's doc pointers
    // (set by the subcommand's CmdDesc::enter) are restored when l_entrypoint
    // returns.
    let _guard = SubcommandGuard::new();
    // Flush before the handler so buffered bash/C output cannot be reordered
    // against direct fd writes from Rust.
    if let Some(ret) = args.var {
        // -v VAR was provided: capture subcommand stdout into VAR
        capture_into_variable(ret, true, || unsafe { subcommand(list.as_ptr()) })
    } else {
        unsafe { subcommand(list.as_ptr()) }
    }
}
