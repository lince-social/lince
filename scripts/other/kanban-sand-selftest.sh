#!/usr/bin/env bash
# Behavioral verification of the rebuilt kanban sand (Stage 8b, Track A).
#
# The kanban sand is now a pure Protein(reads)+Actions(writes) client. This drives
# its JS in headless chromium against a STUBBED widget bridge — no server boot
# needed — and asserts what the migration requires of the ported kanban:
#   1. snapshot   : records delivered by a Protein subscription bucket into columns
#   2. read shape : it subscribes to `{ source: record }` by default
#   3. move r/t   : dragging a card to another column issues `set-concept`
#   4. create r/t : the per-column add button issues `create-record` (+ set-concept)
#   5. ABI        : clicking a card emits `recordClicked` carrying `record.uid`
#   6. driving    : picking a saved Protein re-subscribes via `subscribeSaved`
#
# Requires: chromium on PATH. Usage: scripts/other/kanban-sand-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Extract the sand's JS from its Rust raw-string literal (r#" ... "#).
awk 'f && /^    "#$/{f=0} f{print} /r#"$/{f=1}' \
  "$ROOT/crates/web/src/sand/kanban/script.rs" > "$WORK/kanban.js"
[ -s "$WORK/kanban.js" ] || { echo "could not extract kanban.js"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<main id="kanban-app">
  <header><span id="kanban-status"></span>
    <button id="kanban-info-open"></button></header>
  <aside id="kanban-details" hidden>
    <button id="kanban-info-close"></button>
    <div class="kanbanDetailGrid"></div>
  </aside>
  <section id="kanban-board"></section>
  <div id="kanban-toasts"></div>
</main>
<script>
  window.__acts = [];
  window.__events = [];
  window.__cardState = {}; // host-side card state; the modal writes this
  window.LinceWidgetHost = {
    getCardState() { return window.__cardState; },
    subscribeProtein(subId, protein, handler) {
      window.__protein = protein;
      handler({ rows: [
        { uid: "k_1", slug: "a", kind: "plain", head: "Design", body: "", quantity: 0, concept: "todo" },
        { uid: "k_2", slug: "b", kind: "plain", head: "Build", body: "b", quantity: 0, concept: "doing" },
        { uid: "k_3", slug: null, kind: "plain", head: "Loose", body: "", quantity: 0, concept: null },
      ], live: true });
      return () => {};
    },
    subscribeSaved(subId, name, handler) {
      window.__savedName = name;
      handler({ rows: [
        { uid: "k_9", slug: "z", kind: "plain", head: "Saved", body: "", quantity: 0, concept: "done" },
      ], live: true });
      return () => {};
    },
    act(action) { window.__acts.push(action); return Promise.resolve({ ok: true, created: "k_new" }); },
    emit(topic, data) { window.__events.push({ topic, data }); },
  };
</script>
<script src="kanban.js"></script>
<script>
  // 1+2: initial render buckets rows into columns.
  const colsAtStart = document.querySelectorAll('#kanban-board .kanbanColumn').length;

  // 3: drag k_1 (todo) onto the "doing" column -> set-concept.
  const doingCol = document.querySelector('.kanbanColumn[data-column-id="doing"]');
  const dt = new DataTransfer();
  dt.setData("text/x-uid", "k_1");
  doingCol.dispatchEvent(new DragEvent("drop", { dataTransfer: dt, bubbles: true }));

  // 5: click a card -> recordClicked ABI event with record.uid.
  document.querySelector('.kanbanCard[data-uid="k_2"]').click();

  // 4: per-column add on "doing" -> create-record then set-concept.
  doingCol.querySelector('.kanbanColumnAdd').click();

  // 6: the modal picks a saved Protein to drive this card: set state + notify.
  setTimeout(() => {
    window.__cardState = { savedProtein: "views.tasks" };
    document.dispatchEvent(new CustomEvent("lince-bridge-state", { detail: {} }));
    setTimeout(() => {
      const evt = window.__events[0] || {};
      document.title =
        "COLS=" + colsAtStart +
        " SRC=" + (window.__protein && window.__protein.source) +
        " SAVED=" + (window.__savedName || "") +
        " EVT=" + (evt.topic || "") + ":" + ((evt.data && evt.data.record && evt.data.record.uid) || "") +
        " ACTS=" + JSON.stringify(window.__acts);
    }, 60);
  }, 40);
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --virtual-time-budget=2000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"

fail=0
grep -q "COLS=3" <<<"$TITLE" || { echo "FAIL: expected 3 columns (todo, doing, Unassigned)"; fail=1; }
grep -q "SRC=record" <<<"$TITLE" || { echo "FAIL: expected default subscription to source=record"; fail=1; }
grep -q '"action":"set-concept","target":"k_1","concept":"doing"' <<<"$TITLE" || { echo "FAIL: drag-move did not issue set-concept"; fail=1; }
grep -q '"action":"create-record"' <<<"$TITLE" || { echo "FAIL: per-column add did not issue create-record"; fail=1; }
grep -q '"action":"set-concept","target":"k_new","concept":"doing"' <<<"$TITLE" || { echo "FAIL: created card was not classified into its column"; fail=1; }
grep -q "EVT=recordClicked:k_2" <<<"$TITLE" || { echo "FAIL: card click did not emit recordClicked with record.uid"; fail=1; }
grep -q "SAVED=views.tasks" <<<"$TITLE" || { echo "FAIL: picking a saved Protein did not re-subscribe via subscribeSaved"; fail=1; }

[ "$fail" -eq 0 ] && echo "PASS: kanban snapshot + columns + move/create Actions + recordClicked ABI + driving-Protein swap" || exit 1
