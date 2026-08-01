
> Extracted from `docs/Central: Karma.md` on 2026-07-29. The fine-grained end of
> this — CRDT text editing itself — is specified in `docs/Central: CRDT Sync.md`;
> what follows is the coarse end plus the shared relay both scales ride.

## [ ] Sync, Organs, and CRDT — multi-Cell communication, two scales of one idea

Both are "tell another organ what changed": CRDT is the fine-grained scale
(live operational state — who's editing this record right now, whose cursor
is where, right now), Sync is the coarse scale (batches of whole records/
facts moving between Cells on their own schedule). One relay, two
granularities and two sets of metadata.

- [x] Introduction: `GET /organ/introduction` returns identity + public
  keys; `adopt_introduction` registers the contact under the REMOTE organ's
  own uid (identity replicates by uid) and stores its keys so its signed
  facts verify.
- [x] Contacts (`organ_contact`): trust `unknown|known|blocked`, numeric
  `proximity`, per-organ `sync_out`/`sync_in` policy — **blocked rejects
  everything everywhere** (imports AND discovery).
- [x] Push: `enqueue_sync_to(organ)` builds a visibility-gated package (the
  same gate Protein uses) into `sync_outbox`; `drain_outbox` sends with
  retry over `POST /organ/inbox` (failures stay queued).
- [x] Import hardening: every incoming fact must pass its hash-chain step
  and its signature; rejected rows land verbatim in `sync_quarantine` with a
  reason, the rest of the package still applies. Import is idempotent by
  fact uid, and quantity sync is conflict-free by construction (deltas
  commute).
- [x] Concepts ride along a package (uid + name + ancestors) and are adopted
  with lineage before records land — a stranger's data arrives
  understandable.
- [x] Discovery: `GET /organ/open-promises` exports the OPEN promises a
  subject may see; `refresh_discovery` upserts them into the local cache,
  stamping proximity from OUR contact row (your proximity never travels
  outward).
- The still-missing Organ polling scheduler is tracked in Transfer Phase T1,
  where its first complete acceptance is proposal delivery; the scheduler
  remains shared Sync infrastructure rather than Transfer-owned transport.
- [x] Organ-scoped selection, Protein-driven: every record carries
  `organ_uid` (its origin organ — stamped locally on creation, carried
  through relaying so lineage survives multiple hops rather than collapsing
  to the last hop); Protein's `organ_eq`/`organ_in` select "every record
  belonging to organ X" (unknown-origin records never match). File Sync
  (`Engine::sync_to_disk`/`sync_from_disk`) writes/reads a Protein-selected
  Package to/from one JSON file on disk — Protein stops being read-only and
  becomes the selector for what leaves a Cell.
- [x] File Sync to markdown on disk (`Engine::file_sync_tick`, restoring the
  pre-refactor `file_sync.rs` convention): every record whose origin is a
  given organ mirrors to `{head}.md` (head = filename, body = file content,
  collisions disambiguated `{head} -- {uid}.md`) in a directory, both ways.
  A disk edit applies through `Action::EditRecordText` — the normal write
  path, so it fires the same annotation fact and reaches live-subscribed
  sands exactly like an app edit. **Disk wins** on a same-tick conflict (a
  hand edit overrides a concurrent app edit); a new file becomes a new
  record; a file's disappearance HARD-deletes its record only after 2
  consecutive misses (debounced, so an editor's atomic save — temp-write +
  rename — never reads as a delete). Identity is tracked by uid in memory
  (`FileSyncState`), not by re-parsing the filename. Selection is hardcoded to
  "belongs to this organ" (`organ_eq`) for v1 — **an arbitrary configurable
  Protein filter is deferred future work**, not yet wired to anything.
  Per-organ config (`enabled`, `path`) lives in that organ record's
  `lince.file_sync` extension, edited from the **Organ** sand (see
  `docs/Sand: Index.md`).
  `engine::file_sync::spawn_configured_watchers` runs once at `lince` boot
  (`serve_cell_api_only`): it reads every organ's `lince.file_sync` extension
  and spawns a `spawn_watch` loop (2s tick) for each one enabled with a path —
  the first tick after boot both dumps the currently-selected records to disk
  and starts watching for hand edits. This is boot-time only: toggling the
  config from the Organ sand takes effect on the *next* boot, not live — a
  start/stop-on-toggle supervisor is still future work.
- [ ] Wire organ-to-organ Sync (`enqueue_sync_to`) to also narrow by a
  per-contact Protein — COMPOSE with the visibility gate (never replace it;
  `export_package`'s subject-visibility check is the one enforcement point,
  blueprint XV.1), so a contact gets the visible-AND-selected intersection.
- [ ] CRDT text relay for collaborative record `head`/`body` editing:
  zero-delta `text_edit` provenance facts for the merged operations,
  cursors riding the existing ephemeral lanes. Unblocks live multi-Cell
  editing in Record.
