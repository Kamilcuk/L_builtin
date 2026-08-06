//! Lua subcommand implementation using mlua
use crate::bash_api::{
    expand_string, l_check_unbind_variable, l_expand_string_to_string_in_quotes, CStringOwned,
    WordListOwned, WordListView,
};
use crate::bprint_bytes::BDisplay;
use crate::{bash_getopt, beprintln};
use crate::{bprintln, return_on_err};

use std::ffi::{c_char, CStr};
use std::io::Write;
use std::os::raw::c_int;

use mlua::{Lua, Value};

use crate::bash_api::{
    array_flush, array_insert, assoc_flush, assoc_keys_to_word_list, assoc_reference,
    bind_variable, convert_var_to_array, find_variable, l_array_cell, l_array_head, l_array_p,
    l_assoc_cell, l_assoc_insert, l_assoc_p, l_element_forw, l_element_index, l_element_value,
    l_invisible_p, l_readonly_p, l_value_cell, make_new_array_variable, make_new_assoc_variable,
    EX_USAGE, SHELL_VAR, WORD_LIST,
};

#[cfg(not(feature = "bash_lt_4_3"))]
use crate::bash_api::l_execute_command_string;

impl BDisplay for mlua::BorrowedBytes<'_> {
    fn bwrite<W: Write + ?Sized>(&self, w: &mut W) {
        w.write_all(self).ok();
    }
}

impl BDisplay for mlua::String {
    fn bwrite<W: Write + ?Sized>(&self, w: &mut W) {
        w.write_all(self.as_bytes().as_ref()).ok();
    }
}

impl BDisplay for mlua::Error {
    fn bwrite<W: Write + ?Sized>(&self, w: &mut W) {
        w.write_all(self.to_string().as_bytes()).ok();
    }
}
const ENAME: &str = "L_builtin lua";

fn print_lua_help() {
    bprintln!(
        "\
Usage: L_builtin lua [-v VAR] <script> [args...]

Run a Lua script in-process, with access to a bash.* API.

Options:
  -v VAR, --var VAR    Bind the script's return value to shell variable VAR
  -h, --help          Show this help and exit

Arguments:
  script              Lua script: inline code, or a file path
  args...             Arguments exposed to the script via the Lua 'arg' table
"
    );
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn l_lua_subcommand(list: *mut WORD_LIST) -> c_int {
    let (opts, args) = bash_getopt!(list, print_lua_help, [], [v]);
    let store = unsafe { WordListView::from_raw(args) };
    let mut args = store.iter_bytes();
    let script = match args.next() {
        Some(script) => script,
        None => {
            // No script was given — only options (or nothing).
            beprintln!(ENAME, ": missing script");
            beprintln!("Usage: L_builtin lua [-v VAR] <script> [args...]");
            return EX_USAGE;
        }
    };
    let lua = Lua::new();
    let return_value = return_on_err!(ENAME, run_lua_script(&lua, script, args), 1);
    if let Some(var_name) = opts.v {
        return_on_err!(
            ENAME,
            set_bash_from_lua_in(&lua, var_name, return_value, None),
            1
        );
    }
    0
}

/// Run a Lua script and return its result as an mlua::Value.
/// The Lua instance must stay alive for the returned Value to remain valid.
fn run_lua_script<'a>(
    lua: &'a Lua,
    script: &'a [u8],
    args: impl Iterator<Item = &'a [u8]>,
) -> Result<mlua::Value, mlua::Error> {
    // Register bash API functions
    register_bash_api(lua)?;
    // Set up arg table
    let arg_table = lua.create_table()?;
    for (i, arg) in args.enumerate() {
        let s = lua.create_string(arg)?;
        arg_table.set(i + 1, s)?;
    }
    lua.globals().set("arg", arg_table)?;
    // Execute the script using safe API (handles stack, errors, callbacks)
    lua.load(script).eval()
}

/// Validate the optional index-base argument of bash.get/bash.set.
fn parse_base(base: Option<i64>) -> Result<i64, mlua::Error> {
    match base {
        None => Ok(1),
        Some(b @ (0 | 1)) => Ok(b),
        Some(b) => Err(mlua::Error::RuntimeError(format!(
            "base must be 0 or 1, got {b}"
        ))),
    }
}

