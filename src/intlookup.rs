// Integer-keyed compile-time lookup table
// Packs byte strings into integers (u32/u64/u128) for O(log N) const lookup.

#![allow(unused)]

use std::marker::PhantomData;

///////////////////////////////////////////////////

struct ConstRange {
    start: usize,
    end: usize,
}

impl ConstRange {
    const fn new(start: usize, end: usize) -> Self {
        assert!(end > start);
        Self { start, end }
    }

    const fn next(&mut self) -> Option<usize> {
        if self.start < self.end {
            let current = self.start;
            self.start += 1;
            Some(current)
        } else {
            None
        }
    }
}

pub struct ConstRevRange {
    curr: usize,
    end: usize,
}

impl ConstRevRange {
    pub const fn new(start: usize, end: usize) -> Self {
        assert!(start >= end, "ConstRevRange: start must be >= end");
        Self { curr: start, end }
    }

    pub const fn next(&mut self) -> Option<usize> {
        if self.curr > self.end {
            let val = self.curr;
            self.curr -= 1;
            Some(val)
        } else {
            None
        }
    }
}

///////////////////////////////////////////////////

/// Given an array of str and something, return the longest str length.
const fn keys_max_bits<T>(arr: &[(&str, T)]) -> usize {
    assert!(arr.len() != 0, "Array length is 0");
    let mut max = 0;
    let mut range = ConstRange::new(0, arr.len());
    while let Some(i) = range.next() {
        let len = arr[i].0.len();
        if len > max {
            max = len;
        }
    }
    assert!(max != 0, "Max key length is 0");
    max
}

/// Given an array of str and something, return the best type width to represent them.
pub const fn key_type_bits<T>(arr: &[(&str, T)]) -> usize {
    match keys_max_bits(arr) {
        0..=4 => 32,
        0..=8 => 64,
        0..=16 => 128,
        _ => panic!("Key exceeds maximum supported length of 16 bytes or 128 bits"),
    }
}

///////////////////////////////////////////////////

pub struct KeyBits<const BITS: usize, T, const N: usize>(PhantomData<T>);

