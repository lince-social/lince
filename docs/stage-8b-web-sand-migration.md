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
- [x] Move ABI events (`emit` / `onEvent`) from the in-page event bus onto
  transport ephemeral lanes, preserving the same sand-facing API. Each ABI
  topic is a lane room `abi:<topic>`; the board joins the rooms its cards
  listen to and emit on. Same-board siblings still fan out in-page (one board
  is one connection and the transport suppresses self-echo), while other
  sessions/devices receive the event over the lane. Transport substrate is
  covered by `transport/tests/session.rs::ephemeral_lanes_fan_out_and_never_persist`.
  The bridge relay in `widget-bridge.js` is now driven end-to-end in headless
  chromium against a stubbed WebSocket + fake frames by
  `scripts/other/abi-lane-selftest.sh` (PASS): in-page fan-out to listening
  siblings with self-echo suppression (source frame that itself listens is not
  echoed), lane mirror (`lane_send` on `abi:<topic>`), inbound `lane_event`
  delivery (feeding the exact recorded send payload back proves send/receive
  shapes can't drift), and the full room-churn cycle — join on listen, leave
  when no card listens (incl. emit-only rooms), and re-join on re-emit.
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
- [x] Port kanban to the new core. Kanban is REBUILT fresh on the table-sand
  template (do **not** lift the ~3.7k-line legacy `crates/web/src/sand/kanban/
  script.rs`; read it for the feature list only). Track A (data port) DONE;
  Track B (group infrastructure) DONE (2026-07-10): nested groups, sand-as-group
  packaging, and the add-as-group default all landed + selftested — kanban now
  adds from the catalog as a group (board + record_info), and the two sands emit/
  listen `recordClicked` over the unified bridge's group-scoped ABI room. See the
  Phase 3 note under Track B and "Base task 2" for details. Pending only the
  user's visual confirmation in the running app. The work splits into two
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
    - [ ] Human note: back then i wanted a feature to cluster records, so if i have a normal record and i want to cluster them with tags (independently of them being an organ record or command record), so like 'Tasks', or 'Project A'. Make sure that is possible, to filter with inluding and excluding in protein to only show 'Tasks' and 'Project A' or only show not 'Project B'.
      - RESOLVED → multi-valued via **Lingua** (`tag: []`). See **Track B, Phase 5**
        below for the full plan. Decision (user): a record carries *many* tags, not
        one. `record.concept_uid` is single-valued (`crates/store/migrations/
        0001_init.sql:11`), so tags are **links** (already multi-valued and first-
        class): a `tag`/`in-cluster` link kind from the record to a **cluster
        concept**. Include+exclude then composes with the existing boolean
        predicates, but needs one NEW filterable predicate (links are only surfaced
        via Protein `include` today, not `where`).
  - **Assignees / resource refs** → Lingua **links** to person/resource records
    (link kinds e.g. `assigned-to`, `resource-of`); Protein `include:{ links:{ kind } }`.
    Link primitives exist; the specific kinds + the referenced person/resource
    records need seeding — implement the link plumbing, seed kinds as needed.

  GAPS — no native new-core home yet (document, don't invent tables now):
  For now skip it. I'll ask you later.

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
  - [ ] Make sure we dont fall back to legacy, refactor if you need to.
    - RESOLVED → READY TO BUILD (no infra needed). In `crates/web/src/sand/
      record_info/script.rs`, delete the legacy SSE path: `openStream`,
      `loadSnapshot`, `streamBase`, `resolveOrigin`, `findRecordRow`, the
      `EventSource` machinery, and the `else` fallback at ~509–512; keep only
      `openProtein`. Guard before deleting: confirm no record is reachable *only* by
      a legacy numeric id (`openProtein` bails on `legacy-id`). ADDITIVE-safe: this
      removes only the *sand's client fallback*, not any server SSE endpoint. See
      **Track B, Phase 1**.

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
      - [ ] Do a refactoring of all the sands, i want them inside rust, so we can make a build process that creates the group and reuse components between sands (like lego, we have the kanban sand, the record info sand and both of them in a group that makes it when i click in a card in kanban it events it to record info), we still maintain the embedding and spitting in lince/web/sand on startup. The old web stuff still has examples in the web crate, look at how we did.
        - RESOLVED (user decision) → **Rust `OFFICIAL_WIDGETS` build path is canonical.**
          There are two embed subsystems today, both writing to `~/.config/lince/web/
          sand/`: (a) the mature Rust path — ~30 sands as maud `mod/body/styles/
          script.rs`, registered in `OFFICIAL_WIDGETS` (`crates/web/src/sand/mod.rs:
          102-227`) and emitted by `render_official_widgets` (`:236-251`); (b) a newer
          `cell_surface.rs` path — `include_dir!` over `cell-sand-src/sands/` holding 5
          thin `.html` sands (`kanban/record_info/relations/table/todo`, 100–170 lines
          each) + an `example-bundle/` directory-bundle (`index.html` + `manifest.toml`
          {title,description,entry} + assets = an unpacked `.lince`). **Fold the 5
          `.html` sands' content INTO their Rust namesakes and make bundling/grouping a
          Rust build step — WITHOUT losing the 5 sands' content** (acceptance bar). The
          Rust versions are far richer (364–1469 lines), so this is mostly: port any
          Cell-transport wiring the `.html` has that Rust lacks, then delete the
          duplicate `.html`. See **Track B, Phase 2**. NOTE: `cargo check -p lince-web`
          is currently red (`ServerBootstrap` unimported in `crates/web/src/lib.rs:306,
          319`, pre-existing Cell-cutover WIP) — **Phase 0** clears that first.
  - [x] **Kanban imports-as-group by default.** DONE (2026-07-10). Adding "Kanban"
    from the catalog yields the group: kanban board (z=1) + `record_info` (z=2, same
    rect, record on top), mostly hidden until a card is clicked. Server surfaces the
    group as the "Kanban" catalog entry; `store.addImportedGroup` drops both. See
    "Base task 2".
  - [x] **Delete kanban's built-in record sidepanel.** DONE (Track A rebuild ships no
    sidepanel; the reusable `record_info` sand is the detail view).
  - [x] **Scoped `recordClicked` delivery.** DONE (2026-07-10). kanban's `kanban.html`
    emits `H.emit("recordClicked", {record})` on card click; `record_info.html`
    `joinRoom`s + `onLane`s it and re-subscribes focused on that record's `uid_eq`.
    The unified bridge (base task 1) delivers it in-page, group-scoped — only the
    record_info packaged in the SAME group receives it. Bridge mechanism proven by
    `bridge-unification-selftest.sh`; sands wired + served (verified via curl).
  - [ ] **Groupception.** A kanban group can nest inside another group (e.g. a todo
    sand + a kanban sand). Disbanding/unlocking the outer group releases the todo
    and kanban but PRESERVES kanban's internal grouping. The data model + logic are
    DONE & tested (Phase 3: nested `group_ids`, stack-aware lock/unlock, `import_group`
    prepends the outer group; `store.addImportedGroup` re-homes to a fresh inner id so
    inner grouping survives an outer disband). The specific todo+kanban nesting is
    exercisable in-app; not separately selftested here.
  - [ ] **DEFERRED (user).** The source note trailed off mid-sentence: "make sure the
    sands can also show a …". Best guess was a *preview/collapsed state* of a sand;
    user chose to defer — do not build in this pass. Revisit when the user finishes
    the thought.

  ### Track B — implementation plan (decisions resolved)

  Land + selftest each phase separately (driven headless-chromium selftests mirroring
  `scripts/other/kanban-sand-selftest.sh`). `cargo check -p lince-web` green after each.
  Phase 3 is the novel/risky core. Tick tracker boxes only for implemented + selftested
  items.

  - [ ] **Phase 0 — restore a compiling baseline (unblocker).** Fix `crates/web/src/
    lib.rs` so the crate compiles: import `ServerBootstrap` (exists in `domain::board`)
    or revert the incomplete WIP hunk at `:306,319`. Do the minimal thing that
    compiles — do NOT try to finish the broader stalled Cell cutover. Verify `cargo
    check -p lince-web` green + `kanban-sand-selftest.sh` still PASS.

  - [x] **Phase 1 — ready-now cleanups (no new infra).** DONE.
    - [x] Kill record_info's legacy SSE fallback: deleted `openStream`/`loadSnapshot`/
      `streamBase`/`resolveOrigin`/`findRecordRow`/`EventSource` + the `else` fallback
      in `record_info/script.rs`; kept `openProtein`. Guard verified: kanban is the
      only `recordClicked` emitter and always sends `record.uid`. Selftest
      `scripts/other/record-info-sand-selftest.sh` — PASS (SRC=record,
      WHERE=[{uid_eq}], PANEL=yes, ES=0).
    - [x] Delete kanban's built-in sidepanel — already satisfied by the Track A
      rebuild (new kanban ships no sidepanel). No code; box closed here.

  - [x] **Phase 2b — New-way connect path is the default** (user: "adhere completely
    to the new mode"). The 5 migrated sands load `/board/frame.js` and call
    `H.subscribeProtein`/`H.act`/`H.onLive`. Implemented:
    - [x] Restored the membrane pair into `crates/web/static/presentation/board/`:
      `frame.js` (sand-side host → `window.LinceWidgetHost`) + `bridge.js` (board-side,
      one WebSocket, multiplexes Protein subs + Actions). `bridge.js` ws URL repointed
      to `/host/transport/ws`. Served: `/board/frame.js` route on both routers +
      `bridge.js` in `static_assets`.
    - [x] `main.js` instantiates `createBridge({})` (self-wires via global message +
      `lince:ready`). `enhancePackageHtml` no longer injects the legacy bootstrap for
      sands that load `/board/frame.js`. `frame.js` reads the board's
      `data-package-instance-id` so per-card routing works.
    - [x] Wire protocol VERIFIED statically against the known-good `widget-bridge.js`:
      bare `new WebSocket` (no auth handshake), identical envelope
      (`{type:subscribe,id,protein}` / `{type:act,id,action}` out; `{id,rows}` /
      `{action_ok,created,facts}` back). So the data plane genuinely traverses
      frame→bridge→server→back. `frame.js`/`bridge.js` load in chromium exposing the
      expected API. (Live rendering in a real browser vs a running server still app-gated.)
    - [x] **Retired every old-way sand from wiring** (kept files, per user): `OFFICIAL_
      WIDGETS` reduced 31 → 13 (8 `shell` chrome + the 5 new sands). The 18 old sands'
      modules/sources remain on disk under `#[allow(dead_code)]`; not built/emitted/
      served. Seed workspace only places `shell-*` cards, so nothing strands on boot.
    - CAVEATS: (1) RESOLVED (2026-07-10, base task 1) — the two WebSockets are now
      ONE. See "Base task 1" below. (2) `recordClicked`/kanban-as-group interaction:
      the bridge substrate now exists (scoped room fan-out for new sands), still
      needs the kanban/record_info sands to emit/listen (kanban Track B). (3) ABI
      lanes: the unified bridge handles BOTH legacy `abi:<topic>` rooms and new-way
      arbitrary rooms, dispatched by the server's `lane_event.room`.

  - [x] **Base task 1 — merge the two WebSockets into ONE (full bridge
    unification)** (2026-07-10, user chose full unification over a minimal socket-
    share). The board opened up to three sockets to `/host/transport/ws`
    (`bridge.js` new sands + `widget-bridge.js` chrome + `protein-config.js` Data
    panel). Now:
    - [x] New `static/presentation/board/transport.js`: the single shared,
      reconnecting socket. Exposes `send`/`onMessage`/`onOpen`/`onLive`; consumers
      filter inbound by subscription/request `id` (which never collide across
      consumers) and lane `room`. Served via `static_assets.rs` + ServeDir.
    - [x] `widget-bridge.js` is now the ONE unified bridge: it serves BOTH the
      legacy nested-`payload` chrome protocol AND the new-way flat protocol
      (`frame.js`: `lince:ready`/flat `protein-subscribe`/`lince:action`/`lane-join`/
      `lane-send`), routing rows/action-results back in each frame's shape. It adds
      in-page, GROUP-SCOPED ABI fan-out for new-way sands (room-membership map +
      `inEventScope`) — the exact path kanban's scoped `recordClicked` needs — plus
      the server lane mirror for cross-session. `bridge.js` DELETED; `main.js`
      instantiates only the unified bridge; `protein-config.js` shares the socket.
    - [x] Driven proof (headless chromium, stubbed socket + fake frames):
      `scripts/other/abi-lane-selftest.sh` (legacy topic ABI still intact through the
      unified bridge + shared transport — PASS, updated to bundle transport.js and to
      feed the server's real room-tagged `lane_event`) and NEW
      `scripts/other/bridge-unification-selftest.sh` (one socket; flat subscribe→rows
      and action→result in the FLAT shape; scoped emit reaches only the same-group
      sibling, blocks a different-group sand, suppresses the source; lane mirror;
      ungrouped source broadcasts — PASS). `protein-config-selftest.sh` updated to
      copy transport.js — PASS.

  - [x] **Phase 2 — Rust-canonical sand build; the 5 new sands ARE their `.html`
    strings** (line-222 decision, redone per user's reorg). The user moved each new
    `.html` into its sand dir and deleted `cell-sand-src`. Implemented:
    - [x] Each of `kanban/record_info/table/todo/relations` is now a self-contained
      `.html` string: `mod.rs` does `include_str!("<name>.html")` + a Rust
      `PackageManifest` → `LincePackage::new(".html")` (Html transport). Old
      `body.rs`/`script.rs`/`styles.rs` DELETED; `OFFICIAL_WIDGETS` entries switched
      to `Package{package}`. `relations` drops the old d3 `graph_view` archive (the
      shared `d3.v7.min.js`/`LICENSE.txt`, still used by karma_orchestra/transfer,
      were kept). All 31 official packages build; `kanban.html` emits as a raw servable
      HTML doc that talks to `window.LinceWidgetHost` (the board injects the host
      bridge via `enhancePackageHtml`; the `/board/frame.js` tag is a harmless 404).
    - [x] `cell_surface.rs` `include_dir!` extraction RETIRED (its `cell-sand-src` tree
      is gone). `serve_cell_api_only` now populates `/sand/*` via
      `sand::render_official_widgets` + `render_official_groups`;
      `cell_surface::guess_content_type` kept for the route.
    - [x] Groups/bundles ship as `.lince`: `render_official_groups` emits
      `kanban.lince` (a workspace archive = kanban board + record_info, shared inner
      group). Catalog `list()` skips workspace archives via a new content peek
      (`is_workspace_archive_bytes`, checks for `workspace.json`) so a group `.lince`
      is never mis-parsed as a single sand; `is_workspace_archive_filename` accepts
      `.lince`. Roundtrip test updated + PASS; runtime emit test confirms
      `kanban.html` raw + `kanban.lince` group in the sand dir.
    - Incidental WIP fixes to unbreak the crate (the reorg left it non-compiling):
      restored tracked `crates/web/src/presentation/mod.rs` (deleted in worktree but
      still used) and removed the orphaned `pub mod colorscheme;` (no file, no uses).
      Deleted the obsolete `kanban`/`record-info` selftests (they extracted JS from the
      now-deleted `script.rs`).
    - [x] DONE (2026-07-10, base task 3): `example-bundle/` is now wired as an official
      widget. New `crates/web/src/sand/example-bundle/mod.rs` embeds its files
      (`index.html` entry + `script.js`/`style.css` assets) and builds
      `example-bundle.lince` via `LincePackage::new_archive`; registered in
      `OFFICIAL_WIDGETS` (14 entries). It emits to the sand dir at boot, is a SINGLE
      archive package (no `workspace.json`, so `is_workspace_archive_bytes` does not
      exclude it), shows in the catalog, and its assets serve relative to the entry.
      It is the reference multi-file directory-bundle format template.
    - [ ] REMAINING (unrelated to example-bundle): client drag/drop of a `.lince` GROUP
      still routes by extension (`isGroupArchiveFile` checks `.group.sand`), so
      surfacing `.lince` GROUPS in the catalog/import UI is the app-run wiring under
      Phase 3 / base task 2 below.

  - [/] **Phase 3 — nested groups + sand-as-group packaging** (Track B core). Data
    model + logic + build artifact DONE & tested; catalog-UI surfacing is the one
    remaining wiring step (needs the running app).
    - [x] Nested-group representation: ADDED `group_ids: Vec<String>` (outermost →
      innermost) to `BoardCard` (`crates/web/src/domain/board.rs`), authoritative for
      nesting; `group_id` kept in sync as the innermost id for flat-group back-compat
      (less invasive than a hard replace). Pure logic in `group-logic.js`:
      `groupStackOf`/`wrapInGroup`/`disbandGroup`/`sharesGroup`/`inner|outermostGroupId`.
      Persistence round-trips `groupIds` (`store.js` `exportCard`, `grid.js`
      `sanitizeCard`). Interactive lock/unlock is stack-aware (`store.js`
      `setCardsGroup`: lock prepends outer, unlock peels only the outer;
      `main.js` `activateGroupFromCard` activates the OUTERMOST container). Verified:
      `board_js_tests.rs::nested_groups_disband_outer_preserves_inner` (node/CI) +
      chromium replicas of the disband, scope, and lock/unlock truth tables — all PASS.
    - [x] Sand-as-group build artifact: `sand::build_kanban_group_archive` +
      `render_official_groups` (`crates/web/src/sand/mod.rs`) build a `.group.sand`
      workspace archive with 2 cards + both packages embedded; emitted at startup via
      `PackageCatalogStore::new`. Reuses the runtime `import_group`
      (`api/board.rs`) — which now PREPENDS the import group as the outermost, so inner
      grouping survives (groupception). Roundtrip test
      `sand::group_tests::kanban_ships_as_a_group_of_board_plus_record_info` — PASS.
    - [x] Kanban-as-group archive BUILT + tested: [kanban board (z=1) + record_info
      (z=2, **same rect**, `abi_listen:["recordClicked"]`)] sharing one inner group;
      ships board+config, not record-info side data.
    - [x] DONE (2026-07-10, base task 2 / kanban B — user chose add-as-group by
      default). Kanban now adds as a GROUP from the catalog:
      - **Server (cell path):** `InstalledPackageSummary` gains `is_group` +
        `member_count`; `PackageCatalogStore::list()` surfaces each workspace-archive
        `.lince` as ONE `is_group` entry (`summary_from_group`, title/metadata from the
        workspace name + lowest-z "primary" member). A group named the same as a single
        sand REPLACES it (kanban.lince id "kanban" hides kanban.html) — so "Kanban" IS
        the group; reusable members (record_info) keep their own single entry. New
        `GET /host/packages/local/group/{filename}` returns the parsed member cards
        (relative layout, z, group ids, ABI listen, HTML). Curl-verified: kanban entry
        `isGroup:true memberCount:2`, kanban.html suppressed, endpoint returns
        kanban+record_info with `recordClicked`.
      - **Client:** `store.addImportedGroup` drops all members at once, preserving
        relative layout/z/ABI-listen and re-homing the archive's inner group id to a
        FRESH id (repeated adds = independent groups; inner grouping survives outer
        disband = groupception). `addLocalPackageToWorkspace` branches on `isGroup` →
        `addLocalGroupToWorkspace` (fetch cards → `addImportedGroup`); catalog card
        shows a "grupo · N sands" pill. Driven proof:
        `scripts/other/group-add-selftest.sh` (headless chromium, store+grid+group-logic
        as ES modules) — PASS (two members, fresh unique ids, one shared fresh inner
        group, same rect, z-order, recordClicked preserved, repositioned off the archive
        coords, second add independent).
      - **Sands wired + verified end-to-end (node-free):** `kanban.html` emits
        `recordClicked` on card click; `record_info.html` `joinRoom`s/`onLane`s it and
        re-subscribes focused (`uid_eq`). Proven by the COMPOSED selftest
        `scripts/other/kanban-group-e2e-selftest.sh` — real srcdoc-less iframes running
        the REAL served `kanban.html`/`record_info.html` + REAL `frame.js` + REAL
        unified bridge (only the WebSocket stubbed): a real kanban card click drives the
        same-group record_info to re-subscribe on the clicked uid, while a
        DIFFERENT-group record_info does NOT (proves frame.js's instanceId lines up with
        the id `getCardGroupStack` is keyed on, so scoping holds and does not degrade to
        a board-wide broadcast). PASS.
      - PENDING only the user's visual confirmation in the running app: click "Kanban"
        in the catalog modal → two grouped cards appear → click a card → record_info
        panel updates. (The functional composition above is already proven; this is the
        pixels-and-catalog-modal layer.)

  - [x] **Phase 4 — scoped `recordClicked` delivery.** DONE. Added `inEventScope` +
    a `getCardGroupStack` accessor to `widget-bridge.js`: a GROUPED source only reaches
    frames whose stack contains the source's **innermost** (tightest) group — so a
    kanban card's `recordClicked` reaches only the record_info packaged with it, not a
    record_info in another group nor an unrelated sand sharing only an outer container.
    An UNGROUPED source still broadcasts board-wide (back-compat). `main.js` provides
    `getCardGroupStack`. Verified via chromium truth table (kanban→own record_info ✓,
    kanban→other-group record_info ✗, kanban→outer-only sibling ✗, ungrouped→broadcast ✓).
    Cross-session lane-room group qualifier deferred (local in-page scoping covers the
    kanban+record_info case).

  - [x] **Phase 5 — Lingua multi-tag clustering** (line-180 decision). DONE (built on
    the WIP's link infrastructure).
    - [x] Model: tags are **links** of a `tag` kind from a record to a **cluster
      record** (`tag: []` = the set of linked targets). Multi-valued; `record.
      concept_uid` stays the single classification, tags are orthogonal. Write path
      already exists in the WIP (`AddLink`/`RemoveLink`); links surfaced via
      `LinksInclude`.
    - [x] New core work — the filterable predicate. Added
      `Predicate::LinkedTo { kind, to }` to `crates/protein/src/lib.rs` (enum +
      `PredicateCtx.link_sources` preload via `store::links::records_to` + match arm).
      General over link kinds (also covers assignees `LinkedTo{assigned-to,…}`).
      Include+exclude composes with `Any`/`Not`/`All`. Wire form:
      `{ "linked_to": { "kind": "tag", "to": "tasks" } }`.
    - [x] Test `crates/protein/tests/features.rs::
      linked_to_filters_by_tag_cluster_with_include_and_exclude` — PASS. Verifies a
      multi-tagged record + `All([Any([tasks,project-a]), Not(project-b)])` returns
      exactly the right set. Full protein suite green (10 tests).
    - Kanban/table filter-by-tag works TODAY via the driving Protein (Data panel) — no
      sand code needed. Column-group-BY-tag is DEFERRED: a card can be in many clusters
      → many columns, which is semantically odd for a board; revisit if wanted.

- [x] Merge Relations and Trail into one **Relation** sand on Protein/Actions.
  Relation is the default graph sand for showing records as nodes and links as
  typed relations. The old Trail sand is not a separate concept anymore; it is a
  mode of Relation where one configured order-like link kind is interpreted as a
  chain.
  - [x] **Correct primitive:** relation edges are `link` rows. A statement like
    `Task A @order Task B` is stored as `link.from_uid = Task A`,
    `link.kind_uid = @order`, `link.to_uid = Task B`. The right-side record is
    the next/root-facing node for chain traversal when the configured mode wants
    to read left-to-right as `left @link right`.
  - [x] **Protein controls edge types:** the card config stores either an inline
    Protein or saved Protein. Relation starts with zero edges; every link type
    to show must be explicit in the card/Protein config. Protein uses
    `include:{ links:{ kinds, direction, depth } }` and `order:[{topo:kind}]`
    where chain order is required. Legacy one-kind `{ kind }` still parses.
  - [x] **Multi-edge rendering:** if two records have multiple selected links,
    Relation can render either one collapsed line or all relation fibers. In
    fiber mode, one edge is straight only when the edge count is odd; remaining
    edges are paired as curved parenthesis-like arcs around the center. When the
    count is even, no center line exists; half curve to one side and half to the
    other. Labels/tooltips expose each link kind and quantity.
  - [x] **Graph mode:** default mode renders selected Protein records as nodes
    and explicit selected links as relation fibers. Reads use Protein and writes
    use Actions; the old SQL view filters/projection settings are not part of
    the new Relation data path.
  - [x] **Trail mode:** chain mode uses the same ordering solution as the Todo
    focus queue: order is links, not special trail rows. The Protein result set
    is sequenced with `topo(@precedes)`/`topo(@order)` and then field tie-breaks
    such as `created_at` or promise windows. `relink-order` remains the drag
    reorder Action.
  - [x] **Trail visual rules:** in trail mode, the first negative record in the
    chain is the immediate need and renders orange. Later negative descendants
    are dim orange while blocked by the current immediate need. When the current
    node is made positive, it becomes green and the next child/descendant step is
    promoted to orange; if that promoted child was not already negative, Relation
    writes `set-quantity` to `-1` through Actions. Non-current descendants stay
    gray/dim until their turn.
  - [x] **No separate Trail package:** old Trail package/routes/services were
    removed. New cards instantiate the Relation sand with `mode: "trail"` and a
    configured order link kind; old Trail data compatibility is intentionally
    not preserved.
    Here is the plan that was thought of later after making more decisions, use it if you need to finish the task and these points are not implemented.

  # Relation + Trail Refactor Plan

  ## Summary

  Merge Trail into Relation as trail mode, with no standalone Trail package/service/data compatibility. Relation becomes a pure Protein/Actions sand: Protein controls which records and link
  kinds are visible, Actions mutate records, quantities, and links. Existing Trail-specific abstractions can be removed; old DB/data compatibility is not required.

  ## Key Changes

  - Extend Protein link includes from single-kind links to explicit selected kinds:
      - New shape: include: { links: { kinds: ["before", "contributes"], direction: "both", depth: 0 } }.
      - No implicit edges: if kinds is empty/missing, Relation shows records with zero links.
      - Keep parsing legacy { links: { kind: "before" } } only as a harmless alias for one kind.
      - Link rows returned to sands include enough graph data: uid, from, to, kind, quantity, direction, other.

  - Refactor Relation sand:
      - Replace old SSE /host/widgets/.../stream data path with LinceWidgetHost.subscribeProtein.
      - Replace old postAction("set-need")/SQL-view writes with bridge.act.
      - Card state owns Relation config:
          - mode: "graph" | "trail"
          - protein or savedProtein
          - linkKinds: []
          - edgeRender: "fibers" | "collapsed" default "fibers"
          - trail.orderKind
          - trail.direction: "from_to" | "to_from" default "from_to"

      - Graph mode renders only configured link kinds; with multiple links between two records, default to curved fiber edges, with a collapsed-line toggle.

  - Implement Relation trail mode:
      - UI label remains “Trail mode”, but it is only a Relation mode, not a separate package/concept.
      - Requires explicit trail.orderKind; no fallback to before/order/precedes.
      - Default order reads A @order B as A then B; per-card direction can reverse to read B then A.
      - Uses Protein order: [{ topo: orderKind }, ...tieBreakers].
      - Visual quantity rules:
          - first negative record in ordered chain is current and orange;
          - positive records are green;
          - later negative records blocked by current are dim orange;
          - non-current descendants are gray/dim;
          - completing current writes set-quantity; if the promoted next node is not negative, write set-quantity to -1.

  - Actions and cleanup:
      - Use existing add-link, remove-link, set-quantity, edit-record-text, deactivate.
      - Add relink-order Action for drag reorder: it rewrites adjacent order links for the configured kind over the provided ordered UID list.
      - Remove standalone Trail package registration, Trail routes/pages/services, and Trail widget identity checks after Relation trail mode is working.
      - Update docs to say Trail is only Relation’s ordered mode; no separate Trail data compatibility is promised.

  ## Test Plan

  - Protein tests:
      - multiple selected link kinds are included;
      - empty/missing kinds returns no links;
      - legacy single kind still parses;
      - topo ordering still works with selected order kind.

  - Engine/transport tests:
      - relink-order creates adjacent order links and removes stale order links inside the reordered set;
      - Relation-style subscribe plus set-quantity live update promotes the next trail item;
      - add-link/remove-link produce live Protein updates.

  - Web sand tests:
      - Relation graph subscribes through subscribeProtein, not old stream APIs;
      - graph with no link kinds shows nodes and zero edges;
      - multiple links render as fibers by default and collapse when toggled;
      - Trail mode setup state appears when no orderKind is configured;
      - Trail forward and reverse directions produce opposite chain order;
      - old Trail package no longer appears in official package list.

  - Verification:
      - Run timeout 180s nix develop -c cargo check --workspace --all-targets.
      - Add or update a Relation sand selftest script following the existing table/todo/kanban selftest pattern.

  ## Assumptions

  - It is acceptable to lose old standalone Trail card/data compatibility.
  - Existing local DB can be removed/reset during development if old Trail/Relation state blocks the new model.
  - “Trail” remains a UI mode name inside Relation, but there is no standalone Trail package, service, route, or backend concept.
  - Graph mode starts with zero edges unless link kinds are explicitly configured in Protein/card state.   

END_OF_RELATION/TRAIL_REFACTOR_PLAN

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
- [x] Remove the standalone Trail implementation after Relation trail mode has a
  driven test and current cards can migrate to Relation config.
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

- [x] TEST INFRA (2026-07-10): node is permanently unavailable in this
  environment, so the `board_js_tests` (which shell out to `node`) now SKIP
  cleanly via a `node_available()` guard instead of panicking at
  `expect("failed to launch node")` — `cargo test -p lince-web` is meaningful
  again (46 passed). The same board JS logic is covered node-free by chromium
  selftests under `scripts/other/`. Remaining `cargo test` failures
  (`transfer_widget` ×2, `terminal_store` ×1) are pre-existing WIP / PTY-env,
  unrelated to this work. All new selftests use chromium, never node.
- [x] FIX (2026-07-11): kanban group layout + stale-JS cache.
  - **Layout:** the group stacked `record_info` at the SAME rect on top of the
    kanban board — but an opaque iframe there just HID the board, so adding kanban
    showed only record_info's "Provenance" panel and no visible group. Now
    `build_kanban_group_archive` places record_info BESIDE the board (to its
    right, 320-wide); both are visible, the board stays usable, and clicking a
    card focuses record_info via the scoped `recordClicked`. record_info now shows
    a placeholder ("clique num card…") by default and only fills in on a click
    (it is an event-consumer sand). Group test updated.
  - **Stale JS:** the board JS is served via ServeDir with only `Last-Modified`
    (no `Cache-Control`), so the Tauri webview heuristically cached a stale
    `store.js` next to a fresh `main.js` → "store.addImportedGroup is not a
    function". Added `Cache-Control: no-cache, must-revalidate` app-wide
    (`map_response` layer) + on the embedded `static_assets` path.
- [x] BUGFIX (2026-07-10): clicking a sand in the catalog did not add it. The
  client add-flow fetches `GET /host/packages/local/{id}` for the single-package
  preview, but the cell path (`serve_cell_api_only`) had only `list` + `group` +
  `content` routes — never the single-package route (the catalog was empty before
  the fix above, so it was never exercised). Added `get_local_package` (returns
  the sand's HTML + manifest as the snake_case preview the client's
  `createCardFromPreview` expects) at `/host/packages/local/{package_id}`. Groups
  keep going through `/host/packages/local/group/{filename}`.
- [x] BUGFIX (2026-07-10): the "Catálogo de widgets" was empty on the desktop.
  The desktop boots `serve_cell_api_only`, which stubbed `/host/packages/local`
  to `list_empty_authed` (`[]`), so no on-disk sand ever showed. Wired it to a
  real `list_local_packages` handler backed by a `PackageCatalogStore` field on
  `CellApiState` (mirrors the FullUi handler). Verified against a live headless
  boot: the endpoint now returns all 13 sands, `kanban.lince` correctly excluded.
  Also removed the stale `record_info.html` orphan (underscore duplicate the
  retired `cell-sand-src` extractor left in `~/.config/lince/web/sand/`).
- [x] Current web/Tauri is restored as the product runtime.
- [x] Current web has a new transport route wired to `store`, `engine`, and
  `LaneHub`.
- [x] Current web bridge can relay Protein subscriptions and Actions.
- [x] `record_info` uses Protein/facts first and falls back to the prior view
  stream when needed.
- [x] The new database file is `lince.db`.
- [/] Sand migration is underway.
- [ ] Full current-web feature parity on the new data plane is not complete.

### Migration boot — RESOLVED 2026-07-11: the legacy layer is DELETED

The split below is history. `crates/persistence`, `persistence-table-derive`,
`injection`, `application`, `domain`, `tui`, and `gui` were removed from the
workspace; `lince-legacy.db` is never created or opened. Everything wires to
`store::`/engine/Protein. The legacy FullUi serve path is gone — `lince`,
desktop, and the cell server all boot `serve_cell_api_only` (which also now
imports the installer's staged admin password + language into the Cell). The
permission catalog moved to `utils::auth`; `serve_package_asset` survived into
`presentation/http/package_assets.rs`. Anything below is kept only as the
record of how the split used to work.

### (historical) Migration boot: `store` owns `lince.db`, legacy owns `lince-legacy.db`

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
