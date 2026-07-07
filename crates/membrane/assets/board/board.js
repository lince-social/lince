// Membrane board bootstrap (Stage 8b §4). This is the ONE new file of the
// board-chrome lift: the thin wiring layer that replaces web's 6,592-line
// main.js. Everything hard (camera math, card placement, drag/resize/marquee,
// the workspace/card store) is REUSED unchanged from web's pure modules —
// carried over verbatim per blueprint VII.4. This file only wires them to
//   (a) localStorage for host-state persistence (local-first, no backend), and
//   (b) the re-pointed widget bridge, so each card is a sand iframe speaking
//       Protein + Actions.
import { createGridConfig } from "/board/grid.js";
import { createBoardStore } from "/board/store.js";
import { createBoardViewport } from "/board/viewport.js";
import { attachBoardInteractions, groupBounds } from "/board/interactions.js";
import { resolveMarqueeGroup } from "/board/group-logic.js";
import { createBridge } from "/board/bridge.js";

const STORAGE_KEY = "lince.membrane.board";
const RESIZE_HANDLES = ["nw", "ne", "sw", "se", "n", "e", "s", "w"];

// ---- host-state persistence: localStorage (blueprint: works with WS down) ----
function loadBoardState() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}
function saveBoardState(state) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    /* private mode / quota — board still works in-memory this session */
  }
}

// First-run seed: two sand cards so the board isn't empty. Cards use the store's
// "package" kind — the widget-card model — because only that kind preserves
// widgetState through sanitization. A card carries its sand route in
// widgetState.sand (arbitrary host JSON, never the Ledger). They are placed
// near the WORLD CENTER because the default camera frames the world's middle —
// seeding at (0,0) would render them off-screen.
function seedCards(cfg) {
  const cx = (cfg.world?.width ?? 10000) / 2;
  const cy = (cfg.world?.height ?? 10000) / 2;
  return [
    { id: "table", title: "Records", kind: "package",
      x: cx - 440, y: cy - 180, width: 420, height: 340, widgetState: { sand: "/sand/table" } },
    { id: "provenance", title: "Provenance", kind: "package",
      x: cx + 20, y: cy - 180, width: 420, height: 340, widgetState: { sand: "/sand/record-info" } },
    { id: "todo", title: "Todo", kind: "package",
      x: cx - 440, y: cy + 200, width: 420, height: 340, widgetState: { sand: "/sand/todo" } },
    { id: "kanban", title: "Kanban", kind: "package",
      x: cx + 20, y: cy + 200, width: 720, height: 420, widgetState: { sand: "/sand/kanban" } },
    { id: "relations", title: "Relations", kind: "package",
      x: cx + 780, y: cy - 180, width: 420, height: 420, widgetState: { sand: "/sand/relations" } },
  ];
}

const config = createGridConfig({ density: 4 });
const saved = loadBoardState();
const store = createBoardStore({
  config,
  seedCards: saved ? [] : seedCards(config),
  initialBoardState: saved,
  persistState(nextState) { saveBoardState(nextState); },
});

// ---- DOM refs (ids are the contract the reused modules bind to) ----
const shell = document.getElementById("board-shell");
const canvas = document.getElementById("board-canvas");
const world = document.getElementById("board-world");
const cardsLayer = document.getElementById("cards-layer");
const pinnedLayer = document.getElementById("pinned-layer");
const workspaceTabs = document.getElementById("workspace-tabs");
const editToggle = document.getElementById("edit-toggle");
const zoomIndicator = document.getElementById("board-zoom-indicator");
const liveDot = document.getElementById("live-dot");

let editMode = false;
let activeGroup = null; // { id, cardIds: [] } — the marquee selection
const cardEls = new Map(); // cardId -> element (reconciled across renders)

