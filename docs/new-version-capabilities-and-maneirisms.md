# New-version capabilities and maneirisms

State of the whole app after the 2026-07-11 backend finalization pass. This is
THE living document — manual, theory, and tracker in one: what each pillar can
do **today**, how you invoke it, its maneirisms (the little behaviors you must
know to not fight it), what a sand could be built on top of it right now, the
condensed theory (§16), and everything still to do at the bottom, below the
`<!-- Tasks below here -->` marker. The workflow: do a task, delete it from
below, and write the resulting behavior into the sections above. (The old
blueprint `docs/fable-improvement.md` and the Stage 8b tracker were merged in
here and deleted; their full history is in git.)

**Where we stand:** the legacy layer (`persistence`, `lince-legacy.db`, old
karma/views/tui/gui, the FullUi legacy server) is deleted. Everything runs on
`nucleus → store → engine → protein → transport`, 128 backend tests green.
Blueprint Parts 0–XIII and XV are implemented and tested; Fiote (XIV) is
deferred by decision. What remains is surface work (sands) plus three named
backend gaps: OSM routing (`route_eta`, polygon `within`), the CRDT text relay
for collaborative head/body editing, and the periodic polling task that drives
the organ HTTP boundary automatically (the boundary itself is live).

---

## 1. How a sand talks to the Cell

One WebSocket at `/host/transport/ws`, multiplexed. A sand never sees SQL or
tables — only Protein (reads) and Actions (writes), plus ephemeral lanes for
presence/events.

```js
// through the board bridge (widget host)
H.subscribeProtein("my-sub", { source: "record", where: [{ quantity_lt: 0 }] },
                   rows => render(rows));
H.act({ action: "set-quantity", target: "apples.stock", value: 3 });
H.joinRoom("abi:recordClicked"); H.onLane(...); H.emit("recordClicked", {...});
```

Wire maneirisms:
- Actions are JSON with a kebab-case `"action"` tag; every other field is
  snake_case. Protein predicates/includes are snake_case too.
