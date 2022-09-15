use xshell::{cmd, Shell};

#[test]
fn test_formatting() {
    let sh = Shell::new().unwrap();
    cmd!(sh, "cargo fmt --all -- --check").run().unwrap()
}
