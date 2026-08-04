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
- [x] `nearby` — Organs currently visible on the local network (2026-08-04).
  NOT a departure from "the read contract": Protein already serves derived,
  non-table sources (`frequency`, `recurrence`) and already has a LOCAL-ONLY
  source in `decision`, which returns empty to any remote subject. `nearby`
  is that same category — a Cell's own runtime view, never exported.
  It exists because a sand reads through Protein and nothing else, and the
  alternative was worse in two ways: an Action polled every few seconds is a
  round trip through the whole write-shaped path to answer a read, and
  mirroring discovery into a synced extension would tell every contact who is
  on your local network.
  **Liveness is the part to get right.** Protein subscriptions recompute off
  the FACT BUS, and discovery deliberately emits no Facts — it must never
  touch the Ledger. A naive subscription would therefore never update, which
  is worse than polling because it *looks* live. So the session re-runs
  ephemeral subscriptions on a short tick and pushes a Snapshot only when the
  result CHANGED. Quiet when nothing moves, and a sand cannot tell the
  difference from a reactive source.
  Deliberately NOT clustered with presence. Presence is per-keystroke and
  room-scoped, and ephemeral lanes (blueprint VII.3) already carry it; routing
  it through Protein would replace the right tool with a worse one. And
  per-contact connection state needs no new source at all — contacts ARE
  Records, so it rides the existing `contact` include. One new source, and
  the other two cases reuse what exists.
  As built: `Context` carries process state into `execute_for_with_context`,
  the way `installed_signer_actor` already carried it; `is_ephemeral` is the
  complement of `affects`, and the ws driver arms its tick only for sessions
  that hold such a subscription. Rows sort by NodeId — the backing list is a
  map, and an arbitrary order would both reorder the rendered list and defeat
  the change comparison the whole design rests on.
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
(Scope revised by the iroh box below: the code is required on FIRST contact
through discovery and skippable over an already-verified channel. What it
defends changed — not key substitution on the wire, which iroh retires, but
a relay attacker announcing their own NodeId under a friend's display name.)
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

### The Organ/Sync refactor

The Part 1 / Part 2 framing is dropped (2026-08-03) — it was scaffolding
for a conversation, not a real boundary, and everything below is one
refactor of how Organs identify each other and sync. The list is ordered
by dependency, not by release. The goal it converges on: **two Lince
instances find each other on a network, talk, exchange keys, become
contacts, and sync exactly what each chooses to share — with an
internet-reachable Cell whose app_users edit Records live.**

Earlier stages (do these first):
1. iroh endpoint with a per-Cell node key; ALPN `lince/sync/1` carrying
   today's inbox/ops JSON bodies on QUIC streams; `remote_node_id()`
   replaces per-request signature verification. Op-batch payload signing
   stays, now domain-prefixed.
   All three items carried out of stage 1 LANDED with stage 2:
   `MAX_FRAMES_PER_CONNECTION` bounds the frame count as well as the size,
   `tracing` is a real dependency of the engine crate, and a batch/peer
   mismatch answers `WireResponse::Refused { code }` — a security event,
   distinguishable from an ordinary import failure.
2. [x] DONE 2026-08-03. `lan_discovery.rs` DELETED — multicast announce,
   `Announce`, `NearbyPeers`, expiry bookkeeping and all. Replaced by iroh
   mDNS through `iroh-mdns-address-lookup` (pinned `=0.4.0`; mDNS left iroh
   core at 1.0), subscribed on its event stream into `wire::Nearby`.
   Two decisions worth keeping:
   - **A Lince-specific mDNS service name** (`lince`, not iroh's default
     `irohv1`), or the nearby list shows every unrelated iroh application on
     the network as an Organ.
   - **`organ_uid` is no longer broadcast.** The old announce shouted the
     Organ uid across the LAN so a receiver could tell whether it was a
     known contact; the NodeId now answers that through
     `organ_contact.node_id`, and the uid arrives at pairing over an
     authenticated connection. Strictly less is published and nothing is
     lost. The display name still rides iroh `UserData` as the untrusted
     label it always was.
   `GET /organ/nearby` re-backed off `wire::Nearby`, returning a short NodeId
   fingerprint per row. `POST /organ/pair` now takes a `node_id` and dials
   over `lince/thread/1`; the reached NodeId is bound to the adopted contact.
   The LAN-sighting candidate url is gone from `sync_runner` — iroh resolves
   a NodeId to a transport path, not an HTTP base url.
2b. [x] DONE 2026-08-04: **`nearby` is a Protein source** (see §10). The
   Action-plus-client-poll shipped in stage 2 was working around the sand
   boundary instead of extending it, and it made discovery update on a fixed
   client timer whether or not anything had changed. A local-only Protein
   source with a change-gated server tick is both the smaller mechanism and
   the more efficient one — no traffic when the network is quiet.
   What landed: `Source::Nearby`, gated exactly like `Decision` (a remote
   subject gets an empty list, because who is physically near you is the last
   thing that should travel). Process state reaches Protein through
   `Context` — a struct passed to `execute_for_with_context`, following the
   `installed_signer_actor` precedent of handing in what no query could find,
   rather than hanging network state off `Store`. `NearbyPeer` moved to the
   nucleus so both sides can name it without the engine/Protein dependency
   inverting. `is_ephemeral` is the complement of `affects`: the driver arms
   a 3s tick ONLY for sessions holding such a subscription, and the session
   pushes an Update only when the rows differ from what it last sent.
   `Action::NearbyOrgans` and the client poll are deleted.
3. [x] DONE 2026-08-03, folded into stage 2 because pairing cannot work
   without it: an unknown Organ must be able to fetch an Introduction or a
   nearby list is decorative. ALPN accept policy, default closed
   (`lince.discovery.accept_unknown = false`): `known` → `lince/sync/1`;
   unknown → `lince/thread/1` and, today, `Introduction` and nothing else;
   `blocked` → closed on BOTH ALPNs, checked before the ALPN split.
   Note that `known` means `trust='known'`, not merely "a contact row
   exists" — a row with `trust='unknown'` is someone added but not vetted,
   and it gets the thread door like any stranger.
   **`add_contact` now defaults to `unknown`** (fixed 2026-08-04; it wrote
   `known`). The gate above was tightened precisely because having a row is
   not consent, and a default of `known` quietly undid that from the other
   side: every path that recorded an address opened the sync door. Callers
   that HAVE made the decision — pairing, adopting a code, reconciling an
   introduction — say so with `set_trust`. The change broke three test
   helpers that leaned on the default, which is the fix working: a helper
   called `know` now does the knowing out loud.
1b. Roster-of-one, BEFORE any key is ever published or QR'd. The signed
   roster format lands with exactly one member Cell, so the string people
   save is final from the first exchange. Multi-device UX (enrolment,
   device panel, race-dial) arrives later without stranding anyone. The
   insurance is needed before publishing, not before the first connection.
4. Threads as individual replica: the synced unit is one Record per
   relationship holding MANY threads, as Records joined by Assertions, so
   a new topic needs no new grant.
   Brings with it:
   - **individual replica**, the new per-record-per-contact sync axis
     (grant, accept, revoke, feed-serve filtering, and an import gate that
     drops ops for revoked grants). This is the single largest item here —
     larger than the iroh refactor — and the import gate is load-bearing
     security, not bookkeeping: a bug there lets a contact write to
     Records that were never shared. Scoped by the `replica_root` column
     (see the Threads box): traverse once at grant time, never per op.
   - a conversation view in the Record sand, over message Records.
   - invites into notifications.
   - at-rest encryption of `record.head`/`body` and `sync_op.value`.
   Messages are plain Records synced as `set` ops — no Loro, no sealed
   bodies, no append-only special case, and `collab.rs` is untouched by
   this item. Either party may edit or delete anything shared; that is
   accepted rather than constrained.
5. First-contact key exchange, ranked strongest first. Only the ACQUISITION
   of a NodeId is ever at risk; once held, connections to it cannot be
   intercepted. So offer, in this order:
   a. **QR code in person** — the local Organ renders its NodeId as a QR,
      the other scans it. The visual channel cannot be relayed and you can
      see who you are handing it to. Best available, and the reason to
      build it first.
   b. **Paste it into a messaging app you already trust** (Signal, etc.).
      Equally strong: that channel is already authenticated to that human.
   c. Discovery + conversation alone — weakest; a live relay passes it.
      Acceptable only because (a) and (b) cover the real flows.
   **The verification code leaves the plan entirely** — decided 2026-08-02.
   It only ever defended REMOTE first contact with no other trusted
   channel. Every flow this product actually has is in-person (show a QR)
   or already-trusted (paste into an existing chat), and in both the code
   is redundant ceremony. `engine::peers::verification_code` stays in the
   tree as an optional "verify this contact" panel for anyone pairing
   remotely; no normal flow shows it.
   What the original 5 chars were reaching for — "which of these forty
   peers in a stadium is my friend" — is DISAMBIGUATION, not security, and
   the honest fix is different: show a short fingerprint of each peer's
   NodeId beside its untrusted display name in the nearby list. Derived
   from the real key, so it cannot be spoofed by a name, and it answers
   "which row is them" without pretending to be a security check.
   In-thread: a share-my-key button, and adding the peer as a known Organ
   with a name the local user types. Manual paste in the Organ sand kept.
   QR also solves the blocked-mDNS case (guest wifi, hotels, corporate):
   embed NodeId AND current addresses in the QR and no discovery mechanism
   is needed at all for an in-person exchange.
5e. [x] **Reopened 2026-08-04 from reviewing what stage 5 actually shipped;
   CLOSED the same day.** All three items landed — the reconciliation bug,
   the honesty wording, and the camera capability with backend QR decode.
   - **Pasted-contact uid reconciliation — a BUG, not a refinement.**
     `add-known-organ` never talks to anyone, so it cannot learn the other
     side's real Organ uid and invents `o-<node_id>`. But the wire
     authenticates an inbound peer as `contact.record_uid` — that invented
     uid — while their op batches carry their ACTUAL uid. The two never
     match, so `batch_peer_mismatch` refuses every push: a contact added by
     paste can be reached and can never sync. Silent, and only two people
     trying it would notice.
     FIXED 2026-08-04. `organ_contact.pending_introduction` (migration 0045)
     marks a row added from a code — an explicit column, because sniffing the
     derived shape would silently replace a real uid that happened to look
     like one. `Wire::reconcile_pending` runs at the top of every sync pass,
     BEFORE `push_outbox`: it dials each pending contact on `lince/thread/1`
     (not the sync ALPN — the pending peer holds no row for us, so their sync
     door is shut to this Cell by design), takes their Introduction, retires
     the placeholder, and adopts them under the uid they declare. Deleting
     the placeholder is a purely local operation: `add_contact` writes its
     Record with plain SQL rather than through the Record write path, so
     nothing was ever logged to the op log or sent to any peer.
     The security property is the root key. It was trust-on-first-use'd from
     the code, and that is the only thing adding by code verifies — so if the
     Organ answering at that address presents a DIFFERENT root key, this is
     not the peer the code was for: reconciliation refuses, the row stays
     pending, and a human looks at it. Silently adopting the new key would
     discard the one thing that had been established.
     Offline adding still works: the row waits, and the surface says
     "not connected yet" rather than showing it as an ordinary contact.
   - **Say what dialing actually proves.** Two paths end at `trust='known'`:
     paste/scan a code (no network, trust-on-first-use on the root key), and
     dial plus fetch an Introduction. The second does NOT prove more about
     WHO someone is — if you were handed the wrong NodeId, the handshake
     authenticates the wrong person flawlessly. What dialing adds is their
     true identity fields and proof of reachability. Both rest on where the
     code came from, and the UI must say so rather than implying that having
     connected constitutes verification.
   - **Sand `media_capture` capability**, and QR DECODE in the backend
     (`rqrr`), matching the render side. Decoding belongs there for the same
     reasons rendering does — no QR library under the sand CSP — plus one
     more: a decoder emits a PAIRING CODE, so it is security-adjacent enough
     to want in one audited place instead of in every sand that scans.
     Two data paths under one permission, and they are not alike: QR needs a
     SINGLE FRAME sent to the backend; audio/video calls in threads need a
     LIVE STREAM peer-to-peer, which never routes through the backend at all.
     Build the capability and the frame path now; the stream path belongs to
     the Communication sand (F2), not here.
     DONE 2026-08-04. `engine::pairing::decode_qr` (rqrr + `image`, restricted
     to the PNG/JPEG a browser canvas actually emits) sits beside `qr_svg`;
     `POST /organ/qr-decode` takes one frame and returns the decoded text,
     with "no code in this frame" as a 200 carrying `null` — a scan loop must
     not read failures to know it is still looking.
     The capability's shape is the part that matters: the CHROME owns the
     camera, not the sand. A sand calls `H.scanCode()` and receives a STRING;
     the host opens the stream, shows the preview, posts frames to the
     decoder, and stops the tracks on every exit path. So `media_capture`
     grants "read a code the user pointed at", never "watch the room" — and
     that is enforced in `widget-bridge.js`, gated on the card's declared
     permission plus a check that the message came from that card's real
     iframe, exactly as `terminal_session` is.
     Scanning FILLS THE FIELD and stops. A scan is a strong story about where
     a code came from, but it is still a story, so the human presses Add —
     consistent with the honesty point above.
     Known gap, not fixed here: `getUserMedia` needs a secure context, so
     scanning does not work over plain HTTP to a LAN hostname — which is how
     a second device reaches this Cell. The chrome says so specifically
     instead of failing as "no camera". Stage 7's hostname/TLS item is what
     actually closes it.
5b. **At-rest encryption: DROPPED 2026-08-03.** It was not free, and the
   honest accounting is that it bought little for a lot. Encrypting at the
   field boundary above `log_set` covers both plaintext copies cheaply, but
   every READER then has to decrypt — query/projection, Protein, search,
   export, File Sync — and a missed one fails quietly as garbled text
   rather than loudly as an error. What it defends is narrow: a database
   copied out through a backup, a synced folder, or a disk pulled from a
   machine. It does NOT defend against anyone executing as your user, who
   reads the key file sitting beside the database. Full-disk encryption
   covers the same threat better for no code at all.
   State the loss plainly rather than pretending: thread bodies sit in
   PLAINTEXT SQLite on both ends. The original ask — "can we e2e encrypt it
   independent of lan?" — was about the wire, and that IS delivered: iroh's
   QUIC/TLS 1.3, authenticated and forward-secret. At-rest was an addition
   on top, not the request.
   `replica_root IS NOT NULL` remains the exact scope if it is ever wanted,
   and the reason it would be cheap to add later is that the column already
   marks precisely the Records that would need it.
   Still wanted from this item: a send queue that flushes on next connect.
5c. Multi-Cell Organs — PULLED EARLIER because the key
   format everyone saves must be right the first time. One published
   identity key, several Cells, a signed roster. See the box below for the
   `(actor_organ, hlc)` split this requires. Dial policy: RACE all known
   Cells and take the first that answers, with a preference order only as
   a tiebreak (prefer a LAN-local Cell for latency, the always-on one for
   bulk). No leader election — leaders exist for consensus, and an op log
   with CRDTs converges without one.
   The gap this must close, or "one key is all they save" stays false:
   a contact learns roster v2 only by reaching a Cell listed in roster v1,
   so adding a laptop while the old Cells are off or lost strands the new
   one forever. Fix: publish the SIGNED ROSTER under the Organ identity key
   via pkarr — it stores signed records addressed by an ed25519 public key,
   which is exactly what the identity key is. Then identity key → current
   Cells resolves with no prior roster and the saved key is genuinely
   self-sufficient.
   **What pkarr is, in one line:** a phone book whose lookup key is your
   public key. You publish a small signed blob into the BitTorrent DHT;
   anyone knowing the public key fetches it and verifies the signature.
   Nothing to do with Lince Records — the "record" in "resource record" is
   a DNS-style entry. The size limit is the DHT's per-entry byte cap
   (mainline BEP44: 1000 bytes) and a roster fits with room to spare: a
   NodeId is 32 bytes, so five Cells plus a version counter, expiry and
   signature lands near 250 bytes. DHT entries expire in hours, so
   something must republish on a timer — a natural job for the always-on
   Cell, and a reason a laptop-only Organ should republish at every boot.
   **Republishing needs no private key.** It re-broadcasts an
   already-signed blob, so the VPS Cell can do it while holding no
   identity-signing material — do not let "the VPS republishes the roster"
   become "the VPS signs the roster" and quietly undo the key split.
   Naming, because this is where the bugs will live: each Cell's database
   holds TWO `kind=organ` Records.
   - The **Cell Record** — this running instance, this laptop. What
     `organs::local()` returns today (fixed slug). Its uid is stamped as
     `sync_op.actor_organ` on every op written here, which is what keeps
     ops from the laptop and the phone distinguishable and the
     `(actor_organ, hlc)` uniqueness intact.
   - The **Organ Record** — the person across all their devices. Holds the
     published identity key and the Cell roster. What `record.organ_uid`
     points at, so a Record reads as coming from *you*, not from *your
     laptop*.
   Today these are one row doing both jobs, so every existing call site
   says `organs::local()` meaning "who am I" and "who authored this"
   interchangeably. After the split each one has to be re-read and
   assigned, and both ways of getting it wrong are bad: a site that means
   identity but keeps using the Cell uid makes one person look like three
   different Organs to everyone else; a site that means authorship but uses
   the Organ uid brings back the HLC collision that silently swallows
   remote ops. Audit every caller, do not pattern-match.
   The audit has one funnel: `store::organs::local()` (`organs.rs:76`,
   fixed slug) is what every caller goes through. Two already-known
   examples of each meaning — `collab.rs` passes `&local.uid` as
   `actor_organ`, which means the CELL; `records.rs:121-131` stamps the
   origin through `set_organ_origin`, which means the ORGAN.
   Note also that `record.organ_uid` is `Option<String>` and the origin is
   stamped by a separate call after insert, so the split lands on a field
   that can already be NULL — existing rows with no origin need a defined
   meaning before the audit lands, not after.

#### What the split gives the product: profile vs device

The distinction earns its cost only if each side owns real things. It does:

- [ ] **Organ Record = the public profile.** Display name, description,
  avatar, published identity key, the Cell roster, discovery preference
  (visible / relay-only / dark). This is what a contact saves, what a QR
  encodes, what goes on a website. It survives every device change.
- [ ] **Cell Record = this device.** Device label ("laptop", "phone",
  "vps"), its node key, whether it is always-on, and every local-only
  setting that has no business travelling: File Sync paths, storage
  config, local cache sizes. Never published except as a roster entry.
- [ ] Organ sand grows two panels accordingly: **Profile** (identity, the
  shareable key + QR, display name) and **Devices** (roster list with
  labels, last-seen, add, revoke).
- [ ] Enrolling a new device is pairing with YOURSELF, and deserves its
  own flow rather than reusing contact pairing: an existing Cell shows a
  QR carrying its NodeId plus a single-use, short-lived enrolment token;
  the new Cell scans, connects, proves the token, and is signed into the
  roster. Single-use and short-lived because this QR grants membership in
  your identity, which is strictly more than a contact QR grants.
- [ ] Who may sign roster changes — the question enrolment forces. Cells
  carry a flag for whether they hold identity-signing material; the Organ
  key lives only on flagged Cells, and unflagged ones sync like any member
  but cannot enrol or revoke.
  Which Cells get the flag is a genuine question, and "the VPS is the
  least trusted machine" is too glib — a phone is stolen on the street far
  more often than a datacentre is breached. The honest comparison is that
  they fail in different ways:
  - A personal device is likely to be lost or stolen, but that failure is
    LOUD. You know the moment it happens, and revoking it from another
    device is exactly what the roster is for. Full-disk encryption turns
    the theft into a non-event — the thief holds a brick.
  - A VPS cannot be pickpocketed and can be hardened far below a laptop's
    attack surface (no browser, no GUI, keys-only SSH). But the hosting
    provider holds permanent hypervisor-level access — disk snapshots,
    memory, and compliance with legal process — which you cannot remove,
    cannot detect, and did not consent to per-incident. That failure is
    SILENT.
  Detectability is what should decide it, not raw probability. A stolen
  laptop you revoke within the hour. A quietly compromised VPS holding the
  identity key lets an attacker sign a NEW roster adding their own device,
  and lock the real owner out of their own identity permanently, with no
  moment at which anything looked wrong.
  This framing is superseded by the root/operational split below, which
  dissolves most of the question: once no running Cell holds the root, the
  VPS-versus-laptop comparison stops being about the identity at all.

#### Key compromise: the root/operational split, and honest recovery

**BUILT 2026-08-03** (migrations 0043/0044, `store::roster`, `engine::roster`,
Organ sand Profile + Devices panels). Beyond the boxes below, three things the
implementation settled:
- **The root key is created at most ONCE, ever.** Recreating it whenever the
  file is missing would mint a brand-new identity the first time the owner
  does the thing this whole split encourages — moving the root to offline
  media — and every contact would then see a key chaining from nothing. No
  file AND no roster means first boot; no file WITH a roster means the root is
  deliberately elsewhere, and the Cell simply cannot enrol or revoke until it
  returns. Everything else keeps working, which is the point.
- **Detach verifies the copy byte-for-byte before deleting.** "Detach" without
  that check is "irrecoverably destroy your identity because you thought you
  had a backup", and there is no authority anywhere to appeal to. Export
  likewise refuses to overwrite: a file already at the destination might be
  another identity's root.
- **Republishing preserves the other members.** Dropping a name from the
  roster IS revocation, so it must never be a side effect of a reboot.
Revocations are pulled BEFORE rosters on each sync pass, so a key cannot be
accepted in the same pass that learns it is dead.

The problem, stated without flinching: if an attacker obtains the Organ
identity private key, they can sign a roster adding their own device, sign
ops as the owner, and BE that Organ to everyone holding the public key.
There is no central authority to report it to. Worse, the attacker can do
exactly what the victim can — both can announce "I was compromised, here
is my new key" — so contacts face a claim and a counter-claim with no
referee. Any mechanism that lets the owner recover is a mechanism the
attacker can also attempt to walk. That is the whole difficulty, and no
design removes it; designs only change who has to be fooled.

Three principles, in order of leverage.

**1. Make the catastrophic case rare — root offline, operational keys
online.** This is the standard shape (TLS roots and intermediates, SSH
CAs, PGP primary keys with subkeys, Signal identity keys with prekeys) and
it is the single highest-value change here:
- [ ] The Organ **root key** signs two things only — the Cell roster and
  key successions — and lives OFFLINE: a hardware token, or a printed or
  drawer-kept drive. It is on no running Cell, not the laptop and not the
  VPS. Using it is a deliberate, occasional act.
- [ ] Each Cell holds an **operational key**, certified by the root, used
  for everything routine. Compromise of a device is then compromise of one
  revocable credential, not of the identity.
- [ ] **The signed roster IS the certificate** — simplified 2026-08-03.
  There is no separate certificate object to define, sign, store, ship and
  validate. A roster entry already names a Cell, its operational key and
  its NodeId, and the roster already carries a monotonic version and an
  expiry; being listed in the current root-signed roster IS what certifies
  an operational key, and being dropped from the next one is what revokes
  it. One signed blob does membership, certification, versioning and
  expiry together.
- [ ] This retires the earlier "identity-signing Cells" flag. Enrolling or
  revoking a device requires the root — which is correct: those are rare
  acts and SHOULD feel deliberate.
- [ ] It also largely answers the provider-access worry. A VPS snapshot
  yields an operational key the owner can revoke, not the identity. The
  thing that could not be defended is simply no longer there to steal.

**2. Make substitution VISIBLE — detection beats recovery.** A compromise
noticed in a day is survivable; one noticed in a year is not.
- [ ] Contacts store the full **key-succession chain** for each Organ, not
  just the current key. A succession is accepted only if it chains from a
  key already held. Anything else is a loud, blocking warning that
  requires a human decision — NEVER a silent update. This is the cheap
  approximation of key transparency (CONIKS, Certificate Transparency),
  and it converts a silent takeover into a visible alarm.
- [ ] **Pre-signed revocation certificate**, generated at key creation and
  stored offline beside the root. It does not prove a new key is genuine,
  but it kills the old one immediately — damage limitation that works even
  when identity cannot yet be re-established. PGP has done this for
  decades and it costs nothing.
- [ ] ~~**Time-locked succession**~~ — RECOMMENDED CUT (2026-08-03),
  pending confirmation. A new root taking effect only after a veto window
  requires peers to agree about time, requires the victim to be online and
  watching during the window, and adds a second state machine to the one
  part of the system that must never be subtly wrong. What it defends is
  the case where the key was COPIED and the owner still holds it — which
  is precisely the case the pre-signed revocation certificate already
  handles, immediately and with no clock assumptions. Near-zero security
  loss for a real drop in complexity.

**3. Recovery stays deliberately simple.** M-of-N social recovery was
considered and REJECTED (2026-08-03): every recovery path is also an
attack path, and a quorum scheme adds a second door that must be defended,
audited, and kept from becoming cheaper to walk than stealing the key. It
buys convenience in a rare event at the cost of permanent attack surface.
Not worth it here.

What remains is enough, because layers 1 and 2 have already made the
catastrophic case rare and visible:
- [ ] Publish the pre-signed revocation certificate. The old key is dead
  immediately, whatever happens next.
- [ ] Re-establish through the channels that worked the first time — QR in
  person, or a chat app already authenticated to that human. Tedious,
  completely safe, and no new mechanism to attack.
- [ ] Existing threads help more than they appear to: key theft is not the
  same as data theft. An announcement arriving inside a long shared thread
  from someone who knows its history is strong evidence when the attacker
  took a key but not a database.
The honest summary: with the root offline, losing it requires physical
access to a drawer or a token. That is a threat model a person can
actually reason about, which is worth more than a clever protocol.
- [ ] Revocation is a roster version bump plus republish (monotonic
  counter, so an old roster cannot be replayed to re-add a stolen device).
  **Who can do it — contradiction resolved 2026-08-03.** An earlier line
  said "from any identity-holding Cell", written before the root/
  operational split retired identity-holding Cells entirely. Under
  root-offline there are none, so signing a new roster ALWAYS requires the
  root. That is the correct cost and it is the point of the split, but it
  must be stated plainly rather than discovered during a theft: revoking a
  stolen device means going to the drawer.
  Two things keep this from being painful. Roster entry EXPIRY means a
  stolen Cell loses authority on its own even if the owner never reaches
  the root — self-limiting credentials doing the work that urgency would
  otherwise have to. And the pre-signed revocation certificate is
  generated at key creation and stored offline WITH the root, so the
  drawer trip yields both acts at once.
- [ ] **Two tiers of publishing** — the resolution of "I want an
  add-me-in-Lince key without exposing my devices."
  The identity key itself is safe to publish anywhere: on its own it is
  an identifier and reveals nothing. The exposure is not in the key, it is
  in what the key RESOLVES TO. So split it:
  - **Public tier (the DHT record, readable by anyone with the key):**
    the front-door Cell only — the always-on VPS. One address. A stranger
    who finds the key on a website learns that one machine exists and
    nothing else.
  - **Contact tier (shared over an already-authenticated connection):**
    the full roster, so contacts can reach personal devices directly for
    speed instead of always paying the front-door hop.
  Personal devices then never appear in any PUBLIC record, and incoming
  strangers land on the front door. Contacts holding the roster do dial
  them directly — that is what the contact tier is for, and it is what the
  race-all-Cells dial policy above races over. "Never found" scopes to
  non-contacts only; personal Cells are not outbound-only.
- [ ] Front-door mechanics, currently undefined and needed for
  "add me in Lince" to actually work: a stranger's invite arrives at the
  VPS, whose owner may be on a phone that is not in the public record and
  may be offline. The front door QUEUES the invite until a personal Cell
  syncs, reusing the offline send queue rather than forwarding live. The
  VPS holds no identity-signing material, so it cannot accept on the
  owner's behalf — it can only hold the request until a Cell that can
  decide sees it.
  What the untiered version would have leaked to anyone holding the
  published key: how many devices the Organ has, each one's current IP,
  and which are online right now — which is to say whether the owner is
  home, travelling, or asleep. That is a daily-pattern leak to the entire
  internet, and it is the reason the tiers exist.
  A contact you already sync with necessarily learns which Cell it is
  talking to; that is unavoidable and harmless. The tiering is about
  non-contacts.
  Cost to accept: if the front door is down, a stranger cannot reach the
  Organ at all. Existing contacts, holding the full roster, still can.
5d. **Compatibility and revocation floor** — the guarantee that shipping
   more Lince never strands old keys or leaves a lost device authorized
   forever. This must land EARLY: every one of these is cheap now and
   brutal to retrofit once keys are in other people's hands.
   - Version the ALPN strings (`lince/sync/1`, `lince/thread/1`). A
     protocol change bumps to `/2` and both are served during transition,
     so an old peer gets old behaviour rather than a broken half-upgrade.
   - Key succession: the OLD identity key signs a statement endorsing the
     NEW one, so an Organ can rotate without every contact re-pairing.
     Build the signed succession record now even if the UI lands later.
   - Roster versioning with a monotonic counter: a contact accepts only a
     roster NEWER than the one it holds. This is what makes removing a
     stolen device stick — an old roster cannot be replayed to re-add it.
   - Roster entry expiry (not-after): a Cell that stops syncing fresh
     rosters loses authority on its own. Self-limiting credentials beat
     remembering to revoke.
   - Pin iroh to an exact version (`=x.y.z`), as Loro already is. Its API
     has broken across releases; an unpinned bump is a silent protocol
     change between two Cells on different builds.
   - Fail closed on the unknown: an unrecognised op `kind`, grant version,
     or frame type quarantines rather than crashing or silently applying.
     A newer Organ syncing to an older one must degrade, never widen.
6. `live` mode — the only name for it; "live login" is retired as a phrase
   because it is the same thing. An app_user logs into a hosted Cell,
   Record sand streams that Organ's data into memory, full CRDT text
   editing, nothing persisted locally. This item is where the collab code
   written blind in the previous phase finally gets RUN and made to work —
   it has only ever been type-checked. Includes:
   - [x] the read-permission gate on `collab_join` — done: `may_read_record`
     gates both `CollabJoin` and `CollabUpdate`, refusing with
     `collab_not_visible`;
   - `read`+`write` permission is exactly what enables CRDT editing — collab
     is not a separate privilege, it is what having those permissions means;
   - [x] presence: cursor position plus who it is, where identity is shown
     only to a viewer allowed to know. Without it the sand still renders the
     cursors, unnamed. Presence is ephemeral — lanes, never the op log.
     Done: `LaneEvent.from_subject` carries the sender, `spawn_lane_forwarder`
     resolves it to a name only when the viewer may read that Person, and the
     `record_editor` sand renders "someone" otherwise. The sand never decides
     whose name it may show.
   - [x] **The client collab layer, RUN at last** (2026-08-04). The
     `record_editor` sand joins a Record's shared Loro document, edits
     `head`/`body` as ordinary text, and sends a DELTA since its last send —
     not a snapshot per keystroke. Both converge; only one stays cheap as the
     document grows, and the difference is invisible until it is expensive.
     The vendored bundle is now EXECUTED in tests (`crates/web/tests/
     collab_wasm.rs`, real node): it initializes, exposes the `head`/`body`
     containers the engine materializes from, converges through deltas, and —
     the one worth pinning — re-importing the server's echo of this client's
     own work is a no-op rather than duplicated text.
     NOT proven by those tests, and stated plainly rather than implied: that a
     sand IFRAME may load the wasm. That depends on the frame's CSP and
     sandbox flags at runtime and needs a browser. For the record, the live
     board serves NO CSP header today (only the archive export sets one) and
     sand frames are same-origin `srcdoc` with `allow-scripts
     allow-same-origin`, so the same-origin ESM and wasm should load.
   **REVISED 2026-08-04, and the revision removes a dependency rather than
   adding one.** The line above said live mode "needs a hostname, certs and a
   reverse proxy" because "browsers speak HTTPS, not QUIC-to-a-NodeId". The
   first half does not follow from the second: the browser never has to be
   the thing that crosses the network.
   **Live sessions ride iroh, on `lince/live/1`.** A guest's browser opens an
   ordinary websocket to its OWN Cell on localhost — no certificate, no
   hostname, nothing to configure — and that Cell relays the frames to the
   host Cell over iroh (`/live/{organ}/connect`, `transport::live`). The only
   leg crossing a network is authenticated by KEY rather than address, so
   there is no hostname to go stale and no certificate bound to one, and QUIC
   migrates the path under a connection that stays open. Change network
   mid-sentence and the session continues — which was the actual requirement,
   and which TLS to a hostname would NOT have satisfied.
   `Session` needed no changes: it was already transport-agnostic, so the
   QUIC driver is a second driver of the same shape as `ws.rs`. Note that
   `MAX_FRAMES_PER_CONNECTION` deliberately does NOT apply — a live
   connection is handed off whole, because a 4096-frame cap would hang up on
   someone a few thousand keystrokes into a sentence.
   **A login is a BINDING, not a credential** (`organ_login`, migration
   0048). No password: the handshake already proved which Organ is on the
   connection, with a key rather than a secret someone could retype — adding
   a password would be a second, weaker way in. What the login decides is
   which PERSON that Organ acts as, and every read they make is then gated by
   that Person's visibility. So granting one grants a named identity, not a
   door: a test asserts a fresh login sees nothing until something is shared
   with that Person. Requires `trust='known'` — reaching the thread door is
   not the same as being allowed inside. Revoking is one row deleted: local,
   immediate, not a request the other side may decline (§12).
   Still true, and still not blocked by any of this: an ordinary HTTPS login
   would need a hostname and certs. That path is simply no longer the only
   one, and is not what the workflow depends on.

Ordering note: item 6 is independent of items 1–5. If the iroh refactor
turns out to break things, item 6 lands first or last — it must never be
held hostage by transport work.

Later stages (depend on the above):

These assume the decisions above (iroh transport, NodeId addressing, no
dual address kind, live = in-memory session, multi-Cell already landed).

**They open with the VPS**, because everything after improves once the
user's own always-on infrastructure exists:
- Run `iroh-relay` on the VPS. It needs a public IP, a DNS name, and TLS
  (the relay speaks HTTPS/WebSocket to nodes); point the Cells at it as
  their configured relay instead of the default public ones. From then on
  the Organ's own machine carries its own connection metadata.
- Self-host address publishing too (a pkarr/DNS publisher), so reachability
  does not depend on n0's infrastructure either.
- Run a Cell on the VPS as a member of the Organ roster: the always-on
  device that makes offline delivery work without either laptop being up.
- Only then does relay-only mode (below) cost nothing that matters — the
  relay being depended on is the user's own.

- Replica bootstrap and initial snapshot, over iroh streams.
- `record_editor` sand, `Note` rename, kanban/relation/table embeds — all
  consumers of the collab binding, unaffected by transport.
- `_ack` frames and richer presence (selection ranges, idle states) on top
  of the cursor presence in stage 6.
- Audit, retention, pruning: checkpoint-gated op pruning. Unchanged by
  iroh — it is op-log work.
- Discovery UI polish and proximity signals, now fed by iroh discovery
  rather than the retired multicast announce.
- Self-hosted `iroh-relay` on the user's VPS, so the Organ's own
  infrastructure carries its own connection metadata.
- Store-and-forward threads through a THIRD Organ, if ever wanted — the
  only scenario that reintroduces message-layer sealing, and then only
  with an audited ratchet.
- Live mode via iroh for hostname-less Cells: your Cell fronts a remote
  Organ that has no public door.

### Transport: iroh (supersedes the HTTP peer plumbing above)

Decided 2026-08-02. The Peers prose above describes code that WORKS and
stays running until the iroh path replaces it piece by piece — it is
superseded, not wrong. What changes is only how two Organs find and reach
each other; the op log, checkpoints, Loro merge, trust, visibility, and
`sync_out`/`sync_in` are untouched by any of this.

The reason is not elegance, it is reach: `lan_discovery.rs` is UDP
multicast, so it cannot find a peer off the local segment at any amount of
polish, and nothing in the current design traverses a NAT. iroh is QUIC
with an ed25519 keypair as the node identity, address resolution by
DNS/pkarr/mDNS, hole punching, and relay fallback when hole punching
fails. Verified against docs.rs before deciding: `SecretKey::from_bytes(&[u8;
32])` seeds an endpoint from an EXISTING ed25519 secret, and `NodeId` is
`PublicKey`, a 32-byte compressed Edwards point that derefs to `[u8; 32]`.
So the Organ keypair COULD have been the node identity byte for byte.

Decided 2026-08-02 not to do that — **node key ≠ identity key, from day
one, even on a single-Cell Organ.** They answer different questions:

- The **node key** authenticates a LIVE CONNECTION. Its public half is the
  NodeId, which is the address; it is used in the QUIC/TLS handshake and
  answers "is the endpoint I just dialed really that endpoint." Per Cell,
  generated at first boot, never leaves the device, cheap to rotate.
- The **identity key** authenticates DURABLE BYTES. It signs op batches,
  facts, and the Cell roster, and answers "who wrote this" a year later
  from a backup with no connection in sight. Per Organ, published, and the
  thing `identity_key` + `trust.rs` already hold.

The security argument, which is why this is the choice even though fusing
them is simpler: the node key is on the network constantly and lives on
every device including the least trusted one. If it is also the identity
key, then compromising any running Cell means forging that Organ's history
forever. Split, a stolen node key costs one connection identity and the
attacker still cannot sign a single op. Separating later is far more
expensive than separating now — every published key everyone already holds
would become wrong.

What gets shared is still ONE string: the NodeId. On first connect the peer
sends its Organ identity key and a signature binding that key to this
NodeId, so both halves arrive over an already-authenticated connection and
the binding is proven, not assumed. No `node_id` column on the wire and
nothing for a human to copy twice.

**Stages 1–3 CLOSED 2026-08-03.** The HTTP peer path is deleted, not merely
superseded: `/organ/introduction`, `/organ/inbox`, `/organ/ops`, the whole
`peer_auth` layer, and with them `request_signing_payload` /
`response_signing_payload` / `timestamp_fresh` / `verify_peer_signature` /
`sign_peer_request` / `sign_peer_response` / `verify_peer_response` and the
120s replay window. All of it existed to establish what an iroh handshake
establishes for free. `peers.rs` now holds only the pairing verification code.
Op-batch and Fact signing in `trust.rs` is untouched — different question.
Also closed: changing `lince.discovery` REBINDS the endpoint live
(`wire_supervisor`, the File Sync live-supervisor pattern) rather than
demanding a reboot, since discovery is an Endpoint builder option fixed at
construction. The node key is reloaded from the same file, so the NodeId
survives the rebind — a Cell whose NodeId changed when a setting was toggled
would strand every contact who had saved it. The Organ sand grew a Discovery
panel on the LOCAL Organ for both switches, and it states the cost of each
rather than presenting them as neutral.

Replaced by iroh:
- [ ] `lan_discovery.rs` in full — the multicast announce, the `Announce`
  struct, the nearby-expiry bookkeeping. iroh's mDNS discovery covers the
  LAN case and DNS/pkarr covers the case multicast never could. `GET
  /organ/nearby` survives as a route, backed by iroh's discovery stream.
- [ ] `organ_contact.last_seen_addr` as a routing input — resolution is
  iroh's job. Keep the column as a debugging breadcrumb or drop it.
- [ ] Per-request signing as TRANSPORT auth: `request_signing_payload` /
  `response_signing_payload` / `timestamp_fresh` / the 120s replay window.
  An iroh connection is mutually authenticated at handshake, so
  `connection.remote_node_id()` gives what `verify_signed_request` returns
  today. The substitution is one line in each of three handlers; the JSON
  request and response bodies stay byte-identical, carried on a QUIC
  bi-stream under ALPN `lince/sync/1`.
- [ ] `GET /organ/introduction` as a challenge: connecting IS the proof.
  The route stays for key/name exchange, but it no longer establishes
  anything the connection did not already establish.

NOT replaced — payload signing stays. Op-batch signatures are durable
provenance that must survive store-and-forward through a relaying Organ,
where transport auth proves nothing about the origin. Transport auth
answers "who is on this socket"; payload signing answers "who wrote this
op." Only the first is iroh's.

**The verification code, and why QR retires it** — settled 2026-08-03.
Under iroh the address IS the key, so dialing a NodeId reaches that keypair
or nothing: no wire left to substitute on. The residual threat is only
MISDELIVERY — being handed the wrong NodeId. On a discovery list that is
real: an attacker announces their own NodeId under the display name
"Eduardo's laptop", you dial them, the handshake honestly succeeds, and
they relay your "what did we do last Thursday?" to the real friend and the
answer back. A conversational challenge does not defeat a live relay.

But a QR code scanned in person does, completely, and so does pasting the
key into a chat app already authenticated to that human. Both channels are
unrelayable. Since every flow this product has is one of those two, the
code defends a case that no longer occurs, and a security step users are
taught to click past is worse than no step.
- [ ] Dropped from all normal flows. Kept as an optional verify panel for
  remote pairing, where (a) and (b) are genuinely unavailable.
- [ ] Nearby lists show a short NodeId fingerprint beside the untrusted
  display name — disambiguation among many peers, explicitly not a
  security check, and impossible to spoof by choosing a name.
- [ ] Manual paste-a-key in the Organ sand is trust-on-first-use on the
  identity key. Fine when the key came from somewhere you trust; label it
  as TOFU in the UI rather than implying the typing verified anything.
- [x] Signing-payload domain prefix — take the stronger option: prefix
  `lince/peer/1\n`. Colliding with TLS 1.3 CertificateVerify was already
  impossible (that blob carries 64 spaces and a fixed context string), so
  this is safe-by-design replacing safe-by-luck, and it costs one line
  while the code is being rewritten anyway.
- [ ] Accept policy — NEW surface, the cost of a published key. Today an
  unknown peer cannot get past `verify_signed_request` because we hold no
  key for them. Under iroh, anyone holding the published NodeId can open a
  connection. So the ALPN handler must gate by contact state: a known
  contact gets `lince/sync/1`; an unknown NodeId gets `lince/thread/1` and
  nothing else, rate-limited to ONE pending invite per NodeId; `blocked`
  gets the connection closed. Plus a discoverable on/off switch.
  Default is CLOSED: `lince.discovery.accept_unknown = false` — an unknown
  NodeId is refused at the ALPN gate, so publishing the key advertises
  reachability to people who already know you and grants nothing to anyone
  else. Turning it on is what opens the invite door, and the discovery UI
  must SAY so: the headline flow (meet a stranger on the LAN) needs
  the toggle on, and a nearby list that silently refuses everyone reads as
  broken.
- [ ] Privacy cost of publishing the key — it is a PRIVACY issue, not a
  security one, and the distinction matters. Reaching a Cell across the
  internet works because discovery publishes NodeId → current addresses,
  so anyone holding the published key can resolve that Cell's current IP.
  Nobody thereby reads your data or forges your signature; what leaks is
  roughly WHERE you are (city-level geolocation) and WHEN you are online.
  For a key pasted on a personal website, that is a daily-pattern and
  approximate-home-location leak to anyone who looks.
  It is the same mechanism that makes beach-then-home work, so it is not
  separable — but it IS optional: relay-only mode publishes no direct
  addresses, and peers see only the relay. The cost is a latency and
  bandwidth hop plus dependence on that relay, which is why the later stages open
  with running `iroh-relay` on the user's own VPS. Depending on your own
  machine is not a dependency problem.
- [ ] Discovery mechanisms, concretely, because "internet discovery" is
  vague. iroh offers: **mDNS** (multicast, LAN only, finds peers on the
  same network); **DNS discovery** (nodes publish addresses to a DNS
  server — n0's by default); **pkarr/Mainline DHT** (signed address
  records on the BitTorrent DHT, no central party); and **static** (you
  supply the address). "Turning it on" is an Endpoint builder option, not
  a user action — so it becomes Organ-sand settings: `local` controls mDNS
  advertising/listening on the LAN, independently from `internet`, which
  controls internet address publication. Both default ON. **Default ON**
  (DHT + DNS) for internet reach means a
  Cell that is not resolvable across the internet cannot serve the case
  that motivates the whole design — the VPS telling the phone about a
  change the laptop made. Off is the deliberate choice, not the default.
  Configured through the ordinary extension/config table (`lince.discovery`
  already exists), never a build flag. One wrinkle: discovery is an
  Endpoint builder option fixed at construction, so changing it restarts
  the endpoint — reuse the File Sync live-supervisor pattern (§2) that
  already restarts watchers on a config Fact, rather than demanding a
  reboot.
- [ ] What a relay actually does, since the mental model matters: both
  Cells hold a standing connection to it, so it is a mailbox that is
  always reachable. To reach B, A first sends through the relay — and
  immediately both sides start exchanging the addresses they observe and
  firing probe packets straight at each other. Those outbound probes punch
  a return path through each side's NAT or firewall, and when they meet,
  the connection UPGRADES to direct and the relay leaves the data path.
  So the user's model is right: point at the relay, find each other there,
  continue peer-to-peer. The one correction is the failure case — against
  a symmetric NAT or a strict firewall the punch never lands, and traffic
  keeps flowing through the relay for the life of the connection. It is a
  rendezvous AND a fallback, not only a rendezvous.
  Both VPS jobs coexist on one box: `iroh-relay` on its public address,
  and a Lince Cell that is a member of the Organ roster. They are separate
  processes with separate ports and no interaction.
- [ ] IPv6: prefer it wherever available. NAT exists only because IPv4 ran
  out; with IPv6 every device can have a globally routable address, so
  there is no translation layer to defeat and direct connections succeed
  far more often. iroh already binds dual-stack and races v4/v6 paths, so
  the Lince-side work is only to not get in the way — bind both, publish
  v6 addresses in discovery, hardcode no v4 assumptions. The remaining
  gate is outside Lince entirely: the ISP must hand out IPv6 and the home
  router must have it enabled. Even then most routers keep a stateful
  inbound firewall, so hole punching is still needed — but punching a
  firewall pinhole is far more reliable than traversing address
  translation. IPv6 shrinks the relay's job; it does not remove it.
- [ ] Multi-Cell Organs (phone + home computer + always-on VPS) — designed
  2026-08-02. §2 defines a Cell as one running instance and an Organ as the
  boundary *a* Cell represents; one-Organ-many-Cells is a NEW extension of
  that split, not something already decided, and it has an op-log
  consequence that dictates the shape.

  The consequence first: `idx_sync_op_identity` is UNIQUE on
  `(actor_organ, hlc)` and the comment is explicit that an HLC is unique
  per actor, so the index IS the op uid AND the import idempotency key.
  Three Cells appending under one shared uid would be three independent
  `hlc::next()` clocks in one uniqueness domain — two Cells could mint the
  same identity for different ops, and because import dedupes on the same
  key, the collision does not merely fail a constraint, it can swallow a
  remote op as already-seen. So Cells MUST NOT share a uid.

  Therefore split the two jobs `organ_uid` does today. Each Cell keeps its
  own local Organ Record and uid (`organs::local()` already resolves a
  fixed slug per database, so this is what the code does already):
  `sync_op.actor_organ` is the CELL, HLC uniqueness is untouched, and
  `last_synced_seq`/checkpoints work per-Cell. Cells are then ordinary
  full-trust contacts of each other over the existing op log — no new sync
  path, no new machinery — so "edited on the phone all day, walk in the
  door, laptop converges" is just sync, with the VPS as the Cell that is
  always up so the other two never need be online at the same moment.
  `record.organ_uid` (the §2 origin stamp) carries the SHARED published
  identity, so records still read as one Organ's to the outside.
  Two fields, two jobs: identity vs. authorship.

  The published identity is a roster: one Organ key, signed, listing its
  member Cell uids and NodeIds. An outsider knows only the published key
  and resolves it to whichever member is reachable.

  Why NOT one shared node key across devices: iroh publishes a discovery
  record mapping NodeId → current addresses, so two endpoints with the same
  NodeId each overwrite the other's and a dialer reaches whichever wrote
  last. Worse, it forces the Organ private key onto the VPS — the least
  trusted machine — where compromise is compromise of the identity itself,
  unrevocable in isolation. With per-Cell keys a stolen VPS costs one
  roster entry: revoke that NodeId, the Organ key never touched it. The
  key stays safe to publish precisely because it is only ever an identity,
  never a device.
- [ ] Verify before building: every `actor_organ` write site (they run
  through `store::sync_ops::append`, called from `collab.rs` and the
  append path with `organs::local().uid`) must mean the Cell, and every
  place reading `record.organ_uid` must mean the published Organ. Nothing
  else may assume the two are the same value.
- [ ] Relay metadata: a relay coordinates hole punching and forwards
  packets for nodes that cannot connect directly. It cannot read anything
  (QUIC is encrypted end to end) but it observes that A dialed B at a given
  time. Two things follow. On the LAN no relay participates at all — mDNS
  finds the peer and the connection is direct, so the whole
  find-your-friend-in-a-room flow leaks nothing off the network. Across the
  internet, a successful hole punch leaves the relay with only the
  coordination; a failed one routes through it. Running Lince on your own
  machine does NOT make you a relay — a relay must be publicly reachable at
  a stable address, which is exactly what a node behind NAT is not. The
  always-on VPS from the Cell-roster box is the natural place to run
  `iroh-relay`, and then the Organ's own infrastructure carries its own
  metadata.
- [ ] Licensing: iroh is MIT OR Apache-2.0, Lince is MIT — compatible, take
  it under MIT. (The workspace Cargo.toml said `GPL-3.0-or-later` until
  2026-08-03; the `LICENSE` file has always been MIT, and the manifest was
  simply never updated after the switch. Fixed.) Unlike loro-wasm (browser JS that had to be vendored and
  served under CSP), iroh is an ordinary crates.io dependency: pin it in
  Cargo.toml and carry its MIT text in the licenses dir. No vendoring
  needed for the obligation, only attribution.
- [ ] Migration shape — simplified 2026-08-02: NO dual `url | node_id`
  address kind. That scaffolding only existed to preserve contacts made
  before the refactor, and local dev databases are expendable here (see the
  standing "best schema over back-compat" rule). A contact is reached by
  NodeId, full stop; re-pair the handful of existing ones. The HTTP peer
  routes stay in the tree until the iroh path has actually run end to end,
  then they are deleted outright — that is a working-tree precaution, not a
  schema one.

### Threads: reaching someone before you trust them

Settled 2026-08-03 after two reframes; this paragraph is the current
version and the boxes below elaborate it.

A thread is how a stranger becomes a contact — trust is established by
talking, and only then does `trust` go to `known`. First contact is
verified by QR in person or by a key pasted over an already-trusted chat;
the derived code is retired from normal flows (see the transport box).

**A thread is not a subsystem. It is Records synced with exactly one
peer.** Sharing IS granting that peer sync access, and the same act shares
any Record with any contact — "I choose what to sync with whom, they
agree, only we see it". That axis is **individual replica**, as distinct
from the whole-Organ `sync_out`/`sync_in` feed.

Messaging is NOT collab. A message is a Record appended to a thread and
synced as ordinary `set` ops; two people typing into one string is collab,
and a conversation is not that. The two mechanisms compose — individual
replica carries a Record, collab merges a body someone is co-writing —
but neither is built out of the other.

Why this is a simplification rather than a new feature: it deletes the
special message-delivery path, reuses the op log and the outbox, and makes
messaging a *consequence* of sharing. Discovery demotes to ergonomics for
creating a thread and handing someone access to it.

Deliberately orthogonal to the transport box: a thread works over iroh or
over signed HTTP, and either box can land without the other.

- [ ] Shape — settled 2026-08-03. The synced unit is a **Record per
  relationship**, and MANY THREADS live inside it. Three levels, one grant:
  `Record (kind='conversation', shared with one contact)`
  → `threads` → `messages`. Clicking "chat" on a discovered stranger
  creates the Record and its first thread and opens the Record sand on that
  thread. Adding a second thread later — a different topic with the same
  person — needs no new grant, no new pairing, no new sync setup, because
  it is inside a Record already being synced. That is the whole reason to
  nest rather than make every thread its own Record.
  A `with` Assertion binds the Record to the contact's Organ Record, as
  §1/§3 already allow — still no new ACL table.
- [ ] **The grant cascades via `replica_root`, a denormalized column —
  simplified 2026-08-03.** Since messaging is not collab, the three levels
  are three separate Records joined by Assertions — conversation → thread
  → message. A grant per message Record is absurd and racy (the grant row
  would have to exist before the peer could legitimately receive the
  message it describes), so a grant on the conversation must cover every
  Record inside it.
  The earlier draft did that with a TRANSITIVE ASSERTION TRAVERSAL at
  three enforcement points. That is replaced. The insight: **traverse once
  at grant time, not on every enqueue, serve and import.**
  - `record.replica_root TEXT NULL` — the uid of the Record whose grants
    govern this one. NULL means "rides the ordinary feed", which is every
    Record that exists today, so the migration is a no-op backfill.
  - A grant is a row `(root_record, contact_organ, …)`. One root, many
    contacts.
  - A Record created inside a conversation INHERITS `replica_root` from
    its parent at creation, which is when the parent is known anyway.
  - Sharing an arbitrary existing Record makes it its own root
    (`replica_root = own uid`) and stamps its subtree in ONE walk at that
    moment — an explicit "adopt into root" operation. New descendants
    inherit thereafter.
  The three enforcement points then become the same indexed equality
  check instead of three graph traversals that must agree:
  - the outbox enqueue predicate, today
    `INSERT…SELECT FROM organ_contact WHERE sync_out=1`, which now also
    admits "a grant on this record's `replica_root` covers this contact";
  - feed-serve filtering;
  - the import gate.
  No depth limit, no cycle handling, no traversal denial-of-service, and
  no accidental oversharing through an Assertion nobody thought of as a
  containment edge. Revoking is deleting grant rows; an in-flight Record
  whose grant vanished mid-transfer fails the same check on arrival.
  The `sync_outbox` PK still fits with no schema change — what changed is
  the selection, not the shape.
  **Three guards, without which this is a regression and not a
  simplification:**
  1. `replica_root` is LOCAL-ONLY and IMMUTABLE — never a settable synced
     field. Otherwise a contact sends a `set` moving a Record between
     roots and re-scopes what gets shared with third parties.
  2. On import the root comes from the CHANNEL, not the payload. Ops
     arrive on a stream already scoped to a grant; assign the root from
     that context and never read it off the wire.
  3. ONE root per Record. A Record cannot belong to two roots; a root may
     be granted to many contacts. That covers every case described and is
     the narrow form of the consequence recorded below.
  **BUILT 2026-08-03** (migration 0042, `store::replica`, `engine::threads`).
  Two constraints the implementation forced, both worth keeping:
  - **Stamped at CREATION, not at grant time.** The draft above had a walk
    that adopted an arbitrary existing Record into a root. That opens a
    window: the Record already logged ops with `replica_root = NULL`, those
    ops already rode the general feed, and any `sync_in` contact gets them
    on their next catch-up. Adoption is therefore DEFERRED — it needs a
    backfill decision and a UI that says plainly that already-sent ops
    cannot be un-sent. Creation-time inheritance covers conversation →
    thread → message, which is the shape that exists. It is also what makes
    the immutability claim true, and the denormalized `sync_op.replica_root`
    is only safe because of it.
  - **Import enforces immutability, it does not merely apply the stamp.**
    Three cases on arrival: the Record exists in THIS root → apply; exists
    in a different root or on the general feed → quarantine, because that is
    a grantee re-scoping one of your uids through a channel that does not
    govern it; does not exist → create inside the channel root.
  Two bugs the tests caught, recorded because both were silent: imported
  Records were not being stamped at all (the receiver's copy would have
  ridden the RECEIVER's general feed), and an Assertion's predicate Concept
  lives on the general feed, so a grant-channel import hit a foreign-key
  failure that rejected the whole conversation — the importer now inserts a
  Concept stub, since depending on general-feed sync would break exactly the
  case that matters, a contact granted one conversation and no feed at all.
- [ ] **Messaging is NOT collab** — settled 2026-08-03, and it retires the
  `threads`-Loro-Map plan written a day earlier. Sending a message is
  appending a Record to a thread; it is not two people typing into one
  string. So messages are ordinary Records synced as ordinary `set` ops
  through individual replica, ordered by HLC, and no Loro doc is involved.
  "Ordered by HLC" is `record.created_hlc` (migration 0047), added 2026-08-04
  when the conversation view was found ordering by `created_at` — a local
  wall clock, so a peer whose clock is slow sorts into the past forever and
  it reads as a rendering bug rather than the clock problem it is.
  Denormalized onto the row for the same reason `replica_root` is: written
  once at creation, never changed, so a copy cannot drift. One stamp per
  record rather than per op, because `log_local` mints an HLC per FIELD and
  "the record's HLC" would otherwise be several values. The import path
  carries the ORIGIN's stamp — a fresh local one would silently make it
  arrival order, which is the same bug by another route. Pinned by a test
  that skews the receiver's clock backwards and asserts the order holds.
  Concurrent sends do not conflict — they are two different Records, both
  arrive, both display. Editing a sent message is LWW on that Record.
  Collab (Loro, real-time merge, presence) stays what it always was: two
  people editing the SAME record's body at the same time. A conversation
  is not that.
  What this deletes, all of it invented and none of it needed:
  - kind-dependent record-doc layout — `collab.rs` keeps its
    `doc.getText("body")` assumption untouched, and the "largest engine
    change the reframe causes" no longer exists;
  - `record_doc.snapshot` as a place conversations leak, since a thread
    has no doc — the at-rest surface drops from three copies to two
    (`record.head`/`body` and `sync_op.value`), with the snapshot relevant
    only for Records that are separately collab-edited;
  - unbounded doc growth for chat, and with it the urgency behind shallow
    snapshots. A thread is rows, not a document.
- [ ] Efficiency follows from that for free: rendering a thread is a
  SELECT of the last N messages on an index over (thread, hlc), with older
  pages fetched on scroll. Nothing is loaded whole into memory, and a
  ten-year conversation costs the same to open as a new one. Shallow
  snapshots remain a later concern for genuinely long-lived collab
  documents, where human authorship bounds the size anyway.
- [ ] Consequence to accept: one Record, one grant, so ALL threads inside
  are shared with that contact — you cannot share one thread and withhold
  another from the same person. That is the right default for a
  per-relationship Record, and the escape hatch if it is ever wrong is to
  put the private topic in a different Record.
- [ ] Record sand gains a conversation view: thread list plus a message
  composer over the message Records. The existing text binding is the
  other branch of the same sand, untouched — a Record is either being
  talked in or being co-written, and the two views never contend.
- [ ] Collab's role in messaging is exactly this small, and no larger: if
  both parties happen to OPEN THE SAME message Record, its body behaves
  like any other collab-edited body — Loro merge, cursors, presence. That
  is the whole of it. Everything else about a conversation is normal sync.
  No conversation-specific CRDT, no special layout, no new mechanism.
- [ ] Individual replica, the new sync axis: today `sync_out`/`sync_in` are
  per-CONTACT and mean the whole visible feed. Individual replica is
  per-RECORD-per-contact: this Record syncs to these peers and to nobody
  else, and does NOT ride the general feed. It is a genuinely new
  enforcement point, not "the §12 visibility gate with a narrower
  selector" — §12 asks whether a contact may see the feed at all, and
  this asks which Records leave the Cell for whom. Both run; neither
  substitutes for the other.
- [ ] Bidirectional by agreement: the sharer offers, the receiver accepts,
  and only then does the Record land in their Cell. Acceptance is what
  turns "you may see this" into "I keep a copy," and it is also what stops
  an Organ from pushing unwanted Records into someone's store.
- [ ] Reach is the individual-replica GRANT — corrected 2026-08-03. The
  earlier "reach is derived from a live thread Record, deletion IS the
  revocation" no longer holds: under the reframe a thread is a synced
  Record and the grant is precisely a stored per-record-per-contact row.
  Deleting a Record and revoking a grant are now two acts, and conflating
  them breaks in both directions:
  - the peer keeps pushing `set` ops for that uid, so a purely local
    delete leaves them writing to a record that is gone — import must
    DROP those ops, never resurrect the record. (No Loro copy is involved:
    messages are not collab, so a thread has no doc.)
  - `tombstone` is a synced op kind, so if deletion emitted one it would
    delete THEIR copy of the conversation too, contradicting "both parties
    keep a copy."
  So: **deletion = local removal + explicit revocation of the grant**, and
  the grant is what the ALPN gate and the import path check. Their copy
  survives, their ops stop being accepted, and nothing is reached into on
  their Cell — which is exactly §12's honest split between revoke (hard,
  local, guaranteed) and forget (a request the remote may honour).
- [ ] Good news on plumbing: `sync_outbox`'s primary key is already
  `(contact_organ, tbl, uid, field)` — per-contact-per-record. Individual
  replica fits the existing outbox with no schema change.
  Scope: a known contact with `sync_out`/`sync_in` still gets
  `lince/sync/1` and still syncs — cutting a contact off entirely is
  `trust='blocked'`, which is the terminal switch §2 already defines.
  Silencing a conversation does not unfriend.
- [ ] **Revocation level — SETTLED 2026-08-03: at the conversation, i.e.
  at the individually-synced Record.** The nested shape had moved the
  grant one level above the thread, so per-thread revocation was no longer
  expressible without giving up the property that made nesting attractive
  (a new topic needing no new grant). Resolved in favour of the simple
  workflow: **reach closes when either party blocks the other, or when
  either party deletes the individually-synced Record.** Deleting a single
  thread inside it is then just deleting a Record, with no reach
  consequence — the conversation is the unit of relationship, and it is
  the unit of revocation.
  Symmetric by construction: both sides hold the same switch, and neither
  needs the other's cooperation. Blocking (`trust='blocked'`, terminal per
  §2) closes everything with that Organ; deleting the shared Record closes
  just that conversation. Two switches, both local, both guaranteed —
  nothing here is a request the remote may decline.
