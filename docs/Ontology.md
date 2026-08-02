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
- [x] Organ interface: lists `kind = organ` Records, edits File Sync config,
  and — folded into the same sand as a friends-list panel — edits contact
  trust (unknown/known/blocked) and proximity, with block/unblock as
  `trust: "blocked"`/`"known"` (`set-contact-trust`/`set-contact-proximity`
  Actions, `contact` Protein include on `organ_contact`). Scope is
  trust/proximity only; sync policy (`sync_out`/`sync_in`) and quarantine
  inspection stay out of this component.
- [x] Live config supervisor: start/stop File Sync watchers when config
  toggles, instead of requiring a reboot (`engine::file_sync::spawn_supervisor`,
  reacts to `SetExtension` facts on the bus; boot still seeds via the same
  reconcile pass).

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
- [x] `refine` helper: atomically turn `A @task` into `A @task [Project K]`
  under the same predicate (`store::assertions::refine`, `RefineAssertion`
  Action) — a transactional wrapper over retract+assert, not a new operation;
  idempotent if the binary tuple already exists.

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

## 11. Sync: one channel, two persistence modes

Sync and CRDT are one capability: an authenticated connection to another
Organ carrying deltas over one channel. Per contact, the connection has a
persistence mode. **Live**: data stays at the remote Organ; a sand accesses
it in memory over a Protein WebSocket, and writes go through the *remote*
Organ's change handler, fanning out to every connected peer — multiplayer
editing falls out for free, nothing is stored locally. **Replica**: the
syncable tables are copied locally and kept converged. Login authenticates
both; live editing is on by default in both, because it *is* the delta
channel applied to an open editor.

Syncable tables: `record`, `record_extension`, `concept`,
`record_assertion`, `record_fact`. `organ_contact` (trust/proximity) never
leaves a Cell; promises travel only through discovery (§10 visibility).

The unit of sync is the **op**, not the row. Every local write becomes a
field-level operation — `(table, uid, field, value, hlc, actor)` — stamped
with an HLC and appended to a local **op log** with a monotonic sequence
number. Facts were already ops (append-only, idempotent, commuting); this
makes every syncable table op-shaped, so one engine serves fact and
non-fact tables alike. (An HLC — hybrid logical clock — is a timestamp that
follows wall-clock time but is bumped past any remote timestamp it sees, so
"latest wins" stays consistent across machines with skewed clocks and never
goes backwards.)

Two mechanisms keep replicas converged, and neither ever rescans a table:

1. **Reactive deltas** — every write passes the one central change-handling
   function, which fans out to whatever features are enabled; sync is one
   such feature and immediately pushes the op to each synced Organ.
2. **Catch-up reconciliation** — each pairing keeps a checkpoint (the last
   op sequence the other side acknowledged). Every 30s and on reconnect, an
   Organ asks "ops after my checkpoint"; the answer is one
   indexed query. Converged means an empty answer — the cycle is O(1), and
   after a gap the cost is O(missed ops), never O(table). Any delta the
   reactive path dropped is, by definition, an op past the checkpoint, so
   catch-up finds it without hashing anything.

Merge intelligence is split along one line. **Record state** (head/body
text, scalar columns, extension namespaces, list fields) lives in one
**Loro doc per record** — vendored, MIT, pinned — where per-key LWW maps,
text CRDTs, and movable lists make different-field edits, concurrent text,
and concurrent list moves merge correctly *by construction*; SQLite always
holds the materialized current values, so queries never touch Loro and the
data outlives the dependency. **The ledger** (facts, assertions, record
tombstones) stays homebrew: signed, hash-chained, individually inspectable
rows with HLC ordering — accountability semantics no CRDT register can
give. Quantity is a fold of facts and must never become a CRDT value: LWW
would silently drop one of two concurrent payments. Deletes are tombstone
ops that replicate like any write, which is what makes catch-up unable to
resurrect deleted data.

### Transport & identity

