use crate::bash_api::{c_char, c_int, Cpnt, WordListView, WORD_LIST};

pub const GETOPT_HELP: c_int = -99;
extern "C" {
    pub fn internal_getopt(list: *mut WORD_LIST, optstring: *const c_char) -> c_int;
    pub fn reset_internal_getopt();
    pub static mut list_optarg: *mut c_char;
    pub static mut loptend: *mut WORD_LIST;
}

/// Check whether `loptend` points to `--help`; if so, call `builtin_usage()`
/// and return `true`. The caller should return `EX_USAGE`.
pub unsafe fn check_help() -> bool {
    if let Some(next) = unsafe { WordListView::from_raw(loptend).current() } {
        if unsafe { next.strcmp("--help\0") } {
            unsafe { crate::bash_api::builtin_usage() };
            return true;
        }
    }
    false
}

/// One item returned by [`BashGetopt::next`].
pub enum GetoptItem {
    /// A valid option character.
    Opt(c_int),
    /// End of options.
    Done,
    /// An error occurred (invalid option, `-h`, or `--help`).
    /// `builtin_usage()` has already been called for `-h`/`--help`.
    Err(c_int),
}

impl GetoptItem {
    /// Unwrap the option character, or call `on_err` with the error code.
    /// `on_err` is expected to return from the enclosing function
    /// (e.g. `|e| return e`).
    pub fn unwrap(self, on_err: impl FnOnce(c_int) -> c_int) -> Option<c_int> {
        match self {
            GetoptItem::Opt(c) => Some(c),
            GetoptItem::Done => None,
            GetoptItem::Err(e) => {
                on_err(e);
                unreachable!()
            }
        }
    }
}

/// Iterator-style wrapper around bash's `internal_getopt`.
///
/// Created by [`BashGetopt::new`], yields [`GetoptItem`] values via
/// [`BashGetopt::next`]. Handles `-h` and `--help` automatically by calling
/// `builtin_usage()`.
pub struct BashGetopt {
    list: *mut WORD_LIST,
    optstring: *const c_char,
    done: bool,
}

impl BashGetopt {
    /// Create a new `BashGetopt` and call `reset_internal_getopt()`.
    pub fn new(list: *mut WORD_LIST, optstring: &[u8]) -> Self {
        unsafe { reset_internal_getopt() };
        BashGetopt {
            list,
            optstring: optstring.as_ptr() as *const c_char,
            done: false,
        }
    }

    /// Return the next option, or `Done`/`Err`.
    pub fn next(&mut self) -> GetoptItem {
        if self.done {
            return GetoptItem::Done;
        }
        if unsafe { check_help() } {
            return GetoptItem::Err(crate::bash_api::EX_USAGE);
        }
        let c = unsafe { internal_getopt(self.list, self.optstring) };
        match c {
            -1 => {
                self.done = true;
                GetoptItem::Done
            }
            GETOPT_HELP => {
                unsafe { crate::bash_api::builtin_usage() };
                GetoptItem::Err(crate::bash_api::EX_USAGE)
            }
            c if c == b'h' as c_int => {
                unsafe { crate::bash_api::builtin_usage() };
                GetoptItem::Err(crate::bash_api::EX_USAGE)
            }
            c => GetoptItem::Opt(c),
        }
    }

    /// Return the optarg for the current option.
    pub fn optarg(&self) -> Cpnt<'static> {
        unsafe { Cpnt::new(list_optarg) }
    }

    /// Return the remaining words after option parsing.
    pub fn remaining(&self) -> *mut WORD_LIST {
        unsafe { loptend }
    }
}

