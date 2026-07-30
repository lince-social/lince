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

