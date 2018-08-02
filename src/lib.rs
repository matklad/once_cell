#[macro_use]
pub mod unsync {
    use std::{
        ops::Deref,
        cell::UnsafeCell,
    };

    #[derive(Debug, Default)]
    pub struct OnceCell<T> {
        // Invariant: written to at most once.
        inner: UnsafeCell<Option<T>>,
    }

    impl<T> OnceCell<T> {
        pub const INIT: OnceCell<T> = OnceCell { inner: UnsafeCell::new(None) };

        pub fn new() -> OnceCell<T> {
            OnceCell { inner: UnsafeCell::new(None) }
        }

        pub fn get(&self) -> Option<&T> {
            // Safe due to `inner`'s invariant
            unsafe { &*self.inner.get() }.as_ref()
        }

        pub fn set(&self, value: T) -> Result<(), T> {
            let slot = unsafe { &mut *self.inner.get() };
            if slot.is_some() {
                return Err(value);
            }
            // This is the only place where we set the slot,
            // no races due to reentrancy/concurrency are possible,
            // and we've checked that slot is currently `None`, so
            // this write maintains the `inner`'s invariant.
            *slot = Some(value);
            Ok(())
        }

        pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
            enum Void {}
            match self.get_or_try_init(|| Ok::<T, Void>(f())) {
                Ok(val) => val,
                Err(void) => match void {},
            }
        }

        pub fn get_or_try_init<E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<&T, E> {
            if let Some(val) = self.get() {
                return Ok(val);
            }
            let val = f()?;
            assert!(self.set(val).is_ok(), "reentrant init");
            Ok(self.get().unwrap())
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
                __cell: OnceCell::INIT,
                __init: f,
            }
        }
    }

    impl<T, F: Fn() -> T> Deref for Lazy<T, F> {
        type Target = T;
        fn deref(&self) -> &T {
            self.__cell.get_or_init(|| (self.__init)())
        }
    }

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
        sync::{Once, atomic::{AtomicPtr, Ordering::Relaxed}},
        ops::Deref,
    };

    #[derive(Debug)]
    pub struct OnceCell<T> {
        // Invariant: `inner` is written to only from within `once`.
        // Corollary: inner is written at most once.
        // Corollary: all reads & writes to inner are fine with `Relaxed` ordering.
        // Invariant: if not null, ptr came from `Box::into_raw`.
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
            once: Once::new(),
        };

        pub fn new() -> OnceCell<T> {
            OnceCell {
                inner: AtomicPtr::new(ptr::null_mut()),
                once: Once::new(),
            }
        }

        pub fn get(&self) -> Option<&T> {
            let ptr = self.inner.load(Relaxed);
            // Safe due to Corollary
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
        pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
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
            // Safe due to Corollary
            unsafe {
                drop(Box::from_raw(ptr))
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
    }

    impl<T, F: Fn() -> T> Deref for Lazy<T, F> {
        type Target = T;
        fn deref(&self) -> &T {
            self.__cell.get_or_init(|| (self.__init)())
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
