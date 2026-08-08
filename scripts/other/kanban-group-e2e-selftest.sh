#!/usr/bin/env bash
# COMPOSED end-to-end verification of the kanban add-as-group flow (Stage 8b,
# base task 1 + task 2 / kanban Track B) with the REAL pieces, node-free:
#   - the real served sands `kanban.html` + `Record.html`
#   - the real `frame.js` sand host running INSIDE real iframes
#   - the real unified `widget-bridge.js` + shared `transport.js`
#   - wired with `getFrames`/`getCardGroupStack` exactly as `main.js` does
# Only the transport WebSocket is stubbed. This closes the seam every other
# selftest stubs: real iframe -> frame.js reading `data-package-instance-id` ->
# postMessage -> unified bridge -> group-scoped `recordClicked` -> the packaged
# Record re-subscribing FOCUSED on the clicked record (`uid_eq`).
#
# The sharp risk it guards: if frame.js's resolved instanceId did NOT line up
# with the id `getCardGroupStack` is keyed on, the emit would read as ungrouped
# and broadcast board-wide (scoping silently broken). So it asserts BOTH that the
# same-group Record focuses AND that a different-group Record does not.
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/kanban-group-e2e-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BOARD="$ROOT/crates/web/static/presentation/board"
SAND="$ROOT/crates/web/src/sand"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Bundle transport + unified bridge as one classic script (strip import/export).
sed 's/^export function/function/; /^import /{:a;/;$/!{N;ba};d}' "$BOARD/transport.js"       > "$WORK/bundle.js"
sed 's/^export function/function/; /^import /{:a;/;$/!{N;ba};d}' "$BOARD/widget-bridge.js"    >> "$WORK/bundle.js"

