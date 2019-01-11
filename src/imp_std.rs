use std::cell::UnsafeCell;
use std::sync::{Once, ONCE_INIT};
use std::sync::atomic::{AtomicBool, Ordering};

/// A thread-safe cell which can be written to only once.
///
/// Unlike `::std::sync::Mutex`, a `OnceCell` provides simple `&`
/// references to the contents.
///
/// # Example
/// ```
/// use once_cell::sync::OnceCell;
///
/// static CELL: OnceCell<String> = OnceCell::INIT;
/// assert!(CELL.get().is_none());
///
/// ::std::thread::spawn(|| {
///     let value: &String = CELL.get_or_init(|| {
///         "Hello, World!".to_string()
///     });
///     assert_eq!(value, "Hello, World!");
/// }).join().unwrap();
///
/// let value: Option<&String> = CELL.get();
/// assert!(value.is_some());
/// assert_eq!(value.unwrap().as_str(), "Hello, World!");
/// ```
#[derive(Debug)]
pub struct OnceCell<T> {
    once: Once,
    value: UnsafeCell<Option<T>>,
    is_initialized: AtomicBool,
}

impl<T> Default for OnceCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<T> for OnceCell<T> {
    fn from(value: T) -> Self {
        let cell = Self::new();
        cell.get_or_init(|| value);
        cell
    }
}

impl<T: PartialEq> PartialEq for OnceCell<T> {
    fn eq(&self, other: &OnceCell<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T> OnceCell<T> {
    /// An empty cell, for initialization in a `const` context.
    pub const INIT: OnceCell<T> = OnceCell {
        once: ONCE_INIT,
        value: UnsafeCell::new(None),
        is_initialized: AtomicBool::new(false),
    };

    /// Creates a new empty cell.
    pub fn new() -> OnceCell<T> {
        OnceCell {
            once: Once::new(),
            value: UnsafeCell::new(None),
            is_initialized: AtomicBool::new(false),
        }
    }

    /// Gets the reference to the underlying value. Returns `None`
    /// if the cell is empty.
    pub fn get(&self) -> Option<&T> {
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

    /// Sets the contents of this cell to `value`. Returns
    /// `Ok(())` if the cell was empty and `Err(value)` if it was
    /// full.
    ///
    /// # Example
    /// ```
    /// use once_cell::sync::OnceCell;
    ///
    /// static CELL: OnceCell<i32> = OnceCell::INIT;
    ///
    /// fn main() {
    ///     assert!(CELL.get().is_none());
    ///
    ///     ::std::thread::spawn(|| {
    ///         assert_eq!(CELL.set(92), Ok(()));
    ///     }).join().unwrap();
    ///
    ///     assert_eq!(CELL.set(62), Err(62));
    ///     assert_eq!(CELL.get(), Some(&92));
    /// }
    /// ```
    pub fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        self.once.call_once(|| {
            let value = value.take().unwrap();
            unsafe { self.set_inner(value) }
        });
        match value {
            None => Ok(()),
            Some(value) => Err(value)
        }
    }

    /// Gets the contents of the cell, initializing it with `f`
    /// if the cell was empty. May threads may call `get_or_init`
    /// concurrently with different initializing functions, but
    /// it is guaranteed that only one function will be executed.
    ///
    /// # Example
    /// ```
    /// use once_cell::sync::OnceCell;
    ///
    /// let cell = OnceCell::new();
    /// let value = cell.get_or_init(|| 92);
    /// assert_eq!(value, &92);
    /// let value = cell.get_or_init(|| unreachable!());
    /// assert_eq!(value, &92);
    /// ```
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        self.once.call_once(|| {
            let value = f();
            unsafe { self.set_inner(value); }
        });
        self.get().unwrap()
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
