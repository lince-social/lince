# Ontology

Ontology is Lince's specification for modeling information. One division
underlies everything else:

> **Records model things. Assertions model what is said about them. Concepts
> name those assertions. Lingua lets different Organs share or translate their
> meanings.**

Everything else — Protein, Sync, CRDT, federation — is a projection,
constraint, or feature built on this base.

## 1. Record: a modeled thing

A **Record** is one thing that matters to an Organ: a task, project, person,
message, tool, place, transfer, rule, note, or saved Protein. Meaning comes
from assertions, not from a fixed taxonomy.

```text
Record
    uid                 stable global identity; never inferred from a title
    slug?               optional local, human-friendly alias
    kind                small operational shape; `plain` by default
    head                short title
    body                canonical Markdown content
    quantity            exact cached current level
    unit?               Concept naming the quantity's unit
    place?              place reference
    origin Organ?       where this Record originated
    created_at, updated_at
```

- [x] Core fields above; `uid` is identity, slug is disposable local
  convenience, `kind` is operational (create a new one only for a distinct
  lifecycle or sidecar, never just to make a Record filterable).
- [x] Direct operations, deliberately small — identity/tag/relationship
  changes go through assertions (§3), not through Record columns:
  `create_record`, `edit_record_text`, `set_slug`, `set_unit`, `set_place`,
  `set_extension`, `set_quantity`/`add_quantity`, `deactivate`/`delete_record`.
- [x] Quantity is an exact decimal fold of hash-chained Facts, never a float
  column or SQL `SUM()`: negative is a Need, positive a Contribution, zero
  neither. Facts are ground truth; a quantity change appends a signed-when-
  possible delta.
- [x] Deactivation (quantity zero) keeps the Record and history visible.
  Deletion is a hard tombstone: hidden from normal reads, slug released, row
  and Fact chain kept for verifiable history.
- [x] Undo is compensation (append the inverse Fact, never erase one). Metadata
  edits append a zero-delta annotation Fact. Replaying a known Fact UID is a
  no-op.
- [x] `record_extension`: namespaced JSON sidecar (`record_uid`, `namespace`,
  `version`, `fds`), at most one per `(record, namespace)`. Escape hatch for a
  new workflow, not a substitute for shared meaning — promote a widely-used
  shape to a typed, versioned namespace that can refuse a version it doesn't
  understand.
  - example: task work data as `{ start, due, estimate_min, logs: [{start,
    end}] }` — a property of the task, not a quantity Fact.
- [ ] Extensions merge per field, not per blob. `set_extension` today replaces
  the whole `fds` in one write (`store/records.rs:241`), so two Cells editing
  different parts of the same namespace (a worklog on a phone, an estimate on
  a laptop) lose whichever synced first. Sharpest for list fields
  (`work.logs`), where append-vs-append is exactly what per-field merge fixes
  for free.

## 2. Organ: the sovereign Record holder

A **Cell** is one running Lince instance and its local data. An **Organ** is
the social/network boundary a Cell represents (personal, family, company,
project, community); other reachable Lince nodes are Organs regardless of
their internal arrangement. An Organ is a `kind = organ` Record; it owns its
Records, structure, policy, credentials, and private Lingua. Every Record
carries its origin Organ, preserved through relaying.

- [x] Contacts (`organ_contact`): `trust` (unknown/known/blocked), numeric
  `proximity` (local-only, never exported), independent `sync_out`/`sync_in`
  policy. `blocked` is terminal everywhere — import, discovery, export,
  delivery all reject.
- [x] Introduction: exchanges identity + public keys; adopting stores the
  remote Organ under its own uid. Organ identity is transport/trust, not
  permission for one Person to act or sign for another.
- [x] Sync transport: visibility-gated packages through a durable outbox/inbox
  with retry; imports verify hash-chain + signature, rejects go to quarantine
  verbatim, valid items still apply; import is idempotent by uid, quantity
  deltas commute. (Full mechanics in §11.)
- [x] Protein selects Records by origin Organ (`organ_eq`/`organ_in`).
- [x] File Sync: `lince.file_sync` extension (`enabled`, `path`) mirrors
  selected Records to Markdown files; disk edits return through the normal
  `EditRecordText` action. Disk wins on same-tick conflict; a missing file
  deletes its Record only after two consecutive misses (debounce against
  atomic editor saves). Config changes apply on next boot, not live.
- [x] Organ interface: lists `kind = organ` Records, edits File Sync config.
- [ ] Contacts manager UI: edit trust/proximity/sync policy, block a contact,
  inspect quarantine. Not built — today's Organ interface only lists organs
  and edits File Sync config.
