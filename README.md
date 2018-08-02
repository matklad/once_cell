# once_cell

[![Build Status](https://travis-ci.org/matklad/once_cell.svg?branch=master)](https://travis-ci.org/matklad/once_cell)
[![Crates.io](https://img.shields.io/crates/v/once_cell.svg)](https://crates.io/crates/once_cell)
[![API reference](https://docs.rs/once_cell/badge.svg)](https://docs.rs/once_cell/)

A macroless alternative to [`lazy_static`](https://github.com/rust-lang-nursery/lazy-static.rs).
If you like this, you might also like [`lazycell`](https://github.com/indiv0/lazycell/)

```rust
fn hashmap() -> &'static HashMap<u32, &'static str> {
    static INSTANCE: OnceCell<HashMap<u32, &'static str>> = OnceCell::INIT;
    INSTANCE.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(0, "foo");
        m.insert(1, "bar");
        m.insert(2, "baz");
        m
    })
}
```

If you want slightly sweeter syntax, we have macros as well!

```rust
static HASHMAP: Lazy<HashMap<u32, &'static str>> = sync_lazy! {
    let mut m = HashMap::new();
    m.insert(0, "foo");
    m.insert(1, "bar");
    m.insert(2, "baz");
    m
};
```
