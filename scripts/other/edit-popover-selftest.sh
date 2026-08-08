#!/usr/bin/env bash
# Geometry proof for the board's own chrome, in a real engine.
#
# Two things this measures that no node harness can:
#
#   1. The Edit popover is shrink-to-fit: the frame the sand ASKS the board for
#      has to contain every control it draws. The cap it used to carry
#      (`max-height:calc(100vh - 10px)`) was self-referential — `100vh` is the
#      iframe, and the iframe is sized from a measurement of this very element,
#      so the box could never settle on its natural size and the bottom row of
#      icons hung outside the frame.
#
#   2. Only one tooltip is ever up. CSS shows one on :hover AND on
#      :focus-visible, and :hover matches every ancestor under the pointer, so
#      more than one can be showing at once — including across sands, which are
#      separate documents that cannot see each other (that pair is relayed
#      through the board, mirrored here).
#
# The Edit sand's markup is generated in Rust, so it comes from the real
# renderer (`--example render_official_sands`), not a copy. The CSS/JS are the
# shipped files.
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/edit-popover-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BOARD="$ROOT/crates/web/static/presentation/board"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

cargo run -q -p lince-web --example render_official_sands -- --target "$WORK/sand" >/dev/null

cp "$BOARD/lynx-ui.css" "$BOARD/lynx-ui.js" "$BOARD/LynxDS-components.js" "$WORK/"

# Same document the Cell serves, with the host-relative asset paths pointed at
# the copies above and the bridge bootstrap swapped for a stub host.
sed -e 's#/host/static/presentation/board/widget-frame-bootstrap.js#host-stub.js#' \
    -e 's#/host/static/presentation/board/LynxDS-components.js#LynxDS-components.js#' \
    -e 's#/board/lynx-ui.js#lynx-ui.js#' \
    -e 's#/board/lynx-ui.css#lynx-ui.css#' \
    "$WORK/sand/lince-shell-edit.html" > "$WORK/edit-sand.html"
grep -q "editPopover" "$WORK/edit-sand.html" || { echo "edit sand did not render"; exit 1; }

cat > "$WORK/host-stub.js" <<'JS'
// Only what the Edit sand reads: edit mode on, a few areas to list. Everything
// it writes (host.shell) goes nowhere — this measures layout, not wiring.
window.LinceWidgetHost = {
  subscribe(handler) {
    handler({ meta: { instanceId: "card-edit", shell: {
      editMode: true, selfEditCardId: "", density: 4, gridSnap: 8,
      notifications: { count: 2 },
      workspace: { items: [
        { id: "w1", name: "Trabalho", active: true, canDelete: false },
        { id: "w2", name: "Casa", active: false, canDelete: true },
        { id: "w3", name: "Projeto muito comprido", active: false, canDelete: true },
      ] },
    } } });
    return () => {};
  },
  shell() {},
};
JS

# A second sand, so the cross-document half of "one tooltip" has somewhere to
# be wrong. Plain lynx-ui, nothing board-specific.
cat > "$WORK/other-sand.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="lynx-ui.css"><script src="lynx-ui.js"></script></head>
<body class="lynx-ui">
  <div id="outer" data-lynx-tooltip="Outer container">
    <button id="inner" class="lynx-button" data-lynx-tooltip="Inner button">x</button>
  </div>
  <button id="lone" class="lynx-button" data-lynx-tooltip="Lone button">y</button>
