#!/usr/bin/env bash
# Behavioral verification of the Stage 8b todo sand data-plane port.
#
# Drives the todo sand JS in headless chromium against a stubbed widget bridge:
#   1. snapshot   : Protein rows render as a focusable list
#   2. read shape : default subscription is the focus queue Protein
#   3. Action r/t : Space completes the active item via set-quantity
#   4. swap       : changing card state to a saved Protein re-subscribes
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

awk 'f && /^"##\.to_string\(\)$/{f=0} f{print} /r##"$/{f=1}' \
  "$ROOT/crates/web/src/sand/todo/script.rs" > "$WORK/todo.js"
[ -s "$WORK/todo.js" ] || { echo "could not extract todo.js"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><title>pending</title></head><body>
<main id="app">
  <header><button id="todo-status"></button></header>
  <div id="todo-blob-layer"></div>
  <aside id="todo-details" hidden>
    <input id="task-ids-visible" type="checkbox">
    <input id="blob-enabled" type="checkbox">
    <input id="blob-viscosity" type="range">
    <input id="blob-energy" type="range">
    <input id="blob-color-input" type="color">
    <button id="blob-add-color"></button>
    <div id="blob-palette"></div>
    <div id="todo-detail-source"></div>
    <div id="todo-detail-active"></div>
    <div id="todo-detail-preview"></div>
    <div id="todo-detail-endpoint"></div>
    <div id="todo-detail-count"></div>
    <div id="todo-detail-source-count"></div>
  </aside>
  <section id="todo-list-panel" tabindex="0"></section>
</main>
<script>
  window.__acts = [];
  window.__subscriptions = [];
  window.__unsubscribed = 0;
  window.__cardState = {};
  window.LinceWidgetHost = {
    getCardState() { return window.__cardState; },
    subscribeProtein(subId, protein, handler) {
      window.__subscriptions.push({ kind: "protein", subId, protein });
      handler({ rows: [
        { uid: "r_need_1", kind: "plain", slug: "exercise", head: "Exercise", body: "", quantity: -1 },
        { uid: "r_need_2", kind: "plain", slug: "breakfast", head: "Breakfast", body: "", quantity: -1 },
      ], live: true });
      return () => { window.__unsubscribed += 1; };
    },
    subscribeSaved(subId, name, handler) {
      window.__subscriptions.push({ kind: "saved", subId, name });
      handler({ rows: [{ uid: "r_saved", kind: "plain", slug: "saved", head: "Saved", quantity: -1 }], live: true });
      return () => { window.__unsubscribed += 1; };
    },
    act(action) { window.__acts.push(action); return Promise.resolve({ ok: true, facts: 1 }); },
  };
</script>
<script src="todo.js"></script>
<script>
  const list = document.getElementById("todo-list-panel");
  list.dispatchEvent(new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true }));
  window.__cardState = { savedProtein: "views.focus" };
  document.dispatchEvent(new CustomEvent("lince-bridge-state", { detail: {} }));
  setTimeout(() => {
    const first = window.__subscriptions[0] || {};
    const saved = window.__subscriptions.find((entry) => entry.kind === "saved") || {};
    const rendered = Array.from(document.querySelectorAll(".todoItemTitle")).map((node) => node.textContent.trim()).join(",");
    document.title = "ROWS=" + document.querySelectorAll(".todoItem").length
      + " SOURCE=" + first.protein?.source
      + " WHERE=" + JSON.stringify(first.protein?.where || [])
      + " ORDER=" + JSON.stringify(first.protein?.order || [])
      + " SAVED=" + (saved.name || "")
      + " UNSUB=" + window.__unsubscribed
      + " RENDERED=" + rendered
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
grep -q "SOURCE=record" <<<"$TITLE" || { echo "FAIL: expected default subscription to source=record"; fail=1; }
grep -q '"quantity_lt":0' <<<"$TITLE" || { echo "FAIL: expected focus queue quantity_lt predicate"; fail=1; }
grep -q '"kind_eq":"plain"' <<<"$TITLE" || { echo "FAIL: expected focus queue kind_eq predicate"; fail=1; }
grep -q '"topo":"before"' <<<"$TITLE" || { echo "FAIL: expected focus queue topo order"; fail=1; }
grep -q '"action":"set-quantity","target":"r_need_1","value":0' <<<"$TITLE" || { echo "FAIL: Space did not issue set-quantity for the active item"; fail=1; }
grep -q "SAVED=views.focus" <<<"$TITLE" || { echo "FAIL: saved Protein card state did not re-subscribe via subscribeSaved"; fail=1; }
grep -q "UNSUB=1" <<<"$TITLE" || { echo "FAIL: expected old Protein subscription to be unsubscribed on swap"; fail=1; }
grep -q "ROWS=1" <<<"$TITLE" || { echo "FAIL: expected saved Protein rows to render after swap"; fail=1; }
grep -q "RENDERED=Saved" <<<"$TITLE" || { echo "FAIL: expected saved Protein row title to render"; fail=1; }

[ "$fail" -eq 0 ] && echo "PASS: todo sand snapshot + focus Protein + Action round-trip + driving-Protein swap" || exit 1
