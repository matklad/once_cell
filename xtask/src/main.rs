use std::env;

use xaction::{cargo_toml, cmd, cp, git, push_rustup_toolchain, rm_rf, section, Result};

const MSRV: &str = "1.36.0";

fn main() {
    if let Err(err) = try_main() {
        eprintln!("error: {}", err);
        std::process::exit(1)
    }
}

fn try_main() -> Result<()> {
    let subcommand = std::env::args().nth(1);
    match subcommand {
        Some(it) if it == "ci" => (),
        _ => {
            print_usage();
            Err("invalid arguments")?
        }
    }

    let cargo_toml = cargo_toml()?;

    {
        let _s = section("TEST_STABLE");
        let _t = push_rustup_toolchain("stable");
        cmd!("cargo test --features unstable").run()?;
        cmd!("cargo test --features unstable --release").run()?;

        // Skip doctests, they need `std`
        cmd!("cargo test --features unstable --no-default-features --test it").run()?;

        cmd!("cargo test --features 'unstable std parking_lot' --no-default-features").run()?;
        cmd!("cargo test --features 'unstable std parking_lot' --no-default-features --release")
            .run()?;

        cmd!("cargo test --features 'unstable alloc' --no-default-features --test it").run()?;
        cmd!("cargo test --features 'unstable std parking_lot alloc' --no-default-features")
            .run()?;
    }

    {
        let _s = section("TEST_BETA");
        let _t = push_rustup_toolchain("beta");
        cmd!("cargo test --features unstable").run()?;
        cmd!("cargo test --features unstable --release").run()?;
    }

    {
        let _s = section("TEST_MSRV");
        let _t = push_rustup_toolchain(MSRV);
        cp("Cargo.lock.msrv", "Cargo.lock")?;
        cmd!("cargo build").run()?;
    }

    {
        let _s = section("TEST_MIRI");
        rm_rf("./target")?;

        let miri_nightly= cmd!("curl -s https://rust-lang.github.io/rustup-components-history/x86_64-unknown-linux-gnu/miri").read()?;
        let _t = push_rustup_toolchain(&format!("nightly-{}", miri_nightly));

        cmd!("rustup component add miri").run()?;
        cmd!("cargo miri setup").run()?;
        cmd!("cargo miri test --features unstable").run()?;
    }

    let version = cargo_toml.version()?;
    let tag = format!("v{}", version);

    let dry_run =
        env::var("CI").is_err() || git::has_tag(&tag)? || git::current_branch()? != "master";
    xaction::set_dry_run(dry_run);

    {
        let _s = section("PUBLISH");
        cargo_toml.publish()?;
        git::tag(&tag)?;
        git::push_tags()?;
    }
    Ok(())
}

fn print_usage() {
    eprintln!(
        "\
Usage: cargo run -p xtask <SUBCOMMAND>

SUBCOMMANDS:
    ci
"
    )
}
