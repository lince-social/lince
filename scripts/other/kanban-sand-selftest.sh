#!/usr/bin/env bash
# Port verification — the OLD kanban UX adapted to Protein/Actions/frame.js:
#   - the real served `kanban.html` with the real `frame.js` inlined in an iframe
#   - the real unified `widget-bridge.js` + shared `transport.js`
#   - only the transport WebSocket is stubbed (acts are auto-ACKed)
# Proves the ported board-level features: default driving Protein -> "New task"
# delegates creation to the grouped Record (`recordCreate` lane event, no
# kanban create sheet) -> checkbox lines in card bodies render as REAL
# checkboxes and toggling writes the flipped body (`edit-record-text`,
# optimistic, not a recordClicked) -> optimistic drag/drop move
# (`set-quantity`, card moves BEFORE the ack) -> lane collapse persisted as
# host state -> Data-panel saved-Protein swap (`subscribe_saved`).
# Surfaces gone BY DECISION: view/filter chrome (Protein control is base UI —
# the Data panel) and the full edit sheet (head/metadata/assignees/resources
# stay the grouped Record's job). Revised 2026-07-17: the kanban DOES
# perform one more record write beyond checkbox toggles/body edits/column
# moves — bulk delete of selected cards (`delete-record`, confirmed via an
# in-sand modal), covered by its own block below.
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/kanban-sand-selftest.sh
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

awk -v framefile="$BOARD/frame.js" -v editorfile="$BOARD/editor.js" '
  index($0, "<script src=\"/board/frame.js\"></script>") {
    print "<script>"
    while ((getline line < framefile) > 0) print line
    close(framefile)
    print "</script>"
    next
  }
  index($0, "<script src=\"/board/editor.js\"></script>") {
    print "<script>"
    while ((getline line < editorfile) > 0) print line
    close(editorfile)
    print "</script>"
    next
  }
  { print }