</body></html>
HTML

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><style>body{margin:0}</style></head>
<body>
<iframe id="edit" src="edit-sand.html" style="width:180px;height:90px;border:0"></iframe>
<iframe id="other" src="other-sand.html" style="width:300px;height:200px;border:0"></iframe>
<script>
  const editFrame = document.getElementById("edit");
  const otherFrame = document.getElementById("other");
  let reported = null;

  window.addEventListener("message", (event) => {
    const data = event.data;
    if (!data || typeof data !== "object") return;
    // The board's shrink-to-fit: the frame becomes whatever the sand asked for.
    if (data.type === "lince:widget-content-size") {
      reported = data.payload;
      editFrame.style.width = reported.width + "px";
      editFrame.style.height = reported.height + "px";
      return;
    }
    // The board's tooltip relay (widget-bridge.js handleMessage).
    if (data.type === "lince:tooltip-shown") {
      for (const frame of [editFrame, otherFrame]) {
        frame.contentWindow?.postMessage(data, "*");
      }
    }
  });

  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  // CSS :hover cannot be synthesized by dispatchEvent — only real input moves
  // it — so the two halves are checked separately: that the shipped rule hides
  // a suppressed tooltip, and that the JS suppresses exactly the right ones on
  // the pointerover/focusin a real pointer would send.
  const suppressed = (node) => node.hasAttribute("data-lynx-tooltip-suppressed");
  const showing = (doc) =>
    [...doc.querySelectorAll("[data-lynx-tooltip]")].filter((node) => !suppressed(node));
  function hidesSuppressed(doc) {
    for (const sheet of doc.styleSheets) {
      for (const rule of sheet.cssRules || []) {
        if (!rule.selectorText || !rule.style) continue;
        if (
          rule.selectorText.includes("data-lynx-tooltip-suppressed") &&
          rule.style.display === "none"
        ) return true;
      }
    }
    return false;
  }

  (async () => {
    const results = {};
    const mark = () => { document.title = "RESULT=" + JSON.stringify(results); };
    try {
      await wait(600);
      const doc = editFrame.contentDocument;
      const popover = doc.getElementById("popover");

      results.popover_open = popover.classList.contains("isOpen");
      results.reported_size = !!reported && reported.width > 0 && reported.height > 0;

      // The frame the board gave us has to contain the whole popover...
      const frameWidth = editFrame.clientWidth;
      const frameHeight = editFrame.clientHeight;
      const box = popover.getBoundingClientRect();
      results.popover_inside_frame =
        Math.ceil(box.bottom) <= frameHeight && Math.ceil(box.right) <= frameWidth;

      // ...and the popover has to contain every control it draws. The bottom
      // row (notifications / new area / import / export) is the one that hung
      // outside, so check the deepest, last thing rather than the box alone.
      const controls = [...popover.querySelectorAll("button")];
      const lowest = Math.max(...controls.map((node) => node.getBoundingClientRect().bottom));
      const widest = Math.max(...controls.map((node) => node.getBoundingClientRect().right));
      results.controls_count = controls.length;
      results.controls_inside_frame =
        controls.length >= 7 && Math.ceil(lowest) <= frameHeight && Math.ceil(widest) <= frameWidth;

      // Nothing was clipped away to make that true: the popover is at its
      // natural height, not pinned to a cap derived from the frame.
      const style = doc.defaultView.getComputedStyle(popover);
      results.no_viewport_cap = style.maxHeight === "none" && style.maxWidth === "none";
      mark();

      // ── one tooltip at a time ──────────────────────────────────────────
      const otherDoc = otherFrame.contentDocument;
      const inner = otherDoc.getElementById("inner");
      const lone = otherDoc.getElementById("lone");
      const over = (node) =>
        node.dispatchEvent(new node.ownerDocument.defaultView.PointerEvent("pointerover", { bubbles: true }));

      results.suppressed_is_hidden = hidesSuppressed(otherDoc) && hidesSuppressed(doc);

      // Hovering the inner button also hovers #outer, which has its own
      // tooltip: :hover matches both.
      over(inner);
      await wait(50);
      const hovered = showing(otherDoc);
      results.one_per_document = hovered.length === 1 && hovered[0].id === "inner";

      // Coming BACK to a button that lost once has to clear its own mark. A
      // stale suppression is worse than two tooltips: that button would never
      // show one again.
      over(lone);
      await wait(50);
      over(inner);
      await wait(50);
      const again = showing(otherDoc);
      results.suppression_clears = again.length === 1 && again[0].id === "inner";
      mark();

      // A focused button keeps its tooltip while the pointer moves on — and
      // the pointer moving on lands in a DIFFERENT sand.
      lone.focus();
      await wait(50);
      results.focus_takes_over = showing(otherDoc).length === 1 && !suppressed(lone);
      const editButton = doc.querySelector('[data-action="card.add"]');
      over(editButton);
      await wait(80);
      const total = showing(otherDoc).length + showing(doc).length;
      results.one_across_sands = total === 1;
      results.winner_is_hovered = showing(doc)[0] === editButton;
      mark();
    } catch (error) {
      results.error = String(error && error.message ? error.message : error);
      mark();
    }
    mark();
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
check popover_open           "the Edit popover never opened"
check reported_size          "the sand never reported a content size to the board"
check popover_inside_frame   "the popover box hangs outside the frame it asked for"
check controls_inside_frame  "a control in the popover is outside the frame (bottom row clipped)"
check no_viewport_cap        "the popover still carries a viewport-unit cap (self-referential)"
check suppressed_is_hidden   "lynx-ui.css does not hide a suppressed tooltip"
check one_per_document       "hovering a nested tooltip showed its ancestor's tooltip too"
check suppression_clears     "a button that lost once stayed suppressed — its tooltip is dead"
check focus_takes_over       "focusing a button did not make its tooltip the only one"
check one_across_sands       "two sands showed a tooltip at the same time"
check winner_is_hovered      "the surviving tooltip is not the hovered one"

[ "$fail" -eq 0 ] || exit 1
echo "PASS: edit popover fits its frame and only one tooltip is ever up"
