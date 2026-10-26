#!/usr/bin/env bash
set -euo pipefail
checker=$(dirname "${BASH_SOURCE[0]}")/check-build-info.sh
temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT
printf '%s\n' '{"version":"0.8.0","revision":"fixture","target":"x86_64-unknown-linux-gnu","optimized":true,"features":{"ui":true,"instinct":true,"park":true}}' > "$temporary/valid.json"
bash "$checker" "$temporary/valid.json" "$temporary/valid.json"
for mutation in '.optimized=false' '.features.ui=false' '.target="aarch64-apple-darwin"' '.version=null' '.features.extra=true'; do
    jq "$mutation" "$temporary/valid.json" > "$temporary/invalid.json"
    if bash "$checker" "$temporary/invalid.json"; then
        echo "Invalid build information was accepted: $mutation" >&2
        exit 1
    fi
done
printf '%s\n' '{} {}' > "$temporary/invalid.json"
if bash "$checker" "$temporary/invalid.json"; then exit 1; fi
jq '.revision="different"' "$temporary/valid.json" > "$temporary/different.json"
if bash "$checker" "$temporary/valid.json" "$temporary/different.json"; then exit 1; fi
printf '%s\n' 'Build information checks passed.'
