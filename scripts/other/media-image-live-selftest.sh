#!/usr/bin/env bash
# Media image, LIVE end-to-end (2026-07-17) — the real `lince` cell server,
# the real board page, the real kanban+Record group, no stubs:
#   upload a real PNG through `POST /host/media` (curl, standing in for the
#   editor.js file-picker upload — headless chromium cannot drive a native
#   OS file chooser, so `body-editor-selftest.sh` covers the picker/placeholder
#   half and this covers the render half) -> seed the kanban group -> "New
#   task" -> fill a body with `![](<the uploaded /host/media path>)` -> Create
#   for real -> the SAME shared editor.js renderer runs in both the kanban
#   card and Record -> both actually load the image (`naturalWidth > 0`,
#   not just an <img> tag existing — that was the older, weaker check).
#
# Requires: chromium + jq on PATH, target/debug/lince (cargo build -p lince).
# Usage: scripts/other/media-image-live-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }
command -v jq >/dev/null || { echo "jq not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$ROOT/target/debug/lince"
[ -x "$BIN" ] || { echo "target/debug/lince missing — run: cargo build -p lince"; exit 2; }

PORT=$((39700 + RANDOM % 400))
BASE="http://127.0.0.1:$PORT"
WORK="$(mktemp -d)"
HARNESS="$ROOT/crates/web/static/media-image-harness.html"
cleanup() {
  [ -n "${CHROME_PID:-}" ] && kill "$CHROME_PID" 2>/dev/null || true
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null || true
  rm -f "$HARNESS"
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

# a real 1x1 transparent PNG (well-known minimal fixture)
base64 -d > "$WORK/pixel.png" <<'B64'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YA
AAAASUVORK5CYII=
B64
IMG_PATH="$(curl -sf -F "file=@$WORK/pixel.png;type=image/png" "$BASE/host/media" | jq -r '.path')"
case "$IMG_PATH" in
  /host/media/*.png) : ;;
  *) echo "FAIL: setup upload did not return a /host/media path"; exit 1 ;;
esac

# Seed the board with the kanban GROUP (same fixture as kanban-live-k0-selftest.sh).
# A kanban card ships with an EMPTY widgetState (no default Protein,
# 2026-07-18 — the user must configure one via the Data panel), so this
# patches one in, the same way a real Data panel edit would.
curl -sf "$BASE/host/packages/local/group/kanban.lince" > "$WORK/group.json"
curl -sf "$BASE/host/board/state" > "$WORK/state.json"
jq -s '
  {source: "record", where: [{kind_eq: "plain"}], order: [{asc: "quantity"}, {asc: "created_at"}],
   include: {links: {kinds: ["assigned-to", "part-of"], direction: "out"}}} as $protein
  | (.[0].cards | map(
    if .id == "card-kanban" then . + {x: 4400, y: 4400, width: 760, height: 560, widgetState: {protein: $protein}}
    elif .id == "card-kanban-record" then . + {x: 5180, y: 4400, width: 380, height: 560}
    else . end)) as $group
  | .[1]
  | .workspaces[0].cards += $group
  | .workspaces[0].camera = {x: -4300, y: -4300, scale: 1.0}
' "$WORK/group.json" "$WORK/state.json" > "$WORK/next-state.json"
PUT_CODE="$(curl -s -o /dev/null -w "%{http_code}" -X PUT \
  -H "content-type: application/json" --data-binary @"$WORK/next-state.json" \
  "$BASE/host/board/state")"
[ "$PUT_CODE" = "200" ] || { echo "FAIL: board state PUT returned $PUT_CODE"; exit 1; }

cat > "$HARNESS" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<script type="module">
  const results = {};
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  async function poll(fn, ms = 10000, step = 150) {
    const until = Date.now() + ms;
    for (;;) {
      try { const v = fn(); if (v) return v; } catch {}
      if (Date.now() > until) return null;
      await wait(step);
    }
  }
  const mark = () => { console.log("MRESULT=" + JSON.stringify(results)); };
  try {
    const imgPath = new URLSearchParams(location.search).get("img");
    const frame = document.createElement("iframe");
    frame.src = "/";
    frame.style.cssText = "width:1600px;height:1000px;border:0";
    document.body.appendChild(frame);
    const sandDoc = (id) => {
      const doc = frame.contentDocument;
      if (!doc) return null;
      const el = doc.querySelector(`iframe[data-package-instance-id="${id}"]`);
      const inner = el && el.contentDocument;
      return inner && inner.readyState !== "loading" ? inner : null;
    };
    const kb = await poll(() => { const d = sandDoc("card-kanban"); return d && d.querySelector(".col") ? d : null; });
    const ri = await poll(() => { const d = sandDoc("card-kanban-record"); return d && d.getElementById("create") ? d : null; });
    if (!kb || !ri) throw new Error("sands did not render");

    kb.getElementById("open-create").click();
    await poll(() => ri.getElementById("create").classList.contains("open"), 6000);
    ri.getElementById("c-head").value = "Photo card";
    ri.getElementById("c-body").value = `![](${imgPath})`;
    ri.getElementById("c-submit").click();
    await poll(() => [...ri.querySelectorAll(".rec .name")].some((n) => n.textContent === "Photo card"));

    const kbImg = await poll(() => kb.querySelector(".card-body img"));
    results.kanban_img_present = !!kbImg;
    results.kanban_img_loaded = !!(await poll(() => kbImg && kbImg.complete && kbImg.naturalWidth > 0));
    const riImg = await poll(() => ri.querySelector("img"));
    results.recinfo_img_present = !!riImg;
    results.recinfo_img_loaded = !!(await poll(() => riImg && riImg.complete && riImg.naturalWidth > 0));
  } catch (err) {
    results.error = String(err && err.message ? err.message : err);
  }
  mark();
  console.log("MDONE");
</script>
</body></html>
HTML

ENCODED_IMG="$(printf '%s' "$IMG_PATH" | sed 's#/#%2F#g')"
CHROME_LOG="$WORK/chrome.log"
timeout -k 5 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --enable-logging=stderr --v=0 "$BASE/static/media-image-harness.html?img=$ENCODED_IMG" \
  > "$CHROME_LOG" 2>&1 < /dev/null &
CHROME_PID=$!
for _ in $(seq 1 55); do
  grep -aq "MDONE" "$CHROME_LOG" 2>/dev/null && break
  kill -0 "$CHROME_PID" 2>/dev/null || break
  sleep 1
done
kill "$CHROME_PID" 2>/dev/null || true
wait "$CHROME_PID" 2>/dev/null || true

JSON="$(sed -n 's/.*MRESULT=\(.*\)", source:.*/\1/p' "$CHROME_LOG" | tail -1)"
echo "result: $JSON"
[ -n "$JSON" ] || { echo "FAIL: harness produced no result"; cat "$CHROME_LOG"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check kanban_img_present  "the kanban card did not render an <img> for the uploaded media path"
check kanban_img_loaded   "the kanban card's image did not actually load (naturalWidth was 0)"
check recinfo_img_present "Record did not render an <img> for the uploaded media path"
check recinfo_img_loaded  "Record's image did not actually load (naturalWidth was 0)"

[ "$fail" -eq 0 ] && echo "PASS: uploaded /host/media image actually renders in both kanban and Record (shared editor.js renderer)" || exit 1