/// Takes WORD_LIST, a function printing help message, and then flag letters and options letters.
/// Returns an object that for flag letters have the letters as members as bool
/// and for options letters has those letters as Option<*c_char>.
#[macro_export]
macro_rules! bash_getopt {
    (
        $list:expr,
        $help_fn:expr,
        [ $( $flag:ident ),* $(,)? ],
        [ $( $opt:ident ),* $(,)? ] $(,)?
    ) => {{
        const OPTSTRING: &[u8] = concat!(
            $( stringify!($flag), )*
            $( stringify!($opt), ":", )*
            "h\0",
        ).as_bytes();
        struct ParsedOpts {
            $( pub $flag: bool, )*
            $( pub $opt: Option<*mut std::os::raw::c_char>, )*
        }
        #[allow(unused_mut)]
        let mut parsed = ParsedOpts {
            $( $flag: false, )*
            $( $opt: None, )*
        };
        $crate::bash_getopt::reset_internal_getopt();
        let list = $list;
        loop {
            let c = unsafe {
                $crate::bash_getopt::internal_getopt(list, OPTSTRING.as_ptr().cast())
            };
            match c {
                -1 => break,
                c if c == b'h' as std::os::raw::c_int => { $help_fn(); return 0; }
                $crate::bash_getopt::GETOPT_HELP => { $help_fn(); return 2; },
                $(
                    c if c == stringify!($flag).as_bytes()[0] as std::os::raw::c_int => {
                        parsed.$flag = true;
                    }
                )*
                $(
                    c if c == stringify!($opt).as_bytes()[0] as std::os::raw::c_int => {
                        parsed.$opt = Some(unsafe { $crate::bash_getopt::list_optarg.cast() });
                    }
                )*
                _ => return EX_USAGE,
            }
        };
        (parsed, $crate::bash_getopt::loptend)
    }};
}

/// Specifies the arity of a positional argument.
///
/// Each variant carries the argument's name as a `&'static str` (produced by
/// [`parse_positional_spec!`] via `stringify!` — zero allocation).
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

