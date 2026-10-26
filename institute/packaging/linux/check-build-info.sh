#!/usr/bin/env bash
set -euo pipefail
[[ $# -ge 1 && $# -le 2 ]] || { echo 'Usage: check-build-info.sh ACTUAL.json [EXPECTED.json]' >&2; exit 2; }
jq -e -s '
  length == 1 and (.[0] |
    type == "object" and
    all(.version, .revision, .target; type == "string" and length > 0) and
    (.target | endswith("-linux-gnu")) and
    .optimized == true and
    .features == {ui: true, instinct: true, park: true})
' "$1" >/dev/null
if [[ $# == 2 ]]; then
    jq -e -s 'length == 2 and .[0] == .[1]' "$1" "$2" >/dev/null
fi
