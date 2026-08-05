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
use crate::shared::{capture_into_variable, sort_by_byte_key};
use crate::{bash_getopt, beprintln, bprintln};

use std::os::raw::c_int;

const ENAME: &str = "L_builtin core";

/// The argv iterator passed to a uutils `uumain`: argv[0] = subcommand name,
/// followed by the remaining args. It is exactly `Iterator<Item = OsString>`,
/// so it satisfies `uucore::Args`.
type UuArgs<'a> = WordListIterOsString<'a>;

/// Type of a uutils `uumain` wrapper: forwards the prebuilt argv iterator to
/// the util's `uumain` and returns the process exit code. Each util needs its
/// own one-line wrapper because `uumain` is a generic fn item
/// (`fn(args: impl uucore::Args) -> i32`) — a homogeneous const array cannot
/// unify the distinct generic fn-item types, so we store plain `fn` pointers.
type UuMain = for<'a> fn(UuArgs<'a>) -> c_int;

fn uu_dirname_main(args: UuArgs) -> c_int {
    uu_dirname::uumain(args)
}
fn uu_ls_main(args: UuArgs) -> c_int {
    uu_ls::uumain(args)
}
fn uu_rm_main(args: UuArgs) -> c_int {
    uu_rm::uumain(args)
}
fn uu_stat_main(args: UuArgs) -> c_int {
    uu_stat::uumain(args)
}

/// Dispatch table: subcommand name -> uutils `uumain` wrapper.
///
/// Sorted by name at compile time (`sort_by_byte_key` is a const fn), so
/// lookups use binary search — mirroring `SUBCOMMAND_TABLE` in dispatch.rs.
const UU_DISPATCH_TABLE: &[(&[u8], UuMain)] = &sort_by_byte_key([
    (b"dirname", uu_dirname_main as UuMain),
    (b"ls", uu_ls_main as UuMain),
    (b"rm", uu_rm_main as UuMain),
    (b"stat", uu_stat_main as UuMain),
]);

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
    let val = match view.into_iter().current() {
        Some(val) => val,
        None => {
            // No subcommand was given — only options (or nothing).
            beprintln!(ENAME, b": missing subcommand");
            beprintln!(b"Usage: L_builtin core [-v VAR] <subcommand> [args...]");
            beprintln!(b"Available: ls, stat, dirname, rm");
            return EX_NOTFOUND;
        }
    };
    // First free argument is the subcommand — resolve it now.
    let uumain = match UU_DISPATCH_TABLE.binary_search_by(|(n, _)| n.cmp(&val)) {
        Ok(i) => UU_DISPATCH_TABLE[i].1,
        Err(_) => {
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