macro_rules! define_functions {
    ($IntLookupN:ident, $T:ty, $BITS:literal) => {

        impl<T: Copy, const N: usize> KeyBits<$BITS, T, N> {
            pub const fn build(self, keys: &[(&str, T)]) -> $IntLookupN<T, N> {
                $IntLookupN::new(keys)
            }
        }

        /// Represents two arrays - array of keys and connected array of user values.
        pub struct $IntLookupN<V, const N: usize>(pub [$T; N], pub [V; N]);

        impl<V: Copy, const N: usize> $IntLookupN<V, N> {

            /// Pack bytes into an integer $N.
            const fn pack(bytes: &[u8]) -> $T {
                let size = std::mem::size_of::<$T>();
                assert!(
                    bytes.len() <= size,
                    concat!(
                        "Key length exceeds maximum byte capacity of destination integer type '",
                        stringify!($T),
                        "'. Consider using a greater type."
                    )
                );
                assert!(!bytes.is_empty(), "Key cannot be empty");
                let mut val = 0 as $T;
                let mut range = ConstRange::new(0, bytes.len());
                while let Some(i) = range.next() {
                    val |= (bytes[i] as $T) << ((size - 1 - i) * 8);
                }
                val
            }
            /// Returns the number of non-zero bytes in the big-endian packed representation of $T.
            const fn packed_len(val: $T) -> usize {
                if val == 0 {
                    return 0;
                }
                // Leading zeros in $T map to empty tail bytes.
                // Each non-zero byte takes 8 bits.
                let total_bits = (std::mem::size_of::<$T>() * 8) as u32;
                let unused_bits = val.trailing_zeros();
                let occupied_bits = total_bits - unused_bits;
                // Convert occupied bits to whole byte count
                ((occupied_bits + 7) / 8) as usize
            }

            /// Unpack integer $T into a fixed-size byte array of length `LEN`,
            /// stopping at the first zero byte. Returns `([u8; LEN], actual_len)`.
            const fn unpack<const LEN: usize>(val: $T) -> [u8; LEN] {
                let size = std::mem::size_of::<$T>();
                assert!(LEN <= size, "LEN exceeds type size");
                assert!(LEN == Self::packed_len(val), "LEN does not match packed payload length");
                let mut out = [0u8; LEN];
                let mut range = ConstRange::new(0, LEN);
                while let Some(i) = range.next() {
                    let shift = (size - 1 - i) * 8;
                    let byte = ((val >> shift) & 0xFF) as u8;
                    assert!(byte != 0, "Encountered unexpected NUL byte within LEN payload");
                    out[i] = byte;
                }
                out
            }

            /// Given an array of tuples, unpack it into two arrays.
            pub const fn new(arr: &[(&str, V)]) -> Self {
                assert!(
                    arr.len() == N,
                    "Slice length does not match array const generic parameter N"
                );
                let size = std::mem::size_of::<$T>();
                let max_len = keys_max_bits(arr);
                assert!(
                    max_len <= size,
                    concat!(
                        "Key length exceeds integer capacity: max key size requires a larger type than '",
                        stringify!($T),
                        "'"
                    )
                );
                let min_required_len = match size {
                    16 => 9, // u128 requires at least 1 key > 8 bytes (otherwise u64 works)
                    8  => 5, // u64 requires at least 1 key > 4 bytes (otherwise u32 works)
                    _  => 0, // u32 is our minimum size, so 0..=4 bytes is optimal
                };
                assert!(
                    max_len >= min_required_len,
                    concat!(
                        "Suboptimal key type '",
                        stringify!($T),
                        "': all keys fit in a smaller integer type"
                    )
                );
                let mut keys = [0 as $T; N];
                let mut vals = [arr[0].1; N];
                let mut range = ConstRange::new(0, N);
                while let Some(i) = range.next() {
                    keys[i] = Self::pack(arr[i].0.as_bytes());
                    vals[i] = arr[i].1;
                }
                let mut v = Self(keys, vals);
                v.sort();
                v
            }

            /// Sort arrays for binary search lookup.
            const fn sort(&mut self) {
                let mut range = ConstRange::new(0, N);
                while let Some(i) = range.next() {
                    let key = self.0[i];
                    let val = self.1[i];
                    let mut j = i;
                    while j > 0 {
                        let prev_key = self.0[j - 1];
                        assert!(
                            key != prev_key,
                            "Duplicate key detected in intlookup table"
                        );
                        if key >= prev_key {
                            break;
                        }
                        self.0[j] = self.0[j - 1];
                        self.1[j] = self.1[j - 1];
                        j -= 1;
                    }
                    self.0[j] = key;
                    self.1[j] = val;
                }
            }

            /// Find key in array of keys and return associated user value.
            pub fn lookup(&self, key: &[u8]) -> Option<V> {
                // If key is longer then all keys, nothing we can do anyway.
                let size = std::mem::size_of::<$T>();
                if key.len() > size {
                    return None;
                }
                let target = Self::pack(key);
                if false {
                    // Linear scan for small arrays could be vectorized.
                    if N <= 16 {
                        self.lookup_linear(target)
                    } else {
                        self.lookup_binary_search(target)
                    }
                } else {
                    self.lookup_shortest_unique_prefix(key)
                }
            }

            pub fn lookup_shortest_unique_prefix(&self, key: &[u8]) -> Option<V> {
                let size = std::mem::size_of::<$T>();
                if key.len() > size {
                    return None;
                }
                let target = Self::pack(key);
                let shift = (size - key.len()) * 8;
                let mask = (!0 as $T) << shift;
                let masked_target = target & mask;
                // Find the lower bound via binary search
                let mut low = 0;
                let mut high = N;
                while low < high {
                    let mid = low + (high - low) / 2;
                    if (self.0[mid] & mask) < masked_target {
                        low = mid + 1;
                    } else {
                        high = mid;
                    }
                }
                // Check if the lower bound matches the prefix
                if low < N && (self.0[low] & mask) == masked_target {
                    // Match exactly.
                    if self.0[low] == target {
                        return Some(self.1[low]);
                    }
                    // Since keys are sorted, check if the *next* element also matches.
                    // If it does, the prefix is ambiguous (not unique).
                    if low + 1 < N && (self.0[low + 1] & mask) == masked_target {
                        return None; // Ambiguous match
                    }
                    return Some(self.1[low]);
                }
                None
            }

            fn lookup_linear(&self, target: $T) -> Option<V> {
                for i in 0..N {
                    if self.0[i] == target {
                        return Some(self.1[i]);
                    }
                }
                None
            }

            fn lookup_binary_search(&self, target: $T) -> Option<V> {
                let mut base = 0;
                let mut size = N;
                while size > 1 {
                    let half = size / 2;
                    let mid = base + half;
                    base = if self.0[mid] <= target { mid } else { base };
                    size -= half;
                }
                if self.0[base] == target {
                    Some(self.1[base])
                } else {
                    None
                }
            }
        }

    };
}

define_functions!(IntLookup32, u32, 32);
define_functions!(IntLookup64, u64, 64);
define_functions!(IntLookup128, u128, 128);

///////////////////////////////////////////////////

pub const fn infer_bits<const BITS: usize, T, const N: usize>(
    _keys: &[(&str, T)],
) -> KeyBits<BITS, T, N> {
    KeyBits(PhantomData)
}

#[macro_export]
macro_rules! intlookup {
    ($ARR:expr) => {{
        const N: usize = $ARR.len();
        const BITS: usize = $crate::intlookup::key_type_bits($ARR);
        $crate::intlookup::infer_bits::<BITS, _, N>($ARR).build($ARR)
    }};
}