// Marquee rectangle (screen space) + group outline (world space), reused verbatim
// in spirit from web's main.js — ctrl/⌘+drag selects cards into a group that
// then moves/resizes together through the reused interactions module.
const marqueeEl = document.createElement("div");
marqueeEl.id = "board-marquee"; marqueeEl.hidden = true;
const groupOutline = document.createElement("div");
groupOutline.id = "group-outline"; groupOutline.hidden = true;

// ---- the re-pointed bridge: the board owns the one WebSocket ----
// onLive reflects the transport connection state on the chrome dot (the sands
// get their own live pings via postMessage).
createBridge({ onLive: (live) => liveDot.classList.toggle("live", !!live) });

// ---- viewport: pan/zoom, camera persisted to the active workspace ----
const viewport = createBoardViewport({
  viewportElement: canvas,
  worldElement: world,
  onCameraChanged(camera) {
    renderZoom(camera);
    store.updateActiveCamera(camera, { persist: true });
  },
});
// Restore the active workspace camera on load.
viewport.setCamera(store.getSnapshot().activeCamera || { x: 0, y: 0, scale: 1 }, { silent: true });
renderZoom(store.getSnapshot().activeCamera || { x: 0, y: 0, scale: 1 });

// Keep the overflow-hidden pane pinned at (0,0) — focusing an iframe can
// otherwise auto-scroll it and displace every absolute layer (web main.js note).
canvas.scrollLeft = 0; canvas.scrollTop = 0;
canvas.addEventListener("scroll", () => {
  if (canvas.scrollLeft !== 0 || canvas.scrollTop !== 0) { canvas.scrollLeft = 0; canvas.scrollTop = 0; }
});

// ---- interactions: move / resize / pinned-move (reused, unchanged math) ----
attachBoardInteractions({
  boardElement: canvas,
  config,
  readCards: () => store.getCards(),
  replaceCards: (cards, options) => store.replaceCards(cards, options),
  isEditMode: () => editMode,
  isCardEditable: () => editMode,
  getScale: () => viewport.getScale(),
  // A card that belongs to the active group drags the whole group with it.
  resolveCardGroupIds: (card) =>
    activeGroup && activeGroup.cardIds.includes(card.id) ? [...activeGroup.cardIds] : null,
  getActiveGroupCardIds: () => (activeGroup ? [...activeGroup.cardIds] : null),
  onInteractionStart: () => viewport.setInteractionLocked(true),
  onInteractionEnd: () => { viewport.setInteractionLocked(false); syncGroupOutline(); },
});

// ---- marquee grouping (ctrl/⌘+drag on empty canvas) ----
canvas.appendChild(marqueeEl);
world.appendChild(groupOutline);
let marquee = null;

canvas.addEventListener("pointerdown", (event) => {
  if (!editMode || event.button !== 0 || !(event.ctrlKey || event.metaKey)) return;
  if (event.target.closest("[data-card-id]")) return; // start on empty canvas only
  event.preventDefault(); event.stopPropagation();
  viewport.setInteractionLocked(true);
  marquee = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY };
  drawMarquee(event.clientX, event.clientY);
  window.addEventListener("pointermove", onMarqueeMove);
  window.addEventListener("pointerup", onMarqueeUp);
  window.addEventListener("pointercancel", onMarqueeUp);
}, true);

function drawMarquee(cx, cy) {
  const r = canvas.getBoundingClientRect();
  marqueeEl.style.left = `${Math.min(marquee.startX, cx) - r.left}px`;
  marqueeEl.style.top = `${Math.min(marquee.startY, cy) - r.top}px`;
  marqueeEl.style.width = `${Math.abs(cx - marquee.startX)}px`;
  marqueeEl.style.height = `${Math.abs(cy - marquee.startY)}px`;
  marqueeEl.hidden = false;
}

function onMarqueeMove(event) {
  if (!marquee || event.pointerId !== marquee.pointerId) return;
  event.preventDefault();
  drawMarquee(event.clientX, event.clientY);
}

