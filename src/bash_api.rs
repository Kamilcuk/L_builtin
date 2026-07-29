#![allow(dead_code)]
#![allow(unused_imports)]

pub use std::ffi::CStr;
pub use std::ops::Deref;
pub use std::os::raw::{c_char, c_int, c_void};

pub use libc::free;

// Bash exit-code macros (from shell.h)
pub const EX_USAGE: c_int = 258; /* syntax error in usage */
pub const EXECUTION_SUCCESS: c_int = 0;
pub const EXECUTION_FAILURE: c_int = 1;

// Opaque type representing Bash's internal SHELL_VAR structure
#[repr(C)]
pub struct SHELL_VAR {
    _private: [u8; 0],
}

// Opaque types for Bash internal structures
#[repr(C)]
pub struct ARRAY {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ARRAY_ELEMENT {
    _private: [u8; 0],
}

#[repr(C)]
pub struct WORD_DESC {
    _private: [u8; 0],
}

#[repr(C)]
pub struct WORD_LIST {
    _private: [u8; 0],
}

extern "C" {
    pub fn find_variable(name: *const c_char) -> *mut SHELL_VAR;
    pub fn l_value_cell(var: *mut SHELL_VAR) -> *mut c_char;
    pub fn l_readonly_p(var: *mut SHELL_VAR) -> c_int;
    pub fn l_invisible_p(var: *mut SHELL_VAR) -> c_int;
    pub fn l_array_p(var: *mut SHELL_VAR) -> c_int;
    pub fn l_array_cell(var: *mut SHELL_VAR) -> *mut ARRAY;
    pub fn bind_variable(name: *const c_char, value: *const c_char, flags: c_int) -> *mut SHELL_VAR;
    pub fn find_function(name: *const c_char) -> *mut SHELL_VAR;
    pub fn make_new_array_variable(name: *const c_char) -> *mut SHELL_VAR;
    pub fn array_flush(array: *mut ARRAY);
    pub fn array_insert(array: *mut ARRAY, key: i64, value: *const c_char) -> c_int;
    pub fn make_word(string: *const c_char) -> *mut WORD_DESC;
    pub fn make_word_list(word: *mut WORD_DESC, list: *mut WORD_LIST) -> *mut WORD_LIST;
    pub fn execute_shell_function(var: *mut SHELL_VAR, args: *mut WORD_LIST) -> c_int;
    pub fn dispose_words(list: *mut WORD_LIST);
    pub fn expand_string_to_string(string: *const c_char, quoted: c_int) -> *mut c_char;
    pub fn expand_string(string: *const c_char, flags: c_int) -> *mut WORD_LIST;
    pub fn l_expand_string_to_string_in_quotes(string: *const c_char) -> *mut c_char;

    // Minimal C shim functions for struct field dereferencing
    pub fn l_array_head(array: *mut ARRAY) -> *mut ARRAY_ELEMENT;
    pub fn l_element_forw(element: *mut ARRAY_ELEMENT) -> *mut ARRAY_ELEMENT;
    pub fn l_element_value(element: *mut ARRAY_ELEMENT) -> *mut c_char;
    pub fn l_word_list_next(list: *mut WORD_LIST) -> *mut WORD_LIST;
    pub fn l_word_list_word(list: *mut WORD_LIST) -> *mut WORD_DESC;
    pub fn l_word_desc_string(word: *mut WORD_DESC) -> *mut c_char;
}

/// RAII wrapper for C strings that need to be freed with `free()`
#[repr(transparent)]
pub struct CStringOwned(pub *mut c_char);

impl CStringOwned {
    #[inline]
    pub fn to_bytes(&self) -> impl AsRef<[u8]> {
        if self.0.is_null() {
            b""
        } else {
            unsafe { CStr::from_ptr(self.0).to_bytes() }
        }
    }
}

impl Drop for CStringOwned {
    #[inline]
    fn drop(&mut self) {
        unsafe { free(self.0 as *mut c_void) };
    }
}

pub unsafe fn l_expand_string_to_string_in_quotes_owned(s: *const c_char) -> CStringOwned {
    CStringOwned(l_expand_string_to_string_in_quotes(s))
}


/// RAII wrapper for C strings that need to be freed with `free()`
#[repr(transparent)]
pub struct WordListOwned(pub *mut WORD_LIST);

impl Default for WordListOwned {
    fn default() -> Self {
        Self(std::ptr::null_mut())
    }
}

impl Drop for WordListOwned {
    #[inline]
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { dispose_words(self.0) }
        }
    }
}

pub unsafe fn l_expand_string_owned(string: *const c_char, flags: c_int) -> WordListOwned {
    WordListOwned(expand_string(string, flags))
}
