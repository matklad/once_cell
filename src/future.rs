use std::{
    cell::UnsafeCell,
    convert::Infallible,
    future::Future,
    panic::{RefUnwindSafe, UnwindSafe},
    pin::Pin,
    ptr,
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
    sync::Mutex,
    task,
};

/// A thread-safe cell which can be written to only once.
///
/// This allows initialization using an async closure which is guaranteed to only be called once.
#[derive(Debug)]
pub struct OnceCell<T> {
    value: UnsafeCell<Option<T>>,
    inner: Inner,
}

unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}

impl<T: RefUnwindSafe + UnwindSafe> RefUnwindSafe for OnceCell<T> {}
impl<T: UnwindSafe> UnwindSafe for OnceCell<T> {}

/// Monomorphic portion of the state
#[derive(Debug)]
struct Inner {
    state: AtomicUsize,
    queue: AtomicPtr<Queue>,
}

/// Transient state during initialization
///
/// Unlike the sync OnceCell, this cannot be a linked list through stack frames, because Futures
/// can be freed at any point by any thread.  Instead, this structure is allocated on the heap
/// during the first initialization call and freed after the value is set (or when the OnceCell is
/// dropped, if the value never gets set).
struct Queue {
    wakers: Mutex<Option<Vec<task::Waker>>>,
}

/// This is somewhat like Arc<Queue>, but holds the refcount in Inner instead of Queue so it can be
/// freed once the cell's initialization is complete.
struct QueueRef<'a> {
    inner: &'a Inner,
    queue: *const Queue,
}
unsafe impl<'a> Sync for QueueRef<'a> {}
unsafe impl<'a> Send for QueueRef<'a> {}

struct QuickInitGuard<'a>(&'a Inner);

/// A Future that waits for acquisition of a QueueHead
struct QueueWaiter<'a> {
    guard: Option<QueueRef<'a>>,
}

/// A guard for the actual initialization of the OnceCell
struct QueueHead<'a> {
    guard: QueueRef<'a>,
}

const NEW: usize = 0x0;
const QINIT_BIT: usize = 1 + (usize::MAX >> 2);
const READY_BIT: usize = 1 + (usize::MAX >> 1);

impl Inner {
    const fn new() -> Self {
        Inner { state: AtomicUsize::new(NEW), queue: AtomicPtr::new(ptr::null_mut()) }
    }

    /// Try to grab a lock without allocating.  This succeeds only if there is no contention, and
    /// on Drop it will check for contention again.
    #[cold]
    fn try_quick_init(&self) -> Option<QuickInitGuard> {
        if self.state.compare_exchange(NEW, QINIT_BIT, Ordering::Acquire, Ordering::Relaxed).is_ok()
        {
            // On success, this acquires a write lock on value
            Some(QuickInitGuard(self))
        } else {
            None
        }
    }

    /// Initialize the queue (if needed) and return a waiter that can be polled to get a QueueHead
    /// that gives permission to initialize the OnceCell.
    ///
    /// The Queue referenced in the returned QueueRef will not be freed until the cell is populated
    /// and all references have been dropped.  If any references remain, further calls to
    /// initialize will return the existing queue.
    #[cold]
    fn initialize(&self) -> QueueWaiter {
        // Increment the queue's reference count.  This ensures that queue won't be freed until we exit.
        let prev_state = self.state.fetch_add(1, Ordering::Acquire);

        // Note: unlike Arc, refcount overflow is impossible.  The only way to increment the
        // refcount is by calling poll on the Future returned by get_or_try_init, which is !Unpin.
        // The poll call requires a Pinned pointer to this Future, and the contract of Pin requires
        // Drop to be called on any !Unpin value that was pinned before the memory is reused.
        // Because the Drop impl of QueueRef decrements the refcount, an overflow would require
        // more than (usize::MAX / 4) QueueRef objects in memory, which is impossible as these
        // objects take up more than 4 bytes.

        let mut guard = QueueRef { inner: self, queue: self.queue.load(Ordering::Acquire) };

        if guard.queue.is_null() && prev_state & READY_BIT == 0 {
            let wakers = if prev_state & QINIT_BIT != 0 {
                // Someone else is taking the fast path; start with no QueueHead available.  As
                // long as our future is pending, QuickInitGuard::drop will observe the nonzero
                // reference count and correctly participate in the queue.
                Mutex::new(Some(Vec::new()))
            } else {
                Mutex::new(None)
            };

            // Race with other callers of initialize to create the queue
            let new_queue = Box::into_raw(Box::new(Queue { wakers }));

            match self.queue.compare_exchange(
                ptr::null_mut(),
                new_queue,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_null) => {
                    // Normal case: it was actually set.  The Release part of AcqRel orders this
                    // with all Acquires on the queue.
                    guard.queue = new_queue;
                }
                Err(actual) => {
                    // we lost the race, but we have the (non-null) value now.
                    guard.queue = actual;
                    // Safety: we just allocated it, and nobody else has seen it
                    unsafe {
                        Box::from_raw(new_queue);
                    }
                }
            }
        }
        QueueWaiter { guard: Some(guard) }
    }

    fn set_ready(&self) {
        // This Release pairs with the Acquire any time we check READY_BIT, and ensures that the
        // writes to the cell's value are visible to the cell's readers.
        let prev_state = self.state.fetch_or(READY_BIT, Ordering::Release);

        debug_assert_eq!(prev_state & READY_BIT, 0, "Invalid state: somoene else set READY_BIT");
    }
}

