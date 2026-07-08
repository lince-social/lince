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
- [x] Port the real current-web table sand to Protein/Actions without losing
  its current UX: drafts, schema selection, toasts, info panel, and LynxDS
  surface. REBUILT fresh (feature parity, not pixel parity): the sand was a
  server-rendered datastar table on an SSE stream; it is now a client-rendered
  records table that subscribes to `{ source: record }` and writes via typed
  Actions — cell edits map by column (slug→`set-slug`, head/body→
  `edit-record-text`, quantity→`set-quantity`), Create→`create-record` (kind
  select + draft fields), delete→`deactivate`. Live updates, toasts, info/metrics
  panel preserved. Driven proof in headless chromium against a stubbed bridge:
  `scripts/other/table-sand-selftest.sh` (snapshot render + `source=record` +
  create-record & set-slug round-trip) — PASS. This sand is the fan-out TEMPLATE
  for the rest. DEFERRED (was in the old server table, not yet rebuilt): rich
  keyboard-grid navigation/caret editing, helix/common mode, the LynxDS "nerd"
  surface, and concept/unit inline editors.
- [x] Add Actions required by table and editor parity:
  `edit-record-text`, `set-extension`, `set-slug`, `set-concept`, `set-unit`,
  undo/compensation, and delete/deactivate semantics. `edit-record-text`,
  `set-extension`, `set-slug`, `set-concept`, `set-unit` are engine `Action`
  variants + `store::records` setters; each metadata edit drops a zero-delta
  annotation fact so live subscriptions refresh. `Deactivate` covers
  delete/deactivate (append-only Ledger: delete == quantity→0). `Compensate
  { fact }` is the undo primitive — appends the inverse delta caused by the
  original (`CauseKind::Compensation`), no-op on zero-delta facts
  (`store::facts::get`). Covered by `engine/tests/record_edits.rs`.
- [x] Add new-Cell record threads/messages for Record Info: threads and
  messages are ordinary records (`kind=thread`, `kind=message`) connected by
  Lingua link kinds (`thread-of`, `message-in`, `reply-to`). Actions
  `create-thread` and `create-message` create the records and links, Protein
  `include: { threads: ... }` reads nested message trees, and the Record Info
  sand can create threads plus root/reply messages for the clicked record.
