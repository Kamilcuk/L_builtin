use std::ffi::{c_char, c_int, CStr};
use std::marker::PhantomData;
use std::str::from_utf8_unchecked;

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

pub struct SpecifierIterator<'a> {
    fmt: &'a CStr,
    idx: usize,
}

impl<'a> SpecifierIterator<'a> {
    pub const fn new(fmt: &'a CStr) -> Self {
        Self { fmt, idx: 0 }
    }
    /// Advances the parser and returns `Some((has_dynamic_width, has_dynamic_precision, specifier_str))`
    pub const fn next(&mut self) -> Option<(bool, bool, &'a str)> {
        let bytes = self.fmt.to_bytes();
        while self.idx < bytes.len() {
            if bytes[self.idx] == b'%' {
                // Skip escaped '%%'
                if self.idx + 1 < bytes.len() && bytes[self.idx + 1] == b'%' {
                    self.idx += 2;
                    continue;
                }
                let spec_start = self.idx;
                let mut j = self.idx + 1;
                let mut dynamic_width = false;
                let mut dynamic_prec = false;
                // Skip flags (-, +, 0, space, #)
                while j < bytes.len()
                    && (bytes[j] == b'-'
                        || bytes[j] == b'+'
                        || bytes[j] == b'0'
                        || bytes[j] == b' '
                        || bytes[j] == b'#')
                {
                    j += 1;
                }
                // Check dynamic width (%*s)
                if j < bytes.len() && bytes[j] == b'*' {
                    dynamic_width = true;
                    j += 1;
                } else {
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                }
                // Check dynamic precision (%.*s)
                if j < bytes.len() && bytes[j] == b'.' {
                    j += 1;
                    if j < bytes.len() && bytes[j] == b'*' {
                        dynamic_prec = true;
                        j += 1;
                    } else {
                        while j < bytes.len() && bytes[j].is_ascii_digit() {
                            j += 1;
                        }
                    }
                }
                // Skip length modifiers (h, hh, l, ll, z, j, t, L)
                while j < bytes.len()
                    && (bytes[j] == b'h'
                        || bytes[j] == b'l'
                        || bytes[j] == b'z'
                        || bytes[j] == b'j'
                        || bytes[j] == b't'
                        || bytes[j] == b'L')
                {
                    j += 1;
                }
                if j < bytes.len() {
                    let spec_end = j + 1;
                    self.idx = spec_end;
                    // Safety: CStr byte slices are guaranteed to be valid ASCII/UTF-8
                    let spec_slice = unsafe {
                        let ptr = bytes.as_ptr().add(spec_start);
                        let len = spec_end - spec_start;
                        from_utf8_unchecked(std::slice::from_raw_parts(ptr, len))
                    };
                    return Some((dynamic_width, dynamic_prec, spec_slice));
                }
            }
            self.idx += 1;
        }
        None
    }
}

////////////////////////////////////////////////

