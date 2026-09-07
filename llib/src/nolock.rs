use std::cell::UnsafeCell;

#[repr(transparent)]
pub struct NoLock<T>(UnsafeCell<T>);
unsafe impl<T> Sync for NoLock<T> {}

impl<T> NoLock<T> {
    pub const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }
    pub fn get(&self) -> &T {
        unsafe { &*self.0.get() }
    }
    pub fn set(&self, value: T) {
        unsafe {
            *self.0.get() = value;
        }
    }
}
impl<T> NoLock<Option<T>> {
    pub fn get_or_try_init<E, F>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        unsafe {
            let slot = &mut *self.0.get();
            if slot.is_none() {
                *slot = Some(f()?);
            }
            Ok(slot.as_ref().unwrap())
        }
    }
}



/// `*const c_char` made `Sync` so a `static` array of C string literals can
/// back a `long_doc` field across threads without unsafe blocks.
#[repr(transparent)]
pub struct SyncPtr<T>(pub T);
unsafe impl<T> Sync for SyncPtr<T> {}
impl<T> SyncPtr<T> {
    pub const fn as_ptr(self) -> T
    where
        T: Copy,
    {
        self.0
    }
}

/// Build a null-terminated array of `c_char` pointers suitable for a bash
/// `long_doc` field. Each `$cstr` literal is converted to a `SyncPtr`, and a
/// trailing `SyncPtr(null)` is appended as the required sentinel. The macro
/// accepts a comma-separated list of `c"..."` literals. The matching
/// `doc_array_len!` macro counts the same entries (including `cfg`-gated
/// ones) at compile time so the array length does not have to be maintained
/// by hand.
#[macro_export]
macro_rules! doc_array {
    ($( $( #[cfg($meta:meta)] )? $cstr:literal ),* $(,)?) => {
        [
            $(
                $( #[cfg($meta)] )?
                $crate::nolock::SyncPtr($cstr.as_ptr()),
            )*
            $crate::nolock::SyncPtr(core::ptr::null()),
        ]
    };
}
