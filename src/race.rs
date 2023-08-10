//! Thread-safe, non-blocking, "first one wins" flavor of `OnceCell`.
//!
//! If two threads race to initialize a type from the `race` module, they
//! don't block, execute initialization function together, but only one of
//! them stores the result.
//!
//! This module does not require `std` feature.
//!
//! # Atomic orderings
//!
//! All types in this module use `Acquire` and `Release`
//! [atomic orderings](Ordering) for all their operations. While this is not
//! strictly necessary for types other than `OnceBox`, it is useful for users as
//! it allows them to be certain that after `get` or `get_or_init` returns on
//! one thread, any side-effects caused by the setter thread prior to them
//! calling `set` or `get_or_init` will be made visible to that thread; without
//! it, it's possible for it to appear as if they haven't happened yet from the
//! getter thread's perspective. This is an acceptable tradeoff to make since
//! `Acquire` and `Release` have very little performance overhead on most
//! architectures versus `Relaxed`.

#[cfg(feature = "critical-section")]
use atomic_polyfill as atomic;
use core::ptr::NonNull;
#[cfg(not(feature = "critical-section"))]
use core::sync::atomic;

use atomic::{AtomicPtr, AtomicUsize, Ordering};
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::num::NonZeroUsize;
use core::ptr;

/// A thread-safe cell which can be written to only once.
#[derive(Default, Debug)]
pub struct OnceNonZeroUsize {
    inner: AtomicUsize,
}

impl OnceNonZeroUsize {
    /// Creates a new empty cell.
    #[inline]
    pub const fn new() -> OnceNonZeroUsize {
        OnceNonZeroUsize { inner: AtomicUsize::new(0) }
    }

    /// Gets the underlying value.
    #[inline]
    pub fn get(&self) -> Option<NonZeroUsize> {
        let val = self.inner.load(Ordering::Acquire);
        NonZeroUsize::new(val)
    }

