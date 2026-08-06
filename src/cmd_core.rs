//! L_builtin `core` subcommand: coreutils via uutils, running in-process.
//!
//! Usage: `L_builtin core [-v VAR] <subcommand> [args...]`
//!
//! With `-v VAR` the subcommand's stdout is redirected to a memfd and the
//! captured output is bound to the shell variable VAR (trailing newlines
//! stripped, matching `$(...)` semantics) — no fork, no pipe.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WordListIterOsString, WordListView, EX_NOTFOUND, EX_USAGE, WORD_LIST};
use crate::intlookup::IntLookup64;
use crate::shared::capture_into_variable;
use crate::{bash_getopt, beprintln, bprintln, intlookup};

use std::os::raw::c_int;

const ENAME: &str = "L_builtin core";

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
];

const UU_DISPATCH_TABLE: IntLookup64<UuMain, { UU_DISPATCH_ENTRIES.len() }> = intlookup!(UU_DISPATCH_ENTRIES);

fn print_core_help() {
    bprintln!(
        b"\
L_builtin core [-v VAR] <subcommand> [options] [args]

Core utilities via uutils/coreutils

Options:
    -v VAR   Capture stdout of the subcommand into shell variable VAR
            (trailing newlines stripped, like $(...))

Available subcommands:
    ls       List directory contents
    stat     Display file status
    dirname  Strip last component from file name
    rm       Remove files or directories

Use 'L_builtin core <subcommand> -h' for more information.
"
    );
}

/// # Safety
///
/// is safe
#[no_mangle]
pub unsafe extern "C" fn l_core_subcommand(list: *mut WORD_LIST) -> c_int {
    let (opts, args) = bash_getopt!(list, print_core_help, [], [v]);
    let view = unsafe { WordListView::from_raw(args) };
    let val = match view.iter().current() {
        Some(val) => val.to_bytes(),
        None => {
            // No subcommand was given — only options (or nothing).
            beprintln!(ENAME, b": missing subcommand");
            beprintln!(b"Usage: L_builtin core [-v VAR] <subcommand> [args...]");
            beprintln!(b"Available: ls, stat, dirname, rm");
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
    match opts.v {
        Some(var) => capture_into_variable(ENAME, var, false, || uumain(rest)),
        None => uumain(rest),
    }
}
