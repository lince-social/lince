#!/usr/bin/env bash
# Behavioral verification of the ABI event relay in the board widget bridge
# (Stage 8b). Sand-to-sand ABI events (`emit` / `onEvent`) ride transport
# ephemeral lanes: one room per topic (`abi:<topic>`). The board fans out to
# same-board siblings in-page and mirrors the event onto the lane so OTHER
# sessions/devices receive it.
#
# The transport substrate (cross-session fan-out, never-persist) is already
# covered by `transport/tests/session.rs::ephemeral_lanes_fan_out_and_never_persist`.
# This drives the JS relay in `widget-bridge.js` end-to-end in headless chromium
# against a STUBBED WebSocket + fake frames, and asserts the bridge contract:
#   1. in-page fan-out : emit reaches sibling frames that listen, never the source
#   2. lane mirror      : emit sends `lane_send` on room `abi:<topic>`
#   3. lane receive     : an inbound `lane_event` is delivered to listening frames
#   4. room churn        : rooms are joined for listened topics, left when no card
#                          listens, and re-joined when emitted again (the exact
#                          emit-only left/re-joined path the tracker calls out)
#
# Requires: chromium on PATH. Usage: scripts/other/abi-lane-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The bridge is an ES module that imports the shared transport (Stage 8b, base
# task 1). Bundle transport.js + widget-bridge.js into one classic script by
# stripping the `import` line and the `export ` keywords, so `getSharedTransport`
# and `createWidgetBridge` become globals for the harness to drive. The stubbed
# `window.WebSocket` (set before `createWidgetBridge` runs) is what the lazy
# transport connects with.
sed 's/^export function/function/; /^import /{:a;/;$/!{N;ba};d}' \
  "$ROOT/crates/web/static/presentation/board/transport.js" > "$WORK/bridge.js"
sed 's/^export function/function/; /^import /{:a;/;$/!{N;ba};d}' \
  "$ROOT/crates/web/static/presentation/board/widget-bridge.js" >> "$WORK/bridge.js"
