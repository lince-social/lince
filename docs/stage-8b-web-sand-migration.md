# Stage 8b - Current Web/Tauri Migration to Protein + Actions

This is the running tracker for Part VII.4 of `docs/fable-improvement.md`.
The product target is the existing `crates/web` surface as launched by Tauri.
The goal is to refactor that surface in place so its board, bridge, and sands
use the new Cell database, Protein reads, typed Actions, one transport
WebSocket, and ephemeral lanes.

The prior experimental host proved useful backend and bridge ideas. Those ideas
are now inputs to the current web/Tauri refactor, not a replacement frontend.
When a feature works there, the task is to port the feature into `crates/web`
and then remove the duplicate/reference path once current web has parity.

Database direction: `~/.config/lince/lince.db` is now the new Cell schema.
The previous database was moved to `~/.config/lince/old_lince.db`. Current web
opens `lince.db` for the new `store`/`engine`/`transport` path.

## Rules

- Current web/Tauri remains the main user interface.
- Do not replace the mature board or sand UX with a parallel frontend.
- Move new capabilities into `crates/web` first, then delete duplicate
  reference code after parity is proven.
- Board chrome is frontend state; sand data is Cell state.
- Sands read through Protein and write through Actions.
- Keep legacy `/host/integrations/...` and SSE paths only until each sand no
  longer needs them.
- Use `cargo check`, not `cargo build`.
- Do not alter past migrations unless explicitly asked.

## Current Shape

The current web board keeps owning presentation state:

- canvas camera
- workspaces
- card position and size
- pinning
- z-index
- grouping
- edit mode
- per-card `widgetState`
- package/import/publish metadata

Those are not Ledger facts. They remain in the current board state store and
widget state.

The sand data plane is the part being replaced:

| Previous path | New path |
|---|---|
| `EventSource` saved-view stream | `LinceWidgetHost.subscribeProtein(id, protein, handler)` |
| `/host/integrations/.../table` create/update | `LinceWidgetHost.act(action)` |
| `serverId` / `viewId` as data identity | Protein AST for reads, Action payload for writes |
| sand-specific REST plumbing | shared transport WebSocket at `/host/transport/ws` |
| ad hoc provenance UI | `include: { facts: ... }` |

## Short Term

Short term means: take the already-proven Cell/Protein/Action/transport
features and make them run inside current web/Tauri.

- [x] Keep Tauri booting `web::serve_with_bound_addr_sender(..., FullUi, ...)`.
- [x] Point the new Cell data path at `~/.config/lince/lince.db`.
- [x] Add current-web transport endpoint at `/host/transport/ws`.
- [x] Add current-web bridge methods:
  `subscribeProtein`, `subscribeSaved`, and `act`.
- [x] Multiplex per-sand Protein subscriptions and Actions through the parent
  widget bridge.
- [x] Port `record_info` to prefer Protein with `include: facts`, while keeping
  the previous view stream as fallback.
- [x] Add `uid_eq` to Protein so current web sands can target one record
  directly instead of subscribing to a broad window and filtering client-side.
- [/] Move ABI events (`emit` / `onEvent`) from the in-page event bus onto
  transport ephemeral lanes, preserving the same sand-facing API. Each ABI
  topic is a lane room `abi:<topic>`; the board joins the rooms its cards
  listen to and emit on. Same-board siblings still fan out in-page (one board
  is one connection and the transport suppresses self-echo), while other
  sessions/devices receive the event over the lane. Transport substrate is
  covered by `transport/tests/session.rs::ephemeral_lanes_fan_out_and_never_persist`.
  REMAINING: the bridge relay itself has not been driven end-to-end (no JS
  runtime here) — needs the browser selftest, incl. the room churn path where
  emit-only rooms are left/rejoined across renders.
- [ ] Port the real current-web table sand to Protein/Actions without losing
  its current UX: drafts, schema selection, toasts, info panel, and LynxDS
  surface.
- [/] Add Actions required by table and editor parity:
  `edit-record-text`, `set-extension`, `set-slug`, `set-concept`, `set-unit`,
  undo/compensation, and delete/deactivate semantics. DONE: `edit-record-text`,
  `set-extension`, `set-slug`, `set-concept`, `set-unit` (engine `Action`
  variants + `store::records` setters + `engine/tests/record_edits.rs`, 5
  tests); each metadata edit drops a zero-delta annotation fact so live
  subscriptions refresh. `Deactivate` already covers delete/deactivate
  (append-only Ledger: delete == quantity→0). REMAINING: undo/compensation
  (compensating-fact reversal), landing with the record editor.
