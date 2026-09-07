//! L_builtin `sig` subcommand group: inspect and manipulate the shell's
//! process signal mask.
//!
//! Subcommands:
//!   `sig list [-v VAR]`             Print the signals currently blocked in the
//!                                   shell's signal mask, one per line. With
//!                                   `-v VAR`, write the names into the indexed
//!                                   array VAR instead.
//!   `sig block sigspec...`          Block the listed signals in the calling
//!                                   shell process. Persistent shell state:
//!                                   subsequent commands inherit the modified
//!                                   mask.
//!   `sig unblock sigspec...`        Inverse of `sig block`.
//!   `sig run sigspec... -- cmd`     Run `cmd` with the listed signals unblocked
//!                                   in the *child* only. The caller's mask is
//!                                   not permanently modified; the signal
//!                                   modification is restored via the C helper
//!                                   `l_run_with_unblocked`.
//!
//! Signal specifications follow Bash's existing conventions (see
//! `decode_signal` in trap.c): names accept an optional `SIG` prefix,
//! comparisons are case-insensitive, and the special token `all` matches every
//! signal. Numeric signal numbers are also accepted.
//!
//! This builtin manages the *mask* only. It does not alter trap dispositions
//! or signal handlers.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::os::raw::{c_char, c_int};

use cmdargs_derive::CmdArgs;

use crate::bash_api::{
    array_insert, arrayind_t, decode_signal, l_prepare_indexed_array, l_run_with_unblocked,
    sh_invalidsig, signal_name, ARRAY, NSIG, DSIG_NOCASE, DSIG_SIGPREFIX, EXECUTION_FAILURE,
    WORD_LIST,
};
use crate::cmdargs::BashVar;
use crate::subcmd::{CmdDesc, CmdResult, SubCommandCallerArgs, SubcommandFn};
use crate::{bprintln, l_builtin_error, l_builtin_usage_error};

/// Sentinel value returned by [`decode_sigspec`] for the case-insensitive
/// token `all`. Larger than any real signal number so callers can branch on
/// it.
const ALL_SIG: c_int = c_int::MAX;

/// Decode a NUL-terminated bash word as a signal specification. Recognises
/// the literal token "all" (case-insensitive) and otherwise defers to bash's
/// `decode_signal`. Returns the resulting signal number, or `None` if the
/// spec is invalid (in which case `sh_invalidsig` has already been called).
unsafe fn decode_sigspec(cptr: *mut c_char) -> Option<c_int> {
    if cptr.is_null() {
        return None;
    }
    let bytes = std::ffi::CStr::from_ptr(cptr).to_bytes();
    if bytes.eq_ignore_ascii_case(b"all") {
        return Some(ALL_SIG);
    }
    let sig = decode_signal(cptr.cast(), DSIG_NOCASE | DSIG_SIGPREFIX);
    if sig < 0 {
        sh_invalidsig(cptr.cast());
        return None;
    }
    Some(sig)
}

/// Add a signal spec to `set`. Recognises "all" via the [`decode_sigspec`]
/// sentinel. Returns false on invalid specs (with `sh_invalidsig` already
/// having been called by `decode_sigspec`).
unsafe fn sigspec_add(set: *mut libc::sigset_t, cptr: *mut c_char) -> bool {
    match decode_sigspec(cptr) {
        Some(ALL_SIG) => {
            libc::sigfillset(set);
            true
        }
        Some(sig) => {
            libc::sigaddset(set, sig);
            true
        }
        None => false,
    }
}