    /// Sets the contents of this cell to `value`.
    ///
    /// Returns `Ok(())` if the cell was empty and `Err(())` if it was
    /// full.
    #[inline]
    pub fn set(&self, value: NonZeroUsize) -> Result<(), ()> {
        let exchange =
            self.inner.compare_exchange(0, value.get(), Ordering::AcqRel, Ordering::Acquire);
        match exchange {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    /// Gets the contents of the cell, initializing it with `f` if the cell was
    /// empty.
    ///
    /// If several threads concurrently run `get_or_init`, more than one `f` can
    /// be called. However, all threads will return the same value, produced by
    /// some `f`.
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

    /// Gets the contents of the cell, initializing it with `f` if
    /// the cell was empty. If the cell was empty and `f` failed, an
    /// error is returned.
    ///
    /// If several threads concurrently run `get_or_init`, more than one `f` can
    /// be called. However, all threads will return the same value, produced by
    /// some `f`.
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<NonZeroUsize, E>
    where
        F: FnOnce() -> Result<NonZeroUsize, E>,
    {
        let val = self.inner.load(Ordering::Acquire);
        let res = match NonZeroUsize::new(val) {
            Some(it) => it,
            None => {
                let mut val = f()?.get();
                let exchange =
                    self.inner.compare_exchange(0, val, Ordering::AcqRel, Ordering::Acquire);
                if let Err(old) = exchange {
                    val = old;
                }
                unsafe { NonZeroUsize::new_unchecked(val) }
            }
        };
        Ok(res)
    }
}

/// A thread-safe cell which can be written to only once.
#[derive(Default, Debug)]
pub struct OnceBool {
    inner: OnceNonZeroUsize,
}

impl OnceBool {
    /// Creates a new empty cell.
    #[inline]
    pub const fn new() -> OnceBool {
        OnceBool { inner: OnceNonZeroUsize::new() }
    }

    /// Gets the underlying value.
    #[inline]
    pub fn get(&self) -> Option<bool> {
        self.inner.get().map(OnceBool::from_usize)
    }

    /// Sets the contents of this cell to `value`.
    ///
    /// Returns `Ok(())` if the cell was empty and `Err(())` if it was
    /// full.
    #[inline]
    pub fn set(&self, value: bool) -> Result<(), ()> {
        self.inner.set(OnceBool::to_usize(value))
    }

    /// Gets the contents of the cell, initializing it with `f` if the cell was
    /// empty.
    ///
    /// If several threads concurrently run `get_or_init`, more than one `f` can
    /// be called. However, all threads will return the same value, produced by
    /// some `f`.
    pub fn get_or_init<F>(&self, f: F) -> bool
    where
        F: FnOnce() -> bool,
    {
        OnceBool::from_usize(self.inner.get_or_init(|| OnceBool::to_usize(f())))
    }

    /// Gets the contents of the cell, initializing it with `f` if
    /// the cell was empty. If the cell was empty and `f` failed, an
    /// error is returned.
    ///
    /// If several threads concurrently run `get_or_init`, more than one `f` can
    /// be called. However, all threads will return the same value, produced by
    /// some `f`.
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<bool, E>
    where
        F: FnOnce() -> Result<bool, E>,
    {
        self.inner.get_or_try_init(|| f().map(OnceBool::to_usize)).map(OnceBool::from_usize)
    }

    #[inline]
    fn from_usize(value: NonZeroUsize) -> bool {
        value.get() == 1
    }

    #[inline]
    fn to_usize(value: bool) -> NonZeroUsize {
        unsafe { NonZeroUsize::new_unchecked(if value { 1 } else { 2 }) }
    }
}

/// Public-in-private items to support `T: Sized` and `[T]` for `OnceRef` and
/// `OnceBox`.
mod once_ptr {
    use super::*;

    pub trait OncePointee {
        type OncePtr;

        /// The default uninitialized state.
        const UNINIT: Self::OncePtr;

        fn get(once_ptr: &Self::OncePtr) -> *mut Self;

        fn set(once_ptr: &Self::OncePtr, new: NonNull<Self>) -> Result<(), ()>;

        fn get_or_try_init<F, E>(once_ptr: &Self::OncePtr, f: F) -> Result<NonNull<Self>, E>
        where
            F: FnOnce() -> Result<NonNull<Self>, E>;

        #[cfg(feature = "alloc")]
        unsafe fn drop_box(once_ptr: &mut Self::OncePtr);
    }
}

use once_ptr::*;

impl<T> OncePointee for T {
    type OncePtr = AtomicPtr<T>;

    const UNINIT: AtomicPtr<T> = AtomicPtr::new(ptr::null_mut());

    fn get(once_ptr: &AtomicPtr<T>) -> *mut Self {
        once_ptr.load(Ordering::Acquire)
    }

    fn set(once_ptr: &AtomicPtr<T>, new: NonNull<Self>) -> Result<(), ()> {
        let ptr = new.as_ptr();

        let exchange =
            once_ptr.compare_exchange(ptr::null_mut(), ptr, Ordering::AcqRel, Ordering::Acquire);

        match exchange {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    fn get_or_try_init<F, E>(once_ptr: &Self::OncePtr, f: F) -> Result<NonNull<Self>, E>
    where
        F: FnOnce() -> Result<NonNull<Self>, E>,
    {
        let mut ptr = once_ptr.load(Ordering::Acquire);

        if ptr.is_null() {
            ptr = f()?.as_ptr();

            let exchange = once_ptr.compare_exchange(
                ptr::null_mut(),
                ptr,
                Ordering::AcqRel,
                Ordering::Acquire,
            );

            if let Err(old) = exchange {
                ptr = old;
            }
        }

        Ok(unsafe { NonNull::new_unchecked(ptr) })
    }

    #[cfg(feature = "alloc")]
    unsafe fn drop_box(once_ptr: &mut AtomicPtr<T>) {
        let ptr = *once_ptr.get_mut();
        if !ptr.is_null() {
            drop(alloc::boxed::Box::from_raw(ptr));
        }
    }
}

impl<T> OncePointee for [T] {
    type OncePtr = (AtomicPtr<T>, AtomicUsize);

    // An uninitialized slice pointer has both a null address and invalid
    // length. This enables us to initialize the parts in two separate
    // operations while ensuring that concurrent readers can determine whether
    // the slice is fully initialized.
    //
    // TODO: Use `slice_from_raw_parts_mut` once stable in `const`.
    const UNINIT: Self::OncePtr = (AtomicPtr::new(ptr::null_mut()), AtomicUsize::new(usize::MAX));

    fn get((once_ptr, once_len): &(AtomicPtr<T>, AtomicUsize)) -> *mut Self {
        let mut ptr = ptr::null_mut();

        let len = once_len.load(Ordering::Acquire);
        let len_is_init = (len as isize) >= 0;

        // The pointer is not initialized until after the length is.
        if len_is_init {
            ptr = once_ptr.load(Ordering::Acquire);
        }

        ptr::slice_from_raw_parts_mut(ptr, len)
    }

    fn set(
        (once_ptr, once_len): &(AtomicPtr<T>, AtomicUsize),
        new: NonNull<Self>,
    ) -> Result<(), ()> {
        let new_ptr = new.as_ptr() as *mut T;
        let new_len = unsafe { new.as_ref().len() };

        let exchange =
            once_len.compare_exchange(usize::MAX, new_len, Ordering::AcqRel, Ordering::Acquire);

        match exchange {
            Ok(_) => {
                // We initialized our length first, so store our pointer.
                once_ptr.store(new_ptr, Ordering::Release);
                Ok(())
            }
            Err(_) => Err(()),
        }
    }

    fn get_or_try_init<F, E>(
        (once_ptr, once_len): &(AtomicPtr<T>, AtomicUsize),
        f: F,
    ) -> Result<NonNull<Self>, E>
    where
        F: FnOnce() -> Result<NonNull<Self>, E>,
    {
        let mut ptr = once_ptr.load(Ordering::Acquire);
        let mut len;

        if ptr.is_null() {
            let slice_ptr = f()?;
            ptr = slice_ptr.as_ptr() as *mut T;
            len = unsafe { slice_ptr.as_ref().len() };

            // Attempt to initialize our length.
            let exchange =
                once_len.compare_exchange(usize::MAX, len, Ordering::AcqRel, Ordering::Acquire);

            match exchange {
                Ok(_) => {
                    // We initialized our length first, so store our pointer.
                    once_ptr.store(ptr, Ordering::Release);
                }
                Err(real_len) => {
                    len = real_len;

                    // Spin briefly until the other thread initializes the
                    // pointer.
                    ptr = loop {
                        let real_ptr = once_ptr.load(Ordering::Acquire);
                        if real_ptr.is_null() {
                            core::hint::spin_loop();
                        } else {
                            break real_ptr;
                        }
                    }
                }
            }
        } else {
            // The pointer is not initialized until after the length is, so we
            // can assume a valid slice.
            len = once_len.load(Ordering::Acquire);
        }

        Ok(unsafe { NonNull::new_unchecked(ptr::slice_from_raw_parts_mut(ptr, len)) })
    }

    #[cfg(feature = "alloc")]
    unsafe fn drop_box((ptr, len): &mut (AtomicPtr<T>, AtomicUsize)) {
        let ptr = *ptr.get_mut();
        let len = *len.get_mut();

        if !ptr.is_null() {
            drop(alloc::boxed::Box::from_raw(ptr::slice_from_raw_parts_mut(ptr, len)));
        }
    }
}

/// A thread-safe cell which can be written to only once.
pub struct OnceRef<'a, T: ?Sized + OncePointee> {
    inner: T::OncePtr,
    ghost: PhantomData<UnsafeCell<&'a T>>,
}

// TODO: Replace UnsafeCell with SyncUnsafeCell once stabilized
unsafe impl<'a, T: ?Sized + Sync + OncePointee> Sync for OnceRef<'a, T> {}

impl<'a, T> core::fmt::Debug for OnceRef<'a, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "OnceRef({:?})", self.inner)
    }
}

