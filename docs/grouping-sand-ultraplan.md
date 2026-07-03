# Grouping, Z-order and Sand ABI

Design document for four board features: per-card z-index ordering, a grouping
system (with lock and publish sub-features), an ABI-like event system between
sands, and a new Record Info sand that consumes those events.

Conventions used below:

- Every implementable point is a `- [ ]` todo. Top-level features are the
  `# - [ ] Feature N:` headings; sub-todos live inside each section.
- Code examples are illustrative but grounded in the real codebase — file and
  line references point at the functions to extend or mirror.
- Settled decisions (already discussed, not open questions) are stated as
  requirements.

Layer bands recap, used throughout: unpinned cards default to `z=1`
(`default_card_z_index` in `crates/web/src/domain/board.rs`), pinned cards
render at `z=50`, and system shell UI occupies `90–95`. The active card gets a
temporary `max(zIndex, 100)` override at render time
(`crates/web/static/presentation/board/main.js:2073-2077`).

---

# - [x] Feature 1: Z-index ordering:

Users can reorder overlapping cards with four commands: **bring to front**,
**send to back**, **bring forward** (one step), **send backward** (one step).
Reordering operates **within the card's layer band**: an unpinned card is only
reordered relative to other unpinned cards of the same workspace, a pinned card
relative to other pinned cards. Reordering must never push a card across the
pinned (`50`) or system (`90–95`) bands.

UI (settled): hover-toolbar buttons plus keyboard shortcuts —
`Ctrl+]` bring forward, `Ctrl+[` send backward, `Ctrl+Shift+]` bring to front,
`Ctrl+Shift+[` send to back, applied to the active/hovered card in edit mode.

## Store mutation

Add a `reorderCard(cardId, direction)` mutation to the store object in
`crates/web/static/presentation/board/store.js` (next to `updateCard`,
`store.js:742`). It sorts the same-layer cards of the card's workspace by
`zIndex`, moves the card within that order, then reassigns compact values so
z-indexes never grow unboundedly. Persistence rides the existing
`commit()`/`persist()` path (`store.js:435` / `store.js:419`) — no new endpoint.

```js
// store.js — sketch, lives next to updateCard (store.js:742)
reorderCard(cardId, direction, options = {}) {
  for (const workspace of state.workspaces) {
    const card = workspace.cards.find((entry) => entry.id === cardId);
    if (!card) continue;

    // Same layer band only: pinned cards reorder among pinned, unpinned among unpinned.
    const layer = workspace.cards
      .filter((entry) => entry.pinned === card.pinned && entry.system !== true)
      .sort((a, b) => (Number(a.zIndex) || 1) - (Number(b.zIndex) || 1));

    const from = layer.indexOf(card);
    const to =
      direction === "front" ? layer.length - 1
      : direction === "back" ? 0
      : direction === "forward" ? Math.min(from + 1, layer.length - 1)
      : Math.max(from - 1, 0);
    if (to === from) return null;
    layer.splice(from, 1);
    layer.splice(to, 0, card);

    // Compact reassignment inside the band: unpinned 1..N (N < 50), pinned 50..50+N (< 90).
    const base = card.pinned === true ? 50 : 1;
    layer.forEach((entry, index) => {
      entry.zIndex = base + index;
    });
    return commit(options);
  }
  return null;
},
```

- [x] Implement `reorderCard(cardId, direction)` in `store.js` as above and
      expose it on the returned store object.
- [x] Cap band size so compact reassignment cannot leak into the next band
      (unpinned must stay `< 50`, pinned `< 90`); clamp rather than error.
- [x] Mirror the new ordering on first paint: the server renders initial
      `z-index` inline styles (`crates/web/src/presentation/pages/shared.rs:81`),
      which already emits `card.z_index` — verify persisted values round-trip
      through `BoardCard::z_index` (`crates/web/src/domain/board.rs`) untouched.

## Toolbar buttons and shortcuts

Four new `data-card-action` buttons in `renderCardToolbarContent`
(`crates/web/static/presentation/board/main.js:804`), following the existing
button pattern:

```js
// main.js:804 — added to the array in renderCardToolbarContent(card)
isEditable
  ? `<button type="button" class="card-toolbar-btn" data-card-action="bring-forward"
       aria-label="Trazer para frente ${escapeHtml(card.title)}">${renderBringForwardIcon()}</button>`
  : "",
// ...same for "send-backward", "bring-to-front", "send-to-back"
```