- [/] Port kanban to the new core. Kanban is REBUILT fresh on the table-sand
  template (do **not** lift the ~3.7k-line legacy `crates/web/src/sand/kanban/
  script.rs`; read it for the feature list only). Track A (data port) is DONE;
  Track B (group infrastructure) is the remaining work. The work splits into two
  independent, separately-landable tracks: the **data port** (kanban reads/writes
  on Protein + Actions) and the **group infrastructure** (the user's headline
  "extra features" — kanban ships and imports as a *group of sands*). Land one
  increment with a driven selftest before starting the next; the data port is the
  safe first landing, the group infra is the novel/risky part.

  ### Track A — Kanban data port (Protein reads + Action writes) — DONE

  Landed: `crates/web/src/sand/kanban/{mod,body,styles,script}.rs` rebuilt fresh
  (the old 3.7k-line SSE sand replaced). Proven by
  `scripts/other/kanban-sand-selftest.sh` (headless chromium vs a stubbed bridge):
  columns bucket, default `source=record`, drag-move → `set-concept`, per-column
  add → `create-record`+`set-concept`, card click → `recordClicked` ABI, saved-
  Protein swap → `subscribeSaved`. Caveat: a full `cargo check -p lince-web` is
  currently blocked by unrelated pre-existing working-tree WIP (`ServerBootstrap`
  not imported in `web/src/lib.rs`); the Rust here mirrors the known-good table
  template field-for-field — re-run the crate check once that WIP compiles.

  - [x] Rebuild the kanban sand on the table template: subscribes a driving
    Protein, renders the board (columns + cards) client-side, writes via typed
    Actions only (move column → `set-concept` by default, extensible via a
    `COLUMN_ACTIONS` map that also covers `set-quantity`; edit title →
    `edit-record-text`; create card → `create-record` (+ classify); delete →
    `deactivate`). Manifest `requires_server = false`; `read_view_stream` dropped
    (permissions now `bridge_state`/`protein_subscribe`/`act`).
  - [x] Driven by a GENERAL, reusable Protein — not kanban-specific. Tolerant-
    ignore of extra included data (renders uid/head/slug/body + the column field,
    drops the rest). Honors the card's saved/inline Protein from the Data panel
    (`cardState.savedProtein` / `cardState.protein`), re-subscribing on
    `lince-bridge-state` — same pattern as table/todo.
  - [x] Column mapping: columns are the distinct values of one configurable record
    field (`cardState.kanban.columnField`, default `concept`) unioned with any
    configured `columns`, minus `hiddenColumns` ("hiding part of the kanban"). A
    card move rewrites that field via the mapped Action. COSMETIC GAP: when
    grouping by `concept` the column label is the concept *uid* (Protein returns
    `concept_uid`, not the name) — configured columns can supply friendly labels;
    resolving concept names in the Protein row is a follow-up.
  - [x] Old task metadata / categories / dates / estimates / assignees / comments /
    resource refs / worklogs / filters / view settings mapped below; gaps
    (dates/estimates/worklogs) documented, not invented, per user guidance.
  - [x] Driven selftest `scripts/other/kanban-sand-selftest.sh` — PASS (mirrors
    `table-sand-selftest.sh`).

  #### Old→new kanban field map

  Maps cleanly (implement):

  - **Task metadata** (title/description) → record `head` / `body`.
  - **Comments** → threads + messages (`kind=thread`/`kind=message`, link kinds
    `thread-of`/`message-in`/`reply-to`); Actions `create-thread`/`create-message`,
    Protein `include:{ threads }`. This is the already-`[x]` record-threads item.
  - **Filters** → Protein predicates (`kind_eq`, `concept_in`, `slug_eq`,
    `quantity_lt|gt|eq`, `state_in`, `near`) in the driving Protein's `where`.
  - **View settings** → the saved Protein (successor to a named SQL view) plus
    per-card board chrome in `widgetState` (host state, not Ledger).
  - **Categories** → Lingua concepts on the record (`concept`), filtered via
    `concept_in`. Columns can also group by concept.
  - **Assignees / resource refs** → Lingua **links** to person/resource records
    (link kinds e.g. `assigned-to`, `resource-of`); Protein `include:{ links:{ kind } }`.
    Link primitives exist; the specific kinds + the referenced person/resource
    records need seeding — implement the link plumbing, seed kinds as needed.

  GAPS — no native new-core home yet (document, don't invent tables now):

  - **Dates** (start/due) → no native date column on `record`. Park in
    `record_extension` (open-ended fds JSON) for now, or model as annotation
    facts later. GAP.
  - **Estimates** → no native estimate field (`quantity` is the delta cache, not a
    per-record estimate). Park in `record_extension`. GAP.
  - **Worklogs** → no native worklog/time-entry table. Candidate future model:
    facts carrying a time-concept delta on the record. GAP.

  Note: the `record_info` sand (Track B) is **Protein-first** — its
  `recordClicked` handler calls `openProtein(recordId, record)`, subscribing
  `{ source: record, where: [{ uid_eq }] , include:{ facts, threads } }` whenever
  the event payload carries `record.uid` (or `slug`); it only falls back to the
  legacy SSE `/snapshot`+`/stream` view path for legacy numeric ids. So a
  Protein-based kanban that emits `recordClicked` with `data.record = { uid, … }`
  drives record_info over Protein with no server view stream.

  ### Track B — Kanban group infrastructure (the "extra features")

  Kanban is the first sand to exercise these. They generalize the flat grouping
  system (ultraplan Feature 2, all `[x]`, single `groupId` per card) into nesting
  and sand-as-group packaging.

  - [ ] **Nested groups (groups within groups).** A group may contain groups.
    Disbanding/unlocking the OUTER group must NOT disband/unlock the inner groups —
    they survive as their own groups. (Current model is a single flat
    `groupId: Option<String>` per card; nesting needs a representation that a card/
    group can belong to a parent group while keeping its own inner group identity.)
  - [ ] **Sand-as-group packaging.** Shipping/importing a sand can ship a *group*
    of sub-sands with a relative layout + z-order, not a single card. Shipping
    kanban ships the group — the kanban board with its columns and kanban config
    (which parts are hidden) — but **not** the record-info side data (assignees,
    worklogs, estimates, filters, views).
  - [ ] **Kanban imports-as-group by default.** Out of the box a kanban sand is a
    group of two sands: the kanban board (bottom layer) and a `record_info` sand
    (upper z-index, **same size** as the kanban, record on top), the record_info
    mostly hidden until a card is clicked.
  - [ ] **Delete kanban's built-in record sidepanel.** The reusable `record_info`
    sand replaces it, so record-detail viewing is coded once and reused everywhere.
  - [ ] **Scoped `recordClicked` delivery.** Clicking a kanban card emits
    `recordClicked` that reaches ONLY the `record_info` sand in the SAME group
    (group-scoped ABI fanout, not board-wide), which then displays that record.
  - [ ] **Groupception.** A kanban group can nest inside another group (e.g. a todo
    sand + a kanban sand). Disbanding/unlocking the outer group releases the todo
    and kanban but PRESERVES kanban's internal grouping (its columns + config + its
    record_info sand). This is the concrete test case for nested groups above.
  - [ ] **INCOMPLETE — needs the user.** The source note trailed off mid-sentence:
    "make sure the sands can also show a …". Intent unknown; finish the thought
    before building. (Best guess to confirm: sands should also be able to *show a
    preview/collapsed state* of themselves, but do not build on a guess.)

