#![allow(dead_code)]
#![allow(unused_imports)]

use std::ffi::{c_void, CStr, OsStr, OsString};
use std::fmt;
use std::fmt::Display;
use std::iter::Map;
use std::marker::PhantomData;
pub use std::ops::Deref;
pub use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::sync::OnceLock;

use crate::{beprintln, bprintln};

// Bash exit-code macros (from shell.h)
pub const EX_USAGE: c_int = 258; /* syntax error in usage */
pub const EX_NOTFOUND: c_int = 127; /* command not found */
pub const EXECUTION_SUCCESS: c_int = 0;
pub const EXECUTION_FAILURE: c_int = 1;

#[repr(C)]
pub struct SHELL_VAR {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ARRAY {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ARRAY_ELEMENT {
    _private: [u8; 0],
}

#[repr(C)]
pub struct HASH_TABLE {
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

/// The `struct builtin` definition from Bash's builtins.h has not changed in
/// the past 10 years (since Bash 4.4, 2016). Its fields remain:
/// - char *name
/// - sh_builtin_func_t *function
/// - int flags
/// - char * const *long_doc
/// - const char *short_doc
/// - char *handle
///
/// Only the `flags` bitmask has been extended with new capabilities:
/// - LOCALVAR_BUILTIN (0x40) added in Bash 4.4 (2016)
/// - ARRAYREF_BUILTIN (0x80) added in Bash 5.2 (2021)
#[repr(C)]
pub struct Builtin {
    pub name: *mut c_char,
    pub function: Option<unsafe extern "C" fn(*mut WORD_LIST) -> c_int>,
    pub flags: c_int,
    pub long_doc: *const *const c_char,
    pub short_doc: *const c_char,
    pub handle: *mut c_char,
}

extern "C" {
    pub static mut this_command_name: *mut c_char;
    pub fn l_xmalloc(size: usize) -> *mut c_void;
    pub fn l_xrealloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    pub fn l_xfree(ptr: *mut c_void);
    pub fn find_variable(name: *const c_char) -> *mut SHELL_VAR;
    pub fn l_value_cell(var: *mut SHELL_VAR) -> *mut c_char;
    pub fn l_readonly_p(var: *mut SHELL_VAR) -> c_int;
    pub fn l_invisible_p(var: *mut SHELL_VAR) -> c_int;
    pub fn l_array_p(var: *mut SHELL_VAR) -> c_int;
    pub fn l_array_cell(var: *mut SHELL_VAR) -> *mut ARRAY;
    pub fn l_assoc_p(var: *mut SHELL_VAR) -> c_int;
    pub fn l_assoc_cell(var: *mut SHELL_VAR) -> *mut HASH_TABLE;
    pub fn bind_variable(name: *const c_char, value: *const c_char, flags: c_int)
        -> *mut SHELL_VAR;
    pub fn find_function(name: *const c_char) -> *mut SHELL_VAR;
    pub fn make_new_array_variable(name: *const c_char) -> *mut SHELL_VAR;
    /// Convert an existing scalar variable to an indexed array in place
    /// (preserves scope; the old value becomes element 0).
    pub fn convert_var_to_array(var: *mut SHELL_VAR) -> *mut SHELL_VAR;
    pub fn make_new_assoc_variable(name: *const c_char) -> *mut SHELL_VAR;
    pub fn array_flush(array: *mut ARRAY);
    pub fn array_insert(array: *mut ARRAY, key: i64, value: *const c_char) -> c_int;
    pub fn assoc_flush(hash: *mut HASH_TABLE);
    /// Returns a word list of the assoc array's keys (unordered); caller
    /// disposes with dispose_words.
    pub fn assoc_keys_to_word_list(hash: *mut HASH_TABLE) -> *mut WORD_LIST;
    /// Borrowed pointer to the value stored under `key`, or NULL.
    pub fn assoc_reference(hash: *mut HASH_TABLE, key: *const c_char) -> *mut c_char;
    /// Prints bash's own error and returns -2 for readonly/non-unsettable
    /// variables; otherwise unbinds and returns unbind_variable's status.
    pub fn l_check_unbind_variable(name: *const c_char) -> c_int;
    pub fn make_word(string: *const c_char) -> *mut WORD_DESC;
    pub fn make_word_list(word: *mut WORD_DESC, list: *mut WORD_LIST) -> *mut WORD_LIST;
    pub fn execute_shell_function(var: *mut SHELL_VAR, args: *mut WORD_LIST) -> c_int;
    pub fn dispose_words(list: *mut WORD_LIST);
    pub fn expand_string_to_string(string: *const c_char, quoted: c_int) -> *mut c_char;
    pub fn expand_string(string: *const c_char, flags: c_int) -> *mut WORD_LIST;
    pub fn l_expand_string_to_string_in_quotes(string: *const c_char) -> *mut c_char;
    pub fn l_array_head(array: *mut ARRAY) -> *mut ARRAY_ELEMENT;
    pub fn l_element_forw(element: *mut ARRAY_ELEMENT) -> *mut ARRAY_ELEMENT;
    pub fn l_element_value(element: *mut ARRAY_ELEMENT) -> *mut c_char;
    pub fn l_element_index(element: *mut ARRAY_ELEMENT) -> i64;
    pub fn l_assoc_insert(hash: *mut HASH_TABLE, key: *const c_char, value: *const c_char)
        -> c_int;
    pub fn l_word_list_next(list: *mut WORD_LIST) -> *mut WORD_LIST;
    pub fn l_word_list_word(list: *mut WORD_LIST) -> *mut WORD_DESC;
    pub fn l_word_desc_string(word: *mut WORD_DESC) -> *mut c_char;
    #[cfg(not(feature = "bash_lt_4_3"))]
    pub fn l_execute_command_string(cmd: *const c_char) -> c_int;
}

///////////////////////////////////////////////////////////////////////////

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
        unsafe { l_xfree(self.0 as *mut c_void) };
    }
}

///////////////////////////////////////////////////////////////////////////

/// Wrapper to track lifetimes of char pointers.
#[repr(transparent)]
pub struct Cpnt<'a>(pub *mut c_char, pub PhantomData<&'a c_char>);

