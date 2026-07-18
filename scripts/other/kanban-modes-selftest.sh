#!/usr/bin/env bash
# K1 verification — kanban card body view modes (head/compact/full), node-free:
#   - the real served `kanban.html` with the real `frame.js` inlined in an iframe
#   - the real unified `widget-bridge.js` + shared `transport.js`
#   - only the transport WebSocket is stubbed
# Proves the whole K1 loop: default full -> cardState adoption (host pushes
# `lince:bridge-state`, sand re-renders) -> "All head" toolbar persists through
# the NEW flat `lince:patch-card-state` (frame.js -> bridge -> patchCardState
# dep) -> per-card override beats the board default -> per-column override
# applies -> compact truncates long bodies -> mode clicks never emit
# `recordClicked`.
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/kanban-modes-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BOARD="$ROOT/crates/web/static/presentation/board"
SAND="$ROOT/crates/web/src/sand"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

sed 's/^export function/function/; /^import /d' "$BOARD/transport.js"      > "$WORK/bundle.js"
sed 's/^export function/function/; /^import /d' "$BOARD/widget-bridge.js" >> "$WORK/bundle.js"

awk -v framefile="$BOARD/frame.js" -v editorfile="$BOARD/editor.js" '
  index($0, "<script src=\"/board/frame.js\"></script>") {
    print "<script>"
    while ((getline line < framefile) > 0) print line
    close(framefile)
    print "</script>"
    next
  }
  index($0, "<script src=\"/board/editor.js\"></script>") {
    print "<script>"
    while ((getline line < editorfile) > 0) print line
    close(editorfile)
    print "</script>"
    next
  }
  { print }
' "$SAND/kanban/kanban.html" > "$WORK/kanban-frame.html"
grep -q "LinceWidgetHost" "$WORK/kanban-frame.html" || { echo "frame.js inline failed"; exit 1; }
grep -q "LinceBodyEditor" "$WORK/kanban-frame.html" || { echo "editor.js inline failed"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="status"></div>
<script src="bundle.js"></script>
<script>
  window.__sent = [];
  window.__wsCount = 0;
  class FakeWS {
    constructor(url) { this.url = url; this.readyState = 0; window.__ws = this;
      window.__wsCount += 1; this._open = [];
      setTimeout(() => { this.readyState = 1; this._open.forEach((f) => f()); }, 0); }
    addEventListener(t, f) { if (t === "open") this._open.push(f);
      else if (t === "message") this._msg = f; else if (t === "close") this._close = f;
      else if (t === "error") this._err = f; }
    send(raw) { window.__sent.push(JSON.parse(raw)); }
    close() { this.readyState = 3; }
  }
  FakeWS.CONNECTING=0; FakeWS.OPEN=1; FakeWS.CLOSING=2; FakeWS.CLOSED=3;
  window.WebSocket = FakeWS;
  window.__inbound = (obj) => window.__ws._msg({ data: JSON.stringify(obj) });

  const patches = [];
  const metaStore = { cardState: {} };

  const frame = document.createElement("iframe");
  frame.className = "package-widget__frame";
  frame.dataset.packageInstanceId = "card-kanban";
  frame.src = "kanban-frame.html";
  document.body.appendChild(frame);

  const bridge = createWidgetBridge({
    statusNode: document.getElementById("status"),
    getFrames: () => [frame],
    initialState: {},
    getCardMeta: () => metaStore,
    getCardAbiListen: () => [],
    getCardGroupStack: () => [],
    setCardState: () => {},
    patchCardState: (id, patch) => {
      patches.push({ id, patch });
      metaStore.cardState = Object.assign({}, metaStore.cardState, patch);
    },
    setCardStreamsEnabled: () => {}, handleShellAction: () => {},
    invalidateServerAuth: () => {}, onError: () => {},
  });

  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const doc = () => frame.contentDocument;
  const cardByTitle = (t) => [...doc().querySelectorAll(".card")]
    .find((c) => (c.querySelector(".card-title") || {}).textContent === t);
  const bodyText = (t) => {
    const el = cardByTitle(t) && cardByTitle(t).querySelector(".card-body");
    return el ? el.textContent : null;
  };
  const clickAll = (mode) => {
    const btn = doc().querySelector(`#all-modes button[data-mode="${mode}"]`);
    btn.click();
  };
  const recordClickedSent = () => window.__sent.some((m) =>
    m.type === "lane_send" && m.room === "abi:recordClicked");

  (async () => {
    const results = {};
    await wait(400);

    results.one_socket = window.__wsCount === 1;
    results.kanban_subscribed = window.__sent.some((m) =>
      m.type === "subscribe" && String(m.id || "") === "card-kanban:kanban");

    const longBody = ["l1","l2","l3","l4","l5","l6","l7","l8","l9","l10"].join("\n");
    window.__inbound({ type: "snapshot", id: "card-kanban:kanban", rows: [
      { uid: "r_long",  head: "Long",  slug: "long",  body: longBody, quantity: 0 },
      { uid: "r_short", head: "Short", slug: "short", body: "just one line", quantity: -1 },
    ]});
    await wait(150);

    // Default is full: the whole body shows, last line included, no ellipsis.
    const full = bodyText("Long");
    results.full_default = !!full && full.includes("l10") && !full.includes("...");

    // Host pushes cardState (e.g. restored board) -> the sand adopts compact.
    metaStore.cardState = { kanban: { defaultBodyMode: "compact" } };
    bridge.syncFrames();
    await wait(150);
    const compact = bodyText("Long");
    results.compact_adopted = !!compact && compact.includes("l6")
      && !compact.includes("l10") && compact.includes("...");

    // "All head" from the toolbar: bodies hidden, override maps cleared, and
    // the preference persisted up through the flat patch-card-state path.
    clickAll("head");
    await wait(150);
    results.head_hides_body = bodyText("Long") === null && bodyText("Short") === null;
    results.patch_persisted = patches.some((p) => p.id === "card-kanban"
      && p.patch && p.patch.kanban && p.patch.kanban.defaultBodyMode === "head");

    // Per-card override beats the board default (2026-07-18: the per-card
    // hover controls are three direct-select icons now, not a cycle button):
    // Long picks "full" directly; Short stays hidden (still board default).
    window.__sent.length = 0;
    cardByTitle("Long").querySelector('[data-card-mode="full"]').click();
    await wait(120);
    const overridden = bodyText("Long");
    results.card_override_beats_default = !!overridden && overridden.includes("l10")
      && bodyText("Short") === null;
    results.mode_click_not_recordclicked = !recordClickedSent();

    // Per-column override beats the board default: All full, then the "Next"
    // column (Short lives there) to head -> Short hidden, Long still full.
    clickAll("full");
    await wait(120);
    doc().querySelector('[data-column-mode="next"]').click();
    await wait(120);
    results.column_override_beats_default =
      bodyText("Short") === null && !!bodyText("Long");

    document.title = "RESULT=" + JSON.stringify(results);
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=8000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check one_socket                    "more than one transport socket was opened"
check kanban_subscribed             "kanban did not subscribe its driving Protein"
check full_default                  "default mode did not render the full body"
check compact_adopted               "pushed cardState (compact) was not adopted / not truncated"
check head_hides_body               "All-head did not hide card bodies"
check patch_persisted               "the mode change was not persisted via patch-card-state"
check card_override_beats_default   "per-card override did not beat the board default"
check mode_click_not_recordclicked  "clicking a mode button leaked a recordClicked emit"
check column_override_beats_default "per-column override did not beat the board default"

[ "$fail" -eq 0 ] && echo "PASS: K1 body view modes (head/compact/full, overrides, persistence)" || exit 1
