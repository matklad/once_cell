extern crate parking_lot;

use std::cell::UnsafeCell;
use self::parking_lot::{Once, OnceState};

#[derive(Debug)]
pub(crate) struct OnceCell<T> {
    once: Once,
    value: UnsafeCell<Option<T>>,
}


impl<T> OnceCell<T> {
    pub(crate) const INIT: OnceCell<T> = OnceCell {
        once: Once::new(),
        value: UnsafeCell::new(None),
    };

    pub(crate) fn new() -> OnceCell<T> {
        OnceCell {
            once: Once::new(),
            value: UnsafeCell::new(None),
        }
    }

    pub(crate) fn get(&self) -> Option<&T> {
        if self.once.state() == OnceState::Done {
            let value: &Option<T> = unsafe { &*self.value.get() };
            value.as_ref()
        } else {
            None
        }
    }

    pub(crate) fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        self.once.call_once(|| {
            let slot: &mut Option<T> = unsafe { &mut *self.value.get() };
            *slot = value.take();
        });
        match value {
            None => Ok(()),
            Some(value) => Err(value)
        }
    }

    pub(crate) fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        self.once.call_once(|| {
            let value = f();
            let slot: &mut Option<T> = unsafe { &mut *self.value.get() };
            *slot = Some(value);
        });
        self.get().unwrap()
    }
}

// Why do we need `T: Send`?
// Thread A creates a `OnceCell` and shares it with
// scoped thread B, which fills the cell, which is
// then destroyed by A. That is, destructor observes
// a sent value.
unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}
