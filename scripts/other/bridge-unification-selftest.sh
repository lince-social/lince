#!/usr/bin/env bash
# Behavioral verification that the ONE unified widget bridge (Stage 8b, base
# task 1) serves the NEW-WAY (flat) sand protocol over the single shared
# transport, including group-scoped in-page ABI fan-out — the exact substrate
# kanban Track B's scoped `recordClicked` -> its packaged record_info needs.
#
# Drives `transport.js` + `widget-bridge.js` end-to-end in headless chromium
# against a STUBBED WebSocket + fake sand frames, asserting the flat contract:
#   1. one socket      : both the bridge and a second consumer share ONE socket
#   2. flat subscribe  : `lince:protein-subscribe` -> `subscribe`; snapshot rows
#                        come back FLAT (`{type:lince:protein-rows, subId, rows}`)
#   3. flat action     : `lince:action` -> `act`; `action_ok` -> FLAT
#                        `{type:lince:action-result, reqId, ok}`
#   4. scoped ABI      : an emit reaches ONLY the same-group sibling in-page,
#                        never a different-group sand; ungrouped source broadcasts
#   5. lane mirror     : the emit is also mirrored to the server lane
#
# Requires: chromium on PATH. Usage: scripts/other/bridge-unification-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Bundle transport + bridge into one classic script (strip import/export). The
# stubbed WebSocket is installed before the bridge lazily connects.
sed 's/^export function/function/; /^import /d' \
  "$ROOT/crates/web/static/presentation/board/transport.js" > "$WORK/bundle.js"
sed 's/^export function/function/; /^import /d' \
  "$ROOT/crates/web/static/presentation/board/widget-bridge.js" >> "$WORK/bundle.js"
