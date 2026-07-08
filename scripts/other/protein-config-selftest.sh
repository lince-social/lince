#!/usr/bin/env bash
# Behavioral verification of the sand-settings "Data (Protein)" panel (Stage 8b).
#
# Drives board/protein-config.js in headless chromium against a STUBBED transport
# WebSocket and injected board deps — no server boot — and asserts the CRUD +
# driver wiring the migration needs:
#   1. list   : it subscribes to the saved-Protein list and renders its options
#   2. drive  : picking a saved Protein writes { savedProtein } to the card state
#   3. create : Save issues a `save-protein` Action
#
# Requires: chromium on PATH. Usage: scripts/other/protein-config-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

cp "$ROOT/crates/web/static/presentation/board/protein-config.js" "$WORK/protein-config.js"

cat > "$WORK/pharness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<select id="widget-config-protein-driver"></select>
<input id="widget-config-protein-name"><input id="widget-config-protein-title">
<textarea id="widget-config-protein-ast"></textarea>
<button id="widget-config-protein-validate"></button>
<button id="widget-config-protein-save"></button>
<button id="widget-config-protein-delete"></button>
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
      { uid: "p1", slug: "views.stock", kind: "protein", head: "Stock", body: '{"source":"record"}', quantity: 1 },
    ]});
    const sel = document.getElementById("widget-config-protein-driver");
    const optCount = sel.options.length;
    sel.value = "saved:views.stock";
    sel.dispatchEvent(new Event("change"));
    document.getElementById("widget-config-protein-name").value = "views.new";
    document.getElementById("widget-config-protein-ast").value = '{"source":"record","limit":5}';
    document.getElementById("widget-config-protein-save").click();
    setTimeout(() => {
      const sent = window.__sent || [];
      const save = sent.find(m => m.type==="act" && m.action?.action==="save-protein");
      document.title = "OPTIONS=" + optCount + " DRIVE=" + (window.__patch[0]?.savedProtein)
        + " SAVED=" + (save?.action?.slug) + " SUB=" + (sent.some(m=>m.type==="subscribe"&&m.id==="protein-list")?"yes":"no");
    }, 20);
  }, 20);
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=2500 --dump-dom pharness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"

fail=0
grep -q "SUB=yes" <<<"$TITLE" || { echo "FAIL: panel did not subscribe to the saved-Protein list"; fail=1; }
grep -q "OPTIONS=3" <<<"$TITLE" || { echo "FAIL: expected All + Inline + 1 saved option"; fail=1; }
grep -q "DRIVE=views.stock" <<<"$TITLE" || { echo "FAIL: picking a saved Protein did not drive the card"; fail=1; }
grep -q "SAVED=views.new" <<<"$TITLE" || { echo "FAIL: Save did not issue save-protein"; fail=1; }

[ "$fail" -eq 0 ] && echo "PASS: protein-config panel list + drive + save" || exit 1
