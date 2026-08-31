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
pub use crate::intstr::{IntStrPtr, ToIntStr};
use crate::{
    bash_api::{find_variable, l_readonly_p},
    subcmd::CmdResult,
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
define_parse_FromCpnt!(u16);
define_parse_FromCpnt!(u32);
define_parse_FromCpnt!(usize);
define_parse_FromCpnt!(u64);
define_parse_FromCpnt!(f64);

impl FromCpnt for &'static str {
    type Err = String;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
        CStr::from_ptr(cptr.as_ptr() as *const c_char)
            .to_str()
            .map(|s| s as &'static str)
            .map_err(|e| e.to_string())
    }
}

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

impl FromCpnt for &'static [u8] {
    type Err = std::convert::Infallible;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
        Ok(CStr::from_ptr(cptr.as_ptr() as *const c_char).to_bytes() as &'static [u8])
    }
}

/// A duration value that can be used as a `#[derive(CmdArgs)]` field type.
///
/// Accepts both human-readable duration strings (via the `parse_duration`
/// crate) and bare floating-point numbers for backward compatibility:
///
/// - `"500ms"`, `"1s"`, `"1h30m"`, `"2min"`, `"1d"` -- parsed by
///   `parse_duration::parse`, which understands units like `ns`, `us`, `ms`,
///   `s`, `m`, `h`, `d`, etc.
/// - `"1.5"`, `"0.25"` -- interpreted as seconds (f64), preserving the
///   original behavior of the `sleep` / `timerfd` subcommands.
///
/// Construct one from a bash word via `#[positional] name: Duration` (or
/// `Option<Duration>` / `#[opt('s')] name: Option<Duration>`).
///
/// # Example
/// ```ignore
/// #[derive(CmdArgs)]
/// struct SleepArgs {
///     #[positional]
///     seconds: Duration,
/// }
/// ```
#[derive(Clone, Copy)]
pub struct Duration(std::time::Duration);

impl Duration {
    /// Return the duration as seconds (f64), including fractional part.
    pub fn as_secs_f64(&self) -> f64 {
        self.0.as_secs_f64()
    }

    /// Convert to a `libc::timespec`, clamping nanoseconds to `[0, 1e9)`.
    pub fn as_timespec(&self) -> libc::timespec {
        let secs = self.0.as_secs();
        let nanos = self.0.subsec_nanos();
        libc::timespec {
            tv_sec: secs as libc::time_t,
            tv_nsec: nanos as libc::c_long,
        }
    }
}

impl Default for Duration {
    fn default() -> Self {
        Duration(std::time::Duration::ZERO)
    }
}

impl From<std::time::Duration> for Duration {
    fn from(d: std::time::Duration) -> Self {
        Duration(d)
    }
}

impl std::ops::Deref for Duration {
    type Target = std::time::Duration;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromCpnt for Duration {
    type Err = String;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
        let s = cptr.as_str().map_err(|e| e.to_string())?;
        // First try `parse_duration` for human-readable strings ("1s", "500ms",
        // "1h30m", etc.).
        if let Ok(d) = parse_duration::parse(s) {
            return Ok(Duration(d));
        }
        // Fall back to a bare f64 number interpreted as seconds, preserving
        // backward compatibility with the original `SECONDS` argument.
        let secs = s
            .parse::<f64>()
            .map_err(|e| format!("invalid duration: {s}: {e}"))?;
        if secs < 0.0 {
            return Err(format!("invalid duration: {s}: negative value"));
        }
        Ok(Duration(std::time::Duration::from_secs_f64(secs)))
    }
}

/////////////////////////////////////////////////////////

#[allow(clippy::not_unsafe_ptr_arg_deref)]
unsafe fn l_bind_variable_check(name: *const c_char, value: *const c_char) -> CmdResult {
    unsafe {
        debug_assert!(!name.is_null(), "name is null");
        debug_assert!(!value.is_null(), "value is null");
        let var = find_variable(name);
        if !var.is_null() && l_readonly_p(var) != 0 {
            return Err(crate::l_builtin_error!(name, ": readonly variable"));
        }
        if crate::bash_api::bind_variable(name, value.cast_mut(), 0).is_null() {
            return Err(crate::l_builtin_error!("failed to set variable: ", name));
        }
    }
    Ok(())
}

/// A shell variable name, validated at construction, that can bind a value to
/// itself through [`crate::bash_api::bind_variable`].
///
/// Construct one from a bash word via `#[positional] name: BashVar` (or
/// `Option<BashVar>` / `#[opt('v')] name: Option<BashVar>`). The name is checked
/// against bash's `legal_identifier` as it is parsed, so an illegal name aborts
/// the parse with `EX_USAGE` before any side effects run. The [`BashVar::set`]
/// method performs the actual binding and reports a failure uniformly.
pub struct BashVar {
    name: *const c_char,
}

impl Default for BashVar {
    fn default() -> Self {
        BashVar {
            name: std::ptr::null(),
        }
    }
}

impl BashVar {
    pub unsafe fn validate(name: *const c_char) -> Result<Self, String> {
        if crate::bash_api::legal_identifier(name) == 0 {
            let display = CStr::from_ptr(name).to_string_lossy();
            return Err(format!("`{display}': not a valid identifier"));
        }
        let var = find_variable(name);
        if !var.is_null() && l_readonly_p(var) != 0 {
            let display = CStr::from_ptr(name).to_string_lossy();
            return Err(format!("{display}: readonly variable"));
        }
        Ok(BashVar { name })
    }
    pub fn set(&self, value: *const c_char) -> CmdResult {
        unsafe { l_bind_variable_check(self.name, value) }
    }
    pub fn set_int<T: ToIntStr>(&self, value: T) -> CmdResult {
        self.set(value.to_intstr().as_ptr())
    }
    pub fn as_ptr(&self) -> *const c_char {
        self.name
    }
}

impl FromCpnt for BashVar {
    type Err = String;
    unsafe fn from_cpnt(cptr: Cpnt) -> Result<Self, Self::Err> {
        BashVar::validate(cptr.as_ptr())
    }
}

/////////////////////////////////////////////////////////

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
            return Result::Err(crate::l_builtin_usage_error!(b"too many arguments"));
        }

        this.post()?;

        Result::Ok(this)
    }
}
