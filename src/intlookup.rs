use core::ffi::c_char;

// 1. CONST KEY LENGTH CALCULATION
pub(crate) const fn max_key_len<T: Copy, const N: usize>(arr: &[(&'static [u8], T); N]) -> usize {
    let mut max = 0;
    let mut i = 0;
    while i < N {
        if arr[i].0.len() > max {
            max = arr[i].0.len();
        }
        i += 1;
    }
    max
}

// 2. CONCRETE CONST PACKING FUNCTIONS (No Traits)
pub(crate) const fn pack_u32(bytes: &[u8]) -> u32 {
    let mut val = 0u32;
    let mut i = 0;
    while i < bytes.len() {
        val |= (bytes[i] as u32) << ((3 - i) * 8);
        i += 1;
    }
    val
}

pub(crate) const fn pack_u64(bytes: &[u8]) -> u64 {
    let mut val = 0u64;
    let mut i = 0;
    while i < bytes.len() {
        val |= (bytes[i] as u64) << ((7 - i) * 8);
        i += 1;
    }
    val
}

pub(crate) const fn pack_u128(bytes: &[u8]) -> u128 {
    let mut val = 0u128;
    let mut i = 0;
    while i < bytes.len() {
        val |= (bytes[i] as u128) << ((15 - i) * 8);
        i += 1;
    }
    val
}

/////////////////////////////////////////////////////////////////////

// 3. CONCRETE CONST ARRAY PACKERS & SORTING (No Traits)
macro_rules! generate_pack_and_sort_fns {
    ($($ty:ident, $pack_fn:ident, $sort_fn:ident, $pack_arr_fn:ident);* $(;)?) => {
        $(
            pub(crate) const fn $pack_arr_fn<T: Copy, const N: usize>(
                arr: &[(&'static [u8], T); N],
            ) -> [($ty, T); N] {
                let mut packed = [(0 as $ty, arr[0].1); N];
                let mut i = 0;
                while i < N {
                    packed[i] = ($pack_fn(arr[i].0), arr[i].1);
                    i += 1;
                }
                packed
            }

            pub(crate) const fn $sort_fn<T: Copy, const N: usize>(
                mut arr: [($ty, T); N],
            ) -> [($ty, T); N] {
                let mut i = 1;
                while i < N {
                    let item = arr[i];
                    let mut j = i;
                    while j > 0 && item.0 < arr[j - 1].0 {
                        arr[j] = arr[j - 1];
                        j -= 1;
                    }
                    arr[j] = item;
                    i += 1;
                }
                arr
            }
        )*
    };
}

generate_pack_and_sort_fns!(
    u32, pack_u32, sort_packed_u32, pack_array_u32;
    u64, pack_u64, sort_packed_u64, pack_array_u64;
    u128, pack_u128, sort_packed_u128, pack_array_u128;
);

/////////////////////////////////////////////////////////////////////

// 4. UNIFIED ENUM FOR MULTI-WIDTH MACRO RETURN
#[derive(Copy, Clone)]
pub enum PackedTable<T: Copy, const N: usize> {
    U32([(u32, T); N]),
    U64([(u64, T); N]),
    U128([(u128, T); N]),
}

#[inline]
pub const fn build_u32<T: Copy, const N: usize>(
    keys: &[(&'static [u8], T); N],
) -> PackedTable<T, N> {
    PackedTable::U32(sort_packed_u32(pack_array_u32(keys)))
}

#[inline]
pub const fn build_u64<T: Copy, const N: usize>(
    keys: &[(&'static [u8], T); N],
) -> PackedTable<T, N> {
    PackedTable::U64(sort_packed_u64(pack_array_u64(keys)))
}

#[inline]
pub const fn build_u128<T: Copy, const N: usize>(
    keys: &[(&'static [u8], T); N],
) -> PackedTable<T, N> {
    PackedTable::U128(sort_packed_u128(pack_array_u128(keys)))
}



// 5. CONST MACRO DISPATCHER (Pure Const evaluation using if/else inside macro block)
macro_rules! pack_and_sort {
    ($keys:expr) => {{
        const MAX_LEN: usize = $crate::intlookup::max_key_len($keys);
        if MAX_LEN <= 4 {
            $crate::intlookup::build_u32($keys)
        } else if MAX_LEN <= 8 {
            $crate::intlookup::build_u64($keys)
        } else if MAX_LEN <= 16 {
            $crate::intlookup::build_u128($keys)
        } else {
            panic!("Key exceeds maximum supported length of 16 bytes");
        }
    }};
}

// 6. RUNTIME & LOOKUP LOGIC (Traits can be used here at runtime, non-const)
#[inline]
pub unsafe fn bytes_from_c_str<'a>(ptr: *const c_char, max_len: usize) -> &'a [u8] {
    let mut len = 0;
    while len < max_len {
        let byte = *ptr.add(len) as u8;
        if byte == 0 {
            break;
        }
        len += 1;
    }
    core::slice::from_raw_parts(ptr as *const u8, len)
}

macro_rules! impl_const_binary_search {
    ($fn_name:ident, $key_ty:ty) => {
        pub const fn $fn_name<T: Copy, const N: usize>(
            table: &[($key_ty, T); N],
            target: $key_ty,
        ) -> Option<T> {
            let mut left = 0;
            let mut right = N;
            while left < right {
                let mid = left + (right - left) / 2;
                let (k, v) = table[mid];
                if k == target {
                    return Some(v);
                } else if k < target {
                    left = mid + 1;
                } else {
                    right = mid;
                }
            }
            None
        }
    };
}

// Generate const binary search functions for your packed types
impl_const_binary_search!(binary_search_u32, u32);
impl_const_binary_search!(binary_search_u64, u64);
impl_const_binary_search!(binary_search_u128, u128);

impl<T: Copy, const N: usize> PackedTable<T, N> {
    #[inline]
    pub fn lookup_slice(&self, key: &[u8]) -> Option<T> {
        match self {
            PackedTable::U32(table) => {
                if key.len() > 4 {
                    return None;
                }
                binary_search_u32(table, pack_u32(key))
            }
            PackedTable::U64(table) => {
                if key.len() > 8 {
                    return None;
                }
                binary_search_u64(table, pack_u64(key))
            }
            PackedTable::U128(table) => {
                if key.len() > 16 {
                    return None;
                }
                binary_search_u128(table, pack_u128(key))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opt {
    Help,
    Verbose,
    Version,
}

// 1. Compile-time generation:
// - Calculates max key length ("--version" = 9 bytes)
// - Selects `PackedTable::U128`
// - Packs keys into u128 integers (big-endian)
// - Sorts the array at compile time
pub static OPTIONS: PackedTable<Opt, 3> = pack_and_sort!([
    (b"-h", Opt::Help),
    (b"-v", Opt::Verbose),
    (b"--version", Opt::Version),
]);

fn main() {
    // 2. O(log N) lookup matching against packed u128 integers
    assert_eq!(OPTIONS.lookup_slice(b"-h"), Some(Opt::Help));
    assert_eq!(OPTIONS.lookup_slice(b"-v"), Some(Opt::Verbose));
    assert_eq!(OPTIONS.lookup_slice(b"--version"), Some(Opt::Version));

    // Exceeds table width or non-existent key returns None
    assert_eq!(OPTIONS.lookup_slice(b"--nonexistent-flag"), None);
    assert_eq!(OPTIONS.lookup_slice(b"-x"), None);
}
