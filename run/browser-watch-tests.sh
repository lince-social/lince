#!/usr/bin/env bash

set -euo pipefail

jobs=10

while [[ $# -gt 0 ]]; do
  case "$1" in
    -j|--jobs)
      jobs="${2:-}"
      shift 2
      ;;
    -h|--help)
      cat <<'EOF'
Usage: run/browser-watch-tests.sh [--jobs N]

Runs all browser regression tests in headed watch mode.
EOF
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

cargo test -p lince-web --features browser-watch browser_ -- --nocapture --test-threads="$jobs"