Handled in `handleCardActionClick` (`main.js:5188`) by calling
`store.reorderCard(cardId, direction)`; keyboard shortcuts added to the global
keydown handler (`main.js:5703`), gated on edit mode and an active card:

```js
// main.js:5703 — inside document.addEventListener("keydown", ...)
if (editMode && activeCardId && (event.ctrlKey || event.metaKey) && (event.key === "]" || event.key === "[")) {
  event.preventDefault();
  const direction = event.shiftKey
    ? (event.key === "]" ? "front" : "back")
    : (event.key === "]" ? "forward" : "backward");
  store.reorderCard(activeCardId, direction);
  return;
}
```

- [x] Add the four toolbar buttons (+ icons) to `renderCardToolbarContent`
      (`main.js:804`) and their cases in `handleCardActionClick` (`main.js:5188`).
- [x] Add the `Ctrl+]/[` and `Ctrl+Shift+]/[` shortcuts to the global keydown
      handler (`main.js:5703`) — implemented via `event.code`
      (`BracketRight`/`BracketLeft`) so Shift+`]` still matches, targeting the
      hovered card (fallback: active card).

## Known gotchas (each is a fix task)

- [x] **Pin toggle resets `zIndex`**: the `"pin"` action (`main.js:5212-5231`)
      currently sets `zIndex: card.pinned === true ? 1 : max(zIndex, 50)` —
      unpinning always drops the card to the back of the unpinned band. Change
      it to re-slot the card into the target band preserving relative order —
      implemented by landing the card at the top of the target band (49 when
      unpinning, 89 when pinning); reorder commands compact the band afterwards.
- [x] **Active-card override**: rendering forces
      `max(zIndex, 100)` for the active card (`main.js:2073-2077`), which
      visually masks reorder results while a card is active. Keep the override
      (it is what makes the focused card readable) but make sure the *stored*
      `zIndex` is what the reorder commands mutate, so the order is correct
      again on blur.
- [x] **Server-rendered initial styles**: `render_card` writes `z-index` inline
      (`crates/web/src/presentation/pages/shared.rs:81`); confirm hydration in
      `main.js` re-applies stored `zIndex` (it does, at `main.js:2073-2077`) so
      no separate migration is needed.

---

# - [x] Feature 2: Grouping system:

In edit mode, users can marquee-select multiple cards into a **group**: a
temporary construct with its own outline and toolbar that supports group move,
proportional resize, delete, pin, **lock** (persistence) and **publish**
(export). Groups are ephemeral by default — pure frontend state — and only the
lock sub-feature writes anything to the board model.

Settled decisions:

- Marquee = **ctrl + left-drag** on the board canvas in edit mode.
- Selection includes only cards **fully contained** in the marquee rectangle,
  and only **unpinned, world-space** cards (pinned/system cards are never
  group members).
- Group resize is **proportional**: card positions *and* sizes scale with the
  group bounding box.
- Locked groups have **zero footprint outside edit mode** (no outline, no
  toolbar, no interaction change).

## Marquee selection

Ctrl+pointerdown on `#board-canvas` in edit mode must win over panzoom: lock
the viewport for the duration of the drag with
`boardViewport.setInteractionLocked(true)`
(`crates/web/static/presentation/board/viewport.js:220`) — the same mechanism
card drag/resize already uses. Draw a screen-space `<div>` selection rectangle;
on pointerup convert both corners to world coordinates with
`worldPointFromClient` (`viewport.js:101`) and select every unpinned,
non-system card whose rect is fully inside the world rect.

```js
// main.js — marquee finish (sketch)
const a = boardViewport.worldPointFromClient(startClientX, startClientY);
const b = boardViewport.worldPointFromClient(endClientX, endClientY);
const rect = {
  x: Math.min(a.x, b.x), y: Math.min(a.y, b.y),
  w: Math.abs(a.x - b.x), h: Math.abs(a.y - b.y),
};
const memberIds = store
  .getCards()
  .filter((card) => card.pinned !== true && card.system !== true)
  .filter((card) =>
    card.x >= rect.x && card.y >= rect.y &&
    card.x + card.width <= rect.x + rect.w &&
    card.y + card.height <= rect.y + rect.h)
  .map((card) => card.id);
```

