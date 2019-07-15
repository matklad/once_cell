use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
    hint::unreachable_unchecked,
    panic::{UnwindSafe, RefUnwindSafe},
    fmt,
};

use parking_lot::{
    RawMutex,
    lock_api::RawMutex as _RawMutex,
};

pub(crate) struct OnceCell<T> {
    mutex: Mutex,
    is_initialized: AtomicBool,
    value: UnsafeCell<Option<T>>,
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

impl<T: fmt::Debug> fmt::Debug for OnceCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OnceCell").field("value", &self.get()).finish()
    }
}

impl<T> OnceCell<T> {
    pub(crate) const fn new() -> OnceCell<T> {
        OnceCell {
            mutex: Mutex::new(),
            is_initialized: AtomicBool::new(false),
            value: UnsafeCell::new(None),
        }
    }

    pub(crate) fn get(&self) -> Option<&T> {
        if self.is_initialized.load(Ordering::Acquire) {
            // This is safe: if we've read `true` with `Acquire`, that means
            // we've are paired with `Release` store, which sets the value.
            // Additionally, no one invalidates value after `is_initialized` is
            // set to `true`
            let value: &Option<T> = unsafe { &*self.value.get() };
            value.as_ref()
        } else {
            None
        }
    }

    pub(crate) fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        {
            // We *could* optimistically check here if cell is initialized, but
            // we don't do that, assuming that `set` actually sets the value
            // most of the time.
            let _guard = self.mutex.lock();
            // Relaxed loads are OK under the mutex, because it's mutex
            // unlock/lock that establishes "happens before".
            if !self.is_initialized.load(Ordering::Relaxed) {
                // Uniqueness of reference is guaranteed my mutex and flag
                let slot: &mut Option<T> = unsafe { &mut *self.value.get() };
                debug_assert!(slot.is_none());
                *slot = value.take();
                // This `Release` guarantees that `get` sees only fully stored
                // value
                self.is_initialized.store(true, Ordering::Release);
            }
        }
        match value {
            None => Ok(()),
            Some(value) => Err(value),
        }
    }

    pub(crate) fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        enum Void {}
        match self.get_or_try_init(|| Ok::<T, Void>(f())) {
            Ok(val) => val,
            Err(void) => match void {},
        }
    }

    pub(crate) fn get_or_try_init<F: FnOnce() -> Result<T, E>, E>(&self, f: F) -> Result<&T, E> {
        // Standard double-checked locking pattern.

        // Optimistically check if value is initialized, without locking a
        // mutex.
        if !self.is_initialized.load(Ordering::Acquire) {
            let _guard = self.mutex.lock();
            // Relaxed is OK, because mutex unlock/lock establishes "happens
            // before".
            if !self.is_initialized.load(Ordering::Relaxed) {
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
        }

        // Value is initialized here, because we've read `true` from
        // `is_initialized`, and have a "happens before" due to either
        // Acquire/Release pair (fast path) or mutex unlock (slow path).
        // While we could have just called `get`, that would be twice
        // as slow!
        let value: &Option<T> = unsafe { &*self.value.get() };
        return match value.as_ref() {
            Some(it) => Ok(it),
            None => {
                debug_assert!(false);
                unsafe { unreachable_unchecked() }
            }
        };
    }

    pub(crate) fn into_inner(self) -> Option<T> {
        // Because `into_inner` takes `self` by value, the compiler statically verifies
        // that it is not currently borrowed. So it is safe to move out `Option<T>`.
        self.value.into_inner()
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
#[cfg(pointer_width = "64")]
fn test_size() {
    use std::mem::size_of;

    assert_eq!(size_of::<OnceCell<u32>>, 2 * size_of::<u32>);
}