- [ ] **Record extensions merge per field, not per blob.** Today a
  `record_extension` write replaces the whole `fds`, and Record's offline
  queue keeps one pending op per record (last write wins). So two Cells
  touching different parts of the same namespace lose one side: start a
  worklog on a phone, edit the estimate on a laptop, and whichever syncs
  second wins the entire `work` object — including the other's logs.
  - This belongs here rather than in whichever sand shows the extension.
    It is the same problem as concurrent `body` editing, one nesting level
    up: a JSON object whose fields have independent authors.
  - A list inside an extension (`work.logs`) is the sharp case, because
    append-vs-append is exactly what a CRDT resolves for free and what
    blob replacement is guaranteed to break.

# CRDT Sync Phase 2

## Implemented Baseline

Record-bundle sync exists: numeric local IDs stay local, sync IDs map rows across organs, ownership lives on records, organ policies control direction, and record-related tables move through the existing operation/snapshot/fingerprint flow.

Text CRDT relay exists: `record.head` and `record.body` writes enqueue durable `record_text_crdt_update` rows, expose pull/push/snapshot endpoints, materialize remote text through `SyncOrigin::RemoteCrdt`, and mark pushed updates with `sent_at`.

The current CRDT payload is homebrew plain-text replacement data. It is a durable relay and merge envelope, not yet collaborative character-level editing.

## Remaining Goal

Build one reusable Record editor sand that owns editing for `record.head` and `record.body` everywhere.

The editor may start with homebrew replacement updates and later switch to Y.js. Other sands must not implement their own record text editors. They embed the same editor so CRDT behavior, save behavior, sync identity, permissions, and future presence/cursors stay consistent.

## Architecture

Keep three layers:

1. Record operation sync: row identity, ownership, tombstones, sidecars, relations, work metadata, and non-text fields.
2. Text CRDT relay: durable text update rows, catch-up endpoints, organ-to-organ push/pull, and plain-text materialization.
3. Record editor sand: reusable UI/editor component for `record.head` and `record.body`.

Do not make relation graphs, Kanban cards, notes, or table CRUD own CRDT logic. They pass a record context into the Record editor sand.

## Record Editor Sand

Name: `record_editor`.

Responsibilities:

- edit `record.head`
- edit `record.body`
- load current materialized record text
- pull text CRDT snapshot/deltas for both fields
- submit local text updates
- expose clean embed lifecycle events
- render the same editing UI in standalone and embedded contexts
- hide all CRDT transport details from parent sands

Inputs from host or parent sand:

- `record_id`
- `record_sync_uid`
- `owner_organ_id`
- `mode`: `standalone` or `embedded`
- `field_policy`: `head_body`, `body_only`, or future allowlisted variants
- auth/session context inherited from the host

Outputs to host or parent sand:

- `record-created`
- `record-updated`
- `dirty-changed`
- `save-state-changed`
- `focus-requested`
- `error`

Rules:

- embedded mode never creates records
- embedded mode never shows the standalone record picker
- embedded mode receives a concrete record and edits only that record
- all local edits go through the text CRDT endpoint or the internal record write helper that enqueues CRDT updates
- parent sands subscribe to editor events and refresh their local view state; they do not parse CRDT updates directly

## Note Sand

Rename the current markdown editor concept to `Note`.

Solo `Note` behavior:

- while the title is empty, the note is frontend state only
- no `record` row exists before the user provides `record.head`
- when the user enters a title, create a `record`
- title maps to `record.head`
- markdown content maps to `record.body`
- after creation, editing is delegated to the Record editor sand

Solo `Note` record picker:

- show a small green status ball in the top right, like the document reader pattern
- clicking it opens record search/select
- selecting a record binds the Note to that existing record
- when bound to an existing record, the Note behaves as a Record editor shell
- this picker exists only in solo Note mode

Embedded `Note` behavior:

- no green status ball
- no record creation
- no record search
- parent sand passes the record context
- Note renders the Record editor sand for that record

Naming:

- user-facing sand name: `Note`
- internal reusable editor sand: `record_editor`
- do not expose "CRDT" in normal UI labels

## Embedded Use Cases

Relation sand:

- side panel embeds `record_editor`
- selected graph node supplies record context
- changing selected node destroys or rebinds the editor instance
- relation graph remains responsible for relation edges only
- record text changes come from editor events or stream refresh, not relation-local text logic

Kanban sand:

- focus card body editor should embed `record_editor`
- quick card previews remain read-only materialized text
- edit sheets should call the same record editor flow or delegate title/body controls to it

Table sand:

- generic CRUD may still edit scalar fields
- when editing `record.head` or `record.body`, prefer launching or embedding `record_editor`
- table-level edits that touch head/body remain supported as fallback because backend write helpers enqueue CRDT updates