- [x] Intercept ctrl+pointerdown on `#board-canvas` (edit mode only) before the
      panzoom handlers; lock/unlock via `setInteractionLocked`
      (`viewport.js:220`).
- [x] Screen-space marquee overlay element + pointermove sizing; on pointerup,
      world-rect containment test as above.

## Ephemeral group state and chrome

Group state lives in `main.js` as plain module state:
`activeGroup = { id, cardIds }` (id is a `crypto.randomUUID()` used only for
DOM bookkeeping unless the group gets locked). The group is dissolved by
`Escape`, by clicking a card that is not a member, or by leaving edit mode.

Chrome: one group outline element (positioned at the union bounding box of the
member cards, updated on every layout render) and a group toolbar with
move handle, resize handles, delete, pin, lock and publish buttons — reusing
the `card-toolbar-btn` styling and the `positionCardControlsToolbar` placement
approach (`main.js:824`).

- [x] `activeGroup` state + dissolve rules (Esc in the `main.js:5703` keydown
      handler, click-outside in the board click handler, edit-mode exit).
- [x] Group outline element + group toolbar with `data-group-action` buttons,
      dispatched by a `handleGroupActionClick` sibling of
      `handleCardActionClick` (`main.js:5188`).

## Group move and proportional resize

`interactions.js` currently tracks a single card per gesture — the interaction
object stores one `cardId` (`interactions.js:144`) and a `baseCards` snapshot
of the whole layout (`interactions.js:147`), previewing by swapping that one
card into the snapshot. Generalize the interaction to a **set of card ids**:
snapshot once, and on every pointermove map *each member's* base rect through
the gesture transform before handing the layout to the preview.

Proportional resize maps each member rect through the bounding-box transform:

```js
// interactions.js — group resize math (sketch)
// base: group bounding box at gesture start; next: box after handle drag
const scaleX = next.w / base.w;
const scaleY = next.h / base.h;
for (const card of memberBaseCards) {
  candidate.x = next.x + (card.x - base.x) * scaleX;
  candidate.y = next.y + (card.y - base.y) * scaleY;
  candidate.width = card.width * scaleX;
  candidate.height = card.height * scaleY;
  // snap/clamp each result exactly like single-card gestures do:
  clampCard(candidate, config); // grid.js:220
}
```

- [x] Generalize the interaction object in `interactions.js` from `cardId` to
      `cardIds` (single-card gestures become a set of one), keeping the
      `baseCards` snapshot/preview pattern intact.
- [x] Group move: apply the drag delta to every member's base rect.
- [x] Group proportional resize from the group toolbar's handles, per the math
      above, with `clampCard` (`grid.js:220`) snapping per member.
- [x] Enforce min card sizes during group resize: stop scaling down once any
      member hits the `clampCard` minimum, instead of letting members drift
      apart from the proportional layout.

## Group delete and group pin

- [x] Group delete: one confirmation modal (reuse the delete-card modal), then
      loop `store.removeCard(cardId)` over members; batch persistence by
      passing `{ persist: false }` per removal and one final `commit()`.
- [x] Group pin: apply the pin action's world↔screen conversion
      (`main.js:5212-5231`, using `worldPointFromClient` /
      `getBoundingClientRect`) to each member so the group visually stays put
      when it switches bands. Pinning dissolves the group (pinned cards are not
      valid members).

## - [x] Sub-feature: Lock (persistent groups)

Locking makes the current ephemeral group survive reloads. Persistence model
(settled, simplest possible): a nullable group id **on the card itself** — no
group table, no group entity.

```rust
// crates/web/src/domain/board.rs — BoardCard (struct at board.rs:5-44)
#[serde(default)]
pub group_id: Option<String>,
```

Because the field is `#[serde(default)]`, old persisted boards deserialize
cleanly — **do not bump `BOARD_STATE_SCHEMA_VERSION`**: a version mismatch
resets the whole board to defaults
(`crates/web/src/infrastructure/board_state_store.rs:51`).

Frontend mirror (same `groupId` camelCase key via the struct's
`rename_all = "camelCase"`):

- `sanitizeCard` (`grid.js:158`) — carry `groupId` through sanitization.
- `exportCard` (`store.js:270`) — include `groupId` in persisted payloads.
- `cardTemplate` (`store.js:336`) — default `groupId: null` for new cards.

