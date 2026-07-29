//! Lua subcommand implementation using mlua
use crate::shared::word_list_to_os_strings;

use std::os::raw::{c_int};
use std::ffi::{CStr, CString};

use clap::{Command, Arg};
use mlua::{Lua, Value};

use crate::bash_api::{
    SHELL_VAR, WORD_LIST, WordListOwned, array_flush, array_insert, bind_variable, execute_shell_function, find_function, find_variable, l_array_cell, l_array_head, l_array_p, l_element_forw, l_element_value, l_expand_string_owned, l_expand_string_to_string_in_quotes_owned, l_invisible_p, l_readonly_p, l_value_cell, l_word_desc_string, l_word_list_next, l_word_list_word, make_new_array_variable, make_word, make_word_list,
};


#[no_mangle]
pub extern "C" fn l_lua_subcommand(list: *mut WORD_LIST) -> c_int {
    let args = word_list_to_os_strings(list);
    let matches = build_cli()
        .try_get_matches_from(args);
    let matches = match matches {
        Ok(m) => m,
        Err(e) => {
            // -h/--help lands here as ErrorKind::DisplayHelp; print to
            // stdout and return success. Real parse errors go to stderr
            // and return 2. e.print() picks the stream by error kind.
            let is_help = e.kind() == clap::error::ErrorKind::DisplayHelp;
            let _ = e.print();
            return if is_help { 0 } else { 2 };
        }
    };
    let ret_var = matches.get_one::<String>("var").cloned();
    let script = matches.get_one::<String>("script").cloned().unwrap();
    let script_args: Vec<String> = matches
        .get_many::<String>("args")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let result = run_lua_script(&script, &script_args);
    let return_value = match result {
        Ok(val) => val,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };
    if let Some(var_name) = ret_var {
        let c_var_name = CString::new(var_name.as_bytes())
            .map_err(|e| format!("invalid variable name: {}", e))
            .unwrap();
        let c_result = CString::new(return_value.as_bytes())
            .map_err(|e| format!("invalid result: {}", e))
            .unwrap();
        let ret = unsafe {
            bind_variable(c_var_name.as_ptr(), c_result.as_ptr(), 0)
        };
        if ret.is_null() {
            eprintln!("failed to set variable: {}", var_name);
            return 1;
        }
    }
    0
}

/// Build the clap command for argument parsing
fn build_cli() -> Command {
    Command::new("lua")
        .no_binary_name(true)
        .disable_version_flag(true)
        .arg(
            Arg::new("var")
                .short('v')
                .long("var")
                .value_name("VAR")
                .help("Variable to bind return value to")
                .num_args(1)
        )
        .arg(
            Arg::new("script")
                .help("Lua script (file path or inline code)")
                .required(true)
                .num_args(1)
        )
        .arg(
            Arg::new("args")
                .help("Arguments passed to the script")
                .num_args(0..)
        )
}

/// Run a Lua script and return the exit code or string result
fn run_lua_script(script: &str, args: &[String]) -> Result<String, String> {
    let lua = Lua::new();

    // Register bash API functions
    register_bash_api(&lua).map_err(|e| e.to_string())?;

    // Set up arg table
    let arg_table = lua.create_table().map_err(|e| e.to_string())?;
    for (i, arg) in args.iter().enumerate() {
        arg_table.set(i + 1, arg.clone()).map_err(|e| e.to_string())?;
    }
    lua.globals().set("arg", arg_table).map_err(|e| e.to_string())?;

    // Execute the script and get the return value (similar to luaL_dostring in C)
    let chunk = lua.load(script);
    let result: mlua::Value = chunk.eval().map_err(|e| e.to_string())?;

    // Convert the return value to a string (similar to lua_tostring in C)
    Ok(result.to_string().map_err(|e| e.to_string())?)
}