impl<'a> Cpnt<'a> {
    pub const fn new(ptr: *mut c_char) -> Self {
        Self(ptr, PhantomData)
    }
    pub unsafe fn to_bytes(&self) -> &'a [u8] {
        CStr::from_ptr(self.0).to_bytes()
    }
    pub const fn as_ptr(&self) -> *mut c_char {
        self.0
    }
}

impl<'a> fmt::Display for Cpnt<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_null() {
            return write!(f, "(null)");
        }
        unsafe {
            let bytes = self.to_bytes();
            let text = String::from_utf8_lossy(bytes);
            write!(f, "{text}")
        }
    }
}

///////////////////////////////////////////////////////////////////////////
/// WORD_LIST utilities

#[repr(transparent)]
pub struct WordListView<'a>(*mut WORD_LIST, PhantomData<&'a WORD_LIST>);

#[repr(transparent)]
#[derive(Clone)]
pub struct WordListIterCpnt<'a>(*mut WORD_LIST, PhantomData<&'a WORD_LIST>);

pub type WordListIterBytes<'a> = Map<WordListIterCpnt<'a>, fn(Cpnt<'a>) -> &'a [u8]>;

pub type WordListIterOsString<'a> = Map<WordListIterBytes<'a>, fn(&[u8]) -> OsString>;

impl<'a> WordListView<'a> {
    pub unsafe fn from_raw(head: *mut WORD_LIST) -> Self {
        Self(head, PhantomData)
    }
    pub fn iter(&self) -> WordListIterCpnt<'a> {
        WordListIterCpnt(self.0, PhantomData)
    }
    pub fn iter_bytes(&self) -> WordListIterBytes<'_> {
        self.iter().map(|c| unsafe { c.to_bytes() })
    }
    pub fn iter_osstring(&self) -> WordListIterOsString<'_> {
        self.iter_bytes().map(|s| OsString::from_vec(s.to_vec()))
    }
    pub const fn as_ptr(&self) -> *mut WORD_LIST {
        self.0
    }
    pub unsafe fn current(&self) -> Option<Cpnt<'a>> {
        self.iter().current()
    }
}

impl<'a> IntoIterator for WordListView<'a> {
    type Item = Cpnt<'a>;
    type IntoIter = WordListIterCpnt<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> WordListIterCpnt<'a> {
    pub unsafe fn print(&self) {
        for i in self.clone() {
            println!("{i}");
        }
    }
    pub unsafe fn current(&self) -> Option<Cpnt<'a>> {
        if !self.0.is_null() {
            let word_ptr = l_word_list_word(self.0);
            if !word_ptr.is_null() {
                let pnt = l_word_desc_string(word_ptr);
                if !pnt.is_null() {
                    return Some(Cpnt::new(pnt));
                }
            }
        }
        None
    }
    pub unsafe fn advance(&mut self) {
        if !self.0.is_null() {
            self.0 = l_word_list_next(self.0);
        }
    }
    pub const fn as_ptr(&self) -> *mut WORD_LIST {
        self.0
    }
}

impl<'a> Iterator for WordListIterCpnt<'a> {
    type Item = Cpnt<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let item = self.current();
            self.advance();
            item
        }
    }
}

#[repr(transparent)]
pub struct WordListOwned(pub *mut WORD_LIST);

impl WordListOwned {
    pub fn as_view(&self) -> WordListView<'_> {
        unsafe { WordListView::from_raw(self.0) }
    }
    pub fn iter(&self) -> WordListIterCpnt<'_> {
        self.as_view().iter()
    }
}

impl Default for WordListOwned {
    fn default() -> Self {
        Self(std::ptr::null_mut())
    }
}

impl Drop for WordListOwned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { dispose_words(self.0) }
        }
    }
}

impl<'a> IntoIterator for &'a WordListOwned {
    type Item = Cpnt<'a>;
    type IntoIter = WordListIterCpnt<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
