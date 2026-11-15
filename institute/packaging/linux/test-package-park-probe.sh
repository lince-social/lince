#!/usr/bin/env bash
set -euo pipefail
packager=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/package-park-probe.sh
temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT
export LINCE_REVISION=fixture BUILD_TARGET=x86_64-unknown-linux-gnu
printf '%s\n' 'fixture executable bytes' > "$temporary/probe"
printf '%s\n' '{"version":"0.8.0","revision":"fixture","target":"x86_64-unknown-linux-gnu","optimized":true,"features":{"ui":true,"instinct":true,"park":true}}' > "$temporary/application.json"
printf '%s\n' '{"probe":"park-responsiveness-v1","package":"lince","version":"0.8.0","revision":"fixture","optimized":true,"architecture":"x86_64","os":"linux"}' > "$temporary/probe.json"
package() {
    bash "$packager" "$temporary/probe" "$1" "$2" "$3"
}
package "$temporary/probe.json" "$temporary/application.json" "$temporary/artifacts/probe.tar.gz"
(cd "$temporary/artifacts" && sha256sum --check probe.tar.gz.sha256)
mkdir "$temporary/extracted"
tar -xzf "$temporary/artifacts/probe.tar.gz" -C "$temporary/extracted"
(cd "$temporary/extracted" && sha256sum --check SHA256SUMS)
test -x "$temporary/extracted/park-responsiveness"
cmp "$temporary/probe" "$temporary/extracted/park-responsiveness"
cmp "$temporary/application.json" "$temporary/extracted/application-build-info.json"
cmp "$temporary/probe.json" "$temporary/extracted/probe-build-info.json"
for mutation in '.revision="other"' '.version="other"' '.optimized=false' '.package="lince-park"' '.probe="unknown"' '.architecture="aarch64"' '.os="macos"'; do
    jq "$mutation" "$temporary/probe.json" > "$temporary/invalid.json"
    if package "$temporary/invalid.json" "$temporary/application.json" "$temporary/rejected.tar.gz"; then
        echo "Invalid probe was accepted: $mutation" >&2; exit 1
    fi
done
for mutation in '.revision="other"' '.version="other"' '.optimized=false' '.features.park=false' '.target="aarch64-unknown-linux-gnu"'; do
    jq "$mutation" "$temporary/application.json" > "$temporary/invalid.json"
    if package "$temporary/probe.json" "$temporary/invalid.json" "$temporary/rejected.tar.gz"; then
        echo "Mismatched application was accepted: $mutation" >&2; exit 1
    fi
done
cat "$temporary/probe.json" "$temporary/probe.json" > "$temporary/invalid.json"
if package "$temporary/invalid.json" "$temporary/application.json" "$temporary/rejected.tar.gz"; then exit 1; fi
if LINCE_REVISION=other package "$temporary/probe.json" "$temporary/application.json" "$temporary/rejected.tar.gz"; then exit 1; fi
if BUILD_TARGET=aarch64-unknown-linux-gnu package "$temporary/probe.json" "$temporary/application.json" "$temporary/rejected.tar.gz"; then exit 1; fi
test ! -e "$temporary/rejected.tar.gz"
printf '%s\n' 'Park probe packaging and identity checks passed.'
