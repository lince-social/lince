#!/usr/bin/env bash
# Media upload/serve (2026-07-17) — the real `lince` cell server, no stubs:
# proves the ONLY path a `![](...)` body image can point at on disk is one
# this server itself wrote. `POST /host/media` sniffs bytes (magic numbers,
# not the client's claimed filename/extension) against a raster allowlist,
# writes under `<data-dir>/web/media/` with an OPAQUE generated name, and
# returns `/host/media/<name>`; `GET` that path serves the bytes back with
# the right Content-Type + `X-Content-Type-Options: nosniff`. A non-image
# upload (even one claiming a .png name) is rejected; a GET for any filename
# that isn't the exact `<uuid>.<ext>` shape (traversal, wrong extension, made
# up name) is rejected before the filesystem is ever touched.
#
# Requires: curl + jq on PATH, target/debug/lince (cargo build -p lince).
# Usage: scripts/other/media-upload-selftest.sh
set -euo pipefail

command -v jq >/dev/null || { echo "jq not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$ROOT/target/debug/lince"
[ -x "$BIN" ] || { echo "target/debug/lince missing — run: cargo build -p lince"; exit 2; }

PORT=$((39600 + RANDOM % 400))
BASE="http://127.0.0.1:$PORT"
WORK="$(mktemp -d)"
cleanup() {
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

"$BIN" --data-dir "$WORK/data" --listen-addr "127.0.0.1:$PORT" --quiet \
  > "$WORK/server.log" 2>&1 &
SERVER_PID=$!

up=""
for _ in $(seq 1 60); do
  if curl -sf -o /dev/null "$BASE/host/board/state"; then up=1; break; fi
  sleep 0.25
done
[ -n "$up" ] || { echo "server did not come up"; cat "$WORK/server.log"; exit 1; }

fail=0
check() { [ "$1" = "$2" ] || { echo "FAIL: $3 (got '$1', want '$2')"; fail=1; }; }
check_true() { [ "$1" = "true" ] || { echo "FAIL: $2"; fail=1; }; }

# a real 1x1 transparent PNG (well-known minimal fixture)
base64 -d > "$WORK/pixel.png" <<'B64'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YA
AAAASUVORK5CYII=
B64
PNG_BYTES=$(wc -c < "$WORK/pixel.png" | tr -d ' ')

# 1. Upload a real PNG -> 200, JSON {path: "/host/media/<uuid>.png"}
curl -sf -o "$WORK/upload.json" -w '%{http_code}' \
  -F "file=@$WORK/pixel.png;type=image/png" "$BASE/host/media" > "$WORK/upload.status"
check "$(cat "$WORK/upload.status")" "200" "PNG upload did not return 200"
PATH_OUT="$(jq -r '.path' "$WORK/upload.json")"
case "$PATH_OUT" in
  /host/media/????????-????-????-????-????????????.png) : ;;
  *) echo "FAIL: uploaded path is not the expected uuid.png shape: $PATH_OUT"; fail=1 ;;
esac
NAME="${PATH_OUT#/host/media/}"

# 2. The file actually landed under <data-dir>/web/media/ (not somewhere else)
[ -f "$WORK/data/web/media/$NAME" ] && ON_DISK=true || ON_DISK=false
check_true "$ON_DISK" "uploaded file did not land under <data-dir>/web/media/"

# 3. GET it back: 200, right Content-Type, nosniff header, same byte count
STATUS=$(curl -s -o "$WORK/fetched.png" -w '%{http_code}' \
  -D "$WORK/fetched.headers" "$BASE$PATH_OUT")
check "$STATUS" "200" "GET of the uploaded image did not return 200"
grep -qi '^content-type: image/png' "$WORK/fetched.headers" \
  && CT_OK=true || CT_OK=false
check_true "$CT_OK" "GET did not serve Content-Type: image/png"
grep -qi '^x-content-type-options: nosniff' "$WORK/fetched.headers" \
  && NOSNIFF_OK=true || NOSNIFF_OK=false
check_true "$NOSNIFF_OK" "GET did not send X-Content-Type-Options: nosniff"
FETCHED_BYTES=$(wc -c < "$WORK/fetched.png" | tr -d ' ')
check "$FETCHED_BYTES" "$PNG_BYTES" "fetched image byte count did not match the upload"

# 4. A non-image upload is rejected regardless of claimed filename/type
STATUS=$(curl -s -o /dev/null -w '%{http_code}' \
  -F 'file=@/dev/null;filename=evil.png;type=image/png' "$BASE/host/media")
[ "$STATUS" = "415" ] || [ "$STATUS" = "400" ] || {
  echo "FAIL: non-image upload was not rejected (got $STATUS)"; fail=1
}
echo "not an image, just text claiming to be one" > "$WORK/fake.png"
STATUS=$(curl -s -o /dev/null -w '%{http_code}' \
  -F "file=@$WORK/fake.png;type=image/png" "$BASE/host/media")
[ "$STATUS" = "415" ] || { echo "FAIL: text-as-png upload was not rejected by sniffing (got $STATUS)"; fail=1; }

# 5. GET rejects anything that isn't the exact <uuid>.<ext> shape — traversal,
#    wrong extension, made-up names — before touching the filesystem
for BAD in "../../../etc/passwd" "evil.svg" "evil.html" "not-a-uuid.png" "$NAME/../x"; do
  STATUS=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/host/media/$BAD")
  [ "$STATUS" = "400" ] || [ "$STATUS" = "404" ] || {
    echo "FAIL: GET /host/media/$BAD was not rejected (got $STATUS)"; fail=1
  }
done

[ "$fail" -eq 0 ] && echo "PASS: media upload/serve (sniffed allowlist, opaque names, traversal rejected, in-dir file served with nosniff)" || exit 1
