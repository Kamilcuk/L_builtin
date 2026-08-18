use std::os::raw::c_char;

pub const fn max_str_len_for_bits(bits: usize) -> usize {
    // ceil(bits * log10(2)) + 1 (sign) + 1 (null terminator)
    ((bits * 30103) / 100000) + 3
}

pub struct IntStr<const N: usize> {
    buf: [u8; N],
    start: usize,
}

impl<const N: usize> IntStr<N> {
    pub fn as_ptr(&self) -> *const c_char {
        &self.buf[self.start] as *const u8 as *const c_char
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[self.start..N - 1]
    }
}

pub trait ToIntStr {
    type Output: IntStrPtr;
    fn to_intstr(self) -> Self::Output;
}

pub trait IntStrPtr {
    fn as_ptr(&self) -> *const c_char;
}

impl<const N: usize> IntStrPtr for IntStr<N> {
    fn as_ptr(&self) -> *const c_char {
        self.as_ptr()
    }
}

macro_rules! impl_signed_int_str {
    ($t:ty, $bits:expr) => {
        impl ToIntStr for $t {
            type Output = IntStr<{ max_str_len_for_bits($bits) }>;
            fn to_intstr(self) -> Self::Output {
                let mut buf = [0u8; max_str_len_for_bits($bits)];
                let mut i = buf.len() - 1;
                buf[i] = 0;
                let is_negative = self < 0;
                let mut val = self;
                if val == 0 {
                    i -= 1;
                    buf[i] = b'0';
                } else {
                    while val != 0 && i > 0 {
                        i -= 1;
                        let rem = (val % 10).unsigned_abs();
                        buf[i] = b'0' + rem as u8;
                        val /= 10;
                    }
                }
                if is_negative && i > 0 {
                    i -= 1;
                    buf[i] = b'-';
                }
                IntStr { buf, start: i }
            }
        }
    };
}

macro_rules! impl_unsigned_int_str {
    ($t:ty, $bits:expr) => {
        impl ToIntStr for $t {
            type Output = IntStr<{ max_str_len_for_bits($bits) }>;
            fn to_intstr(mut self) -> Self::Output {
                let mut buf = [0u8; max_str_len_for_bits($bits)];
                let mut i = buf.len() - 1;
                buf[i] = 0;
                if self == 0 {
                    i -= 1;
                    buf[i] = b'0';
                } else {
                    while self > 0 && i > 0 {
                        i -= 1;
                        buf[i] = b'0' + (self % 10) as u8;
                        self /= 10;
                    }
                }
                IntStr { buf, start: i }
            }
        }
    };
}

impl_signed_int_str!(i8, 8);
impl_signed_int_str!(i16, 16);
impl_signed_int_str!(i32, 32);
impl_signed_int_str!(i64, 64);
impl_signed_int_str!(isize, core::mem::size_of::<isize>() * 8);

impl_unsigned_int_str!(u8, 8);
impl_unsigned_int_str!(u16, 16);
impl_unsigned_int_str!(u32, 32);
impl_unsigned_int_str!(u64, 64);
impl_unsigned_int_str!(usize, core::mem::size_of::<usize>() * 8);
