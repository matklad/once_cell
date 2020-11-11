use core::{
    num::NonZeroUsize,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Default, Debug)]
pub struct OnceNonZeroUsize {
    inner: AtomicUsize,
}

impl OnceNonZeroUsize {
    pub const fn new() -> OnceNonZeroUsize {
        OnceNonZeroUsize { inner: AtomicUsize::new(0) }
    }

    pub fn get(&self) -> Option<NonZeroUsize> {
        let val = self.inner.load(Ordering::Acquire);
        NonZeroUsize::new(val)
    }

    pub fn set(&self, value: NonZeroUsize) -> Result<(), ()> {
        let val = self.inner.compare_and_swap(0, value.get(), Ordering::AcqRel);
        if val == 0 {
            Ok(())
        } else {
            Err(())
        }
    }

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

    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<NonZeroUsize, E>
    where
        F: FnOnce() -> Result<NonZeroUsize, E>,
    {
        let val = self.inner.load(Ordering::Acquire);
        let res = match NonZeroUsize::new(val) {
            Some(it) => it,
            None => {
                let mut val = f()?.get();
                let old_val = self.inner.compare_and_swap(0, val, Ordering::AcqRel);
                if old_val != 0 {
                    val = old_val;
                }
                unsafe { NonZeroUsize::new_unchecked(val) }
            }
        };
        Ok(res)
    }
}

#[derive(Default, Debug)]
pub struct OnceBool {
    inner: OnceNonZeroUsize,
}

impl OnceBool {
    fn from_usize(value: NonZeroUsize) -> bool {
        value.get() == 1
    }
    fn to_usize(value: bool) -> NonZeroUsize {
        unsafe { NonZeroUsize::new_unchecked(if value { 1 } else { 2 }) }
    }

    pub const fn new() -> OnceBool {
        OnceBool { inner: OnceNonZeroUsize::new() }
    }

    pub fn get(&self) -> Option<bool> {
        self.inner.get().map(OnceBool::from_usize)
    }

    pub fn set(&self, value: bool) -> Result<(), ()> {
        self.inner.set(OnceBool::to_usize(value))
    }

    pub fn get_or_init<F>(&self, f: F) -> bool
    where
        F: FnOnce() -> bool,
    {
        OnceBool::from_usize(self.inner.get_or_init(|| OnceBool::to_usize(f())))
    }

    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<bool, E>
    where
        F: FnOnce() -> Result<bool, E>,
    {
        self.inner.get_or_try_init(|| f().map(OnceBool::to_usize)).map(OnceBool::from_usize)
    }
}

#[cfg(feature = "alloc")]
pub use self::once_box::OnceBox;
#[cfg(feature = "alloc")]
mod once_box {
    use alloc::boxed::Box;
    use core::{
        marker::PhantomData,
        ptr,
        sync::atomic::{AtomicPtr, Ordering},
    };

    #[derive(Default, Debug)]
    pub struct OnceBox<T> {
        inner: AtomicPtr<T>,
        ghost: PhantomData<Option<Box<T>>>,
    }

    impl<T> Drop for OnceBox<T> {
        fn drop(&mut self) {
            let ptr = *self.inner.get_mut();
            if !ptr.is_null() {
                drop(unsafe { Box::from_raw(ptr) })
            }
        }
    }

    impl<T> OnceBox<T> {
        pub const fn new() -> OnceBox<T> {
            OnceBox { inner: AtomicPtr::new(ptr::null_mut()), ghost: PhantomData }
        }

        pub fn get(&self) -> Option<&T> {
            let ptr = self.inner.load(Ordering::Acquire);
            if ptr.is_null() {
                return None;
            }
            Some(unsafe { &*ptr })
        }

        // Result<(), Box<T>> here?
        pub fn set(&self, value: T) -> Result<(), ()> {
            let ptr = Box::into_raw(Box::new(value));
            let old_ptr = self.inner.compare_and_swap(ptr::null_mut(), ptr, Ordering::AcqRel);
            if !old_ptr.is_null() {
                drop(unsafe { Box::from_raw(ptr) });
                return Err(());
            }
            Ok(())
        }

        pub fn get_or_init<F>(&self, f: F) -> &T
        where
            F: FnOnce() -> T,
        {
            enum Void {}
            match self.get_or_try_init(|| Ok::<T, Void>(f())) {
                Ok(val) => val,
                Err(void) => match void {},
            }
        }

        pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
        where
            F: FnOnce() -> Result<T, E>,
        {
            let mut ptr = self.inner.load(Ordering::Acquire);

            if ptr.is_null() {
                let val = f()?;
                ptr = Box::into_raw(Box::new(val));
                let old_ptr = self.inner.compare_and_swap(ptr::null_mut(), ptr, Ordering::AcqRel);
                if !old_ptr.is_null() {
                    drop(unsafe { Box::from_raw(ptr) });
                    ptr = old_ptr;
                }
            };
            Ok(unsafe { &*ptr })
        }
    }

    /// ```compile_fail
    /// struct S(*mut ());
    /// unsafe impl Sync for S {}
    ///
    /// fn share<T: Sync>(_: &T) {}
    /// share(&once_cell::race::OnceBox::<S>::new());
    /// ```
    unsafe impl<T: Sync + Send> Sync for OnceBox<T> {}
}
