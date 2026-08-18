//! Runtime support for the `#[derive(CmdArgs)]` argument parser.
//!
//! This module owns:
//! - the [`FromCpnt`] trait: convert one bash word ([`Cpnt`]) into a typed
//!   value. Implementations that only forward the pointer (`*const c_char`,
//!   [`Cpnt`]) perform **no** `strlen`/decode; conversions that need the value
//!   (`c_int`, `i64`) pay for it.
//! - the [`CmdArgs`] trait: the contract the derive fills in (option dispatch,
//!   positional binding, optstring metadata).
//! - re-exports of the bash FFI symbols the generated code calls, so the derive
//!   only ever refers to `crate::cmdargs::*`.

#![allow(non_camel_case_types)]

pub use crate::bash_api::{
    builtin_usage, internal_getopt, l_builtin_usage_long, list_optarg, loptend,
    reset_internal_getopt, Cpnt, WordListIterCpnt, WordListView, EX_USAGE, GETOPT_HELP, WORD_LIST,
};
pub use std::ffi::{c_char, c_int, CStr};

/// Convert a single bash word ([`Cpnt`]) into a typed Rust value.
///
/// `Err` must be `Display` so the generated parser can print it and return
/// `EX_USAGE`. Implementations that merely forward the raw pointer are
/// zero-copy and never fail (`Infallible`); numeric/string conversions decode
/// the bytes and therefore may fail.
pub trait FromCpnt: Sized {
    type Err: std::fmt::Display;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err>;
}

impl FromCpnt for *const c_char {
    type Err = std::convert::Infallible;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
        Ok(cptr.as_ptr() as *const c_char)
    }
}

impl FromCpnt for c_int {
    type Err = String;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
        let s = cptr.as_str().map_err(|e| e.to_string())?;
        s.parse::<c_int>().map_err(|e| e.to_string())
    }
}

#[macro_export]
macro_rules! define_parse_FromCpnt {
    ($T:ty) => {
        impl FromCpnt for $T {
            type Err = String;
            unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
                let s = cptr.as_str().map_err(|e| e.to_string())?;
                s.parse::<$T>().map_err(|e| e.to_string())
            }
        }
    };
}

define_parse_FromCpnt!(i64);
define_parse_FromCpnt!(u32);
define_parse_FromCpnt!(usize);
define_parse_FromCpnt!(u64);
define_parse_FromCpnt!(f64);

impl FromCpnt for *mut c_char {
    type Err = std::convert::Infallible;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
        Ok(cptr.as_ptr())
    }
}

impl FromCpnt for &'static CStr {
    type Err = std::convert::Infallible;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
        Ok(CStr::from_ptr(cptr.as_ptr() as *const c_char))
    }
}

/// Run a custom `#[parse]` converter `f` against one bash word.
///
/// The `F: FnOnce(Cpnt) -> Result<T, E>` bound pins the closure's parameter type
/// to [`Cpnt`], so user closures written as `|cptr| ...` infer `cptr: Cpnt`
/// without an explicit annotation.
pub fn parse_with<T, E, F>(cptr: Cpnt, f: F) -> Result<T, E>
where
    F: FnOnce(Cpnt) -> Result<T, E>,
{
    f(cptr)
}

/// Contract implemented by the `#[derive(CmdArgs)]` macro for every annotated
/// struct. The derive generates the bodies; the trait exists so that
/// `#[flatten]` can merge a child `CmdArgs` struct's options and positionals
/// into its parent.
///
/// `OPTSTRING` holds the full optstring this struct contributes - its own
/// chars plus every flattened descendant's chars, joined at **compile time** via
/// a generated `__cmdargs_inherit_*` macro - with the trailing `h` and NUL
/// terminator already appended, so it can be passed straight to
/// `internal_getopt`.
pub trait CmdArgs {
    /// Option characters this struct contributes, e.g. `"v:"` for `-v VALUE`.
    /// Includes the trailing `h` and NUL terminator, so it can be passed
    /// straight to `internal_getopt` when this struct is parsed standalone.
    const OPTSTRING: &'static str;
    /// Whether this struct (or a flattened child) declares a variadic `rest`.
    const HAS_REST: bool;
    /// Construct with every field in its "empty" state.
    fn new_default() -> Self;
    /// Apply one parsed option character `c` with optarg pointer `p`.
    unsafe fn apply_opt(&mut self, c: c_int, p: *mut c_char) -> Result<(), c_int>;
    /// Bind the remaining positional words from `iter`.
    unsafe fn fill_positionals(&mut self, iter: &mut WordListIterCpnt) -> Result<(), c_int>;

    /// Cross-field validation hook, run once after every option and positional has
    /// been bound. Override it in an *inherent* `impl` of the args struct (an
    /// inherent method shadows this trait default during method resolution) to
    /// enforce constraints that span several fields - for example that `-s`, `-n`
    /// and `-f` are mutually exclusive. The default does nothing.
    ///
    /// Returning `Err(code)` aborts parsing with that exit code (the generated
    /// dispatcher already prints a message via [`crate::l_builtin_error!`]).
    fn post(&self) -> Result<(), c_int> {
        Result::Ok(())
    }

    /// Parse `list` (the subcommand's `WORD_LIST`) into `Self`.
    ///
    /// Shared across every `#[derive(CmdArgs)]` struct: runs
    /// `reset_internal_getopt` + `internal_getopt` over the words, dispatches value
    /// options via [`CmdArgs::apply_opt`], then binds positionals via
    /// [`CmdArgs::fill_positionals`]. The derived `impl` only provides the consts
    /// and the per-field methods; the loop lives here once.
    #[inline]
    unsafe fn parse(list: *mut WORD_LIST) -> Result<Self, c_int>
    where
        Self: Sized,
    {
        let mut this = Self::new_default();

        reset_internal_getopt();
        loop {
            let c = internal_getopt(list, Self::OPTSTRING.as_ptr() as *mut c_char);
            if c == -1 {
                break;
            }
            if c == GETOPT_HELP || c == (b'h' as c_int) {
                l_builtin_usage_long();
                return Result::Err(0);
            }
            if c == (b'?' as c_int) || c == (b':' as c_int) {
                builtin_usage();
                return Result::Err(EX_USAGE);
            }
            Self::apply_opt(&mut this, c, list_optarg)?;
        }

        let mut iter = WordListView::from_raw(loptend).iter();
        Self::fill_positionals(&mut this, &mut iter)?;

        if !Self::HAS_REST && iter.next().is_some() {
            crate::l_builtin_usage_error!(b"too many arguments");
            return Result::Err(EX_USAGE);
        }

        this.post()?;

        Result::Ok(this)
    }
}