impl<'a> Drop for QueueRef<'a> {
    fn drop(&mut self) {
        // Release the reference to queue
        let prev_state = self.inner.state.fetch_sub(1, Ordering::Release);
        // Note: as of now, self.queue may be invalid

        let curr_state = prev_state - 1;
        if curr_state == READY_BIT || curr_state == READY_BIT | QINIT_BIT {
            // We just removed the only waiter on an initialized cell.  This means the
            // queue is no longer needed.  Acquire the queue again so we can free it.
            let queue = self.inner.queue.swap(ptr::null_mut(), Ordering::Acquire);
            if !queue.is_null() {
                // Safety: the last guard is being freed, and queue is only used by guard-holders.
                // Due to the swap, we are the only one who is freeing this particualr queue.
                unsafe {
                    Box::from_raw(queue);
                }
            }
        }
    }
}

impl<'a> Drop for QuickInitGuard<'a> {
    fn drop(&mut self) {
        // Relaxed ordering is sufficient here: all decisions are made based solely on the value
        // and timeline of the state value.  On the slow path, initialize acquires the reference as
        // normal and so we don't need any more ordering than that already provides.
        let prev_state = self.0.state.fetch_and(!QINIT_BIT, Ordering::Relaxed);
        if prev_state == QINIT_BIT | READY_BIT {
            return; // fast path, init succeeded. The Release in set_ready was sufficient.
        }
        if prev_state == QINIT_BIT {
            return; // fast path, init failed.  We made no writes that need to be ordered.
        }
        // Get a guard, create the QueueHead we should have been holding, then drop it so that the
        // tasks are woken as intended.  This is needed regardless of if we succeeded or not -
        // either waiters need to run init themselves, or they need to read the value we set.
        let guard = self.0.initialize().guard.unwrap();
        // Note: in strange corner cases we can get a guard with a NULL queue here
        drop(QueueHead { guard })
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let queue = *self.queue.get_mut();
        if !queue.is_null() {
            // Safety: nobody else could have a reference
            unsafe {
                Box::from_raw(queue);
            }
        }
    }
}

impl<'a> Future for QueueWaiter<'a> {
    type Output = Option<QueueHead<'a>>;
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<QueueHead<'a>>> {
        let guard = self.guard.as_ref().expect("Polled future after finished");

        // Fast path for waiters that get notified after the value is set
        let state = guard.inner.state.load(Ordering::Acquire);
        if state & READY_BIT != 0 {
            return task::Poll::Ready(None);
        }

        // Safety: the guard holds a place on the waiter list and we just check that the state is
        // not ready, so the queue is non-null and will remain valid until guard is dropped.
        let queue = unsafe { &*guard.queue };
        let mut lock = queue.wakers.lock().unwrap();

        // Another task might have called set_ready() and dropped its QueueHead between our
        // optimistic lock-free check and our lock acquisition.  Don't return a QueueHead unless we
        // know for sure that we are allowed to initialize.
        let state = guard.inner.state.load(Ordering::Acquire);
        if state & READY_BIT != 0 {
            return task::Poll::Ready(None);
        }

        match lock.as_mut() {
            None => {
                // take the head position and start a waker queue
                *lock = Some(Vec::new());
                drop(lock);

                task::Poll::Ready(Some(QueueHead { guard: self.guard.take().unwrap() }))
            }
            Some(wakers) => {
                // Wait for the QueueHead to be dropped
                let my_waker = cx.waker();
                for waker in wakers.iter() {
                    if waker.will_wake(my_waker) {
                        return task::Poll::Pending;
                    }
                }
                wakers.push(my_waker.clone());
                task::Poll::Pending
            }
        }
    }
}

