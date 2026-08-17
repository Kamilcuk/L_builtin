//! L_builtin Rust implementation
//!
//! This crate provides Rust implementations of L_builtin commands,
//! leveraging uutils/coreutils for core utilities like ls, stat.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub(crate) mod accept;
pub(crate) mod bash_alloc;
pub(crate) mod bash_api;
pub(crate) mod bprint_bytes;
pub(crate) mod cmd_barrier;
pub(crate) mod cmd_core;
pub(crate) mod cmd_lua;
pub(crate) mod cmd_mutex;
pub(crate) mod cmd_semaphore;
pub(crate) mod cmd_shm;
pub(crate) mod connect;
pub(crate) mod entrypoint;
pub(crate) mod eventfd;
pub(crate) mod getopts;
pub(crate) mod intlookup;
pub(crate) mod listen;
pub(crate) mod lseek;
pub(crate) mod memfd;
pub(crate) mod pipe;
pub(crate) mod pthread;
pub(crate) mod recv;
pub(crate) mod send;
pub(crate) mod shared;
pub(crate) mod shutdown;
pub(crate) mod signalfd;
pub(crate) mod sleep;
pub(crate) mod splice;
pub(crate) mod subcmd;
pub(crate) mod timerfd;
pub(crate) mod variadic;
pub(crate) mod unittest;
pub(crate) mod vardb;

// Test-only stand-ins for bash C symbols (allocator) so `cargo test` links
// without a bash process. Never compiled into the shipped `.so`.
#[cfg(test)]
mod test_stubs;

/// Route all Rust allocations through bash's allocator, so the whole process
/// shares one heap regardless of how bash was configured. See `bash_alloc`.
#[global_allocator]
static GLOBAL_ALLOCATOR: bash_alloc::BashAllocator = bash_alloc::BashAllocator;
