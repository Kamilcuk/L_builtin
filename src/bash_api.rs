#![allow(dead_code)]
#![allow(unused_imports)]

use std::ffi::{CStr, OsStr};
use std::iter::Map;
use std::marker::PhantomData;
pub use std::ops::Deref;
pub use std::os::raw::{c_char, c_int, c_void};
use std::os::unix::ffi::OsStrExt;

use crate::{beprintln, bprintln};

// Bash exit-code macros (from shell.h)
pub const EX_USAGE: c_int = 258; /* syntax error in usage */
pub const EX_NOTFOUND: c_int = 127; /* command not found */
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

extern "C" {
    pub static mut this_command_name: *mut c_char;
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
    pub fn check_unbind_variable(name: *const c_char) -> c_int;
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
    pub fn l_element_index(element: *mut ARRAY_ELEMENT) -> i64;
    /// Insert into an associative array; key and value are both copied.
    pub fn l_assoc_insert(hash: *mut HASH_TABLE, key: *const c_char, value: *const c_char)
        -> c_int;
    pub fn l_word_list_next(list: *mut WORD_LIST) -> *mut WORD_LIST;
    pub fn l_word_list_word(list: *mut WORD_LIST) -> *mut WORD_DESC;
    pub fn l_word_desc_string(word: *mut WORD_DESC) -> *mut c_char;

    /// Execute a word list as a single shell command line (words are
    /// single-quoted before joining) and return its exit status.
    pub fn l_execute_word_list(list: *mut WORD_LIST) -> c_int;
    pub fn xfree(pnt: *mut c_void);
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
        unsafe { xfree(self.0 as *mut c_void) };
    }
}

///////////////////////////////////////////////////////////////////////////

// --- Borrowed View (Zero Allocation) ---

/// Zero-copy, non-owning view over a C `WORD_LIST`.
///
/// Ties the lifetime `'a` to the caller's stack frame or parent handle.
#[derive(Copy, Clone)]
pub struct WordListView<'a> {
    pub head: *mut WORD_LIST,
    _marker: PhantomData<&'a WORD_LIST>,
}

pub type WordListIterOsStr<'a> = Map<WordListIter<'a>, fn(&[u8]) -> &OsStr>;

impl<'a> WordListView<'a> {
    /// Construct a view from a raw `*mut WORD_LIST`.
    ///
    /// # Safety
    /// `head` must point to valid memory or be null, and must not be mutated by C
    /// for the duration of `'a`.
    #[inline]
    pub unsafe fn from_raw(head: *mut WORD_LIST) -> Self {
        Self {
            head,
            _marker: PhantomData,
        }
    }

    /// Yields `&'a OsStr` slices directly referencing C memory.
    #[inline]
    pub fn iter_osstr(&self) -> WordListIterOsStr<'_> {
        self.iter().map(OsStr::from_bytes)
    }

    /// Creates an iterator yielding `&'a OsStr` slices directly referencing C memory.
    #[inline]
    pub fn iter(&self) -> WordListIter<'a> {
        WordListIter {
            head: self.head,
            _marker: PhantomData,
        }
    }
}

impl<'a> IntoIterator for WordListView<'a> {
    type Item = &'a [u8];
    type IntoIter = WordListIter<'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone)]
pub struct WordListIter<'a> {
    pub head: *mut WORD_LIST,
    _marker: PhantomData<&'a WORD_LIST>,
}

impl<'a> WordListIter<'a> {
    pub unsafe fn print(&self) {
        for i in self.clone().into_iter() {
            bprintln!(i);
        }
    }

    pub unsafe fn current_cpnt(&self) -> *const c_char {
        if self.head.is_null() {
            return std::ptr::null();
        }
        let word_ptr = l_word_list_word(self.head);
        if word_ptr.is_null() {
            return std::ptr::null();
        }
        l_word_desc_string(word_ptr)
    }

    pub unsafe fn current(&self) -> Option<&'a [u8]> {
        self.current_cpnt()
            .as_ref()
            .map(|v| CStr::from_ptr(v).to_bytes())
    }
    pub unsafe fn advance(&mut self) {
        if !self.head.is_null() {
            self.head = l_word_list_next(self.head);
        }
    }
}

impl<'a> Iterator for WordListIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let item = self.current();
            self.advance();
            item
        }
    }
}

// --- Owned RAII Container ---

/// RAII wrapper for a `WORD_LIST` freed via `dispose_words`.
#[repr(transparent)]
pub struct WordListOwned(pub *mut WORD_LIST);

impl WordListOwned {
    /// Borrow this owned list as a `WordListView<'a>`.
    #[inline]
    pub fn as_view(&self) -> WordListView<'_> {
        unsafe { WordListView::from_raw(self.0) }
    }

    /// Yields a zero-copy iterator over the owned list's elements.
    #[inline]
    pub fn iter(&self) -> WordListIter<'_> {
        self.as_view().iter()
    }
}

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

impl<'a> IntoIterator for &'a WordListOwned {
    type Item = &'a [u8];
    type IntoIter = WordListIter<'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