- [ ] Organ polling scheduler (tracked under Transfer Phase T1, first
  acceptance is proposal delivery) — shared Sync infrastructure, not
  Transfer-owned transport.
- [ ] Per-contact Protein filter narrowing what `enqueue_sync_to` exports,
  composed with (never replacing) the visibility gate, so a contact receives
  the visible-AND-selected intersection. Today only the visibility gate
  applies.
- [ ] Live config supervisor: start/stop File Sync watchers when config
  toggles, instead of requiring a reboot.

## 3. Assertion: a statement about Records

An **Assertion** applies one Concept to a subject Record and optionally names
an object Record:

```text
assertion
    subject: Record
    predicate: Concept
    object: Record?       optional
```

```text
Brush teeth @task
Brush teeth @task [Morning routine]     ← binary: subject → object
Brush teeth @health
```

Tag and link are useful interface words for unary/binary assertions; neither
is a separate storage model. Canonical table:

```text
record_assertion
    uid, subject_uid, predicate_uid, object_uid?
    role                ordinary | identity
    quantity?, unit_uid?
    asserted_by?, created_at, retracted_at?, retracted_by?
```

- [x] Retraction, not in-place edit: a replacement is a new statement with a
  new uid, preserving the old one's provenance.
- [x] Identity assertion: at most one per Record, unary, unquantified —
  the constrained answer to "what kind of thing is this?" when specialized
  behavior needs one. A Record with no identity is valid when its domain
  permits it.
  - example: `Toothbrush @toothbrush` (identity), `Toothbrush @health` and
    `Toothbrush @cost` (both ordinary, coexisting).
- [x] Invariants: subject/predicate always exist; identity is unary and
  unquantified with at most one active per Record; an active unary
  `(subject, predicate)` or binary `(subject, predicate, object)` occurs at
  most once; direction is always subject→object (incoming is a query view);
  missing object means genuinely unary; missing unit means unspecified, not a
  wildcard.
- [x] Operations: `assert(subject, predicate, object?, quantity?, unit?,
  role?)` (idempotent for an existing active tuple), `retract(assertion_uid)`,
  `set_identity(subject, predicate?)` (atomically replaces old identity,
  promotes a matching unary assertion instead of duplicating it).
- [ ] `refine` helper: atomically turn `A @task` into `A @task [Project K]`.
  Not implemented — assert/retract remain the fundamental operations.

## 4. Concept: a named meaning

A **Concept** is the predicate vocabulary for assertions and the unit
vocabulary for quantities: stable uid, multilingual names, optional origin,
many-parent DAG.

```text
@bug is-a @task
@task is-a @work-item
```

- [x] Parentage means widening, not exclusive taxonomy — both `A @bug` and
  `A @bug [Project K]` answer `@task` queries. A Concept may have many
  parents (`@food` under both `@substance` and `@cost`). Inherited assertions
  resolve at query time, never copied into storage.
- [x] Concept equivalence joins dialects explicitly (distinct from parentage,
  broader/narrower mapping, or resemblance) without claiming shared history.
- [x] Units are Concepts. Conversion is explicit, exact, and valid only within
  a shared ancestor dimension: one authoritative rational factor per
  unordered pair, reverse by inversion. Lince never silently converts to make
  an expression type-check.
- [ ] Time-varying rates (currency exchange): needs a separate future model.
  A static conversion factor must not rewrite a historical value.

## 5. Lingua: shared vocabulary, not a global ruler

A **Lingua** is an identifiable collection of Concepts an Organ uses
privately, shares with peers, publishes, or adopts from elsewhere — common
ground between ontologies, not a required global schema.

```text
lingua
    uid, name, owner Organ?
    visibility          private | shared | public
    Concepts            adopted by membership
```

- [x] Adopting a foreign Concept preserves its uid and lineage; re-adoption is
  a no-op. Unknown precise Concepts may fall back to the nearest known
  ancestor while the original is retained.
- [x] Usage spans: a private personal Lingua, a small shared Lingua between
  peers, Institute-published defaults (non-ruling), an adopted/mapped famous
  external vocabulary, or genuinely unknown meaning left unknown rather than
  falsely normalized.
- [x] Concepts needed to interpret synchronized assertions travel with the
  data. Sync currently carries ordinary assertions when subject and (if
  present) object are both included; identity travels with the Record seed.

## 6. Query and projection

One assertion query surface replaces the old split between concept and
relation filters:

```text
@task                       predicate matches; object may be absent or present
@task []                    predicate matches; object is absent
@task [Project K]           predicate and object match
incoming @task [Project K]  Project K is the object
identity @task              identity assertion only
```

