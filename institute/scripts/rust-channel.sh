#!/usr/bin/env bash
set -euo pipefail
channel=$(awk -F '"' '/^[[:space:]]*channel[[:space:]]*=/ { print $2; exit }' rust-toolchain.toml)
if [[ -z "$channel" ]]; then
  printf '%s\n' 'Missing Rust toolchain channel' >&2
  exit 1
fi
if [[ "$channel" == stable ]]; then
  channel=1.96.0
fi
printf 'version=%s\n' "$channel"
