//! Shared utilities for L_builtin Rust implementation

use std::ffi::{CStr, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

use crate::bash_api::{WORD_LIST, l_word_list_next, l_word_list_word, l_word_desc_string};

/// Iterator over Bash WORD_LIST arguments
pub struct WordListArgs {
    current: *mut WORD_LIST,
}

impl WordListArgs {
    /// # Safety
    /// `head` must be a valid pointer to a Bash `WORD_LIST` structure (or null).
    pub unsafe fn new(head: *mut WORD_LIST) -> Self {
        Self { current: head }
    }
}

impl Iterator for WordListArgs {
    type Item = OsString;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.current.is_null() {
            unsafe {
                let node = self.current;
                // Use shim function instead of direct field access
                self.current = l_word_list_next(node);
                let word_desc = l_word_list_word(node);
                if !word_desc.is_null() {
                    let str_ptr = l_word_desc_string(word_desc);
                    if !str_ptr.is_null() {
                        let c_str = CStr::from_ptr(str_ptr);
                        return Some(OsStr::from_bytes(c_str.to_bytes()).to_os_string());
                    }
                }
            }
        }
        None
    }
}

/// Extracts the current node's word as an `OsString` and mutates `list` to point to the next `WORD_LIST` node.
/// Returns `None` if `*list` is null or if the string pointers inside are null.
pub unsafe fn word_list_next<'a>(list: &mut *mut WORD_LIST) -> Option<&'a std::ffi::OsStr> {
    let current = *list;
    if current.is_null() {
        return None;
    }
    let word_ptr = l_word_list_word(current);
    if word_ptr.is_null() {
        return None;
    }
    let str_ptr = l_word_desc_string(word_ptr);
    if str_ptr.is_null() {
        return None;
    }
    // Advance to next node before returning
    *list = l_word_list_next(current);
    // Borrow C memory directly as &[u8] -> &OsStr (Zero-Copy)
    let bytes = std::ffi::CStr::from_ptr(str_ptr).to_bytes();
    Some(std::ffi::OsStr::from_bytes(bytes))
}

/// Convert Bash WORD_LIST to Rust Vec<&OsStr> using byte-level conversion
pub fn word_list_to_os_str<'a>(mut list: *mut WORD_LIST) -> Vec<&'a std::ffi::OsStr> {
    let mut args = Vec::new();
    unsafe {
        while let Some(arg) = word_list_next(&mut list) {
            args.push(arg);
        }
    }
    args
}

/// Convert Bash WORD_LIST to owned Vec<OsString>
pub fn word_list_to_os_strings(mut list: *mut WORD_LIST) -> Vec<OsString> {
    let mut args = Vec::new();
    unsafe {
        while let Some(arg) = word_list_next(&mut list) {
            args.push(arg.to_os_string());
        }
    }
    args
}

/// Convert Bash WORD_LIST to owned Vec<OsString> from argv-style list (first element is command name)
pub fn argv_to_os_strings(list: *mut WORD_LIST) -> Vec<OsString> {
    word_list_to_os_strings(list)
}

/// Lexicographic `a < b` for byte slices, usable in const context.
pub const fn bytes_lt(a: &[u8], b: &[u8]) -> bool {
    let min = if a.len() < b.len() { a.len() } else { b.len() };
    let mut i = 0;
    while i < min {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
        i += 1;
    }
    a.len() < b.len()
}

/// Sort an array of `(byte-key, value)` pairs by key at compile time.
///
/// Insertion sort; runs entirely in const evaluation, so the resulting
/// table is stored pre-sorted in the binary and can be binary-searched at
/// runtime.
pub const fn sort_by_byte_key<T: Copy, const N: usize>(
    mut arr: [(&'static [u8], T); N],
) -> [(&'static [u8], T); N] {
    let mut i = 1;
    while i < N {
        let mut j = i;
        while j > 0 && bytes_lt(arr[j].0, arr[j - 1].0) {
            let tmp = arr[j];
            arr[j] = arr[j - 1];
            arr[j - 1] = tmp;
            j -= 1;
        }
        i += 1;
    }
    arr
}

/// Wrapper for printing OsString as raw bytes without UTF-8 conversion
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct RawCStr<'a>(pub &'a CStr);

impl<'a> RawCStr<'a> {
    #[inline]
    pub fn to_os_string(&self) -> OsString {
        OsString::from(std::ffi::OsStr::from_bytes(self.0.to_bytes()))
    }
}