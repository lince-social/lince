#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${REPO_DIR:-/root/git/lince-social/lince}"
BRANCH="${BRANCH:-dev}"

"$REPO_DIR/scripts/sync-branch.sh"
"$REPO_DIR/scripts/http-release.sh"
