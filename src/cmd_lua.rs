//! Lua subcommand implementation using mlua
use crate::bash_api::{WordListIter, WordListIterOsStr, WordListOwned, WordListView, expand_string};
use crate::beprintln;
use crate::bprint_bytes::BDisplay;
use crate::bprintln;
use crate::return_on_err;
use crate::shared::{bind_shell_variable, lexopt_unexpected};

use std::ffi::{CStr, OsStr, OsString};
use std::io::Write;
use std::os::raw::c_int;
use std::os::unix::ffi::OsStrExt;

use lexopt::{prelude::*, Parser};
use mlua::{Lua, Value};

use crate::bash_api::{
    array_flush, array_insert, assoc_flush, assoc_keys_to_word_list, assoc_reference,
    bind_variable, check_unbind_variable, convert_var_to_array, execute_shell_function,
    find_function, find_variable, l_array_cell, l_array_head, l_array_p, l_assoc_cell,
    l_assoc_insert, l_assoc_p, l_element_forw, l_element_index, l_element_value,
    l_expand_string_to_string_in_quotes_owned, l_invisible_p, l_readonly_p, l_value_cell,
    make_new_array_variable, make_word, make_word_list, EX_USAGE, SHELL_VAR, WORD_LIST,
};

impl BDisplay for mlua::Error {
    fn bwrite<W: Write + ?Sized>(&self, w: &mut W) {
        w.write_all(self.to_string().as_bytes()).unwrap();
    }
}
const ENAME: &str = "L_builtin lua";

#[no_mangle]
pub extern "C" fn l_lua_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut list = unsafe { WordListView::from_raw(list) }.iter();
    let mut listosstr = list.by_ref().map(OsStr::from_bytes);
    let mut parser = Parser::from_args(&mut listosstr);
    let mut ret_var: Option<OsString> = None;
    while let Some(arg) = return_on_err!(ENAME, parser.next(), EX_USAGE) {
        match arg {
            Short('v') | Long("var") => {
                ret_var = Some(return_on_err!(ENAME, parser.value(), EX_USAGE));
            }
            Short('h') | Long("help") => {
                print_lua_help();
                return 0;
            }
            Value(val) => {
                let return_value = return_on_err!(ENAME, run_lua_script(&val, list), 1);
                if let Some(var_name) = ret_var {
                    return_on_err!(
                        ENAME,
                        bind_shell_variable(&var_name, return_value.into_bytes()),
                        1
                    );
                }
                return 0;
            }
            _ => {
                return lexopt_unexpected(ENAME, arg);
            }
        }
    }
    // No script was given — only options (or nothing).
    beprintln!(ENAME, b": missing script");
    beprintln!(b"Usage: L_builtin lua [-v VAR] <script> [args...]");
    return 2;
}

/// Print usage to stdout (matching the `-h`/`--help` contract the tests expect).
fn print_lua_help() {
    bprintln!(b"Usage: L_builtin lua [-v VAR] <script> [args...]");
    bprintln!(b"");
    bprintln!(b"Run a Lua script in-process, with access to a bash.* API.");
    bprintln!(b"");
    bprintln!(b"Options:");
    bprintln!(b"  -v VAR, --var VAR   Bind the script's return value to shell variable VAR");
    bprintln!(b"  -h, --help          Show this help and exit");
    bprintln!(b"");
    bprintln!(b"Arguments:");
    bprintln!(b"  script              Lua script: inline code, or a file path");
    bprintln!(b"  args...             Arguments exposed to the script via the Lua 'arg' table");
}

