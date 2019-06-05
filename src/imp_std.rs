use std::{
    cell::UnsafeCell,
    sync::{
        Once,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug)]
pub(crate) struct OnceCell<T> {
    once: Once,
    value: UnsafeCell<Option<T>>,
    is_initialized: AtomicBool,
}

impl<T> OnceCell<T> {
    pub(crate) const fn new() -> OnceCell<T> {
        OnceCell {
            once: Once::new(),
            value: UnsafeCell::new(None),
            is_initialized: AtomicBool::new(false),
        }
    }

    pub(crate) fn get(&self) -> Option<&T> {
        // This might be a hot path, so use `Acquire` here.
        // It synchronizes with the corresponding `SeqCst`
        // in `set_inner`, which ensures that, when we read the
        // `T` out of the slot below, it was indeed fully written
        // by `set_inner`.
        if self.is_initialized.load(Ordering::Acquire) {
            let slot: &Option<T> = unsafe { &*self.value.get() };
            slot.as_ref()
        } else {
            None
        }
    }

    pub(crate) fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        self.once.call_once(|| {
            let value = value.take().unwrap();
            unsafe { self.set_inner(value) }
        });
        match value {
            None => Ok(()),
            Some(value) => Err(value),
        }
    }

    pub(crate) fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        self.once.call_once(|| {
            let value = f();
            unsafe {
                self.set_inner(value);
            }
        });
        // Value is definitely initialized here, so we don't need
        // synchronization or matching of None. While we can use `Self::get`
        // here, that is twice as slow!
        unsafe {
            let value: &Option<T> = &*self.value.get();
            match value.as_ref() {
                Some(it) => it,
                None => std::hint::unreachable_unchecked(),
            }
        }
    }

    // Unsafe, because must be guarded by `self.once`.
    unsafe fn set_inner(&self, value: T) {
        let slot: &mut Option<T> = &mut *self.value.get();
        *slot = Some(value);
        // This is a cold path, so, while `Release` should be enough,
        // there's no reason not to use `SeqCst`.
        self.is_initialized.store(true, Ordering::SeqCst);
    }
}

// Why do we need `T: Send`?
// Thread A creates a `OnceCell` and shares it with
// scoped thread B, which fills the cell, which is
// then destroyed by A. That is, destructor observes
// a sent value.
unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}