impl<'a> Drop for QueueHead<'a> {
    fn drop(&mut self) {
        // Safety: if queue is not null, then it is valid as long as the guard is alive
        if let Some(queue) = unsafe { self.guard.queue.as_ref() } {
            let mut lock = queue.wakers.lock().unwrap();
            // Take the waker queue so the next QueueWaiter can make a new one
            let wakers = lock.take().expect("QueueHead dropped without a waker list");
            for waker in wakers {
                waker.wake();
            }
        }
    }
}

impl<T> OnceCell<T> {
    /// Creates a new empty cell.
    pub const fn new() -> Self {
        Self { value: UnsafeCell::new(None), inner: Inner::new() }
    }

    /// Gets the contents of the cell, initializing it with `init` if the cell was empty.
    ///
    /// Many tasks may call `get_or_init` concurrently with different initializing futures, but
    /// it is guaranteed that only one future will be executed as long as the resuting future is
    /// polled to completion.
    ///
    /// If f panics, the panic is propagated to the caller, and the cell remains uninitialized.
    ///
    /// If the Future returned by this function is dropped prior to completion, the cell remains
    /// uninitialized (and another init futures may be selected for polling).
    ///
    /// It is an error to reentrantly initialize the cell from `init`.  The current implementation
    /// deadlocks, but will recover if the offending task is dropped.
    pub async fn get_or_init(&self, init: impl Future<Output = T>) -> &T {
        match self.get_or_try_init(async move { Ok::<T, Infallible>(init.await) }).await {
            Ok(t) => t,
            Err(e) => match e {},
        }
    }

    /// Gets the contents of the cell, initializing it with `init` if the cell was empty.   If the
    /// cell was empty and f failed, an error is returned.
    ///
    /// If f panics, the panic is propagated to the caller, and the cell remains uninitialized.
    ///
    /// If the Future returned by this function is dropped prior to completion, the cell remains
    /// uninitialized.
    ///
    /// It is an error to reentrantly initialize the cell from `init`.  The current implementation
    /// deadlocks, but will recover if the offending task is dropped.
    pub async fn get_or_try_init<E>(
        &self,
        init: impl Future<Output = Result<T, E>>,
    ) -> Result<&T, E> {
        let state = self.inner.state.load(Ordering::Acquire);

        if state & READY_BIT == 0 {
            if state == NEW {
                // If there is no contention, we can initialize without allocations.  Try it.
                if let Some(guard) = self.inner.try_quick_init() {
                    let value = init.await?;
                    unsafe {
                        *self.value.get() = Some(value);
                    }
                    self.inner.set_ready();
                    drop(guard);

                    return Ok(unsafe { (&*self.value.get()).as_ref().unwrap() });
                }
            }

            let guard = self.inner.initialize();
            if let Some(init_lock) = guard.await {
                // We hold the QueueHead, so we know that nobody else has successfully run an init
                // poll and that nobody else can start until it is dropped.  On error, panic, or
                // drop of this Future, the head will be passed to another waiter.
                let value = init.await?;

                // We still hold the head, so nobody else can write to value
                unsafe {
                    *self.value.get() = Some(value);
                }
                // mark the cell ready before giving up the head
                init_lock.guard.inner.set_ready();
                // drop of QueueHead notifies other Futures
                // drop of QueueRef (might) free the Queue
            } else {
                // someone initialized it while waiting on the queue
            }
        }

        // Safety: initialized on all paths
        Ok(unsafe { (&*self.value.get()).as_ref().unwrap() })
    }

    /// Gets the reference to the underlying value.
    ///
    /// Returns `None` if the cell is empty or being initialized. This method never blocks.
    pub fn get(&self) -> Option<&T> {
        let state = self.inner.state.load(Ordering::Acquire);

        if state & READY_BIT == 0 {
            None
        } else {
            unsafe { (&*self.value.get()).as_ref() }
        }
    }

    /// Gets a mutable reference to the underlying value.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.value.get_mut().as_mut()
    }

    /// Takes the value out of this `OnceCell`, moving it back to an uninitialized state.
    pub fn take(&mut self) -> Option<T> {
        self.value.get_mut().take()
    }

    /// Consumes the OnceCell, returning the wrapped value. Returns None if the cell was empty.
    pub fn into_inner(self) -> Option<T> {
        self.value.into_inner()
    }
}