- [ ] Not Karma grants (K5.1). Those delegate one PERSON's authority to
  another and are signed by the Person's key. Thread reach is an ORGAN-level
  question — who may open a stream — resolved before any Action exists.
  Different axis, different layer; do not fold them together.
- [x] An invite is not a thread: `kind='thread_invite'`. One pending per
  Organ, so a deleted thread cannot become a spam channel. Accepting opens
  the conversation; `trust='blocked'` (terminal everywhere per §2) drops the
  invite before it is written.
  DONE 2026-08-04, with two changes from the sketch above.
  **Not an Assertion.** The sender lives in a `thread_invite` table with
  `from_organ` UNIQUE, and that constraint IS the one-pending rule — enforced
  in SQL rather than by a check-then-insert, because an Organ is several
  Cells and a contact's laptop and VPS can offer at the same moment. A
  `lince.invite` extension mirrors it for display, the way `lince.pairing`
  mirrors the invite code onto the Organ record.
  **Written with plain SQL, never `records::create`.** The Record write path
  logs an op and enqueues it to every known contact, so creating an invite
  the ordinary way would push "Bea is asking to talk to me" to everyone you
  know. `organs::add_contact` already had this shape and for the same reason.
  A test asserts the op log and the outbox both stay untouched.
  The grant row and the invite are kept SEPARATE: `replica_grant` is the
  mechanism (it decides whether ops are accepted), the invite is the surface
  (it is what a person answers). Collapsing them would mean an offer could
  not be shown without already having decided something.
  Two exits only, and no "dismiss": accepting grants, declining REVOKES.
  Clearing an invite without answering would leave the sender waiting on a
  reply that never comes while the one-per-Organ slot stayed occupied — so
  they could not ask again either. Declining frees both.
  A repeat offer gets the same answer as a first one. Telling a sender their
  offer was dropped would tell them whether the last was declined or merely
  unanswered, which is not theirs to know.
  `blocked` needed no new code: `serve_connection` closes on it before the
  ALPN split, so a blocked Organ never reaches the handler at all.
