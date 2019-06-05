use once_cell::sync::OnceCell;

const N_THREADS: usize = 32;
const N_ROUNDS: usize = 100_000_000;

static CELL: OnceCell<usize> = OnceCell::new();

fn main() {
    let start = std::time::Instant::now();
    let threads = (0..N_THREADS)
        .map(|i| std::thread::spawn(move || thread_main(i)))
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    println!("{:?}", start.elapsed());
}

fn thread_main(i: usize) {
    for _ in 0..N_ROUNDS {
        let &value = CELL.get_or_init(|| i);
        assert!(value < N_THREADS)
    }
}
