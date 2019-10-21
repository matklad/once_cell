use std::{
    cell::UnsafeCell,
    panic::{RefUnwindSafe, UnwindSafe},
    sync::atomic::{AtomicBool, Ordering},
};

use parking_lot::{lock_api::RawMutex as _RawMutex, RawMutex};

pub(crate) struct OnceCell<T> {
    mutex: Mutex,
    is_initialized: AtomicBool,
    // FIXME: switch to `std::mem::MaybeUninit` once we are ready to bump MSRV
    // that far. It was stabilized in 1.36.0, so, if you are reading this and
    // it's higher than 1.46.0 outside, please send a PR! ;) (and to the same
    // for `Lazy`, while we are at it).
    pub(crate) value: UnsafeCell<Option<T>>,
}

// Why do we need `T: Send`?
// Thread A creates a `OnceCell` and shares it with
// scoped thread B, which fills the cell, which is
// then destroyed by A. That is, destructor observes
// a sent value.
unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}

impl<T: RefUnwindSafe + UnwindSafe> RefUnwindSafe for OnceCell<T> {}
impl<T: UnwindSafe> UnwindSafe for OnceCell<T> {}

impl<T> OnceCell<T> {
    pub(crate) const fn new() -> OnceCell<T> {
        OnceCell {
            mutex: Mutex::new(),
            is_initialized: AtomicBool::new(false),
            value: UnsafeCell::new(None),
        }
    }

    /// Safety: synchronizes with store to value via Release/Acquire.
    #[inline]
    pub(crate) fn is_initialized(&self) -> bool {
        self.is_initialized.load(Ordering::Acquire)
    }

    /// Safety: synchronizes with store to value via `is_initialized` or mutex
    /// lock/unlock, writes value only once because of the mutex.
    #[cold]
    pub(crate) fn initialize<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let _guard = self.mutex.lock();
        if !self.is_initialized() {
            // We are calling user-supplied function and need to be careful.
            // - if it returns Err, we unlock mutex and return without touching anything
            // - if it panics, we unlock mutex and propagate panic without touching anything
            // - if it calls `set` or `get_or_try_init` re-entrantly, we get a deadlock on
            //   mutex, which is important for safety. We *could* detect this and panic,
            //   but that is more complicated
            // - finally, if it returns Ok, we store the value and store the flag with
            //   `Release`, which synchronizes with `Acquire`s.
            let value = f()?;
            let slot: &mut Option<T> = unsafe { &mut *self.value.get() };
            debug_assert!(slot.is_none());
            *slot = Some(value);
            self.is_initialized.store(true, Ordering::Release);
        }
        Ok(())
    }
}

/// Wrapper around parking_lot's `RawMutex` which has `const fn` new.
struct Mutex {
    inner: RawMutex,
}

impl Mutex {
    const fn new() -> Mutex {
        Mutex { inner: RawMutex::INIT }
    }

    fn lock(&self) -> MutexGuard<'_> {
        self.inner.lock();
        MutexGuard { inner: &self.inner }
    }
}

struct MutexGuard<'a> {
    inner: &'a RawMutex,
}

impl Drop for MutexGuard<'_> {
    fn drop(&mut self) {
        self.inner.unlock();
    }
}

#[test]
#[ignore]
// Size depends on the size of `parking_lot::lock_api::RawMutex`, which depends on the compiler
// version. With Rust >= 1.34 `RawMutex` is 1 byte, otherwise pointer sized.
// FIXME: re-enable once the MSVR is >= 1.34
fn test_size() {
    use std::mem::size_of;
    use std::num::NonZeroU32;

    // FIXME: test with `u32` instead of `NonZeroU32` after the switch to use
    // `std::mem::MaybeUninit`.
    assert_eq!(size_of::<OnceCell<NonZeroU32>>(), 8);
}