## CRDT Strategy

Current homebrew mode:

- local text change stores a base64 JSON payload with `type = plain_text_replace`
- backend materializes `record.head` or `record.body`
- latest materialized text is the user-visible read model
- update ordering uses `update_clock`, `source_organ_id`, and `update_uid`

Future collaborative mode:

- use a field document: `record:<record_sync_uid>:head` or `record:<record_sync_uid>:body`
- use Y.js in the browser only if character-level concurrent editing is needed
- vendor Y.js as a pinned ESM asset with license and notices beside it
- wrap Y.js behind a local adapter module
- keep Rust as auth, storage, dedupe, fanout, compaction, and materialization layer
- add `yrs` only if backend compaction or trusted server-side materialization requires it

Adapter API:

- `openDocument(context)`
- `applyRemoteUpdate(update)`
- `observeLocalUpdate(callback)`
- `getMaterializedText()`
- `replaceMaterializedText(text)`
- `destroyDocument()`

The editor must support both adapters:

- `plain_text_replace` adapter for current implementation
- `yjs` adapter for future collaborative editing

## Transport Still Needed

HTTP already exists for snapshot, updates, and push. Add socket delivery for active documents.

Socket frames:

- `crdt_text_subscribe`
- `crdt_text_unsubscribe`
- `crdt_text_update`
- `crdt_text_ack`
- `crdt_text_presence`
- `crdt_text_error`

Rules:

- subscribe by `document_uid`
- server validates record read permission and organ policy before subscription
- local updates fan out to active subscribers immediately
- offline peers catch up through HTTP
- socket fanout is an optimization, not the source of truth

## Presence

Presence is future-only and ephemeral.

Rules:

- never store cursor state in SQLite
- identify actor as `user@organ`
- scope presence to one `document_uid`
- throttle cursor frames
- drop presence on socket close
- parent sands do not implement presence directly; the Record editor sand owns it

## Compaction

The update table must not grow forever.

Rules:

- compact per `document_uid`
- trigger by delta count or byte threshold
- write one `snapshot` row
- mark older delta rows `compacted_at`
- keep recent uncompacted deltas for active clients
- never delete uncompacted rows needed by unacked sessions
- materialized text after compaction must equal reconstructed text before compaction

In homebrew mode, compaction can collapse multiple `plain_text_replace` deltas into one latest snapshot.

In Y.js mode, compaction stores a merged Y.js document update.

## Idle And CPU Rules

No full polling loops.

Triggers:

- push local editor updates after debounce
- pull on editor open
- pull after organ sync reports text update mismatch
- slow periodic health check per organ

Health check:

- compare document clocks or hashes
- request only mismatched documents
- skip when no authenticated remote token exists
- skip when the organ policy disables record sync
- skip when no records in scope have CRDT text updates

Config still needed:

- per-organ text CRDT enable flag if policy needs to diverge from record sync
- per-organ check interval
- zero interval disables periodic checks but keeps open-document pull and push-on-change

## Delete And Lifecycle Rules

- record tombstone wins over older text updates
- reject new text updates for deleted records
- explicit undelete/recreate must create a new valid lifecycle operation before accepting more text
- keep CRDT updates for audit until retention cleanup exists
- standalone Note draft without title is not a record and has no CRDT identity
- creating a Note record initializes both text documents

## Future Implementation Steps

1. Create `record_editor` sand with standalone and embedded modes.
2. Rename the markdown note experience to `Note`.
3. Add Note draft state that creates no record until title is set.
4. Add Note green-ball record picker for solo mode only.
5. Embed `record_editor` in Note after record creation or selection.
6. Embed `record_editor` in Relation side panel for selected records.
7. Replace Kanban focus body editing with `record_editor` or an equivalent embedded editor mount.
8. Add socket subscriptions and fanout for active text documents.
9. Add compaction for homebrew `plain_text_replace` updates.
10. Add document clock/hash health checks.
11. Decide whether Y.js is necessary after the shared editor is used in real workflows.
12. If needed, vendor Y.js with license/notice files and implement the adapter behind the existing editor API.
13. Add optional `yrs` only if backend-side Y.js compaction/materialization is required.

## Tests

Keep `cargo check` passing. Warnings are errors.

Add focused tests for:

- Note draft does not create a record before title
- Note title creates a record and initializes editor context
- Note can bind to an existing record through picker
- embedded Record editor cannot create or switch records
- Relation side panel passes selected record context into Record editor
- local editor write creates `record_text_crdt_update`
- remote CRDT materialization does not re-enqueue a loop
- duplicate `update_uid` is ignored
- sent CRDT updates are not pushed again
- deleted records reject text updates
- compaction preserves materialized text
- socket subscriber receives local updates
- Y.js vendored asset includes license and notice files if added