# Build the sand iframe documents from the REAL served sands, inlining the real
# frame.js in place of the `/board/frame.js` script tag (which cannot resolve on
# file://). This keeps frame.js's instanceId resolution + host API intact.
# (node and python are both unavailable here — awk does the inlining.)
inline_frame() {
  local src="$1" out="$2"
  awk -v framefile="$BOARD/frame.js" '
    index($0, "<script src=\"/board/frame.js\"></script>") {
      print "<script>"
      while ((getline line < framefile) > 0) print line
      close(framefile)
      print "</script>"
      next
    }
    { print }
  ' "$src" > "$out"
  grep -q "LinceWidgetHost" "$out" || { echo "frame.js inline failed for $src"; exit 1; }
}
inline_frame "$SAND/kanban/kanban.html"        "$WORK/kanban-frame.html"
inline_frame "$SAND/record/record.html" "$WORK/recinfo-frame.html"

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
    send(raw) {
      const msg = JSON.parse(raw);
      window.__sent.push(msg);
      // Auto-ACK Actions so sands awaiting H.act(...) proceed (creation flow).
      if (msg.type === "act") {
        setTimeout(() => window.__inbound(
          { type: "action_ok", id: msg.id, created: "r_new", facts: [] }), 0);
      }
    }
    close() { this.readyState = 3; }
  }
  FakeWS.CONNECTING=0; FakeWS.OPEN=1; FakeWS.CLOSING=2; FakeWS.CLOSED=3;
  window.WebSocket = FakeWS;
  window.__inbound = (obj) => window.__ws._msg({ data: JSON.stringify(obj) });

  // Three cards, mirroring an added kanban group + an unrelated Record in
  // a DIFFERENT group. Same ids the iframes carry; getCardGroupStack keyed on
  // them, exactly like main.js.
  const groups = { "card-kanban": ["g1"], "card-recinfo": ["g1"], "card-recinfo-other": ["g2"] };
  const abiListen = {
    "card-kanban": [],
    "card-recinfo": ["recordClicked", "recordCreate"],
    "card-recinfo-other": ["recordClicked", "recordCreate"],
  };

  function makeIframe(id, file) {
    const f = document.createElement("iframe");
    f.className = "package-widget__frame";
    f.dataset.packageInstanceId = id; // set BEFORE load so frame.js reads it
    f.src = file;
    document.body.appendChild(f);
    return f;
  }
  const kanban = makeIframe("card-kanban", "kanban-frame.html");
  const recinfo = makeIframe("card-recinfo", "recinfo-frame.html");
  const other = makeIframe("card-recinfo-other", "recinfo-frame.html");
  const frames = [kanban, recinfo, other];

  // A kanban card ships with NO driving Protein by default (2026-07-18) — it
  // subscribes to nothing until the Data panel picks one. Seed the same
  // shape the sand's own (now-removed) DEFAULT_PROTEIN used to auto-apply so
  // this test still exercises the real subscribe/render path. Relies on the
  // widget-bridge `lince:ready` handshake re-pushing bridge-state to a frame
  // once it's actually listening (2026-07-18 fix) rather than a manual
  // `syncFrames()` call — this proves that fix works, not just papers over it.
  const cardMeta = {
    "card-kanban": { cardState: { protein: {
      source: "record",
      where: [{ kind_eq: "plain" }],
      order: [{ asc: "quantity" }, { asc: "created_at" }],
      include: { links: { kinds: ["assigned-to", "part-of"], direction: "out" } },
    } } },
  };

  createWidgetBridge({
    statusNode: document.getElementById("status"),
    getFrames: () => frames,
    initialState: {},
    getCardMeta: (id) => cardMeta[id] || {},
    getCardAbiListen: (id) => abiListen[id] || [],
    getCardGroupStack: (id) => groups[id] || [],
    setCardState: () => {}, patchCardState: () => {}, setCardStreamsEnabled: () => {},
    handleShellAction: () => {}, invalidateServerAuth: () => {}, onError: () => {},
  });

  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const sentSubs = () => window.__sent.filter((m) => m.type === "subscribe");
  const uidEqSubForCard = (cardId, uid) => sentSubs().some((m) =>
    String(m.id || "").startsWith(cardId + ":") &&
    JSON.stringify(m.protein || {}).includes('"uid_eq":"' + uid + '"'));

  (async () => {
    const results = {};
    // Let the iframes load, run frame.js, announce ready, and post their initial
    // subscribes up through the bridge.
    await wait(400);

    results.one_socket = window.__wsCount === 1;

    // kanban subscribed a driving protein (id "<card-kanban>:kanban"); feed it a
    // snapshot with one record so it renders a clickable card.
    const kanbanSub = sentSubs().find((m) => String(m.id || "") === "card-kanban:kanban");
    results.kanban_subscribed = !!kanbanSub;
    window.__inbound({ type: "snapshot", id: "card-kanban:kanban",
      rows: [{ uid: "r_click", head: "Ship it", slug: "ship-it", body: "", quantity: 0 }] });
    await wait(150);

    // The card rendered inside the REAL kanban iframe?
    // Click the TITLE (2026-07-17: title -> Record; body -> in-place edit).
    const cardEl = kanban.contentDocument && kanban.contentDocument.querySelector(".card .card-title");
    results.card_rendered = !!cardEl;

    // Click it. Now only assert on frames sent AFTER the click.
    window.__sent.length = 0;
    if (cardEl) cardEl.click();
    await wait(200);

    // Same-group Record re-subscribed FOCUSED on the clicked uid (proves
    // click -> scoped ABI -> frame.js instanceId lines up with the group id ->
    // Record focus). Different-group Record did NOT (scoping holds;
    // the emit was not a board-wide broadcast).
    results.same_group_focused = uidEqSubForCard("card-recinfo", "r_click");
    results.diff_group_not_focused = !uidEqSubForCard("card-recinfo-other", "r_click");

    // "New task" in kanban opens CREATION MODE in the same-group Record:
    // the same fields the get shows, writable and empty. Scoped like clicks.
    window.__sent.length = 0;
    kanban.contentDocument.getElementById("open-create").click();
    await wait(250);
    const rc = recinfo.contentDocument;
    const oc = other.contentDocument;
    results.create_mode_same_group = rc.getElementById("create").classList.contains("open");
    results.create_mode_scoped = !oc.getElementById("create").classList.contains("open");
    results.create_fields_empty = rc.getElementById("c-head").value === ""
      && rc.getElementById("c-body").value === "";

    // Fill + submit: Record writes create-record, then focuses the
    // created record (the ack's uid) with a uid_eq subscription.
    rc.getElementById("c-head").value = "Fresh";
    rc.getElementById("c-submit").click();
    await wait(300);
    results.create_action = window.__sent.some((m) => m.type === "act"
      && m.action && m.action.action === "create-record" && m.action.head === "Fresh");
    results.created_focused = uidEqSubForCard("card-recinfo", "r_new");

    document.title = "RESULT=" + JSON.stringify(results);
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=4000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result (iframe/module load?)"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check one_socket             "more than one transport socket was opened"
check kanban_subscribed      "the real kanban sand did not subscribe its driving Protein"
check card_rendered          "the real kanban sand did not render a clickable card from the snapshot"
check same_group_focused     "clicking a kanban card did not focus the same-group Record on that uid"
check diff_group_not_focused "the click leaked to a different-group Record (scoping broken / broadcast)"
check create_mode_same_group "New task did not open creation mode in the same-group Record"
check create_mode_scoped     "creation mode leaked to a different-group Record"
check create_fields_empty    "creation mode fields were not empty/writable"
check create_action          "Record creation did not send create-record"
check created_focused        "Record did not focus the created record after the ack"

[ "$fail" -eq 0 ] && echo "PASS: real iframe -> frame.js -> unified bridge -> group-scoped recordClicked/recordCreate -> Record focus + creation" || exit 1
