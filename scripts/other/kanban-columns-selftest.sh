#!/usr/bin/env bash
# K3 — the column system: the real served `kanban.html` + real `frame.js` in an
# iframe, real unified bridge; only the transport WebSocket is stubbed (acts
# auto-ACKed). Proves:
#   - Columns sheet: add / rename / delete / reorder columns, saved as host
#     state (cardState.kanban.lanesDef)
#   - RANGE columns (3..10): rows bucket into the range; moving a card in
#     writes the range's representative (first) value via set-quantity
#   - CONCEPT columns (@food): rows bucket by concept; moving writes set-concept
#   - concept UIDs resolve to NAMES (source:"concept" subscription) on chips
#   - PRESETS: built-ins apply in one tap; "save current as preset" writes a
#     sand-configuration record (create-record kind=sand + set-extension
#     namespace kanban.columns); preset records list, apply, and delete
#     (deactivate); the presets subscription filters kind_eq=sand
#   - hide a column / hide empty columns
#   - multi-select -> bulk move (bulk delete lives in kanban-sand-selftest.sh,
#     reinstated 2026-07-17 as delete-record behind a confirm modal)
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/kanban-columns-selftest.sh
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

awk -v framefile="$BOARD/frame.js" '
  index($0, "<script src=\"/board/frame.js\"></script>") {
    print "<script>"
    while ((getline line < framefile) > 0) print line
    close(framefile)
    print "</script>"
    next
  }
  { print }