- [x] Introduction: `GET /organ/introduction` returns identity + public keys;
  `adopt_introduction` registers the contact under the REMOTE organ's own uid
  and stores its keys so its signed facts verify.
- [x] Push: the payload is op batches ONLY (`WireOp`/`OpBatch`; facts ride
  hydrated inside their op). Enqueue happens at op-append time into the
  bounded outbox; `drain_outbox` builds one batch per contact and retries
  over `POST /organ/inbox` (failures stay queued). The pre-op Package format
  is deleted — no migration, no back-compat.
- [x] Import hardening: every incoming fact passes its hash-chain step and
  signature; rejects land verbatim in `sync_quarantine` with a reason, the
  rest of the batch still applies. Import is idempotent by op identity AND
  fact uid; quantity sync is conflict-free by construction (deltas commute).
- [x] Concept ops ride the same log; assertions arriving before their
  concepts create stub rows resolved by the concept's own op (§5 lineage
  now travels as ordinary `concept` ops).
- [x] Discovery: `GET /organ/open-promises` exports OPEN promises a subject
  may see; `refresh_discovery` upserts them into the local cache, stamping
  proximity from OUR contact row (proximity never travels outward).
- [x] Organ-scoped selection: every record carries `organ_uid` (origin,
  stamped on creation, preserved through relay hops); Protein's
  `organ_eq`/`organ_in` select "every record belonging to organ X."

### Peers: identity, discovery, proximity

An Organ's identity is its keypair, never its address. The public keys
exchanged at introduction ARE the organ; IPs, ports, and hostnames are
hints that may rot or be taken over by a stranger. Nothing flows on any
connection until the far side proves possession of the private key — the
"is my old friend still living at this address" check is cryptographic,
not postal.

Built — possession proven both directions before a single op moves: every
sync request (`/organ/inbox`, `/organ/ops`) carries an ed25519 signature by
the caller's organ key over `(method, path, timestamp, body hash)` with a
120s freshness bound; unknown organ, stale stamp, or bad signature is a
plain 403, and inbox additionally requires the batch's `from_organ` to be
the proven signer. Responses are signed the same way and the sync runner
verifies them against the contact's STORED key before importing a byte — a
peer at a known address that fails verification is a stranger: drop, keep
the contact, flag the address stale. (Stateless signed-timestamp form of
the challenge: replay is bounded by the window and op-identity idempotency
makes a replayed batch a no-op.) `/organ/introduction` stays open — it is
how strangers meet. Address book: `organ_contact.last_seen_addr` is a
cached hint written ONLY after a verified exchange from a new address; the
runner tries last-verified → base_url → LAN sighting — no trust-on-IP,
ever. LAN discovery, LocalSend-style: UDP multicast announce on
`224.0.0.167:54917` every 5s ± jitter carrying `{organ_uid, pubkey
fingerprint, api port, display name}` (no secrets); nearby peers expire
after ~3 missed announces; a fingerprint matching a known contact becomes
an UNVERIFIED address candidate only. Gated by the `lince.discovery`
extension (default enabled); scope is LAN multicast only — internet-wide
discovery, NAT traversal, and relays are explicitly out. `GET
/organ/nearby` and `POST /organ/pair` (introduction + adopt + derived
code) serve the Organ sand.
- [ ] Discovery UI in the Organ sand: a "nearby" list of announced organs;
  selecting one runs the normal introduction + challenge flow and, on
  confirmation, adds it to contacts. Display names are untrusted labels —
  the UI must never present a name as identity.
Built: the verification code (Signal safety-number pattern) —
`engine::peers::verification_code` derives base32 (A-Z2-7) of the first 25
bits of `sha256(sorted both organs' pubkeys)` → 5 chars like `Y3HS4`;
symmetric, deterministic, unit-tested. Both sides derive it from the
introduction's keys and the humans compare out loud; matching codes prove
no third party substituted keys on the wire. Derived, never transmitted —
only `/organ/pair`'s own reply carries the local derivation for the UI.
- [ ] Physical-proximity signals (same-LAN sighting, BLE, UWB) are a
  post-sync feature — §13; discovery here only finds peers, it never
  scores nearness.