/// Run a Lua script and return its string result, or the underlying `mlua`
/// error. Callers map the error to a message + exit code in one place.
fn run_lua_script(script: &OsStr, args: WordListIter) -> Result<String, mlua::Error> {
    let lua = Lua::new();
    // Register bash API functions
    register_bash_api(&lua)?;
    // Set up arg table
    let arg_table = lua.create_table()?;
    for (i, arg) in args.enumerate() {
        let s = lua.create_string(arg)?;
        arg_table.set(i + 1, s)?;
    }
    lua.globals().set("arg", arg_table)?;
    // Execute the script and get the return value (similar to luaL_dostring in C)
    let chunk = lua.load(script.as_bytes());
    let result: mlua::Value = chunk.eval()?;
    // Convert the return value to a string (similar to lua_tostring in C)
    Ok(result.to_string()?)
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

/// Convert a scalar Lua value to the bytes bash should store.
///
/// boolean -> "true"/"false"; number -> decimal text (integral floats print
/// without a fraction, like Lua's tostring); string -> raw bytes. Everything
/// else is a type error, per the API contract.
fn scalar_to_bytes(v: &Value) -> Result<Vec<u8>, mlua::Error> {
    match v {
        Value::Boolean(b) => Ok(if *b {
            b"true".to_vec()
        } else {
            b"false".to_vec()
        }),
        Value::Integer(i) => Ok(i.to_string().into_bytes()),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                Ok(format!("{}", *n as i64).into_bytes())
            } else {
                Ok(format!("{n}").into_bytes())
            }
        }
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(mlua::Error::RuntimeError(format!(
            "cannot convert Lua {} to a bash value",
            other.type_name()
        ))),
    }
}

/// Read a Lua table key as a bash array index (integers only).
fn table_key_to_index(k: &Value) -> Result<i64, mlua::Error> {
    match k {
        Value::Integer(i) => Ok(*i),
        Value::Number(n) if n.fract() == 0.0 && n.is_finite() => Ok(*n as i64),
        other => Err(mlua::Error::RuntimeError(format!(
            "indexed array keys must be integers, got {}",
            other.type_name()
        ))),
    }
}

/// Fill (or create) the indexed array `name` from a Lua table, shifting the
/// table's keys down by `base`. `var` is the existing variable or null.
unsafe fn set_array_from_table(
    name: *const std::os::raw::c_char,
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
        let mut bytes = scalar_to_bytes(&v)?;
        bytes.push(0);
        array_insert(array, idx, bytes.as_ptr().cast());
    }
    Ok(Value::Boolean(true))
}

/// Fill the existing associative array `var` from a Lua table. Keys may be
/// strings or numbers; both keys and values go through scalar conversion.
unsafe fn set_assoc_from_table(
    var: *mut SHELL_VAR,
    table: &mlua::Table,
) -> Result<Value, mlua::Error> {
    let hash = l_assoc_cell(var);
    if hash.is_null() {
        return Err(mlua::Error::RuntimeError(
            "internal error: no assoc cell".to_string(),
        ));
    }
    assoc_flush(hash);
    for pair in table.pairs::<Value, Value>() {
        let (k, v) = pair?;
        let mut key = scalar_to_bytes(&k)?;
        key.push(0);
        let mut val = scalar_to_bytes(&v)?;
        val.push(0);
        l_assoc_insert(hash, key.as_ptr().cast(), val.as_ptr().cast());
    }
    Ok(Value::Boolean(true))
}