On board load, locked groups are rebuilt purely by scanning cards for shared
`groupId` values; in edit mode selecting any member selects the whole locked
group. Outside edit mode a locked group renders and behaves exactly like
ungrouped cards (zero footprint). Unlocking sets every member's `groupId` back
to `null` and the group becomes ephemeral again.

- [x] Add `group_id: Option<String>` + `#[serde(default)]` to `BoardCard`
      (`domain/board.rs:5-44`); no schema-version bump.
- [x] Mirror `groupId` in `sanitizeCard` (`grid.js:158`), `exportCard`
      (`store.js:270`), `cardTemplate` (`store.js:336`).
- [x] Lock action: write the ephemeral group's id into each member's `groupId`
      via `store.updateCard` (`store.js:742`); unlock clears it.
- [x] Rebuild locked groups from shared `groupId` on load; edit-mode selection
      of a member activates the whole group.

## - [x] Sub-feature: Publish group

The group toolbar's publish button opens a **publishing modal** (name,
description, confirm). Simplicity goes into how the artifact is built: reuse
the existing workspace-archive machinery, filtered to the group.

- Build: call `build_workspace_archive`
  (`crates/web/src/domain/workspace_archive.rs:42`) with a synthetic
  `BoardWorkspace` containing only the group's cards (and only the packages
  those cards reference), producing a group `.sand` file.
- Import: parse with `parse_workspace_archive` (`workspace_archive.rs:92`) but
  — unlike the workspace import at
  `crates/web/src/presentation/http/api/board.rs:75-118`, which creates a brand
  new workspace with a fresh id/name — **merge the cards into the current
  workspace**: regenerate card ids, offset positions to the viewport center,
  persist referenced packages the same way, and assign all imported cards one
  fresh shared `groupId` so the grouping arrives locked.

- [x] Publishing modal wired to the group toolbar's publish button.
- [x] Server endpoint that filters `build_workspace_archive` to a card-id list
      and streams the group `.sand` (mirror the export handler above
      `import_workspace` in `api/board.rs`).
- [x] Group import path that merges into the **current** workspace (new card
      ids, fresh shared `groupId`), contrasted with `import_workspace`'s
      new-workspace behavior (`api/board.rs:75-118`).

---

# - [x] Feature 3: Sand ABI (event dispatcher/listener):

Sands need a way to talk to each other: an event dispatcher/listener ABI where
one sand emits a typed event (e.g. `recordClicked` carrying record info) and
other sands consume it (e.g. to parameterize their SQL for an SSE view query).
Settled decisions: fanout happens **in the host bridge, no server round-trip**,
and each sand is **configured with which events it listens to**.

## Host-side fanout

The bridge already fans host state out to every sand iframe: `render()` loops
`getFrames()` and posts to each frame
(`crates/web/static/presentation/board/widget-bridge.js:194-201`). The ABI adds
a new action to `handleAction` (`widget-bridge.js:204`) that re-broadcasts an
event message the same way:

```js
// widget-bridge.js — inside handleAction (widget-bridge.js:204)
if (action === "emit-event") {
  const eventMessage = {
    type: "lince:bridge-event",
    payload: {
      topic: String(message.payload?.topic || ""),
      data: message.payload?.data ?? null,
      sourceInstanceId: message.instanceId || "",
    },
  };
  for (const frame of getFrames()) {
    const instanceId = frame?.dataset?.packageInstanceId || "";
    if (instanceId === eventMessage.payload.sourceInstanceId) continue; // never echo back
    if (!frameListensTo(instanceId, eventMessage.payload.topic)) continue; // abi.listen check
    frame.contentWindow?.postMessage(eventMessage, "*");
  }
  return;
}
```

- [x] `emit-event` action in `handleAction` (`widget-bridge.js:204`) with the
      `getFrames()` fanout above; never echo back to the source frame.
- [x] `frameListensTo(instanceId, topic)`: host-side allowlist check backed by
      the per-card `abi.listen` config (below), consulted before delivery.

## Iframe API

`widget-frame-bootstrap.js` already re-emits host messages as DOM
`CustomEvent`s (helper at `widget-frame-bootstrap.js:62`, `lince-bridge-state`
emit at `:121`) and exposes `window.LinceWidgetHost` (`:134`). The ABI adds two
methods and one event type:

