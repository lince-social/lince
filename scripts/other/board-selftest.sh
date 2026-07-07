#!/usr/bin/env bash
# Behavioral verification of the membrane board chrome (Stage 8b §4).
#
# Boots the membrane host, loads the board in headless chromium with the
# ?selftest hook (which synthesizes real pointer interactions — drag-move,
# z-order, marquee group, pin, workspace switch — and writes PASS/FAIL to
# <body data-selftest>), and asserts every interaction passed.
#
# This drives the interaction WIRING the reused modules are glued into — the
# part that is membrane-specific and not covered by web's board_js_tests.rs.
# Requires: chromium on PATH. Usage: scripts/other/board-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

PORT="${PORT:-4699}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

PROFILE="$(mktemp -d)"  # shared chromium profile so localStorage survives runs
LINCE_SELFTEST=1 LINCE_DB="sqlite::memory:" LINCE_ADDR="127.0.0.1:$PORT" \
  cargo run -q -p membrane --manifest-path "$ROOT/Cargo.toml" >/tmp/board-selftest-server.log 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null || true; rm -rf "$PROFILE"' EXIT

# wait for the server to accept connections
for _ in $(seq 1 40); do
  curl -sf -o /dev/null "http://127.0.0.1:$PORT/" && break || sleep 0.5
done

fail=0

# 1. Chrome interactions (synchronous: drag/resize/zoom/z-order/marquee-group/
#    pin/workspace + persistence write-path). --dump-dom + virtual time is fine —
#    no real network wait. Uses the shared profile so localStorage persists.
DOM="$(timeout 30 "$CHROMIUM" --headless --no-sandbox --disable-gpu \
  --user-data-dir="$PROFILE" --window-size=1400,900 --virtual-time-budget=8000 \
  --dump-dom "http://127.0.0.1:$PORT/?selftest" 2>/dev/null)"
CHROME="$(printf '%s' "$DOM" | grep -oE 'data-selftest="[^"]*"' || true)"
echo "chrome:  $CHROME"
{ [ -n "$CHROME" ] && ! printf '%s' "$CHROME" | grep -q "FAIL" && printf '%s' "$CHROME" | grep -q "PASS"; } || fail=1

# 1b. Persistence RESTORE path: a second load with the SAME profile — the store
#     must hydrate the workspace added (and persisted) in step 1 from localStorage.
PDOM="$(timeout 30 "$CHROMIUM" --headless --no-sandbox --disable-gpu \
  --user-data-dir="$PROFILE" --window-size=1400,900 --virtual-time-budget=8000 \
  --dump-dom "http://127.0.0.1:$PORT/?selftest=persist-read" 2>/dev/null)"
PERSIST="$(printf '%s' "$PDOM" | grep -oE 'data-selftest-persist="[^"]*"' || true)"
echo "persist: $PERSIST"
printf '%s' "$PERSIST" | grep -q "restore:PASS" || fail=1

# 2. Bridge data plane (read + write + live-update-after-write through the real
#    transport). This needs REAL time, not a virtual clock — the sand's Protein
#    round-trips over the WebSocket. The page POSTs its result to /selftest-result
#    so we read it from the server log instead of --dump-dom's virtual clock.
timeout 15 "$CHROMIUM" --headless --no-sandbox --disable-gpu --window-size=1400,900 \
  "http://127.0.0.1:$PORT/?selftest=bridge" >/dev/null 2>&1 || true
BRIDGE="$(grep -oE '\[selftest-result\].*' /tmp/board-selftest-server.log | tail -1 | sed 's/%3A/:/g; s/%20/ /g; s/%3D/=/g; s/-%3E/->/g')"
echo "bridge:  $BRIDGE"
{ printf '%s' "$BRIDGE" | grep -q "render:PASS" && printf '%s' "$BRIDGE" | grep -q "live:PASS"; } || fail=1

if [ "$fail" -ne 0 ]; then
  echo "BOARD SELF-TEST FAILED"
  exit 1
fi
echo "BOARD SELF-TEST PASSED"