function onMarqueeUp(event) {
  if (!marquee) return;
  window.removeEventListener("pointermove", onMarqueeMove);
  window.removeEventListener("pointerup", onMarqueeUp);
  window.removeEventListener("pointercancel", onMarqueeUp);
  marqueeEl.hidden = true;
  viewport.setInteractionLocked(false);
  const state = marquee; marquee = null;

  // screen → world so selection is camera-independent (reused viewport math)
  const a = viewport.worldPointFromClient(state.startX, state.startY);
  const b = viewport.worldPointFromClient(event.clientX ?? state.startX, event.clientY ?? state.startY);
  const rect = { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y),
                 width: Math.abs(a.x - b.x), height: Math.abs(a.y - b.y) };
  if (rect.width < 4 && rect.height < 4) { dissolveGroup(); return; }

  const group = resolveMarqueeGroup(store.getCards(), rect);
  if (!group) { dissolveGroup(); return; }
  activeGroup = group;
  store.setCardsGroup(group.cardIds, group.id);
  syncGroupOutline();
}

function dissolveGroup() {
  if (activeGroup) store.setCardsGroup(activeGroup.cardIds, null);
  activeGroup = null;
  groupOutline.hidden = true;
}

// Draw the outline around the current group members (world space).
function syncGroupOutline() {
  if (!activeGroup || !editMode) { groupOutline.hidden = true; return; }
  const members = store.getCards().filter((c) => activeGroup.cardIds.includes(c.id));
  if (!members.length) { groupOutline.hidden = true; return; }
  const box = groupBounds(members);
  const pad = 8;
  groupOutline.style.left = `${box.x - pad}px`;
  groupOutline.style.top = `${box.y - pad}px`;
  groupOutline.style.width = `${box.width + pad * 2}px`;
  groupOutline.style.height = `${box.height + pad * 2}px`;
  groupOutline.hidden = false;
}

// ---- render: reconcile the store snapshot into card DOM ----
store.subscribe((snapshot) => render(snapshot));

function render(snapshot) {
  renderWorkspaces(snapshot);
  const cards = snapshot.cards || [];
  const seen = new Set();

  cards.forEach((card, index) => {
    seen.add(card.id);
    let el = cardEls.get(card.id);
    if (!el) { el = createCardEl(card); cardEls.set(card.id, el); }
    positionCard(el, card, index);
    const layer = card.pinned ? pinnedLayer : cardsLayer;
    if (el.parentElement !== layer) layer.appendChild(el);
  });

  // remove cards no longer present
  for (const [id, el] of cardEls) {
    if (!seen.has(id)) { el.remove(); cardEls.delete(id); }
  }
  syncGroupOutline();
}

function createCardEl(card) {
  const el = document.createElement("article");
  el.className = "board-card panzoom-exclude";
  el.dataset.cardId = card.id;
  el.dataset.cardKind = card.kind || "sand";

  const header = document.createElement("header");
  header.className = "card-header";
  const title = document.createElement("span");
  title.className = "card-title";
  title.textContent = card.title || card.id;
  const actions = document.createElement("div");
  actions.className = "card-actions";
  actions.append(
    chromeBtn("⤒", "Bring to front", () => store.reorderCard(card.id, "front")),
    chromeBtn("⤓", "Send to back", () => store.reorderCard(card.id, "back")),
    chromeBtn("📌", "Pin", () => togglePin(card.id)),
  );
  header.append(title, actions);

  const body = document.createElement("div");
  body.className = "card-body";
  const frame = document.createElement("iframe");
  frame.title = card.title || card.id;
  frame.dataset.linceInstanceId = card.id;
  const src = card.widgetState && card.widgetState.sand;
  if (src) frame.src = src;
  body.appendChild(frame);

  el.append(header, body);
  for (const h of RESIZE_HANDLES) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = `resize-handle resize-handle--${h}`;
    btn.tabIndex = -1;
    btn.setAttribute("aria-hidden", "true");
    btn.dataset.resizeHandle = h;
    el.appendChild(btn);
  }
  return el;
}

