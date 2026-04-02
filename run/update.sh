#!/usr/bin/env bash

set -euo pipefail

RUST_CHANNEL="${RUST_CHANNEL:-stable}"

rustup self update
rustup update "$RUST_CHANNEL"
cargo upgrade
