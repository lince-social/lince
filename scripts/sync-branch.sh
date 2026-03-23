#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${REPO_DIR:-/root/git/lince-social/lince}"
BRANCH="${BRANCH:-dev}"

if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi

cd "$REPO_DIR"

git fetch origin "$BRANCH"
git checkout "$BRANCH"
git reset --hard "origin/$BRANCH"
