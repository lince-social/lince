#!/usr/bin/env bash
set -euo pipefail
[[ $# == 4 ]] || { echo 'Usage: package-park-probe.sh PROBE PROBE_INFO.json APPLICATION_INFO.json DESTINATION.tar.gz' >&2; exit 2; }
: "${LINCE_REVISION:?Set the expected source revision}"
: "${BUILD_TARGET:?Set the expected target triple}"
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
bash "$repository/institute/packaging/linux/check-build-info.sh" "$3"
jq -e -s --arg revision "$LINCE_REVISION" --arg target "$BUILD_TARGET" --slurpfile application "$3" '
  length == 1 and (.[0] |
    .probe == "park-responsiveness-v1" and .package == "lince" and
    .revision == $revision and .revision == $application[0].revision and
    .version == $application[0].version and .optimized == true and
    .architecture == "x86_64" and .os == "linux" and
    $target == "x86_64-unknown-linux-gnu" and $application[0].target == $target)
' "$2" > /dev/null
destination=$(realpath -m "$4")
mkdir -p "$(dirname "$destination")"
temporary=$(mktemp -d "$(dirname "$destination")/park-probe-XXXXXX")
trap 'rm -rf -- "$temporary"' EXIT
install -m 755 "$1" "$temporary/park-responsiveness"
install -m 644 "$2" "$temporary/probe-build-info.json"
install -m 644 "$3" "$temporary/application-build-info.json"
(cd "$temporary" && sha256sum park-responsiveness probe-build-info.json application-build-info.json > SHA256SUMS)
tar -I 'gzip -1' -cf "$temporary/archive.tar.gz" -C "$temporary" park-responsiveness probe-build-info.json application-build-info.json SHA256SUMS
mv "$temporary/archive.tar.gz" "$destination"
(cd "$(dirname "$destination")" && sha256sum "$(basename "$destination")" > "$(basename "$destination").sha256")