enum ScalarCstr<'a> {
    Lua(mlua::BorrowedBytes<'a>),
    Vec(Vec<u8>),
    Arr(&'a [u8]),
}

impl<'a> ScalarCstr<'a> {
    pub fn as_ptr(&self) -> *const c_char {
        let r = match self {
            ScalarCstr::Lua(b) => b.as_ref(), // derefs BorrowedBytes to &[u8]
            ScalarCstr::Vec(v) => v.as_slice(),
            ScalarCstr::Arr(a) => a,
        };
        debug_assert!(
            !r.is_empty() && r.last() == Some(&0),
            "ScalarCstr slice must be non-empty and null-terminated, found: {:?}",
            r
        );
        r.as_ptr().cast()
    }
}

/// Convert a scalar Lua value to the bytes bash should store.
///
/// boolean -> "true"/"false"; number -> decimal text (integral floats print
/// without a fraction, like Lua's tostring); string -> raw bytes. Everything
/// else is a type error, per the API contract.
fn scalar_to_bytes<'a>(v: &'a Value) -> Result<ScalarCstr<'a>, mlua::Error> {
    match v {
        Value::Boolean(true) => Ok(ScalarCstr::Arr(b"true\0")),
        Value::Boolean(false) => Ok(ScalarCstr::Arr(b"false\0")),
        Value::Integer(i) => Ok(ScalarCstr::Vec({
            let mut v = i.to_string().into_bytes();
            v.push(0);
            v
        })),
        Value::Number(n) => Ok(ScalarCstr::Vec(format!("{}\0", *n).into_bytes())),
        Value::String(s) => Ok(ScalarCstr::Lua(s.as_bytes_with_nul())),
        other => Err(mlua::Error::RuntimeError(format!(
            "cannot convert Lua {} to a bash value",
            other.type_name()
        ))),
    }
}

/// Fill (or create) the indexed array `name` from a Lua table, shifting the
/// table's keys down by `base`. `var` is the existing variable or null.
unsafe fn set_array_from_table(
    _lua: &Lua,
    name: *const c_char,
    var: *mut SHELL_VAR,
    table: &mlua::Table,
    base: i64,
) -> Result<Value, mlua::Error> {
    let var = if !var.is_null() && l_array_p(var) != 0 {
        var
    } else if !var.is_null() {
        // Scalar: convert in place, which preserves the variable's scope
        // (unbind + create would drop a `local` and make a shadowed global).
        convert_var_to_array(var)
    } else {
        make_new_array_variable(name)
    };
    if var.is_null() {
        return Err(mlua::Error::RuntimeError(
            "array creation failed".to_string(),
        ));
    }
    let array = l_array_cell(var);
    if array.is_null() {
        return Err(mlua::Error::RuntimeError(
            "internal error: no array cell".to_string(),
        ));
    }
    array_flush(array);
    for pair in table.pairs::<Value, Value>() {
        let (k, v) = pair?;
        let idx = table_key_to_index(&k)? - base;
        array_insert(array, idx, scalar_to_bytes(&v)?.as_ptr());
    }
    Ok(Value::Boolean(true))
}

/// Fill the existing associative array `var` from a Lua table, or create a new
/// one if `var` is null. Keys may be strings or numbers; both keys and values
/// go through scalar conversion.
unsafe fn set_assoc_from_table(
    name: *const c_char,
    var: *mut SHELL_VAR,
    table: &mlua::Table,
) -> Result<Value, mlua::Error> {
    let var = if !var.is_null() && l_assoc_p(var) != 0 {
        var
    } else if !var.is_null() {
        // Existing variable but not associative - this shouldn't happen if
        // callers check l_assoc_p first, but handle gracefully.
        return Err(mlua::Error::RuntimeError(
            "variable exists but is not associative".to_string(),
        ));
    } else {
        make_new_assoc_variable(name)
    };
    if var.is_null() {
        return Err(mlua::Error::RuntimeError(
            "associative array creation failed".to_string(),
        ));
    }
    let hash = l_assoc_cell(var);
    if hash.is_null() {
        return Err(mlua::Error::RuntimeError(
            "internal error: no assoc cell".to_string(),
        ));
    }
    assoc_flush(hash);
    for pair in table.pairs::<Value, Value>() {
        let (k, v) = pair?;
        l_assoc_insert(
            hash,
            scalar_to_bytes(&k)?.as_ptr(),
            scalar_to_bytes(&v)?.as_ptr(),
        );
    }
    Ok(Value::Boolean(true))
}

