use std::{
    ptr,
    sync::{
        Once, ONCE_INIT,
        atomic::{AtomicPtr, Ordering},
    },
};

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
    value: AtomicPtr<T>,
}

impl<T> OnceCell<T> {
    /// An empty cell, for initialization in a `const` context.
    pub const INIT: OnceCell<T> = OnceCell {
        once: ONCE_INIT,
        value: AtomicPtr::new(ptr::null_mut()),
    };

    /// Creates a new empty cell.
    pub fn new() -> OnceCell<T> {
        OnceCell {
            once: Once::new(),
            value: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Gets the reference to the underlying value. Returns `None`
    /// if the cell is empty.
    pub fn get(&self) -> Option<&T> {
        let ptr = self.value.load(Ordering::Acquire);
        unsafe { ptr.as_ref() }
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

    unsafe fn set_inner(&self, value: T) {
        let ptr = Box::into_raw(Box::new(value));
        self.value.store(ptr, Ordering::Release);
    }
}

impl<T> Drop for OnceCell<T> {
    fn drop(&mut self) {
        let ptr = self.value.load(Ordering::Acquire);
        if !ptr.is_null() {
            drop(unsafe { Box::from_raw(ptr) })
        }
    }
}
