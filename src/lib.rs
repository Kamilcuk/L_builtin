//! L_builtin Rust implementation
//!
//! This crate provides Rust implementations of L_builtin commands,
//! leveraging uutils/coreutils for core utilities like ls, stat.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub(crate) mod bash_alloc;
pub(crate) mod bash_api;
pub(crate) mod bprint_bytes;
pub(crate) mod cmd_core;
pub(crate) mod cmd_lua;
pub(crate) mod entrypoint;
pub(crate) mod shared;
pub(crate) mod bash_getopt;
pub(crate) mod intlookup;

/// Route all Rust allocations through bash's allocator, so the whole process
/// shares one heap regardless of how bash was configured. See `bash_alloc`.
#[global_allocator]
static GLOBAL_ALLOCATOR: bash_alloc::BashAllocator = bash_alloc::BashAllocator;
