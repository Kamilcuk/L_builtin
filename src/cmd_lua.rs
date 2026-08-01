//! Lua subcommand implementation using mlua
use crate::bash_api::{
    expand_string, l_expand_string_to_string_in_quotes, CStringOwned, WordListOwned, WordListView,
};
use crate::beprintln;
use crate::bprint_bytes::BDisplay;
use crate::bprintln;
use crate::return_on_err;
use crate::shared::{getargs_unexpected, CByteStr};

use std::ffi::{c_char, CStr};
use std::io::Write;
use std::os::raw::c_int;

use getargs::{Opt, Options};
use mlua::{Lua, Value};

use crate::bash_api::{
    array_flush, array_insert, assoc_flush, assoc_keys_to_word_list, assoc_reference,
    bind_variable, check_unbind_variable, convert_var_to_array, execute_shell_function,
    find_function, find_variable, l_array_cell, l_array_head, l_array_p, l_assoc_cell,
    l_assoc_insert, l_assoc_p, l_element_forw, l_element_index, l_element_value, l_invisible_p,
    l_readonly_p, l_value_cell, make_new_array_variable, make_new_assoc_variable, make_word,
    make_word_list, EX_USAGE,
    SHELL_VAR, WORD_LIST,
};

impl BDisplay for mlua::Error {
    fn bwrite<W: Write + ?Sized>(&self, w: &mut W) {
        w.write_all(self.to_string().as_bytes()).unwrap();
    }
}
const ENAME: &str = "L_builtin lua";

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn l_lua_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut args = unsafe { WordListView::from_raw(list) }.into_iter();
    let mut opts = Options::new(&mut args);
    let mut ret_var: Option<&[u8]> = None;
    while let Some(arg) = return_on_err!(ENAME, opts.next_opt(), EX_USAGE) {
        match arg {
            Opt::Short(b'v') | Opt::Long(b"var") => {
                ret_var = Some(return_on_err!(ENAME, opts.value(), EX_USAGE));
            }
            Opt::Short(b'h') | Opt::Long(b"help") => {
                print_lua_help();
                return 0;
            }
            _ => {
                return getargs_unexpected(ENAME, arg);
            }
        }
    }
    let script = match opts.next_positional() {
        Some(script) => script,
        None => {
            // No script was given — only options (or nothing).
            beprintln!(ENAME, b": missing script");
            beprintln!(b"Usage: L_builtin lua [-v VAR] <script> [args...]");
            return EX_USAGE;
        }
    };
    let lua = Lua::new();
    let return_value = return_on_err!(ENAME, run_lua_script(&lua, script, args), 1);
    if let Some(var_name) = ret_var {
        let var_cname = CByteStr::new(var_name);
        return_on_err!(
            ENAME,
            set_bash_from_lua_in(&lua, var_cname.as_ptr(), return_value, None),
            1
        );
    }
    0
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
        let mut bytes = scalar_to_bytes(&v)?;
        bytes.push(0);
        array_insert(array, idx, bytes.as_ptr().cast());
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
        let mut key = scalar_to_bytes(&k)?;
        key.push(0);
        let mut val = scalar_to_bytes(&v)?;
        val.push(0);
        l_assoc_insert(hash, key.as_ptr().cast(), val.as_ptr().cast());
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
                    table_has_string_keys(&table)?
                };
                if is_assoc {
                    set_assoc_from_table(name, var, &table)
                } else {
                    set_array_from_table(lua, name, var, &table, base)
                }
            }
            other => {
                let mut bytes = scalar_to_bytes(&other)?;
                bytes.push(0);
                let result = bind_variable(name, bytes.as_ptr().cast(), 0);
                Ok(Value::Boolean(!result.is_null()))
            }
        }
    }
}

/// Check if a Lua table has any string keys (indicating it should be an associative array)
fn table_has_string_keys(table: &mlua::Table) -> mlua::Result<bool> {
    for pair in table.pairs::<Value, Value>() {
        let (k, _) = pair?;
        match k {
            Value::String(_) => return Ok(true),
            _ => {}
        }
    }
    Ok(false)
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
                table.set(idx + 1, lua.create_string(word)?)?;
            }
            Ok(Value::Table(table))
        })?,
    )?;
    globals.set("bash", bash_module)?;
    Ok(())
}
