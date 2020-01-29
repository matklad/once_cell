use std::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    panic::{RefUnwindSafe, UnwindSafe},
    sync::atomic::{AtomicBool, Ordering},
};

use parking_lot::{lock_api::RawMutex as _RawMutex, RawMutex};

pub(crate) struct OnceCell<T> {
    mutex: Mutex,
    is_initialized: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
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
            value: UnsafeCell::new(MaybeUninit::uninit()),
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
            // Safe b/c we have a unique access and no panic may happen
            // until the cell is marked as initialized.
            unsafe { self.value.get().write(MaybeUninit::new(value)) };
            self.is_initialized.store(true, Ordering::Release);
        }
        Ok(())
    }

    /// Get the reference to the underlying value, without checking if the cell
    /// is initialized.
    ///
    /// # Safety
    ///
    /// Caller must ensure that the cell is in initialized state, and that
    /// the contents are acquired by (synchronized to) this thread.
    pub(crate) unsafe fn get_unchecked(&self) -> &T {
        debug_assert!(self.is_initialized());
        let slot: &MaybeUninit<T> = &*self.value.get();
        &*slot.as_ptr()
    }

    /// Gets the mutable reference to the underlying value.
    /// Returns `None` if the cell is empty.
    pub(crate) fn get_mut(&mut self) -> Option<&mut T> {
        if self.is_initialized() {
            // Safe b/c we have a unique access and value is initialized.
            unsafe {
                let slot: &mut MaybeUninit<T> = &mut *self.value.get();
                Some(&mut *slot.as_mut_ptr())
            }
        } else {
            None
        }
    }

    /// Consumes this `OnceCell`, returning the wrapped value.
    /// Returns `None` if the cell was empty.
    pub(crate) fn into_inner(mut self) -> Option<T> {
        // Because `into_inner` takes `self` by value, the compiler statically
        // verifies that it is not currently borrowed.
        // So, it is safe to move out `Option<T>`.
        //
        // Safe b/c marking this `OnceCell` as uninitialized below.
        let value = unsafe { self.take_inner() };
        if value.is_some() {
            // `Relaxed` is OK here, because `self` is taking by value, so no
            // concurrent operations are possible.
            self.is_initialized.store(false, Ordering::Relaxed);
        }
        value
    }

    /// Takes the wrapped value out of this `OnceCell`.
    ///
    /// # Safety
    ///
    /// After moving out contained value this `OnceCell` will contain
    /// uninitialized memory while still being considered as initialized,
    /// so subsequent calls will cause UB (reading uninitialized memory).
    ///
    /// It's up to the caller to guarantee that the `OnceCell` is marked
    /// as uninitialized in case this function returns `Some(value)`
    /// and subsequent calls will be performed.
    ///
    /// The reason we're not marking this `OnceCell` as uninitialized inside
    /// this function is to avoid redundant performance penalties on regular
    /// drops.
    ///
    /// Only used by `into_inner` and `drop`.
    unsafe fn take_inner(&mut self) -> Option<T> {
        // The mutable reference guarantees there are no other threads observing
        // us taking out the contained value.
        // Right after this function `self` is supposed to be freed, so it makes
        // little sense to atomically set the state to uninitialized.
        if self.is_initialized() {
            let value = mem::replace(&mut self.value, UnsafeCell::new(MaybeUninit::uninit()));
            Some(value.into_inner().assume_init())
        } else {
            None
        }
    }
}

impl<T> Drop for OnceCell<T> {
    fn drop(&mut self) {
        // Safe b/c there is no subsequent `take_inner` calls.
        unsafe { self.take_inner() };
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
fn test_size() {
    use std::mem::size_of;

    assert_eq!(size_of::<OnceCell<bool>>(), 2 * size_of::<bool>() + size_of::<u8>());
}