/// has width, precision, spec string.
pub type SpecMeta = (bool, bool, &'static str);

pub const fn assert_spec_meta(fmt: &CStr, metas: &[SpecMeta]) {
    let mut iter = SpecifierIterator::new(fmt);
    let mut i = 0;

    while i < metas.len() {
        let (has_w, has_p, expected) = metas[i];
        match iter.next() {
            Some((w, p, spec)) if w == has_w && p == has_p && str_eq(spec, expected) => {}
            Some(_) => panic!("Format specifier mismatch!"),
            None => panic!("Not enough format specifiers for provided arguments!"),
        }
        i += 1;
    }

    if iter.next().is_some() {
        panic!("Format string contains extra specifiers!");
    }
}

////////////////////////////////////////////////

pub type Fmt = *const c_char;
pub type Fn = unsafe extern "C" fn(*const c_char, ...);

pub trait CallCVariadic<Fmt> {
    unsafe fn call(self, func: Fn, fmt: Fmt);
}

impl<A> CallCVariadic<Fmt> for (A,) {
    #[inline(always)]
    unsafe fn call(self, func: Fn, fmt: Fmt) {
        func(fmt, self.0)
    }
}

impl<A, B> CallCVariadic<Fmt> for (A, B) {
    #[inline(always)]
    unsafe fn call(self, func: Fn, fmt: Fmt) {
        func(fmt, self.0, self.1)
    }
}

impl<A, B, C> CallCVariadic<Fmt> for (A, B, C) {
    #[inline(always)]
    unsafe fn call(self, func: Fn, fmt: Fmt) {
        func(fmt, self.0, self.1, self.2)
    }
}

// Helper trait for combining two arguments
pub trait CallCVariadic2<T2> {
    unsafe fn call(self, arg2: T2, func: Fn, fmt: Fmt);
}

impl<A, B> CallCVariadic2<(B,)> for (A,) {
    #[inline(always)]
    unsafe fn call(self, arg2: (B,), func: Fn, fmt: Fmt) {
        func(fmt, self.0, arg2.0)
    }
}

impl<A, B, C> CallCVariadic2<(B, C)> for (A,) {
    #[inline(always)]
    unsafe fn call(self, arg2: (B, C), func: Fn, fmt: Fmt) {
        func(fmt, self.0, arg2.0, arg2.1)
    }
}

impl<A, B, C> CallCVariadic2<(C,)> for (A, B) {
    #[inline(always)]
    unsafe fn call(self, arg2: (C,), func: Fn, fmt: Fmt) {
        func(fmt, self.0, self.1, arg2.0)
    }
}

impl<A, B, C, D> CallCVariadic2<(C, D)> for (A, B) {
    #[inline(always)]
    unsafe fn call(self, arg2: (C, D), func: Fn, fmt: Fmt) {
        func(fmt, self.0, self.1, arg2.0, arg2.1)
    }
}

////////////////////////////////////////////////

pub struct InferSpec<T>(core::marker::PhantomData<T>);
pub struct InferArg<T>(pub T);

macro_rules! define_infer {
    ( $name:ident$(<$lt:lifetime>)?, $nameSpec:ident, $type:ty, $spec_meta:expr, $args_type:ty, $args_expr:expr) => {
        pub struct $name$(<$lt>)?(pub $type);
        impl$(<$lt>)? $name$(<$lt>)? {
            #[inline]
            pub fn into_args(self) -> $args_type {
                ($args_expr as fn(Self) -> $args_type)(self)
            }
        }
        impl$(<$lt>)? InferArg<$type> {
            #[inline]
            pub fn infer(self) -> $name$(<$lt>)? {
                $name(self.0)
            }
        }

        impl$(<$lt>)? InferSpec<$type> {
            #[inline]
            pub const fn spec() -> SpecMeta {
                $spec_meta
            }
        }
    };
}

define_infer!(
    MutRawPtrArg,
    MutRawPtrArgSpec,
    *mut c_char,
    (false, false, "%s"),
    (*const c_char,),
    |s| (s.0 as *const c_char,)
);

define_infer!(
    RefMutRawPtrArg<'a>,
    RefMutRawPtrArgSpec,
    &'a *mut i8,
    (false, false, "%s"),
    (*mut c_char,),
    |s| (*s.0 as *mut c_char,)
);

////////////////////////////////////////////////

#[inline]
pub fn infer_arg<T>(val: &T) -> InferArg<&T> {
    InferArg(val)
}


#[inline]
pub const fn infer_spec<T>(val: &T) -> SpecMeta {
    InferSpec<T>::spec()
}

#[macro_export]
macro_rules! call_c_variadic1 {
    ($func:expr, $fmt:expr, $args:expr) => {
        unsafe {
            // Update the path below to match where you defined the traits
            $crate::variadic::CallCVariadic::call(
                $args,
                $func as unsafe extern "C" fn(_, ...),
                $fmt,
            )
        }
    };
}

#[macro_export]
macro_rules! call_c_variadic2 {
    ($func:expr, $fmt:expr, ($arg1:expr, $arg2:expr)) => {
        unsafe {
            // Update the path below to match where you defined the traits
            $crate::variadic::CallCVariadic2::call(
                $arg1,
                $arg2,
                $func as unsafe extern "C" fn(_, ...),
                $fmt,
            )
        }
    };
}

#[macro_export]
macro_rules! variadic {
    ($func:expr, $fmt:expr $(,)?) => {{
        const _: () = $crate::variadic::assert_spec_meta($fmt, &[]);
        unsafe { $func($fmt.as_ptr()) }
    }};
    ($func:expr, $fmt:expr, $arg:expr $(,)?) => {{
        let raw_arg = $arg;
        let arg = $crate::variadic::infer_arg(&raw_arg).infer();
        const _: () = $crate::variadic::assert_spec_meta($fmt, [arg.spec()]);
        let args = arg.into_args();
        $crate::call_c_variadic1!($func, $fmt.as_ptr(), args);
    }};
    ($func:expr, $fmt:expr, $arg1:expr, $arg2:expr $(,)?) => {{
        let a1 = $crate::variadic::infer_arg(&$arg1).infer();
        let a2 = $crate::variadic::infer_arg(&$arg2).infer();
        const _: () = $crate::variadic::assert_spec_meta($fmt, [a1.spec(), a2.spec()]);
        let args1 = a1.into_args();
        let args2 = a2.into_args();
        $crate::call_c_variadic2!($func, $fmt.as_ptr(), (args1, args2));
    }};
}
