//! L_builtin `core` subcommand: coreutils via uutils, running in-process.
//!
//! Usage: `L_builtin core [-v VAR] <subcommand> [args...]`
//!
//! With `-v VAR` the subcommand's stdout is redirected to a memfd and the
//! captured output is bound to the shell variable VAR (trailing newlines
//! stripped, matching `$(...)` semantics) - no fork, no pipe.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{
    WordListIterCpnt, WordListIterOsString, WordListView, EX_NOTFOUND, WORD_LIST,
};
use crate::l_builtin_error;
use crate::subcmd::{CmdDesc, CmdResult};
use crate::{beprintln, intlookup};
use cmdargs_derive::CmdArgs;

const CMD: CmdDesc = CmdDesc::new(
    c"core",
    c"<subcommand> [options] [args]",
    c"\
Core utilities via uutils/coreutils

Available subcommands:
    ls       List directory contents
    stat     Display file status
    dirname  Strip last component from file name
    rm       Remove files or directories
    tee      Copy stdin to each FILE and stdout
    sleep    Delay for a specified amount of time

Use 'L_builtin core <subcommand> --help' for more information.
",
);

/// The argv iterator passed to a uutils `uumain`: argv[0] = subcommand name,
/// followed by the remaining args. It is exactly `Iterator<Item = OsString>`,
/// so it satisfies `uucore::Args`.
type UuArgs<'a> = WordListIterOsString<'a>;

/// Type of a uutils `uumain` wrapper: forwards the prebuilt argv iterator to
/// the util's `uumain` and returns the process exit code.
type UuMain = for<'a> fn(UuArgs<'a>) -> c_int;

macro_rules! uu_entry {
    ($path:ident) => {
        (|args: UuArgs| $path::uumain(args)) as UuMain
    };
}
const UU_DISPATCH_ENTRIES: &[(&str, UuMain)] = &[
    ("dirname", uu_entry!(uu_dirname)),
    ("ls", uu_entry!(uu_ls)),
    ("rm", uu_entry!(uu_rm)),
    ("stat", uu_entry!(uu_stat)),
    ("tee", uu_entry!(uu_tee)),
    ("sleep", uu_entry!(uu_sleep)),
];

const UU_DISPATCH_TABLE: crate::intlookup::U64::IntLookup<UuMain, { UU_DISPATCH_ENTRIES.len() }> =
    intlookup!(UU_DISPATCH_ENTRIES);

/// # Safety
///
/// is safe
#[derive(CmdArgs)]
struct CoreDispatchArgs {
    #[rest]
    rest: WordListIterCpnt<'static>,
}

/// # Safety
///
/// is safe
pub unsafe fn l_core_subcommand(list: *mut WORD_LIST) -> CmdResult {
    CMD.enter();
    let args = CoreDispatchArgs::parse(list)?;
    let rest_view = args.rest;
    let val = match rest_view.current() {
        Some(val) => val.as_bytes(),
        None => {
            // No subcommand was given - only options (or nothing).
            l_builtin_error!(b"missing subcommand");
            beprintln!(b"Usage: L_builtin core <subcommand> [args...]");
            beprintln!(b"Available: ls, stat, dirname, rm, tee");
            return Err(EX_NOTFOUND);
        }
    };
    let uumain = match UU_DISPATCH_TABLE.lookup(val) {
        Some(f) => f,
        None => {
            l_builtin_error!(b"unknown subcommand: ", val);
            return Err(EX_NOTFOUND);
        }
    };
    let rest_view_lv = WordListView::from_raw(rest_view.as_ptr());
    let uuargs: UuArgs = rest_view_lv.iter_osstring();
    let r = uumain(uuargs);
    if r == 0 {
        Ok(())
    } else {
        Err(r)
    }
}
