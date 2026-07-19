#!/usr/bin/env bash
# COMPOSED end-to-end verification of the Relations group (graph + Record)
# on the new data plane with the REAL pieces, node-free:
#   - the real served sands `relations.html` (d3 force graph) + `Record.html`
#   - the real vendored d3.v7.min.js running the actual force simulation
#   - the real `frame.js` sand host running INSIDE real iframes
#   - the real unified `widget-bridge.js` + shared `transport.js`
#   - wired with `getFrames`/`getCardGroupStack` exactly as `main.js` does
# Only the transport WebSocket is stubbed. This replaces the stale
# `relation-sand-selftest.sh` (which drove the deleted pre-rebuild script.rs).
#
# It proves: the graph renders nodes+edges from a live Protein snapshot (the
# direction:"both" link echo deduped), a node click emits the group-scoped
# `recordClicked` and ONLY the same-group Record focuses, Shift+drag
# node->node sends add-link and a CYCLE WARNING in the ack rides the new
# warnings plumbing (protocol -> bridge -> frame.js) into the sand's status
# line as advice (not an error), edge click + the header chip sends
# remove-link, and "New record" opens creation mode scoped to the group.
# TRAIL MODE: switching mode + picking a root lays the root's `before` tree
# out in topo layers (non-tree nodes hidden), Done is gated on parents-done,
# the promotion cascade sets next steps AUTOMATICALLY (optimistic + ack),
# Undo cascades back, preset CRUD writes kind:"sand" + relations.trail
# extension records that live-sync to a SECOND relations group, and the
# @todo/@next/@wip/@done concept preset writes set-concept (the shared
# cross-sand status vocabulary, same fact as a kanban concept column).
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/relations-group-e2e-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BOARD="$ROOT/crates/web/static/presentation/board"
SAND="$ROOT/crates/web/src/sand"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Bundle transport + unified bridge as one classic script (strip import/export).
sed 's/^export function/function/; /^import /d' "$BOARD/transport.js"       > "$WORK/bundle.js"
sed 's/^export function/function/; /^import /d' "$BOARD/widget-bridge.js"    >> "$WORK/bundle.js"

# Inline a `<script src="..."></script>` tag with a local file's contents
# (file:// cannot resolve the board's absolute URLs). awk does the inlining.
inline_script() {
  local src="$1" tag="$2" file="$3" out="$4"
  awk -v tag="$tag" -v inlinefile="$file" '
    index($0, tag) {
      print "<script>"
      while ((getline line < inlinefile) > 0) print line
      close(inlinefile)
      print "</script>"
      next
    }
    { print }
  ' "$src" > "$out"
}

# relations.html: inline BOTH frame.js and the real vendored d3.
inline_script "$SAND/relations/relations.html" '<script src="/board/frame.js"></script>' \
  "$BOARD/frame.js" "$WORK/relations-step1.html"
inline_script "$WORK/relations-step1.html" '<script src="/board/vendor/d3.v7.min.js"></script>' \
  "$SAND/relations/d3.v7.min.js" "$WORK/relations-frame.html"
grep -q "LinceWidgetHost" "$WORK/relations-frame.html" || { echo "frame.js inline failed"; exit 1; }
grep -q "forceSimulation" "$WORK/relations-frame.html" || { echo "d3 inline failed"; exit 1; }

inline_script "$SAND/record/record.html" '<script src="/board/frame.js"></script>' \
  "$BOARD/frame.js" "$WORK/recinfo-frame.html"