- [ ] Port relations data plumbing to Protein/Actions while preserving current
  graph behavior, relation/category filters, edits, delete/deactivate behavior,
  and projection settings.
- [/] Port todo with current-web parity: focus queue over Protein, create and
  complete via Actions, then restore undo/history/details behavior. CURRENT:
  the current-web todo sand now defaults to the Protein focus queue
  (`quantity_lt 0`, `kind_eq plain`, `topo("before")`) and also honors the
  card's saved/inline Protein from the Data panel. Completion plus local
  undo/redo writes use typed `set-quantity` Actions through the widget bridge;
  the old server/view SSE stream and table PATCH path are removed from the sand.
  Driven proof against a stubbed bridge:
  `scripts/other/todo-sand-selftest.sh` (snapshot + focus Protein shape +
  set-quantity round-trip + driving-Protein swap) — PASS.
  REMAINING: create task UI/Action, richer history backed by compensation/facts,
  details parity, and a live-update browser path beyond the stubbed bridge.
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

### Migration boot: `store` owns `lince.db`, legacy owns `lince-legacy.db`

The collision (verified: with no `LINCE_DATA_DIR_OVERRIDE`, both the legacy
`persistence` layer and the new `store` opened the **same** `lince.db` with
**different** sqlx migration sets, so the second to boot failed with
`migration 20260625182502 was previously applied but is missing in the resolved
migrations`) is **resolved by a file split**:

- `store` (new Cell schema, `crates/store/migrations/0001_init.sql`) is the sole
  creator/owner of `lince.db`.
- `persistence` (legacy schema) now uses `lince-legacy.db`
  (`crates/persistence/src/connection.rs`). It keeps creating/migrating that
  file because it is still **load-bearing at boot** — local admin, active
  configuration, karma cache, views/collections all live in the legacy schema
  (`bootstrap_database` → `seed`, `ensure_local_admin_if_needed`,
  `configuration.get_active`, `refresh_karma_cache`). The legacy layer no
  longer creates `lince-legacy.db` if it is missing; it may still open/migrate
  an existing file while remaining old surfaces are removed or ported.
- Both files resolve through `utils::config::lince_data_dir()` now (the new cell
  path was updated to match), so they always sit side by side and both honor
  `LINCE_DATA_DIR_OVERRIDE` — the earlier bypass is gone.

Note: the pre-existing `lince-persistence` unit test
`embedded_migrations_create_structured_transfer_tables` fails on `dev`
independent of this split (the root `migrations/` no longer create the
`transfer_*`/`work_*` tables it asserts — refactor drift, not caused here).

Cutover rule: deleting legacy web data paths is allowed sand by sand after parity.
Deleting or replacing the current web/Tauri surface is out of scope.