// bash.get(var_name [, base]) -> string (scalar), table (array/assoc), or nil
//
// Indexed arrays become tables whose keys are the bash indices shifted by
// `base` (default 1, may be 0). Sparse arrays keep their holes; no dense
// renumbering. Associative arrays become string-keyed tables (base is
// irrelevant).
fn get_bash_from_lua(
    lua: &mlua::Lua,
    (name, base): (mlua::String, Option<i64>),
) -> mlua::Result<mlua::Value> {
    let name = name.as_bytes_with_nul();
    let base = parse_base(base)?;
    unsafe {
        let var = find_variable(name.as_ptr().cast());
        if var.is_null() || l_invisible_p(var) != 0 {
            return Ok(Value::Nil);
        }
        if l_array_p(var) != 0 {
            let table = lua.create_table()?;
            let array = l_array_cell(var);
            if !array.is_null() {
                let head = l_array_head(array);
                if !head.is_null() {
                    // Circular doubly-linked list with `head` as sentinel.
                    let mut curr = l_element_forw(head);
                    while curr != head {
                        let val = l_element_value(curr);
                        let s = lua.create_string(if val.is_null() {
                            b""
                        } else {
                            CStr::from_ptr(val).to_bytes()
                        })?;
                        table.set(l_element_index(curr) + base, s)?;
                        curr = l_element_forw(curr);
                    }
                }
            }
            return Ok(Value::Table(table));
        }
        if l_assoc_p(var) != 0 {
            let table = lua.create_table()?;
            let hash = l_assoc_cell(var);
            if !hash.is_null() {
                for key in &WordListOwned(assoc_keys_to_word_list(hash)) {
                    let val = assoc_reference(hash, key.as_ptr());
                    let k = lua.create_string(key.to_bytes())?;
                    let v = lua.create_string(if val.is_null() {
                        b""
                    } else {
                        CStr::from_ptr(val).to_bytes()
                    })?;
                    table.set(k, v)?;
                }
            }
            return Ok(Value::Table(table));
        }
        let val = l_value_cell(var);
        if val.is_null() {
            Ok(Value::Nil)
        } else {
            Ok(Value::String(
                lua.create_string(CStr::from_ptr(val).to_bytes())?,
            ))
        }
    }
}

fn set_bash_from_lua_in(
    lua: &Lua,
    name: *const c_char,
    value: mlua::Value,
    base: Option<i64>,
) -> mlua::Result<mlua::Value> {
    let base = parse_base(base)?;
    unsafe {
        let var = find_variable(name);
        if !var.is_null() && l_readonly_p(var) != 0 {
            return Err(mlua::Error::RuntimeError("readonly variable".to_string()));
        }
        match value {
            Value::Table(table) => {
                // Determine if the table should be an associative array or indexed array
                // If the variable already exists and is associative, use that.
                // Otherwise, check if the table has any string keys.
                let is_assoc = if !var.is_null() && l_assoc_p(var) != 0 {
                    true
                } else {
                    table_has_non_integer_keys(&table)?
                };
                if is_assoc {
                    set_assoc_from_table(name, var, &table)
                } else {
                    set_array_from_table(lua, name, var, &table, base)
                }
            }
            other => {
                let result = bind_variable(name, scalar_to_bytes(&other)?.as_ptr(), 0);
                Ok(Value::Boolean(!result.is_null()))
            }
        }
    }
}

