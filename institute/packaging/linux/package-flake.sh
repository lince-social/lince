#!/usr/bin/env bash
set -euo pipefail
artifacts=${1:-release-artifacts}
asset=$artifacts/lince-x86_64-unknown-linux-gnu.tar.gz
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"$/\1/p' Cargo.toml)
[[ -n $version && -n ${GITHUB_SHA:-} && -n ${GITHUB_REPOSITORY:-} ]]
temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT
root=$temporary/lince
mkdir -p "$root/institute/server" "$root/institute/anicca"
tar -xOf "$asset" lince/build-info.json > "$temporary/build-info.json"
bash "$(dirname "${BASH_SOURCE[0]}")/check-build-info.sh" "$temporary/build-info.json"
jq -e --arg version "$version" --arg revision "$GITHUB_SHA" '.version == $version and .revision == $revision' "$temporary/build-info.json" >/dev/null
digest=$(sha256sum "$asset" | cut -d ' ' -f1)
checksum=$(printf '%b' "$(printf '%s' "$digest" | sed 's/../\\x&/g')" | base64 -w0)
jq --arg url "https://github.com/$GITHUB_REPOSITORY/releases/download/main-binary/$(basename "$asset")" --arg hash "sha256-$checksum" '{features,target,version,revision} + {url:$url,hash:$hash}' "$temporary/build-info.json" > "$root/binary.json"
cp flake.nix flake.lock Cargo.toml "$root/"
cp institute/server/configuration.nix "$root/institute/server/"
cp institute/anicca/Lince.lingua "$root/institute/anicca/"
cp -R institute/packaging "$root/institute/"
tar -czf "$artifacts/lince-flake.tar.gz" -C "$temporary" lince
printf '%s  lince-flake.tar.gz\n' "$(sha256sum "$artifacts/lince-flake.tar.gz" | cut -d ' ' -f1)" > "$artifacts/lince-flake.tar.gz.sha256"
