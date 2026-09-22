#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

artifacts=${1:-release-artifacts}
archives=("$artifacts"/lince-*.tar.gz "$artifacts"/lince-*.zip)
if [ "${#archives[@]}" -eq 0 ]; then
  echo "No binary archives found in $artifacts" >&2
  exit 1
fi

assets=()
for archive in "${archives[@]}"; do
  assets+=("$archive")
  for sidecar in "$archive.sha256" "$archive.target"; do
    if [ -f "$sidecar" ]; then
      assets+=("$sidecar")
    fi
  done
done

if ! gh release view main-binary --repo "$GH_REPO" >/dev/null 2>&1; then
  gh release create main-binary \
    --repo "$GH_REPO" \
    --target "$GITHUB_SHA" \
    --title "Lince main binaries" \
    --prerelease \
    --notes "Native binaries and a prebuilt Nix flake from the latest successful main build."
fi
gh release upload main-binary "${assets[@]}" --clobber --repo "$GH_REPO"
