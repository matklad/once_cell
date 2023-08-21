<p align="center"><img src="design/logo.png" alt="once_cell"></p>


[![Build Status](https://github.com/matklad/once_cell/actions/workflows/ci.yaml/badge.svg)](https://github.com/matklad/once_cell/actions)
[![Crates.io](https://img.shields.io/crates/v/once_cell.svg)](https://crates.io/crates/once_cell)
[![API reference](https://docs.rs/once_cell/badge.svg)](https://docs.rs/once_cell/)

# Overview

`once_cell` provides two new cell-like types, `unsync::OnceCell` and `sync::OnceCell`. `OnceCell`
might store arbitrary non-`Copy` types, can be assigned to at most once and provide direct access
to the stored contents. In a nutshell, API looks *roughly* like this:

```rust
impl OnceCell<T> {
    fn new() -> OnceCell<T> { ... }
    fn set(&self, value: T) -> Result<(), T> { ... }
    fn get(&self) -> Option<&T> { ... }
}
```

Note that, like with `RefCell` and `Mutex`, the `set` method requires only a shared reference.
Because of the single assignment restriction `get` can return an `&T` instead of `Ref<T>`
or `MutexGuard<T>`.

`once_cell` also has a `Lazy<T>` type, build on top of `OnceCell` which provides the same API as
the `lazy_static!` macro, but without using any macros:

```rust
use std::{sync::Mutex, collections::HashMap};
use once_cell::sync::Lazy;

static GLOBAL_DATA: Lazy<Mutex<HashMap<i32, String>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(13, "Spica".to_string());
    m.insert(74, "Hoyten".to_string());
    Mutex::new(m)
});

fn main() {
    println!("{:?}", GLOBAL_DATA.lock().unwrap());
}
```

More patterns and use-cases are in the [docs](https://docs.rs/once_cell/)!

# Related crates

* [double-checked-cell](https://github.com/niklasf/double-checked-cell)
* [lazy-init](https://crates.io/crates/lazy-init)
* [lazycell](https://crates.io/crates/lazycell)
* [mitochondria](https://crates.io/crates/mitochondria)
* [lazy_static](https://crates.io/crates/lazy_static)
* [async_once_cell](https://crates.io/crates/async_once_cell)
* [generic_once_cell](https://crates.io/crates/generic_once_cell) (bring your own mutex)

Parts of `once_cell` API are included into `std` [as of Rust 1.70.0](https://github.com/rust-lang/rust/pull/105587).

# Minimum Supported Rust Version (MSRV)

We support workflows which target up-to-date, supported versions of Rust compiler, but we also give somewhat generous grace period, as keeping perfectly up-to-date is hard.

We explicitly do not support workflows which depend on using the old compiler. Making sure that the latest once_cell can be compiled with rustc packaged with debian stable is a non-goal.

If users of once_cell find themselves with an outdated compiler, the following actions are suggested:

 1. Find a way to upgrade compiler
 2. If using old compiler is required, stick to old version of once_cell as well
 3. If a combination of old compiler and new once_cell is required, it's on the consumer to maintain a fork with backports.

See https://github.com/matklad/once_cell/issues/201 for talk about MSRV.
