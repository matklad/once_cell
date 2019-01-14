/*!
# Overview

`once_cell` provides two new cell-like types, `unsync::OnceCell` and `sync::OnceCell`. `OnceCell`
might store arbitrary non-`Copy` types, can be assigned to at most once and provide direct access
to the stored contents. In a nutshell, API looks *roughly* like this:

```no-run
impl OnceCell<T> {
    fn set(&self, value: T) -> Result<(), T> { ... }
    fn get(&self) -> Option<&T> { ... }
}
```

Note that, like with `RefCell` and `Mutex`, the `set` method requires only a shared reference.
Because of the single assignment restriction `get` can return an `&T` instead of `ReF<T>`
or `MutexGuard<T>`.

# Patterns

`OnceCell` might be useful for a variety of patterns.

## Safe Initialization of global data


```
use std::{env, io};
use once_cell::sync::OnceCell;

#[derive(Debug)]
pub struct Logger {
    // ...
}
static INSTANCE: OnceCell<Logger> = OnceCell::INIT;

impl Logger {
    pub fn global() -> &'static Logger {
        INSTANCE.get().expect("logger is not initialized")
    }

    fn from_cli(args: env::Args) -> Result<Logger, io::Error> {
       // ...
#      Ok(Logger {})
    }
}

fn main() {
    let logger = Logger::from_cli(env::args()).unwrap();
    INSTANCE.set(logger).unwrap();
    // use `Logger::global()` from now on
}
```

## Lazy initialized global data

This is essentially `lazy_static!` macro, but without a macro.

```
use std::{sync::Mutex, collections::HashMap};
use once_cell::sync::OnceCell;

fn global_data() -> &'static Mutex<HashMap<i32, String>> {
    static INSTANCE: OnceCell<Mutex<HashMap<i32, String>>> = OnceCell::INIT;
    INSTANCE.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(13, "Spica".to_string());
        m.insert(74, "Hoyten".to_string());
        Mutex::new(m)
    })
}
```

There are also `sync::Lazy` and `unsync::Lazy` convenience types and macros
to streamline this pattern:

```
#[macro_use]
extern crate once_cell;

use std::{sync::Mutex, collections::HashMap};
use once_cell::sync::Lazy;

static GLOBAL_DATA: Lazy<Mutex<HashMap<i32, String>>> = sync_lazy! {
    let mut m = HashMap::new();
    m.insert(13, "Spica".to_string());
    m.insert(74, "Hoyten".to_string());
    Mutex::new(m)
};

fn main() {
    println!("{:?}", GLOBAL_DATA.lock().unwrap());
}
```

## General purpose lazy evaluation

Unlike `lazy_static!`, `Lazy` works with local variables.

```
use once_cell::unsync::Lazy;

fn main() {
    let ctx = vec![1, 2, 3];
    let thunk = Lazy::new(|| {
        ctx.iter().sum::<i32>()
    });
    assert_eq!(*thunk, 6);
}
```

If you need a lazy field in a struct, you probably should use `OnceCell`
directly, because that will allow you to access `self` during initialization.

```
use std::{fs, io::{self, Read}, path::PathBuf};
use once_cell::unsync::OnceCell;

struct Ctx {
    config_path: PathBuf,
    config: OnceCell<String>,
}

impl Ctx {
    pub fn get_config(&self) -> Result<&str, io::Error> {
        let cfg = self.config.get_or_try_init(|| -> Result<String, io::Error> {
            let mut buf = String::new();
            fs::File::open(&self.config_path)?
                .read_to_string(&mut buf)?;
            Ok(buf)
        })?;
        Ok(cfg.as_str())
    }
}
```

# Comparison with std

|`!Sync` types         | Access Mode            | Drawbacks                                     |
|----------------------|------------------------|-----------------------------------------------|
|`Cell<T>`             | `T`                    | works only with `Copy` types                  |
|`RefCel<T>`           | `RefMut<T>` / `Ref<T>` | may panic at runtime                          |
|`unsync::OnceCell<T>` | `&T`                   | assignable only once                          |

|`Sync` types          | Access Mode            | Drawbacks                                     |
|----------------------|------------------------|-----------------------------------------------|
|`AtomicT`             | `T`                    | works only with certain `Copy` types          |
|`Mutex<T>`            | `MutexGuard<T>`        | may deadlock at runtime, may block the thread |
|`sync::OnceCell<T>`   | `&T`                   | assignable only once, may block the thread    |

Technically, calling `get_or_init` will also cause a panic or a deadlock if it recursively calls
itself. However, because the assignment can happen only once, such cases should be more rare than
equivalents with `RefCell` and `Mutex`.

# Implementation details

Implementation is based on [`lazy_static`](https://github.com/rust-lang-nursery/lazy-static.rs/) and
[`lazy_cell`](https://github.com/indiv0/lazycell/) crates and in some sense just streamlines and
unifies the APIs of those crates.

To implement a sync flavor of `OnceCell`, this crates uses either `::std::sync::Once` or
`::parking_lot::Once`. This is controlled by the `parking_lot` feature, which is enabled by default.

When using `parking_lot`, the crate is compatible with rustc 1.25.0, without `parking_lot` a minimum
of `1.29.0` is required.

This crate uses unsafe.
*/

