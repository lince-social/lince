#!/usr/bin/env bash

set -euo pipefail

if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then
    echo "main only"
    exit 1
fi

gh auth status >/dev/null

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
cargo set-version "${version#v}"
git add .
git commit -m "chore(release): ${version}"
git tag -a "$version" -m "$version"
git push origin main "$version"
gh run list --workflow=build-release --limit 1 --json url -q '.[0].url'
