// There are two pieces of tricky concurrent code in this module that both operate on the same
// atomic `OnceCell::state_and_queue`:
// - one to make sure only one thread is initializing the `OnceCell`, the `state` part.
// - another to manage a queue of waiting threads while the state is RUNNING.
//
// The concept of a queue of waiting threads using a linked list, where every node is a struct on
// the stack of a waiting thread, is taken from `std::sync::Once`.
//
// Differences with `std::sync::Once`:
//   * no poisoning
//   * init function can fail
//   * thread parking is factored out of `initialize`

use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    panic::{RefUnwindSafe, UnwindSafe},
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread::{self, Thread},
};

#[derive(Debug)]
pub(crate) struct OnceCell<T> {
    // `state_and_queue` is actually an encoded version of just a pointer to a
    // `Waiter`, so we add the `PhantomData` appropriately.
    state_and_queue: AtomicUsize,
    _marker: PhantomData<*mut Waiter>,
    // FIXME: switch to `std::mem::MaybeUninit` once we are ready to bump MSRV
    // that far. It was stabilized in 1.36.0, so, if you are reading this and
    // it's higher than 1.46.0 outside, please send a PR! ;) (and to the same
    // for `Lazy`, while we are at it).
    pub(crate) value: UnsafeCell<Option<T>>,
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

// Three states that a OnceCell can be in, encoded into the lower bits of `state_and_queue` in
// the OnceCell structure.
const EMPTY: usize = 0x0;
const RUNNING: usize = 0x1;
const COMPLETE: usize = 0x2;

// Mask to learn about the state. All other bits are the queue of waiters if
// this is in the RUNNING state.
const STATE_MASK: usize = 0x3;

impl<T> OnceCell<T> {
    pub(crate) const fn new() -> OnceCell<T> {
        OnceCell {
            state_and_queue: AtomicUsize::new(EMPTY),
            _marker: PhantomData,
            value: UnsafeCell::new(None),
        }
    }

    /// Safety: synchronizes with store to value via Release/Acquire.
    #[inline]
    pub(crate) fn is_initialized(&self) -> bool {
        // An `Acquire` load is enough because that makes all the initialization
        // operations visible to us, and, this being a fast path, weaker
        // ordering helps with performance.
        self.state_and_queue.load(Ordering::Acquire) == COMPLETE
    }

    #[cold]
    pub(crate) fn initialize<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        // Note on atomic orderings of `self.status`:
        // - The only data that has to be synchronised across threads is `self.value`.
        // - The only store that needs `Release` is after the data is written and `status` is set to
        //   COMPLETE (in the destructor of `wake_waiters_on_drop`).
        //   All other stores can be `Relaxed`.
        // - At the end of this `initialize` function we have to guarantee the data is acquired.
        //   Every load that can lead to the end of this function must have `Acquire` ordering.
        let mut state_and_queue = self.state_and_queue.load(Ordering::Acquire);
        loop {
            match state_and_queue {
                COMPLETE => break,
                EMPTY => {
                    // Try to register this thread as the one doing initialization (RUNNING).
                    let old = self.state_and_queue.compare_and_swap(EMPTY,
                                                                    RUNNING,
                                                                    Ordering::Acquire);
                    if old != EMPTY {
                        state_and_queue = old;
                        continue;
                    }
                    // The destructor of `wake_waiters_on_drop` will do cleanup here.
                    // It will wake up other threads that may have waited on us.
                    // If the closure panics or returns Err it will reset `state_and_queue` to
                    // EMPTY, otherwise to COMPLETED.
                    let mut wake_waiters_on_drop = WaiterQueue::new(&self.state_and_queue);
                    // Run the initialization closure.
                    let value = f()?;
                    let slot: &mut Option<T> = unsafe { &mut *self.value.get() };
                    debug_assert!(slot.is_none());
                    *slot = Some(value);
                    wake_waiters_on_drop.completed = true;
                    break;
                }
                _ => {
                    wait(&self.state_and_queue, state_and_queue);
                    state_and_queue = self.state_and_queue.load(Ordering::Acquire);
                }
            }
        }
        Ok(())
    }
}

// Representation of a node in the linked list of waiters.
struct Waiter {
    thread: Option<Thread>,
    signaled: AtomicBool, // Needs Release and Acquire orderings, because the thread that manages
                          // the `WaiterQueue` reaches inside to take out `thread`.
    next: *mut Waiter,
    // Note: we have to use a raw pointer for `next`. There is an instant right after setting
    // `signaled` where the next thread may free its `Waiter`, while we still hold a live reference.
}

// Head of a linked list of waiters.
// Will wake up the waiters when it gets dropped.
struct WaiterQueue<'a> {
    state_and_queue: &'a AtomicUsize,
    completed: bool,
}

impl WaiterQueue<'_> {
    fn new<'a>(state_and_queue: &'a AtomicUsize) -> WaiterQueue<'a> {
        WaiterQueue {
            state_and_queue: state_and_queue,
            completed: false,
        }
    }
}

