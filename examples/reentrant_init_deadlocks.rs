fn main() {
    let cell = once_cell::sync::OnceCell::<u32>::new();
    cell.get_or_init(|_| {
        cell.get_or_init(|_| 1);
        2
    });
}