/// Block / unblock each positional sigspec into the caller's process mask.
///
/// `block = true`  -> SIG_BLOCK
/// `block = false` -> SIG_UNBLOCK
unsafe fn sig_modify_block(sigs: WordListIterCpnt<'_>, block: bool) -> CmdResult {
    let mut set = std::mem::zeroed::<libc::sigset_t>();
    libc::sigemptyset(&mut set);
    let mut saw_any_sig = false;
    for cpnt in sigs {
        let cptr = cpnt.as_ptr();
        if !unsafe { sigspec_add(&mut set, cptr) } {
            return Err(EXECUTION_FAILURE);
        }
        saw_any_sig = true;
    }
    if !saw_any_sig {
        return Err(l_builtin_usage_error!(b"no signals specified"));
    }
    let how = if block { libc::SIG_BLOCK } else { libc::SIG_UNBLOCK };
    if libc::sigprocmask(how, &set, std::ptr::null_mut()) < 0 {
        return Err(l_builtin_error!(
            b"sigprocmask: ",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Walk `sigs` to find the `--` separator, parsing each preceding word as a
/// signal specification. Returns the constructed sigset and the WORD_LIST
/// node that is the first word *after* `--`.
///
/// `Ok((set, cmd_list))`   -- success
/// `Err(code)`             -- `EX_USAGE` for missing/ambiguous arguments,
///                            `EXECUTION_FAILURE` for invalid signal specs
unsafe fn split_sigs_and_cmd(
    mut sigs: WordListIterCpnt<'_>,
) -> Result<(libc::sigset_t, *mut WORD_LIST), c_int> {
    let mut set = std::mem::zeroed::<libc::sigset_t>();
    libc::sigemptyset(&mut set);
    let mut saw_any_sig = false;
    while let Some(cpnt) = sigs.next() {
        let cptr = cpnt.as_ptr();
        if cpnt == &b"--"[..] {
            if !saw_any_sig {
                return Err(l_builtin_usage_error!(b"no signals specified"));
            }
            // sigs.next() already advanced past `--`, so sigs.as_ptr() now
            // points at the first word of the command.
            if sigs.as_ptr().is_null() {
                return Err(l_builtin_usage_error!(b"no command given after '--'"));
            }
            return Ok((set, sigs.as_ptr()));
        }
        if !unsafe { sigspec_add(&mut set, cptr) } {
            return Err(EXECUTION_FAILURE);
        }
        saw_any_sig = true;
    }
    Err(l_builtin_usage_error!(
        b"missing '--' separator before command"
    ))
}

// ---------------------------------------------------------------------------
// Subcommand args
// ---------------------------------------------------------------------------

#[derive(CmdArgs)]
struct SigListArgs {
    /// Capture the blocked-signal names into the indexed array VAR instead
    /// of printing one per line.
    #[opt('v')]
    var: Option<BashVar>,
}

#[derive(CmdArgs)]
struct SigBlockArgs {
    #[rest]
    sigs: WordListIterCpnt<'static>,
}

#[derive(CmdArgs)]
struct SigUnblockArgs {
    #[rest]
    sigs: WordListIterCpnt<'static>,
}

#[derive(CmdArgs)]
struct SigRunArgs {
    #[rest]
    rest: WordListIterCpnt<'static>,
}

// ---------------------------------------------------------------------------
// Subcommand handlers
// ---------------------------------------------------------------------------

unsafe fn sig_list(list: *mut WORD_LIST) -> CmdResult {
    SIG_LIST_CMD.enter();
    let args = SigListArgs::parse(list)?;
    sig_list_emit(args.var.as_ref())
}

/// Print every signal currently blocked in the calling shell's process mask,
/// or capture the names into the indexed array `var` if `Some`.
///
/// Signal names mirror bash's `signal_name(i)` (e.g. `SIGINT`); one per line
/// when printing, sparse 1-based indexed array when capturing.
unsafe fn sig_list_emit(var: Option<&BashVar>) -> CmdResult {
    let mut mask = std::mem::zeroed::<libc::sigset_t>();
    if libc::sigprocmask(libc::SIG_BLOCK, std::ptr::null(), &mut mask) < 0 {
        return Err(l_builtin_error!(
            b"sigprocmask: ",
            std::io::Error::last_os_error()
        ));
    }

    let a: Option<*mut ARRAY> = match var {
        Some(v) => {
            let a = l_prepare_indexed_array(v.as_ptr());
            if a.is_null() {
                return Err(l_builtin_error!(
                    b"sig list: cannot create array ",
                    v.as_ptr()
                ));
            }
            Some(a)
        }
        None => None,
    };

    let mut next_index: arrayind_t = 1;
    for i in 1..NSIG {
        if libc::sigismember(&mask, i) == 0 {
            continue;
        }
        let name = signal_name(i);
        if name.is_null() {
            continue;
        }
        match a {
            Some(a) => {
                // array_insert calls savestring() internally, so passing a
                // borrowed pointer is safe.
                array_insert(a, next_index, name);
                next_index += 1;
            }
            None => bprintln!(name),
        }
    }
    Ok(())
}

unsafe fn sig_block(list: *mut WORD_LIST) -> CmdResult {
    SIG_BLOCK_CMD.enter();
    let args = SigBlockArgs::parse(list)?;
    sig_modify_block(args.sigs, true)
}

unsafe fn sig_unblock(list: *mut WORD_LIST) -> CmdResult {
    SIG_UNBLOCK_CMD.enter();
    let args = SigUnblockArgs::parse(list)?;
    sig_modify_block(args.sigs, false)
}

unsafe fn sig_run(list: *mut WORD_LIST) -> CmdResult {
    SIG_RUN_CMD.enter();
    let args = SigRunArgs::parse(list)?;
    let (set, cmd_list) = split_sigs_and_cmd(args.rest)?;
    let rc = l_run_with_unblocked(cmd_list, &set as *const _ as *const crate::bash_api::sigset_t);
    if rc == 0 {
        Ok(())
    } else {
        Err(rc)
    }
}

// ---------------------------------------------------------------------------
// Dispatch tables
// ---------------------------------------------------------------------------

const SIG_CMD: CmdDesc = CmdDesc::new(
    c"sig",
    c"list [-v VAR] | block sigspec... | unblock sigspec... | run sigspec... -- cmd [args...]",
    c"\
Inspect and modify the shell's process signal mask.

Subcommands:
  list [-v VAR]            Print the signals currently blocked in the shell's
                           signal mask (one per line), or write them into the
                           indexed array VAR when -v is given.
  block sigspec...         Block the listed signals in the current shell
                           process. Subsequent commands inherit the modified
                           mask.
  unblock sigspec...       Unblock the listed signals in the current shell
                           process.
  run sigspec... -- cmd    Run `cmd` with the listed signals unblocked. The
                           caller's signal mask is not permanently modified.

Signal specifications follow Bash's existing conventions: signal names with
or without a `SIG` prefix, numeric signal numbers, and the special case-
insensitive token `all` (matches every signal).

Exit Status:
Returns success unless an invalid signal is provided or a system error
occurs. For `run`, returns the executed command's exit status.

Examples:
  L_builtin sig block USR1 USR2
  L_builtin sig list
  # SIGUSR1
  # SIGUSR2
  L_builtin sig list -v blocked
  echo \"${blocked[@]}\"
",
);

const SIG_LIST_CMD: CmdDesc = CmdDesc::new(
    c"list",
    c"list [-v VAR]",
    c"\
Print the signals currently blocked in the shell's signal mask, one per line.
With `-v VAR`, write the names into the indexed array VAR (sparse, 1-based).

This is the process signal mask, not the disposition of any trapped or ignored
signal. Use `trap` for the latter.

Examples:
  L_builtin sig block USR1 USR2
  L_builtin sig list
  # SIGUSR1
  # SIGUSR2

  L_builtin sig list -v blocked
  echo \"There are ${#blocked[@]} blocked signals\"

Exit Status:
Returns success.
",
);

const SIG_BLOCK_CMD: CmdDesc = CmdDesc::new(
    c"block",
    c"block sigspec...",
    c"\
Block the listed signals in the current shell process.

The modification persists for the lifetime of the shell (or until changed
again): subsequent commands run with the modified mask unless they restore
it themselves.

Signal specifications: `SIGINT`/`INT`, numeric numbers, or the case-
insensitive token `all` to block every signal.

Examples:
  L_builtin sig block INT
  L_builtin sig block USR1 USR2

  # Block SIGINT/SIGTERM around a critical region, then briefly unblock
  # them via `sig run` so Ctrl-C / kill can land during the sleep.
  cancel=0
  trap 'cancel=1' INT TERM
  L_builtin sig block INT TERM
  while (( !cancel )); do
    echo 'critical step'
    L_builtin sig run INT TERM -- sleep 1
  done

Exit Status:
Returns success unless an invalid signal is provided or a system error
occurs.
",
);

const SIG_UNBLOCK_CMD: CmdDesc = CmdDesc::new(
    c"unblock",
    c"unblock sigspec...",
    c"\
Unblock the listed signals in the current shell process.

Inverse of `sig block`. Modifies the persistent signal mask; use `sig run` to
unblock signals only for the duration of a single command.

Examples:
  L_builtin sig unblock USR1
  L_builtin sig unblock USR1 USR2
  L_builtin sig unblock all

Exit Status:
Returns success unless an invalid signal is provided or a system error
occurs.
",
);

const SIG_RUN_CMD: CmdDesc = CmdDesc::new(
    c"run",
    c"run sigspec... -- cmd [args...]",
    c"\
Run `cmd` with the listed signals unblocked in the child only.

The caller's signal mask is not permanently modified: the underlying helper
restores it after the command finishes (or after a non-local exit via bash's
unwind-protect machinery).

`--` separates the signal list from the command. At least one signal must be
specified.

Signal specifications: `SIGINT`/`INT`, numeric numbers, or the case-
insensitive token `all`.

Examples:
  # Block SIGINT for a critical step, but let the user Ctrl-C out of the
  # sleep.
  cancel=0
  trap 'cancel=1' INT
  L_builtin sig block INT
  while (( !cancel )); do
    echo 'critical step'
    L_builtin sig run INT -- sleep 1
  done

  # Unblock several signals at once.
  L_builtin sig run USR1 USR2 -- my-command

Exit Status:
Returns the exit status of the executed command.
",
);

const SIG_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("list", sig_list),
    ("block", sig_block),
    ("unblock", sig_unblock),
    ("run", sig_run),
];

const SIG_TABLE: llib::intlookup::U64::IntLookup<SubcommandFn, 4> =
    llib::intlookup!(&SIG_SUBCOMMANDS);

pub unsafe fn sig_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SIG_CMD.enter();
    let args = SubCommandCallerArgs::parse(list)?;
    match args.handler(SIG_TABLE) {
        Ok(caller) => caller.call(),
        Err(code) => Err(code),
    }
}