```js
// widget-frame-bootstrap.js — additions to window.LinceWidgetHost (:134)
emit(topic, data) {
  send(WIDGET_ACTION, { action: "emit-event", topic: String(topic || ""), data });
},
onEvent(topic, handler) {
  const listener = (event) => {
    if (event.detail?.topic === topic) handler(event.detail);
  };
  window.addEventListener("lince-bridge-event", listener);
  return () => window.removeEventListener("lince-bridge-event", listener);
},
```

plus a `message` case that turns incoming `lince:bridge-event` messages into
`emit("lince-bridge-event", payload)` DOM events, exactly like
`lince-bridge-state`.

- [x] `LinceWidgetHost.emit(topic, data)` and
      `LinceWidgetHost.onEvent(topic, handler)` in
      `widget-frame-bootstrap.js` (:134), with the `lince:bridge-event` →
      `lince-bridge-event` CustomEvent relay.

## Per-card listen configuration

"Configure if you want a sand to listen to X events or not": an `abi.listen`
allowlist of topics stored on the card (inside the card's persisted state,
alongside the existing widget config) and edited from the card's configure
modal. The host checks it in `frameListensTo` before delivering — sands never
see topics they were not subscribed to, so the check is enforcement, not
convention.

- [x] `abi.listen: string[]` stored per card and surfaced in the configure
      modal (checkbox list of known topics, free-text for custom ones).
- [x] Host delivery consults the allowlist (default: empty = listens to
      nothing).

## Event convention: `recordClicked`

First concrete topic. Payload carries enough for a listener to parameterize
its SQL and open an SSE view stream:

```json
{
  "serverId": "srv-…",
  "viewId": 42,
  "table": "records",
  "recordId": 1337,
  "record": { "id": 1337, "head": "…", "…": "…" }
}
```

The consumption pattern already exists in the Table sand: it builds
`/host/integrations/servers/{serverId}/views/{viewId}/stream`
(`crates/web/src/sand/table/script.rs:748-766`) and opens an `EventSource` on
it (`script.rs:2189`); the endpoints (`/stream`, `/snapshot`) are routed at
`crates/web/src/presentation/http/router.rs:109-120`.

- [x] Document `recordClicked` as the first ABI topic with the payload above.
- [x] Emit `recordClicked` from the Relations sand's `selectNode`
      (`crates/web/src/sand/relations/script.rs:2311-2331`) — selection is
      currently iframe-internal; add a `LinceWidgetHost.emit("recordClicked", …)`
      call when a node with record data is selected.

---

# - [x] Feature 4: Record Info sand:

A new built-in sand that displays whatever record was last clicked anywhere on
the board. It is the reference consumer of the ABI: no board-level hidden
field, no polling — it reacts to `recordClicked` events only.

Structure: `crates/web/src/sand/record_info/` with `mod.rs`, `body.rs`,
`script.rs`, `styles.rs`, registered in `crates/web/src/sand/mod.rs` exactly
like the other built-ins (module declaration + feature flag + builder entry;
see the `relations` registration at `sand/mod.rs:17-18` and `:178-179`).

UI modeled on the Relations sand's sidepanel (`renderPanel` at
`crates/web/src/sand/relations/script.rs:546`, `renderSelection` at `:890`):

- **Empty state**: the sand renders as a small "ball" — a compact circular
  badge, no chrome.
- **On `recordClicked`** (received via `LinceWidgetHost.onEvent("recordClicked", …)`,
  Feature 3): the ball expands into a sidepanel; the sand builds an SSE view
  query for that record — fetch the `/snapshot` first, then keep an
  `EventSource` open on `/stream`
  (same endpoints as the Table sand, `router.rs:109-120`; URL-building
  pattern at `sand/table/script.rs:748-766`) — and renders the record's
  fields live. This request pair is dedicated to the sand and kept separate
  from any other in-flight requests.
- **Close** collapses back to the ball; closing tears down the `EventSource`.

- [x] Scaffold `crates/web/src/sand/record_info/` (mod/body/script/styles) and
      register it in `sand/mod.rs` (pattern: `relations` at `:17-18`,
      `:178-179`).
- [x] Ball ↔ sidepanel states; sidepanel layout borrowed from
      `relations/script.rs` `renderPanel`/`renderSelection`.
- [x] Subscribe via `LinceWidgetHost.onEvent("recordClicked", …)`; ship with
      `abi.listen: ["recordClicked"]` as its default card config.
- [x] Snapshot + `EventSource` per received event, torn down on close or on
      the next event (one live stream at a time).
