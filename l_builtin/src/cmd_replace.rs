//! `L_builtin replace VAR PATTERN REPLACEMENT`
//!
//! Apply a Rust `regex` substitution to the values of a bash variable, in place.
//! This is a self-contained subcommand built on the `regex` crate (byte mode); it
//! has nothing to do with the `shm` subcommand family.
//!
//! For a scalar variable the whole value is transformed; for an indexed or
//! associative array every element value is transformed. The substitution is
//! global (every match is replaced) and runs on raw bytes, so variable values
//! that are not valid UTF-8 are handled correctly.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::{CStr, CString};
use std::result::Result;

use cmdargs_derive::CmdArgs;

use crate::bash_api::{
    array_insert, find_variable, l_array_cell, l_array_p, l_assoc_cell, l_assoc_insert, l_assoc_p,
    l_value_cell, ArrayIterator, AssocIterator, SHELL_VAR, WORD_LIST,
};
use crate::cmdargs::BashVar;
use crate::subcmd::{CmdDesc, CmdResult};
use crate::{l_builtin_error, l_builtin_usage_error};

/// `replace VAR PATTERN REPLACEMENT`: replace every regex match in VAR's value(s).
#[derive(CmdArgs)]
struct ReplaceArgs {
    /// Bash variable whose value(s) are transformed.
    #[positional]
    var: BashVar,
    /// Rust `regex` pattern matched against each value.
    #[positional]
    pattern: &'static str,
    /// Replacement string; `$1`/`${name}` reference capture groups, an unescaped
    /// `$` is a literal (use `$$` for a literal dollar sign).
    #[positional]
    replacement: &'static CStr,
}

const REPLACE_CMD: CmdDesc = CmdDesc::new(
    c"replace",
    c"replace VAR PATTERN REPLACEMENT",
    c"\
Apply a regular-expression substitution to the values of the bash variable VAR,
replacing every match of PATTERN with REPLACEMENT. PATTERN is a Rust `regex`
pattern (https://docs.rs/regex); REPLACEMENT may reference capture groups via
$1, ${name}, and an unescaped $ is a literal (use $$ for a literal dollar sign).

For a scalar VAR the whole value is transformed; for an indexed or associative
array every element value is transformed in place. The substitution runs in byte
mode (regex::bytes), so values that are not valid UTF-8 are handled correctly.
Replacement is global: every match within each value is replaced.

Examples:
  v='foobar'; L_builtin replace v 'o' '0'   # v becomes 'f00bar'
  arr=( foo bar ); L_builtin replace arr 'a' '@'
",
);

/// Replace every match of `re` in `value` with `replacement`, returning a new
/// `CString`. Errors if the result would contain a NUL byte (impossible to store
/// in a bash value).
fn replaced(value: &[u8], re: &regex::bytes::Regex, replacement: &[u8]) -> Result<CString, c_int> {
    CString::new(re.replace_all(value, replacement).into_owned())
        .map_err(|_| l_builtin_error!(b"replace: replacement introduced a NUL byte"))
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn replace_subcommand(list: *mut WORD_LIST) -> CmdResult {
    REPLACE_CMD.enter();
    let args = ReplaceArgs::parse(list)?;
    let name = args.var.as_ptr();
    let shellvar = find_variable(name);
    if shellvar.is_null() {
        return Err(l_builtin_error!(b"replace: variable not found: ", name));
    }
    let re = regex::bytes::Regex::new(args.pattern)
        .map_err(|e| l_builtin_usage_error!(b"replace: invalid pattern: ", e.to_string()))?;
    let replacement = args.replacement.to_bytes();

    if l_assoc_p(shellvar) != 0 {
        let hash = l_assoc_cell(shellvar as *mut SHELL_VAR);
        for (k, v) in AssocIterator::new(hash) {
            let new = replaced(v.to_bytes(), &re, replacement)?;
            l_assoc_insert(hash, k.as_ptr(), new.as_ptr());
        }
    } else if l_array_p(shellvar) != 0 {
        let arr = l_array_cell(shellvar as *mut SHELL_VAR);
        for (idx, v) in ArrayIterator::new(arr) {
            let new = replaced(v.to_bytes(), &re, replacement)?;
            array_insert(arr, idx, new.as_ptr() as *mut ::std::os::raw::c_char);
        }
    } else {
        let val = l_value_cell(shellvar as *mut SHELL_VAR);
        let bytes = if val.is_null() {
            b""
        } else {
            CStr::from_ptr(val).to_bytes()
        };
        let new = replaced(bytes, &re, replacement)?;
        args.var.set(new.as_ptr())?;
    }
    Ok(())
}
