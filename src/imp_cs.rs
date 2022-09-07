use core::panic::{RefUnwindSafe, UnwindSafe};

use critical_section::{CriticalSection, Mutex};

use crate::unsync::OnceCell as UnsyncOnceCell;

pub(crate) struct OnceCell<T> {
    value: Mutex<UnsyncOnceCell<T>>,
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
        OnceCell { value: Mutex::new(UnsyncOnceCell::new()) }
    }

    pub(crate) const fn with_value(value: T) -> OnceCell<T> {
        OnceCell { value: Mutex::new(UnsyncOnceCell::with_value(value)) }
    }

    #[inline]
    pub(crate) fn is_initialized(&self) -> bool {
        // The only write access is synchronized in `initialize` with a
        // critical section, so read-only access is valid everywhere else.
        unsafe { self.value.borrow(CriticalSection::new()).get().is_some() }
    }

    #[cold]
    pub(crate) fn initialize<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        critical_section::with(|cs| {
            let cell = self.value.borrow(cs);
            cell.get_or_try_init(f).map(|_| ())
        })
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
        // The only write access is synchronized in `initialize` with a
        // critical section, so read-only access is valid everywhere else.
        self.value.borrow(CriticalSection::new()).get_unchecked()
    }

    #[inline]
    pub(crate) fn get_mut(&mut self) -> Option<&mut T> {
        self.value.get_mut().get_mut()
    }

    #[inline]
    pub(crate) fn into_inner(self) -> Option<T> {
        self.value.into_inner().into_inner()
    }
}
