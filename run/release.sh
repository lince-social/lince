#!/usr/bin/env bash

set -euo pipefail

ensure_release_tools() {
    if ! command -v git-cliff >/dev/null 2>&1; then
        echo "git-cliff is required. Run this via 'mise release' or install git-cliff." >&2
        exit 1
    fi

    if ! cargo set-version --help >/dev/null 2>&1; then
        echo "cargo set-version is required. Run this via 'mise release' or install cargo-edit." >&2
        exit 1
    fi
}

sync_workspace_versions() {
    local release_version="$1"

    perl -0pi -e 's/(\[workspace\.package\]\nversion = ")[^"]+(")/$1'"$release_version"'$2/' Cargo.toml
    perl -0pi -e 's/version = "=[^"]*"/version = "='"$release_version"'"/g' Cargo.toml
}

ensure_gh_auth() {
    if gh auth status >/dev/null 2>&1; then
        return 0
    fi

    echo "GitHub CLI is not authenticated. Starting login..."
    gh auth login

    if gh auth status >/dev/null 2>&1; then
        return 0
    fi

    echo "GitHub CLI authentication did not complete." >&2
    exit 1
}

if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then
    echo "main only"
    exit 1
fi

ensure_release_tools
ensure_gh_auth

suggested_ver="$(git cliff --bumped-version 2>/dev/null || echo "0.1.0")"
read -rp "Version (auto: ${suggested_ver}): " user_input
version="${user_input:-$suggested_ver}"

case "$version" in
    v*)
        ;;
    *)
        version="v${version}"
        ;;
esac

git cliff --unreleased --bump --tag "$version" -o CHANGELOG.md
cargo set-version --workspace "${version#v}"
sync_workspace_versions "${version#v}"
git add .
git commit -m "chore(release): ${version}"
git tag -a "$version" -m "$version"
git push origin main "$version"
gh run list --workflow=build-release --limit 1 --json url -q '.[0].url'
echo "If you also want cargo install support from crates.io, run ./run/publish-crates.sh"