' "$SAND/kanban/kanban.html" > "$WORK/kanban-frame.html"
grep -q "LinceWidgetHost" "$WORK/kanban-frame.html" || { echo "frame.js inline failed"; exit 1; }
grep -q "LinceBodyEditor" "$WORK/kanban-frame.html" || { echo "editor.js inline failed"; exit 1; }

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
      // Auto-ACK every Action so sands awaiting H.act(...) proceed.
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
  const cardByTitle = (t) => [...doc().querySelectorAll(".card")]
    .find((c) => (c.querySelector(".card-title") || {}).textContent === t);

  (async () => {
    const results = {};
    await wait(400);
    // The bridge's own initial render() fires before the iframe finishes
    // loading (postMessage to a still-loading frame is lost) — re-push the
    // seeded Protein now that it's up.
    bridge.syncFrames();
    await wait(150);

    // 1. The Data-panel-configured driving Protein reaches the sand (there is
    //    no auto-applied default anymore, 2026-07-18 — see KANBAN_TEST_PROTEIN).
    const sub = window.__sent.find((m) => m.type === "subscribe"
      && String(m.id || "") === "card-kanban:kanban");
    results.configured_protein_subscribed = !!sub && sub.protein
      && sub.protein.source === "record"
      && JSON.stringify(sub.protein.where || []).includes('"kind_eq":"plain"');

    window.__inbound({ type: "snapshot", id: "card-kanban:kanban", rows: [
      { uid: "r_a", head: "Alpha", slug: "alpha",
        body: "- [ ] buy milk\n- [x] call bob\nplain note", quantity: 0,
        links: [
          { uid: "l1", kind: "assigned-to", direction: "out", other: "p_ana", hop: 1 },
          { uid: "l2", kind: "part-of", direction: "out", other: "r_b", hop: 1 },
        ] },
      { uid: "r_b", head: "Beta",  slug: "beta",  body: "bbb", quantity: -1 },
      { uid: "r_c", head: "Pic",   slug: "pic",
        body: "![](https://x.test/shot.png)\nsee @beta", quantity: -1 },
    ]});
    window.__inbound({ type: "snapshot", id: "card-kanban:kanban-people", rows: [
      { uid: "p_ana", head: "Ana", slug: "ana", kind: "person" },
    ]});
    await wait(150);

    // 1b. Default Protein rides links along; cards badge the assignee (K4/K5).
    results.links_included = !!sub && JSON.stringify(sub.protein.include || {})
      .includes('"assigned-to"');
    results.assignee_badge = [...doc().querySelectorAll(".assignee-chip")]
      .some((b) => b.textContent === "Ana");
    // Parent context is a board OPTION (Columns sheet), off by default.
    results.parent_ctx_off = !doc().querySelector(".parent-ctx");
    doc().getElementById("open-columns").click();
    await wait(80);
    doc().getElementById("show-parent").click();
    doc().getElementById("columns-save").click();
    await wait(150);
    results.parent_ctx_on = [...doc().querySelectorAll(".parent-ctx")]
      .some((c) => c.textContent.includes("Beta"));

    // 2. "New task" emits the group-scoped recordCreate event (creation lives
    //    in Record, not a kanban sheet); the lane also mirrors to the
    //    server for other sessions.
    doc().getElementById("open-create").click();
    await wait(150);
    results.newtask_emits = window.__sent.some((m) => m.type === "lane_send"
      && m.room === "recordCreate");
    results.no_create_sheet = !doc().getElementById("record-submit")
      || !doc().getElementById("sheet-record").classList.contains("open");

    // 3. Checkbox part of K2: `- [ ]` lines are real checkboxes; the second
    //    (done) renders checked. Toggling the first flips ONLY that line and
    //    writes the whole body via edit-record-text — optimistically (the box
    //    is checked before the ack) and WITHOUT emitting recordClicked.
    // scoped to .card-body: the card also carries a bulk-select checkbox
    // (input[type=checkbox] too, but in .card-actions, not the rendered body)
    const boxes = () => [...cardByTitle("Alpha").querySelectorAll('.card-body input[type="checkbox"]')];
    results.checkbox_renders = boxes().length === 2
      && boxes()[0].checked === false && boxes()[1].checked === true;
    window.__sent.length = 0;
    boxes()[0].click();
    results.checkbox_optimistic = boxes()[0].checked === true;
    await wait(200);
    results.checkbox_action = acts().some((a) => a && a.action === "edit-record-text"
      && a.target === "r_a" && a.head === "Alpha"
      && a.body === "- [x] buy milk\n- [x] call bob\nplain note");
    results.checkbox_not_click = !window.__sent.some((m) => m.type === "lane_send"
      && m.room === "recordClicked");

    // 3c. Click targets (2026-07-17): the TITLE opens Record; the BODY
    //     edits IN PLACE (textarea + slash palette, Ctrl+Enter saves via
    //     edit-record-text); body click emits NO recordClicked; images and
    //     @chips render on the card through the shared editor.js renderer.
    window.__sent.length = 0;
    cardByTitle("Beta").querySelector(".card-title").click();
    await wait(150);
    results.title_click_focuses = window.__sent.some((m) => m.type === "lane_send"
      && m.room === "recordClicked");
    window.__sent.length = 0;
    cardByTitle("Beta").querySelector(".card-body").click();
    await wait(100);
    const editArea = cardByTitle("Beta").querySelector(".card-body.editing textarea");
    results.body_click_edits = !!editArea && editArea.value === "bbb"
      && !window.__sent.some((m) => m.type === "lane_send" && m.room === "recordClicked");
    editArea.value = "bbb\n- [ ] new step";
    editArea.dispatchEvent(new Event("input", { bubbles: true }));
    editArea.dispatchEvent(new KeyboardEvent("keydown",
      { key: "Enter", ctrlKey: true, bubbles: true, cancelable: true }));
    await wait(200);
    results.body_edit_saves = acts().some((a) => a && a.action === "edit-record-text"
      && a.target === "r_b" && a.body === "bbb\n- [ ] new step");
    results.body_edit_optimistic = !cardByTitle("Beta").querySelector(".card-body.editing")
      && !!cardByTitle("Beta").querySelector('input[data-md-line="1"]');
    const picBody = cardByTitle("Pic").querySelector(".card-body");
    results.body_image_renders = !!picBody.querySelector('img[src="https://x.test/shot.png"]');
    results.body_ref_chip = !!picBody.querySelector('[data-ref="r_b"]');
    // the SAME <img> element, not a same-src replacement — recreating it on
    // an unrelated re-render forces a re-decode that reads as a flicker
    // (2026-07-18: reported when releasing a drag elsewhere on the board)
    const picImgBeforeMove = picBody.querySelector("img");

    // 4. Optimistic drag/drop: Beta (next) -> WIP. The card moves BEFORE any
    //    server update lands, and set-quantity goes out.
    window.__sent.length = 0;
    const dt = new DataTransfer();
    dt.setData("text/lince-record", "r_b");
    const target = doc().querySelector('.cards[data-lane="wip"]');
    target.dispatchEvent(new DragEvent("drop", { dataTransfer: dt, bubbles: true }));
    await wait(120);
    const beta = cardByTitle("Beta");
    results.optimistic_move = !!beta && beta.closest(".cards").dataset.lane === "wip";
    results.move_action = acts().some((a) => a && a.action === "set-quantity"
      && a.target === "r_b" && a.value === -2);
    const picImgAfterMove = cardByTitle("Pic").querySelector(".card-body img");
    results.image_node_stable_across_unrelated_move =
      !!picImgBeforeMove && picImgBeforeMove === picImgAfterMove;

    // 5. The full edit sheet is GONE: head/metadata/assignees/resources still
    //    edit in the grouped Record, not a kanban sheet.
    results.no_edit_surface = !doc().getElementById("sheet-record")
      && !doc().querySelector("[data-edit-record]");

    // 5b. Bulk delete (2026-07-17 reversal of the "kanban never deletes" call):
    //     select via the per-card checkbox, confirm via the in-sand modal, a
    //     HARD delete-record goes out (NOT deactivate — this query has no
    //     quantity filter, so a soft zero-out would leave the card sitting on
    //     the board), and the card actually disappears from the DOM.
    window.__sent.length = 0;
    cardByTitle("Alpha").querySelector("input[data-select-record]").click();
    await wait(60);
    doc().getElementById("bulk-delete").click();
    await wait(80);
    results.delete_confirm_opens = doc().getElementById("sheet-confirm").classList.contains("open");
    doc().getElementById("confirm-ok").click();
    await wait(200);
    results.delete_action = acts().some((a) => a && a.action === "delete-record" && a.target === "r_a");
    results.delete_removes_card = !cardByTitle("Alpha");

    // 6. View/filter chrome is gone: Protein control is base UI (Data panel).
    results.no_protein_chrome = !doc().getElementById("open-filter")
      && !doc().getElementById("open-view")
      && !doc().getElementById("search");

    // 7. Lane collapse persists as host state (cardState.kanban.lanes).
    doc().querySelector('[data-lane-toggle="backlog"]').click();
    await wait(150);
    const backlogCol = doc().querySelector('.cards[data-lane="backlog"]');
    results.lane_collapsed = !backlogCol || backlogCol.closest(".col").classList.contains("is-collapsed");
    results.collapse_persisted = patches.some((p) => p.patch?.kanban?.lanes?.backlog?.collapsed === true);

    // 8. Data-panel swap to a saved Protein -> subscribe_saved.
    metaStore.cardState = Object.assign({}, metaStore.cardState, { savedProtein: "views.stock" });
    bridge.syncFrames();
    await wait(200);
    results.saved_swap = window.__sent.some((m) => m.type === "subscribe_saved"
      && String(m.name || "") === "views.stock");

    document.title = "RESULT=" + JSON.stringify(results);
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=9000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check configured_protein_subscribed "the configured driving Protein is not source=record kind_eq=plain"
check links_included      "default Protein does not include assigned-to/part-of links"
check assignee_badge      "assignee did not badge on the card"
check parent_ctx_off      "parent context wrongly shows before the board option is on"
check parent_ctx_on       "the show-parent board option did not render parent context"
check newtask_emits       "New task did not emit the recordCreate lane event"
check no_create_sheet     "New task wrongly opened a kanban create sheet"
check checkbox_renders    "checkbox lines did not render as checkboxes (with [x] checked)"
check checkbox_optimistic "checkbox did not toggle before the server ack"
check checkbox_action     "checkbox toggle did not write the flipped body via edit-record-text"
check checkbox_not_click  "checkbox toggle wrongly emitted recordClicked"
check title_click_focuses "clicking the TITLE did not emit recordClicked"
check body_click_edits    "clicking the BODY did not open in-place editing (or leaked recordClicked)"
check body_edit_saves     "Ctrl+Enter did not save the edited body via edit-record-text"
check body_edit_optimistic "the edited body did not render optimistically (new checkbox line)"
check body_image_renders  "![](url) in a body did not render as an image on the card"
check body_ref_chip       "@slug in a body did not render as a reference chip on the card"
check image_node_stable_across_unrelated_move "an unrelated card's move recreated (flickered) another card's <img>"
check optimistic_move     "dropped card did not move before the server ack"
check move_action         "drop did not send set-quantity"
check no_edit_surface     "a full edit surface is still in the kanban (Record's job)"
check delete_confirm_opens "bulk-delete did not open the confirm modal"
check delete_action        "confirming bulk-delete did not send delete-record"
check delete_removes_card  "the deleted card did not disappear from the board"
check no_protein_chrome   "view/filter/search chrome is still in the sand (base-UI duty)"
check lane_collapsed      "lane toggle did not collapse the column"
check collapse_persisted  "lane collapse was not persisted via patch-card-state"
check saved_swap          "cardState savedProtein did not re-subscribe via subscribe_saved"

[ "$fail" -eq 0 ] && echo "PASS: old-kanban UX on Protein/Actions (recordCreate delegation, checkbox toggles, optimistic move, lanes, saved swap, bulk delete; no full edit surface)" || exit 1
