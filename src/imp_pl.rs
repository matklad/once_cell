extern crate parking_lot;

use std::cell::UnsafeCell;
use self::parking_lot::{Once, ONCE_INIT, OnceState};

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
}

impl<T> Default for OnceCell<T> {
    fn default() -> OnceCell<T> {
        OnceCell::new()
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
    };

    /// Creates a new empty cell.
    pub fn new() -> OnceCell<T> {
        OnceCell {
            once: ONCE_INIT,
            value: UnsafeCell::new(None),
        }
    }

    /// Gets the reference to the underlying value. Returns `None`
    /// if the cell is empty.
    pub fn get(&self) -> Option<&T> {
        if self.once.state() == OnceState::Done {
            let value: &Option<T> = unsafe { &*self.value.get() };
            value.as_ref()
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
            let slot: &mut Option<T> = unsafe { &mut *self.value.get() };
            *slot = value.take();
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