- [ ] Encryption — revised 2026-08-02, and the revision is BOTH simpler and
  stronger. The earlier plan (static X25519 DH, seal each body) has no
  forward secrecy: one long-term key stolen in two years decrypts every
  message ever recorded. Instead: **threads are direct Cell-to-Cell over
  iroh, and iroh's QUIC/TLS 1.3 already provides authenticated, encrypted,
  FORWARD-SECRET transport** — ephemeral session keys, discarded after use,
  so a later key theft yields nothing. What is left is protecting messages
  AT REST, which is a local storage-key problem, not a DH problem.
  Message-layer sealing only buys something when a message passes through a
  THIRD Organ; keep threads direct and it buys nothing while costing
  forward secrecy. The always-on VPS Cell holds messages when a peer is
  offline — but that Cell is the user's OWN Organ, so it is not a third
  party. If store-and-forward through someone else's Organ is ever wanted,
  that is when sealing returns, and it should return as a real ratchet
  (`openmls` or an audited Double Ratchet crate), never hand-rolled.
- [ ] Reach is the GRANT, and revoking it is what stops inbound messages —
  refused at the import gate, not filtered in the UI. No time limits, no
  expiring grants. (This bullet formerly said "reach is the thread"; that
  was the pre-nesting model, superseded by the grant bullet above and by
  the open decision on revocation level.)
