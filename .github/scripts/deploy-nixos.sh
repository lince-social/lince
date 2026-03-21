#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${REPO_DIR:-/root/git/lince-social/lince}"
BRANCH="${BRANCH:-dev}"
FLAKE_HOST="${FLAKE_HOST:-manas-organ}"

cd "$REPO_DIR"

git fetch origin "$BRANCH"
git checkout "$BRANCH"
git reset --hard "origin/$BRANCH"

sudo nixos-rebuild switch --flake ".#${FLAKE_HOST}"
