#![feature(test)]
use std::mem::size_of;

extern crate test;

use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use test::black_box;

const N_THREADS: usize = 4;
const N_ROUNDS: usize = 1_000_000;

static CELL: OnceCell<usize> = OnceCell::new();
static OTHER: AtomicUsize = AtomicUsize::new(0);

fn main() {
    let start = std::time::Instant::now();
    let threads =
        (0..N_THREADS).map(|i| std::thread::spawn(move || thread_main(i))).collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    println!("{:?}", OTHER.load(Ordering::Acquire));
    println!("{:?}", start.elapsed());
    println!("size_of::<OnceCell<()>>()   = {:?}", size_of::<OnceCell<()>>());
    println!("size_of::<OnceCell<bool>>() = {:?}", size_of::<OnceCell<bool>>());
    println!("size_of::<OnceCell<u32>>()  = {:?}", size_of::<OnceCell<u32>>());
}

#[inline(never)]
fn thread_main(i: usize) {
    let mut data = [0u32; 100];
    let mut accum = 0u32;
    for _ in 0..N_ROUNDS {
        let &value = CELL.get_or_init(|| i);
        for i in data.iter_mut() {
            *i = (*i).wrapping_add(accum);
            accum = accum.wrapping_add(3);
        }
        OTHER.fetch_add(1, Ordering::Relaxed);
        black_box(value);
        black_box(data);
    }
}