- [ ] What remains after deletion is exactly one thing: they may send an
  INVITE to open a new thread, one pending at a time, which lands in
  notifications. `trust='blocked'` drops invites too.
- [x] Invites surface as a queue: who is asking, and nothing they chose to
  call themselves. Accepting opens the thread; it does NOT set trust or
  enable sync.
  **The surface moved, 2026-08-04.** This bullet said "the notification
  panel on the board base rail". That panel is board CHROME fed by a host
  route (`organ_login_required`, `app_update_installable`, dismissal over
  `fetch`) — putting Ledger data in it would have meant a new host route
  plus accept/decline endpoints, and a fetch-polled list where a live
  subscription belongs. Invites live in the **Conversations sand** instead,
  read with an ordinary Protein subscription on `kind='thread_invite'` and
  answered with `accept-thread-invite` / `decline-thread-invite`. No new
  route, no new Protein source, live for free, and consistent with the
  Protein-first direction stage 2b set.
  What is rendered is the Organ uid the CONNECTION proved. There is no
  claimed display name on an invite at all — the sketch above would have
  shown one "marked as untrusted", and not having it is simpler and safer.
- [ ] Key exchange IS the promotion step, and it happens inside the thread:
  a "send my key" button posts the local Organ's identity key + NodeId as a
  message; receiving one offers "add as known Organ" with a name field the
  local user types (never the sender's claimed label). That single action
  writes the contact, adopts the key, and sets `trust='known'`. The Organ
  sand keeps the same thing by hand — paste a key, type a name — for
  contacts who never used a thread.
- [ ] Message Records must NOT ride the ordinary record sync feed. They are
  delivered over the thread ALPN only. Otherwise a visibility bug in the
  normal feed leaks a private conversation to an unrelated contact, and the
  sealed body would still expose who is talking to whom.
- [ ] Consequence of dropping message-layer DH: no separate X25519 key is
  needed at all, and no ed25519→montgomery conversion. One less published
  key, one less primitive to get wrong.
- [ ] But be exact about what was traded: dropping DH is stronger IN
  TRANSIT (forward secrecy) and WEAKER AT REST — without message-layer
  sealing, thread bodies sit in plaintext SQLite on both ends. That is a
  real regression against the original "e2e encrypt it" ask, so **local
  at-rest encryption of message bodies is an EARLY item**, not a someday
  box. Only with it does "encrypted in transit, forward secret, encrypted
  at rest" become a true sentence.
  This also resolves an inconsistency: the VPS was called the least
  trusted machine when arguing to split node key from identity key, and it
  cannot then be trusted enough to hold plaintext conversations. Vetting
  answers are exactly the material an impersonator would want — "what did
  we do last Thursday" is replayable once read.
- [x] Offline delivery, early: a peer with a closed laptop is
  unreachable, so a local send queue that flushes on next successful
  connect must exist or the beach case (meet, exchange keys, they go home)
  fails on the first message.
  VERIFIED 2026-08-04, and it already worked — no announce protocol was
  needed. `sync_outbox` holds the queued rows, a failed pass leaves them
  queued, and the next pass that connects delivers them. The sketch above
  proposed "a Cell coming online announces itself to the contacts it KNOWS,
  which wakes their queues"; that would only reduce latency from "their next
  interval" to "immediately", because both sides dial anyway — the sender
  retries in `push_outbox` and the receiver pulls in `pull_catch_up`.
  Pinned by a test that sends while nobody is serving, asserts nothing
  arrived, then starts serving and asserts it does. **The retry IS the
  delivery**, and if that ever stops being true this breaks silently in the
  one case it exists for.
  Opting out of being pinged is just moving that contact to `unknown` —
  no separate setting.
#### At-rest encryption, scoped to individual replica

The earlier plan — skip the op log for message Records — DOES NOT SURVIVE
the reframe. A thread is now a synced Record, so its ops must exist and
must ship; an op that is never written cannot sync. So the protection moves
from "don't log it" to "log it encrypted."

Verified hazard it has to solve: `records::log_set` calls
`sync_ops::log_local` with the value inline, so a `set` on a body writes
that body verbatim into `sync_op.value`, and `crdt` ops carry Loro update
bytes the same way. Encrypting `record.body` alone would leave a plaintext
copy of everything in the op log.

- [ ] Plumbing prerequisite — SIMPLIFIED 2026-08-03. `records::log_set` is
  a low-level store function with no notion of grants, so something must
  tell it that this Record's op values are encrypted. That something is
  **`replica_root IS NOT NULL`** — the same column that scopes the grant
  cascade. No separate boolean, and the two scopes cannot drift apart,
  because "individually replicated" and "encrypted at rest" are by
  definition the same set of Records. The column must still exist BEFORE
  the encryption work starts: retrofitting it after rows are written means
  rewriting history.
- [ ] Encrypt at the FIELD boundary, above `log_set` — simplified
  2026-08-03. If `records::set_text` encrypts before calling `log_set`,
  then the value handed to the op log is ALREADY ciphertext and
  `sync_op.value` is covered with no second cipher call site. Verified
  2026-08-03 that this closes the log: `sync_outbox` stores `seq`, a
  POINTER into `sync_op` (PK `(contact_organ, tbl, uid, field)`, column
  `seq INTEGER`), so it never holds a value copy and is not a third
  plaintext store.
  The wire then needs decrypt-on-send and encrypt-on-import, since the
  peer holds a different local key — two points, both inside the sync
  path, replacing the enumerated storage-edge list below.
- [ ] Scope: individually-replicated Records only. The general feed is not
  blanket-encrypted — that is a whole-database problem with a different
  answer (full-disk encryption, SQLCipher) and it is not what is being
  bought here. What is bought is that private, per-peer material is not
  legible in a stolen copy of the database.
- [ ] Boundary: encrypt on write, decrypt on read. NOT applied to the
  payload before sending — the peer holds a different local key and could
  not read it. On the wire, iroh's transport encryption is the entire
  story, forward secrecy included.
- [ ] TWO plaintext copies exist, and both are covered by the single
  field-boundary encrypt above:
  1. `record.head` / `record.body` — the materialised text.
  2. `sync_op.value` — written inline by `records::log_set` →
     `sync_ops::log_local`, and therefore already ciphertext.
  `record_doc.snapshot` was a third copy under the old Loro-thread plan
  and is no longer one: a thread has no doc. It matters only for Records
  that are separately collab-edited, and if such a Record is ever
  individually replicated, `collab.rs::with_doc` and `maybe_compact` need
  the decrypt too.
- [ ] **Every reader that does not decrypt sees ciphertext** — the thing
  most likely to bite mid-implementation, and it fails quietly as garbled
  text rather than loudly as an error. Enumerate and fix them all:
  query and projection (§6), Protein reads (§10), search, export/archive,
  and File Sync's materialisation to disk. A Record that is encrypted at
  rest must either decrypt for these or be deliberately excluded from
  them, and which one is a per-consumer decision — File Sync writing an
  encrypted body to a plaintext file on disk would defeat the whole
  scheme, while search silently indexing ciphertext is merely useless.
- [ ] Cipher and key: XChaCha20-Poly1305 with a 32-byte key in a file
  beside the database at mode 0600. Chosen over the alternatives for
  specific reasons — an OS keychain does not exist on a headless VPS;
  deriving from a login password makes data unreadable whenever nobody is
  logged in, which breaks background sync and is exactly wrong for an
  always-on Cell; SQLCipher encrypts everything including material that
  gains nothing from it, and adds a heavy dependency.
- [ ] What it defends, stated exactly so the UI does not overclaim: a
  database copied out through a backup, a synced folder, a cloud drive, or
  a disk pulled from a machine without full-disk encryption. What it does
  NOT defend: anyone already executing as your user, who can read the key
  file as easily as the database. It raises the cost of casual
  exfiltration; it is not a defence against a compromised host.
- [ ] Consequence to accept: a Cell must hold the key online to serve its
  own data, so the key is warm whenever Lince runs. That is inherent to
  a server that answers requests, not a flaw in the choice.
- [ ] Upgrade path, not the starting point: OS keychain integration where
  a keychain exists, leaving the file only for headless installs.

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

Corrected 2026-08-02. An earlier pass redefined `live` as "hold the iroh
connection open" — that was a drift and is withdrawn. Connection pinning is
a transport tactic, not a mode. The mode is about WHERE THE DATA LIVES, as
originally written, and it is the same thing the product calls "live
login":

- [ ] `live` = an in-memory session against a REMOTE Organ. Zero local
  rows, the remote is authoritative, and the sand renders data streamed
  into memory that is never persisted locally. Logging into another Lince
  and editing its Records — with full CRDT on text — IS live mode; there is
  no second meaning of the word.
- [ ] `replica` = a local persistent copy. Ops sync both ways, both sides
  store, checkpoints track what the other has seen.
- [ ] Transport is orthogonal to the mode. A browser reaching a Cell uses
  HTTPS/WS; a Cell reaching another Cell uses iroh. Both can serve a live
  session; neither changes what `live` means.
- [ ] Where iroh genuinely extends live mode (later option): a Cell with
  no public hostname cannot be browsed to, but YOUR Cell can reach it over
  iroh and front it. Your local Lince becomes the door to a remote Organ
  that has no door of its own.
- [ ] Until then `sync_out=1` is mode-independent: the outbox drains to
  every non-blocked contact with the flag set, fact-bus-woken (~250ms
  coalesce), which is already live-ish in practice. The `mode` column
  exists and DEFAULT 'replica' but no code branches on it yet.
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


# Alexandria
Was a library inside a temple, well maintained and kept, in a city with a port, many travelers where asked to hand their books and manuscripts and receive a copy instead. While there is the common imagination that it was burned, the details are a little conflicting. What remains from the story is the idea of a great body of knowledge, that worked because it was cared for, and the fact that it can suddenly catch fire and be lost. Great care can be put into maintaining and expanding knowledge. It will most likely provide itself useful if used for the meeting of our Needs.

In Lince, the Alexandria vibe means sharing Records as knowledge of what things are, how they work, their consequences, and how to implement them.

That in turn means possibly caring for the building of interfaces and components to access knowledge, learn it and help use it while also helping with the management of knowledge: writing it and sharing it.

Below are some cases for the first steps towards having such alexandria abstraction with lince, a free flow of information to better us all. We must advance our knowledge of how to perform this great task, turning knowledge refinement into a craft. Many have done it in the past, and we now stand in their shoulders. Knowledge can be inbued into components, when it is activated it creates Records with that content. Or maybe knowledge can be data in one specific server, but then you would need to contact such server to access it, if it's gated you loose access. Would it be best if it where inside a binary, inside a sand, in seed? We must find out which one is best, and support ourselves with past work, that made available to all a vast amount of knowledge in the internet, free, maybe we can import it, integrate with it, to jumpstart Alexandria.

# Nutrition - Home Manager.

Implementation checklist for the Home Manager nutrition tab.

- [x] Replace the old Home Manager surface with thin top tabs: Nutrition and Bills.
- [x] Keep Bills as a thin placeholder tab for this pass.
- [x] Base nutrition rules on Brazil's current Ministry of Health food guide: prefer in natura and minimally processed foods, use culinary ingredients in small amounts, limit processed foods, avoid ultraprocessed foods.
- [x] Use TBCA-style per-100g food-composition fields for built-in frontend data.
- [x] Embed the initial food knowledge base in the Home Manager sand as JavaScript objects, not Lince tables.
- [x] Include roughly 100 common food objects across fruit, vegetables, legumes, grains, roots, meat, eggs, dairy, nuts, seeds, oils, and Brazilian staples.
- [x] Include calories, macros, fiber, common vitamin fields, common mineral fields, category, NOVA group, density, and generic-currency price fields where known.
- [x] Store unknown custom-food micronutrients as `null`, never silently converting an empty edit field to zero.
- [x] Add a custom alimentum record workflow using `record` plus `record_extension`.
- [x] Use `record_extension.namespace = "nutrition.alimentum.v1"` for custom alimenta.
- [x] Merge built-in foods and custom alimentum records in the frontend catalog.
- [x] Keep marmita plans, optimizer inputs, allocations, prices, and shopping lists as widget/card state.
- [x] Let the user configure weight, height, sex, age, activity factor, days, pot count, pot volume, meals per day, food min/max, and forced grams.
- [x] Generate marmita allocation, nutrient totals, shopping list, total price, and price per marmita.
- [x] Add a lowest-price optimizer using a frontend two-phase simplex linear optimizer with infeasibility reporting.
- [x] Add visual workflow coverage for tab switching, custom alimentum creation, price editing, plan generation, optimizer run, and shopping-list display.

## What is left after stages 1–7 (2026-08-04)

Stages 1–7 are closed and the workflow they exist for runs end to end: discover
an Organ, add them by QR/paste/nearby, invite and talk in threads, grant a
login, and edit a Record together with live cursors — all over iroh, so
changing network does not break any of it. What follows is everything
deliberately NOT done, with enough of the reasoning to pick it up cold.

**Replica bootstrap.** A new contact receives ops from the moment you connect;
they do not receive what already existed. This is the initial catch-up, and it
bites when someone adds a second device or accepts a conversation with months
behind it. Left undone because it needs decisions rather than typing: snapshot
versus replaying the op log, how to page it so a large replica does not hold a
connection open, and how a half-transferred replica presents itself (a partial
copy that looks complete is worse than an obvious gap). Nothing in the current
workflow reaches it, because both Cells were present from the start.

**A contradiction to resolve before anyone relies on either sentence.** Item 5b
DROPPED at-rest encryption as "not free". The Threads section still says
"local at-rest encryption of message bodies is an EARLY item, not a someday
box", and reasons elsewhere as though it exists. Both cannot be true. Today
thread bodies sit in plaintext SQLite on both ends, which is a real regression
against the original "encrypt it" intent, and the honest sentence is
"encrypted in transit, forward secret, plaintext at rest". Decide which
statement survives and delete the other.

**Live sessions do not resume.** QUIC migrates a path under a connection that
stays open, which is what makes roaming work — but if the connection is
actually lost (laptop asleep, peer restarts) there is no reconnect or resume.
The browser socket simply ends. A resume needs a session id and a decision
about what a client may replay.

**`contact.mode` is unused.** The column distinguishes `replica` from `live`,
and nothing reads it: live sessions are opened explicitly rather than chosen
per contact. Either wire the setting or drop the column — a field that lies
about being a setting is worse than no field.

**Collab embeds are partial by design and by accident.**
- Relations has NO embed, deliberately: it has no text surface, because its
  record sidepanels were removed by an earlier decision recorded in that
  sand's own header. Reversing that is a design call, not a task.
- Kanban and table embeds are best-effort — they degrade to a plain input if
  the shared document cannot be reached, which is correct, but means a silent
  loss of liveness that nothing surfaces.
- Only the record editor renders CURSORS. The embeds sync text but show no
  presence.

**The wasm is unproven in an iframe.** `crates/web/tests/collab_wasm.rs` runs
the shipped bundle and the shipped editor module in real node, including
concurrent edits through a relay that echoes like the engine. What no test
here can prove is that a sand IFRAME may load it — that needs a browser. The
live board serves no CSP header today and sands are same-origin `srcdoc`
frames, so it should work; "should" is doing real work in that sentence.

**Off-LAN camera scanning.** `getUserMedia` needs a secure context, so scanning
a QR code does not work over plain HTTP to a LAN hostname — which is how a
second device reaches this Cell. The chrome says so specifically rather than
failing as "no camera". Fixed by the hostname/TLS item, not before.

**The ordinary HTTPS login path** (hostname, certificate, reverse proxy) is
still unbuilt. It is no longer on the critical path — live-over-iroh replaced
it for the workflow that motivated it — but it remains the only way a browser
reaches a Cell that is not its own.

**Audit and retention/pruning** were never started. The op log grows without
bound and nothing summarizes who did what.

**Fiote** remains deferred, as it has been since the persistence cutover.

## Storage Architecture

The built-in nutrition knowledge base is package data. It is compiled into the official Home Manager sand and is never written to Lince persistence. This keeps researched base values deterministic, reviewable, and portable with the widget.

Custom foods are the only nutrition records created by this pass. They are stored as `record` rows with a single `record_extension` payload in the `nutrition.alimentum.v1` namespace. The frontend reads those records through dedicated Home Manager widget actions and merges them with the built-in catalog at render time.

Marmita plans are operational state, not records. Profile settings, food selection, min/max/forced grams, prices, generated allocations, optimizer output, and shopping lists are saved in widget/card state through the bridge, with localStorage only as a preview fallback.

## Data Shape

Built-in and custom foods share this object shape:

```json
{
  "id": "builtin:arroz-integral",
  "name": "Arroz integral cozido",
  "category": "cereals",
  "nova": "minimally_processed",
  "densityGPerMl": 0.78,
  "pricePerKg": 7.5,
  "portionG": 100,
  "nutrients": {
    "kcal": 124,
    "proteinG": 2.6,
    "carbG": 25.8,
    "fatG": 1.0,
    "fiberG": 2.7,
    "calciumMg": 5,
    "ironMg": 0.3,
    "magnesiumMg": 43,
      "potassiumMg": 86,
      "zincMg": 0.6,
      "sodiumMg": 1,
      "phosphorusMg": 83,
      "seleniumMcg": 5.1,
      "copperMg": 0.1,
      "manganeseMg": 0.7,
      "vitaminCMg": 0,
      "vitaminAMcg": 0,
      "vitaminDMcg": 0,
      "vitaminEMg": 0.2,
      "vitaminKMcg": 1,
      "thiaminMg": 0.1,
      "riboflavinMg": 0,
      "niacinMg": 1.3,
      "vitaminB6Mg": 0.1,
      "folateMcg": 4,
      "b12Mcg": 0
  },
  "source": "Brazil food-guide category and TBCA-compatible planning value per 100g"
}
```

Custom records store the same object, without the `builtin:` identity, in:

```json
{
  "schema": "nutrition.alimentum.v1",
  "food": { "...": "same shape" }
}
```

## Optimizer

The optimizer minimizes total generic-currency price over food gram variables. It uses a two-phase simplex tableau with these constraints:

- per-food min, max, and forced grams
- total marmita volume from pot count and pot volume
- minimum calories from Mifflin-St Jeor estimated expenditure
- minimum daily protein
- minimum daily fiber

If the LP is infeasible, the UI reports the violated class of constraint and keeps a generated fallback rather than silently producing a broken plan.

## Sources

- Ministry of Health, `Guia Alimentar para a Populacao Brasileira`, 2nd edition, official Gov.br listing updated 2021-07-29: https://www.gov.br/saude/pt-br/assuntos/saude-brasil/publicacoes-para-promocao-a-saude/guia_alimentar_populacao_brasileira_2ed.pdf/view
- Ministry of Health PDF mirror in BVS: https://bvsms.saude.gov.br/bvs/publicacoes/guia_alimentar_populacao_brasileira_2ed.pdf
- TBCA/USP food composition database: https://www.tbca.net.br/

The UI must not claim the current official guide is a food pyramid. It may present a practical hierarchy based on NOVA processing groups. The embedded catalog is a planning database shaped from food-guide categories and TBCA-style per-100g fields; it is not a clinical or labeling-grade copy of TBCA records.

## AniccaDB: Lingua-aware Markdown file projection (proposal)

AniccaDB can make a file-synced Markdown note carry a small, readable view of
the Record's Lingua state before its ordinary text. This is a **projection** of
existing tables, not a second database and not a replacement for `record.body`:
Records, Concepts, and Assertions remain authoritative. The feature belongs to
File Sync configuration because the generated section is for an interoperable
file representation, rather than for every Record body in the Cell.

For example, a configured Record could be written as:

```markdown
---
@task [[Project A]]
[Image #1]
quantity: 12 @hour
---

Write the project brief.
```

The delimiters resemble front matter, but this is deliberately **not YAML
front matter**: each line is Lingua syntax. `@task [[Project A]]` is a binary
assertion (the note is the subject; `task` is the Concept; `Project A` is the
object Record). `[Image #1]` is a Record link/reference rendered according to
the configured mapping; its precise assertion predicate must be explicit in
the mapping rather than inferred from display text. A unary assertion renders
as `@task`. Links use titles for people, while retaining a stable Record UID in
machine-owned metadata or a collision-safe encoding so renamed Records and two
Records with the same title cannot change their identity.

Quantity is an important projected property: when enabled, it renders the
Record's current exact cached level together with its unit Concept (for
example, `quantity: 12 @hour`). It remains a projection of the authoritative
Ledger Fact fold; editing it from disk must use the normal quantity action,
which appends a Fact, rather than overwriting a cached column.