grep -q "function createWidgetBridge" "$WORK/bridge.js" || { echo "could not prepare bridge.js"; exit 1; }
grep -q "function getSharedTransport" "$WORK/bridge.js" || { echo "could not prepare transport.js"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="status"></div>
<script src="bridge.js"></script>
<script>
  // ---- Stubbed transport socket: record every JSON frame the bridge sends. ----
  window.__sent = [];
  class FakeWS {
    constructor(url) {
      this.url = url;
      this.readyState = 0; // CONNECTING
      window.__ws = this;
      this._open = [];
      // Open asynchronously, like a real socket.
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

  // Push an inbound server frame to the bridge (as if from the transport).
  window.__inbound = (obj) => window.__ws._msg({ data: JSON.stringify(obj) });

  // ---- Fake frames: plain objects with the shape the bridge reads. ----
  function makeFrame(id) {
    const posts = [];
    return {
      dataset: { packageInstanceId: id },
      contentWindow: { postMessage: (m) => posts.push(m) },
      __posts: posts,
    };
  }
  const frameA = makeFrame("A"); // emitter
  const frameB = makeFrame("B"); // listener
  const frameC = makeFrame("C"); // silent
  const frames = [frameA, frameB, frameC];

  // Per-instance ABI listen config (mutable — drives the churn phases). The
  // emitter A also LISTENS to recordClicked, so self-echo suppression is a real
  // guard here (source skip), not a side effect of A not listening.
  const listen = { A: ["recordClicked"], B: ["recordClicked"], C: [] };

  window.__bridge = createWidgetBridge({
    statusNode: document.getElementById("status"),
    getFrames: () => frames,
    initialState: {},
    getCardMeta: () => ({}),
    getCardAbiListen: (id) => listen[id] || [],
    setCardState: () => {},
    patchCardState: () => {},
    setCardStreamsEnabled: () => {},
    handleShellAction: () => {},
    invalidateServerAuth: () => {},
    onError: () => {},
  });

  // Message-type constants (bridge.js declares its own globals with these
  // names, so the harness uses distinct identifiers with the same values).
  const T_WIDGET_ACTION = "lince:widget-action";
  const T_WIDGET_EVENT = "lince:bridge-event";

  const emit = (instanceId, topic, data) =>
    window.postMessage(
      { type: T_WIDGET_ACTION, instanceId, payload: { action: "emit-event", topic, data } },
      "*",
    );

  const resetSent = () => { window.__sent.length = 0; };
  const resetPosts = () => frames.forEach((f) => (f.__posts.length = 0));
  const sentHas = (pred) => window.__sent.some(pred);
  const laneSend = (room) =>
    window.__sent.find((m) => m.type === "lane_send" && m.room === room);
  const eventsOn = (frame, topic) =>
    frame.__posts.filter((m) => m.type === T_WIDGET_EVENT && m.payload.topic === topic);
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));

  (async () => {
    const results = {};
    await wait(20); // let construction connect + join B's initial room

    // Phase 1: construction joined the room B listens to.
    results.p1_join_initial = sentHas((m) => m.type === "lane_join" && m.room === "abi:recordClicked");

    // Phase 2: A emits recordClicked -> in-page fan-out + lane mirror.
    resetSent(); resetPosts();
    emit("A", "recordClicked", { uid: "r_1" });
    await wait(20);
    results.p2_fanout_B = eventsOn(frameB, "recordClicked").length === 1;
    results.p2_source_A_suppressed = eventsOn(frameA, "recordClicked").length === 0;
    results.p2_nonlistener_C = eventsOn(frameC, "recordClicked").length === 0;
    const mirror = laneSend("abi:recordClicked");
    results.p2_lane_send = !!mirror &&
      mirror.payload.topic === "recordClicked" &&
      mirror.payload.data.uid === "r_1" &&
      mirror.payload.sourceInstanceId === "A";

    // Phase 3: feed the EXACT recorded lane_send payload back as an inbound
    // lane_event (as another session would) -> B receives it. Reusing the sent
    // payload proves send-shape and receive-shape cannot drift apart. Source is
    // "A" so A is still suppressed; assert on B.
    resetPosts();
    // The real server tags every lane_event with its `room` (transport
    // ws.rs) and suppresses self-echo; mirror this exactly.
    window.__inbound({ type: "lane_event", room: "abi:recordClicked", from: "remote-session", payload: mirror.payload });
    await wait(20);
    results.p3_inbound_B = eventsOn(frameB, "recordClicked").length === 1;
    results.p3_inbound_A_suppressed = eventsOn(frameA, "recordClicked").length === 0;

    // Phase 4: emit-only topic nobody listens to -> joins + mirrors, no local
    // frame receives it.
    resetSent(); resetPosts();
    emit("A", "ghost", { n: 1 });
    await wait(20);
    results.p4_join_ghost = sentHas((m) => m.type === "lane_join" && m.room === "abi:ghost");
    results.p4_send_ghost = !!laneSend("abi:ghost");
    results.p4_no_local_ghost = frames.every((f) => eventsOn(f, "ghost").length === 0);

    // Phase 5: reconfigure B to listen to a different topic, then reconcile.
    // The room B no longer listens to AND the emit-only ghost room are left; the
    // new room is joined.
    resetSent();
    listen.A = [];
    listen.B = ["other"];
    window.__bridge.syncFrames();
    await wait(20);
    results.p5_leave_recordClicked = sentHas((m) => m.type === "lane_leave" && m.room === "abi:recordClicked");
    results.p5_leave_ghost = sentHas((m) => m.type === "lane_leave" && m.room === "abi:ghost");
    results.p5_join_other = sentHas((m) => m.type === "lane_join" && m.room === "abi:other");

    // Phase 6: emit the previously-left ghost topic again -> the room is
    // RE-joined (the emit-only left/re-joined churn cycle the tracker flags).
    resetSent();
    emit("A", "ghost", { n: 2 });
    await wait(20);
    results.p6_rejoin_ghost = sentHas((m) => m.type === "lane_join" && m.room === "abi:ghost");
    results.p6_resend_ghost = !!laneSend("abi:ghost");

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
check() { # check <key> <human description>
  grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }
}

check p1_join_initial          "construction did not join the room B listens to"
check p2_fanout_B              "emit did not fan out in-page to the listening sibling"
check p2_source_A_suppressed   "emit echoed back to the source frame (self-echo not suppressed)"
check p2_nonlistener_C         "emit reached a frame that does not listen to the topic"
check p2_lane_send             "emit did not mirror onto the topic lane with the right payload"
check p3_inbound_B             "inbound lane_event was not delivered to the listening frame"
check p3_inbound_A_suppressed  "inbound lane_event leaked to the source frame"
check p4_join_ghost            "emit-only topic did not join its lane room"
check p4_send_ghost            "emit-only topic did not mirror onto its lane"
check p4_no_local_ghost        "emit-only topic was delivered to a frame that does not listen"
check p5_leave_recordClicked   "reconcile did not leave the room B stopped listening to"
check p5_leave_ghost           "reconcile did not leave the emit-only room"
check p5_join_other            "reconcile did not join the room B now listens to"
check p6_rejoin_ghost          "re-emitting a left topic did not re-join its lane room"
check p6_resend_ghost          "re-emitting a left topic did not mirror onto its lane"

[ "$fail" -eq 0 ] && echo "PASS: ABI relay in-page fan-out + lane mirror + lane receive + room churn (leave/re-join)" || exit 1
