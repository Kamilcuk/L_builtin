//! L_builtin Rust implementation
//!
//! This crate provides Rust implementations of L_builtin commands,
//! leveraging uutils/coreutils for core utilities like ls, stat.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod bash_alloc;
mod bash_api;
mod cmd_core;
mod cmd_lua;
mod dispatch;
pub mod shared;

/// Route all Rust allocations through bash's allocator, so the whole process
/// shares one heap regardless of how bash was configured. See `bash_alloc`.
#[global_allocator]
static GLOBAL_ALLOCATOR: bash_alloc::BashAllocator = bash_alloc::BashAllocator;