A `lince.file_sync` configuration should opt in per Organ and define which
fields and assertion predicates project, their order, link rendering, and
whether a line is editable from disk. An initial useful configuration shape is
conceptual rather than a frozen wire format:

```json
{
  "lingua_prelude": {
    "enabled": true,
    "assertions": ["task", "references"],
    "include_identity": true,
    "include_quantity": true,
    "link_style": "wiki",
    "disk_editable": true
  }
}
```

File Sync writes `generated prelude + blank separator + record.body`. All
normal body surfaces receive and edit only `record.body`; they neither display
nor allow a user to accidentally alter the generated prelude. This avoids
placing a stale duplicate of assertions in canonical text and makes a normal
body edit independent of a relation edit.

On export, changing a selected assertion, identity Concept, Record title, or
the title of an object Record causes File Sync to rewrite every affected
projection. The generated header must be deterministic (configured ordering,
then stable UID ordering) so a no-op tick does not churn files. It must also be
tracked separately from the source body hash, otherwise the File Sync watcher
would interpret its own rewrite as a user body edit.

On import, the parser first recognizes and removes the delimited prelude. The
remaining Markdown updates `record.body` through the existing
`EditRecordText` path. If `disk_editable` is enabled, valid changed prelude
lines are translated into the corresponding assertion/identity actions;
otherwise any prelude edit is reported as a conflict and regenerated from the
database. Ambiguous titles, unsupported syntax, duplicate links, and unknown
Concepts must never silently create or retarget Records: retain the file,
report the import error, and leave authoritative state unchanged until the
user resolves it.

