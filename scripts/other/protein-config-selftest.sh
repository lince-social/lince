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
#   4. all     : "All records" drives an EXPLICIT { source: "record" } Protein,
#                not { savedProtein: null, protein: null } — that sentinel now
#                means "no data source configured" to kanban/relations sands
#                (2026-07-18 no-automatic-fallback change), which would render
#                as empty instead of everything (the bug this regression-tests).
#   5. autocomplete: link-kind text inputs (the "of kind" include and the
#                linked_to pair filter) subscribe to { source: "concept" } and
#                offer the results via a shared <datalist>, since link kinds
#                are open-ended Lingua vocabulary, not a fixed enum.
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
    window.__ws.emit({ type: "snapshot", id: "protein-concepts", rows: [
      { uid: "c1", name: "before" }, { uid: "c2", name: "blocks" },
    ]});
    const listEl = document.getElementById("widget-config-protein-list");
    const items = listEl.querySelectorAll(".protein-item").length; // All records + Stock
    listEl.querySelectorAll(".protein-item__pick")[0].click();     // drive with "All records"
    const allDrive = JSON.stringify(window.__patch[0]);
    listEl.querySelectorAll(".protein-item__pick")[1].click();     // drive with "Stock"

    document.querySelector(".protein-new").click();                // + New Protein
    const b = document.getElementById("widget-config-protein-builder");
    const nameInput = b.querySelector(".protein-field input");
    nameInput.value = "My Query"; nameInput.dispatchEvent(new Event("input"));
    // links include: multi-kind rows, each wired to the link-kind datalist
    const checks = Array.from(b.querySelectorAll(".protein-check"));
    const linksCheck = checks.find((l) => l.textContent.includes("links"));
    linksCheck.querySelector("input").click();
    const kindInputs = () => Array.from(b.querySelectorAll(".protein-checks .protein-row input"));
    const linksKindList = kindInputs()[0].getAttribute("list");
    kindInputs()[0].value = "before"; kindInputs()[0].dispatchEvent(new Event("input"));
    b.querySelector(".protein-checks .protein-add").click();       // + kind
    kindInputs()[1].value = "blocks"; kindInputs()[1].dispatchEvent(new Event("input"));
    b.querySelectorAll(".protein-add")[0].click();                 // + filter (kind_eq plain)
    const filterRow = b.querySelector(".protein-row");
    const filterSelect = filterRow.querySelector("select");
    filterSelect.value = "quantity_gte";
    filterSelect.dispatchEvent(new Event("change"));
    const quantityInput = b.querySelector(".protein-row input");
    quantityInput.value = "0";
    quantityInput.dispatchEvent(new Event("input"));
    // + filter #2: assignee sugar (linked_to kind=assigned-to)
    b.querySelectorAll(".protein-add")[0].click();
    let rows = b.querySelectorAll(".protein-row");
    let sel = rows[1].querySelector("select");
    sel.value = "assignee"; sel.dispatchEvent(new Event("change"));
    rows = b.querySelectorAll(".protein-row");
    const aInput = rows[1].querySelector("input");
    aInput.value = "ana"; aInput.dispatchEvent(new Event("input"));
    // + filter #3: raw linked_to (kind + to pair)
    b.querySelectorAll(".protein-add")[0].click();
    rows = b.querySelectorAll(".protein-row");
    sel = rows[2].querySelector("select");
    sel.value = "linked_to"; sel.dispatchEvent(new Event("change"));
    rows = b.querySelectorAll(".protein-row");
    const pair = rows[2].querySelectorAll("input");
    const pairKindList = pair[0].getAttribute("list");
    pair[0].value = "tag"; pair[0].dispatchEvent(new Event("input"));
    pair[1].value = "tasks"; pair[1].dispatchEvent(new Event("input"));
    const datalist = document.getElementById("protein-link-kinds");
    const datalistOptions = datalist ? Array.from(datalist.querySelectorAll("option")).map((o) => o.value) : [];
    b.querySelector(".protein-actions .button--accent").click();   // Save
    setTimeout(() => {
      const sent = window.__sent || [];
      const save = sent.find(m => m.type==="act" && m.action?.action==="save-protein");
      document.title = "ITEMS=" + items
        + " DRIVE=" + (window.__patch[1]?.savedProtein)
        + " ALLDRIVE=" + allDrive
        + " LINKSKINDLIST=" + linksKindList
        + " LINKSKINDS=" + JSON.stringify(save?.action?.ast?.include?.links?.kinds)
        + " PAIRKINDLIST=" + pairKindList
        + " DATALIST=" + JSON.stringify(datalistOptions)
        + " SLUG=" + (save?.action?.slug)
        + " HEAD=" + (save?.action?.head)
        + " QGTE=" + (save?.action?.ast?.where?.[0]?.quantity_gte)
        + " ASSIGNEE=" + JSON.stringify(save?.action?.ast?.where?.[1]?.linked_to)
        + " LINKED=" + JSON.stringify(save?.action?.ast?.where?.[2]?.linked_to)
        + " SUB=" + (sent.some(m=>m.type==="subscribe"&&m.id==="protein-list")?"yes":"no")
        + " SUBCONCEPT=" + (sent.some(m=>m.type==="subscribe"&&m.id==="protein-concepts"&&m.protein?.source==="concept")?"yes":"no");
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
grep -q "SUBCONCEPT=yes" <<<"$TITLE" || { echo "FAIL: panel did not subscribe to { source: concept } for link-kind autocomplete"; fail=1; }
grep -q "ITEMS=2" <<<"$TITLE" || { echo "FAIL: expected 'All records' + 1 saved item"; fail=1; }
grep -q 'ALLDRIVE={"savedProtein":null,"protein":{"source":"record"}}' <<<"$TITLE" || { echo "FAIL: 'All records' must drive an explicit { source: record } Protein, not the { savedProtein: null, protein: null } no-data-source sentinel"; fail=1; }
grep -q "DRIVE=views.stock" <<<"$TITLE" || { echo "FAIL: picking a saved Protein did not drive the card"; fail=1; }
grep -q "LINKSKINDLIST=protein-link-kinds" <<<"$TITLE" || { echo "FAIL: the links 'of kinds' input is not wired to the link-kind datalist"; fail=1; }
grep -q 'LINKSKINDS=\["before","blocks"\]' <<<"$TITLE" || { echo "FAIL: the links include did not build MULTIPLE kinds into the AST"; fail=1; }
grep -q "PAIRKINDLIST=protein-link-kinds" <<<"$TITLE" || { echo "FAIL: the linked_to pair filter's kind input is not wired to the link-kind datalist"; fail=1; }
grep -q 'DATALIST=\["before","blocks"\]' <<<"$TITLE" || { echo "FAIL: the link-kind datalist did not offer the existing concepts"; fail=1; }
grep -q "SLUG=my-query" <<<"$TITLE" || { echo "FAIL: Save did not derive the slug from the name"; fail=1; }
grep -q "HEAD=My Query" <<<"$TITLE" || { echo "FAIL: Save did not send the name as head"; fail=1; }
grep -q "QGTE=0" <<<"$TITLE" || { echo "FAIL: the GUI quantity >= filter did not build into the AST"; fail=1; }
grep -q 'ASSIGNEE={"kind":"assigned-to","to":"ana"}' <<<"$TITLE" || { echo "FAIL: assignee sugar did not build linked_to kind=assigned-to"; fail=1; }
grep -q 'LINKED={"kind":"tag","to":"tasks"}' <<<"$TITLE" || { echo "FAIL: linked_to pair did not build into the AST"; fail=1; }

[ "$fail" -eq 0 ] && echo "PASS: Data panel list + drive + GUI build/save + all-records fix + link-kind autocomplete" || exit 1