impl<'a, T> core::fmt::Debug for OnceRef<'a, [T]> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (ptr, _) = &self.inner;
        write!(f, "OnceRef({:?})", ptr)
    }
}

impl<'a, T: ?Sized + OncePointee> Default for OnceRef<'a, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T: ?Sized + OncePointee> OnceRef<'a, T> {
    /// Creates a new empty cell.
    pub const fn new() -> OnceRef<'a, T> {
        OnceRef { inner: T::UNINIT, ghost: PhantomData }
    }

    /// Gets a reference to the underlying value.
    pub fn get(&self) -> Option<&'a T> {
        let ptr = <T as OncePointee>::get(&self.inner);
        unsafe { ptr.as_ref() }
    }

    /// Sets the contents of this cell to `value`.
    ///
    /// Returns `Ok(())` if the cell was empty and `Err(())` if it was full.
    pub fn set(&self, value: &'a T) -> Result<(), ()> {
        <T as OncePointee>::set(&self.inner, NonNull::from(value))
    }

    /// Gets the contents of the cell, initializing it with `f` if the cell was
    /// empty.
    ///
    /// If several threads concurrently run `get_or_init`, more than one `f` can
    /// be called. However, all threads will return the same value, produced by
    /// some `f`.
    pub fn get_or_init<F>(&self, f: F) -> &'a T
    where
        F: FnOnce() -> &'a T,
    {
        enum Void {}
        match self.get_or_try_init(|| Ok::<&'a T, Void>(f())) {
            Ok(val) => val,
            Err(void) => match void {},
        }
    }

    /// Gets the contents of the cell, initializing it with `f` if
    /// the cell was empty. If the cell was empty and `f` failed, an
    /// error is returned.
    ///
    /// If several threads concurrently run `get_or_init`, more than one `f` can
    /// be called. However, all threads will return the same value, produced by
    /// some `f`.
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&'a T, E>
    where
        F: FnOnce() -> Result<&'a T, E>,
    {
        <T as OncePointee>::get_or_try_init(&self.inner, || f().map(NonNull::from))
            .map(|ptr| unsafe { ptr.as_ref() })
    }

    /// ```compile_fail
    /// use once_cell::race::OnceRef;
    ///
    /// let mut l = OnceRef::new();
    ///
    /// {
    ///     let y = 2;
    ///     let mut r = OnceRef::new();
    ///     r.set(&y).unwrap();
    ///     core::mem::swap(&mut l, &mut r);
    /// }
    ///
    /// // l now contains a dangling reference to y
    /// eprintln!("uaf: {}", l.get().unwrap());
    /// ```
    fn _dummy() {}
}

