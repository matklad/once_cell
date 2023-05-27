//! A `Cell<Option<F>>`, but covariant, at the cost of a very reduced API.
//!
//! (the idea being: it starts as `Some(F)`, and the only `&`-based operation
//! is `take()`ing it. This guarantees covariance to be fine, since the `F`
//! value is never overwritten).

use ::core::{cell::Cell, mem::ManuallyDrop};

pub struct CovariantCellOption<F> {
    /// Invariant: if this is `true` then `value` must contain a non-dropped `F`;
    is_some: Cell<bool>,
    value: ManuallyDrop<F>,
}

impl<F> CovariantCellOption<F> {
    pub const fn some(value: F) -> Self {
        Self { is_some: Cell::new(true), value: ManuallyDrop::new(value) }
    }

    pub fn into_inner(self) -> Option<F> {
        // Small optimization: disable drop glue so as not to have to overwrite `is_some`.
        let mut this = ManuallyDrop::new(self);
        let is_some = this.is_some.get_mut();
        is_some.then(|| unsafe {
            // SAFETY: as per the invariant, we can use `value`. We can also *consume* it by doing:
            // *is_some = false;
            // but we actually don't even need to do it since we don't use `this` anymore.
            ManuallyDrop::take(&mut this.value)
        })
    }

    pub fn take(&self) -> Option<F> {
        self.is_some.get().then(|| unsafe {
            // SAFETY: as per the invariant, we can use `value`.
            // Clearing the `is_some` flag also lets us *consume* it.
            self.is_some.set(false);
            // `ManuallyDrop::take_by_ref`, morally.
            <*const F>::read(&*self.value)
        })
    }
}

impl<F> Drop for CovariantCellOption<F> {
    fn drop(&mut self) {
        if *self.is_some.get_mut() {
            unsafe {
                // SAFETY: as per the invariant, we can use `value`.
                ManuallyDrop::drop(&mut self.value)
            }
        }
    }
}

#[cfg(test)]
fn _assert_covariance<'short>(
    long: *const (CovariantCellOption<&'static ()>,),
) -> *const (CovariantCellOption<&'short ()>,) {
    long
}
