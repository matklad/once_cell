#[macro_use]
extern crate once_cell;

use std::{
    mem,
    thread,
    ptr,
    cell::Cell,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};
use once_cell::{sync, unsync};

fn go<F: FnOnce() -> ()>(mut f: F) {
    struct Yolo<T>(T);
    unsafe impl<T> Send for Yolo<T> {}

    let ptr: *const u8 = &mut f as *const F as *const u8;
    mem::forget(f);
    let yolo = Yolo(ptr);
    thread::spawn(move || {
        let f: F = unsafe { ptr::read(yolo.0 as *const F) };
        f();
    }).join().unwrap();
}

#[test]
fn unsync_once_cell() {
    let c = unsync::OnceCell::new();
    assert!(c.get().is_none());
    c.get_or_init(|| 92);
    assert_eq!(c.get(), Some(&92));

    c.get_or_init(|| {
        panic!("Kabom!")
    });
    assert_eq!(c.get(), Some(&92));
}

#[test]
fn sync_once_cell() {
    let c = sync::OnceCell::new();
    assert!(c.get().is_none());
    go(|| {
        c.get_or_init(|| 92);
        assert_eq!(c.get(), Some(&92));
    });
    c.get_or_init(|| panic!("Kabom!"));
    assert_eq!(c.get(), Some(&92));
}

#[test]
fn unsync_once_cell_drop() {
    static DROP_CNT: AtomicUsize = AtomicUsize::new(0);
    struct Dropper;
    impl Drop for Dropper {
        fn drop(&mut self) {
            DROP_CNT.fetch_add(1, SeqCst);
        }
    }

    let x = unsync::OnceCell::new();
    x.get_or_init(|| Dropper);
    assert_eq!(DROP_CNT.load(SeqCst), 0);
    drop(x);
    assert_eq!(DROP_CNT.load(SeqCst), 1);
}

#[test]
fn sync_once_cell_drop() {
    static DROP_CNT: AtomicUsize = AtomicUsize::new(0);
    struct Dropper;
    impl Drop for Dropper {
        fn drop(&mut self) {
            DROP_CNT.fetch_add(1, SeqCst);
        }
    }

    let x = sync::OnceCell::new();
    go(|| {
        x.get_or_init(|| Dropper);
        assert_eq!(DROP_CNT.load(SeqCst), 0);
        drop(x);
    });
    assert_eq!(DROP_CNT.load(SeqCst), 1);
}

#[test]
fn unsync_lazy_macro() {
    let called = Cell::new(0);
    let x = unsync_lazy! {
        called.set(called.get() + 1);
        92
    };

    assert_eq!(called.get(), 0);

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.get(), 1);

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.get(), 1);
}

#[test]
fn sync_lazy_macro() {
    let called = AtomicUsize::new(0);
    let x = sync_lazy! {
        called.fetch_add(1, SeqCst);
        92
    };

    assert_eq!(called.load(SeqCst), 0);

    go(|| {
        let y = *x - 30;
        assert_eq!(y, 62);
        assert_eq!(called.load(SeqCst), 1);
    });

    let y = *x - 30;
    assert_eq!(y, 62);
    assert_eq!(called.load(SeqCst), 1);
}


#[test]
fn static_lazy() {
    static XS: sync::Lazy<Vec<i32>> = sync_lazy! {
        let mut xs = Vec::new();
        xs.push(1);
        xs.push(2);
        xs.push(3);
        xs
    };
    assert_eq!(&*XS, &vec![1, 2, 3]);
}

#[test]
fn static_lazy_no_macros() {
    fn xs() -> &'static Vec<i32> {
        static XS: sync::OnceCell<Vec<i32>> = sync::OnceCell::INIT;
        XS.get_or_init(|| {
            let mut xs = Vec::new();
            xs.push(1);
            xs.push(2);
            xs.push(3);
            xs
        })
    }
    assert_eq!(xs(), &vec![1, 2, 3]);
}

#[test]
fn sync_once_cell_is_sync_send() {
    fn assert_traits<T: Send + Sync>() {}
    assert_traits::<sync::OnceCell<String>>();
}