This gives zettelkasten-compatible `[[other note]]` ergonomics while preserving
the ontology's one-source-of-truth rule. It is also a small database-like
document view: adding a new property means adding an explicit projection
mapping, not adding a bespoke Record column or parsing arbitrary prose for
meaning.


# Simulation

DST - Deterministic Simulation Testing.

https://alex-ii.github.io/notes/2018/04/29/distributed_systems_with_deterministic_simulation.html

DST is amazing! The idea (I think) is to have three things:
        1. The Seed: the user's DNA (la ele).
        2. The Rules: What events should be bookmarked or stop the simulation?
        3. The Engine: How will this simulation happen? With the normal flow of time, or a tampered one? Connecting to the outside world with Commands?

        This way we can create futures shown to the user so they can see to the end of their Karma and catch bugs or unintended behavior.
        This is useful in finantial simulation, or for understanding the costs of time for doing tasks (like the Calendar feature).

        With DST we may duplicate the DNA to change it freely without affecting the user's data, or perhaps not changing persistent data at all,
        just manipulating data inside the program.

        TigerBeetle is the GOATED db for this, perhaps Lince can learn from it, fork it, or use it with a different schema for Transaction of Records.

        https://youtu.be/sC1B3d9C_sI?si=_HbNMQ9NVegLyS2a

        https://www.youtube.com/watch?v=JoYjji1DZCE


        Turso does not use a basic test script that just writes random data to different databases. Instead, Turso utilizes Deterministic Simulation Testing (DST) by completely abstracting the environment—including time, the network, and file system I/O—and replacing it with a pseudo-randomly seeded simulator. [1, 2, 3]
