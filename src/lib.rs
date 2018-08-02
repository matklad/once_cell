#[cfg(feature = "parking_lot")]
extern crate parking_lot;

#[macro_use]
pub mod unsync {
    use std::{
        ops::Deref,
        cell::UnsafeCell,
    };

    /// A cell which can be written to only once. Not thread safe.
    ///
    /// Unlike `::std::cell::RefCell`, a `OnceCell` provides simple `&`
    /// references to the contents.
    ///
    /// # Example
    /// ```
    /// use once_cell::unsync::OnceCell;
    ///
    /// let cell = OnceCell::new();
    /// assert!(cell.get().is_none());
    ///
    /// let value: &String = cell.get_or_init(|| {
    ///     "Hello, World!".to_string()
    /// });
    /// assert_eq!(value, "Hello, World!");
    /// assert!(cell.get().is_some());
    /// ```
    #[derive(Debug, Default)]
    pub struct OnceCell<T> {
        // Invariant: written to at most once.
        inner: UnsafeCell<Option<T>>,
    }

    impl<T> OnceCell<T> {
        /// An empty cell, for initialization in a `const` context.
        pub const INIT: OnceCell<T> = OnceCell { inner: UnsafeCell::new(None) };

        /// Creates a new empty cell.
        pub fn new() -> OnceCell<T> {
            OnceCell { inner: UnsafeCell::new(None) }
        }

        /// Gets the reference to the underlying value. Returns `None`
        /// if the cell is empty.
        pub fn get(&self) -> Option<&T> {
            // Safe due to `inner`'s invariant
            unsafe { &*self.inner.get() }.as_ref()
        }

        /// Sets the contents of this cell to `value`. Returns
        /// `Ok(())` if the cell was empty and `Err(value)` if it was
        /// full.
        ///
        /// # Example
        /// ```
        /// use once_cell::unsync::OnceCell;
        ///
        /// let cell = OnceCell::new();
        /// assert!(cell.get().is_none());
        ///
        /// assert_eq!(cell.set(92), Ok(()));
        /// assert_eq!(cell.set(62), Err(62));
        ///
        /// assert!(cell.get().is_some());
        /// ```
        pub fn set(&self, value: T) -> Result<(), T> {
            let slot = unsafe { &mut *self.inner.get() };
            if slot.is_some() {
                return Err(value);
            }
            // This is the only place where we set the slot, no races
            // due to reentrancy/concurrency are possible, and we've
            // checked that slot is currently `None`, so this write
            // maintains the `inner`'s invariant.
            *slot = Some(value);
            Ok(())
        }

        /// Gets the contents of the cell, initializing it with `f`
        /// if cell was empty.
        ///
        /// # Example
        /// ```
        /// use once_cell::unsync::OnceCell;
        ///
        /// let cell = OnceCell::new();
        /// let value = cell.get_or_init(|| 92);
        /// assert_eq!(value, &92);
        /// let value = cell.get_or_init(|| unreachable!());
        /// assert_eq!(value, &92);
        /// ```
        pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
            enum Void {}
            match self.get_or_try_init(|| Ok::<T, Void>(f())) {
                Ok(val) => val,
                Err(void) => match void {},
            }
        }

        /// Gets the contents of the cell, initializing it with `f` if
        /// cell was empty. If the cell was empty and `f` failed, an
        /// error is returned.
        ///
        /// # Example
        /// ```
        /// use once_cell::unsync::OnceCell;
        ///
        /// let cell = OnceCell::new();
        /// assert_eq!(cell.get_or_try_init(|| Err(())), Err(()));
        /// assert!(cell.get().is_none());
        /// let value = cell.get_or_try_init(|| -> Result<i32, ()> {
        ///     Ok(92)
        /// });
        /// assert_eq!(value, Ok(&92));
        /// assert_eq!(cell.get(), Some(&92))
        /// ```
        pub fn get_or_try_init<F: FnOnce() -> Result<T, E>, E>(&self, f: F) -> Result<&T, E> {
            if let Some(val) = self.get() {
                return Ok(val);
            }
            let val = f()?;
            assert!(self.set(val).is_ok(), "reentrant init");
            Ok(self.get().unwrap())
        }
    }

    /// A value which is initialized on the first access.
    ///
    /// # Example
    /// ```
    /// use once_cell::unsync::Lazy;
    ///
    /// let lazy: Lazy<i32> = Lazy::new(|| {
    ///     println!("initializing");
    ///     92
    /// });
    /// println!("A");
    /// println!("{}", *lazy);
    /// println!("{}", *lazy);
    ///
    /// // prints
    /// //   A
    /// //   initializing
    /// //   92
    /// //   92
    /// ```
    #[derive(Debug)]
    pub struct Lazy<T, F: Fn() -> T = fn() -> T> {
        #[doc(hidden)]
        pub __cell: OnceCell<T>,
        #[doc(hidden)]
        pub __init: F,
    }

    impl<T, F: Fn() -> T> Lazy<T, F> {
        /// Creates a new lazy value with the given initializing
        /// function.
        pub fn new(init: F) -> Lazy<T, F> {
            Lazy {
                __cell: OnceCell::INIT,
                __init: init,
            }
        }

        /// Forces the evaluation of this lazy value and
        /// returns a reference to result. This is equivalent
        /// to the `Deref` impl, but is explicit.
        ///
        /// # Example
        /// ```
        /// use once_cell::unsync::Lazy;
        ///
        /// let lazy = Lazy::new(|| 92);
        ///
        /// assert_eq!(Lazy::force(&lazy), &92);
        /// assert_eq!(&*lazy, &92);
        /// ```
        pub fn force(this: &Lazy<T, F>) -> &T {
            this.__cell.get_or_init(|| (this.__init)())
        }
    }

    impl<T, F: Fn() -> T> Deref for Lazy<T, F> {
        type Target = T;
        fn deref(&self) -> &T {
            Lazy::force(self)
        }
    }

    /// Creates a new lazy value initialized by the given closure block.
    /// This macro works in const contexts.
    /// If you need a `move` closure, use `Lazy::new` constructor function.
    ///
    /// # Example
    /// ```
    /// # #[macro_use] extern crate once_cell;
    /// # fn main() {
    /// use once_cell::unsync::Lazy;
    ///
    /// let hello = "Hello, World!".to_string();
    ///
    /// let lazy = unsync_lazy! {
    ///     hello.to_uppercase()
    /// };
    ///
    /// assert_eq!(&*lazy, "HELLO, WORLD!");
    /// # }
    /// ```
    #[macro_export]
    macro_rules! unsync_lazy {
        ($($block:tt)*) => {
            $crate::unsync::Lazy {
                __cell: $crate::unsync::OnceCell::INIT,
                __init: || { $($block)* },
            }
        };
    }
}

