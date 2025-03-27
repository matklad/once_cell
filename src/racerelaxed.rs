//! Thread-safe, non-blocking, "first one wins" flavor of `OnceCell` *without*
//! Acquire/Release semantics.
//!
//! If two threads race to initialize a type from the `race` module, they
//! don't block, execute initialization function together, but only one of
//! them stores the result.
//!
//! This module does not require `std` feature.
//!
//! # Atomic orderings
//!
//! All types in this module use `Relaxed` [atomic orderings](Ordering) for all
//! their operations. Any side-effects caused by the setter thread prior to them
//! calling `set` or `get_or_init` will *NOT* necessarily be made visible from
//! the getter thread's perspective.

#[cfg(not(feature = "portable-atomic"))]
use core::sync::atomic;
#[cfg(feature = "portable-atomic")]
use portable_atomic as atomic;

use atomic::{AtomicUsize, Ordering};
use core::num::NonZeroUsize;

/// A thread-safe cell which can be written to only once, *without*
/// Acquire/Release semantics.
#[derive(Default, Debug)]
pub struct OnceNonZeroUsizeRelaxed {
    inner: AtomicUsize,
}

impl OnceNonZeroUsizeRelaxed {
    /// Creates a new empty cell.
    #[inline]
    pub const fn new() -> Self {
        Self { inner: AtomicUsize::new(0) }
    }

    /// Gets the underlying value.
    #[inline]
    pub fn get(&self) -> Option<NonZeroUsize> {
        let val = self.inner.load(Ordering::Relaxed);
        NonZeroUsize::new(val)
    }

    /// Get the reference to the underlying value, without checking if the cell
    /// is initialized.
    ///
    /// # Safety
    ///
    /// Caller must ensure that the cell is in initialized state, and that
    /// the contents are acquired by (synchronized to) this thread.
    pub unsafe fn get_unchecked(&self) -> NonZeroUsize {
        #[inline(always)]
        fn as_const_ptr(r: &AtomicUsize) -> *const usize {
            use core::mem::align_of;

            let p: *const AtomicUsize = r;
            // SAFETY: "This type has the same size and bit validity as
            // the underlying integer type, usize. However, the alignment of
            // this type is always equal to its size, even on targets where
            // usize has a lesser alignment."
            const _ALIGNMENT_COMPATIBLE: () =
                assert!(align_of::<AtomicUsize>() % align_of::<usize>() == 0);
            p.cast::<usize>()
        }

        // TODO(MSRV-1.70): Use `AtomicUsize::as_ptr().cast_const()`
        // See https://github.com/rust-lang/rust/issues/138246.
        let p = as_const_ptr(&self.inner);

        // SAFETY: The caller is responsible for ensuring that the value
        // was initialized and that the contents have been acquired by
        // this thread. Assuming that, we can assume there will be no
        // conflicting writes to the value since the value will never
        // change once initialized. This relies on the statement in
        // https://doc.rust-lang.org/1.83.0/core/sync/atomic/ that "(A
        // `compare_exchange` or `compare_exchange_weak` that does not
        // succeed is not considered a write."
        let val = unsafe { p.read() };

        // SAFETY: The caller is responsible for ensuring the value is
        // initialized and thus not zero.
        unsafe { NonZeroUsize::new_unchecked(val) }
    }

    /// Sets the contents of this cell to `value`.
    ///
    /// Returns `Ok(())` if the cell was empty and `Err(())` if it was
    /// full.
    #[inline]
    pub fn set(&self, value: NonZeroUsize) -> Result<(), ()> {
        match self.compare_exchange(value) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    /// Gets the contents of the cell, initializing it with `f` if the cell was
    /// empty.
    ///
    /// If several threads concurrently run `get_or_init`, more than one `f` can
    /// be called. However, all threads will return the same value, produced by
    /// some `f`.
    pub fn get_or_init<F>(&self, f: F) -> NonZeroUsize
    where
        F: FnOnce() -> NonZeroUsize,
    {
        enum Void {}
        match self.get_or_try_init(|| Ok::<NonZeroUsize, Void>(f())) {
            Ok(val) => val,
            Err(void) => match void {},
        }
    }

    /// Gets the contents of the cell, initializing it with `f` if
    /// the cell was empty. If the cell was empty and `f` failed, an
    /// error is returned.
    ///
    /// If several threads concurrently run `get_or_init`, more than one `f` can
    /// be called. However, all threads will return the same value, produced by
    /// some `f`.
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<NonZeroUsize, E>
    where
        F: FnOnce() -> Result<NonZeroUsize, E>,
    {
        match self.get() {
            Some(it) => Ok(it),
            None => self.init(f),
        }
    }

    #[cold]
    #[inline(never)]
    fn init<E>(&self, f: impl FnOnce() -> Result<NonZeroUsize, E>) -> Result<NonZeroUsize, E> {
        let nz = f()?;
        let mut val = nz.get();
        if let Err(old) = self.compare_exchange(nz) {
            val = old;
        }
        Ok(unsafe { NonZeroUsize::new_unchecked(val) })
    }

    #[inline(always)]
    fn compare_exchange(&self, val: NonZeroUsize) -> Result<usize, usize> {
        self.inner.compare_exchange(0, val.get(), Ordering::Relaxed, Ordering::Relaxed)
    }
}
