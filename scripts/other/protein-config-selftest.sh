#!/usr/bin/env bash
# Behavioral verification of the sand-settings "Data" panel (Stage 8b).
#
# Drives board/protein-config.js in headless chromium against a STUBBED transport
# WebSocket and injected board deps — no server boot — and asserts the GUI CRUD +
# driver wiring:
#   1. list    : subscribes to the saved-Protein list and renders pickable items
#   2. drive   : picking a saved Protein writes { savedProtein } to the card state
#   3. build   : "+ New" + Name + "+ filter" + Save issues save-protein with the
#                GUI-built AST and a slug derived from the name (no AST typed),
#                including inclusive quantity operators.
#
# Requires: chromium on PATH. Usage: scripts/other/protein-config-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

cp "$ROOT/crates/web/static/presentation/board/protein-config.js" "$WORK/protein-config.js"
# protein-config.js now imports the shared transport (Stage 8b, base task 1),
# so the module needs it resolvable beside it. The FakeWS stub (set before the
# module runs) is what the lazily-connecting transport uses.
cp "$ROOT/crates/web/static/presentation/board/transport.js" "$WORK/transport.js"

cat > "$WORK/pharness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="widget-config-protein-list"></div>
<div id="widget-config-protein-builder"></div>
<p id="widget-config-protein-help" hidden></p>
<script>
  class FakeWS {
    static CONNECTING=0; static OPEN=1; static CLOSING=2; static CLOSED=3;
    constructor(url){ this.url=url; this.readyState=0; this.listeners={}; window.__ws=this;
      setTimeout(()=>{ this.readyState=1; (this.listeners.open||[]).forEach(f=>f()); },0); }
    addEventListener(t,f){ (this.listeners[t]=this.listeners[t]||[]).push(f); }
    send(d){ (window.__sent=window.__sent||[]).push(JSON.parse(d)); }
    emit(o){ (this.listeners.message||[]).forEach(f=>f({data:JSON.stringify(o)})); }
  }
  window.WebSocket = FakeWS;
  window.__patch = [];
</script>
<script type="module">
  import { createProteinConfigPanel } from "./protein-config.js";
  const panel = createProteinConfigPanel({
    getCard: () => ({ id: "card1", widgetState: {} }),
    patchCardState: (id, patch) => window.__patch.push(patch),
    syncFrames: () => {},
  });
  panel.open("card1");
  setTimeout(() => {
    window.__ws.emit({ type: "snapshot", id: "protein-list", rows: [
      { uid: "p1", slug: "views.stock", kind: "protein", head: "Stock", body: '{"source":"record","limit":5}', quantity: 1 },
    ]});
    const listEl = document.getElementById("widget-config-protein-list");
    const items = listEl.querySelectorAll(".protein-item").length; // All records + Stock
    listEl.querySelectorAll(".protein-item__pick")[1].click();     // drive with "Stock"

    document.querySelector(".protein-new").click();                // + New Protein
    const b = document.getElementById("widget-config-protein-builder");
    const nameInput = b.querySelector(".protein-field input");
    nameInput.value = "My Query"; nameInput.dispatchEvent(new Event("input"));
    b.querySelectorAll(".protein-add")[0].click();                 // + filter (kind_eq plain)
    const filterRow = b.querySelector(".protein-row");
    const filterSelect = filterRow.querySelector("select");
    filterSelect.value = "quantity_gte";
    filterSelect.dispatchEvent(new Event("change"));
    const quantityInput = b.querySelector(".protein-row input");
    quantityInput.value = "0";
    quantityInput.dispatchEvent(new Event("input"));
    b.querySelector(".protein-actions .button--accent").click();   // Save
    setTimeout(() => {
      const sent = window.__sent || [];
      const save = sent.find(m => m.type==="act" && m.action?.action==="save-protein");
      document.title = "ITEMS=" + items
        + " DRIVE=" + (window.__patch[0]?.savedProtein)
        + " SLUG=" + (save?.action?.slug)
        + " HEAD=" + (save?.action?.head)
        + " QGTE=" + (save?.action?.ast?.where?.[0]?.quantity_gte)
        + " SUB=" + (sent.some(m=>m.type==="subscribe"&&m.id==="protein-list")?"yes":"no");
    }, 40);
  }, 40);
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=3000 --dump-dom pharness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"

fail=0
grep -q "SUB=yes" <<<"$TITLE" || { echo "FAIL: panel did not subscribe to the saved-Protein list"; fail=1; }
grep -q "ITEMS=2" <<<"$TITLE" || { echo "FAIL: expected 'All records' + 1 saved item"; fail=1; }
grep -q "DRIVE=views.stock" <<<"$TITLE" || { echo "FAIL: picking a saved Protein did not drive the card"; fail=1; }
grep -q "SLUG=my-query" <<<"$TITLE" || { echo "FAIL: Save did not derive the slug from the name"; fail=1; }
grep -q "HEAD=My Query" <<<"$TITLE" || { echo "FAIL: Save did not send the name as head"; fail=1; }
grep -q "QGTE=0" <<<"$TITLE" || { echo "FAIL: the GUI quantity >= filter did not build into the AST"; fail=1; }

[ "$fail" -eq 0 ] && echo "PASS: Data panel list + drive + GUI build/save" || exit 1
