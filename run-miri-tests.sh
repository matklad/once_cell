#!/usr/bin/env bash
set -e

MIRI_NIGHTLY=nightly-$(curl -s https://rust-lang.github.io/rustup-components-history/x86_64-unknown-linux-gnu/miri)
echo "Installing latest nightly with Miri: $MIRI_NIGHTLY"
rustup toolchain add "$MIRI_NIGHTLY"

rustup component add miri --toolchain "$MIRI_NIGHTLY"
rustup run "$MIRI_NIGHTLY" -- cargo miri setup

# Miri considers runtime-allocated data in statics as leaked, so we
# have to ignore leeks. See <https://github.com/rust-lang/miri/issues/940>.
rustup run "$MIRI_NIGHTLY" -- cargo miri test -- -Zmiri-ignore-leaks