fn wait(state_and_queue: &AtomicUsize, current_state: usize) {
    // Create the node for our current thread that we are going to try to slot in at the head of the
    // linked list.
    let mut node = Waiter {
        thread: Some(thread::current()),
        signaled: AtomicBool::new(false),
        next: ptr::null_mut(),
    };
    let me = &mut node as *mut Waiter as usize;
    assert!(me & STATE_MASK == 0); // We assume pointers have an alignment of > 4 bytes,
                                   // the 2 free bits are used as state.

    // Try to slide in the node at the head of the linked list.
    // Run in a loop where we make sure the status is still RUNNING, and that another thread did not
    // just replace the head of the linked list.
    let mut old_head_and_status = current_state;
    loop {
        if old_head_and_status & STATE_MASK != RUNNING {
            return; // No need anymore to enqueue ourselves.
        }

        node.next = (old_head_and_status & !STATE_MASK) as *mut Waiter;
        let old = state_and_queue.compare_and_swap(old_head_and_status,
                                                   me | RUNNING,
                                                   Ordering::Relaxed);
        if old == old_head_and_status {
            break; // Success!
        }
        old_head_and_status = old;
    }

    // We have enqueued ourselves, now lets wait.
    // Guard against spurious wakeups by reparking ourselves until we are signaled.
    while !node.signaled.load(Ordering::Acquire) {
        thread::park();
    }
}

impl Drop for WaiterQueue<'_> {
    fn drop(&mut self) {
        let state_and_queue = if self.completed {
            self.state_and_queue.swap(COMPLETE, Ordering::Release)
        } else {
            self.state_and_queue.swap(EMPTY, Ordering::Relaxed)
        };

        // We should only ever see an old state which was RUNNING.
        assert_eq!(state_and_queue & STATE_MASK, RUNNING);

        // Walk the entire linked list of waiters and wake them up (in lifo order).
        // Note that storing `true` in `signaled` must be the last action (before unparking) because
        // right after that the node can be freed if there happens to be a spurious wakeup.
        unsafe {
            let mut queue = (state_and_queue & !STATE_MASK) as *mut Waiter;
            while !queue.is_null() {
                let next = (*queue).next;
                let thread = (*queue).thread.take().unwrap();
                (*queue).signaled.store(true, Ordering::Release);
                // Here is the reason for using a raw pointer for `next`: at this point we have
                // signaled the other thread and the node may have been freed, while we still hold a
                // live reference (even though we do not use the reference anymore).
                thread.unpark();
                queue = next;
            }
        }
    }
}

// These test are snatched from std as well.
#[cfg(test)]
mod tests {
    use std::panic;
    #[cfg(not(miri))] // miri doesn't support threads
    use std::{sync::mpsc::channel, thread};

    use super::OnceCell;

    impl<T> OnceCell<T> {
        fn init(&self, f: impl FnOnce() -> T) {
            enum Void {}
            let _ = self.initialize(|| Ok::<T, Void>(f()));
        }
    }

    #[test]
    fn smoke_once() {
        static O: OnceCell<()> = OnceCell::new();
        let mut a = 0;
        O.init(|| a += 1);
        assert_eq!(a, 1);
        O.init(|| a += 1);
        assert_eq!(a, 1);
    }

    #[test]
    #[cfg(not(miri))] // miri doesn't support threads
    fn stampede_once() {
        static O: OnceCell<()> = OnceCell::new();
        static mut RUN: bool = false;

        let (tx, rx) = channel();
        for _ in 0..10 {
            let tx = tx.clone();
            thread::spawn(move || {
                for _ in 0..4 {
                    thread::yield_now()
                }
                unsafe {
                    O.init(|| {
                        assert!(!RUN);
                        RUN = true;
                    });
                    assert!(RUN);
                }
                tx.send(()).unwrap();
            });
        }

        unsafe {
            O.init(|| {
                assert!(!RUN);
                RUN = true;
            });
            assert!(RUN);
        }

        for _ in 0..10 {
            rx.recv().unwrap();
        }
    }

    #[test]
    #[cfg(not(miri))] // miri doesn't support panics
    fn poison_bad() {
        static O: OnceCell<()> = OnceCell::new();

        // poison the once
        let t = panic::catch_unwind(|| {
            O.init(|| panic!());
        });
        assert!(t.is_err());

        // we can subvert poisoning, however
        let mut called = false;
        O.init(|| {
            called = true;
        });
        assert!(called);

        // once any success happens, we stop propagating the poison
        O.init(|| {});
    }

    #[test]
    #[cfg(not(miri))] // miri doesn't support panics
    fn wait_for_force_to_finish() {
        static O: OnceCell<()> = OnceCell::new();

        // poison the once
        let t = panic::catch_unwind(|| {
            O.init(|| panic!());
        });
        assert!(t.is_err());

        // make sure someone's waiting inside the once via a force
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();
        let t1 = thread::spawn(move || {
            O.init(|| {
                tx1.send(()).unwrap();
                rx2.recv().unwrap();
            });
        });

        rx1.recv().unwrap();

        // put another waiter on the once
        let t2 = thread::spawn(|| {
            let mut called = false;
            O.init(|| {
                called = true;
            });
            assert!(!called);
        });

        tx2.send(()).unwrap();

        assert!(t1.join().is_ok());
        assert!(t2.join().is_ok());
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn test_size() {
        use std::mem::size_of;

        assert_eq!(size_of::<OnceCell<u32>>(), 4 * size_of::<u32>());
    }
}