### Op log

The `sync_op` table exists: `(seq, tbl, uid, field, kind, value, hlc,
actor_organ)`, `seq` an AUTOINCREMENT rowid so pruned seqs never return; the
unique index on `(actor_organ, hlc)` IS the op uid and the import
idempotency check — no UUID column. Every syncable store writer logs its own
ops where the SQL happens (record fields, per-KEY extension diffs so two
Cells editing one namespace never clobber each other, assertion
set/tombstone per uid, concept renames, facts as kind `fact`), skipping
silently when no local organ exists yet. Applying a remote op appends it too
(original HLC, local `seq`) so downstream contacts can relay.

- [ ] Op kinds: `set` (field value), `tombstone` (delete record/assertion/
  extension-key), `fact` (existing signed fact rows, unchanged semantics —
  they join the log rather than a parallel channel), `crdt` (a binary Loro
  update for one record-doc — commutes by construction, so ordering is
  irrelevant and idempotency is Loro's own dedupe plus the op identity).
  Snapshots are NOT an op kind: bootstrap and visibility grants serve the
  current row state synthesized from the read model at serve time (each
  field carrying its stored HLC; record-docs as a Loro shallow snapshot),
  so the log holds only real writes and never bloats with copies of state.
The HLC is one packed 64-bit integer (`nucleus::hlc`): 48 bits wall-clock ms
+ 16-bit logical counter — a single `INTEGER` column, native int
compare/sort/index. One clock per Cell, stamped on every local op, advanced
past any imported HLC and past the log's max at boot.

- [ ] Retention: an op is prunable once every synced contact's checkpoint
  has passed it (and, for text, once compacted). A contact that fell behind
  pruning re-bootstraps from serve-time snapshots — never a full-table diff.

### Reactive deltas

Built: appending any op enqueues it to every `sync_out` contact in the same
statement flow (one `INSERT … SELECT … ON CONFLICT DO UPDATE`), so sync IS
the write path, not a pipeline beside it. The outbox is bounded: at most one
queued op per `(contact, tbl, uid, field)` — a newer `set` replaces the
queued one, so a burst of typing while a peer is offline queues one op, not
thousands. The boot-time sync runner wakes on any fact-bus event (250ms
burst coalesce) and drains immediately; relayed imports are never echoed to
their source. Reactive ops apply through the same import path as catch-up
batches (signature, quarantine, merge policy) — no trust shortcut.

- [ ] Transport reuse: when a live Protein WS to the contact is already
  open, reactive deltas ride it; the HTTP outbox drain is the fallback, not
  a second channel — the one-WS rule stays intact.

### Catch-up reconciliation

Built: `organ_contact.last_synced_seq` per pairing; the sync runner pulls
`GET /organ/ops?after=<checkpoint>&limit=` (default 500, cap 2000) per
`sync_in` contact — one indexed rowid-range query, empty answer = converged
= O(1); the checkpoint advances only after the batch imports successfully,
and the feed's `from_organ` must match the contact. Per-contact
`catchup_interval_secs` (default 30, clamped 5–300); `0` disables the pull
cycle but keeps reactive deltas. The engine is visibility-free: an
authenticated contact gets the full feed; record hiding is §12's serve-time
filter, never state on the write path.
- [ ] Integrity audit — on-demand command, never a loop: checkpoints trust
  the peer's log, so a corrupted/buggy peer log is invisible to catch-up.
  `audit(organ)` walks both synced sets in uid order, streams
  `(uid, field_hlc_hash)` pages, and reports rows whose state disagrees
  despite equal checkpoints; repair reuses the normal import path.

### Merge: Loro record-docs + the homebrew ledger

Built: one Loro doc per record for its TEXT — `head` and `body` as Loro text
containers (character-level concurrent editing, `engine::collab` is the only
module that may import Loro). Scalar columns, extension namespaces (already
per-KEY ops), assertions, and concepts stay on the homebrew set/tombstone
LWW path — per-field ops already give per-key-map semantics, so the doc
carries only what benefits from a CRDT: text now, movable lists when the
client lands (list scope is *visual ordering only*: a kanban move BETWEEN
columns changes quantity and/or @concept — ledger/assertion territory,
never doc state). Each `crdt` op's value is the cumulative tail since the
last stored snapshot, which makes bounded-outbox replacement lossless;
compaction (≥100 ops or ≥256 KiB) stores a full snapshot in `record_doc`
(shallow snapshots deferred to §retention — a peer compacting at a
divergent frontier could otherwise produce unimportable tails). Docs are
lazy (snapshot + tail on first touch, LRU cap 64, zero memory for untouched
records). Doc seeding from pre-CRDT text uses a deterministic peer id
derived from (uid, head, body), so two Cells seeding the same replicated
record produce identical ops that dedupe instead of doubling the text.
Materialized read model, always: applying any text write or `crdt` op
immediately writes `record.head`/`record.body` back to SQLite; Protein,
queries, File Sync, and Archive read ONLY SQLite — if Loro vanished
tomorrow the data is plain rows and only concurrent merging degrades.
Built: deletes are tombstone ops, never row removal — a record tombstone
freezes its doc (`crdt` ops skip apply but still relay); undelete is a
newer write. Per-table application is one plain `match` in
`import_op_batch` — facts: set union with unchanged hash-chain + signature
checks (quantity stays a fold, structurally excluded from any doc);
assertions: set/tombstone per uid, later HLC wins; record text: `crdt` op →
doc → materialize; other record fields + extensions: per-field LWW;
concepts: plain per-field LWW. Physical cleanup after checkpoints pass is
§retention.

### Modes: live and replica

- [ ] Per-contact mode on top of `sync_out`/`sync_in`: `live` opens a
  Protein WS session against the remote Organ (in-memory access, remote
  change handler is authoritative, zero local rows); `replica` pulls an
  initial snapshot then rides reactive deltas + reconciliation.
- [ ] Organ sand controls the whole pairing per contact: outgoing sync
  (my records go there), incoming sync (their records land here), both, or
  live-only — driven by the existing `sync_out`/`sync_in` flags plus mode.
- [ ] Organ polling scheduler (§2, tracked under Transfer T1).

### File Sync (downstream consumer of the replica)

- [x] File Sync to disk (`Engine::file_sync_tick`, restoring the pre-refactor
  `file_sync.rs` convention): every record whose origin is a given organ
  mirrors to `{head}.md` (collisions disambiguated `{head} -- {uid}.md`) in a
  directory, both ways; `spawn_configured_watchers` runs at boot per enabled
  organ (2s tick). Selection is hardcoded to `organ_eq` for v1.
- [x] Per-organ directory: `lince.file_sync` (`enabled`, `path`) is configured
  per organ Record, so the local Organ mirrors to `mydir/` while a replicated
  remote Organ mirrors to `work/` — organ sync fills the replica, File Sync
  projects it to that organ's own directory.
- [x] Extensions are namespace-isolated (`record_extension` keyed by
  `(record_uid, namespace)`, §1): one Cell writing `lince.file_sync` can never
  clobber another namespace on the same record.
- [ ] Arbitrary configurable Protein filter for File Sync selection —
  deferred, not wired to anything.

### Collab: Loro, the reusable binding, live editing everywhere

Three layers, none may leak into the others: (1) op sync — the sections
above; (2) the **Loro engine** — vendored library + `crdt` op relay +
shallow-snapshot compaction; (3) the **collab binding**, one reusable
client element that gives ANY sand surface multiplayer editing, with
`record_editor` as the rich UI built on top of it. Relation graphs, Kanban
cards, notes, and table CRUD must never own CRDT logic — they mount the
binding (or `record_editor`) and pass a Record context, nothing more.

"Instant save" is a UI behavior, not a mechanism: the binding commits
through the normal write path after a short debounce (~500ms of no
keystrokes), the fanout does the rest; no save button, no separate
unsaved-state layer. Text older than the debounce is always committed; the
cosmetic gap (cursors, "who's typing") is presence, ephemeral only.

Built — **Vendoring Loro**: Rust `loro = "=1.13.9"` pinned in the
workspace `Cargo.toml`, all calls confined to `engine::collab` (yrs stays
the named fallback); browser `loro-crdt@1.13.9` (npm, SAME release as the
crate) vendored at `crates/web/src/sand/collab/vendor/` and served on the
always-registered routes `/board/vendor/loro-index.js`, `/loro_wasm.js`,
`/loro_wasm_bg.wasm` (siblings on purpose: the glue resolves the wasm
relative to `import.meta.url`) with the MIT license beside them at
`/board/vendor/loro.LICENSE.txt`. Upgrades bump both pins together.

Built — **`crdt` op relay** (layer 2): local doc changes export as
cumulative Loro update tails and ride the op log as `crdt` ops (the
bounded outbox replaces per (contact, record) losslessly because each
tail is a superset of the last); remote `crdt` ops apply through
`engine::collab`, then materialize head/body to SQLite; a zero-delta
Sync refresh fact per touched record wakes Protein and collab sessions.
- [ ] **Compaction = Loro shallow snapshot**: per record-doc, triggered by
  update count or byte threshold; store one snapshot, prune older `crdt`
  ops under the normal checkpoint-gated retention; loading a doc is
  snapshot + tail — never a history replay, so cost is O(current state),
  not O(edit history). Materialized text must be identical before/after.
First cut built (2026-08-02): the Record sand's body textarea binds
directly to the record-doc over `H.collabJoin`/`H.collabUpdate` —
join-snapshot seeds a client LoroDoc, keystrokes fold in as single-region
diffs (200ms debounced send), remote snapshots merge caret-preserving,
and in-flight typing is folded before any remote reflect so it is never
clobbered. The REUSABLE element below (paths beyond `body`, presence,
events out) is still open:

- [ ] **Collab binding** (layer 3, the reusable element): one client-side
  element/module that binds a DOM surface to `(record_uid, path)` where
  path targets doc content — `head` or `body` (text), `record.<column>`
  (map key), `<namespace>.<key>` (map key), or a movable list.
  - Owns: doc open/close through the adapter (LRU, lazy), subscribe over
    the one WS, local edits → debounced `crdt` ops, remote ops → surface
    patch (no full re-render), presence, connection state.
  - Used by: record sand body, kanban focus-card body, table cells,
    Relation side panel, future surfaces — record sand and a kanban card
    editing the same record converge automatically because they bind the
    SAME doc; there is no kanban-specific sync code, ever.
  - Contract: fact-backed values (quantity) are structurally excluded —
    the binding refuses them; quantity displays update live because fact
    ops arrive on the same channel, not because the number is a CRDT.
  - Events out: `remote-change`, `dirty-changed`, `save-state-changed`,
    `presence-changed`, `error` — parents react to events, never parse
    CRDT payloads.
- [ ] **`record_editor` sand** — the rich UI on top of the binding,
  standalone and embedded modes. Rich editing lives HERE, above the CRDT:
  the doc stores plain markdown text; slash commands are input affordances
  that insert markdown/block syntax at the caret (and `/slash` blocks stay
  a record_info product, K-plan unchanged); images are markdown links
  rendered at display time through the local `/host/media` pipeline;
  preview/rendering never writes. Because rich features are a layer over
  plain text, they need zero CRDT awareness and remote edits can never
  corrupt a block — worst case is concurrent text inside one block,
  which Loro text merges character-wise.
  - Inputs: `record_id`, `owner_organ_id`, `mode` (standalone/embedded),
    `field_policy` (`head_body`/`body_only`/future), inherited auth/session.
  - Rules: embedded mode never creates records or shows the record picker,
    edits only the concrete record it's given; ALL writes go through the
    binding; parents subscribe to editor events.
- [ ] **`Note` sand** (rename of the current markdown editor):
  - solo mode: title-empty note is frontend-only (no `record` row); entering
    a title creates the record (title→`head`, markdown→`body`) and hands off
    editing to `record_editor`; a green status-ball picker (top right, like
    the document-reader pattern) lets the user bind to an existing record
    instead.
  - embedded mode: no status ball, no creation, no search — parent passes
    record context, Note renders `record_editor` for it.
  - naming: user-facing name stays `Note`; `record_editor` and the binding
    are internal; never say "CRDT" or "Loro" in normal UI labels.
- [ ] **Embed into existing sands**:
  - Relation: side panel embeds `record_editor` for the selected graph
    node; switching node rebinds/destroys the instance; Relation keeps its
    binary-assertion projection independent of editor state.
  - Kanban: focus-card body embeds `record_editor`; quick-card previews are
    read-only materialized text from SQLite (no doc load for closed cards —
    a 200-card board costs zero Loro memory until a card opens).
  - Table: scalar cells may use the bare binding on `record.<column>` /
    `<namespace>.<key>`; `head`/`body` prefer embedding `record_editor`.
Built — **Socket transport** for active docs, on the ONE existing WS:
`collab_join` (reply: `collab_state`, the doc's full snapshot),
`collab_leave`, `collab_update` (client Loro update bytes → engine merge →
one cumulative `crdt` op + materialize + refresh fact). Fan-out rides the
fact bus: any fact touching a joined record pushes `collab_change` with
the merged snapshot — the same signal covers a sibling session typing AND
a peer Organ syncing in, and client imports dedupe by version vector, so
over-delivery is harmless. The board bridge multiplexes per-record
membership (`lince:collab-*` frames, rejoin-with-snapshot on reconnect).
Still open: `_ack`/`_presence` frames, and per-subject read-permission
gating on join (collab frames are currently a trusted-session capability
like terminals).
- [ ] **Presence** (ephemeral only — never persisted to SQLite): actor
  `user@organ`, scoped to one record-doc, throttled, dropped on socket
  close; owned entirely by the binding, never by a parent sand.
- [ ] **Delete/lifecycle rules**: a record tombstone freezes its doc —
  new `crdt` ops against it are rejected; undelete must land as a newer
  lifecycle op before edits resume; a title-less Note draft has no doc; a
  new record's doc initializes from its materialized columns. (Today
  deletion is a hard tombstone with no `Undelete`/`Restore` action — that
  part is future work.)
- [ ] **Test coverage** once the above exists: Note draft creates no record
  before a title; title creates the record and its doc; Note binds to an
  existing record via the picker; embedded editor cannot create/switch
  records; two bindings on the same record (record sand + kanban card)
  converge both ways; concurrent edits to different fields/keys both
  survive; concurrent moves in a movable list produce no duplicates; local
  edits append `crdt` ops; applying a remote op materializes SQLite and
  never re-enqueues a loop; duplicate op identity is a no-op; a deleted
  record rejects `crdt` ops; shallow-snapshot compaction preserves
  materialized text and doc load never replays full history; slash-command
  insertion and image rendering survive a concurrent remote edit; socket
  subscribers receive local updates; the vendored `loro-wasm` asset ships
  its LICENSE/notice files and both pins are the same version.

## 12. Visibility: what a logged-in Organ may see

Deliberately simple: login to an Organ plus `read record` permission grants
full visibility of the sync feed; hiding is per-record (whole rows kept out
of a contact's feed), never per-field. What §10 already excludes stays
excluded — Decision Queue, concept-level promises, and proximity never
leave a Cell. The visibility gate is the *single* filter applied when
serving ops, snapshots, and audits; nothing else decides what travels.

- [ ] Per-record hiding: the existing visibility gate, applied at feed-serve
  time, is the one mechanism; no per-contact state on the write path.
- [ ] Per-contact Protein narrowing on top of the visibility gate (§2).
- [ ] Grant: a record becoming visible enters the contact's feed as a
  snapshot generated at serve time from the read model — grants don't write
  history into the op log.
- [ ] Revoke — the honest part: revoking stops all future ops for that
  record, but the remote already holds what it saw, and a "delete your copy"
  instruction is *unenforceable* — the remote runs its own code and promised
  nothing. Spec it as two separate things: revoke (hard, local, guaranteed:
  nothing more travels) and `forget` request (a polite op the remote MAY
  honor; honoring is a trust signal, not a protocol guarantee). Never let UI
  imply revoke reaches into another Organ.
- [ ] Mid-stream changes vs checkpoints: hiding a record after some of its
  ops were served filters only *subsequent* ops; no history rewriting, no
  checkpoint rollback. A re-grant later re-snapshots current state rather
  than replaying the hidden gap.

## 13. Proximity: physical nearness as a local signal (post-sync)

`organ_contact.proximity` is a numeric, LOCAL-ONLY score of how physically
near a contact tends to be — it never travels outward (§2) and nothing in
sync depends on it. This section is future work, layered after sync ships;
it exists so the signals below land in one place instead of leaking into
discovery (§11), which only *finds* peers and never scores nearness.

How a device can know something is physically near, from weakest to
strongest signal:

1. **Same network.** Seeing a contact's challenge-verified LAN announce
   means "same router as me" — usually the same building, but a large
   office or a VPN can stretch that. Free (discovery already produces it),
   works on desktop, and is the only signal that needs no extra radio.
2. **BLE — Bluetooth Low Energy.** Radios take two roles: *advertising*
   (broadcasting a small beacon, ≤31 bytes legacy / ~250 extended, a few
   times per second at negligible battery cost) and *scanning* (listening
   for beacons). Hearing a beacon at all bounds distance to roughly a
   room–building (~10–100m); the received signal strength (RSSI) gives a
   coarse near/far estimate — walls and bodies make it noisy, so treat it
   as buckets (immediate/near/far), never meters. Platform reality:
   phones have BLE and OSes gate it behind explicit permissions (Android:
   BLUETOOTH_ADVERTISE/SCAN; iOS: CoreBluetooth, with background
   advertising heavily restricted — reliable beacons mean the app is
   foregrounded or using OS-specific rendezvous services); most desktops
   can scan but advertise poorly. So BLE proximity is mobile-first by
   nature, and that's fine.
3. **UWB — ultra-wideband.** Time-of-flight ranging (Apple U1/U2,
   Android UWB API on some devices): actual distance in centimeters. The
   precise-but-rare option; design must treat it as a bonus, never a
   requirement.

- [ ] Same-LAN sighting bumps proximity: a verified discovery announce
  from a known contact raises the score with time-decay (nearness fades if
  never seen again); the bump uses the *verified* fingerprint, never the
  announce alone.
- [ ] BLE beacon (mobile): advertise the same transport-agnostic payload
  as the LAN announce (`organ_uid`, fingerprint — it already fits a beacon
  by design, §11); scanning devices match fingerprints against known
  contacts and bump proximity by RSSI bucket. Unknown fingerprints are
  ignored — BLE never drives introduction, because there is no channel for
  the challenge handshake until a network exists.
- [ ] Decay + buckets, not meters: proximity is a slowly-decaying score
  fed by discrete events (LAN sighting, BLE bucket, UWB range when
  present); the UI never shows raw RSSI or claims precision the radio
  cannot give.
- [ ] Privacy stance: beacons broadcast only what the LAN announce already
  broadcasts (uid + public fingerprint, no names, no secrets); scanning
  stores sightings of KNOWN contacts only, locally, decayed — Lince never
  builds a log of strangers' devices.