- [x] Concept matching widens through the DAG; results dedupe Records when
  several assertions match; a targeted assertion already satisfies a general
  predicate query (no redundant unary tag needed).
- [x] Binary assertions project as directed graph edges: depth, incoming/
  outgoing views, topological order, cycle warnings, and relationship
  quantities apply only to this projection. Several predicates may connect
  the same pair; an order-like predicate may loop (retain the valid assertion,
  warn the ordering view — mutual non-order assertions are ordinary).
- [x] Protein exposes Records, Assertions, Concepts, and Linguas, filtered/
  included by predicate, object, direction, hierarchy, and graph depth (hop
  carried on included binary assertions). Sands may call these views "tags,"
  "links," "dependencies," or "relations" — no second model underneath.

## 7. Ledger Facts: a separate assertion domain

Record assertions describe standing truths about a Record. A **Fact
classification** describes what one append-only Ledger movement meant — its
own signing/provenance rules, never merged with Record assertions.

- [x] Example: a flour Record has identity `@flour` (what the Record is)
  while one `-500 g` Fact is classified `@bread` (what that movement was),
  using the same Concept vocabulary.

## 8. Design checklist: constraints belong above the core

Not every predicate fits every Record pair. A Lingua, Sand, or protocol may
constrain expected subject/object identity, direction, cardinality, units,
permissions, or cycle behavior — those are constraints over assertions, never
new per-feature link tables. Before adding a field, sidecar, relation, or
transport message, ask:

1. What things are Records?
2. What standing or relational statements are assertions?
3. Which Concepts name them, and which Lingua owns or shares them?
4. Is this universal Record state, a typed extension, a Fact, or an assertion?
5. Which constraints belong to the feature/protocol rather than the core?

## 9. Federation and Blood: talking to other systems

An Organ owns its local Records and structure; shared vocabulary makes
interoperation possible without forcing matching Record structures. Keep
transport, shape translation, and semantic translation as separate layers:

```text
external transport or document
        ↓
named adapter: identity, shape, provenance
        ↓
Lingua mapping: external meaning ↔ Lince Concept
        ↓
local Records, Facts, assertions, and extensions
```

Blood carries and validates the envelope; Ontology explains the Records/
assertions it expresses; Lingua says which meanings are shared; policy
decides what the Organ accepts, reveals, trusts, or acts upon.

- [x] Adapter contract: preserve external identity, source version/context,
  original payload where appropriate, and anything untranslatable.
  Interpretation never silently becomes local authorship or authority.
- [ ] Schema.org mapping to a Lingua — no adapter built yet.
- [ ] Nostr protocol adapter (identity, addressing, signatures, relay
  delivery) — respected optional future integration, not the default. A
  normal Organ stays native Organ-to-Organ without a relay or Nostr identity.
- [ ] ActivityPub protocol adapter (delivery, addressing, signatures) — same
  status as Nostr: optional, not started.

## 10. Protein: the read contract

- [x] Six sources, one JSON shape: `record` (state vector, all predicates/
  includes), `promise` (`state_in`), `decision` (open Decision Queue, never
  exported to remote subjects), `fact` (the Ledger — `at_since`,
  `cause_kind_eq`, `record_eq`, `concept_in`), `concept` (Lingua vocabulary),
  `transfer` (bundles with derived status, parties, promises, balance).
- [x] Predicates: nested `all/any` groups with condition-level `not` (10
  indentation levels), `quantity_lt/lte/gt/gte/eq`, `uid_eq`, `kind_eq`,
  `slug_eq`, `concept_in` (DAG-aware), generic directional assertion,
  `text_contains`, Record `work_date`, `state_in`, `near`.
- [x] Includes: `facts` (provenance), `promises`, binary assertions (predicate,
  direction, depth + hop), `threads` (nested messages), `extension`,
  `availability`, `projection` (`{at:"+7d"}` folds agreed/active promise
  deltas — full rule simulation is the engine-side `project`/`snapshot` pair).
- [x] Aggregation (`sum`/`count` by concept/kind on records, by
  cause_kind/day/concept on facts) — visibility gate applies BEFORE
  aggregation, so hidden rows can't leak through sums.
- [x] Saved Proteins are records (`kind='protein'`) referenced by slug — the
  old "view" concept, done right.
- [x] Maneirisms: wire `where` is a JSON array = implicit `all`; fact-source
  predicates don't nest (flat list) in v1; `at_since: "30d"` resolves against
  wall-clock now (use absolute RFC3339 for reproducible reads); remote
  subjects see only whole-row visibility grants — Decision Queue and
  concept-level promises never leave a Cell through Protein.