grep -q "function getSharedTransport" "$WORK/bundle.js" || { echo "could not prepare transport.js"; exit 1; }
grep -q "function createWidgetBridge" "$WORK/bundle.js" || { echo "could not prepare widget-bridge.js"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="status"></div>
<script src="bundle.js"></script>
<script>
  // ---- Stubbed shared transport socket: record every sent frame. ----
  window.__sent = [];
  window.__wsCount = 0;
  class FakeWS {
    constructor(url) {
      this.url = url;
      this.readyState = 0;
      window.__ws = this;
      window.__wsCount += 1; // count how many sockets the page opens
      this._open = [];
      setTimeout(() => { this.readyState = 1; this._open.forEach((fn) => fn()); }, 0);
    }
    addEventListener(type, fn) {
      if (type === "open") this._open.push(fn);
      else if (type === "message") this._msg = fn;
      else if (type === "close") this._close = fn;
      else if (type === "error") this._err = fn;
    }
    send(raw) { window.__sent.push(JSON.parse(raw)); }
    close() { this.readyState = 3; }
  }
  FakeWS.CONNECTING = 0; FakeWS.OPEN = 1; FakeWS.CLOSING = 2; FakeWS.CLOSED = 3;
  window.WebSocket = FakeWS;
  window.__inbound = (obj) => window.__ws._msg({ data: JSON.stringify(obj) });

  // ---- Fake new-way sand frames. ----
  function makeFrame(id) {
    const posts = [];
    return {
      dataset: { packageInstanceId: id },
      contentWindow: { postMessage: (m) => posts.push(m) },
      __posts: posts,
    };
  }
  const kanban = makeFrame("kanban");   // emitter, group g1
  const recinfo = makeFrame("recinfo"); // same group g1 -> should receive
  const other = makeFrame("other");     // group g2 -> must NOT receive
  const frames = [kanban, recinfo, other];

  // Group stacks (outermost -> innermost). kanban+recinfo share g1; other is g2.
  const groups = { kanban: ["g1"], recinfo: ["g1"], other: ["g2"] };

  const bridge = createWidgetBridge({
    statusNode: document.getElementById("status"),
    getFrames: () => frames,
    initialState: {},
    getCardMeta: () => ({}),
    getCardAbiListen: () => [],
    getCardGroupStack: (id) => groups[id] || [],
    setCardState: () => {},
    patchCardState: () => {},
    setCardStreamsEnabled: () => {},
    handleShellAction: () => {},
    invalidateServerAuth: () => {},
    onError: () => {},
  });

  // A second consumer sharing the same transport (as protein-config.js does):
  // proves ONE socket is used, not one-per-consumer.
  const shared = getSharedTransport();

  // Post a flat frame message up to the bridge, exactly as frame.js does.
  const post = (instanceId, msg) => window.postMessage({ instanceId, ...msg }, "*");
  const postsOf = (frame, type) => frame.__posts.filter((m) => m.type === type);
  const sentHas = (pred) => window.__sent.some(pred);
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));

  (async () => {
    const results = {};
    await wait(20);

    // 1. One socket for the bridge + the extra shared consumer.
    results.one_socket = window.__wsCount === 1;

    // 2. Flat subscribe -> `subscribe`; snapshot rows come back FLAT.
    post("kanban", { type: "lince:ready" });
    post("kanban", { type: "lince:protein-subscribe", subId: "main", protein: { source: "record" } });
    await wait(10);
    results.flat_subscribe_sent = sentHas((m) => m.type === "subscribe" && m.id === "kanban:main");
    window.__inbound({ type: "snapshot", id: "kanban:main", rows: [{ uid: "r1" }, { uid: "r2" }] });
    await wait(10);
    const rows = postsOf(kanban, "lince:protein-rows");
    results.flat_rows_delivered =
      rows.length === 1 && rows[0].subId === "main" && Array.isArray(rows[0].rows) &&
      rows[0].rows.length === 2 && rows[0].payload === undefined; // FLAT, not nested

    // 3. Flat action -> `act`; action_ok comes back FLAT.
    window.__sent.length = 0;
    post("kanban", { type: "lince:action", reqId: "k1", action: { kind: "create-record" } });
    await wait(10);
    const act = window.__sent.find((m) => m.type === "act");
    results.flat_action_sent = !!act && act.action.kind === "create-record";
    window.__inbound({ type: "action_ok", id: act.id, created: "r9", facts: 1 });
    await wait(10);
    const ar = postsOf(kanban, "lince:action-result");
    results.flat_action_result =
      ar.length === 1 && ar[0].reqId === "k1" && ar[0].ok === true && ar[0].payload === undefined;

    // 4. Scoped ABI: recinfo + other join the same room; kanban emits.
    post("recinfo", { type: "lince:lane-join", room: "recordClicked" });
    post("other", { type: "lince:lane-join", room: "recordClicked" });
    await wait(10);
    window.__sent.length = 0;
    recinfo.__posts.length = 0; other.__posts.length = 0; kanban.__posts.length = 0;
    post("kanban", { type: "lince:lane-send", room: "recordClicked", payload: { uid: "r1" } });
    await wait(10);
    // same-group sibling receives it (flat lane-event), different-group does not,
    // source never receives its own emit.
    results.scope_same_group = postsOf(recinfo, "lince:lane-event").length === 1 &&
      postsOf(recinfo, "lince:lane-event")[0].payload.uid === "r1";
    results.scope_diff_group_blocked = postsOf(other, "lince:lane-event").length === 0;
    results.scope_source_suppressed = postsOf(kanban, "lince:lane-event").length === 0;
    // 5. The emit is mirrored onto the server lane for other sessions.
    results.lane_mirror = sentHas((m) => m.type === "lane_send" && m.room === "recordClicked" && m.payload.uid === "r1");

    // 4b. An UNGROUPED source broadcasts to every joiner (back-compat).
    groups.kanban = []; // kanban now ungrouped
    recinfo.__posts.length = 0; other.__posts.length = 0;
    post("kanban", { type: "lince:lane-send", room: "recordClicked", payload: { uid: "r2" } });
    await wait(10);
    results.ungrouped_broadcasts =
      postsOf(recinfo, "lince:lane-event").length === 1 &&
      postsOf(other, "lince:lane-event").length === 1;

    document.title = "RESULT=" + JSON.stringify(results);
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --virtual-time-budget=3000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"

JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness did not produce a result (chromium/module load?)"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }

check one_socket               "the bridge + a second consumer opened more than one socket"
check flat_subscribe_sent      "flat lince:protein-subscribe did not become a transport subscribe"
check flat_rows_delivered      "snapshot rows were not delivered in the FLAT shape to the sand"
check flat_action_sent         "flat lince:action did not become a transport act"
check flat_action_result       "action_ok was not delivered in the FLAT shape to the sand"
check scope_same_group         "scoped emit did not reach the same-group sibling in-page"
check scope_diff_group_blocked "scoped emit leaked to a different-group sand"
check scope_source_suppressed  "scoped emit echoed back to the source sand"
check lane_mirror              "scoped emit was not mirrored onto the server lane"
check ungrouped_broadcasts     "an ungrouped source did not broadcast to every joiner"

[ "$fail" -eq 0 ] && echo "PASS: unified bridge serves the flat protocol + group-scoped ABI over one shared socket" || exit 1
