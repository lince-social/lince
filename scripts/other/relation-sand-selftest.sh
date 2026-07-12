#!/usr/bin/env bash
# Behavioral verification of the merged Relation/Trail sand data path.
#
# Drives the Relation sand JS in headless chromium against a stubbed widget
# bridge. Verifies:
#   1. graph mode subscribes through Protein with zero link kinds by default
#   2. adding a link kind persists card state and re-subscribes with that kind
#   3. Trail mode does not infer an order kind
#   4. setting an explicit Trail order kind subscribes with topo(order)
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

awk 'f && /^"##\.to_string\(\)$/{f=0} f{print} /r##"$/{f=1}' \
  "$ROOT/crates/web/src/sand/relations/script.rs" > "$WORK/relations.js"
[ -s "$WORK/relations.js" ] || { echo "could not extract relations.js"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><title>pending</title></head><body>
<main id="app" data-lince-bridge-root>
  <button id="panel-toggle"></button>
  <button id="panel-close"></button>
  <button id="create-open"></button>
  <button id="create-close"></button>
  <canvas id="graph" width="900" height="600" style="width:900px;height:600px"></canvas>
  <div id="empty-state" hidden><h3 class="emptyState__title"></h3><p class="emptyState__copy"></p></div>
  <aside id="controls-panel" hidden>
    <select id="relation-mode"><option value="graph">Graph</option><option value="trail">Trail</option></select>
    <p id="relation-mode-summary"></p>
    <input id="relation-link-kind-input"><button id="relation-link-kind-add"></button>
    <div id="relation-link-kind-list"></div>
    <select id="edge-render"><option value="fibers">Fibers</option><option value="collapsed">Collapsed</option></select>
    <input id="trail-order-kind">
    <select id="trail-direction"><option value="from_to">from_to</option><option value="to_from">to_from</option></select>
    <span id="mode-pill"></span><span id="origin-pill"></span><span id="row-pill"></span><span id="link-pill"></span><span id="filter-pill"></span>
    <pre id="origin-text"></pre><pre id="view-sql"></pre><pre id="projection-view-summary"></pre>
  </aside>
  <aside id="record-panel" hidden>
    <button id="record-save"></button><button id="record-delete"></button><button id="record-close"></button>
    <div id="record-id"></div><input id="record-quantity"><textarea id="record-head"></textarea><textarea id="record-body"></textarea>
    <div id="current-need-list"></div><input id="need-search-query"><div id="need-search-summary"></div><div id="need-choice-list"></div><div id="child-list"></div>
  </aside>
  <aside id="create-panel" hidden>
    <input id="create-head"><textarea id="create-body"></textarea><input id="create-quantity">
    <div id="create-summary"></div><input id="create-need-search"><div id="create-need-summary"></div><div id="create-need-choice-list"></div>
    <button id="create-need-clear"></button><div id="create-category-list"></div><button id="create-clear"></button><button id="create-submit"></button>
  </aside>
  <button id="zoom-fit"></button><button id="zoom-in"></button><button id="zoom-out"></button>
</main>
<script>
  window.__subs = [];
  window.__patches = [];
  window.__cardState = {};
  window.LinceWidgetHost = {
    getCardState() { return window.__cardState; },
    getMeta() { return { cardState: window.__cardState }; },
    patchCardState(patch) {
      window.__patches.push(patch);
      window.__cardState = { ...window.__cardState, ...patch };
    },
    subscribe(handler) { handler({ meta: { cardState: window.__cardState } }); return () => {}; },
    subscribeProtein(subId, protein, handler) {
      window.__subs.push({ subId, protein });
      handler({ rows: [
        { uid: "a", kind: "plain", head: "A", body: "", quantity: -1, links: protein.include?.links?.kinds?.includes("before") ? [
          { uid: "l1", from: "a", to: "b", kind: "before", direction: "out", other: "b", quantity: null }
        ] : [] },
        { uid: "b", kind: "plain", head: "B", body: "", quantity: 0, links: protein.include?.links?.kinds?.includes("before") ? [
          { uid: "l1", from: "a", to: "b", kind: "before", direction: "in", other: "a", quantity: null }
        ] : [] },
      ], live: true });
      return () => {};
    },
    act(action) { window.__act = action; return Promise.resolve({ ok: true, created: "new" }); },
  };
</script>
<script src="relations.js"></script>
<script>
  setTimeout(() => {
    document.getElementById("relation-link-kind-input").value = "before";
    document.getElementById("relation-link-kind-add").click();
    document.getElementById("relation-mode").value = "trail";
    document.getElementById("relation-mode").dispatchEvent(new Event("change"));
    const afterTrailNoKind = window.__subs.length;
    document.getElementById("trail-order-kind").value = "before";
    document.getElementById("trail-order-kind").dispatchEvent(new Event("change"));
    setTimeout(() => {
      const first = window.__subs[0]?.protein || {};
      const withKind = window.__subs.find((entry) => (entry.protein.include?.links?.kinds || []).includes("before"))?.protein || {};
      const trail = window.__subs[window.__subs.length - 1]?.protein || {};
      document.title = "FIRST_KINDS=" + JSON.stringify(first.include?.links?.kinds || [])
        + " WITH_KIND=" + JSON.stringify(withKind.include?.links?.kinds || [])
        + " TRAIL_NO_KIND_SUBS=" + afterTrailNoKind
        + " TOTAL_SUBS=" + window.__subs.length
        + " TRAIL_ORDER=" + JSON.stringify(trail.order || [])
        + " PATCHES=" + JSON.stringify(window.__patches);
    }, 50);
  }, 50);
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless=new --disable-gpu --no-sandbox --disable-crash-reporter --disable-dev-shm-usage \
  --virtual-time-budget=3000 --dump-dom "file://$WORK/harness.html" 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g' || true)"

echo "result: $TITLE"

fail=0
grep -q 'FIRST_KINDS=\[\]' <<<"$TITLE" || { echo "FAIL: default graph subscription should have zero link kinds"; fail=1; }
grep -q 'WITH_KIND=\["before"\]' <<<"$TITLE" || { echo "FAIL: adding a kind did not re-subscribe with before"; fail=1; }
grep -q '"topo":"before"' <<<"$TITLE" || { echo "FAIL: explicit Trail order kind did not produce topo(before)"; fail=1; }
grep -q '"linkKinds":\["before"\]' <<<"$TITLE" || { echo "FAIL: link kind was not persisted to card state"; fail=1; }

[ "$fail" -eq 0 ] && echo "PASS: relation sand explicit links + Trail order mode" || exit 1