grep -q "LinceWidgetHost" "$WORK/recinfo-frame.html" || { echo "frame.js inline failed for Record"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8">
<style>iframe{width:640px;height:480px;border:0}</style>
</head><body>
<div id="status"></div>
<script src="bundle.js"></script>
<script>
  window.__sent = [];
  window.__wsCount = 0;
  class FakeWS {
    constructor(url) { this.url = url; this.readyState = 0; window.__ws = this;
      window.__wsCount += 1; this._open = [];
      setTimeout(() => { this.readyState = 1; this._open.forEach((f) => f()); }, 0); }
    addEventListener(t, f) { if (t === "open") this._open.push(f);
      else if (t === "message") this._msg = f; else if (t === "close") this._close = f;
      else if (t === "error") this._err = f; }
    send(raw) {
      const msg = JSON.parse(raw);
      window.__sent.push(msg);
      // Auto-ACK Actions so sands awaiting H.act(...) proceed. add-link ACKs
      // carry a CYCLE WARNING (order-like kind closing a loop) to prove the
      // warnings plumbing reaches the sand as advice.
      if (msg.type === "act") {
        const warnings = msg.action && msg.action.action === "add-link"
          ? ["these 2 records form a loop: r_b -> r_a -> r_b"] : [];
        setTimeout(() => window.__inbound(
          { type: "action_ok", id: msg.id, created: "l_new", facts: 1, warnings }), 0);
      }
    }
    close() { this.readyState = 3; }
  }
  FakeWS.CONNECTING=0; FakeWS.OPEN=1; FakeWS.CLOSING=2; FakeWS.CLOSED=3;
  window.WebSocket = FakeWS;
  window.__inbound = (obj) => window.__ws._msg({ data: JSON.stringify(obj) });

  // Four cards: an added relations group + an unrelated Record in a
  // DIFFERENT group (the scoping canary, same as the kanban e2e) + a SECOND
  // relations group (proves trail presets live-share across groups).
  const groups = { "card-relations": ["g1"], "card-recinfo": ["g1"], "card-recinfo-other": ["g2"], "card-relations-b": ["g3"] };
  const abiListen = {
    "card-relations": [],
    "card-recinfo": ["recordClicked", "recordCreate"],
    "card-recinfo-other": ["recordClicked", "recordCreate"],
    "card-relations-b": [],
  };

  function makeIframe(id, file) {
    const f = document.createElement("iframe");
    f.className = "package-widget__frame";
    f.dataset.packageInstanceId = id; // set BEFORE load so frame.js reads it
    f.src = file;
    document.body.appendChild(f);
    return f;
  }
  const relations = makeIframe("card-relations", "relations-frame.html");
  const recinfo = makeIframe("card-recinfo", "recinfo-frame.html");
  const other = makeIframe("card-recinfo-other", "recinfo-frame.html");
  const relationsB = makeIframe("card-relations-b", "relations-frame.html");
  const frames = [relations, recinfo, other, relationsB];

  // A relations card ships with NO driving Protein and NO link kinds by
  // default (2026-07-18) — it subscribes to nothing until the Data panel
  // picks one and shows no edges until the user adds kinds. Both relations
  // cards here carry an explicit inline Protein (what the sand's own
  // defaultProtein() used to auto-apply) plus the kind chrome a user would
  // have set, so this test still exercises the real
  // graph-subscription/rendering path instead of the new empty state.
  const RELATIONS_TEST_PROTEIN = { source: "record", include: { links: { kinds: ["before"], direction: "both", depth: 0 } } };
  const RELATIONS_TEST_PREFS = { kinds: ["before"], linkKind: "before", trailKind: "before" };
  const cardMeta = {
    "card-relations": { cardState: { protein: RELATIONS_TEST_PROTEIN, relations: RELATIONS_TEST_PREFS } },
    "card-relations-b": { cardState: { protein: RELATIONS_TEST_PROTEIN, relations: RELATIONS_TEST_PREFS } },
  };

  const bridge = createWidgetBridge({
    statusNode: document.getElementById("status"),
    getFrames: () => frames,
    initialState: {},
    getCardMeta: (id) => cardMeta[id] || {},
    getCardAbiListen: (id) => abiListen[id] || [],
    getCardGroupStack: (id) => groups[id] || [],
    setCardState: () => {}, patchCardState: () => {}, setCardStreamsEnabled: () => {},
    handleShellAction: () => {}, invalidateServerAuth: () => {}, onError: () => {},
  });

  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const sentSubs = () => window.__sent.filter((m) => m.type === "subscribe");
  const uidEqSubForCard = (cardId, uid) => sentSubs().some((m) =>
    String(m.id || "").startsWith(cardId + ":") &&
    JSON.stringify(m.protein || {}).includes('"uid_eq":"' + uid + '"'));

  // Synthetic canvas pointer tap/drag at CSS coords relative to the canvas.
  function canvasEvent(frame, type, point, opts) {
    const canvas = frame.contentDocument.getElementById("graph");
    const rect = canvas.getBoundingClientRect();
    const event = new frame.contentWindow.PointerEvent(type, Object.assign({
      clientX: rect.left + point.x, clientY: rect.top + point.y,
      button: 0, pointerId: 7, bubbles: true,
    }, opts || {}));
    canvas.dispatchEvent(event);
  }
  function tap(frame, point, opts) {
    canvasEvent(frame, "pointerdown", point, opts);
    canvasEvent(frame, "pointerup", point, opts);
  }
  const widget = () => relations.contentWindow.RelationsWidget;
  const nodeScreen = (uid) => {
    const w = widget();
    const node = w.state.nodes.find((n) => n.id === uid);
    return node ? w.worldToScreen(node.x, node.y) : null;
  };

  (async () => {
    const results = {};
    await wait(400);
    // The bridge's own initial render() fires before the iframes finish
    // loading (postMessage to a still-loading frame is lost) — re-push the
    // per-card Protein config now that they're up, same fix the kanban
    // selftests already use for this exact race.
    bridge.syncFrames();
    await wait(150);

    results.one_socket = window.__wsCount === 1;

    // The graph subscribed its driving Protein with the links include.
    const graphSub = sentSubs().find((m) => String(m.id || "") === "card-relations:graph");
    results.graph_subscribed = !!graphSub
      && JSON.stringify((graphSub.protein.include || {}).links || {}).includes('"before"');

    // Feed a snapshot: two records + one `before` link echoed on BOTH rows
    // (direction "both" — the sand must dedupe it into ONE edge).
    window.__inbound({ type: "snapshot", id: "card-relations:graph", rows: [
      { uid: "r_a", head: "Alpha", slug: "alpha", body: "", quantity: 1,
        links: [{ uid: "l_ab", from: "r_a", to: "r_b", kind: "before", direction: "out", other: "r_b", hop: 1 }] },
      { uid: "r_b", head: "Beta", slug: "beta", body: "", quantity: -1,
        links: [{ uid: "l_ab", from: "r_a", to: "r_b", kind: "before", direction: "in", other: "r_a", hop: 1 }] },
    ] });
    await wait(250);

    results.graph_rendered = widget().state.nodes.length === 2
      && widget().state.links.length === 1;

    // Regression guards (2026-07-17 live bugs):
    // - d3 must actually load and the simulation must exist and heat on drag
    //   (when d3 404'd under /static/vendored the graph had NO physics).
    // - `hidden` must beat the panel/empty overlay display rules (the CSS
    //   display:flex/grid overrode [hidden], so the empty-state covered the
    //   graph and the controls panel never closed).
    results.d3_loaded = typeof relations.contentWindow.d3?.forceSimulation === "function";
    results.simulation_exists = !!widget().state.simulation;
    const emptyEl = relations.contentDocument.getElementById("empty-state");
    results.empty_state_hidden = emptyEl.hidden
      && relations.contentWindow.getComputedStyle(emptyEl).display === "none";
    const panelEl = relations.contentDocument.getElementById("controls-panel");
    relations.contentDocument.getElementById("panel-toggle").click();
    const panelOpened = !panelEl.hidden
      && relations.contentWindow.getComputedStyle(panelEl).display !== "none";
    relations.contentDocument.getElementById("panel-close").click();
    results.panel_closes = panelOpened && panelEl.hidden
      && relations.contentWindow.getComputedStyle(panelEl).display === "none";
    const pointP = nodeScreen("r_a");
    canvasEvent(relations, "pointerdown", pointP);
    results.physics_heat_on_drag = widget().state.simulation
      && widget().state.simulation.alphaTarget() > 0;
    canvasEvent(relations, "pointerup", pointP);
    await wait(100);

    // NODE CLICK -> group-scoped recordClicked: same-group Record focuses
    // the clicked uid, different-group Record does NOT.
    window.__sent.length = 0;
    const pointA = nodeScreen("r_a");
    results.node_positioned = !!pointA;
    if (pointA) tap(relations, pointA);
    await wait(200);
    results.same_group_focused = uidEqSubForCard("card-recinfo", "r_a");
    results.diff_group_not_focused = !uidEqSubForCard("card-recinfo-other", "r_a");

    // SHIFT+DRAG r_b -> r_a: add-link with the active kind, then the ack's
    // cycle warning surfaces in the sand status as ADVICE (warn, not error),
    // and the pending edge is settled by the ack.
    window.__sent.length = 0;
    const pointB = nodeScreen("r_b");
    const targetA = nodeScreen("r_a");
    canvasEvent(relations, "pointerdown", pointB, { shiftKey: true });
    canvasEvent(relations, "pointermove", { x: (pointB.x + targetA.x) / 2, y: (pointB.y + targetA.y) / 2 }, { shiftKey: true });
    canvasEvent(relations, "pointerup", targetA, { shiftKey: true });
    await wait(250);
    results.add_link_sent = window.__sent.some((m) => m.type === "act"
      && m.action && m.action.action === "add-link"
      && m.action.from === "r_b" && m.action.kind === "before" && m.action.to === "r_a");
    const statusEl = relations.contentDocument.getElementById("status");
    results.warning_shown = !statusEl.hidden
      && statusEl.dataset.tone === "warn"
      && statusEl.textContent.includes("loop");
    results.action_settled = widget().state.pendingActions === 0;

    // EDGE CLICK selects it; the header chip's ✕ sends remove-link.
    window.__sent.length = 0;
    const midA = nodeScreen("r_a");
    const midB = nodeScreen("r_b");
    tap(relations, { x: (midA.x + midB.x) / 2, y: (midA.y + midB.y) / 2 });
    await wait(150);
    const edgeChip = relations.contentDocument.getElementById("edge-chip");
    results.edge_selected = !edgeChip.hidden
      && relations.contentDocument.getElementById("edge-chip-label").textContent.includes("before");
    edgeChip.querySelector("#edge-remove").click();
    await wait(200);
    results.remove_link_sent = window.__sent.some((m) => m.type === "act"
      && m.action && m.action.action === "remove-link"
      && m.action.from === "r_a" && m.action.kind === "before" && m.action.to === "r_b");

    // "New record" -> group-scoped recordCreate: creation mode opens in the
    // same-group Record only, fields writable and empty.
    window.__sent.length = 0;
    relations.contentDocument.getElementById("create-open").click();
    await wait(250);
    const rc = recinfo.contentDocument;
    const oc = other.contentDocument;
    results.create_mode_same_group = rc.getElementById("create").classList.contains("open");
    results.create_mode_scoped = !oc.getElementById("create").classList.contains("open");
    results.create_fields_empty = rc.getElementById("c-head").value === ""
      && rc.getElementById("c-body").value === "";

    // ---- TRAIL MODE (ported trail sand) --------------------------------------
    // Feed a chain r_a -before-> r_b -before-> r_c plus isolated r_d, then
    // switch to trail mode rooted at r_a via the REAL panel controls.
    window.__inbound({ type: "snapshot", id: "card-relations:graph", rows: [
      { uid: "r_a", head: "Alpha", quantity: 0,
        links: [{ uid: "l_ab", from: "r_a", to: "r_b", kind: "before", direction: "out", other: "r_b", hop: 1 }] },
      { uid: "r_b", head: "Beta", quantity: 0, links: [
        { uid: "l_ab", from: "r_a", to: "r_b", kind: "before", direction: "in", other: "r_a", hop: 1 },
        { uid: "l_bc", from: "r_b", to: "r_c", kind: "before", direction: "out", other: "r_c", hop: 1 }] },
      { uid: "r_c", head: "Gamma", quantity: 0,
        links: [{ uid: "l_bc", from: "r_b", to: "r_c", kind: "before", direction: "in", other: "r_b", hop: 1 }] },
      { uid: "r_d", head: "Delta", quantity: 0, links: [] },
    ] });
    await wait(250);
    relations.contentDocument.getElementById("panel-toggle").click();
    const modeSel = relations.contentDocument.getElementById("mode-select");
    modeSel.value = "trail";
    modeSel.dispatchEvent(new relations.contentWindow.Event("change"));
    await wait(150);
    const rootSel = relations.contentDocument.getElementById("trail-root-select");
    rootSel.value = "r_a";
    rootSel.dispatchEvent(new relations.contentWindow.Event("change"));
    await wait(250);

    const w = widget();
    results.trail_tree_scoped = !!w.state.trailTree
      && w.state.trailTree.ids.size === 3
      && !w.state.trailTree.ids.has("r_d");
    const nA = w.state.nodes.find((n) => n.id === "r_a");
    const nB = w.state.nodes.find((n) => n.id === "r_b");
    const nC = w.state.nodes.find((n) => n.id === "r_c");
    results.trail_layered = !!w.state.trailTree
      && w.state.trailTree.layers.get("r_a") === 0
      && w.state.trailTree.layers.get("r_b") === 1
      && w.state.trailTree.layers.get("r_c") === 2
      && nA.x < nB.x && nB.x < nC.x
      && !w.nodeVisible(w.state.nodes.find((n) => n.id === "r_d"));

    // GATING: Done on r_c (parent r_b not done) is refused with advice, and NO
    // set-quantity leaves the sand.
    window.__sent.length = 0;
    tap(relations, nodeScreen("r_c"));
    await wait(120);
    const trailChip = relations.contentDocument.getElementById("trail-chip");
    results.trail_chip_shown = !trailChip.hidden
      && relations.contentDocument.getElementById("trail-chip-label").textContent.includes("Gamma");
    relations.contentDocument.getElementById("trail-done").click();
    await wait(150);
    const gateStatus = relations.contentDocument.getElementById("status");
    results.trail_gating = !gateStatus.hidden
      && gateStatus.dataset.tone === "error"
      && gateStatus.textContent.includes("parents")
      && !window.__sent.some((m) => m.type === "act" && m.action && m.action.action === "set-quantity");

    // DONE on root r_a: the cascade promotes r_b (0 -> -1) AUTOMATICALLY; r_c
    // stays 0. Optimistic apply is immediate; the Actions settle on their acks.
    window.__sent.length = 0;
    tap(relations, nodeScreen("r_a"));
    await wait(120);
    relations.contentDocument.getElementById("trail-done").click();
    await wait(250);
    const doneSets = window.__sent.filter((m) => m.type === "act" && m.action && m.action.action === "set-quantity")
      .map((m) => `${m.action.target}:${m.action.value}`);
    results.trail_cascade_writes = doneSets.includes("r_a:1") && doneSets.includes("r_b:-1")
      && !doneSets.some((s) => s.startsWith("r_c:"));
    results.trail_optimistic = nA.quantity === 1 && nB.quantity === -1 && nC.quantity === 0;
    results.trail_settled = w.state.pendingActions === 0;

    // UNDO r_a: r_a back to 0 and r_b cascades back (-1 -> 0).
    window.__sent.length = 0;
    relations.contentDocument.getElementById("trail-undo").click();
    await wait(250);
    const undoSets = window.__sent.filter((m) => m.type === "act" && m.action && m.action.action === "set-quantity")
      .map((m) => `${m.action.target}:${m.action.value}`);
    results.trail_undo_cascade = undoSets.includes("r_a:0") && undoSets.includes("r_b:0")
      && nA.quantity === 0 && nB.quantity === 0;

    // PRESET CRUD: save the active preset as a kind:"sand" record with the
    // relations.trail extension; a live snapshot on the presets subscription
    // lists it in BOTH relations groups (shared across the cell); applying it
    // switches the active preset.
    window.__sent.length = 0;
    relations.contentDocument.getElementById("trail-preset-name").value = "My trail";
    relations.contentDocument.getElementById("trail-preset-save").click();
    await wait(250);
    results.preset_saved = window.__sent.some((m) => m.type === "act" && m.action
      && m.action.action === "create-record" && m.action.kind === "sand" && m.action.head === "My trail")
      && window.__sent.some((m) => m.type === "act" && m.action
      && m.action.action === "set-extension" && m.action.namespace === "relations.trail"
      && Array.isArray(m.action.fds && m.action.fds.steps));
    const presetRows = [
      { uid: "p_mine", kind: "sand", head: "My trail", quantity: 1,
        extension: { steps: [
          { key: "road", label: "Road ahead", value: 0 },
          { key: "next", label: "Next", value: -1 },
          { key: "past", label: "Past", value: 1 }] } },
    ];
    window.__inbound({ type: "snapshot", id: "card-relations:trail-presets", rows: presetRows });
    window.__inbound({ type: "snapshot", id: "card-relations-b:trail-presets", rows: presetRows });
    await wait(200);
    const presetSel = relations.contentDocument.getElementById("trail-preset-select");
    results.preset_listed_live = [...presetSel.options].some((o) => o.value === "p_mine" && o.textContent === "My trail");
    const presetSelB = relationsB.contentDocument.getElementById("trail-preset-select");
    results.preset_shared_across_groups = [...presetSelB.options].some((o) => o.value === "p_mine");
    presetSel.value = "p_mine";
    presetSel.dispatchEvent(new relations.contentWindow.Event("change"));
    await wait(120);
    results.preset_applied = w.state.trailPreset === "p_mine";

    // CONCEPT PRESET: the built-in @todo/@next/@wip/@done trail writes
    // set-concept — the SAME fact a kanban concept column writes — and the
    // cascade promotes the child to @next via set-concept too.
    window.__sent.length = 0;
    presetSel.value = "builtin:concepts";
    presetSel.dispatchEvent(new relations.contentWindow.Event("change"));
    await wait(120);
    results.concept_steps_shown = relations.contentDocument.getElementById("trail-steps").textContent.includes("@done");
    tap(relations, nodeScreen("r_a"));
    await wait(120);
    relations.contentDocument.getElementById("trail-done").click();
    await wait(250);
    const conceptWrites = window.__sent.filter((m) => m.type === "act" && m.action && m.action.action === "set-concept")
      .map((m) => `${m.action.target}:${m.action.concept}`);
    results.concept_status_writes = conceptWrites.includes("r_a:done") && conceptWrites.includes("r_b:next");

    // ---- NODE GRAVITY (tree-weight physics) ----------------------------------
    // Real panel controls: strength 0.9, root sinks. Weights come from the
    // trail tree's topo depth (root 1 .. leaves 0, outsiders weigh like
    // leaves); in trail mode the pins come off and only tree nodes simulate.
    const gravStr = relations.contentDocument.getElementById("gravity-strength");
    gravStr.value = "0.9";
    gravStr.dispatchEvent(new relations.contentWindow.Event("input"));
    const gravDir = relations.contentDocument.getElementById("gravity-direction");
    gravDir.value = "down";
    gravDir.dispatchEvent(new relations.contentWindow.Event("change"));
    await wait(150);
    const gTree = w.computeTree("r_a", "before");
    const nD = w.state.nodes.find((n) => n.id === "r_d");
    results.gravity_weights = w.nodeGravityWeight(nA, gTree) === 1
      && w.nodeGravityWeight(nB, gTree) === 0.5
      && w.nodeGravityWeight(nC, gTree) === 0
      && w.nodeGravityWeight(nD, gTree) === 0;
    results.gravity_pull_down = w.nodeBuoyancy(nA, gTree) > 0 && w.nodeBuoyancy(nC, gTree) < 0;
    results.gravity_unpins_trail = nA.fx === null && nA.fy === null
      && w.gravityActive()
      && !w.state.simulation.nodes().some((n) => n.id === "r_d");
    gravDir.value = "up";
    gravDir.dispatchEvent(new relations.contentWindow.Event("change"));
    await wait(120);
    results.gravity_pull_up = w.nodeBuoyancy(nA, gTree) < w.nodeBuoyancy(nC, gTree);
    // Off restores the classic pinned trail layout.
    gravDir.value = "off";
    gravDir.dispatchEvent(new relations.contentWindow.Event("change"));
    await wait(120);
    results.gravity_off_pins = typeof nA.fx === "number" && nA.fy === nA.y;
    // Graph mode: the tree stratifies by weight under the live simulation.
    gravDir.value = "down";
    gravDir.dispatchEvent(new relations.contentWindow.Event("change"));
    modeSel.value = "graph";
    modeSel.dispatchEvent(new relations.contentWindow.Event("change"));
    await wait(200);
    const gravSim = w.state.simulation;
    gravSim.alpha(1);
    for (let tick = 0; tick < 300; tick += 1) gravSim.tick();
    results.gravity_settles_graph = nA.y > nC.y && nB.y < nA.y && nB.y > nC.y;

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
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result (iframe/module load?)"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check one_socket             "more than one transport socket was opened"
check graph_subscribed       "the relations sand did not subscribe its driving Protein with the links include"
check graph_rendered         "the graph did not render 2 nodes + 1 deduped edge from the snapshot"
check d3_loaded              "the vendored d3 script did not load (graph has no physics)"
check simulation_exists      "the d3 force simulation was never created"
check empty_state_hidden     "the empty-state overlay shows over a populated graph"
check panel_closes           "the controls panel does not close"
check physics_heat_on_drag   "dragging a node does not heat the simulation"
check node_positioned        "node r_a had no settled position to click"
check same_group_focused     "node click did not focus the same-group Record on that uid"
check diff_group_not_focused "the click leaked to a different-group Record (scoping broken)"
check add_link_sent          "Shift+drag node->node did not send add-link with the active kind"
check warning_shown          "the add-link cycle warning did not surface in the sand status as advice"
check action_settled         "an Action was still pending after its ack"
check edge_selected          "clicking an edge did not select it (header chip)"
check remove_link_sent       "the edge chip's remove did not send remove-link"
check create_mode_same_group "New record did not open creation mode in the same-group Record"
check create_mode_scoped     "creation mode leaked to a different-group Record"
check create_fields_empty    "creation mode fields were not empty/writable"
check trail_tree_scoped      "trail tree is not scoped to the root's forward reachable set"
check trail_layered          "trail layout is not layered by topo depth (or non-tree nodes still show)"
check trail_chip_shown       "selecting a node in trail mode did not show the Done/Undo chip"
check trail_gating           "Done without done parents was not refused (or a write escaped anyway)"
check trail_cascade_writes   "Done did not write set-quantity for the node AND auto-promote the child to next"
check trail_optimistic       "the cascade did not apply optimistically before the acks"
check trail_settled          "a trail Action was still pending after its ack"
check trail_undo_cascade     "Undo did not cascade the node and its promoted child back to road ahead"
check gravity_weights        "node gravity weight is not root 1 .. leaf/outsider 0 by topo depth"
check gravity_pull_down      "root-sinks buoyancy does not accelerate the root down and the leaves up"
check gravity_unpins_trail   "trail mode with gravity on still pins nodes (or simulates non-tree nodes)"
check gravity_pull_up        "root-floats buoyancy does not invert the acceleration"
check gravity_off_pins       "turning gravity off did not restore the pinned trail layout"
check gravity_settles_graph  "the graph-mode simulation does not stratify the tree by weight"
check preset_saved           "preset save did not create-record kind sand + set-extension relations.trail"
check preset_listed_live     "a saved preset did not live-appear in the preset select"
check preset_shared_across_groups "a saved preset did not live-appear in the SECOND relations group"
check preset_applied         "applying a preset from the select did not switch the active preset"
check concept_steps_shown    "the concept preset's steps are not shown in the panel"
check concept_status_writes  "the concept preset did not write set-concept for done AND the auto-promoted @next"

[ "$fail" -eq 0 ] && echo "PASS: relations graph + trail on Protein/Actions — render, scoped recordClicked/recordCreate, add/remove-link with warnings-as-advice, trail tree layout, Done/Undo cascade with gating, preset CRUD shared across groups, concept status vocabulary, node gravity tree-weight physics in both modes" || exit 1
