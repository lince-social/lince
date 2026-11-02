#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
channel=$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["toolchain"]["channel"])' "$root/rust-toolchain.toml")
if [[ ! "$channel" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf '%s\n' 'Rust toolchain must pin an exact stable version; run python3 institute/scripts/update-rust-toolchain.py' >&2
  exit 1
fi
printf 'version=%s\n' "$channel"
