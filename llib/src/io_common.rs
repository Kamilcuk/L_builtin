//! Shared I/O format parsing and hex encode/decode for send, recv, read, write.
//!
//! All four fd-transfer subcommands accept a `-f format` option with values
//! `raw` (default) or `hex`. This module centralises the [`Format`] enum, the
//! `parse_format` converter used by `#[derive(CmdArgs)]`, and the hex
//! encode/decode helpers that bridge byte data and NUL-terminated bash strings.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CStr;
use std::fmt;
use std::marker::PhantomData;
use std::os::raw::c_char;

/// Wrapper to track lifetimes of char pointers.
#[repr(transparent)]
pub struct Cpnt<'a>(*mut c_char, PhantomData<&'a c_char>);

impl<'a> Cpnt<'a> {
    pub const fn new(ptr: *mut c_char) -> Self {
        Self(ptr, PhantomData)
    }
    pub unsafe fn as_cstr(&self) -> &'a CStr {
        CStr::from_ptr(self.0)
    }
    pub unsafe fn as_bytes(&self) -> &'a [u8] {
        self.as_cstr().to_bytes()
    }
    pub unsafe fn as_str(&self) -> Result<&'a str, std::str::Utf8Error> {
        Ok(std::str::from_utf8(self.as_cstr().to_bytes())?)
    }
    pub const fn as_ptr(&self) -> *mut c_char {
        self.0
    }
}

impl<'a, 'b> PartialEq<&'b [u8]> for Cpnt<'a> {
    fn eq(&self, other: &&'b [u8]) -> bool {
        unsafe { self.as_cstr().to_bytes() == *other }
    }
}

impl<'a> PartialEq<&str> for Cpnt<'a> {
    fn eq(&self, other: &&str) -> bool {
        unsafe { self.as_cstr().to_string_lossy() == *other }
    }
}

impl<'a> fmt::Display for Cpnt<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Safety: The pointer is valid for the lifetime 'a
        unsafe { writeln!(f, "{}", self.as_cstr().to_string_lossy()) }
    }
}

const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

/// Data encoding format for send/recv/read/write subcommands.
#[derive(Copy, Clone)]
pub enum Format {
    Raw,
    Hex,
}

/// CmdArgs converter: parse a bash word into a [`Format`].
pub fn parse_format(cptr: Cpnt) -> Result<Format, String> {
    match unsafe { cptr.as_str() } {
        Ok("hex") => Ok(Format::Hex),
        Ok("raw") => Ok(Format::Raw),
        Ok(s) => Err(format!("invalid format, must be 'raw' or 'hex': {s}")),
        Err(e) => Err(e.to_string()),
    }
}

/// Decode a hex string into raw bytes. Returns None on invalid hex.
pub fn hex_decode(hex: &[u8]) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = match std::str::from_utf8(&hex[i..i + 2]) {
            Ok(s) => u8::from_str_radix(s, 16).ok()?,
            Err(_) => return None,
        };
        out.push(byte);
    }
    Some(out)
}

/// Hex-encode `data` into a NUL-terminated byte vector suitable for passing
/// to bash's string interface (null-byte safe).
pub fn hex_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2 + 1);
    for byte in data {
        out.push(HEX_CHARS[(byte >> 4) as usize]);
        out.push(HEX_CHARS[(byte & 0x0f) as usize]);
    }
    out.push(0);
    out
}
