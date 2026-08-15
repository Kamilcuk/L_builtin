//! Bash option/argument parsing helpers.
//!
//! Single unified API:
//! - [`getopts!`] parses options with bash's `internal_getopt` and runs action
//!   closures; it returns the remaining-words pointer.
//! - [`parse_positionals!`] then consumes the remaining words into typed
//!   positional arguments (required / optional / variadic).

// Re-exported here so `$crate::getopts::*` paths in the macros resolve.
pub use crate::bash_api::{internal_getopt, list_optarg, loptend, reset_internal_getopt};

/// Specifies the arity of a positional argument.
///
/// Each variant carries the argument's name as a `&'static str` (via
/// `stringify!` - zero allocation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionalSpec {
    /// Must be present exactly once.
    Required(&'static str),
    /// May be absent; produces `Option<Cpnt>`.
    Optional(&'static str),
    /// Consumes all remaining words (may be empty); produces `Vec<Cpnt>`.
    ZeroOrMore(&'static str),
    /// Consumes all remaining words (must be at least one); produces `Vec<Cpnt>`.
    OneOrMore(&'static str),
}

impl PositionalSpec {
    /// Return the name of this positional argument.
    pub fn name(&self) -> &'static str {
        match self {
            PositionalSpec::Required(n)
            | PositionalSpec::Optional(n)
            | PositionalSpec::ZeroOrMore(n)
            | PositionalSpec::OneOrMore(n) => n,
        }
    }
}

/// Validate positional specs at compile time: variadic (`*`, `+`) must be last,
/// and no required argument may follow an optional one.
pub const fn validate_positional_specs(specs: &[PositionalSpec]) -> Option<&'static str> {
    let mut seen_optional = false;
    let mut i = 0;

    while i < specs.len() {
        match specs[i] {
            PositionalSpec::Required(_) => {
                if seen_optional {
                    return Some(
                        "required positional argument cannot follow optional positional argument",
                    );
                }
            }
            PositionalSpec::Optional(_) => {
                seen_optional = true;
            }
            PositionalSpec::ZeroOrMore(_) | PositionalSpec::OneOrMore(_) => {
                if i != specs.len() - 1 {
                    return Some(
                        "variadic positional argument ('*' or '+') must be the last positional argument",
                    );
                }
            }
        }
        i += 1;
    }
    None
}

/// Returns `true` if any positional spec is variadic (`*` or `+`).
pub const fn has_variadic(specs: &[PositionalSpec]) -> bool {
    let mut i = 0;
    while i < specs.len() {
        match specs[i] {
            PositionalSpec::ZeroOrMore(_) | PositionalSpec::OneOrMore(_) => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

// Define a trait/helper inside your crate to fix the type context
#[inline(always)]
pub fn invoke_opt_action<F: FnMut(crate::bash_api::Cpnt)>(
    mut action: F,
    arg: crate::bash_api::Cpnt,
) {
    action(arg);
}

/// Parse options from a bash `WORD_LIST` using bash's `internal_getopt`.
///
/// `-h` and `--help` are handled automatically (calling
/// `l_builtin_usage_long()` with the active subcommand's docs) and the
/// enclosing function returns `0`. The `-h` flag is appended to the optstring
/// automatically - do not add it manually.
///
/// # Syntax
///
/// ```ignore
/// let rest = getopts!(
///     list,                                // *mut WORD_LIST
///     [ f => || force = true ],            // flag: -f (action called with no args)
///     [ n => |nm| name = nm.as_ptr() ],    // option -n NAME (action gets the optarg)
/// );
/// ```
///
/// **Option actions** for flags (`f`) are called with no arguments (`||`).
/// For value options (the second bracket group), the action receives `Cpnt`
/// (the optarg). Do not put a `:` in the name - the colon that marks an option
/// as taking an argument is implied by the option living in the *second* bracket
/// group, so write `n => ...`, never `n: => ...`.
///
/// Returns the remaining-words pointer (`loptend`), the words after the end
/// of options. Feed it to [`parse_positionals!`].
///
/// On error prints using bash's help machinery and returns `EX_USAGE` from the
/// enclosing function.
///
/// # Example
///
/// A complete subcommand that takes an optional `-n NAME` and one required
/// positional `VAR` (`VAR` is an `Option<Cpnt>` here, but a required
/// positional would be a bare `VAR`):
///
/// ```ignore
/// let mut name: *mut c_char = std::ptr::null_mut();
/// let rest = getopts!(
///     list,
///     [],
///     [ n => |nm| name = nm.as_ptr().cast() ]
/// );
/// let (var,) = parse_positionals!(rest, [], [VAR]);
/// // `var` is `Option<Cpnt>`; borrow it as a `CStr` with `as_cstr()`.
/// let var_cstr: &CStr = match var {
///     Some(v) => unsafe { v.to_cstr() },
///     None    => c"SHMVAR",
/// };
/// let var_ptr: *const c_char = var_cstr.as_ptr();
/// // ... use `name` (a `*mut c_char`) and `var_ptr` (a `*const c_char`) ...
/// ```
///
/// Note: a value-option action like `n => |nm| ...` receives the optarg as a
/// `Cpnt`; call `.as_ptr()` to get a `*mut c_char`. The `-h`/`--help` flag is
/// added automatically and prints the subcommand's `CmdDesc` long help.
#[macro_export]
macro_rules! getopts {
    (
        $args:expr,
        [ $( $flag:ident => $flag_action:expr ),* $(,)? ],
        [ $( $opt:ident => $opt_action:expr ),* $(,)? ] $(,)?
    ) => {{
        $crate::getopts::reset_internal_getopt();
        let list = $args;
        const OPTSTRING: &[u8] = concat!(
            $( stringify!($flag), )*
            $( stringify!($opt), ":", )*
            "h\0"
        ).as_bytes();
        loop {
            let c = unsafe {
                $crate::getopts::internal_getopt(
                    list,
                    OPTSTRING.as_ptr().cast::<std::os::raw::c_char>().cast_mut(),
                )
            };
            match c {
                -1 => break,
                $crate::bash_api::GETOPT_HELP  | 104 /* 'h' */ => {
                    $crate::bash_api::l_builtin_usage_long();
                    return 0;
                }
                $(
                    c if c == stringify!($flag).as_bytes()[0] as std::os::raw::c_int => {
                        $flag_action();
                    }
                )*
                $(
                    c if c == stringify!($opt).as_bytes()[0] as std::os::raw::c_int => {
                        $crate::getopts::invoke_opt_action(
                            $opt_action,
                            unsafe { $crate::bash_api::Cpnt::new($crate::getopts::list_optarg) },
                        );
                    }
                )*
                _ => {
                    $crate::bash_api::builtin_usage();
                    return $crate::bash_api::EX_USAGE;
                }
            }
        }
        unsafe { $crate::getopts::loptend }
    }};
}

/// Parse positional arguments from the words remaining after option parsing.
///
/// # Syntax
///
/// ```ignore
/// let (src, files)      = parse_positionals!(rest, [src], *files);
/// let (src, dest, files)= parse_positionals!(rest, [src], [dest], *files);
/// let (src, mode, files)= parse_positionals!(rest, [src], [mode], +files);
/// ```
///
/// **Positional grouping** controls arity, expressed by bracket groups and a
/// trailing `*`/`+`, not by inline `?`/`*` inside the first bracket:
/// - First bracket `[a, b]`      - required positionals (`Cpnt`; missing -> `EX_USAGE`).
/// - Second bracket `[c]`         - optional positionals (`Option<Cpnt>`).
/// - Trailing `*files`            - zero or more (`Vec<Cpnt>`).
/// - Trailing `+files`            - one or more (`Vec<Cpnt>`; empty -> `EX_USAGE`).
///
/// So `parse_positionals!(rest, [src], [dest], *files)` means: required `src`,
/// optional `dest`, then zero-or-more `files`. The first bracket holds *only*
/// required idents; the `?`/`*` modifiers must never appear inside it.
///
/// Following the `*`/`+` spec the value is a `Vec<Cpnt>`.
///
/// Returns a tuple `(arg1, arg2, ...)` of the bound positional values.
///
/// Spec ordering is validated at **compile time**: variadic specs (`*`, `+`)
/// must come last, and no required may follow an optional - an invalid
/// ordering is a build error. At runtime, if extra words remain after parsing
/// and no variadic spec is present, prints "too many arguments" to stderr and
/// returns `EX_USAGE`.
///
/// On error prints to stderr and returns `EX_USAGE` from the enclosing function.
#[macro_export]
macro_rules! parse_positionals {
    (
        $words_ptr:expr,
        [ $( $req:ident ),* $(,)? ]
        $( , [ $( $opt:ident ),* $(,)? ] )?
        $( , * $var_zero:ident )?
        $( , + $var_one:ident )?
    ) => {{
        // Compile-time validation
        const HAS_VARIADIC: bool = {
            const SPECS: &[$crate::getopts::PositionalSpec] = &[
                $( $crate::getopts::PositionalSpec::Required(stringify!($req)), )*
                $( $( $crate::getopts::PositionalSpec::Optional(stringify!($opt)), )* )?
                $( $crate::getopts::PositionalSpec::ZeroOrMore(stringify!($var_zero)), )?
                $( $crate::getopts::PositionalSpec::OneOrMore(stringify!($var_one)), )?
            ];

            if let Some(err) = $crate::getopts::validate_positional_specs(SPECS) {
                panic!("{}", err);
            }

            $crate::getopts::has_variadic(SPECS)
        };

        let mut _words = unsafe {
            $crate::bash_api::WordListView::from_raw($words_ptr)
        }.into_iter();

        // 1. Required positional args
        $(
            let $req = match _words.next() {
                Some(val) => val,
                None => {
                    $crate::beprintln!($crate::bash_api::this_cmd_name(), ": missing required argument");
                    return $crate::bash_api::EX_USAGE;
                }
            };
        )*

        // 2. Optional positional args
        $(
            $(
                let $opt = _words.next();
            )*
        )?

        // 3. Zero-or-more variadic (*files -> Vec<Cpnt>)
        $(
            let $var_zero = _words.by_ref().collect::<Vec<_>>();
        )?

        // 4. One-or-more variadic (+files -> Vec<Cpnt>)
        $(
            let $var_one = _words.by_ref().collect::<Vec<_>>();
            if $var_one.is_empty() {
                $crate::beprintln!($crate::bash_api::this_cmd_name(), ": missing required argument");
                return $crate::bash_api::EX_USAGE;
            }
        )?

        // 5. Check for trailing extra arguments if non-variadic
        if !HAS_VARIADIC {
            if let Some(_) = _words.next() {
                $crate::beprintln!($crate::bash_api::this_cmd_name(), ": too many arguments");
                return $crate::bash_api::EX_USAGE;
            }
        }

        // Tuple of bound variables
        (
            $( $req, )*
            $( $( $opt, )* )?
            $( $var_zero, )?
            $( $var_one, )?
        )
    }};
}

