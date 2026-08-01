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

use crate::bash_api::{WordListIter, WordListView, EX_NOTFOUND, EX_USAGE, WORD_LIST};
use crate::shared::{
    capture_into_variable, from_after_null_terminated, getargs_unexpected, sort_by_byte_key,
};
use crate::{beprintln, return_on_err};

use std::ffi::OsString;
use std::os::raw::c_int;
use std::os::unix::ffi::OsStringExt;

use getargs::{IntoPositionals, Opt, Options};

const ENAME: &str = "L_builtin core";

/// The argv iterator passed to a uutils `uumain`: argv[0] = subcommand name,
/// followed by the remaining args. It is exactly `Iterator<Item = OsString>`,
/// so it satisfies `uucore::Args`.
type UuArgs<'a> = std::iter::Map<
    std::iter::Chain<
        std::option::IntoIter<&'a [u8]>,
        IntoPositionals<&'a [u8], &'a mut WordListIterWithPos<'a>>,
    >,
    for<'b> fn(&'b [u8]) -> OsString,
>;

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

fn bytes_to_ostring(s: &[u8]) -> OsString {
    OsString::from_vec(s.to_vec())
}

/// Like WordListIter but also keep previous position so you can restart.
/// When passing args into uumain, we need all args, this is just simpler.
/// The cost is however another strlen.
struct WordListIterWithPos<'a> {
    iter: WordListIter<'a>,
    pub current: WordListIter<'a>,
}

impl<'a> Iterator for WordListIterWithPos<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        self.current = self.iter.clone();
        self.iter.next()
    }
}

impl<'a> WordListView<'a> {
    fn iter_with_pos(&self) -> WordListIterWithPos<'a> {
        let iter = self.iter();
        WordListIterWithPos {
            iter: iter.clone(),
            current: iter,
        }
    }
}

/// # Safety
///
/// is safe
#[no_mangle]
pub unsafe extern "C" fn l_core_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut args = unsafe { WordListView::from_raw(list) }.iter_with_pos();
    let mut opts = Options::new(&mut args);
    let mut capture_var: Option<&[u8]> = None;
    while let Some(arg) = return_on_err!(ENAME, opts.next_opt(), EX_USAGE) {
        match arg {
            Opt::Short(b'v') | Opt::Long(b"var") => {
                capture_var = Some(return_on_err!(ENAME, opts.value(), EX_USAGE));
            }
            Opt::Short(b'h') | Opt::Long(b"help") => {
                print_core_help();
                return 0;
            }
            _ => return getargs_unexpected(ENAME, arg),
        }
    }
    let first = opts.next_positional();
    let val = match first {
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
            beprintln!(b"{ENAME}: unknown subcommand: ", val);
            return EX_NOTFOUND;
        }
    };
    let bytes_to_string_coerced: fn(&[u8]) -> OsString = bytes_to_ostring;
    let rest: UuArgs = first
        .into_iter()
        .chain(opts.into_positionals())
        .map(bytes_to_string_coerced);
    return match capture_var {
        None => uumain(rest),
        Some(var) => {
            // var is a slice from bash WORD_LIST; it comes from a NUL-terminated C string.
            // We can use it directly by treating it as a C string (the NUL is just past the slice).
            // Since we need a *const c_char, cast the slice pointer.
            capture_into_variable(ENAME, from_after_null_terminated(var), || uumain(rest))
        }
    };
}

fn print_core_help() {
    beprintln!(b"Core utilities via uutils/coreutils");
    beprintln!(b"");
    beprintln!(b"L_builtin core [-v VAR] <subcommand> [options] [args]");
    beprintln!(b"");
    beprintln!(b"Options:");
    beprintln!(b"  -v VAR   Capture stdout of the subcommand into shell variable VAR");
    beprintln!(b"           (trailing newlines stripped, like $(...))");
    beprintln!(b"");
    beprintln!(b"Available subcommands:");
    beprintln!(b"  ls       List directory contents");
    beprintln!(b"  stat     Display file status");
    beprintln!(b"  dirname  Strip last component from file name");
    beprintln!(b"  rm       Remove files or directories");
    beprintln!(b"");
    beprintln!(b"Use 'L_builtin core <subcommand> -h' for more information.");
}