function positionCard(el, card, index) {
  // x/y are in the coordinate space of the card's layer (world for unpinned,
  // screen for pinned — togglePin converts across the camera on toggle).
  el.style.left = `${card.x}px`;
  el.style.top = `${card.y}px`;
  el.style.width = `${card.width}px`;
  el.style.height = `${card.height}px`;
  el.style.zIndex = String(card.zIndex || index + 1);
  el.classList.toggle("is-pinned", !!card.pinned);
}

// Pinned cards live in screen space (pinned-layer), unpinned in world space
// (cards-layer). Converting x/y across the camera on toggle keeps the card
// under the cursor instead of teleporting when the board is panned/zoomed.
function togglePin(cardId) {
  const cam = viewport.getCamera();
  const s = cam.scale || 1;
  store.updateCard(cardId, (c) => {
    if (c.pinned) {
      return { ...c, pinned: false, x: (c.x - cam.x) / s, y: (c.y - cam.y) / s };
    }
    return { ...c, pinned: true, x: c.x * s + cam.x, y: c.y * s + cam.y };
  });
}

function chromeBtn(label, title, onClick) {
  const b = document.createElement("button");
  b.type = "button"; b.className = "card-btn"; b.textContent = label; b.title = title;
  b.addEventListener("click", (e) => { e.stopPropagation(); onClick(); });
  return b;
}

// ---- workspaces: tabs + add + switch (reused store ops) ----
function renderWorkspaces(snapshot) {
  workspaceTabs.innerHTML = "";
  for (const ws of snapshot.workspaces) {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.className = "workspace-tab" + (ws.id === snapshot.activeWorkspaceId ? " active" : "");
    tab.textContent = ws.name || "Workspace";
    tab.addEventListener("click", () => {
      store.switchWorkspace(ws.id);
      viewport.setCamera(store.getSnapshot().activeCamera || { x: 0, y: 0, scale: 1 }, { silent: true });
    });
    workspaceTabs.appendChild(tab);
  }
  const add = document.createElement("button");
  add.type = "button"; add.className = "workspace-tab add"; add.textContent = "+"; add.title = "New workspace";
  add.addEventListener("click", () => store.addWorkspace());
  workspaceTabs.appendChild(add);
}

// ---- edit mode: toggles movability; iframes go inert so drags reach cards ----
function setEditMode(on) {
  editMode = on;
  shell.classList.toggle("is-edit", on);
  editToggle.classList.toggle("active", on);
  editToggle.textContent = on ? "Done" : "Edit";
  if (!on) dissolveGroup();
  syncGroupOutline();
}
editToggle.addEventListener("click", () => setEditMode(!editMode));

// ---- zoom controls ----
function renderZoom(camera) {
  zoomIndicator.textContent = `${Math.round((camera.scale || 1) * 100)}%`;
}
document.getElementById("board-zoom-in").addEventListener("click", () => viewport.zoomBy(1.2));
document.getElementById("board-zoom-out").addEventListener("click", () => viewport.zoomBy(1 / 1.2));
document.getElementById("board-recenter").addEventListener("click", () => viewport.recenter());

// initial paint
render(store.getSnapshot());

// ---- self-test (?selftest): drives REAL pointer interactions so headless
// chromium can verify the wiring, not just the rendering. Reads back through
// data-* attributes on <body>. Not shipped behavior; a verification hook. ----
if (location.search.includes("selftest=bridge")) {
  runBridgeSelfTest();
} else if (location.search.includes("selftest=persist-read")) {
  runPersistRead();
} else if (location.search.includes("selftest")) {
  runSelfTest();
}

