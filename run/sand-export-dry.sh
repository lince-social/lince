#!/usr/bin/env bash

set -euo pipefail

cargo run -p xtask -- sand export --dry-run