/// Validate positional specs: variadic (`*`, `+`) must be last, and no required
/// may follow an optional.
pub fn validate_positional_specs(specs: &[PositionalSpec]) -> Result<(), String> {
    let mut seen_optional = false;
    for (i, spec) in specs.iter().enumerate() {
        match spec {
            PositionalSpec::Required(_) => {
                if seen_optional {
                    return Err(format!(
                        "required positional '{}' cannot follow optional positional",
                        spec.name()
                    ));
                }
            }
            PositionalSpec::Optional(_) => seen_optional = true,
            PositionalSpec::ZeroOrMore(_) | PositionalSpec::OneOrMore(_) => {
                if i != specs.len() - 1 {
                    return Err(format!(
                        "variadic positional '{}' must be the last positional",
                        spec.name()
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Convert a positional-spec token into a [`PositionalSpec`] variant.
///
/// | Syntax   | Variant       | Type produced              |
/// |----------|---------------|----------------------------|
/// | `name`   | `Required`    | `Cpnt`              |
/// | `name?`  | `Optional`    | `Option<Cpnt>`      |
/// | `name*`  | `ZeroOrMore`  | `Vec<Cpnt>`         |
/// | `name+`  | `OneOrMore`   | `Vec<Cpnt>`         |
#[macro_export]
macro_rules! parse_positional_spec {
    ($name:ident) => {
        $crate::bash_getopt::PositionalSpec::Required(stringify!($name))
    };
    ($name:ident ?) => {
        $crate::bash_getopt::PositionalSpec::Optional(stringify!($name))
    };
    ($name:ident *) => {
        $crate::bash_getopt::PositionalSpec::ZeroOrMore(stringify!($name))
    };
    ($name:ident +) => {
        $crate::bash_getopt::PositionalSpec::OneOrMore(stringify!($name))
    };
}

/// Internal helper: parse a single positional value from the remaining-words
/// iterator. Dispatches on the modifier token.
#[macro_export]
macro_rules! bash_getopt2_parse_positional {
    // Required (no modifier)
    ($iter:expr, $name:ident) => {
        let $name = match $iter.next() {
            Some(v) => v,
            None => {
                $crate::beprintln!(
                    "L_builtin: missing required argument '",
                    stringify!($name),
                    "'"
                );
                return $crate::bash_api::EX_USAGE;
            }
        };
    };
    // Optional (`?`)
    ($iter:expr, $name:ident ?) => {
        let $name = $iter.next();
    };
    // Zero-or-more (`*`)
    ($iter:expr, $name:ident *) => {
        let $name: Vec<_> = $iter.collect();
    };
    // One-or-more (`+`)
    ($iter:expr, $name:ident +) => {
        let $name: Vec<_> = $iter.collect();
        if $name.is_empty() {
            $crate::beprintln!(
                "L_builtin: '",
                stringify!($name),
                "' requires at least one argument"
            );
            return $crate::bash_api::EX_USAGE;
        }
    };
}

/// Declarative option + positional parser backed by bash's `internal_getopt`.
///
/// Automatically adds `-h` which calls bash's `builtin_usage()` and returns
/// `EX_USAGE`. The `-h` flag is appended to the optstring automatically — do
/// not add it manually.
///
/// # Syntax
///
/// ```ignore
/// bash_getopt2!(
///     $word_list,                          // *mut WORD_LIST
///     options: [
///         e: => |val| env_var = Some(val), // option taking an argument (: suffix)
///         f  => || force = true,           // flag (no colon)
///     ],
///     positionals: [
///         src,                               // Required
///         dest?,                             // Optional
///         files*,                            // Zero or more
///     ]
/// );
/// ```
///
/// **Option actions** for flags (`f`) are called with no arguments (`||`).
/// For value options (`e:`), the action receives `Cpnt` (the optarg).
///
/// **Positional modifiers** control arity:
/// - `<ident>`   — required (missing → `EX_USAGE`)
/// - `<ident>?`  — optional, produces `Option<Cpnt>`
/// - `<ident>*`  — zero or more, produces `Vec<Cpnt>`
/// - `<ident>+`  — one or more (empty → `EX_USAGE`), produces `Vec<Cpnt>`
///
/// Returns a tuple `(arg1, arg2, ...)` of the positional values.
///
/// On error prints to stderr and returns `EX_USAGE` from the enclosing function.
#[macro_export]
macro_rules! bash_getopt2 {
    (
        $args:expr,
        options: [ $( $opt:ident $( : $colon:tt )? => $action:expr ),* $(,)? ],
        positionals: [ $( $pname:ident $( $pmod:tt )? ),* $(,)? ]
    ) => {{
        const OPTSTRING: &[u8] = concat!(
            $( stringify!($opt), $( stringify!($colon), )? )* "h\0"
        ).as_bytes();

        let mut _bg = $crate::bash_getopt::BashGetopt::new($args, OPTSTRING);
        while let Some(c) = _bg.next().unwrap(|e| return e) {
            match c {
                $(
                    c if c == stringify!($opt).as_bytes()[0] as std::os::raw::c_int => {
                        $crate::bash_getopt2_call_action!(
                            $action, _bg.optarg(), $( $colon )?
                        );
                    }
                )*
                _ => return $crate::bash_api::EX_USAGE,
            }
        }

        // Validate positional specs at runtime
        let specs = [$($crate::parse_positional_spec!($pname $( $pmod )?)),*];
        if let Err(e) = $crate::bash_getopt::validate_positional_specs(&specs) {
            $crate::beprintln!("L_builtin: {}", e);
            return $crate::bash_api::EX_USAGE;
        }

        // Parse positional arguments from remaining words
        let mut _pos_words = unsafe {
            $crate::bash_api::WordListView::from_raw(_bg.remaining())
        }
        .into_iter();

        $(
            $crate::bash_getopt2_parse_positional!(_pos_words, $pname $( $pmod )?);
        )*

        ($($pname),*)
    }};
}

/// Internal helper: call the action with or without the optarg.
#[macro_export]
macro_rules! bash_getopt2_call_action {
    // Value option (has colon): pass optarg
    ($action:expr, $optarg:expr, :) => {
        $action($optarg);
    };
    // Flag (no colon): call without argument
    ($action:expr, $optarg:expr,) => {
        $action();
    };
}