// Drives the SAND DATA PLANE through the real bridge in a browser: the table
// iframe subscribes over frame.js → bridge.js → transport WS, renders rows from
// the Protein snapshot; then we submit its Add form (a create-record Action back
// through the same path) and assert a new row arrives live. This is the one
// thing the Rust pilot can't cover — the postMessage relay itself.
async function runBridgeSelfTest() {
  const out = (v) => { document.body.dataset.selftestBridge = v; };
  try {
    const frame = cardEls.get("table").querySelector("iframe");
    const doc = () => frame.contentDocument;
    const rowCount = () => doc()?.querySelectorAll("#rows tr").length || 0;

    // 1. wait for the live snapshot to render rows (seeded records)
    if (!(await waitFor(() => rowCount() > 0, 6000))) { out("render:FAIL"); return; }
    const before = rowCount();

    // 2. drive a create-record Action through the bridge directly via the
    //    iframe's LinceWidgetHost, and report the promise outcome so a hung /
    //    rejected Action relay is distinguishable from a missing live update.
    const host = frame.contentWindow.LinceWidgetHost;
    let actOutcome = "timeout";
    await Promise.race([
      host.act({ action: "create-record", kind: "plain", head: "Bridge self-test row", body: "", quantity: 0 })
        .then((r) => { actOutcome = "resolved:facts=" + r.facts; })
        .catch((e) => { actOutcome = "rejected:" + ((e && e.message) || e); }),
      new Promise((r) => setTimeout(r, 6000)),
    ]);

    // 3. the new record must arrive as a live update (create-record emits a
    //    zero-delta creation fact, so even qty 0 refreshes the subscription)
    const grew = await waitFor(() => rowCount() > before, 4000);
    out(`render:PASS act:${actOutcome} live:${grew ? "PASS" : "FAIL"} rows:${before}->${rowCount()}`);
  } catch (e) {
    out("error:" + String((e && e.message) || e));
  }
  // Report back over HTTP so a headless run can read the result in real time
  // (independent of --dump-dom's virtual clock).
  try { await fetch("/selftest-result?" + encodeURIComponent(document.body.dataset.selftestBridge || "")); } catch {}
}

