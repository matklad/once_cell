#[cfg(feature = "atomic-polyfill")]
use atomic_polyfill as atomic;
#[cfg(not(feature = "atomic-polyfill"))]
use core::sync::atomic;

use atomic::{AtomicU8, Ordering};

use core::panic::{RefUnwindSafe, UnwindSafe};

use crate::unsync::OnceCell as UnsyncOnceCell;

pub(crate) struct OnceCell<T> {
    state: AtomicU8,
    value: UnsyncOnceCell<T>,
}

const INCOMPLETE: u8 = 0;
const RUNNING: u8 = 1;
const COMPLETE: u8 = 2;

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
        OnceCell { state: AtomicU8::new(INCOMPLETE), value: UnsyncOnceCell::new() }
    }

    pub(crate) const fn with_value(value: T) -> OnceCell<T> {
        OnceCell { state: AtomicU8::new(COMPLETE), value: UnsyncOnceCell::with_value(value) }
    }

    #[inline]
    pub(crate) fn is_initialized(&self) -> bool {
        self.state.load(Ordering::Acquire) == COMPLETE
    }

    #[cold]
    pub(crate) fn initialize<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let mut f = Some(f);
        let mut res: Result<(), E> = Ok(());
        let slot: &UnsyncOnceCell<T> = &self.value;
        initialize_inner(&self.state, &mut || {
            let f = unsafe { crate::take_unchecked(&mut f) };
            match f() {
                Ok(value) => {
                    unsafe { crate::unwrap_unchecked(slot.set(value).ok()) };
                    true
                }
                Err(err) => {
                    res = Err(err);
                    false
                }
            }
        });
        res
    }

    #[cold]
    pub(crate) fn wait(&self) {
        while !self.is_initialized() {
            core::hint::spin_loop();
        }
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
        self.value.get_unchecked()
    }

    #[inline]
    pub(crate) fn get_mut(&mut self) -> Option<&mut T> {
        self.value.get_mut()
    }

    #[inline]
    pub(crate) fn into_inner(self) -> Option<T> {
        self.value.into_inner()
    }
}

struct Guard<'a> {
    state: &'a AtomicU8,
    new_state: u8,
}

impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        self.state.store(self.new_state, Ordering::Release);
    }
}

#[inline(never)]
fn initialize_inner(state: &AtomicU8, init: &mut dyn FnMut() -> bool) {
    loop {
        match state.compare_exchange_weak(INCOMPLETE, RUNNING, Ordering::Acquire, Ordering::Acquire)
        {
            Ok(_) => {
                let mut guard = Guard { state, new_state: INCOMPLETE };
                if init() {
                    guard.new_state = COMPLETE;
                }
                return;
            }
            Err(COMPLETE) => return,
            Err(RUNNING) | Err(INCOMPLETE) => core::hint::spin_loop(),
            Err(_) => {
                debug_assert!(false);
                unsafe { core::hint::unreachable_unchecked() }
            }
        }
    }
}
