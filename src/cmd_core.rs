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

use crate::bash_api::{WordListIterOsString, WordListView, EX_NOTFOUND, WORD_LIST};
use crate::subcmd::CmdDesc;
use crate::{beprintln, getopts, intlookup};

use std::os::raw::c_int;

const ENAME: &str = "L_builtin core";

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
];

const UU_DISPATCH_TABLE: crate::intlookup::U64::IntLookup<UuMain, { UU_DISPATCH_ENTRIES.len() }> =
    intlookup!(UU_DISPATCH_ENTRIES);

/// # Safety
///
/// is safe
#[no_mangle]
pub unsafe extern "C" fn l_core_subcommand(list: *mut WORD_LIST) -> c_int {
    CMD.enter();
    let args = getopts!(list, [], []);
    let view = unsafe { WordListView::from_raw(args) };
    let val = match view.iter().current() {
        Some(val) => val.as_bytes(),
        None => {
            // No subcommand was given - only options (or nothing).
            beprintln!(ENAME, b": missing subcommand");
            beprintln!(b"Usage: L_builtin core <subcommand> [args...]");
            beprintln!(b"Available: ls, stat, dirname, rm, tee");
            return EX_NOTFOUND;
        }
    };
    let uumain = match UU_DISPATCH_TABLE.lookup(val) {
        Some(f) => f,
        None => {
            beprintln!(ENAME, b": unknown subcommand: ", val);
            return EX_NOTFOUND;
        }
    };
    let rest: UuArgs = view.iter_osstring();
    uumain(rest)
}
