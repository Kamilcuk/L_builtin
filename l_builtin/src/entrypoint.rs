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
    l_enter_subcommand, this_cmd_name, L_builtin_struct, WordListView, WORD_LIST,
};
use crate::cmdargs::BashVar;
use crate::cmdargs::WordListIterCpnt;
use crate::l_builtin_error;
use crate::shared::Memfd;
use crate::subcmd::{cmd_result_to_cint, CmdResult, SubcommandFn, SubcommandGuard};
use crate::{bprintln, l_builtin_usage_error};
use cmdargs_derive::CmdArgs;
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
    _ename: &str,
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

// Dispatch table: a plain map of subcommand name -> extern "C" handler.
const SUBCOMMAND_ENTRIES: &[(&str, SubcommandFn)] = &[
    ("lseek", crate::lseek::lseek_subcommand),
    ("poll", c_wrap!(l_poll_subcommand)),
    #[cfg(feature = "ppoll")]
    ("ppoll", c_wrap!(l_ppoll_subcommand)),
    ("sigmask", c_wrap!(l_sigmask_subcommand)),
    ("sigunmask", c_wrap!(l_sigunmask_subcommand)),
    ("pipe", crate::pipe::pipe_subcommand),
    ("listen", crate::listen::listen_subcommand),
    ("accept", crate::accept::accept_subcommand),
    ("connect", crate::connect::connect_subcommand),
    ("shutdown", crate::shutdown::shutdown_subcommand),
    ("send", crate::send::send_subcommand),
    ("recv", crate::recv::recv_subcommand),
    ("write", crate::write::write_subcommand),
    ("read", crate::read::read_subcommand),
    ("sleep", crate::sleep::sleep_subcommand),
    ("core", crate::cmd_core::l_core_subcommand),
    ("lua", crate::cmd_lua::l_lua_subcommand),
    ("ext", c_wrap!(l_cmd_ext)),
    ("eventfd", crate::eventfd::eventfd_subcommand),
    ("memfd", crate::memfd::memfd_subcommand),
    ("timerfd", crate::timerfd::timerfd_subcommand),
    ("signalfd", crate::signalfd::signalfd_subcommand),
    ("flock", crate::flock::flock_subcommand),
    ("close", crate::close::close_subcommand),
    ("splice", crate::splice::splice_subcommand),
    ("shm", crate::cmd_shm::shm_subcommand),
    ("fcntl", crate::cmd_fcntl::fcntl_subcommand),
    ("epoll", crate::cmd_epoll::epoll_subcommand),
    ("barrier", crate::cmd_barrier::barrier_subcommand),
    ("mutex", crate::cmd_mutex::mutex_subcommand),
    ("semaphore", crate::cmd_semaphore::semaphore_subcommand),
    ("replace", crate::cmd_replace::replace_subcommand),
    ("sedvar", crate::cmd_sedvar::sedvar_subcommand),
    #[cfg(not(feature = "bash_lt_4_3"))]
    ("run", crate::cmd_run::l_run_subcommand),
    #[cfg(not(feature = "bash_lt_4_3"))]
    ("capture", crate::cmd_run::l_run_subcommand),
    #[cfg(feature = "dev")]
    ("unittest", crate::unittest::l_unittest_subcommand),
    ("version", crate::cmd_version::version_subcommand),
];

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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn l_entrypoint(list: *mut WORD_LIST) -> c_int {
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
        capture_into_variable("L_builtin", ret, true, || unsafe {
            subcommand(list.as_ptr())
        })
    } else {
        unsafe { subcommand(list.as_ptr()) }
    }
}