#[macro_use]
pub mod sync {
    use std::{
        ptr,
        sync::{atomic::{AtomicPtr, Ordering::Relaxed}},
    };
    #[cfg(feature = "parking_lot")]
    use parking_lot::{Once, ONCE_INIT};
    #[cfg(not(feature = "parking_lot"))]
    use std::sync::{Once, ONCE_INIT};

    #[derive(Debug)]
    pub struct OnceCell<T> {
        // Invariant 1: `inner` is written to only from within `once.call_once`.
        // Corollary 1: inner is written at most once.
        // Corollary 2: all reads & writes to inner are fine with `Relaxed` ordering.
        // Invariant 2: if not null, ptr came from `Box::into_raw`.
        inner: AtomicPtr<T>,
        once: Once,
    }

    // Why do we need `T: Send`?
    // Thread A creates a `OnceCell` and shares it with
    // scoped thread B, which fills the cell, which is
    // then destroyed by A. That is, destructor observes
    // a sent value.
    unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}

    unsafe impl<T: Send> Send for OnceCell<T> {}

    impl<T> OnceCell<T> {
        pub const INIT: OnceCell<T> = OnceCell {
            inner: AtomicPtr::new(ptr::null_mut()),
            once: ONCE_INIT,
        };

        pub fn new() -> OnceCell<T> {
            OnceCell {
                inner: AtomicPtr::new(ptr::null_mut()),
                once: Once::new(),
            }
        }

        pub fn get(&self) -> Option<&T> {
            let ptr = self.inner.load(Relaxed);
            // Safe due to Corollary 1
            unsafe { ptr.as_ref() }
        }

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

        /// Guarantees that only one `f` is  ever called.
        pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
            self.once.call_once(|| {
                let value = f();
                unsafe { self.set_inner(value); }
            });
            self.get().unwrap()
        }

        // Invariant: must be called from `self.once`.
        unsafe fn set_inner(&self, value: T) {
            let ptr = Box::into_raw(Box::new(value));
            self.inner.store(ptr, Relaxed);
        }
    }

    impl<T> Drop for OnceCell<T> {
        fn drop(&mut self) {
            let ptr = self.inner.load(Relaxed);
            if !ptr.is_null() {
                // Safe due to Corollary 2
                drop(unsafe { Box::from_raw(ptr) })
            }
        }
    }

    #[derive(Debug)]
    pub struct Lazy<T, F: Fn() -> T = fn() -> T> {
        #[doc(hidden)]
        pub __cell: OnceCell<T>,
        #[doc(hidden)]
        pub __init: F,
    }

    impl<T, F: Fn() -> T> Lazy<T, F> {
        pub fn new(f: F) -> Lazy<T, F> {
            Lazy {
                __cell: OnceCell::new(),
                __init: f,
            }
        }

        pub fn force(this: &Lazy<T, F>) -> &T {
            this.__cell.get_or_init(|| (this.__init)())
        }
    }

    impl<T, F: Fn() -> T> ::std::ops::Deref for Lazy<T, F> {
        type Target = T;
        fn deref(&self) -> &T {
            Lazy::force(self)
        }
    }

    #[macro_export]
    macro_rules! sync_lazy {
        ($($block:tt)*) => {
            $crate::sync::Lazy {
                __cell: $crate::sync::OnceCell::INIT,
                __init: || { $($block)* },
            }
        };
    }
}
