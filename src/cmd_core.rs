//! L_builtin Rust implementation
//!
//! This crate provides Rust implementations of L_builtin commands,
//! leveraging uutils/coreutils for core utilities like ls, stat.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use crate::bash_api::{WORD_LIST, l_word_list_next, l_word_list_word, l_word_desc_string};
use crate::shared::word_list_to_os_strings;

use std::os::raw::{c_int};
use std::ffi::{CStr, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

macro_rules! dispatch_uu_cmds {
    ( $cmd_name:expr, $args:expr, $( $name:literal => $module:ident ),* $(,)? ) => {{
        // Convert OsString to &str for matching (ASCII-only subcommand names)
        let cmd_str = $cmd_name.to_str().unwrap_or("");
        match cmd_str {
            $(
                $name => {
                    use $module::uumain;
                    let mut uu_args = vec![OsString::from($name)];
                    uu_args.extend($args.iter().cloned());
                    uumain(uu_args.into_iter())
                }
            )*
            _ => {
                // Print raw bytes for unknown subcommand
                eprintln!("L_builtin core: unknown subcommand: {}", $cmd_name.to_string_lossy());
                127
            }
        }
    }};
}

#[no_mangle]
pub extern "C" fn l_core_subcommand(list: *mut WORD_LIST) -> c_int {
    // The C dispatcher already skipped the "core" subcommand name
    // list now points to the first argument after "core"
    if list.is_null() {
        eprintln!("L_builtin core: missing subcommand");
        eprintln!("Usage: L_builtin core <subcommand> [args...]");
        eprintln!("Available: ls, stat, dirname, rm");
        return 127;
    }
    let cmd_name = list;
    // Get the word string using shim functions
    let word_desc = unsafe { l_word_list_word(cmd_name) };
    if word_desc.is_null() {
        eprintln!("L_builtin core: missing subcommand");
        return 127;
    }
    let str_ptr = unsafe { l_word_desc_string(word_desc) };
    if str_ptr.is_null() {
        eprintln!("L_builtin core: missing subcommand");
        return 127;
    }
    let cmd_str = unsafe { CStr::from_ptr(str_ptr).to_str().unwrap_or("") };
    if cmd_str == "-h" || cmd_str == "--help" {
        print_core_help();
        return 0;
    }
    let args = word_list_to_os_strings(unsafe { l_word_list_next(cmd_name) });
    // Convert cmd_str to OsStr for dispatch
    let cmd_os = OsStr::from_bytes(cmd_str.as_bytes());
    dispatch_uu_cmds!(
        cmd_os,
        args,
        "ls" => uu_ls,
        "stat" => uu_stat,
        "dirname" => uu_dirname,
        "rm" => uu_rm,
    )
}

fn print_core_help() {
    eprintln!("Core utilities via uutils/coreutils");
    eprintln!();
    eprintln!("L_builtin core <subcommand> [options] [args]");
    eprintln!();
    eprintln!("Available subcommands:");
    eprintln!("  ls       List directory contents");
    eprintln!("  stat     Display file status");
    eprintln!("  dirname  Strip last component from file name");
    eprintln!("  rm       Remove files or directories");
    eprintln!();
    eprintln!("Use 'L_builtin core <subcommand> -h' for more information.");
}
