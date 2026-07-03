# Plan: Restructure `docs/grouping-sand.md` into a per-feature design doc

## Context

`docs/grouping-sand.md` holds four terse bullet ideas (z-index ordering, grouping with lock and publish sub-features, an ABI-like event system between sands). **Note: the file exists only in the user's local working tree — it is not in the remote checkout — so this task writes it fresh.** The task is to rewrite it as an executable design document — **no implementation code in this task, only the doc** — where every implementable point becomes a todo title of the form `# - [ ] Feature N: Name:` followed by a description, code examples grounded in the real codebase, and further sub-todos.

Design decisions already settled with the user:

- **Z-order UI**: hover-toolbar buttons + keyboard shortcuts (Ctrl+]/[, Ctrl+Shift+]/[).
- **Marquee**: ctrl+left-drag in edit mode selects only **fully contained**, **unpinned world-space** cards.
- **Group resize**: proportional — positions and sizes scale with the group bounding box.
- **Locked groups**: invisible outside edit mode; persistence via the simplest model — a `group_id: Option<String>` field on `BoardCard` (presence = locked group; ephemeral groups stay frontend-memory only).
- **Publish group**: a publish button opens a publishing **modal**; artifact built by reusing the workspace-archive machinery filtered to the group's cards; on import the cards merge into the **current** workspace (no new workspace) with grouping preserved.
- **ABI**: event dispatcher/listener (e.g. `recordClicked` with record info so a listener can feed it into its SQL for an SSE view query); per-sand **configuration of which events it listens to**; broadcast fanout via the host bridge (no server round-trip).
- **Record Info sand**: a new built-in sand modeled on the Relations sand's sidepanel — a small "ball" when empty, expands into a sidepanel querying the DB via SSE when a `recordClicked` event arrives. No board-level hidden field.

All file/line anchors below were verified against the repo.

## Deliverable

Write `docs/grouping-sand.md` (only file changed) with four feature sections:

```
docs/grouping-sand.md
├── # - [ ] Feature 1: Z-index ordering        (frontend: store.js, main.js; render: pages/shared.rs)
├── # - [ ] Feature 2: Grouping system         (main.js, interactions.js, viewport.js, grid.js)
│   ├── - [ ] Lock sub-feature                 (domain/board.rs, board_state_store.rs, store.js, grid.js)
│   └── - [ ] Publish sub-feature              (domain/workspace_archive.rs, api/board.rs)
├── # - [ ] Feature 3: Sand ABI                (widget-bridge.js, widget-frame-bootstrap.js)
│   └── - [ ] Emit recordClicked from Relations (sand/relations/script.rs)
└── # - [ ] Feature 4: Record Info sand        (new crates/web/src/sand/record_info/, consumes Feature 3)
```

### `# - [ ] Feature 1: Z-index ordering`

- Commands bring-to-front / send-to-back / bring-forward / send-backward, operating within the card's layer band (unpinned defaults z=1, pinned z=50, system shell 90–95 — bands must be respected so reordering never crosses pinned/system cards).
- Implementation notes + JS example: a `reorderCard(cardId, direction)` store mutation that sorts same-layer workspace cards by `zIndex`, moves the card in the sorted order, reassigns compact z values, persists via the existing `commit()`/`persist()` path (`crates/web/static/presentation/board/store.js:419` `persist`, `:435` `commit`; mutation primitive `store.updateCard` at `store.js:742`).
- UI: four `data-card-action` buttons added to `renderCardToolbarContent` (`main.js:804`) + cases in `handleCardActionClick` (`main.js:5188`); keyboard shortcuts in the global keydown handler (`main.js:5703`).
- Gotchas as sub-todos: pin toggle currently resets `zIndex` (`main.js:5212-5231`); active-card override `max(zIndex,100)` (`main.js:2073-2077`); server-rendered initial styles include `z-index` (`crates/web/src/presentation/pages/shared.rs:81`).

### `# - [ ] Feature 2: Grouping system`

