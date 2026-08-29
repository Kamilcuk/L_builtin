//! `L_builtin sedvar VAR SCRIPT`
//!
//! Run a sed script ([`sed_rs`]) over a bash variable, in place, entirely
//! in-process (no subprocess, no memfd, no duplicated file descriptors).
//!
//! Elements are streamed into the script as separate records:
//!
//! - For an indexed array, every element is one NUL-delimited record fed into a
//!   single script run; the script's printed records become the new array
//!   elements. Because records are NUL-delimited (`sed -z`), an element value
//!   that contains a newline stays a single element, and the script's
//!   line/record addressing (`1`, `$`, `2,3`, `/re/`) maps directly onto array
//!   elements.
//! - For an associative array, each value is transformed independently (the key
//!   is preserved).
//! - For a scalar, the whole value is one record; if the script prints exactly
//!   one record the variable stays scalar, otherwise it is rebuilt as an array.
//!
//! The result is written straight back into the variable. Note that `sed-rs` is
//! UTF-8 oriented, so values that are not valid UTF-8 are lossily decoded.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::{CStr, CString};

use cmdargs_derive::CmdArgs;

use crate::bash_api::{
    array_insert, arrayind_t, find_variable, l_array_cell, l_array_p, l_assoc_cell, l_assoc_insert,
    l_assoc_p, l_prepare_indexed_array, l_value_cell, ArrayIterator, AssocIterator, SHELL_VAR,
    WORD_LIST,
};
use crate::cmdargs::BashVar;
use crate::subcmd::{CmdDesc, CmdResult};
use crate::{l_builtin_error, l_builtin_usage_error};
use sed_rs::Sed;

/// `sedvar VAR SCRIPT`: run SCRIPT (a sed program) over VAR's value(s).
#[derive(CmdArgs)]
struct SedvarArgs {
    /// Bash variable whose value(s) are transformed.
    #[positional]
    var: BashVar,
    /// A sed script (e.g. `s/foo/bar/g`, `/^#/d`, `2,3p`).
    #[positional]
    script: &'static str,
}

const SEDVAR_CMD: CmdDesc = CmdDesc::new(
    c"sedvar",
    c"sedvar VAR SCRIPT",
    c"\
Run a sed script SCRIPT (a GNU-compatible sed program, see `sed-rs`) over the
bash variable VAR, in place, without spawning a subprocess.

For an indexed array, every element is streamed into SCRIPT as a separate record
(records are NUL-delimited, like `sed -z`), and the script's printed records
become the new array elements. This means an element value containing a newline
stays a single element, and the script's addressing (1, $, 2,3, /re/) applies to
array elements. For an associative array, each value is transformed independently
(its key is preserved). For a scalar, the value is one record: if the script
prints exactly one record the variable stays scalar, otherwise it becomes an
array.

The transformation runs in memory on the variable's current values; the result
is assigned back into VAR.

Examples:
  arr=( foo bar ); L_builtin sedvar arr 's/a/@/'
  L_builtin sedvar arr '/bar/d'      # drop the element matching 'bar'
  v=hello; L_builtin sedvar v 's/hello/world/'
",
);

/// Split sed-rs NUL-delimited output into records (dropping the trailing empty
/// segment that follows the final record separator).
fn split_records(out: String) -> Vec<CString> {
    let bytes = out.into_bytes();
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut recs: Vec<&[u8]> = bytes.split(|&b| b == 0).collect();
    if bytes.ends_with(b"\0") {
        recs.pop();
    }
    recs
        .into_iter()
        .map(|r| CString::new(r).unwrap_or_default())
        .collect()
}

/// Run `sed` on a single value (one NUL-delimited record) and return the first
/// output record (the transformed value).
fn transform_one(sed: &Sed, value: &[u8]) -> Result<CString, c_int> {
    let out = sed
        .eval_bytes(value)
        .map_err(|e| l_builtin_error!(b"sedvar: ", e.to_string()))?;
    Ok(split_records(out).into_iter().next().unwrap_or_default())
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn sedvar_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SEDVAR_CMD.enter();
    let args = SedvarArgs::parse(list)?;
    let name = args.var.as_ptr();
    let shellvar = find_variable(name);
    if shellvar.is_null() {
        return Err(l_builtin_error!(b"sedvar: variable not found: ", name));
    }
    let mut sed = Sed::new(args.script)
        .map_err(|e| l_builtin_usage_error!(b"sedvar: invalid script: ", e.to_string()))?;
    sed.null_data(true);

    if l_assoc_p(shellvar) != 0 {
        let hash = l_assoc_cell(shellvar as *mut SHELL_VAR);
        for (k, v) in AssocIterator::new(hash) {
            let new = transform_one(&sed, v.to_bytes())?;
            l_assoc_insert(hash, k.as_ptr(), new.as_ptr());
        }
    } else {
        let is_array = l_array_p(shellvar) != 0;
        let mut input: Vec<u8> = Vec::new();
        if is_array {
            let arr = l_array_cell(shellvar as *mut SHELL_VAR);
            for (_idx, v) in ArrayIterator::new(arr) {
                input.extend_from_slice(v.to_bytes());
                input.push(0);
            }
        } else {
            let val = l_value_cell(shellvar as *mut SHELL_VAR);
            let bytes = if val.is_null() {
                b""
            } else {
                CStr::from_ptr(val).to_bytes()
            };
            input.extend_from_slice(bytes);
            input.push(0);
        }
        let out = sed
            .eval_bytes(&input)
            .map_err(|e| l_builtin_error!(b"sedvar: ", e.to_string()))?;
        let records = split_records(out);
        if !is_array && records.len() <= 1 {
            args.var.set(records.into_iter().next().unwrap_or_default().as_ptr())?;
        } else {
            let arr = l_prepare_indexed_array(name);
            if arr.is_null() {
                return Err(l_builtin_error!(b"sedvar: failed to prepare array"));
            }
            for (i, rec) in records.into_iter().enumerate() {
                array_insert(arr, i as arrayind_t, rec.as_ptr() as *mut ::std::os::raw::c_char);
            }
        }
    }
    Ok(())
}
