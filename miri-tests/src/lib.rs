#[test]
fn aliasing_in_get() {
    let x = once_cell::unsync::OnceCell::new();
    x.set(42).unwrap();
    let at_x = x.get().unwrap(); // --- (shared) borrow of inner `Option<T>` --+
    let _ = x.set(27); // <-- temporary (unique) borrow of inner `Option<T>`   |
    println!("{}", at_x); // <------- up until here ---------------------------+
}