/// Check if a Lua table has any string keys (indicating it should be an associative array)
fn table_has_non_integer_keys(table: &mlua::Table) -> mlua::Result<bool> {
    for pair in table.pairs::<Value, Value>() {
        let (k, _) = pair?;
        match k {
            Value::Integer(_) => continue,
            Value::Number(n) if n.fract() == 0.0 => continue, // integral float like 1.0
            _ => return Ok(true),
        }
    }
    Ok(false)
}

/// Read a Lua table key as a bash array index (integers only).
fn table_key_to_index(k: &Value) -> Result<i64, mlua::Error> {
    match k {
        Value::Integer(i) => Ok(*i),
        Value::Number(n) if n.fract() == 0.0 => Ok(*n as i64),
        other => Err(mlua::Error::RuntimeError(format!(
            "indexed array keys must be integers, got {}",
            other.type_name()
        ))),
    }
}

/// Register bash API functions with Lua
fn register_bash_api(lua: &Lua) -> Result<(), mlua::Error> {
    let globals = lua.globals();
    let bash_module = lua.create_table()?;
    // bash.set(var_name, value [, base]) -> boolean
    //
    // boolean/number/string set a scalar ("true"/"false" for booleans,
    // decimal text for numbers). A table sets an associative array when the
    // existing bash variable is associative, an indexed array otherwise
    // (table keys are shifted by `base`, default 1, may be 0). Any other Lua
    // type raises an error.
    bash_module.set("get", lua.create_function(get_bash_from_lua)?)?;
    bash_module.set(
        "set",
        lua.create_function(
            |lua, (name, value, base): (mlua::String, Value, Option<i64>)| {
                let name = name.as_bytes_with_nul();
                set_bash_from_lua_in(lua, name.as_ptr().cast(), value, base)
            },
        )?,
    )?;
    // bash.unset(var_name) -> boolean
    //
    // true if the variable was removed, false if it did not exist. Raises an
    // error for readonly variables (bash also prints its own diagnostic).
    bash_module.set(
        "unset",
        lua.create_function(|_, sname: mlua::String| unsafe {
            let name = sname.as_bytes_with_nul();
            match l_check_unbind_variable(name.as_ptr().cast()) {
                10000 => Err(mlua::Error::RuntimeError(format!(
                    "cannot unset: readonly: {}",
                    sname.display()
                ))),
                10001 => Err(mlua::Error::RuntimeError(format!(
                    "{}: cannot unset",
                    sname.display()
                ))),
                0 => Ok(Value::Boolean(true)),
                -1 => Ok(Value::Boolean(false)),
                err => Err(mlua::Error::RuntimeError(format!(
                    "{}: cannot unset variable: {}",
                    sname.display(),
                    err
                ))),
            }
        })?,
    )?;
    // bash.eval(command_string) -> integer
    //
    // Executes the command string using bash's parser (like eval).
    // Disabled on bash < 4.3 due to missing l_execute_command_string.
    #[cfg(not(feature = "bash_lt_4_3"))]
    bash_module.set(
        "eval",
        lua.create_function(|_lua, cmd: mlua::String| unsafe {
            let cmd = cmd.as_bytes_with_nul();
            let result = l_execute_command_string(cmd.as_ptr().cast());
            Ok(Value::Integer(result as i64))
        })?,
    )?;
    // bash.expand(string) -> string
    bash_module.set(
        "expand",
        lua.create_function(|lua, s: mlua::String| unsafe {
            let s = s.as_bytes_with_nul();
            let result = CStringOwned(l_expand_string_to_string_in_quotes(s.as_ptr().cast()));
            Ok(Value::String(lua.create_string(result.to_bytes())?))
        })?,
    )?;
    // bash.expand_list(string) -> table
    bash_module.set(
        "expand_list",
        lua.create_function(|lua, s: mlua::String| unsafe {
            let s = s.as_bytes_with_nul();
            let table = lua.create_table()?;
            for (idx, word) in WordListOwned(expand_string(s.as_ptr().cast(), 0))
                .into_iter()
                .enumerate()
            {
                table.set(idx + 1, lua.create_string(word.to_bytes())?)?;
            }
            Ok(Value::Table(table))
        })?,
    )?;
    globals.set("bash", bash_module)?;
    Ok(())
}
