#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${REPO_DIR:-/root/git/lince-social/lince}"
BRANCH="${BRANCH:-dev}"
FLAKE_HOST="${FLAKE_HOST:-manas-organ}"

"$REPO_DIR/scripts/sync-branch.sh"

cd "$REPO_DIR"
sudo nixos-rebuild switch --flake ".#${FLAKE_HOST}"