function waitFor(pred, timeoutMs) {
  return new Promise((resolve) => {
    const start = Date.now();
    const tick = () => {
      let ok = false;
      try { ok = pred(); } catch { ok = false; }
      if (ok) return resolve(true);
      if (Date.now() - start > timeoutMs) return resolve(false);
      setTimeout(tick, 50);
    };
    tick();
  });
}
function runSelfTest() {
  const results = {};
  try {
    setEditMode(true);
    const cards0 = store.getCards();
    const card = cards0.find((c) => c.id === "table");
    const el = cardEls.get("table");
    const header = el.querySelector(".card-header");
    const beforeX = card.x;

    // synthesize a header drag: pointerdown on the card, move+up on window
    // (interactions.js binds move/up to window, so synthetic events drive it)
    const r = header.getBoundingClientRect();
    header.dispatchEvent(new PointerEvent("pointerdown",
      { button: 0, pointerId: 1, clientX: r.left + 10, clientY: r.top + 6, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointermove",
      { pointerId: 1, clientX: r.left + 160, clientY: r.top + 6, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointerup",
      { pointerId: 1, clientX: r.left + 160, clientY: r.top + 6, bubbles: true }));
    const afterX = store.getCards().find((c) => c.id === "table").x;
    results.drag = afterX !== beforeX ? "PASS" : "FAIL";

    // z-order: bring table to front → its zIndex must exceed provenance's
    store.reorderCard("table", "front");
    const cs = store.getCards();
    const tz = cs.find((c) => c.id === "table").zIndex;
    const pz = cs.find((c) => c.id === "provenance").zIndex;
    results.zorder = tz > pz ? "PASS" : "FAIL";

    // marquee grouping: ctrl+drag on empty canvas enclosing both cards forms a
    // group (both cards get the same groupId, activeGroup is set)
    // generous bounds so both cards are fully contained (resolveMarqueeGroup
    // requires containment) regardless of the earlier drag + topbar y-offset
    canvas.dispatchEvent(new PointerEvent("pointerdown",
      { button: 0, pointerId: 2, ctrlKey: true, clientX: 10, clientY: 10, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointermove",
      { pointerId: 2, ctrlKey: true, clientX: 1300, clientY: 760, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointerup",
      { pointerId: 2, ctrlKey: true, clientX: 1300, clientY: 760, bubbles: true }));
    const grouped = store.getCards().filter((c) => c.groupId);
    results.group = activeGroup && grouped.length >= 2
      && grouped.every((c) => c.groupId === grouped[0].groupId) ? "PASS" : "FAIL";
    dissolveGroup(); // reset before the pin/workspace mutations

    // resize: drag the table card's SE handle → width must change (interactions
    // routes on [data-resize-handle]; move/up on window drive it). After the
    // marquee so the enlarged card can't fall outside the selection rect.
    const beforeW = store.getCards().find((c) => c.id === "table").width;
    const handle = el.querySelector('[data-resize-handle="se"]');
    const hr = handle.getBoundingClientRect();
    handle.dispatchEvent(new PointerEvent("pointerdown",
      { button: 0, pointerId: 3, clientX: hr.left + 2, clientY: hr.top + 2, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointermove",
      { pointerId: 3, clientX: hr.left + 140, clientY: hr.top + 110, bubbles: true }));
    window.dispatchEvent(new PointerEvent("pointerup",
      { pointerId: 3, clientX: hr.left + 140, clientY: hr.top + 110, bubbles: true }));
    results.resize = store.getCards().find((c) => c.id === "table").width !== beforeW ? "PASS" : "FAIL";

    // pin: toggles the flag and moves the card into the pinned layer
    togglePin("provenance");
    const pinned = store.getCards().find((c) => c.id === "provenance").pinned === true;
    const inPinnedLayer = cardEls.get("provenance").parentElement === pinnedLayer;
    results.pin = pinned && inPinnedLayer ? "PASS" : "FAIL";

    // workspace: add + switch changes the active workspace id
    const before = store.getSnapshot().activeWorkspaceId;
    store.addWorkspace();
    const afterAdd = store.getSnapshot();
    const other = afterAdd.workspaces.find((w) => w.id !== before);
    store.switchWorkspace(other.id);
    results.workspace = store.getSnapshot().activeWorkspaceId !== before ? "PASS" : "FAIL";

    // zoom last (mutating the camera would shift the marquee's world coords):
    // zoomBy changes the camera scale (pan/zoom share the viewport)
    const beforeScale = viewport.getScale();
    viewport.zoomBy(1.5);
    results.zoom = viewport.getScale() !== beforeScale ? "PASS" : "FAIL";

    // persistence write-path: the mutations above must have serialized to
    // localStorage (the added workspace + the moved card's new x).
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || "null");
    let savedTable = null;
    for (const ws of (saved && saved.workspaces) || []) {
      const c = (ws.cards || []).find((c) => c.id === "table");
      if (c) savedTable = c;
    }
    results.persistwrite = saved && (saved.workspaces || []).length >= 2
      && savedTable && Math.round(savedTable.x) === Math.round(afterX) ? "PASS" : "FAIL";
  } catch (e) {
    results.error = String(e && e.message || e);
  }
  document.body.dataset.selftest = Object.entries(results).map(([k, v]) => `${k}:${v}`).join(" ");
  // report over HTTP too, so a real-time (non-virtual-clock) run can read it
  try { fetch("/selftest-result?chrome=" + encodeURIComponent(document.body.dataset.selftest)); } catch {}
}

// Restore-on-reload: a SECOND load (same browser profile, so localStorage
// survives) after runSelfTest mutated + persisted. The store should hydrate the
// added workspace from localStorage. Proves the persistence READ path.
function runPersistRead() {
  const snap = store.getSnapshot();
  const ok = snap.workspaces.length >= 2; // the workspace added last run survived
  document.body.dataset.selftestPersist = `restore:${ok ? "PASS" : "FAIL"} ws:${snap.workspaces.length}`;
  try { fetch("/selftest-result?persist=" + encodeURIComponent(document.body.dataset.selftestPersist)); } catch {}
}
