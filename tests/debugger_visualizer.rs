use std::num::NonZeroUsize;
use std::sync::Mutex;

use debugger_test::debugger_test;
use once_cell::{sync, unsync};
use once_cell::race::*;

static VEC: sync::Lazy<Vec<String>> = sync::Lazy::new(|| {
    let mut m = Vec::with_capacity(2);
    m.push("Hoyten".to_string());
    m.push("Spica".to_string());
    m
});

fn global_data() -> &'static Mutex<Vec<String>> {
    static INSTANCE: sync::OnceCell<Mutex<Vec<String>>> = sync::OnceCell::new();
    INSTANCE.get_or_init(|| {
        let mut m = Vec::with_capacity(2);
        m.push("Hoyten".to_string());
        m.push("Spica".to_string());
        Mutex::new(m)
    })
}

#[inline(never)]
fn __break() {
    println!("Breakpoint hit");
}

#[debugger_test(
    debugger = "cdb",
    commands = r#"
.nvlist
dx once_bool
dx once_non_zero_usize

g

dx once_bool
dx once_box
dx once_non_zero_usize

dx debugger_visualizer::global_data::INSTANCE
dx debugger_visualizer::global_data::INSTANCE.@"[value]".__0.data

dx debugger_visualizer::VEC

g

dx debugger_visualizer::VEC
dx debugger_visualizer::VEC.@"[value]".__0

dx lazy
dx cell
"#,
    expected_statements = r#"
once_bool        : false [Type: once_cell::race::OnceBool]
    [<Raw View>]     [Type: once_cell::race::OnceBool]

once_non_zero_usize : 0x0 [Type: once_cell::race::OnceNonZeroUsize]
    [<Raw View>]     [Type: once_cell::race::OnceNonZeroUsize]

once_bool        : true [Type: once_cell::race::OnceBool]
    [<Raw View>]     [Type: once_cell::race::OnceBool]

once_box         : "Hello World" [Type: once_cell::race::once_box::OnceBox<alloc::string::String>]
    [<Raw View>]     [Type: once_cell::race::once_box::OnceBox<alloc::string::String>]
    [len]            : 0xb [Type: unsigned __int64]
    [capacity]       : 0xb [Type: unsigned __int64]
    [chars]          : "Hello World"

once_non_zero_usize : 0x48 [Type: once_cell::race::OnceNonZeroUsize]
    [<Raw View>]     [Type: once_cell::race::OnceNonZeroUsize]

debugger_visualizer::global_data::INSTANCE : Some [Type: once_cell::sync::OnceCell<std::sync::mutex::Mutex<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> > >]
    [<Raw View>]     [Type: once_cell::sync::OnceCell<std::sync::mutex::Mutex<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> > >]
    [queue]          [Type: core::sync::atomic::AtomicPtr<once_cell::imp::Waiter>]
    [marker]         [Type: core::marker::PhantomData<ptr_mut$<once_cell::imp::Waiter> >]
    [value]          : Some [Type: core::cell::UnsafeCell<enum2$<core::option::Option<std::sync::mutex::Mutex<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> > > > >]

debugger_visualizer::global_data::INSTANCE.@"[value]".__0.data : { len=0x2 } [Type: core::cell::UnsafeCell<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> >]
    [<Raw View>]     [Type: core::cell::UnsafeCell<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> >]
    [len]            : 0x2 [Type: unsigned __int64]
    [capacity]       : 0x2 [Type: unsigned __int64]
    [0]              : "Hoyten" [Type: alloc::string::String]
    [1]              : "Spica" [Type: alloc::string::String]

debugger_visualizer::VEC : None [Type: once_cell::sync::Lazy<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global>,alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> (*)()>]
    [<Raw View>]     [Type: once_cell::sync::Lazy<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global>,alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> (*)()>]
    [init]           : Some [Type: core::cell::Cell<enum2$<core::option::Option<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> (*)()> > >]
    [queue]          [Type: core::sync::atomic::AtomicPtr<once_cell::imp::Waiter>]
    [marker]         [Type: core::marker::PhantomData<ptr_mut$<once_cell::imp::Waiter> >]
    [value]          : None [Type: core::cell::UnsafeCell<enum2$<core::option::Option<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> > > >]

debugger_visualizer::VEC : Some [Type: once_cell::sync::Lazy<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global>,alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> (*)()>]
    [<Raw View>]     [Type: once_cell::sync::Lazy<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global>,alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> (*)()>]
    [init]           : None [Type: core::cell::Cell<enum2$<core::option::Option<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> (*)()> > >]
    [queue]          [Type: core::sync::atomic::AtomicPtr<once_cell::imp::Waiter>]
    [marker]         [Type: core::marker::PhantomData<ptr_mut$<once_cell::imp::Waiter> >]
    [value]          : Some [Type: core::cell::UnsafeCell<enum2$<core::option::Option<alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> > > >]

debugger_visualizer::VEC.@"[value]".__0 : { len=0x2 } [Type: alloc::vec::Vec<alloc::string::String,alloc::alloc::Global>]
    [<Raw View>]     [Type: alloc::vec::Vec<alloc::string::String,alloc::alloc::Global>]
    [len]            : 0x2 [Type: unsigned __int64]
    [capacity]       : 0x2 [Type: unsigned __int64]
    [0]              : "Hoyten" [Type: alloc::string::String]
    [1]              : "Spica" [Type: alloc::string::String]

lazy             : Some [Type: once_cell::unsync::Lazy<i32,i32 (*)()>]
    [<Raw View>]     [Type: once_cell::unsync::Lazy<i32,i32 (*)()>]
    [init]           : None [Type: core::cell::Cell<enum2$<core::option::Option<i32 (*)()> > >]
    [+0x004] __0              : 92 [Type: int]

cell             : Some [Type: once_cell::unsync::OnceCell<alloc::string::String>]
    [<Raw View>]     [Type: once_cell::unsync::OnceCell<alloc::string::String>]
    [+0x000] __0              : "Hello, World!" [Type: alloc::string::String]
"#
)]
#[inline(never)]
fn test_debugger_visualizer() {
    let once_bool = OnceBool::new();
    let once_box: OnceBox<String> = OnceBox::new();
    let once_non_zero_usize = OnceNonZeroUsize::new();
    __break();

    let _ = once_bool.get_or_init(|| {
        true
    });

    let _ = once_box.get_or_init(|| {
        Box::new("Hello World".to_string())
    });

    let _ = once_non_zero_usize.get_or_init(|| {
        NonZeroUsize::new(72).unwrap()
    });

    let instance = global_data();
    let map = instance.lock().unwrap();
    assert_eq!("Hoyten".to_string(), map[0]);
    __break();
    
    assert_eq!("Spica".to_string(), VEC[1]);

    let lazy: unsync::Lazy<i32> = unsync::Lazy::new(|| {
        92
    });
    assert_eq!(92, *lazy);

    let cell = unsync::OnceCell::new();
    assert!(cell.get().is_none());
    
    let value: &String = cell.get_or_init(|| {
        "Hello, World!".to_string()
    });
    assert_eq!(value, "Hello, World!");
    assert!(cell.get().is_some());
    __break();
}