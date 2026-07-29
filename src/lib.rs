//! L_builtin Rust implementation
//!
//! This crate provides Rust implementations of L_builtin commands,
//! leveraging uutils/coreutils for core utilities like ls, stat.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod bash_api;
mod cmd_core;
mod cmd_lua;
mod dispatch;
pub mod shared;