#!/usr/bin/env bash
# Behavioral verification of the rebuilt table sand (Stage 8b).
#
# The table sand is now a pure Protein(reads)+Actions(writes) client. This drives
# its JS in headless chromium against a STUBBED widget bridge — no server boot
# needed — and asserts the three things the migration requires of a ported sand:
#   1. snapshot   : it renders the rows a Protein subscription delivers
#   2. read shape : it subscribes to `{ source: record }`
#   3. Action r/t : Create issues `create-record`, an inline edit issues `set-slug`
#
# Requires: chromium on PATH. Usage: scripts/other/table-sand-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Extract the sand's JS from its Rust raw-string literal.
awk 'f && /^    "#$/{f=0} f{print} /r#"$/{f=1}' \
  "$ROOT/crates/web/src/sand/table/script.rs" > "$WORK/table.js"
[ -s "$WORK/table.js" ] || { echo "could not extract table.js"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="app">
  <header><span id="table-status"></span>
    <button id="info-open"></button><button id="create-open"></button></header>
  <aside id="table-details" hidden><div class="detailGrid"></div>
    <button id="info-close"></button></aside>
  <aside id="create-panel" hidden>
    <button id="create-close"></button>
    <select id="create-table-select"></select>
    <div id="create-fields"></div>
    <button id="create-submit"></button>
  </aside>
  <section id="table-body"></section>
  <div id="table-toasts"></div>
</div>
<script>
  window.__acts = [];
  window.LinceWidgetHost = {
    subscribeProtein(subId, protein, handler) {
      window.__protein = protein;
      handler({ rows: [
        { uid: "r_1", slug: "apples", kind: "plain", head: "Apples", body: "red", quantity: 3 },
        { uid: "r_2", slug: null, kind: "person", head: "Ana", body: "", quantity: 1 },
      ], live: true });
      return () => {};
    },
    act(action) { window.__acts.push(action); return Promise.resolve({ ok: true, created: "r_new" }); },
  };
</script>
<script src="table.js"></script>
<script>
  document.getElementById("create-open").click();
  const headInput = document.querySelector('#create-fields input');
  headInput.value = "New"; headInput.dispatchEvent(new Event("input"));
  document.getElementById("create-submit").click();
  const slugCell = document.querySelector('td[data-column="slug"]');
  slugCell.click();
  const editor = document.querySelector('#table-body .cellEditor');
  editor.value = "oranges";
  editor.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  setTimeout(() => {
    const rows = document.querySelectorAll('#table-body tbody tr').length;
    document.title = "ROWS=" + rows + " SRC=" + (window.__protein && window.__protein.source)
      + " ACTS=" + JSON.stringify(window.__acts);
  }, 50);
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --virtual-time-budget=2000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"

fail=0
grep -q "ROWS=2" <<<"$TITLE" || { echo "FAIL: expected 2 rendered rows (snapshot)"; fail=1; }
grep -q "SRC=record" <<<"$TITLE" || { echo "FAIL: expected subscription to source=record"; fail=1; }
grep -q '"action":"create-record"' <<<"$TITLE" || { echo "FAIL: Create did not issue create-record"; fail=1; }
grep -q '"action":"set-slug","target":"r_1","slug":"oranges"' <<<"$TITLE" || { echo "FAIL: inline edit did not issue set-slug"; fail=1; }

[ "$fail" -eq 0 ] && echo "PASS: table sand snapshot + read shape + Action round-trip" || exit 1