' "$SAND/kanban/kanban.html" > "$WORK/kanban-frame.html"
grep -q "LinceWidgetHost" "$WORK/kanban-frame.html" || { echo "frame.js inline failed"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="status"></div>
<script src="bundle.js"></script>
<script>
  window.__sent = [];
  class FakeWS {
    constructor(url) { this.url = url; this.readyState = 0; window.__ws = this;
      this._open = [];
      setTimeout(() => { this.readyState = 1; this._open.forEach((f) => f()); }, 0); }
    addEventListener(t, f) { if (t === "open") this._open.push(f);
      else if (t === "message") this._msg = f; else if (t === "close") this._close = f;
      else if (t === "error") this._err = f; }
    send(raw) {
      const msg = JSON.parse(raw);
      window.__sent.push(msg);
      if (msg.type === "act") {
        setTimeout(() => window.__inbound(
          { type: "action_ok", id: msg.id, created: "r_created", facts: [] }), 0);
      }
    }
    close() { this.readyState = 3; }
  }
  FakeWS.CONNECTING=0; FakeWS.OPEN=1; FakeWS.CLOSING=2; FakeWS.CLOSED=3;
  window.WebSocket = FakeWS;
  window.__inbound = (obj) => window.__ws._msg({ data: JSON.stringify(obj) });

  const patches = [];
  // A kanban card ships with NO driving Protein by default (2026-07-18) — it
  // subscribes to nothing until the Data panel picks one. Seed the same
  // shape the sand's own (now-removed) DEFAULT_PROTEIN used to auto-apply so
  // this test still exercises the real subscribe/render path.
  const KANBAN_TEST_PROTEIN = {
    source: "record",
    where: [{ kind_eq: "plain" }],
    order: [{ asc: "quantity" }, { asc: "created_at" }],
    include: { links: { kinds: ["assigned-to", "part-of"], direction: "out" } },
  };
  const metaStore = { cardState: { protein: KANBAN_TEST_PROTEIN } };

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
  const acts = () => window.__sent.filter((m) => m.type === "act").map((m) => m.action);
  const colTitles = () => [...doc().querySelectorAll(".col h2")].map((h) => h.textContent);
  const laneOf = (title) => {
    const el = [...doc().querySelectorAll(".card")]
      .find((c) => (c.querySelector(".card-title") || {}).textContent === title);
    return el ? el.closest(".cards").dataset.lane : null;
  };
  const colRows = () => [...doc().querySelectorAll("#columns-list .col-row")];
  const setInput = (input, v) => { input.value = v; input.dispatchEvent(new Event("input", { bubbles: true })); };

  (async () => {
    const results = {};
    // Progressive: a mid-flight failure still reports how far we got.
    const mark = () => { document.title = "RESULT=" + JSON.stringify(results); };
    try {
    await wait(400);
    // The bridge's own initial render() fires before the iframe finishes
    // loading (postMessage to a still-loading frame is lost) — re-push the
    // seeded Protein now that it's up.
    bridge.syncFrames();
    await wait(150);

    // Feed the driving snapshot + concepts + one saved preset record.
    window.__inbound({ type: "snapshot", id: "card-kanban:kanban", rows: [
      { uid: "r_a", head: "Alpha", slug: "alpha", body: "", quantity: 0, concept: null },
      { uid: "r_b", head: "Beta",  slug: "beta",  body: "", quantity: 5, concept: "c_food" },
      { uid: "r_c", head: "Gamma", slug: "gamma", body: "", quantity: -1, concept: null },
    ]});
    window.__inbound({ type: "snapshot", id: "card-kanban:kanban-concepts", rows: [
      { uid: "c_food", name: "food" },
    ]});
    window.__inbound({ type: "snapshot", id: "card-kanban:kanban-presets", rows: [
      { uid: "r_preset", kind: "sand", head: "My preset", quantity: 1,
        extension: { columns: [
          { key: "cold", label: "Cold", value: 0 },
          { key: "hot", label: "Hot", value: 1 },
        ] } },
    ]});
    await wait(200);

    // 0. The presets subscription targets sand-configuration records.
    const presetSub = window.__sent.find((m) => m.type === "subscribe"
      && String(m.id || "") === "card-kanban:kanban-presets");
    results.preset_sub = !!presetSub
      && JSON.stringify(presetSub.protein.where || []).includes('"kind_eq":"sand"')
      && JSON.stringify(presetSub.protein.include || {}).includes('"kanban.columns"');

    // 1. Concept names resolve on card chips (row carries the uid c_food).
    results.concept_chip = [...doc().querySelectorAll(".concept-chip")]
      .some((c) => c.textContent === "@food");

    // 2. Columns sheet opens with the current lanes editable.
    doc().getElementById("open-columns").click();
    await wait(100);
    results.sheet_open = doc().getElementById("sheet-columns").classList.contains("open");
    results.sheet_rows = colRows().length === 5;

    // 3. Rename Backlog -> Icebox, delete Review, add a range column 3..10,
    //    move Done up one, hide Next. Save -> host state lanesDef.
    setInput(colRows()[0].querySelector(".col-label"), "Icebox");
    colRows()[3].querySelector('[title="Delete column"]').click();
    await wait(50);
    doc().getElementById("column-add").click();
    await wait(50);
    setInput(colRows()[4].querySelector(".col-label"), "Pile");
    setInput(colRows()[4].querySelector(".col-value"), "3..10");
    colRows()[3].querySelector('[title="Move up"]').click();
    await wait(50);
    colRows()[1].querySelector('input[type="checkbox"]').click();
    doc().getElementById("columns-save").click();
    await wait(200);
    const titles = colTitles();
    results.columns_saved = JSON.stringify(titles)
      === JSON.stringify(["Icebox", "Done", "WIP", "Pile"]); // Next hidden, Review gone, Done moved up
    results.lanesdef_persisted = patches.some((p) =>
      JSON.stringify(p.patch?.kanban?.lanesDef || []).includes('"Pile"'));

    // 4. Range bucketing: Beta (quantity 5) sits in Pile (3..10). Moving Alpha
    //    into Pile writes the representative value 3.
    results.range_buckets = laneOf("Beta") === "pile";
    window.__sent.length = 0;
    const dt = new DataTransfer();
    dt.setData("text/lince-record", "r_a");
    doc().querySelector('.cards[data-lane="pile"]')
      .dispatchEvent(new DragEvent("drop", { dataTransfer: dt, bubbles: true }));
    await wait(120);
    results.range_move = acts().some((a) => a && a.action === "set-quantity"
      && a.target === "r_a" && a.value === 3)
      && laneOf("Alpha") === "pile";

    // 5. Concept column: swap in a preset-less concept lane via the sheet, then
    //    moving Gamma into it writes set-concept.
    doc().getElementById("open-columns").click();
    await wait(50);
    setInput(colRows()[0].querySelector(".col-value"), "@food");
    doc().getElementById("columns-save").click();
    await wait(150);
    // Rename keeps the lane KEY stable ("backlog") — only the label and the
    // bucket rule changed. c_food matches @food and beats Pile (first lane wins).
    results.concept_buckets = laneOf("Beta") === "backlog";
    window.__sent.length = 0;
    const dt2 = new DataTransfer();
    dt2.setData("text/lince-record", "r_c");
    doc().querySelector('.cards[data-lane="backlog"]')
      .dispatchEvent(new DragEvent("drop", { dataTransfer: dt2, bubbles: true }));
    await wait(120);
    results.concept_move = acts().some((a) => a && a.action === "set-concept"
      && a.target === "r_c" && a.concept === "food");

    // 6. Built-in preset applies in one tap.
    doc().getElementById("open-columns").click();
    await wait(50);
    doc().querySelector('[data-apply-preset="Todo / WIP / Done"]').click();
    await wait(150);
    results.builtin_preset = JSON.stringify(colTitles()) === JSON.stringify(["Todo", "WIP", "Done"]);

    // 7. Saved preset record: listed, applies, deletes via deactivate.
    results.preset_listed = [...doc().querySelectorAll(".preset-name")]
      .some((n) => n.textContent === "My preset");
    doc().querySelector('[data-apply-preset="My preset"]').click();
    await wait(150);
    results.preset_applies = JSON.stringify(colTitles()) === JSON.stringify(["Cold", "Hot"]);
    window.__sent.length = 0;
    doc().querySelector('[data-delete-preset="r_preset"]').click();
    await wait(120);
    results.preset_delete = acts().some((a) => a && a.action === "deactivate"
      && a.target === "r_preset");

    // 8. Save current as preset -> create-record kind=sand + set-extension.
    window.__sent.length = 0;
    doc().getElementById("preset-name").value = "Mine";
    doc().getElementById("preset-save").click();
    await wait(250);
    results.preset_create = acts().some((a) => a && a.action === "create-record"
      && a.kind === "sand" && a.head === "Mine");
    results.preset_extension = acts().some((a) => a && a.action === "set-extension"
      && a.target === "r_created" && a.namespace === "kanban.columns"
      && JSON.stringify(a.fds || {}).includes('"columns"'));

    // 9. Hide empty columns: Cold holds every card? Move all to Hot first —
    //    bulk path: select Alpha+Beta+Gamma via ctrl-click, bulk move to Hot.
    doc().getElementById("sheet-columns")
      .querySelector('[data-close="sheet-columns"]').click();
    await wait(50);
    for (const t of ["Alpha", "Beta", "Gamma"]) {
      const el = [...doc().querySelectorAll(".card")]
        .find((c) => (c.querySelector(".card-title") || {}).textContent === t);
      el.dispatchEvent(new MouseEvent("click", { ctrlKey: true, bubbles: true }));
      await wait(30);
    }
    results.bulk_bar = !doc().getElementById("bulk-bar").hidden
      && doc().getElementById("bulk-count").textContent === "3 selected";
    window.__sent.length = 0;
    doc().getElementById("bulk-lane").value = "hot";
    doc().getElementById("bulk-move").click();
    await wait(150);
    const setQs = acts().filter((a) => a && a.action === "set-quantity" && a.value === 1);
    results.bulk_move = setQs.length >= 2 && laneOf("Alpha") === "hot" && laneOf("Gamma") === "hot";

    doc().getElementById("open-columns").click();
    await wait(50);
    doc().getElementById("hide-empty").click();
    doc().getElementById("columns-save").click();
    await wait(150);
    results.hide_empty = JSON.stringify(colTitles()) === JSON.stringify(["Hot"]);
    } catch (err) {
      results.error = String(err && err.message ? err.message : err);
    }
    mark();
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=12000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check preset_sub        "presets subscription is not kind_eq=sand with the kanban.columns extension include"
check concept_chip      "concept uid did not resolve to its name on the card chip"
check sheet_open        "Columns button did not open the columns sheet"
check sheet_rows        "columns sheet did not list the current 5 lanes"
check columns_saved     "rename/delete/add/reorder/hide did not produce the expected columns"
check lanesdef_persisted "column config was not persisted via patch-card-state (lanesDef)"
check range_buckets     "a row inside 3..10 did not bucket into the range column"
check range_move        "moving into a range column did not write its representative value"
check concept_buckets   "a row did not bucket into the @concept column"
check concept_move      "moving into a concept column did not write set-concept"
check builtin_preset    "built-in preset did not apply"
check preset_listed     "saved preset record is not listed"
check preset_applies    "saved preset record did not apply"
check preset_delete     "preset delete did not deactivate the record"
check preset_create     "save-as-preset did not create-record kind=sand"
check preset_extension  "save-as-preset did not write the kanban.columns extension"
check bulk_bar          "ctrl-click selection did not show the bulk bar with a count"
check bulk_move         "bulk move did not move the selected cards"
check hide_empty        "hide-empty did not hide the empty columns"

[ "$fail" -eq 0 ] && echo "PASS: K3 column system (config CRUD, ranges, @concept columns, presets as sand-config records, concept names, hide, bulk move)" || exit 1
