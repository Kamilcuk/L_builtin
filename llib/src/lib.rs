//! `llib` — bash/OS-agnostic support libraries used by the L_builtin crate.
//!
//! This crate contains only self-contained libraries with no dependency on
//! bash or the builtin environment.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub mod intlookup;
pub mod intstr;
pub mod io_common;
pub mod nolock;
pub mod variadic;