Because Turso is a ground-up rewrite of SQLite in Rust (originally under the repo name Limbo), they designed the core engine following "TigerStyle" software principles, ensuring that absolutely every background task can be controlled deterministically by a single PRNG seed. [3, 4, 5, 6]

---

## 📂 How It Is Structured

Turso's simulator code is organized inside their repository under their testing directories (such as testing/simulator/). It is broken into four distinct architectural layers: [2, 7, 8]

1.  Simulator (main.rs): The entry point. It generates random configuration setups and interaction plans, executing them sequentially or concurrently inside the runtime loop. [2, 7]
2.  Model (model.rs): A highly simplified, memory-resident representation of what the database should contain. It tracks atomic actions like insertions and selections to acts as a "source of truth". [2, 7]
3.  Generation (generation.rs): The code responsible for pseudo-randomly generating interaction plans, mock database tables, and schema workloads based on a configured workload distribution. [2, 7]
4.  Properties (properties.rs): Defines invariants and core database properties (like transaction atomicity, linearizability, or isolation levels). The engine checks these assertions at every step of the simulation loop. [2, 7, 9]

## 🛠 How the Simulation Logic Works

Turso avoids standard third-party Rust crates that interact directly with the operating system or system clock. Instead, the simulator operates through a strict architectural loop: [10]