- [ ] Port kanban data plumbing to Protein/Actions while preserving current
  task metadata, categories, dates, estimates, assignees, comments, resource
  refs, worklogs, filters, and view settings.
- [ ] Port relations data plumbing to Protein/Actions while preserving current
  graph behavior, relation/category filters, edits, delete/deactivate behavior,
  and projection settings.
- [ ] Port todo with current-web parity: focus queue over Protein, create and
  complete via Actions, then restore undo/history/details behavior.
- [ ] Port transfer to Transfer Actions and promise/availability includes.
- [ ] Port home manager/dashboard to aggregate Proteins and Action writes.
- [ ] Port karma/rules surfaces to rule records, derived values, and rule
  Actions.
- [ ] Port trail/knowledge graph surfaces to records + links + concepts
  Proteins and package import.
- [ ] Build the record editor around `edit-record-text` and the CRDT relay.
- [ ] Keep embed-honest sands working: terminal, freedoom, document viewer,
  chess, and similar packages. When touched, preserve required vendored
  license/credit files.
- [ ] For every ported sand, add a driven test that proves snapshot, Action
  round-trip, and live update in current web, not only Rust compilation.
- [ ] Remove each legacy SSE/table-CRUD path only after the corresponding current
  web sand no longer uses it.

## Long Term

Long term means broader frontend and product work after the current web data
plane is on the new system.

- [ ] Host-state sync for board presentation state across devices.
- [ ] Import/publish package subsystem on top of the new record/package model.
- [ ] Per-sand capability model before imported sands can write arbitrary
  Actions.
- [ ] Sand provenance such as `cause=sand:<uid>` for Action writes.
- [ ] Shared asset/package layout after duplicate reference assets are removed.
- [ ] More complete browser selftests for pan, zoom, grouping, resize, pin,
  workspaces, import, publish, and sand-to-sand events.
- [ ] Frontend polish and redesign work that does not block the data-plane
  migration.
- [ ] New product surfaces: transfer marketplace, route/ride planning, group
  coordination, calls/chat, calendar/time budgeting, finance projections,
  social feed, and Fiote/AI conversation sands.
- [ ] Storage-engine independence beyond SQLite once Protein/Actions are the
  only sand-facing contract.

## Current Status

- [x] Current web/Tauri is restored as the product runtime.
- [x] Current web has a new transport route wired to `store`, `engine`, and
  `LaneHub`.
- [x] Current web bridge can relay Protein subscriptions and Actions.
- [x] `record_info` uses Protein/facts first and falls back to the prior view
  stream when needed.
- [x] The new database file is `lince.db`.
- [/] Sand migration is underway.
- [ ] Full current-web feature parity on the new data plane is not complete.

### Known unresolved: migration boot collision on `lince.db`

Verified (throwaway probe, not committed): with no `LINCE_DATA_DIR_OVERRIDE`,
both the legacy `persistence` layer (`utils::config::lince_data_dir()` =
`config_dir()/lince`, runs the root `migrations/`) and the new `store`
(`crates/web` opens `config_dir()/lince/lince.db`, runs
`crates/store/migrations/0001_init.sql`) open the **same file** `lince.db`
with **different** sqlx migration sets. Whichever boots first wins; the second
fails with e.g. `migration 20260625182502 was previously applied but is missing
in the resolved migrations`. The new cell path also silently bypasses the
`LINCE_DATA_DIR_OVERRIDE` that persistence honors.

This is not cleanly fixable yet: the file split (`old_lince.db`) is vetoed, and
removing the legacy persistence boot breaks the not-yet-ported sands that still
read/write the old schema through `services.db`. Resolution lands with the sand
ports — once every sand is on the new store, the legacy persistence boot (and
the root `migrations/`, kept only as reference) is deleted and `lince.db` is
owned solely by the new schema. Until then, running both paths against one file
requires either the split or an override; do not tick "boots the new schema
cleanly" while both boot.

Cutover rule: deleting legacy web data paths is allowed sand by sand after parity.
Deleting or replacing the current web/Tauri surface is out of scope.
