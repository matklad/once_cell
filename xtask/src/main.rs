#[cfg(test)]
mod tidy;

use std::time::Instant;

use xshell::{cmd, Shell};

fn main() -> xshell::Result<()> {
    let sh = Shell::new()?;

    let _e = push_toolchain(&sh, "stable")?;
    let _e = sh.push_env("CARGO", "");

    {
        let _s = section("BUILD");
        cmd!(sh, "cargo test --workspace --no-run").run()?;
    }

    {
        let _s = section("TEST");

        cmd!(sh, "cargo test --workspace").run()?;

        for &release in &[None, Some("--release")] {
            cmd!(sh, "cargo test --features unstable {release...}").run()?;
            cmd!(
                sh,
                "cargo test --no-default-features --features unstable,std,parking_lot {release...}"
            )
            .run()?;
        }

        // Skip doctests for no_std tests as those don't work
        cmd!(sh, "cargo test --no-default-features --features unstable --test it").run()?;
        cmd!(sh, "cargo test --no-default-features --features unstable,alloc --test it").run()?;

        cmd!(sh, "cargo test --no-default-features --features critical-section").run()?;
        cmd!(sh, "cargo test --features critical-section").run()?;
    }

    {
        let _s = section("TEST_BETA");
        let _e = push_toolchain(&sh, "beta")?;
        // TEMPORARY WORKAROUND for Rust compiler issue ref:
        // - https://github.com/rust-lang/rust/issues/129352
        // - https://github.com/matklad/once_cell/issues/261
        let _e = sh.push_env("RUSTFLAGS", "-A unreachable_patterns");

        cmd!(sh, "cargo test --features unstable").run()?;
    }

    {
        let _s = section("TEST_MSRV");
        let msrv = {
            let manifest = sh.read_file("Cargo.toml")?;
            let (_, suffix) = manifest.split_once("rust-version = \"").unwrap();
            let (version, _) = suffix.split_once("\"").unwrap();
            version.to_string()
        };

        let _e = push_toolchain(&sh, &msrv)?;
        sh.copy_file("Cargo.lock.msrv", "Cargo.lock")?;
        if let err @ Err(_) = cmd!(sh, "cargo update -w -v --locked").run() {
            // `Cargo.lock.msrv` is out of date! Probably from having bumped our own version number.
            println! {"\
                Error: `Cargo.lock.msrv` is out of date. \
                Please run:\n    \
                (cp Cargo.lock{{.msrv,}} && cargo +{msrv} update -w -v && cp Cargo.lock{{.msrv,}})\n\
                \n\
                Alternatively, `git apply` the `.patch` below:\
            "}
            cmd!(sh, "cargo update -q -w").quiet().run()?;
            sh.copy_file("Cargo.lock", "Cargo.lock.msrv")?;
            cmd!(sh, "git --no-pager diff --color=always -- Cargo.lock.msrv").quiet().run()?;
            return err;
        }
        cmd!(sh, "cargo build --locked").run()?;
    }

    {
        let _s = section("TEST_MIRI");
        let miri_nightly= cmd!(sh, "curl -s https://rust-lang.github.io/rustup-components-history/x86_64-unknown-linux-gnu/miri").read()?;
        let _e = push_toolchain(&sh, &format!("nightly-{}", miri_nightly))?;

        sh.remove_path("./target")?;

        cmd!(sh, "rustup component add miri").run()?;
        cmd!(sh, "cargo miri setup").run()?;
        cmd!(sh, "cargo miri test --features unstable").run()?;
    }

    {
        let _s = section("PUBLISH");

        let version = cmd!(sh, "cargo pkgid").read()?.rsplit_once('#').unwrap().1.to_string();
        let tag = format!("v{version}");

        let current_branch = cmd!(sh, "git branch --show-current").read()?;
        let has_tag = cmd!(sh, "git tag --list").read()?.lines().any(|it| it.trim() == tag);
        let dry_run = sh.var("CI").is_err() || has_tag || current_branch != "master";
        eprintln!("Publishing{}!", if dry_run { " (dry run)" } else { "" });

        let dry_run_arg = if dry_run { Some("--dry-run") } else { None };
        cmd!(sh, "cargo publish {dry_run_arg...}").run()?;
        if dry_run {
            eprintln!("{}", cmd!(sh, "git tag {tag}"));
            eprintln!("{}", cmd!(sh, "git push --tags"));
        } else {
            cmd!(sh, "git tag {tag}").run()?;
            cmd!(sh, "git push --tags").run()?;
        }
    }
    Ok(())
}

fn push_toolchain<'a>(
    sh: &'a xshell::Shell,
    toolchain: &str,
) -> xshell::Result<xshell::PushEnv<'a>> {
    cmd!(sh, "rustup toolchain install {toolchain} --no-self-update").run()?;
    let res = sh.push_env("RUSTUP_TOOLCHAIN", toolchain);
    cmd!(sh, "rustc --version").run()?;
    Ok(res)
}

fn section(name: &'static str) -> impl Drop {
    println!("::group::{name}");
    let start = Instant::now();
    defer(move || {
        let elapsed = start.elapsed();
        eprintln!("{name}: {elapsed:.2?}");
        println!("::endgroup::");
    })
}

fn defer<F: FnOnce()>(f: F) -> impl Drop {
    struct D<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for D<F> {
        fn drop(&mut self) {
            if let Some(f) = self.0.take() {
                f()
            }
        }
    }
    D(Some(f))
}