- [ ] Verifiable aggregates: a `verified: true` Protein filter restricting an
  aggregate to signed facts only, so e.g. "@maria's kept-promise ratio for
  @food, last 12 months" is provable to a counterparty without either side
  trusting the computing Cell (leans on `docs/Transfer.md`). Opt-in only,
  between mutually-confiding organs — never a global or public score.

## 11. Sync: batch record/fact exchange between Organs

CRDT (§12) and Sync are the same idea at two scales: CRDT is fine-grained
live operational state (who's editing this record right now); Sync is
coarse-grained batches of whole records/facts moving on their own schedule.
One relay, two granularities.

- [x] Introduction: `GET /organ/introduction` returns identity + public keys;
  `adopt_introduction` registers the contact under the REMOTE organ's own uid
  and stores its keys so its signed facts verify.
- [x] Push: `enqueue_sync_to(organ)` builds a visibility-gated package (same
  gate Protein uses) into `sync_outbox`; `drain_outbox` sends with retry over
  `POST /organ/inbox` (failures stay queued).
- [x] Import hardening: every incoming fact passes its hash-chain step and
  signature; rejects land verbatim in `sync_quarantine` with a reason, the
  rest of the package still applies. Import is idempotent by fact uid;
  quantity sync is conflict-free by construction (deltas commute).
- [x] Concepts needed to understand imported assertions ride along a package
  with lineage before Records land (§5).
- [x] Discovery: `GET /organ/open-promises` exports OPEN promises a subject
  may see; `refresh_discovery` upserts them into the local cache, stamping
  proximity from OUR contact row (proximity never travels outward).
- [x] Organ-scoped selection: every record carries `organ_uid` (origin,
  stamped on creation, preserved through relay hops); Protein's
  `organ_eq`/`organ_in` select "every record belonging to organ X."
- [x] File Sync to disk (`Engine::file_sync_tick`, restoring the pre-refactor
  `file_sync.rs` convention): every record whose origin is a given organ
  mirrors to `{head}.md` (collisions disambiguated `{head} -- {uid}.md`) in a
  directory, both ways; `spawn_configured_watchers` runs at boot per enabled
  organ (2s tick). Selection is hardcoded to `organ_eq` for v1.
  - [ ] Arbitrary configurable Protein filter for File Sync selection —
    deferred, not wired to anything.
- [ ] Per-contact Protein narrowing on top of the visibility gate (§2).
- [ ] Organ polling scheduler (§2, tracked under Transfer T1).

## 12. CRDT: live collaborative editing

Architecture — three layers, none may leak into the others:

1. **Record operation sync** (row identity, ownership, tombstones, sidecars,
   assertions, work metadata, non-text fields) — this is §11, and it exists.
2. **Text CRDT relay** (durable text update rows, catch-up endpoints, organ-
   to-organ push/pull, plain-text materialization) — **removed** along with
   legacy persistence; not present in the current store. Needs rebuilding
   from scratch against the current schema.
3. **Record editor sand** (reusable UI owning `record.head`/`body` editing
   everywhere) — never built.

Relation graphs, Kanban cards, notes, and table CRUD must never own CRDT
logic themselves — they pass a Record context into the Record editor sand.

- [ ] **Text CRDT relay** (layer 2): `record_text_crdt_update` rows
  (`update_uid`, `document_uid`, `record_sync_uid`, `field_name`,
  `source_organ_id`, `update_clock`, `update_kind`, `update_bytes_base64`,
  `materialized_text`, `sent_at`, `compacted_at`) with pull/push/snapshot
  endpoints; ordering by `update_clock`+`source_organ_id`+`update_uid`;
  zero-delta `text_edit` provenance facts.
- [ ] **`record_editor` sand** (layer 3) — standalone and embedded modes.
  - Responsibilities: edit `record.head`/`body`; load materialized text; pull
    CRDT snapshot/deltas; submit local updates; expose lifecycle events
    (`record-created`, `record-updated`, `dirty-changed`, `save-state-changed`,
    `focus-requested`, `error`); hide CRDT transport from parent sands.
  - Inputs: `record_id`, `record_sync_uid`, `owner_organ_id`, `mode`
    (standalone/embedded), `field_policy` (`head_body`/`body_only`/future),
    inherited auth/session.
  - Rules: embedded mode never creates records or shows the record picker,
    edits only the concrete record it's given; all local edits go through the
    CRDT endpoint or the write helper that enqueues CRDT updates; parents
    subscribe to editor events rather than parsing CRDT updates directly.
- [ ] **`Note` sand** (rename of the current markdown editor):
  - solo mode: title-empty note is frontend-only (no `record` row); entering
    a title creates the record (title→`head`, markdown→`body`) and hands off
    editing to `record_editor`; a green status-ball picker (top right, like
    the document-reader pattern) lets the user bind to an existing record
    instead.
  - embedded mode: no status ball, no creation, no search — parent passes
    record context, Note renders `record_editor` for it.
  - naming: user-facing name stays `Note`; `record_editor` is internal; never
    say "CRDT" in normal UI labels.
- [ ] **Embed `record_editor` into existing sands**:
  - Relation: side panel embeds it for the selected graph node; switching
    node rebinds/destroys the instance; Relation keeps owning its binary-
    assertion projection (asserted/retracted/remote refresh) independent of
    editor text state.
  - Kanban: focus-card body editor embeds it; quick-card previews stay
    read-only materialized text.
  - Table: generic CRUD may keep editing scalar fields directly; editing
    `head`/`body` prefers launching/embedding `record_editor`, with direct
    table edits as a fallback (write helpers already enqueue CRDT updates).
- [ ] **CRDT strategy — homebrew mode now, Y.js later**:
  - homebrew: local change stores a base64 JSON `plain_text_replace` payload;
    backend materializes and serves latest text as the read model.
  - future: field documents (`record:<sync_uid>:head`/`:body`); Y.js in-
    browser only if character-level concurrency is needed, vendored as a
    pinned ESM asset with license/notices, wrapped behind a local adapter
    (`openDocument`, `applyRemoteUpdate`, `observeLocalUpdate`,
    `getMaterializedText`, `replaceMaterializedText`, `destroyDocument`); Rust
    stays auth/storage/dedupe/fanout/compaction/materialization; add `yrs`
    only if backend-side Y.js work requires it.
  - the editor must support both the `plain_text_replace` and future `yjs`
    adapters behind the same interface.
- [ ] **Socket transport** for active documents (HTTP already covers
  snapshot/update/push): frames `crdt_text_subscribe`, `_unsubscribe`,
  `_update`, `_ack`, `_presence`, `_error`; subscribe by `document_uid`;
  server validates read permission + organ policy before subscribing; local
  updates fan out immediately; offline peers catch up over HTTP; sockets are
  an optimization, never the source of truth.
- [ ] **Presence** (future, ephemeral only — never persisted to SQLite):
  actor identified as `user@organ`, scoped to one `document_uid`, throttled,
  dropped on socket close; owned entirely by `record_editor`, never
  implemented by a parent sand.
- [ ] **Compaction** so the update table doesn't grow forever: per
  `document_uid`, triggered by delta count or byte threshold, writes one
  `snapshot` row and marks older deltas `compacted_at`, keeps recent
  uncompacted deltas for active clients, never deletes rows an unacked
  session still needs; materialized text must match before/after. Homebrew
  mode collapses multiple `plain_text_replace` deltas into one snapshot;
  Y.js mode stores a merged document update.
- [ ] **Sync triggers, no polling loops**: push after local debounce, pull on
  editor open, pull after organ sync reports a mismatch, slow periodic
  per-organ health check (compare clocks/hashes, request only mismatches,
  skip when no remote token / record-sync disabled / nothing in scope has
  CRDT updates). Config: per-organ enable flag, per-organ check interval
  (zero disables periodic checks but keeps open/push triggers).
- [ ] **Delete/lifecycle rules**: a record tombstone wins over older text
  updates; new text updates against a deleted record are rejected; undelete/
  recreate must create a new valid lifecycle op before accepting more text;
  CRDT updates are kept for audit until retention cleanup exists; a title-
  less Note draft has no CRDT identity; creating a Note record initializes
  both text documents. (Today, deletion is a hard tombstone with no
  `Undelete`/`Restore` action at all — this whole item is future work.)
- [ ] **Test coverage** once the above exists: Note draft creates no record
  before a title; title creates the record and editor context; Note binds to
  an existing record via the picker; embedded editor cannot create/switch
  records; Relation side panel passes selected-record context correctly and
  its own sync still refreshes binary assertions without owning CRDT state;
  local writes create `record_text_crdt_update` rows; remote materialization
  never re-enqueues a loop; duplicate `update_uid` is a no-op; sent updates
  aren't re-pushed; deleted records reject text updates; compaction preserves
  materialized text; socket subscribers receive local updates; any vendored
  Y.js asset ships its license/notice files.
