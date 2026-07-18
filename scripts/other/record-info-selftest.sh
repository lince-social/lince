#!/usr/bin/env bash
# K4+K5 — record_info IS the editing surface: the real served record_info.html
# with the real frame.js inlined in an iframe, real unified bridge; only the
# transport WebSocket is stubbed (acts auto-ACK, optionally auto-FAIL for the
# offline-queue proof). Proves:
#   - the get view is the edit view: head/slug/quantity/body writable, Save
#     writes only what changed; live updates never clobber a dirty form
#   - Zero vs Delete are DIFFERENT: Zero -> deactivate, Delete -> delete-record
#     (HARD); an empty snapshot afterwards shows the deleted placeholder
#   - work metadata on the `work` record_extension: dates/estimate prefill,
#     worklog Play/Pause -> set-extension, failed writes queue in localStorage
#     and FLUSH on reconnect (the old sand's offline pending-stop queue)
#   - assignees: person picker -> add-link assigned-to; chip x -> remove-link
#   - parent: set replaces the part-of link; children render as nav chips
#   - resources: a URL mints a resource record + resource-of link
#   - comments: post -> create-message on the comments thread; @slug becomes a
#     references link; message bodies render @refs as hops + inline images
#   - the dot has an "update in flight" state while an act is pending
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/record-info-selftest.sh
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
' "$SAND/record_info/record_info.html" > "$WORK/recinfo-frame.html"
grep -q "LinceWidgetHost" "$WORK/recinfo-frame.html" || { echo "frame.js inline failed"; exit 1; }
grep -q "LinceBodyEditor" "$WORK/recinfo-frame.html" || { echo "editor.js inline failed"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="status"></div>
<script src="bundle.js"></script>
<script>
  window.__sent = [];
  window.__failActs = false;
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
        const reply = window.__failActs
          ? { type: "error", id: msg.id, message: "offline" }
          : { type: "action_ok", id: msg.id, created: "r_new", facts: [] };
        setTimeout(() => window.__inbound(reply), 0);
      }
    }
    close() { this.readyState = 3; }
  }
  FakeWS.CONNECTING=0; FakeWS.OPEN=1; FakeWS.CLOSING=2; FakeWS.CLOSED=3;
  window.WebSocket = FakeWS;
  window.__inbound = (obj) => window.__ws._msg({ data: JSON.stringify(obj) });

  const frame = document.createElement("iframe");
  frame.className = "package-widget__frame";
  frame.dataset.packageInstanceId = "card-recinfo";
  frame.src = "recinfo-frame.html";
  frame.style.cssText = "width:420px;height:900px";
  document.body.appendChild(frame);

  createWidgetBridge({
    statusNode: document.getElementById("status"),
    getFrames: () => [frame],
    initialState: {},
    getCardMeta: () => ({}),
    getCardAbiListen: () => ["recordClicked", "recordCreate"],
    getCardGroupStack: () => [],
    setCardState: () => {}, patchCardState: () => {}, setCardStreamsEnabled: () => {},
    handleShellAction: () => {}, invalidateServerAuth: () => {}, onError: () => {},
  });

  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const doc = () => frame.contentDocument;
  const acts = () => window.__sent.filter((m) => m.type === "act").map((m) => m.action);
  const focusRow = {
    uid: "r_t", head: "Task", slug: "task", kind: "plain", body: "body", quantity: -1,
    extension: { start: "2026-07-01", due: "2026-07-30", estimate_min: 60,
                 logs: [{ start: "2026-07-17T10:00:00Z", end: "2026-07-17T10:30:00Z" }] },
    links: [
      { uid: "l1", kind: "assigned-to", direction: "out", other: "p_ana", hop: 1 },
      { uid: "l2", kind: "part-of", direction: "out", other: "r_parent", hop: 1 },
      { uid: "l3", kind: "part-of", direction: "in", other: "r_child", hop: 1 },
      { uid: "l4", kind: "resource-of", direction: "in", other: "r_res", hop: 1 },
    ],
    threads: [{ uid: "t1", head: "comments", quantity: 1, messages: [
      { uid: "m1", head: "", body: "see @task.two and https://x.test/pic.png", quantity: 1 },
    ] }],
    facts: [{ delta: 0, cause_kind: "user_edit", at: "2026-07-17T10:00:00Z" }],
  };

  (async () => {
    const results = {};
    const mark = () => { document.title = "RESULT=" + JSON.stringify(results); };
    try {
    await wait(400);

    // Feed the names side-subscription, then focus r_t via the lane event.
    window.__inbound({ type: "snapshot", id: "card-recinfo:names", rows: [
      { uid: "r_t", head: "Task", slug: "task", kind: "plain" },
      { uid: "p_ana", head: "Ana", slug: "ana", kind: "person" },
      { uid: "r_parent", head: "Big project", slug: "big", kind: "plain" },
      { uid: "r_child", head: "Subtask", slug: "sub", kind: "plain" },
      { uid: "r_res", head: "Spec doc", slug: "spec", kind: "plain" },
      { uid: "r_two", head: "Task two", slug: "task.two", kind: "plain" },
    ]});
    frame.contentWindow.postMessage({ type: "lince:lane-event", room: "recordClicked",
      payload: { record: { uid: "r_t" } } }, "*");
    await wait(200);
    window.__inbound({ type: "snapshot", id: "card-recinfo:provenance", rows: [focusRow] });
    await wait(200);

    // 0. The provenance subscribe is WIRE-VALID (the live K0 caught a bad
    //    LinkDirection once — stubbed sockets don't validate).
    const provSub = window.__sent.find((m) => m.type === "subscribe"
      && String(m.id || "") === "card-recinfo:provenance");
    results.sub_wire_valid = !!provSub
      && JSON.stringify(provSub.protein.include || {}).includes('"direction":"both"');

    // 1. The get view is the edit view: fields prefilled and writable.
    results.fields_prefilled = doc().getElementById("f-head").value === "Task"
      && doc().getElementById("f-slug").value === "task"
      && doc().getElementById("f-quantity").value === "-1"
      && doc().getElementById("f-body").value === "body";
    results.work_prefilled = doc().getElementById("w-start").value === "2026-07-01"
      && doc().getElementById("w-estimate").value === "60"
      && doc().getElementById("w-total").textContent.includes("30min");
    results.links_render = !!doc().querySelector('[data-assignee="p_ana"]')
      && !!doc().querySelector('[data-parent="r_parent"]')
      && !!doc().querySelector('[data-child="r_child"]')
      && !!doc().querySelector('[data-resource="r_res"]');
    const msg = doc().querySelector('[data-message-uid="m1"]');
    results.comment_render = !!msg && !!msg.querySelector('[data-ref="r_two"]')
      && !!msg.querySelector('img[src="https://x.test/pic.png"]');
    mark();

    // 2. Zero vs Delete are two different actions; the dot goes busy in flight.
    window.__sent.length = 0;
    doc().getElementById("btn-zero").click();
    results.dot_busy = doc().getElementById("dot").classList.contains("busy");
    await wait(150);
    results.zero_action = acts().some((a) => a && a.action === "deactivate" && a.target === "r_t");
    results.dot_settled = !doc().getElementById("dot").classList.contains("busy");
    mark();

    // 3. Dirty edits survive live updates; Save writes only what changed.
    const fHead = doc().getElementById("f-head");
    fHead.value = "Task v2";
    fHead.dispatchEvent(new Event("input", { bubbles: true }));
    window.__inbound({ type: "snapshot", id: "card-recinfo:provenance",
      rows: [{ ...focusRow, head: "Renamed elsewhere" }] });
    await wait(150);
    results.dirty_guard = doc().getElementById("f-head").value === "Task v2";
    window.__sent.length = 0;
    doc().getElementById("f-save").click();
    await wait(200);
    results.save_edit = acts().some((a) => a && a.action === "edit-record-text"
      && a.target === "r_t" && a.head === "Task v2");
    results.save_narrow = !acts().some((a) => a && (a.action === "set-slug" || a.action === "set-quantity"));
    mark();

    // 3b. K6 in place: the saved body renders as BLOCKS in the preview; a
    //     preview checkbox on a clean form writes immediately; "/" in the
    //     body textarea opens the slash palette; a new @slug in the body
    //     becomes a `references` link on Save.
    window.__inbound({ type: "snapshot", id: "card-recinfo:provenance",
      rows: [{ ...focusRow, body: "## Plan\n- [ ] step one\nsee @task.two" }] });
    await wait(150);
    const preview = doc().getElementById("f-preview");
    results.preview_blocks = !preview.hidden
      && !!preview.querySelector(".md-h2")
      && !!preview.querySelector('input[data-md-line="1"]')
      && !!preview.querySelector('[data-ref="r_two"]');
    window.__sent.length = 0;
    preview.querySelector('input[data-md-line="1"]').click();
    await wait(150);
    results.preview_toggle = acts().some((a) => a && a.action === "edit-record-text"
      && a.target === "r_t" && a.body === "## Plan\n- [x] step one\nsee @task.two");
    const fBody = doc().getElementById("f-body");
    fBody.value = "/h3";
    fBody.selectionStart = fBody.selectionEnd = 3;
    fBody.dispatchEvent(new Event("input", { bubbles: true }));
    await wait(50);
    const pal = doc().querySelector(".lince-editor-palette");
    results.editor_palette = !!pal && pal.style.display === "block";
    fBody.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    await wait(50);
    results.editor_insert = fBody.value.startsWith("### ");
    fBody.value = "### ping @task.two";
    fBody.dispatchEvent(new Event("input", { bubbles: true }));
    window.__sent.length = 0;
    doc().getElementById("f-save").click();
    await wait(250);
    results.body_ref_link = acts().some((a) => a && a.action === "add-link"
      && a.from === "r_t" && a.kind === "references" && a.to === "r_two");

    // 4. Worklogs: Play opens a log, Pause closes it (set-extension on `work`).
    window.__sent.length = 0;
    doc().getElementById("w-play").click();
    await wait(150);
    const played = acts().find((a) => a && a.action === "set-extension" && a.namespace === "work");
    results.play_action = !!played && played.fds.logs.length === 2 && played.fds.logs[1].end === null;
    doc().getElementById("w-pause").click();
    await wait(150);
    const paused = acts().filter((a) => a && a.action === "set-extension").pop();
    results.pause_action = !!paused && paused.fds.logs.every((l) => !!l.end);
    mark();

    // 5. Offline: a failed work write queues and flushes on reconnect.
    window.__failActs = true;
    window.__sent.length = 0;
    doc().getElementById("w-play").click();
    await wait(200);
    window.__failActs = false;
    frame.contentWindow.postMessage({ type: "lince:live", live: true }, "*");
    await wait(250);
    const flushed = acts().filter((a) => a && a.action === "set-extension");
    results.offline_flush = flushed.length >= 2
      && flushed[flushed.length - 1].fds.logs.some((l) => !l.end);
    doc().getElementById("w-pause").click(); // tidy: close the reopened log
    await wait(100);
    mark();

    // 6. Assignees: picker -> add-link; chip x -> remove-link.
    window.__sent.length = 0;
    doc().getElementById("a-pick").value = "p_ana";
    doc().getElementById("a-add").click();
    await wait(150);
    results.assign_action = acts().some((a) => a && a.action === "add-link"
      && a.from === "r_t" && a.kind === "assigned-to" && a.to === "p_ana");
    doc().querySelector('[data-assignee="p_ana"] button').click();
    await wait(150);
    results.unassign_action = acts().some((a) => a && a.action === "remove-link"
      && a.from === "r_t" && a.kind === "assigned-to" && a.to === "p_ana");
    mark();

    // 7. Parent: Set replaces the existing part-of link.
    window.__sent.length = 0;
    doc().getElementById("p-input").value = "task.two";
    doc().getElementById("p-set").click();
    await wait(250);
    results.parent_swap = acts().some((a) => a && a.action === "remove-link"
        && a.kind === "part-of" && a.to === "r_parent")
      && acts().some((a) => a && a.action === "add-link"
        && a.kind === "part-of" && a.to === "task.two");
    mark();

    // 8. Resources: a URL mints a resource record and links it resource-of.
    window.__sent.length = 0;
    doc().getElementById("r-input").value = "https://files.test/spec.pdf";
    doc().getElementById("r-add").click();
    await wait(250);
    results.resource_minted = acts().some((a) => a && a.action === "create-record"
        && a.body === "https://files.test/spec.pdf")
      && acts().some((a) => a && a.action === "add-link"
        && a.from === "r_new" && a.kind === "resource-of" && a.to === "r_t");
    mark();

    // 9. Comments: post on the comments thread; @slug -> references link.
    window.__sent.length = 0;
    doc().getElementById("cm-body").value = "done, see @task.two";
    doc().getElementById("cm-post").click();
    await wait(300);
    results.comment_post = acts().some((a) => a && a.action === "create-message"
      && a.thread === "t1" && a.body === "done, see @task.two");
    results.comment_ref_link = acts().some((a) => a && a.action === "add-link"
      && a.from === "r_new" && a.kind === "references" && a.to === "r_two");
    mark();

    // 9b. Threads are a REAL multi-thread system, not one flattened list:
    // "+ New thread" creates a second, named thread; switching tabs re-scopes
    // BOTH which messages render and which thread the compose box posts into.
    results.thread_tab_renders = !!doc().querySelector('[data-thread="t1"]');
    doc().querySelector('[data-new-thread]').click();
    results.thread_new_opens = !doc().getElementById("thread-new").hidden;
    window.__sent.length = 0;
    doc().getElementById("th-name").value = "design";
    doc().getElementById("th-create").click();
    await wait(150);
    results.thread_create_action = acts().some((a) => a && a.action === "create-thread"
      && a.target === "r_t" && a.head === "design");
    // simulate the live re-subscribe push that would follow a real create-thread
    window.__inbound({ type: "snapshot", id: "card-recinfo:provenance", rows: [{
      ...focusRow,
      threads: [...focusRow.threads, { uid: "r_new", head: "design", quantity: 1, messages: [] }],
    }] });
    await wait(150);
    results.thread_second_renders = !!doc().querySelector('[data-thread="r_new"]');
    results.thread_switch_active = doc().querySelector('[data-thread="r_new"]').classList.contains("active");
    window.__sent.length = 0;
    doc().getElementById("cm-body").value = "first note in the new thread";
    doc().getElementById("cm-post").click();
    await wait(200);
    results.thread_scoped_post = acts().some((a) => a && a.action === "create-message"
      && a.thread === "r_new" && a.body === "first note in the new thread");
    // switch back: the original thread's message is showing again, and IT is active
    doc().querySelector('[data-thread="t1"]').click();
    await wait(50);
    results.thread_switch_back = !!doc().querySelector('[data-message-uid="m1"]')
      && doc().querySelector('[data-thread="t1"]').classList.contains("active")
      && !doc().querySelector('[data-thread="r_new"]').classList.contains("active");
    mark();

    // 10. HARD delete, then the record disappears -> deleted placeholder.
    window.__sent.length = 0;
    doc().getElementById("btn-delete").click();
    await wait(150);
    results.delete_action = acts().some((a) => a && a.action === "delete-record" && a.target === "r_t");
    window.__inbound({ type: "snapshot", id: "card-recinfo:provenance", rows: [] });
    await wait(150);
    results.deleted_view = doc().getElementById("focus").hidden
      && !doc().getElementById("empty").hidden
      && doc().getElementById("empty").textContent.includes("deleted");
    } catch (err) {
      results.error = String(err && err.message ? err.message : err);
    }
    mark();
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=15000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check sub_wire_valid    "the provenance subscribe include is not wire-valid (direction both)"
check fields_prefilled  "the get view did not prefill the editable fields"
check work_prefilled    "the work extension did not prefill dates/estimate/total"
check links_render      "assignee/parent/children/resource chips did not render"
check comment_render    "comment did not render the @ref hop and inline image"
check dot_busy          "the dot did not show the update-in-flight state"
check zero_action       "Zero did not send deactivate"
check dot_settled       "the dot did not settle after the ack"
check dirty_guard       "a live update clobbered the dirty form"
check save_edit         "Save did not send edit-record-text with the typed head"
check save_narrow       "Save wrote fields that did not change"
check preview_blocks    "the body preview did not render heading/checkbox/@chip blocks"
check preview_toggle    "a preview checkbox toggle did not write the flipped body"
check editor_palette    "/ in the body textarea did not open the slash palette"
check editor_insert     "the slash palette did not insert the heading markdown"
check body_ref_link     "a body @slug did not become a references link on Save"
check play_action       "Play did not open a worklog via set-extension"
check pause_action      "Pause did not close the open worklog"
check offline_flush     "a failed work write did not queue + flush on reconnect"
check assign_action     "assigning did not add-link assigned-to"
check unassign_action   "chip remove did not remove-link assigned-to"
check parent_swap       "Set parent did not replace the part-of link"
check resource_minted   "a URL resource was not minted + linked resource-of"
check comment_post      "Post did not create-message on the comments thread"
check comment_ref_link  "@slug in a comment did not become a references link"
check thread_tab_renders     "the seeded thread did not render as a tab"
check thread_new_opens       "+ New thread did not open the name input"
check thread_create_action  "Create did not send create-thread with the typed name"
check thread_second_renders "the newly created thread did not render as a second tab"
check thread_switch_active  "the newly created thread did not become the active tab"
check thread_scoped_post    "Post did not scope create-message to the ACTIVE (new) thread"
check thread_switch_back    "switching back to the first thread did not restore its messages + active state"
check delete_action     "Delete did not send delete-record (the HARD delete)"
check deleted_view      "after deletion the view did not show the deleted placeholder"

[ "$fail" -eq 0 ] && echo "PASS: record_info is the editing surface (fields, zero vs HARD delete, work extension + offline queue, assignees, parent, resources, comments)" || exit 1
