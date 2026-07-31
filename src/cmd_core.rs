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

use crate::bash_api::{EX_NOTFOUND, EX_USAGE, WORD_LIST, WordListView};
use crate::shared::{capture_into_variable, sort_by_byte_key};
use crate::return_on_err;

use std::ffi::OsString;
use std::os::raw::c_int;
use std::os::unix::ffi::OsStrExt;

use lexopt::{prelude::*, Parser};

const ENAME: &str = "L_builtin core";

/// The argv iterator passed to a uutils `uumain`: argv[0] = subcommand name,
/// followed by the remaining args. It is exactly `Iterator<Item = OsString>`,
/// so it satisfies `uucore::Args`.
type UuArgs<'a> = std::iter::Chain<std::iter::Once<OsString>, lexopt::RawArgs<'a>>;

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

#[no_mangle]
pub extern "C" fn l_core_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut parser = Parser::from_args(unsafe { WordListView::from_raw(list) });
    let mut capture_var: Option<OsString> = None;
    while let Some(arg) = return_on_err!(ENAME, parser.next(), EX_USAGE) {
        match arg {
            Short('v') | Long("var") => {
                capture_var = Some(return_on_err!(ENAME, parser.value(), EX_USAGE));
            }
            Short('h') | Long("help") => {
                print_core_help();
                return 0;
            }
            Short(c) => {
                eprintln!("{ENAME}: unknown option -{c}");
                return 2;
            }
            Long(l) => {
                eprintln!("{ENAME}: unknown option --{l}");
                return 2;
            }
            Value(val) => {
                // First free argument is the subcommand — resolve it now.
                let uumain =
                    match UU_DISPATCH_TABLE.binary_search_by(|(n, _)| n.cmp(&val.as_bytes())) {
                        Ok(i) => UU_DISPATCH_TABLE[i].1,
                        Err(_) => {
                            eprintln!("{ENAME}: unknown subcommand: {}", val.to_string_lossy());
                            return EX_NOTFOUND;
                        }
                    };
                let rest: UuArgs = std::iter::once(val).chain(return_on_err!(ENAME, parser.raw_args(), EX_USAGE));
                return match capture_var {
                    None => uumain(rest),
                    Some(var) => capture_into_variable(
                        ENAME,
                        &var,
                        || uumain(rest),
                    ),
                };
            }
        }
    }
    // No subcommand was given — only options (or nothing).
    eprintln!("{ENAME}: missing subcommand");
    eprintln!("Usage: L_builtin core [-v VAR] <subcommand> [args...]");
    eprintln!("Available: ls, stat, dirname, rm");
    return EX_NOTFOUND;
}

fn print_core_help() {
    eprintln!("Core utilities via uutils/coreutils");
    eprintln!();
    eprintln!("L_builtin core [-v VAR] <subcommand> [options] [args]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -v VAR   Capture stdout of the subcommand into shell variable VAR");
    eprintln!("           (trailing newlines stripped, like $(...))");
    eprintln!();
    eprintln!("Available subcommands:");
    eprintln!("  ls       List directory contents");
    eprintln!("  stat     Display file status");
    eprintln!("  dirname  Strip last component from file name");
    eprintln!("  rm       Remove files or directories");
    eprintln!();
    eprintln!("Use 'L_builtin core <subcommand> -h' for more information.");
}