#[cfg(feature = "alloc")]
pub use self::once_box::OnceBox;

#[cfg(feature = "alloc")]
mod once_box {
    use core::{marker::PhantomData, ptr::NonNull};

    use alloc::boxed::Box;

    use super::OncePointee;

    /// A thread-safe cell which can be written to only once.
    pub struct OnceBox<T: ?Sized + OncePointee> {
        inner: T::OncePtr,
        ghost: PhantomData<Option<Box<T>>>,
    }

    impl<T> core::fmt::Debug for OnceBox<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "OnceBox({:?})", self.inner)
        }
    }

    impl<T> core::fmt::Debug for OnceBox<[T]> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            let (ptr, _) = &self.inner;
            write!(f, "OnceBox({:?})", ptr)
        }
    }

    impl<T: ?Sized + OncePointee> Default for OnceBox<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T: ?Sized + OncePointee> Drop for OnceBox<T> {
        fn drop(&mut self) {
            unsafe { T::drop_box(&mut self.inner) }
        }
    }

    impl<T: ?Sized + OncePointee> OnceBox<T> {
        /// Creates a new empty cell.
        pub const fn new() -> OnceBox<T> {
            OnceBox { inner: T::UNINIT, ghost: PhantomData }
        }

        /// Gets a reference to the underlying value.
        pub fn get(&self) -> Option<&T> {
            let ptr = <T as OncePointee>::get(&self.inner);
            unsafe { ptr.as_ref() }
        }

        /// Sets the contents of this cell to `value`.
        ///
        /// Returns `Ok(())` if the cell was empty and `Err(value)` if it was
        /// full.
        pub fn set(&self, value: Box<T>) -> Result<(), Box<T>> {
            let ptr = Box::into_raw(value);

            let result =
                <T as OncePointee>::set(&self.inner, unsafe { NonNull::new_unchecked(ptr) });

            if result.is_err() {
                let value = unsafe { Box::from_raw(ptr) };
                return Err(value);
            }

            Ok(())
        }

        /// Gets the contents of the cell, initializing it with `f` if the cell was
        /// empty.
        ///
        /// If several threads concurrently run `get_or_init`, more than one `f` can
        /// be called. However, all threads will return the same value, produced by
        /// some `f`.
        pub fn get_or_init<F>(&self, f: F) -> &T
        where
            F: FnOnce() -> Box<T>,
        {
            enum Void {}
            match self.get_or_try_init(|| Ok::<Box<T>, Void>(f())) {
                Ok(val) => val,
                Err(void) => match void {},
            }
        }

        /// Gets the contents of the cell, initializing it with `f` if
        /// the cell was empty. If the cell was empty and `f` failed, an
        /// error is returned.
        ///
        /// If several threads concurrently run `get_or_init`, more than one `f` can
        /// be called. However, all threads will return the same value, produced by
        /// some `f`.
        pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
        where
            F: FnOnce() -> Result<Box<T>, E>,
        {
            let mut our_ptr: Option<NonNull<T>> = None;

            let ptr = <T as OncePointee>::get_or_try_init(&self.inner, || {
                let ptr = Box::into_raw(f()?);
                let ptr = unsafe { NonNull::new_unchecked(ptr) };
                our_ptr = Some(ptr);
                Ok(ptr)
            })?;

            // Deallocate our box if it's not the one that's stored.
            if let Some(our_ptr) = our_ptr {
                if ptr != our_ptr {
                    drop(unsafe { Box::from_raw(our_ptr.as_ptr()) });
                }
            }

            Ok(unsafe { ptr.as_ref() })
        }
    }

    unsafe impl<T: ?Sized + OncePointee + Sync + Send> Sync for OnceBox<T> {}

    /// ```compile_fail
    /// struct S(*mut ());
    /// unsafe impl Sync for S {}
    ///
    /// fn share<T: Sync>(_: &T) {}
    /// share(&once_cell::race::OnceBox::<S>::new());
    /// ```
    fn _dummy() {}
}