- Complete I/O Mocking: The core database code doesn't make standard asynchronous calls directly to Linux io_uring or system threads during simulation. All network requests, file writes, and time delays flow through the simulation layer. [1, 10, 11]
- The Power of the Seed: The simulator generates an initial random seed. If an impossible-to-find, edge-case data corruption bug occurs after millions of randomized operations, developers can use that exact seed to replay the execution trace identical to how it failed. [1, 3]
- Fault Injection: Instead of just making normal writes, the simulator intentionally drops network packets, randomly pauses threads, delays storage commits, and shuts down simulated database nodes mid-write to stress-test the MVCC concurrent engine. [11, 12, 13]
- Dual Protection with Antithesis: Because a custom in-house simulator might have its own logical blind spots, Turso also pairs its DST framework with [Antithesis](https://antithesis.com/). Antithesis is a deterministic hypervisor that runs the compiled database in a virtualized environment to inject low-level OS/hardware faults and catch non-simulated I/O bugs. [14, 15, 16]

If you are interested in seeing how they implement this, you can browse the [Turso GitHub Repository](https://github.com/tursodatabase/turso) to look directly at the simulator's logic and the property invariants they test against. [2]
Would you like to explore how to write a basic deterministic state model in Rust, or would you prefer to look deeper into how Turso handles its async I/O loop inside the engine? [11, 17]

[1] [https://journal.resonatehq.io](https://journal.resonatehq.io/p/deterministic-simulation-testing)
[2] [https://github.com](https://github.com/tursodatabase/turso/blob/main/testing/simulator/README.md)
[3] [https://turso.tech](https://turso.tech/blog/a-deep-look-into-our-new-massive-multitenant-architecture)
[4] [https://turso.tech](https://turso.tech/blog/introducing-limbo-a-complete-rewrite-of-sqlite-in-rust)
[5] [https://github.com](https://github.com/tursodatabase/turso)
[6] [https://s2.dev](https://s2.dev/blog/dst)
[7] [https://github.com](https://github.com/tursodatabase/turso/blob/main/testing/simulator/README.md)
[8] [https://mohittalniya.medium.com](https://mohittalniya.medium.com/inside-the-vllm-semantic-router-a-deep-dive-into-intelligent-llm-routing-3e6b42e2a01d)
[9] [https://www.youtube.com](https://www.youtube.com/watch?v=E__g-Mck62U)
[10] [https://www.youtube.com](https://www.youtube.com/watch?v=MV0TNq6G5rk)
[11] [https://dev.to](https://dev.to/arshtechpro/turso-a-rust-rewrite-of-sqlite-setup-guide-and-whether-its-worth-your-time-16lk)
[12] [https://docs.turso.tech](https://docs.turso.tech/cloud/durability)
[13] [https://pierrezemb.fr](https://pierrezemb.fr/posts/learn-about-dst/)
[14] [https://turso.tech](https://turso.tech/blog/turso-the-next-evolution-of-sqlite)
[15] [https://turso.tech](https://turso.tech/blog/turso-the-next-evolution-of-sqlite)
[16] [https://github.com](https://github.com/tursodatabase/limbo/blob/main/CONTRIBUTING.md)
[17] [https://thenewstack.io](https://thenewstack.io/why-we-created-turso-a-rust-based-rewrite-of-sqlite/)
