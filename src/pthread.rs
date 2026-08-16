//! Minimal RAII guard for `pthread_mutex_t`.
//!
//! Locks the mutex on construction and unlocks it on drop, so callers never
//! have to remember to unlock on every return path. The guard stores a raw
//! pointer (not a borrow) so the mutex can still be passed to
//! `pthread_cond_wait` / `pthread_cond_timedwait` while the guard is alive.

use std::os::raw::c_int;

/// Locks a `pthread_mutex_t` on construction and unlocks it when dropped.
pub struct PthreadMutexGuard {
    mtx: *mut libc::pthread_mutex_t,
}

impl PthreadMutexGuard {
    /// Lock `mtx`. Returns `Err(1)` if `pthread_mutex_lock` failed.
    ///
    /// # Safety
    ///
    /// `mtx` must point to a valid, initialized `pthread_mutex_t` that stays
    /// valid for the lifetime of the returned guard and is locked by the
    /// calling thread.
    pub unsafe fn lock(mtx: *mut libc::pthread_mutex_t) -> Result<Self, c_int> {
        if libc::pthread_mutex_lock(mtx) != 0 {
            return Err(1);
        }
        Ok(Self { mtx })
    }
}

impl Drop for PthreadMutexGuard {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_mutex_unlock(self.mtx);
        }
    }
}
