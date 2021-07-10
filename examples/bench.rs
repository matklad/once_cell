use once_cell::sync::OnceCell;

const N_LOOPS: usize = 8;
static CELL: OnceCell<usize> = OnceCell::new();

fn main() {
    let start = std::time::Instant::now();
    for i in 0..N_LOOPS {
        go(i)
    }
    println!("{:0.0?}", start.elapsed());
}

#[inline(never)]
pub fn go(i: usize) {
    for _ in 0..100_000_000 {
        let &value = CELL.get_or_init(|| i);
        assert!(value < N_LOOPS)
    }
}
