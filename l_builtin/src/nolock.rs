use std::cell::UnsafeCell;

#[repr(transparent)]
struct NoLock<T>(UnsafeCell<T>);
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