/// Register bash API functions with Lua
fn register_bash_api(lua: &Lua) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // Create bash module
    let bash_module = lua.create_table()?;

    // bash.get(var_name [, base]) -> string (scalar), table (array/assoc), or nil
    //
    // Indexed arrays become tables whose keys are the bash indices shifted by
    // `base` (default 1, may be 0). Sparse arrays keep their holes; no dense
    // renumbering. Associative arrays become string-keyed tables (base is
    // irrelevant).
    bash_module.set(
        "get",
        lua.create_function(|lua, (name, base): (mlua::String, Option<i64>)| {
            let base = parse_base(base)?;
            unsafe {
                let name = name.as_bytes_with_nul();
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
                                let s = if val.is_null() {
                                    lua.create_string("")?
                                } else {
                                    lua.create_string(CStr::from_ptr(val).to_bytes())?
                                };
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
                            // The slice borrows the C string in place and excludes
                            // its NUL, which is still present just past the end —
                            // so the pointer is a valid C string for lookup.
                            let val = assoc_reference(hash, key.as_ptr().cast());
                            let k = lua.create_string(key)?;
                            let v = if val.is_null() {
                                lua.create_string("")?
                            } else {
                                lua.create_string(CStr::from_ptr(val).to_bytes())?
                            };
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
        })?,
    )?;

    // bash.set(var_name, value [, base]) -> boolean
    //
    // boolean/number/string set a scalar ("true"/"false" for booleans,
    // decimal text for numbers). A table sets an associative array when the
    // existing bash variable is associative, an indexed array otherwise
    // (table keys are shifted by `base`, default 1, may be 0). Any other Lua
    // type raises an error.
    bash_module.set(
        "set",
        lua.create_function(
            |_, (name, value, base): (mlua::String, Value, Option<i64>)| {
                let base = parse_base(base)?;
                unsafe {
                    let name = name.as_bytes_with_nul();
                    let var = find_variable(name.as_ptr().cast());
                    if !var.is_null() && l_readonly_p(var) != 0 {
                        return Err(mlua::Error::RuntimeError("readonly variable".to_string()));
                    }
                    match value {
                        Value::Table(table) => {
                            if !var.is_null() && l_assoc_p(var) != 0 {
                                set_assoc_from_table(var, &table)
                            } else {
                                set_array_from_table(name.as_ptr().cast(), var, &table, base)
                            }
                        }
                        other => {
                            let mut bytes = scalar_to_bytes(&other)?;
                            bytes.push(0);
                            let result =
                                bind_variable(name.as_ptr().cast(), bytes.as_ptr().cast(), 0);
                            Ok(Value::Boolean(!result.is_null()))
                        }
                    }
                }
            },
        )?,
    )?;

    // bash.unset(var_name) -> boolean
    //
    // true if the variable was removed, false if it did not exist. Raises an
    // error for readonly variables (bash also prints its own diagnostic).
    bash_module.set(
        "unset",
        lua.create_function(|_, name: mlua::String| unsafe {
            let name = name.as_bytes_with_nul();
            match check_unbind_variable(name.as_ptr().cast()) {
                0 => Ok(Value::Boolean(true)),
                -2 => Err(mlua::Error::RuntimeError(
                    "cannot unset variable".to_string(),
                )),
                _ => Ok(Value::Boolean(false)),
            }
        })?,
    )?;

    // bash.call(func_name, ...) -> integer
    bash_module.set(
        "call",
        lua.create_function(|_lua, args: mlua::Variadic<mlua::String>| {
            unsafe {
                let Some(name) = args.first() else {
                    return Err(mlua::Error::RuntimeError("no function name".to_string()));
                };
                let func = find_function(name.as_bytes_with_nul().as_ptr().cast());
                if func.is_null() {
                    return Err(mlua::Error::RuntimeError("function not found".to_string()));
                }
                // execute_shell_function consumes the list head as $0 and the
                // rest as positional parameters; make_word_list prepends, so
                // build the list back to front from the full argument vector.
                let mut list = WordListOwned::default();
                for s in args.iter().rev() {
                    let word = make_word(s.as_bytes_with_nul().as_ptr().cast());
                    list.0 = make_word_list(word, list.0);
                }
                Ok(Value::Integer(execute_shell_function(func, list.0) as i64))
            }
        })?,
    )?;

    // bash.expand(string) -> string
    bash_module.set(
        "expand",
        lua.create_function(|lua, s: mlua::String| unsafe {
            let s = s.as_bytes_with_nul();
            let result = l_expand_string_to_string_in_quotes_owned(s.as_ptr().cast());
            Ok(Value::String(lua.create_string(result.to_bytes())?))
        })?,
    )?;

    // bash.expand_list(string) -> table
    bash_module.set(
        "expand_list",
        lua.create_function(|lua, s: mlua::String| unsafe {
            let s = s.as_bytes_with_nul();
            let table = lua.create_table()?;
            let mut idx = 1;
            for word in &WordListOwned(expand_string(s.as_ptr().cast(), 0)) {
                table.set(idx, lua.create_string(word)?)?;
                idx += 1;
            }
            Ok(Value::Table(table))
        })?,
    )?;

    globals.set("bash", bash_module)?;

    Ok(())
}