- A subscription answers with a snapshot, then re-executes and pushes rows on
  every relevant commit (`live` invalidation is coarse-by-source in v1: your
  sand may get refreshes it doesn't strictly need — render idempotently).
- Action responses carry `created` (uid of what was made), `facts` (what the
  Ledger committed, including any Karma cascade), and `warnings` (non-fatal
  advisories like link-cycle warnings and rule Proof loops). Show warnings;
  never treat them as errors.
- Ephemeral lane traffic (cursors, clicks, presence) is **never** persisted.

## 2. Records and the Ledger (the ground truth)

Everything is a record: tasks, rules, signals, transfers, decisions, organs,
people, saved Proteins, threads, messages. Every change is a fact with a hash
chain and (when a signer is set) an ed25519 signature.

What you can do:
- `create-record`, `set-quantity`, `add-quantity`, `edit-record-text`,
  `set-slug`, `set-concept`, `set-unit`, `set-place`, `set-extension`
  (namespaced fds JSON), `activate`/`deactivate`, `delete-record` (HARD
  delete, see below).
- **Undo** = `compensate { fact }`: appends the inverse delta with
  `cause=compensation`. There is no destructive undo.
- Provenance for free: any sand adds `include: { facts: { limit: N } }` and
  gets the "why did this change" drawer — delta, at, cause_kind, cause, actor.
- Checkpoints (`Engine::checkpoint_all`) snapshot levels; compaction folds
  pre-checkpoint history into a cold JSONL archive whose SHA-256 is anchored
  back into the Ledger. Retention horizons are per record-kind
  (`retention_policy`); no policy = keep forever.

Maneirisms:
- `quantity` is a **cache**; the fact is the truth. Negative = Need, positive
  = Contribution, zero = peace. Activation of rules/transfers/signals/sands is
  the same knob (quantity 0 = off).
- **Deleting and zeroing are two different things (2026-07-17).**
  `deactivate` = quantity → 0; the record stays on every read surface (in a
  kanban it sits in the quantity-0 column — honest). `delete-record` = HARD
  delete: the record is tombstoned (`record.deleted_at`) and vanishes from
  every read path — record Proteins, `get`/`resolve`, rule inputs, checkpoint
  sweeps — and its UNIQUE slug is freed for reuse. The Ledger is untouched:
  facts stay, the hash chain stays verifiable, and a final zero-delta
  annotation records `{deleted, slug}`. Rows DO vanish from subscriptions
  after a hard delete; only fact-source queries still see the history.
- Metadata edits (slug/concept/unit/extension/text) drop **zero-delta
  annotation facts** so live subscriptions refresh — expect facts with
  `delta: 0` and a JSON payload describing the edit.
- Appending a fact whose uid already exists is a silent no-op (replay safety).
- Slugs are optional local conveniences (`dot.case`); uids are identity.
  Actions accept either.

## 3. Lingua (concepts)

- `create-concept`, `adopt-concepts` (foreign concepts keep their uid and
  lineage; re-adoption is a no-op), `declare-equivalence` (cross-dialect
  same-ness, e.g. `apple ≡ maçã-fuji`).
- The parent DAG powers widening: `concept_in: "food"` matches `@apple`
  through `apple → fruit → food` in every Protein source that supports it.
- Unit conversion: `concept_conversion` rows, one authoritative row per
  unordered pair, reverse derived as `1/factor`, only within a shared ancestor
  dimension.
- Dialect fallback: `nearest_ancestor_in` — an unknown `@blocks-softly` is
  treated as its nearest known ancestor `@blocks`.
- Concepts travel automatically inside sync packages (see §10) — a record
  arrives already understandable.

## 4. Links (the graph)

- `add-link`, `remove-link`, `relink-order` (drag-reorder sugar). Identity is
  the triple (from, kind, to) — the same two records carry many link kinds.
- Adding a link of an **order-like** kind (`precedes`, `before`, `order`, or
  their descendants) that closes a loop **succeeds but warns**
  (`outcome.warnings`: "these N records form a loop: a -> b -> a"). Non-order
  kinds (like `needs`) never warn — mutual recipes are legal.
- Protein: `include: { links: { kinds: [...], direction, depth } }`. `depth: 2`
  BFS-expands the tree and stamps each link with its `hop`. Ordering:
  `order: [{ topo: "before" }, ...]` — the focus queue.
- Tags/clusters are links: `linked_to: { kind: "tag", to: "tasks" }` composes
  with `any`/`not`/`all` for include+exclude filtering.

## 5. Promises (the social atom)

- `create-promise` (including OPEN promises — published Needs/Contributions
  with an unfilled party slot), `promise-transition` (state machine validated:
  open → proposed → agreed → active → kept/broken/withdrawn),
  `edit-promise-delta` (a counteroffer — it resets every party's agreement).
- **Expiry is automatic**: each heartbeat, past-window promises move
  `agreed/active → broken` (a commitment was not kept — this also enqueues an
  `expiry` decision) or `open/proposed → withdrawn` (a lapsed offer, quietly).
  Sands never need their own deadline logic.
- Reservation: `reserve_from` per promise — explicit value, else the bundle
  transfer's `reserve_default`, else `active`. Protein's
  `include: { availability: true }` returns `available` and `planned`.

## 6. Karma (automation)

A rule is a record (sidecar: condition, gate, carry, debounce) plus consequences.
Full math conditions with these tokens, all live:

```
@x  /  quantity(@x)         sum(@x, 30d)   sum_pos(@x, 30d)   sum_neg(@x, 30d)
freq(@freq.daily-7am)       signal(@fridge-cam)       value(@rules.burn-rate)
promise_state(@p_...)       confidence(@p_...)        projected(@x, 7d)
hours_since_fact(@x)        distance(@a, @b)          demand(@food)
```
(`route_eta` parses but errors cleanly until OSM data lands.)

Consequences: `set_quantity`, `add_quantity`, `emit_promise`, `run_command`,
`run_query` (executes a saved Protein, logs the row count), `run_action`
(re-enters `Engine::act` with a typed Action from the effect queue),
`set_visibility`, `advance_transfer` (within policy, never past it),
`activate`/`deactivate` (works on rules — quiet hours is just a rule turning
another rule off), `ask` (enqueue a decision), `notify` (budgeted, see §9).

How you drive it from a sand:
- `create-rule` / `update-rule` — both reload the registry and return **Proof
  warnings** in `outcome.warnings` when your new rule closes a loop
  ("these 2 rules form a loop"). Save still succeeds; show the warning.
- `create-signal` (command/http/sensor/query source on a schedule; samples land
  as facts and cascade like any change), `create-frequency` (the time
  primitive; day-of-week and catch-up supported).

Maneirisms:
- Delivery is reactive: only rules reading a changed record re-evaluate, then
  their writes cascade (capped at 256 per delivery — a runaway loop survives).
- `debounce: "1h"` holds a rule for an hour after it fires even if inputs keep
  changing. In-memory: the hold resets on rule reload.
- A rule with **zero consequences is a named derived value** — read it with
  `value(@rules.monthly-burn)` like a spreadsheet cell.
- Effects (`run_command`/`run_action`/`run_query`/`notify`) run OUTSIDE
  evaluation from a durable queue, and each logs a zero-delta provenance fact
  on the rule record.
- Worked examples that are real tests: daily habit, reorder ask, quiet hours,
  trust-ahead (`confidence(@p) > 0.9` → `advance_transfer`).

## 7. Protein (the read contract)

Six sources, one JSON shape:

| source | what you get | notes |
|---|---|---|
| `record` | the state vector | all predicates + all includes |
| `promise` | promises (state-filterable) | `state_in` |
| `decision` | the open Decision Queue | never exported to remote subjects |
| `fact` | the Ledger itself | `at_since` ("30d" or RFC3339), `cause_kind_eq`, `record_eq`, `concept_in`; the finance workhorse |
| `concept` | the Lingua vocabulary | names, instincts, parents |
| `transfer` | bundles **with derived status** | plus parties, promises, balance |

Predicates: `all/any/not`, `quantity_lt/lte/gt/gte/eq`, `uid_eq`, `kind_eq`,
`slug_eq`, `concept_in` (DAG-aware), `linked_to`, `state_in`, `near` (place
Instinct), and the fact-source trio above.

Includes on record rows: `facts` (provenance), `promises`, `links` (kinds,
direction, `depth` tree with `hop`), `threads` (nested messages),
`extension` (one fds namespace), `availability` (`available`/`planned`),
`projection` (`{ at: "+7d" }` → `projected` = quantity + agreed/active promise
deltas closing by then — the promise fold; full rule simulation is
`Engine::project`, engine-side).

Aggregation: `{ op: sum|count, by: concept|kind }` on records,
`by: cause_kind|day|concept` on facts. The visibility gate applies **before**
aggregation — hidden rows can't leak through sums.

Saved Proteins are records (`kind='protein'`, AST in the `lince.protein`
extension) written by `save-protein`; sands reference them by slug — the old
"view" concept, done right.

Maneirisms:
- The wire `where` is a JSON array = implicit `all`.
- Fact-source predicates don't nest (flat list) in v1.
- `at_since: "30d"` resolves against wall-clock now (fine for dashboards; use
  absolute RFC3339 for reproducible reads).
- Remote subjects see only whole-row visibility grants; the Decision Queue and
  concept-level promises never leave the Cell through Protein.

## 8. Transfers (promise bundles under agreement)

The full ladder is derived, never stored — a `source: transfer` row carries
`status`: `inactive → draft → proposed → agreed → in_transfer → settled`.

Flow a sand can drive today, all typed Actions:
1. `create-transfer` (agreement `individual|full|percentage|dependency`,
   `satiation`, `reserve_default`, `require_confirmation`).
2. `add-party`, `add-promise-to-transfer` (a condition string makes it a chain
   link or spectator — same grammar as Karma conditions).
3. `agree-transfer { level: 2 }` — level 2 also advances that party's bundled
   promises to agreed. Editing any bundled promise drops everyone back to 0.
4. `activate-transfer` (within policy), then `settle-transfer` — the ONLY
   thing that mutates record quantities, idempotently, with
   `cause=settlement`. Chains fire, satiation withdraws sibling bundles.
5. If `require_confirmation` was set: settlement refuses until both
   `confirm-transfer { confirmation: "delivery" }` and `"receipt"` annotation
   facts are in.
- Advisory balance on every transfer row: `balance` (per-concept promise sums)
  and `balanced` — a trade sums to zero per concept, a donation deliberately
  does not. Advisory only; never blocks.
- Transfer chat = `create-thread` on the transfer record + `create-message`;
  read back with `include: { threads }`. No transfer-specific message model.

## 9. Attention (the Decision Queue — "what should I do next")

`source: decision, live: true` is the queue; `decide` is the answer. Four
deterministic feeders run **inside the heartbeat**, no UI required:

1. **Broken promises** → `kind: "expiry"` decisions.
2. **Karma `ask`** consequences → `kind: "ask"`.
3. **Senses matches** (§11) → `kind: "draft"` — "your promise X meets Y from
   organ Z (score 0.87)", options propose/dismiss.
4. **Projected crossings** → `kind: "crossing"` — "apples.stock hits 0 on
   Tuesday 2026-07-14 (promise)", a week's horizon, per-record deduped.

Maneirisms:
- Answering: `decide { decision, answer }` closes it through the Ledger
  (quantity 1 → 0, so Karma can react to answered decisions). If the chosen
  option carries an `action` field (any typed Action), **decide executes it**
  — one-tap flows like "yes → set-quantity".
- Decisions with `expires_at` are auto-closed as `"expired"` by the heartbeat.
- Every sweep dedups by `(subject, kind)` — one situation asks exactly once
  while its decision stays open.
- **Notify budget**: `configuration.attention_budget_per_day` (default 12) is
  hard. Over-budget notify effects complete with result `parked:digest ...`
  instead of being delivered — a digest sand reads the parked rows; nothing is
  lost, nothing interrupts.

## 10. Sync and Organs (multi-Cell)

Implemented and tested end-to-end (two engines, in-memory wire = the same code
path the HTTP boundary calls):

- **Introduction**: `GET /organ/introduction` returns who I am + my public
  keys; `Engine::adopt_introduction` registers the contact **under the remote
  organ's own uid** (identity replicates by uid) and stores its keys so its
  signed facts verify.
- **Contacts**: `organ_contact` — trust `unknown|known|blocked`, numeric
  `proximity`, per-organ `sync_out`/`sync_in` policy. **Blocked rejects
  everything everywhere** (imports AND discovery), tested.
- **Push**: `enqueue_sync_to(organ)` builds a visibility-gated package (the
  same single gate Protein uses) and queues it in `sync_outbox`;
  `drain_outbox(sender)` sends with retry (failures stay queued). The wire
  endpoint is `POST /organ/inbox`.
- **Import hardening**: every incoming fact must pass its chain step (content
  → hash) and its signature (hash → author); rejected rows land verbatim in
  `sync_quarantine` with a reason, the rest of the package still applies.
  Import is idempotent by fact uid; deltas commute — quantity sync is
  conflict-free by construction.
- **Concepts ride along**: a package carries every concept (uid + name +
  ancestors) its records speak; import adopts them with lineage before the
  records land — a stranger's data arrives understandable.
- **Discovery**: `GET /organ/open-promises` (subject in the `X-Lince-Organ`
  header) exports the OPEN promises that subject may see;
  `refresh_discovery` upserts them into the local cache, stamping proximity
  from OUR contact row (your proximity never travels outward).

**The one unwired piece**: nothing calls the boundary on a timer yet. A small
web-side task (or even a Karma `run_command` with curl, today) needs to
periodically drain the outbox to each contact's `base_url` and pull
open-promises into the cache. Everything on both sides of that call exists.

## 11. Senses + Imagination (the recommendation engine)

- A **match rule is a record**: `create-match-rule { watch_concept,
  max_proximity, min_confidence, auto }` — activate/deactivate like any rule.
  `max_proximity` is a HARD ceiling; matching never auto-expands.
- Each heartbeat, `senses_pass` joins your OPEN promises against the discovery
  cache: sign-opposite deltas, Lingua-aligned concepts (same uid, or through
  the DAG), window overlap, confidence floor — ranked drafts straight into the
  Decision Queue.
- `Engine::project(now, until)` folds promises + rules forward on a virtual
  clock (signals frozen); `Engine::snapshot(now)` hands you the mutable input
  — toggle a rule, clear a promise, re-fold, diff the two timelines: the
  scrubbable/branching future is an engine call, the sand only renders.
- Deterministic numbers, no ML: `confidence(@p)` = the party's
  Laplace-smoothed kept-ratio; `demand(@concept)` = the current hour's share
  of the trailing-30d activity on that concept.

## 12. Trust

Every locally-authored fact is signed on the write path; imported facts keep
their **origin** signature so downstream Cells can still verify the original
author. Two-layer tamper model: the hash chain guards content, the signature
guards authorship. Compaction archives stay verifiable file-side; the anchor
fact makes the file tamper-evident from inside the Ledger. No reputation
scores, ever — kept/broken history is queryable raw material.

---

## 13. The board and the shipped sands (the web surface today)

The product surface is the existing web/Tauri board (`crates/web`), refactored
in place — the data plane under it was replaced, the chrome was not. The Cell
database is `~/.config/lince/lince.db`; the `lince` CLI, the desktop, and the
cell server all boot the same `serve_cell_api_only` (which also imports the
installer's staged admin password + language). The legacy FullUi server,
`persistence/`, and `lince-legacy.db` no longer exist.

- **One socket.** The board opens exactly ONE WebSocket to
  `/host/transport/ws` (`static/presentation/board/transport.js`), shared by
  the unified bridge (`widget-bridge.js`) and the Data panel. The bridge speaks
  BOTH frame protocols — the legacy nested-payload chrome shape and the new
  flat `frame.js` shape (`lince:ready` / `protein-subscribe` / `lince:action` /
  `lane-join` / `lane-send`) — and routes rows/action-results back in each
  frame's own dialect. Consumers filter inbound by subscription id and lane
  room; ids never collide across consumers.
- **Sands are Rust-canonical.** Each official sand is a self-contained `.html`
  string embedded via `include_str!` and registered in `OFFICIAL_WIDGETS`
  (`crates/web/src/sand/mod.rs`); groups ship as `.lince` workspace archives
  (`kanban.lince` = kanban board + record_info); `example-bundle` is the
  multi-file directory-bundle template. Everything is emitted to
  `~/.config/lince/web/sand/` at boot; the catalog peeks content
  (`is_workspace_archive_bytes`) so a group archive is never mis-parsed as a
  single sand, and a group entry replaces a same-named single sand ("Kanban"
  IS the group).
- **Groups nest (groupception).** `BoardCard.group_ids` (outermost →
  innermost) is authoritative; `group_id` stays synced as the innermost for
  back-compat. Disbanding/unlocking an OUTER group preserves inner groups.
  Adding "Kanban" from the catalog adds the group (board + record_info beside
  it), re-homed to a FRESH inner id each add, so repeated adds are independent.
- **Events are scoped.** A grouped sand's `emit` reaches only frames sharing
  its **innermost** group — kanban's `recordClicked` drives its own
  record_info, not another group's; ungrouped sources broadcast board-wide.
  Cross-session mirroring rides lane rooms (`abi:<topic>`); lane traffic is
  never persisted.
- **Per-card host state flows both ways (K1).** `frame.js` exposes
  `H.getCardState()` / `H.onCardState(handler)` (fed by the bridge's
  `lince:bridge-state` push, `meta.cardState`) and `H.patchCardState(patch)`
  (the flat `lince:patch-card-state` message → the board's widgetState) — any
  sand can persist UI prefs as host state without touching the Ledger.
- **Kanban body view modes (K1).** Every card renders in one of three modes —
  `head` (title only), `compact` (a 6-line excerpt ending in `...`), `full`
  (whole body) — resolved per-card override → per-column override → board
  default. The header has an "All head / compact / full" bar — `▁ ▄ █` icons,
  the block glyphs reused everywhere a mode is shown (setting it clears every
  override); each column header carries a small cycle button (highlighted
  when overriding) that wraps `head→compact→full→head` forever (fixed
  2026-07-17: it used to clear back to "auto" after full, which looked like a
  dead click whenever the board default was also full). **Per-card override
  is a direct-pick, not a cycle (revised 2026-07-18):** hovering (or
  focus-within/selected) a card reveals a `.card-actions` overlay — the
  select checkbox plus the three `▁ ▄ █` icons side by side, `z-index`'d
  above the card's own content so they never overlap each other (previously
  an always-visible single cycle button sat inline after the title and
  visually collided with the hover-only checkbox in the same corner);
  clicking a mode icon sets it directly, clicking the active one clears the
  override back to inherited. Persisted in `cardState.kanban` via
  `patchCardState` (localStorage fallback outside a board); mode clicks never
  emit `recordClicked`. Proven by `scripts/other/kanban-modes-selftest.sh` (9
  assertions, real frame.js/bridge, stubbed socket — the per-card assertion
  targets `[data-card-mode="full"]` directly instead of clicking a cycle
  button three times).
- **Kanban is the OLD sand's UX ported** (2026-07-15, from `ea54ead^`),
  client-rendered on Protein/Actions: quantity lanes (Backlog/Next/WIP/
  Review/Done, overridable via `cardState.kanban.lanesDef`) with per-lane
  collapse-to-thin-bar and drag-the-right-edge resize (persisted host state);
  optimistic drag-and-drop moves (`set-quantity`, card moves before the ack,
  rolls back on reject); just the live dot, no "Live" text next to it. **No
  view/filter chrome in the sand by decision (2026-07-15): Protein control —
  filtering, sorting, source choice — is BASE UI, the card's Data panel**
  (`cardState.savedProtein`/`.protein`, re-subscribed on change) — the sand's
  own `{ }` Protein-inspection toggle was removed for the same reason
  (2026-07-17): it's redundant with that GUI. Card detail AND creation live in
  the grouped record_info: card click → scoped `recordClicked`; "New task" →
  scoped **`recordCreate`** (a `+` icon, square, with a tooltip — no kanban
  create sheet or focus sidepanel). **Chrome is glued to the sand's own
  edges (revised 2026-07-18):** `main` carries zero outer padding/margin —
  the header is a single 2rem line (own `.5rem` padding, no border/radius/
  box) with a bottom border as the ONLY divider between it and the board;
  columns sit flush against each other and each other's edges (no gap, no
  vertical divider line) — the only other line left is each column's own
  `col-head` bottom border, separating its name from its cards. Every
  interactive control's border-radius is capped at 2px (the status dot stays
  a circle by design — it's "just the ball"). Proven by
  `scripts/other/kanban-sand-selftest.sh`.
- **The kanban board writes records in FOUR ways (revised 2026-07-17):
  checkbox toggles, column moves, IN-PLACE body edits, and BULK DELETE of
  selected cards.** Head/metadata/assignees/resources still stay the grouped
  record_info's job (single-record editing). Bulk delete: per-card checkbox
  in `.card-actions` (or ctrl-click) plus a per-column "select all" checkbox
  in the col-head feed a `selected` uid set; a floating bottom bar (fixed to
  the sand's own viewport, so it never leaves the iframe) shows move/delete/
  clear as a uniform icon row once anything is selected; delete opens an
  in-sand confirm modal (kanban has no host-modal bridge) and, on OK, fires
  the HARD `delete-record` (NOT `deactivate` — the board's driving Protein
  has no quantity filter, so a soft zero-out would leave the card sitting on
  the board) optimistically, splicing the row out before the ack lands.
  Click targets: the card **TITLE** opens record_info (group-scoped
  `recordClicked`); the card **BODY** — in a body-showing mode — starts
  editing RIGHT THERE: a textarea with the K6 slash palette replaces the
  body, Ctrl+Enter or blur saves (`edit-record-text`, optimistic with
  rollback), Escape cancels, dragging is disabled while typing, and an empty
  body renders an "add body…" target so writing can always start. Bodies
  render through `/board/editor.js::renderMarkdown` (K2 CLOSED): headings,
  REAL checkboxes (toggling flips ONLY that line and writes the body back —
  never a `recordClicked`; indexes map to the ORIGINAL body so compact mode —
  blank-edge-trimmed, 6 lines + `...` — toggles the right line),
  **`![](...)` images displayed as the photos they are**: `http(s)://` URLs
  render directly; local images go through the editor's "/image" block —
  picks a file, uploads it, and inserts `![](/host/media/<uuid>.<ext>)`.
  Bytes are sniffed by magic number against a raster allowlist (png/jpg/gif/
  webp — never trusting the client's filename or extension) and stored under
  `<lince_data_dir>/web/media/` with an OPAQUE generated name — there is
  still NO route that serves an arbitrary disk path (that would let any page
  open in the same browser read local files through a bare `<img>` tag), so
  a hand-typed absolute path never resolves; only the app's own uploads do.
  **The picker itself is two-tier (revised 2026-07-18, see the crash
  postmortem below):** the sand first tries `POST /host/media/pick` — a
  SERVER-SIDE native file dialog (`rfd`, "xdg-portal" feature — ashpd, a
  pure-Rust D-Bus client to xdg-desktop-portal, zero GTK) that opens on
  whatever machine the Cell runs on; a 404 (the route only exists when
  `lince-web`'s `native-picker` Cargo feature is enabled, which only
  `lince-desktop` does) falls back to the ordinary browser `<input
  type=file>` + `POST /host/media` upload. **Why the fallback exists — a real
  crash, root-caused via `coredumpctl`:** WRY/WebKitGTK has no custom
  file-chooser handler registered, so `<input type=file>.click()` fell back
  to WebKitGTK's OWN built-in `GtkFileChooserWidget`; tearing it down after a
  pick calls `_gtk_file_chooser_get_settings_for_widget` → `g_settings_set_
  property` for the `org.gtk.Settings.FileChooser` GSettings schema, and
  GLib treats a missing schema as UNCONDITIONALLY FATAL — `abort()`s the
  whole process (confirmed: `lince-desktop` SIGABRT, cascading into the
  WebKitWebProcess crashing ~3s later; "free(): corrupted unsorted chunks" in
  the terminal is a secondary symptom of that abrupt multi-threaded abort,
  not a separate bug). The existing `GTK_USE_PORTAL=1` (see
  `crates/desktop/src/lib.rs::run`) does NOT cover this — WebKitGTK builds
  that widget regardless. Two things this ruled out: (1) Tauri's own dialog
  plugin / a custom `#[tauri::command]` — the webview loads a real `http://`
  URL, not `tauri://`, so Tauri v2's capability/ACL system blocks IPC there
  without a remote-domain capability grant (its own rabbit hole, not
  simpler); (2) putting `rfd` in `lince-web` unconditionally — its
  "xdg-portal" feature pulls in `wayland-sys`, which needs pkg-config at
  build time, breaking the documented guarantee that a plain `cargo build -p
  lince` needs no native libs. Feature-gating it (`native-picker`, enabled
  only by `crates/desktop/Cargo.toml`) keeps that guarantee. Proven live
  end-to-end (real server, real board, real record create) by
  `scripts/other/media-image-live-selftest.sh`; the upload route itself
  (sniffing, opaque naming, traversal rejection, nosniff header) by
  `scripts/other/media-upload-selftest.sh`; the picker/placeholder UI
  (including the pick→fallback branch) by
  `scripts/other/body-editor-selftest.sh`. RustFS/bucket image CRUD was asked
  for too but was **never actually wired server-side even pre-refactor**
  (`bucket_image_view`'s "host proxy" and document_viewer's `/host/
  integrations/servers/...` both reference a route that was never
  registered) — reviving it is its own sizable feature (S3 client + server/
  organ config CRUD + an auth'd proxy route + a browsing UI), deferred, not
  bundled into this fix. `@slug` chips that
  hop to record_info. Cards also badge the
  **assignee** and (board option in the Columns sheet, the old
  `showParentContext`) the **parent** above the title — the DEFAULT_PROTEIN
  rides `assigned-to`/`part-of` links along, a `kind_eq:"person"` side
  subscription resolves names, and a custom Protein without the include just
  means no badges (tolerant-ignore). The status dot is three-state
  everywhere (K4): green live, AMBER pulsing while an Action awaits its ack,
  red reconnecting.
- **record_info IS the editing surface (K4+K5, 2026-07-17).** The get view is
  the edit view — head/slug/quantity/body are writable inputs; Save writes
  only what changed (`edit-record-text`/`set-slug`/`set-quantity`); a dirty
  form is never clobbered by live updates. **Zero and Delete are separate
  buttons**: Zero = `deactivate`, Delete = the HARD `delete-record` (view
  shows the deleted placeholder when the subscription empties). Collapsible
  sections: **Work** — start/due dates, estimate (minutes), worklogs with
  Play/Pause + per-entry delete + running total, all on the `work`
  record_extension (`{start, due, estimate_min, logs:[{start,end}]}` — the
  freestyle-data-structure choice, same precedent as kanban presets); failed
  work writes queue in localStorage and FLUSH on reconnect (the old sand's
  offline pending-stop queue, generalized). **Assignees** — person picker →
  `add-link assigned-to`, chip ✕ → `remove-link`. **Parent & children** —
  `part-of` links; Set replaces the parent, chips navigate. **Resources** —
  `resource-of` in-links; pasting a URL mints a resource record (body = URL)
  and links it; slugs/uids link existing records. **Threads (revised
  2026-07-18): a REAL multi-thread system, not one flattened message list.**
  The engine already supported any number of named threads per record
  (`create-thread{target,head}` → a `thread` record linked `thread-of` the
  target; `create-message{thread,body}` → a `message` linked `message-in` it)
  — record_info just used to always find-or-create a single thread literally
  headed `"comments"` and pour every post into it. Now every thread the
  record's `threads` Protein include returns renders as a tab/chip
  (`renderThreadTabs`); only the ACTIVE thread's messages show, and the
  compose box's `create-message` targets THAT thread's uid — switching tabs
  re-scopes both. "+ New thread" opens an inline name field that fires
  `create-thread` and switches to it once created; posting with zero threads
  still auto-starts one (named "General", not hardcoded "comments" anymore).
  `@slug` in a body renders as a focus hop AND becomes a real `references`
  link from the message; image URLs render inline. Hit the SAME cascade-tie
  bug the kanban bulk bar already worked around: `#thread-new`'s `.inline`
  class sets `display:flex`, which — same author origin, tied specificity —
  beats the `hidden` attribute's implicit `display:none`, so it needs its own
  `#thread-new[hidden]{display:none}` rule to actually hide (caught by
  screenshotting the rendered sand, not by the DOM-property-only selftest
  assertion, which can't see actual CSS layout). A `names` side subscription
  (`source:"record"`, limit 500) powers pickers, chips, and @resolution.
  Proven by `scripts/other/record-info-selftest.sh` (34 assertions, including
  that the provenance include is wire-valid — `direction:"both"`, the live K0
  caught `"any"` once; 7 of the 34 cover create/switch/scoped-post/switch-back
  for the multi-thread system specifically). `add-link` now ENSURES its kind
  concept (like the thread kinds) so vocabulary kinds need no ceremony.
- **The slash block system lives in `/board/editor.js` (K6, 2026-07-17).**
  A reusable module served embedded beside frame.js exposing
  `window.LinceBodyEditor = { attach, renderMarkdown }`. `attach(textarea,
  {getNames})` gives any sand the Notion-like palette: `/` at a line start
  opens the block list — headings `#`×1–7, image `![](url)` (placeholder
  pre-selected), checkbox `- [ ]` — filterable by NAME (`/h3`, `/img`) or by
  the underlying characters (`/##`); `@` anywhere opens the record picker.
  Arrows/Enter/Tab/Escape + click; insertion replaces the query with plain
  markdown and fires a real `input` event. **The body stays canonical
  MARKDOWN** — the palette and hand-typed characters produce the same stored
  text and the same visual block via the shared `renderMarkdown` (headings,
  clickable checkboxes reporting ORIGINAL line indexes, markdown + bare
  image URLs, `@slug` chips that navigate). record_info wires it everywhere:
  the body textarea, creation mode, and the comment composer all get the
  palette; a live block PREVIEW renders under the body field (checkbox
  clicks on a clean form write `edit-record-text` immediately; on a dirty
  form they flip the draft for Save); comment bodies render through the same
  module; and every resolvable `@slug` in a body becomes a real `references`
  link on Save/Create (already-linked refs are skipped, so re-saving is a
  no-op). The module is optional — a sand without it degrades to plain
  textareas. Proven by `scripts/other/body-editor-selftest.sh` (18
  assertions, standalone) + the record_info selftest integration block.
- **The Data panel builder speaks links (K4 base-UI, 2026-07-17).** The GUI
  filter dropdown grew `linked to` (kind + to pair) and `assignee is` (sugar
  for `linked_to kind=assigned-to`) — cluster-tag and assignee filtering is
  the Data panel's job, never sand chrome. Round-trips through
  `builderFromAst` (an assigned-to linked_to deserializes back to the
  assignee row). Proven in `protein-config-selftest.sh`.
- **Kanban has the full column system (K3, 2026-07-16).** The "Columns" sheet
  does column CRUD — create (name + bucket), rename (the lane KEY stays
  stable, so widths/collapse/hide survive a rename), delete, reorder, hide —
  plus a "hide empty columns" toggle, all persisted as host state
  (`cardState.kanban.lanesDef` + `.lanes[key].hidden` + `.hideEmpty`; `Reset`
  merge-patches `lanesDef` away, back to Backlog/Next/WIP/Review/Done). A
  column buckets by quantity **value** (`1`), quantity **RANGE** (`3..10` —
  rows inside it bucket there; moving a card in writes the range's FIRST
  value via `set-quantity`), or **concept** (`@food` — moving writes
  `set-concept`; this supersedes the old sand's `COLUMN_ACTIONS` mapping).
  First matching lane in order wins. Column **PRESETS** apply in one tap:
  two built-ins (Todo/WIP/Done, Backlog/Next/WIP/Finished) plus user presets
  with full CRUD stored as **sand-configuration records** — kind `sand`,
  record extension namespace `kanban.columns` holding `{columns:[…]}`
  (`create-record` + `set-extension`; delete = `deactivate`) — subscribed via
  `kind_eq:"sand"` + `include.extension`, so presets are shared by every
  kanban on the cell and live-update. Concept UIDs now resolve to NAMES
  through a `source:"concept"` subscription (card chips show `@food`, not the
  uid). Cards **multi-select** (Ctrl/Cmd-click, the hover checkbox, or a
  per-column "select all" checkbox in the col-head) into a floating bulk bar
  for move (per-card optimistic, same Actions as a drag) or delete (bulk
  delete REINSTATED 2026-07-17 — see the kanban-writes entry above; a confirm
  modal gates it, `delete-record` not `deactivate`). Compact
  bodies render tighter (the institute "Pretext" nicety). **Column color
  (2026-07-18):** each lane carries an optional `color` (hex, editable via an
  `<input type=color>` in the same column row as its name/value/hide), and a
  board-wide "Color columns" toggle (`cardState.kanban.columnColor`, off by
  default) decides whether it's used at all. When on, the color washes the
  WHOLE column background at low alpha (`hexToRgba(lane.color, 0.14)` — full
  saturation across a whole column would wreck text contrast) — this replaced
  the old per-card left-color border strip entirely, so a card's color now
  comes from which column it's sitting in, not a bar painted on the card
  itself. Proven by `scripts/other/kanban-columns-selftest.sh` (bulk move) and
  `scripts/other/kanban-sand-selftest.sh` (bulk delete).
- **record_info has a creation mode (2026-07-15).** On `recordCreate` it shows
  the SAME fields a get displays (head, slug, quantity, body) writable and
  empty; Create writes `create-record` (+ `set-slug` when a slug is given) and
  then focuses the new record's provenance via `uid_eq` — the get view now
  also lists those fields above the fact log, so read and create are the same
  surface. Group-scoped end-to-end (a different group's record_info stays
  untouched): `scripts/other/kanban-group-e2e-selftest.sh`.
- **Relations is the d3 force graph on the new data plane, shipped as the
  GROUP (2026-07-17).** `relations.lince` = relations graph + record_info
  beside it, wired by the scoped `recordClicked`; the catalog's "Relations"
  IS the group (same-id replace, kanban precedent). The sand is the retired
  graph's renderer/physics/gesture code ported (`8f0a5df^` script.rs): canvas
  d3 force simulation with the four physics sliders (charge/link distance/
  collision/center force) persisted in `cardState.relations` via
  `patchCardState` (localStorage fallback), golden-angle initial layout +
  140-tick settle, new nodes seeded beside linked neighbors, wheel zoom /
  background pan / fit-to-nodes, zoom-faded node labels, quantity-colored
  node strokes (green > 0, orange < 0), directed arrows, edge kind labels at
  high zoom, parallel-kind fan-out. Reads ride ONE driving Protein (record
  source + `include.links` whose kinds/direction/depth are the sand's
  controls-panel chrome; the Data panel's `savedProtein`/`.protein` replaces
  it wholesale, tolerant-ignore without the links include — no edges).
  Writes: **Shift+drag node → node** = `add-link` of the active kind
  (optimistic dashed edge, rollback on reject); **edge click** selects and
  the header chip's ✕ = `remove-link` (optimistic). Node click → group-scoped
  `recordClicked` (the old record sidepanel is GONE — record_info's job);
  "New record" → scoped `recordCreate`. Three-state dot (K4). The old
  filter/category panels did NOT come back — filtering is the Data panel's
  job (the 2026-07-15 decision). d3 loads from the always-registered embedded
  route `/board/vendor/d3.v7.min.js` with its LICENSE served beside it — NOT
  `/static/vendored/…`: when `static_dir` exists on disk, `/static/*` goes to
  ServeDir alone and the embedded fallback routes are never wired (d3 404'd
  there on first ship = a graph with no physics). Overlay panels rely on a
  `[hidden] { display: none !important }` rule — element-level
  `display: flex/grid` beats the `hidden` attribute (the always-visible
  empty-state / never-closing panel bug, same day).
  Proven by `scripts/other/relations-group-e2e-selftest.sh` (19 assertions:
  real d3 + real frames + real bridge, including d3-loads / physics-heats /
  hidden-overlays regression guards; the stale pre-rebuild
  `relation-sand-selftest.sh` is deleted).
- **Trail mode lives INSIDE the relations sand (2026-07-18).** The Graph
  controls panel's Trail section switches modes: pick a root (dropdown or
  "Use selected node as root") + an order-like trail kind and the sand lays
  that root's FORWARD-reachable link tree out as an ordered path — topo
  layers left → right, nodes pinned, physics off (node drag becomes
  click-only; pan/zoom/fit stay), non-tree nodes and non-trail-kind edges
  hidden, the root ringed. Selecting a node shows the old trail's controls
  as a chip: **Done / Undo**. The promotion cascade is the retired
  `trail/logic.js` generalized over the active preset's buckets: Done needs
  every parent done first (refused with advice otherwise), a done parent
  promotes its road-ahead children to next AUTOMATICALLY, Undo sends the
  node back to road ahead and cascades descendants back (done descendants
  stay done). One Action per changed node, in order, optimistic with
  whole-batch rollback on reject. Trail prefs ride `cardState.relations`
  (`mode`/`trailRoot`/`trailKind`/`trailPreset`) like the rest of the
  chrome; the tree itself is computed client-side from the current
  subscription (raise Tree depth if it looks cut).
- **Trail status presets = kanban column presets (2026-07-18).** A preset is
  ordered steps, first = road ahead, second = next (the auto-promotion
  target), last = done; each step buckets by a quantity value/range (writes
  `set-quantity`, a range writes its first value) or a `@concept` (writes
  `set-concept`) — the kanban column rule, so `@done` here and a kanban
  `@done` column are ONE shared status vocabulary across sands. Presets
  drive node colors in BOTH modes (first step dim blue, last green, next
  orange — the old trail colors; under the classic built-in the graph's old
  quantity-sign coloring is unchanged). Two built-ins ship (classic
  `-1/0/1`, concept `@todo/@next/@wip/@done`); user presets are
  kind:`sand` records with extension namespace `relations.trail` holding
  `{steps:[…]}`, saved/applied/deleted from the panel (delete = deactivate),
  subscribed via `kind_eq:"sand"` + `include.extension` so every relations
  group on the cell shares them live. Concept NAMES resolve through a
  `source:"concept"` side subscription (rows carry uids), exactly like
  kanban. The e2e grew 14 trail assertions (33 total): tree scoping,
  layering, gating, the auto-promotion cascade, undo cascade, preset CRUD
  live-syncing a second relations group, and concept-bucket set-concept
  writes.
- **Action warnings now REACH sands (2026-07-17).** `ServerMessage::ActionOk`
  grew `warnings` (transport protocol), the bridge passes them through, and
  frame.js's `H.act` resolves `{ created, facts, warnings }` — link-cycle and
  Proof-loop advisories show as amber sand status, never errors (the §1
  maneirism made real). The relations e2e proves the whole path with a cycle
  warning in the add-link ack.
- **Ported sands:** table (the rebuild template — cell edits map by column to
  typed Actions), kanban (above), record_info (Protein-only: `uid_eq` + facts/threads includes, the
  legacy SSE fallback is deleted), todo (focus queue + `set-quantity`
  undo/redo), Relations (the group, above). **Board chrome (pan/zoom, workspaces, position/size, pin, z-index,
  grouping, edit mode, per-card `widgetState`) is host state, never Ledger
  facts — that line does not move.**
- **Served sand pages keep frame.js's API (K0 root cause, 2026-07-16).** Board
  cards with a filename load their sand BY URL
  (`/host/packages/local/by-filename/<file>/content/index.html`), and that
  route (`package_assets.rs::inject_package_html`) injects the legacy
  `widget-frame-bootstrap.js` + datastar into the page. It now SKIPS the
  injection when the page uses `/board/frame.js` — the same rule the
  client-side `enhancePackageHtml` applies to srcdoc frames. Before the skip,
  the legacy bootstrap ran after frame.js and clobbered
  `window.LinceWidgetHost` with the legacy API (no `onLive`/`onLane`/
  `joinRoom`), crashing every new-way sand's script: dead status dot, dead
  "New task", dead lane events — while every stubbed selftest stayed green
  because none of them served sands through that route.
- **K0 live proof exists and passes (2026-07-16):**
  `scripts/other/kanban-live-k0-selftest.sh` — the REAL `lince` binary, the
  real board page with its one WebSocket, the kanban group seeded through the
  same endpoint the catalog uses, driven in headless chromium on the real
  clock. Asserts: both sands render → the kanban dot goes LIVE → "New task"
  opens record_info's creation mode with empty head/body/quantity → Create
  writes a real record → record_info focuses it → the kanban grows the card
  via the live Protein update. Reporting rides `console.log` mirrored to
  stderr (`--enable-logging=stderr`); `--dump-dom` is useless for async
  flows — chromium 149 dumps at the `load` event.
- **The legacy table-CRUD/SSE layer is fully DELETED (2026-07-17).** The
  server was already a closed route allow-list; now the last client remnants
  are gone too: the retired Views UI (main.js catalog/search/select code, the
  `/integrations/servers/…/table/view` fetch, the hidden app.rs section) and
  `view_id` erased from the BoardCard schema (`#[serde(default)]` made this
  read-compatible; old board-state files load fine). Six retired sand
  sources whose scripts were the last legacy REST/SSE consumers were deleted
  outright — transfer, home_manager, karma_orchestra, record_editor,
  markdown_notes, role_access (git history keeps them; their rebuilds are
  the Sand ports tasks). Retired embed sands (chess, terminal, …) stay on
  disk per the embed-honest rule but are unwired; their internal legacy
  calls die when each is rebuilt on frame.js. Also fixed: a single
  record_info added from the catalog now defaults `abiListen` to
  `["recordClicked","recordCreate"]` (main.js), matching the group archive.
- **Proof style:** node is permanently unavailable in this environment; every
  sand/bridge behavior is proven by driven headless-chromium selftests under
  `scripts/other/` (stubbed socket + real frames + real served sand HTML),
  e.g. `bridge-unification-selftest.sh`, `kanban-group-e2e-selftest.sh`,
  `group-add-selftest.sh`, plus per-sand table/todo/kanban/record-info
  selftests — plus the ONE live test above that stubs nothing. Board JS is
  served with `Cache-Control: no-cache` so webviews never mix stale/fresh
  module pairs.

## 14. So: what sands could we build right now?

Everything below needs **zero backend work** — each maps to Proteins/Actions
that exist and are tested:

- **The Inbox ("what should I do / how can I contribute")** — the headline.
  `source: decision, live: true`; group by `kind` (draft/crossing/expiry/ask);
  answer with `decide`, with one-tap options executing Actions. This is the
  Attention pillar made visible, and it fills itself from four engine sweeps.
- **Finance / statistics dashboard** — `source: fact` with
  `aggregate: { sum, by: cause_kind }` for monthly flows, `by: day` for the
  spark-line, `concept_in: "money"` scoping; drill-down via `record_eq` for
  the provenance list. The old finance views are one Protein each now.
- **Transfer desk / marketplace panel** — `source: transfer` renders the whole
  lifecycle from the derived `status`; buttons are the §8 Actions; the balance
  advisory badges trades vs donations; the chat tab is the threads include.
- **Pantry / inventory with a future** — records with `availability` and
  `projection` includes: "8 now, 5 available (3 reserved), 2 by Friday"; the
  crossing decisions surface "you run out Thursday" without the sand doing
  math.
- **Timeline (the scrubbable future)** — render `Engine::project` output
  points; branching UI = mutate the snapshot and re-fold; diff view = compare
  two timelines. Engine-side only; needs a small transport verb to expose
  `project` (the one genuinely new endpoint a timeline sand would want).
- **Karma Orchestra v2** — rule CRUD through `create-rule`/`update-rule` with
  live Proof-loop warnings surfaced on save; derived values readable via
  `value()`; signals/frequencies creatable; the DepGraph render feeds off the
  rule records.
- **Neighborhood matching** — a settings card for `create-match-rule` +
  visibility grants ("publish this Need to @neighborhood"), with results
  arriving in the Inbox as drafts. Once the polling task is wired, this is the
  full stranger's-offer-meets-your-need loop.
- **Organ contacts manager** — list contacts with trust/proximity/sync policy,
  introduce via URL (`GET /organ/introduction` → `adopt_introduction`), block
  button, quarantine viewer (every rejected row with its reason).
- **Cluster/tag boards** — kanban/table already do this: `linked_to` tags with
  include/exclude, columns by any field, all writes typed.

**Not buildable yet (backend gaps, by decision):** collaborative text editing
(CRDT relay), route/ride matching (`route_eta`, OSM), automatic background
organ polling (endpoints exist, scheduler doesn't), Fiote conversation sands
(deferred), per-sand capability/permission enforcement on Actions (any sand
can currently call any Action — fine for official sands, gate before running
imported ones freely).

## 15. Cross-cutting maneirisms cheat-sheet

- Everything is a record; activation is quantity; delete is deactivate.
- The fact is the truth, quantity is the cache; undo is compensation.
- Metadata/state changes announce themselves as zero-delta annotation facts.
- Warnings are advice (cycles, Proof loops), never rejections.
- Heartbeat order: promise expiry → decision expiry → timers → signal sampling
  → effects (budgeted notify) → senses pass → crossings pass.
- One situation, one open decision (dedup by subject+kind).
- Uids are identity everywhere, across Cells; slugs are local sugar and get
  dropped on collision at import.
- Visibility is default-hidden, whole-row, enforced in exactly one place —
  and applied before aggregation.
- Blocked organs are rejected at every door (import, discovery, outbox).
- Conventions: uids are prefixed ULIDs (`r_/f_/p_/l_/c_/t_` = record, fact,
  promise, link, concept, transfer); slugs are `dot.case`; timestamps RFC3339;
  durations `90s` / `2h` / `30d`; `@slug` in conditions is sugar for
  `quantity(@slug)`.
- Time is deliberately NOT a record column: record timing = Karma + Frequency;
  declarative time (what strangers match on) lives on promise windows.
- Board chrome is frontend state; sand data is Protein/Actions.

## 16. The theory (from the retired blueprint)

The v4 implementation blueprint and the v3 essay before it live in git history
(as `docs/fable-improvement.md`). What outlives any tracker:

**The refounding, three sentences:**

> **Everything is a Record. Every change is a Fact. Every intended change is a Promise.**

**The pillar map:** Record (state) · Memory/Ledger (facts) · Lingua (shared
concepts; Instinct tier = concepts with engine functions) · Karma (Signals →
Rules → Effects) · Transfer (promise bundles under agreement + visibility) ·
Senses (matching open promises, proximity-scoped) · Trust (verifiable signed
deltas) · Imagination (state(t) projection + confidence) · Protein (declarative
reads) & Actions (typed writes) · Attention (the Decision Queue) · Fiote
(optional agent operating the same knobs).

**The placement rule (the Window):** the core owns what must be computed,
verified, or agreed across Organs; interfaces own what is seen; embed honestly
what the world already built well. Altitude ladder: Primitive → Pillar engine →
Instinct → Lingua concept/unit → fds sidecar → Sand → Embedded foreign app.

**Non-negotiables:** quantity stays central (negative = Need, positive =
Contribution, zero = peace); quantity-as-activation on everything; full math in
Karma conditions (tokens substituted, then evaluated); compatibility fully
ignored — greenfield build, old data ported by hand; DNA is a synonym for the
database, nothing more; local-first; no global reputation score, ever.

**Storage:** SQL is SQLite dialect (sqlx today); Protein is the abstraction
that later permits AniccaDB — replacing `store` must not change one character
of the Protein/Action contract.

**Decision log — 2026-07-11 (the backend finalization):**

1. The legacy layer dies now: `persistence`/`lince-legacy.db` removed; `store`
   is the only crate that speaks SQL.
2. Completion discipline: parts closed in order, each 100% implemented and
   tested before the next; Fiote explicitly deferred.
3. Promise expiry is a heartbeat arm: without it, confidence and availability
   lie.
4. MatchRule is a record: the Senses pass + crossing sweep + expiry feeding the
   Decision Queue IS the "what should I do next / how can I contribute"
   engine, backend-only over the existing transport.
5. Protein grew the missing sources/includes: fact/concept/transfer sources,
   projection/extension/depth includes.
6. Organ networking was the last construction: contacts, outbox, HTTP boundary,
   hardened import, discovery cache; acceptance was the two-Cell DONATION
   bundle over the wire.

**The Window's standing law:** apps are projections of one organism. Finance =
inventory = pantry (units + facts + promises + Imagination); chat = comments =
negotiation (messages on a shared object); profiles = catalogs = libraries
(published records behind visibility). When a new workflow arrives, triage it
against the primitives; if it doesn't decompose, the missing piece is named by
what resists — that is how the next abstraction gets deduced instead of
appended. Held against twenty-one workflows, the triage forced exactly four
core additions — place Instinct, ephemeral lanes, messages-attach-to-anything,
embed-honestly — and nothing else.

**The north star:** Ana wakes. No dashboard. The kitchen scale posts a fact;
beans cross their threshold; a promise to the roaster activates under a rule
she approved months ago; two Cells settle Saturday pickup. One whisper on the
walk to work — a nod, two promises change state. Work is an Organ; the standup
is a view nobody fills in. A second whisper near the market — her mother's
pantry Need, published to family only, met on the way home. In the evening she
scrubs the timeline out of curiosity: rent fine, a bar's event Organ bit on her
guitar Need, the tomatoes surplus in nine days and the donation rule is staged.
The Lincegoshi grows fat and luminous and dissipates. Under four minutes of
managing life, all of it decisions only a human could make.

That is the Death of Lince: management time asymptotically approaching the
irreducible minimum — the moments of actual human choice. More needs met, more
transactions peer-to-peer, more donations, more efficiency: the dance of the
world, made executable. Everything is a Record. Every change is a Fact. Every
intended change is a Promise. The rest is choreography — and Protein is how the
dance is seen.



<!-- Tasks below here -->

# Tasks

The workflow: pick a task, land it (implemented **and driven-tested**), then
delete it here and write the resulting behavior into the sections above — the
top half of this file is always the truth of what the app does.

Rules of the road: work on `dev`, no worktrees; `cargo check`, never build;
narrowest per-crate tests; never alter past migrations (new ones are fine);
sands read Protein and write Actions only; board chrome stays host state; keep
vendored license/credit files when touching embed-honest sands (terminal,
freedoom, chess, document viewer).

## Kanban + Record Info — one product (the active front)

**The standing decision:** kanban ships and is maintained as the GROUP —
kanban board + record_info, wired by the scoped `recordClicked`. Improving
kanban means improving both halves: the column system makes the board better,
record_info makes the expanded-card experience better, and together they make
the greater kanban. record_info stays a reusable sand (other sands drive it
too); **no feature comes back as a kanban-private sidepanel.** Advance without
losing the old sand's features — the feature inventory below comes from the
retired 3.7k-line kanban (`git show ea54ead^:crates/web/src/sand/kanban/
script.rs`) and the institute notes (`notes/institute/{Kanban,Kanban basic
CRUD,Kanban Metadata,Card Interaction & Visualization,Body Magic}.md`). Land
one phase at a time, each with a driven chromium selftest, keeping
`kanban-group-e2e-selftest.sh` green.

## Relations + Trail + Record Info — one group

**The standing decision:** Relations ships and is maintained as the GROUP —
relations graph + record_info, wired by the scoped `recordClicked`, exactly
like kanban (`relations.lince` = relations + record_info; the catalog entry
"Relations" IS the group). The old sand's detail sidepanel does NOT come back:
node focus, editing, and creation are the grouped record_info's job. Trail is
not a separate package and not a separate sand — it is a MODE of the relations
sand, applied to a chosen root record's link tree. **Phase 1 (graph parity)
LANDED 2026-07-17, Phase 2 (trail mode) LANDED 2026-07-18** — the d3 force
graph, physics, gestures, link-kind chrome, group packaging,
warnings-as-advice, the root-scoped trail path with the Done/Undo promotion
cascade, and trail status presets (quantity + concept buckets, shared live via
`relations.trail` sand records, cross-sand `@todo/@next/@wip/@done`
vocabulary) are all live (§13). Feature inventory came from the retired
sources (`git show 8f0a5df^:crates/web/src/sand/trail/script.rs`) and the
still-on-disk pure promotion logic (`crates/web/src/sand/trail/logic.js`).
Keep `relations-group-e2e-selftest.sh` green (33 assertions).

### Proof

- [ ] Re-prove acceptance workflow 1c (Relation trail) end-to-end on the new
      core and check it off above.

## Backend

- [ ] **Organ polling scheduler** — the one unwired sync piece: a periodic task
  that drains `sync_outbox` to each contact's `base_url` (`POST /organ/inbox`)
  and pulls `GET /organ/open-promises` into the discovery cache. Everything on
  both sides of that call exists (§10).
- [ ] **CRDT text relay** for collaborative record `head`/`body` editing,
  dropping zero-delta `text_edit` provenance facts; cursors ride ephemeral
  lanes. Unblocks the record editor sand.
- [ ] **OSM place data**: load a local OSM extract (offline-first), local
  geocoding, `route_eta` (parses, errors cleanly today), polygon `within`
  predicate, `route(a,b)` include. `distance`/`near` are already live.
- [ ] **Senses propose → send**: turn the "propose" answer of a draft decision
  into an actual transfer proposal sent to the remote organ.
- [ ] **Visibility refinements**: field-level grants (the `field` column) and
  most-specific-wins precedence (actor > role > organ > public). Today grants
  are whole-row.
- [ ] **Sign promise state transitions** (facts are signed on the write path;
  promise transitions carry a signature column but aren't signed yet).
- [ ] **Confidence refinements**: window-tightness factor and dispute penalty
  on top of the Laplace kept-ratio.
- [ ] **Karma divergence heuristic** — static Proof refinement; the 256-per-
  delivery cascade cap is the runtime guard today.
- [ ] **Verifiable aggregates**: `verified: true` Protein filter ("kept-promise
  ratio of @maria for @food, last 12 months"); leaderboards only ever as an
  opt-in sand between mutually-confiding Organs — no global score, ever.
- [ ] **Timeline transport verb**: expose `Engine::project`/`Engine::snapshot`
  over the WebSocket so a timeline sand can render and branch the scrubbable
  future. The one genuinely new endpoint a timeline sand needs.
- [ ] Future Instincts, each only when the engine must compute over it:
  duration/calendar math (Frequency is the proto-Instinct), currency
  conversion over the Lingua `@money` dimension.

## Sand ports (current-web parity)

Old sources for these were DELETED 2026-07-17 with the legacy table-CRUD
purge (git history has them); each is a fresh rebuild on the table template.

- [ ] **Transfer sand** → the §8 Action flow + promise/availability includes +
  derived status + threads chat.
- [ ] **Home manager/dashboard** → aggregate Proteins + Action writes.
- [ ] **Karma/rules surface (Orchestra v2)** → rule records, derived values,
  `create-rule`/`update-rule` with Proof-loop warnings surfaced on save,
  DepGraph render off the rule records.
- [ ] **Record editor** around `edit-record-text` (record_info's body editor +
  K6 covers most of it today); full collab blocked on the CRDT relay above.
  markdown_notes folds in here (its old source is also deleted).
- [ ] **Todo remainders**: create-task UI/Action, richer history backed by
  compensation/facts, details parity, live-update proof beyond the stubbed
  bridge. (Also deferred from the old table sand: keyboard-grid navigation,
  helix/common mode, the LynxDS "nerd" surface, concept/unit inline editors.)
- [ ] **`.lince` GROUP drag/drop import**: client routing still checks the
  `.group.sand` extension (`isGroupArchiveFile`) — route by content instead,
  like the catalog does.
- [ ] For every ported sand, a driven chromium selftest proving snapshot,
  Action round-trip, and live update in current web. THREE stale scripts
  found 2026-07-17 need rewrites on the current architecture:
  `board-selftest.sh` (boots the long-gone `membrane` crate) and
  `table-sand-selftest.sh` (references the deleted pre-rebuild `script.rs`).
  (`relation-sand-selftest.sh` was replaced by
  `relations-group-e2e-selftest.sh` — LANDED 2026-07-17.)

## Attention surfaces

- [ ] **Notify platform channels**: device records (`kind='device'`) routing to
  desktop toast / mobile push / sound / text digest, per-source on/off and
  quiet hours; a digest sand reading the `parked:digest` effect rows.
- [ ] **Inward capture sources**: every capture source (phone, scale, camera,
  mic) a visible signal-record with an off switch; AI only ever as a Signal
  implementation, never hidden.

## Long term

- [ ] Host-state sync for board presentation state across devices.
- [ ] Package import/publish subsystem on the new record/package model.
- [ ] Per-sand capability/permission model before imported sands can write
  arbitrary Actions; sand provenance `cause=sand:<uid>`.
- [ ] Broader browser selftests: pan, zoom, grouping, resize, pin, workspaces,
  import, publish, sand-to-sand events.
- [ ] Frontend polish/redesign after the data plane settles.
- [ ] New product surfaces: transfer marketplace, route/ride planning, group
  coordination, calls/chat, calendar/time budgeting, finance projections,
  social feed.
- [ ] Storage-engine independence (AniccaDB): acceptance = replacing `store`
  changes not one character of Protein/Actions.
- [ ] **Fiote (deferred by decision)** — the optional operator. Autonomy ladder
  per scope: `observe → suggest → draft → act-within-budget`. Reads only
  through Protein against `@fiote`-visible data; writes only through Actions;
  every write `cause=fiote`, signed by the user's key with an agent marker —
  delegation visible, inspectable, reversible via compensation. Budgets (max
  actions/day, max promise value, forbidden Action kinds — e.g. never
  `settle-transfer`) enforced by the engine, not the prompt. Optional
  narration of templated whispers per source.

## Parked (needs the user)

- [ ] Dates / estimates / worklogs: LANDED 2026-07-17 on the `work`
  record_extension (K4+K5, following the preset precedent). Revisit ONLY if
  the user prefers worklogs as time-concept delta facts on the Ledger —
  migration would be extension → facts.
- [ ] The trailed-off thought: "make sure the sands can also show a …" (best
  guess: a preview/collapsed sand state). Deferred until the user finishes it.

## Acceptance workflows (the Window)

Check each when the workflow runs end-to-end on the new core. Done already:
focus queue (1b). Relation trail mode (1c) was proven on the OLD core before
the legacy purge — it re-lands and gets checked off with the Relations +
Trail group section above; personal finance's
projected-crossing engine is done, its sand isn't.

- [ ] Todo/knowledge base (habit re-arms daily; done posts a fact with cause)
- [ ] Recurring tasks (monthly rule fires exactly once; catch-up works)
- [ ] Donation & buying (the DONATION and SALE bundles vs a second Cell,
  through real sands)
- [ ] Transport A→B (RIDE bundle drafts itself from two Cells' route×window
  overlap)
- [ ] Group coordination (assignment = promise; standup view fills itself)
- [ ] Chat & calls ("call the parties when the Transfer reaches agreed" as a
  rule; AV embedded)
- [ ] Real-time collab docs (two Cells, one body, cursors on lanes, Ledger
  shows only text_edit annotations)
- [ ] Social network (federated feed from two organs, visibility respected —
  zero new core)
- [ ] n8n-style command flows (signal→rule→effect chain built visually in
  Orchestra, and runs)
- [ ] CRM/people (birthday whisper fires; interaction report = one aggregate
  Protein)
- [ ] Personal finance sand ("rent leaves you short on the 5th unless X
  settles" rendered from the crossing engine)
- [ ] Inventory & production (`derive_needs(@cake, 20)` shopping list; the
  PRODUCTION chain relays settlement)
- [ ] World statistics (need-mountains aggregate from N organs, nothing hidden
  leaks)
- [ ] AI conversation sand (zero core changes)
- [ ] Calendar & time budgeting (projected week renders; moving a promise
  recomputes)
- [ ] Health & IoT (scale posts weight facts; streak rule; off-switch stops it)
- [ ] Games (chess on fds state; embedded engines; THE Game reads records with
  Karma as rulebook)
- [ ] Education (imported Relation trail shows per-student progression)
- [ ] Garden & farm (plant records + watering rules; scales into production)
- [ ] Monthly recaps ("this month in this Cell" generates itself from a rule)
