#!/usr/bin/env bash
# K0 — the LIVE end-to-end: the real `lince` cell server, the real board page
# ("/" with main.js + unified bridge + one real WebSocket), the real emitted
# kanban group (board + record_info from `kanban.lince`), driven in headless
# chromium on the REAL clock (no virtual time, no stubs anywhere):
#   boot -> seed the board state with the kanban group (the same cards the
#   catalog add uses; seeded via curl+jq) -> both sands come up LIVE (green dot
#   = the transport socket actually connected) -> "New task" opens
#   record_info's creation mode (group-scoped recordCreate) -> fill head ->
#   Create -> record_info focuses the REAL created record AND the kanban board
#   grows the new card via the live Protein update. This is the seam every
#   other selftest stubs.
#
# Requires: chromium + jq on PATH, target/debug/lince (cargo build -p lince).
# Usage: scripts/other/kanban-live-k0-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }
command -v jq >/dev/null || { echo "jq not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$ROOT/target/debug/lince"
[ -x "$BIN" ] || { echo "target/debug/lince missing — run: cargo build -p lince"; exit 2; }

PORT=$((39200 + RANDOM % 400))
BASE="http://127.0.0.1:$PORT"
WORK="$(mktemp -d)"
HARNESS="$ROOT/crates/web/static/k0-harness.html"
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

# Seed the board with the kanban GROUP — the exact member cards the catalog
# add-as-group flow drops (same endpoint), positioned side by side in view.
curl -sf "$BASE/host/packages/local/group/kanban.lince" > "$WORK/group.json"
curl -sf "$BASE/host/board/state" > "$WORK/state.json"
jq -s '
  (.[0].cards | map(
    if .id == "card-kanban" then . + {x: 4400, y: 4400, width: 760, height: 560}
    elif .id == "card-kanban-record-info" then . + {x: 5180, y: 4400, width: 380, height: 560}
    else . end)) as $group
  | .[1]
  | .workspaces[0].cards += $group
  | .workspaces[0].camera = {x: -4300, y: -4300, scale: 1.0}
' "$WORK/group.json" "$WORK/state.json" > "$WORK/next-state.json"
PUT_CODE="$(curl -s -o /dev/null -w "%{http_code}" -X PUT \
  -H "content-type: application/json" --data-binary @"$WORK/next-state.json" \
  "$BASE/host/board/state")"
[ "$PUT_CODE" = "200" ] || { echo "FAIL: board state PUT returned $PUT_CODE"; exit 1; }

# The harness is served same-origin from /static so it can script the real
# board page and reach into the sand iframes. UI driving only — no fetches.
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
  // Progressive: each mark is a console line chromium mirrors to stderr in
  // real time (--enable-logging=stderr), so a mid-flight kill still shows how
  // far we got. (--dump-dom is useless here: this chromium dumps at the load
  // event and never sees async work.)
  const mark = () => { console.log("K0RESULT=" + JSON.stringify(results)); };

  try {
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
    const kb = await poll(() => {
      const d = sandDoc("card-kanban");
      return d && d.querySelector(".col") ? d : null;
    });
    results.kanban_rendered = !!kb; mark();
    const ri = await poll(() => {
      const d = sandDoc("card-kanban-record-info");
      return d && d.getElementById("create") ? d : null;
    });
    results.recinfo_loaded = !!ri; mark();
    if (!kb || !ri) throw new Error("sands did not render");

    // The transport is REALLY connected: the sand's live dot goes green.
    results.kanban_live = !!(await poll(
      () => kb.getElementById("dot").classList.contains("live")));
    mark();

    // "New task" -> record_info creation mode (same fields, writable, empty).
    kb.getElementById("open-create").click();
    results.create_mode = !!(await poll(
      () => ri.getElementById("create").classList.contains("open"), 6000));
    results.create_fields = !!ri.getElementById("c-head")
      && !!ri.getElementById("c-body") && !!ri.getElementById("c-quantity")
      && ri.getElementById("c-head").value === "";
    mark();

    // Create for real: the server appends the record; record_info focuses it
    // and the kanban board grows the card through the live subscription.
    ri.getElementById("c-head").value = "Fresh";
    ri.getElementById("c-submit").click();
    results.created_focused = !!(await poll(() =>
      [...ri.querySelectorAll(".rec .name")].some((n) => n.textContent === "Fresh")));
    mark();
    results.kanban_live_update = !!(await poll(() =>
      [...kb.querySelectorAll(".card-title")].some((n) => n.textContent === "Fresh")));
  } catch (err) {
    results.error = String(err && err.message ? err.message : err);
  }
  mark();
  console.log("K0DONE");
</script>
</body></html>
HTML

CHROME_LOG="$WORK/chrome.log"
timeout -k 5 120 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --enable-logging=stderr --v=0 "$BASE/static/k0-harness.html" \
  > "$CHROME_LOG" 2>&1 < /dev/null &
CHROME_PID=$!
for _ in $(seq 1 110); do
  grep -aq "K0DONE" "$CHROME_LOG" 2>/dev/null && break
  kill -0 "$CHROME_PID" 2>/dev/null || break
  sleep 1
done
kill "$CHROME_PID" 2>/dev/null || true
wait "$CHROME_PID" 2>/dev/null || true

JSON="$(sed -n 's/.*K0RESULT=\(.*\)", source:.*/\1/p' "$CHROME_LOG" | tail -1)"
echo "result: $JSON"
[ -n "$JSON" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check kanban_rendered    "the real board did not render the kanban sand"
check recinfo_loaded     "the real board did not render the record_info sand"
check kanban_live        "the kanban dot never went live (transport socket not connected)"
check create_mode        "New task did not open record_info's creation mode"
check create_fields      "creation mode is missing head/body/quantity fields"
check created_focused    "record_info did not focus the REAL created record"
check kanban_live_update "the kanban board did not grow the created card via live update"

[ "$fail" -eq 0 ] && echo "PASS: live K0 — real server + real board + kanban group: create flow works end-to-end" || exit 1