#[cfg(feature = "parking_lot")]
#[path="imp_pl.rs"]
mod imp;
#[cfg(not(feature = "parking_lot"))]
#[path="imp_std.rs"]
mod imp;

#[macro_use]
pub mod unsync {
    use std::ops::Deref;
    use std::cell::UnsafeCell;

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
    #[derive(Debug)]
    pub struct OnceCell<T> {
        // Invariant: written to at most once.
        inner: UnsafeCell<Option<T>>,
    }

    impl<T> Default for OnceCell<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T: Clone> Clone for OnceCell<T> {
        fn clone(&self) -> OnceCell<T> {
            let res = OnceCell::new();
            if let Some(value) = self.get() {
                match res.set(value.clone()) {
                    Ok(()) => (),
                    Err(_) => unreachable!(),
                }
            }
            res
        }
    }

    impl<T: PartialEq> PartialEq for OnceCell<T> {
        fn eq(&self, other: &Self) -> bool {
            self.get() == other.get()
        }
    }

    impl<T> From<T> for OnceCell<T> {
        fn from(value: T) -> Self {
            OnceCell { inner: UnsafeCell::new(Some(value)) }
        }
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
        /// if the cell was empty.
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
        /// the cell was empty. If the cell was empty and `f` failed, an
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
    /// println!("ready");
    /// println!("{}", *lazy);
    /// println!("{}", *lazy);
    ///
    /// // Prints:
    /// //   ready
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
    // Can't use `OnceCell(imp::OnceCell) due to
    // https://github.com/rust-lang/rust/issues/50518
    pub use imp::OnceCell;

    /// A value which is initialized on the first access.
    ///
    /// # Example
    /// ```
    /// #[macro_use]
    /// extern crate once_cell;
    ///
    /// use std::collections::HashMap;
    /// use once_cell::sync::Lazy;
    ///
    /// static HASHMAP: Lazy<HashMap<i32, String>> = sync_lazy! {
    ///     println!("initializing");
    ///     let mut m = HashMap::new();
    ///     m.insert(13, "Spica".to_string());
    ///     m.insert(74, "Hoyten".to_string());
    ///     m
    /// };
    ///
    /// fn main() {
    ///     println!("ready");
    ///     ::std::thread::spawn(|| {
    ///         println!("{:?}", HASHMAP.get(&13));
    ///     }).join().unwrap();
    ///     println!("{:?}", HASHMAP.get(&74));
    ///
    ///     // Prints:
    ///     //   ready
    ///     //   initializing
    ///     //   Some("Spica")
    ///     //   Some("Hoyten")
    /// }
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
        pub fn new(f: F) -> Lazy<T, F> {
            Lazy {
                __cell: OnceCell::new(),
                __init: f,
            }
        }

        /// Forces the evaluation of this lazy value and
        /// returns a reference to result. This is equivalent
        /// to the `Deref` impl, but is explicit.
        ///
        /// # Example
        /// ```
        /// use once_cell::sync::Lazy;
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

    impl<T, F: Fn() -> T> ::std::ops::Deref for Lazy<T, F> {
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
    /// let hello = "Hello, World!".to_string();
    ///
    /// let lazy = sync_lazy! {
    ///     hello.to_uppercase()
    /// };
    ///
    /// assert_eq!(&*lazy, "HELLO, WORLD!");
    /// # }
    /// ```
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