- Marquee: ctrl+pointerdown intercepted on `#board-canvas` in edit mode before panzoom (`boardViewport.setInteractionLocked`, `viewport.js:220`); screen-space selection rectangle overlay; on pointerup convert to world rect via `worldPointFromClient` (`viewport.js:101`) and select fully contained unpinned cards.
- Ephemeral group state in `main.js` (`{ id, cardIds }`), dissolved by Esc, clicking a non-member card, or leaving edit mode. Group outline element + group toolbar (move/resize handles, delete, pin, lock, publish).
- Group move/resize: generalize `interactions.js` (interaction currently tracks a single `cardId`, `interactions.js:144`) to a card-id set reusing the existing `baseCards` snapshot pattern (`interactions.js:147`); proportional resize math example mapping each card's rect through the bounding-box transform, then `clampCard` snapping (`grid.js:220`).
- Group delete (loop `store.removeCard`, single confirm modal) and group pin (apply the world↔screen conversion from the pin action at `main.js:5212-5231` per member).
- Sub-feature todos:
  - **Lock**: `group_id: Option<String>` + `#[serde(default)]` on `BoardCard` (`crates/web/src/domain/board.rs:5-44`; no schema-version bump so existing boards aren't wiped — `infrastructure/board_state_store.rs:51` resets to default state on version mismatch); mirrored in `sanitizeCard` (`grid.js:158`), `exportCard` (`store.js:270`), `cardTemplate` (`store.js:336`); locked groups rebuilt from shared `groupId` on load, zero footprint outside edit mode.
  - **Publish**: publish button opens a publishing modal; artifact built by filtering `build_workspace_archive` (`crates/web/src/domain/workspace_archive.rs:42`, parse at `:92`) to the group's cards, producing a group `.sand`; import merges into the **current** workspace preserving `groupId` (contrast: workspace import at `api/board.rs:75-118` creates a new workspace with a fresh id/name).

### `# - [ ] Feature 3: Sand ABI (event dispatcher/listener)`

- New bridge action `emit-event` in `handleAction` (`widget-bridge.js:204`) fanning out `{ type: "lince:bridge-event", payload: { topic, data, sourceInstanceId } }` to frames, mirroring the existing state fanout loop over `getFrames()` (`widget-bridge.js:194-201`); no server round-trip.
- Iframe API in `widget-frame-bootstrap.js`: `LinceWidgetHost.emit(topic, data)` + `LinceWidgetHost.onEvent(topic, handler)`, re-emitted as a DOM `CustomEvent` like the existing `lince-bridge-state` (`widget-frame-bootstrap.js:62` CustomEvent helper, `:121` emit, `:134` `LinceWidgetHost` surface).
- Per-card listen config: an `abi.listen` allowlist (stored in the card, edited from the configure modal) that the host checks before delivering a topic to a frame — "configure if you want a sand to listen to X events or not".
- Event convention example: `recordClicked` payload `{ serverId, viewId, table, recordId, record }` — enough for a listener to parameterize its SQL and open an SSE view stream (pattern: Table sand `buildStreamUrl` at `crates/web/src/sand/table/script.rs:748-766`, `EventSource` at `:2189`; endpoints `presentation/http/router.rs:109-120` — view `/stream` and `/snapshot`).
- Sub-todo: emit `recordClicked` from the Relations sand's `selectNode` (`crates/web/src/sand/relations/script.rs:2311-2331`), where selection is currently iframe-internal.

### `# - [ ] Feature 4: Record Info sand`

- New built-in sand `crates/web/src/sand/record_info/` (mod.rs/body.rs/script.rs/styles.rs), registered in `sand/mod.rs` like the other built-ins (feature flag + builder entry, see the `relations` registration at `sand/mod.rs:17-18,178-179`); UI modeled on the Relations sand sidepanel (`renderPanel` at `relations/script.rs:546`, `renderSelection` at `:890`).
- Behavior: empty state renders as a small ball; on `recordClicked` (via `LinceWidgetHost.onEvent`) it expands into a sidepanel, builds/executes an SSE view query for that record (snapshot + `EventSource` stream, kept separate from other requests per user guidance), and displays the record; collapses back on close.

## Execution steps

1. Write `docs/grouping-sand.md` with the four feature sections above (todo-title format `# - [ ] Feature N: Name:`, each with description + grounded code examples + sub-todos). Preserve the spirit of the original four bullet ideas as captured in Context.
2. No other files touched; no implementation code.

## Verification

- Re-read the final markdown checking: every implementable point is a `- [ ]` item, each feature has at least one grounded code example with real file/function references, and every settled design decision above is reflected.
- Spot-check that cited file paths/symbols exist (`grep -n` the referenced functions).