/// Register bash API functions with Lua
fn register_bash_api(lua: &Lua) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // Create bash module
    let bash_module = lua.create_table()?;

    // bash.get(var_name) -> string or nil
    bash_module.set("get", lua.create_function(|lua, name: mlua::String| {
        unsafe {
            let var = find_variable(name.to_pointer().cast());
            if var.is_null() || l_invisible_p(var) != 0 {
                return Ok(Value::Nil);
            }
            let val = l_value_cell(var);
            if val.is_null() {
                Ok(Value::Nil)
            } else {
                Ok(Value::String(lua.create_string(CStr::from_ptr(val).to_bytes())?))
            }
        }
    })?)?;

    // bash.set(var_name, value) -> boolean
    bash_module.set("set", lua.create_function(|_, (name, value): (mlua::String, mlua::String)| {
        unsafe {
            let var = find_variable(name.to_pointer().cast());
            if !var.is_null() && l_readonly_p(var) != 0 {
                return Err(mlua::Error::RuntimeError("readonly variable".to_string()));
            }
            let result = bind_variable(name.to_pointer().cast(), value.to_pointer().cast(), 0);
            Ok(Value::Boolean(!result.is_null()))
        }
    })?)?;

    // bash.get_array(var_name) -> table or nil
    bash_module.set("get_array", lua.create_function(|lua, name: mlua::String| {
        unsafe {
            let var = find_variable(name.to_pointer().cast());
            if var.is_null() || l_invisible_p(var) != 0 || l_array_p(var) == 0 {
                return Ok(Value::Nil);
            }
            let array = l_array_cell(var);
            if array.is_null() {
                return Ok(Value::Nil);
            }
            let table = lua.create_table()?;
            let mut idx = 1;
            let head = l_array_head(array);
            if head.is_null() {
                return Ok(Value::Table(table));
            }
            let mut curr = head;
            loop {
                let next = l_element_forw(curr);
                if next == head {
                    break;
                }
                curr = next;
                let val = l_element_value(curr);
                if !val.is_null() {
                    let s = lua.create_string(CStr::from_ptr(val).to_bytes())?;
                    table.set(idx, s)?;
                }
                idx += 1;
            }
            Ok(Value::Table(table))
        }
    })?)?;

    // bash.set_array(var_name, table) -> boolean
    bash_module.set("set_array", lua.create_function(|_lua, (name, table): (mlua::String, mlua::Table)| {
        unsafe {
            let mut var = find_variable(name.to_pointer().cast());
            if !var.is_null() {
                if l_readonly_p(var) != 0 {
                    return Err(mlua::Error::RuntimeError("readonly variable".to_string()));
                }
                if l_array_p(var) == 0 {
                    return Err(mlua::Error::RuntimeError("not an array".to_string()));
                }
            } else {
                var = make_new_array_variable(name.to_pointer().cast());
            }
            if var.is_null() {
                return Err(mlua::Error::RuntimeError("creation failed".to_string()));
            }
            let array = l_array_cell(var);
            if array.is_null() {
                return Err(mlua::Error::RuntimeError("internal error".to_string()));
            }
            array_flush(array);
            for pair in table.pairs::<Value, Value>() {
                let (k, v) = pair?;
                if let (Value::Integer(idx), Value::String(val)) = (k, v) {
                    if idx > 0 {
                        array_insert(array, idx as i64, val.to_pointer().cast());
                    }
                }
            }
            Ok(Value::Boolean(true))
        }
    })?)?;

    // bash.call(func_name, ...) -> integer
    bash_module.set("call", lua.create_function(|_lua, args: mlua::Table| {
        unsafe {
            let mut first = true;
            let mut func: *mut SHELL_VAR = std::ptr::null_mut();
            let mut list = WordListOwned::default();
            for pair in args.pairs::<Value, Value>() {
                let (_, v) = pair?;
                if let Value::String(ref s) = v {
                    if first {
                        first = false;
                        func = find_function(s.to_pointer().cast());
                        if func.is_null() {
                            return Err(mlua::Error::RuntimeError("function not found".to_string()));
                        }
                    } else {
                        let word = make_word(s.to_pointer().cast());
                        list.0 = make_word_list(word, list.0);
                    }
                }
            }
            if func.is_null() {
                return Err(mlua::Error::RuntimeError("no function name".to_string()));
            }
            Ok(Value::Integer(execute_shell_function(func, list.0) as i64))
        }
    })?)?;

    // bash.expand(string) -> string
    bash_module.set("expand", lua.create_function(|lua, s: mlua::String| {
        unsafe {
            let result = l_expand_string_to_string_in_quotes_owned(s.to_pointer().cast());
            Ok(Value::String(lua.create_string(result.to_bytes())?))
        }
    })?)?;

    // bash.expand_list(string) -> table
    bash_module.set("expand_list", lua.create_function(|lua, s: mlua::String| {
        unsafe {
            let list = l_expand_string_owned(s.to_pointer().cast(), 0);
            let table = lua.create_table()?;
            let mut idx = 1;
            let mut curr = list.0;
            while !curr.is_null() {
                let word = l_word_list_word(curr);
                if !word.is_null() {
                    let str_ptr = l_word_desc_string(word);
                    if !str_ptr.is_null() {
                        table.set(idx, lua.create_string(CStr::from_ptr(str_ptr).to_bytes())?)?;
                        idx += 1;
                    }
                }
                curr = l_word_list_next(curr);
            }
            Ok(Value::Table(table))
        }
    })?)?;

    globals.set("bash", bash_module)?;

    Ok(())
}
