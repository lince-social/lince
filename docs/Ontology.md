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

All of the following is built; §11 holds the mechanics.

Contacts live in `organ_contact`: `trust` (unknown/known/blocked), numeric
`proximity` (local-only, never exported), and independent `sync_out`/`sync_in`
policy. `blocked` is terminal everywhere — import, discovery, export and
delivery all reject. Introduction exchanges identity + public keys, and
adopting stores the remote Organ under its own uid; Organ identity is
transport/trust, not permission for one Person to act or sign for another.
Sync carries visibility-gated op batches through a durable outbox with retry;
imports verify hash-chain + signature, rejects go to quarantine verbatim while
valid items still apply, import is idempotent by op identity and uid, and
quantity deltas commute. Protein selects Records by origin Organ
(`organ_eq`/`organ_in`).

File Sync mirrors selected Records to Markdown files through the
`lince.file_sync` extension (`enabled`, `path`); disk edits return through the
normal `EditRecordText` action. Disk wins on a same-tick conflict, and a
missing file deletes its Record only after two consecutive misses — a debounce
against atomic editor saves. A live config supervisor
(`engine::file_sync::spawn_supervisor`) starts and stops watchers when config
toggles, reacting to `SetExtension` facts on the bus rather than requiring a
reboot; boot seeds via the same reconcile pass.

The Organ interface lists `kind = organ` Records, edits File Sync config, and
— folded into the same sand as a friends-list panel — edits contact trust and
proximity, with block/unblock as `trust: "blocked"`/`"known"`
(`set-contact-trust`/`set-contact-proximity` Actions, `contact` Protein
include on `organ_contact`). Its scope is trust/proximity; sync policy
(`sync_out`/`sync_in`) and quarantine inspection stay out of that component.
It has since grown Profile, Devices and Discovery panels — §11.

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
text, scalar columns, extension namespaces) lives in one
**Loro doc per record** — vendored, MIT, pinned — where per-key LWW maps and
text CRDTs make different-field edits and concurrent text
merge correctly *by construction*; SQLite always
holds the materialized current values, so queries never touch Loro and the
data outlives the dependency. **The ledger** (facts, assertions, record
tombstones) stays homebrew: signed, hash-chained, individually inspectable
rows with HLC ordering — accountability semantics no CRDT register can
give. Quantity is a fold of facts and must never become a CRDT value: LWW
would silently drop one of two concurrent payments. Deletes are tombstone
ops that replicate like any write, which is what makes catch-up unable to
resurrect deleted data.

**Current security posture, stated up front so nobody has to infer it:**
encrypted, authenticated and FORWARD-SECRET in transit, via iroh's QUIC/TLS
1.3 — and **PLAINTEXT AT REST** on both ends, including thread bodies. That
second half is a deliberate decision (2026-08-03), not an oversight; the
reasoning and the exact scope it would take to revive are in §11b.

---

## 11a. What is built

The goal this converged on, stated once so the rest reads against it: **two
Lince instances find each other on a network, talk, exchange keys, become
contacts, and sync exactly what each chooses to share — with an
internet-reachable Cell whose app_users edit Records live.** Most of that
now runs. The stage numbering that organised this work (Part 1 / Part 2,
then "stages 1–6") is retired: it was scaffolding for a conversation, and
what survives it is the subsystem prose below.

### Transport: iroh, and the HTTP peer path that it replaced

Decided 2026-08-02, landed 2026-08-03. Two Organs reach each other over
iroh: QUIC with an ed25519 keypair as the node identity, address resolution
by DNS/pkarr/mDNS, hole punching, and relay fallback when hole punching
fails. The reason was reach, not elegance — the old `lan_discovery.rs` was
UDP multicast, so it could not find a peer off the local segment at any
amount of polish, and nothing in the HTTP design traversed a NAT.

**Node key ≠ identity key, from day one, even on a single-Cell Organ.**
They answer different questions. The **node key** authenticates a LIVE
CONNECTION: its public half is the NodeId, which is the address, it is used
in the QUIC/TLS handshake, and it answers "is the endpoint I just dialed
really that endpoint" — per Cell, generated at first boot, never leaves the
device, cheap to rotate. The **identity key** authenticates DURABLE BYTES:
it signs op batches, facts, and the Cell roster, and answers "who wrote
this" a year later from a backup with no connection in sight — per Organ,
published, held by `identity_key` + `trust.rs`. `SecretKey::from_bytes(&[u8;
32])` would have let the Organ keypair BE the node identity byte for byte,
and that was deliberately declined: the node key is on the network
constantly and lives on every device including the least trusted one, so
fusing them would mean compromising any running Cell forges that Organ's
history forever. Split, a stolen node key costs one connection identity and
the attacker still cannot sign a single op. Separating later would have
made every published key everyone already holds wrong.

What gets shared is still ONE string: the NodeId. On first connect the peer
sends its Organ identity key and a signature binding that key to this
NodeId, so both halves arrive over an already-authenticated connection and
the binding is proven, not assumed.

The HTTP peer path is **deleted, not merely superseded**: `/organ/introduction`,
`/organ/inbox`, `/organ/ops`, the whole `peer_auth` layer, and with them
`request_signing_payload` / `response_signing_payload` / `timestamp_fresh` /
`verify_peer_signature` / `sign_peer_request` / `sign_peer_response` /
`verify_peer_response` and the 120s replay window. All of it existed to
establish what an iroh handshake establishes for free — `remote_node_id()`
gives what `verify_signed_request` used to return. `peers.rs` now holds only
the pairing verification code. **Payload signing was NOT replaced**: op-batch
signatures are durable provenance that must survive store-and-forward through
a relaying Organ, where transport auth proves nothing about the origin.
Transport auth answers "who is on this socket"; payload signing answers "who
wrote this op." Only the first is iroh's. The signing payload carries a
domain prefix (`lince/peer/1\n`) — colliding with TLS 1.3 CertificateVerify
was already impossible, so this is safe-by-design replacing safe-by-luck.

Three ALPNs, with an accept policy that is **default closed**
(`lince.discovery.accept_unknown = false`):

```
lince/sync/1     known contacts only — op batches, catch-up, roster
lince/thread/1   unknown NodeIds — Introduction, invites, enrolment
lince/live/1     live sessions relayed to a host Cell
```

`known` means `trust='known'`, not merely "a contact row exists" — a row with
`trust='unknown'` is someone added but not vetted, and it gets the thread door
like any stranger. `blocked` is closed on BOTH ALPNs, checked before the ALPN
split, so a blocked Organ never reaches a handler at all. **`add_contact`
defaults to `unknown`** (fixed 2026-08-04; it wrote `known`, which quietly
undid the gate from the other side — every path that recorded an address
opened the sync door). Callers that HAVE made the decision — pairing, adopting
a code, reconciling an introduction — say so with `set_trust`.

Frames are bounded: `MAX_FRAMES_PER_CONNECTION` caps frame count as well as
size, and a batch/peer mismatch answers `WireResponse::Refused { code }` — a
security event, distinguishable from an ordinary import failure. `tracing` is
a real dependency of the engine crate.

Discovery is iroh's, through `iroh-mdns-address-lookup` (pinned `=0.4.0`; mDNS
left iroh core at 1.0), subscribed on its event stream into `wire::Nearby`.
Two decisions worth keeping: a **Lince-specific mDNS service name** (`lince`,
not iroh's default `irohv1`), or the nearby list shows every unrelated iroh
application on the network as an Organ; and **`organ_uid` is no longer
broadcast** — the old announce shouted the Organ uid across the LAN so a
receiver could tell whether it was a known contact, the NodeId now answers
that through `organ_contact.node_id`, and the uid arrives at pairing over an
authenticated connection. Strictly less is published and nothing is lost. The
display name still rides iroh `UserData` as the untrusted label it always was.
Changing `lince.discovery` REBINDS the endpoint live (`wire_supervisor`,
following the File Sync live-supervisor pattern) rather than demanding a
reboot, since discovery is an Endpoint builder option fixed at construction;
the node key is reloaded from the same file so the NodeId survives the rebind
— a Cell whose NodeId changed when a setting was toggled would strand every
contact who had saved it. The Organ sand has a Discovery panel on the LOCAL
Organ for both switches, and it states the cost of each rather than presenting
them as neutral.

**`nearby` is a Protein source** (2026-08-04), not an Action plus a client
poll. The earlier shape was working around the sand boundary instead of
extending it, and it made discovery update on a fixed client timer whether or
not anything had changed. `Source::Nearby` is gated exactly like `Decision` (a
remote subject gets an empty list, because who is physically near you is the
last thing that should travel). Process state reaches Protein through
`Context` — a struct passed to `execute_for_with_context`, following the
`installed_signer_actor` precedent of handing in what no query could find,
rather than hanging network state off `Store`. `NearbyPeer` lives in the
nucleus so both sides can name it without the engine/Protein dependency
inverting. `is_ephemeral` is the complement of `affects`: the driver arms a 3s
tick ONLY for sessions holding such a subscription, and the session pushes an
Update only when the rows differ from what it last sent. `Action::NearbyOrgans`
and the client poll are deleted.

Licensing: iroh is MIT OR Apache-2.0, Lince is MIT — taken under MIT. (The
workspace Cargo.toml said `GPL-3.0-or-later` until 2026-08-03; the `LICENSE`
file has always been MIT and the manifest was simply never updated after the
switch. Fixed.) Unlike loro-wasm, iroh is an ordinary crates.io dependency:
pin it and carry its MIT text in the licenses dir.

### Peers: identity is a key, never an address

An Organ's identity is its keypair. IPs, ports, and hostnames are hints that
may rot or be taken over by a stranger; nothing flows on any connection until
the far side proves possession of the private key, and under iroh that proof
is the handshake itself. Dialing a NodeId reaches that keypair or nothing.

`GET /organ/nearby` survives as a route, re-backed off `wire::Nearby`,
returning a short NodeId fingerprint per row. `POST /organ/pair` takes a
`node_id`, dials over `lince/thread/1`, and binds the reached NodeId to the
adopted contact. The LAN-sighting candidate url is gone from `sync_runner` —
iroh resolves a NodeId to a transport path, not an HTTP base url.

Introduction still exists as a payload rather than a challenge: it exchanges
identity + public keys, and `adopt_introduction` registers the contact under
the REMOTE organ's own uid and stores its keys so its signed facts verify.
Connecting IS the proof; the exchange no longer establishes anything the
connection did not already establish.

Op-batch push carries op batches ONLY (`WireOp`/`OpBatch`; facts ride hydrated
inside their op). Enqueue happens at op-append time into the bounded outbox;
`drain_outbox` builds one batch per contact and retries (failures stay
queued). The pre-op Package format is deleted — no migration, no back-compat.
Import hardening: every incoming fact passes its hash-chain step and
signature; rejects land verbatim in `sync_quarantine` with a reason, the rest
of the batch still applies. Import is idempotent by op identity AND fact uid;
quantity sync is conflict-free by construction (deltas commute). Concept ops
ride the same log; assertions arriving before their concepts create stub rows
resolved by the concept's own op (§5 lineage travels as ordinary `concept`
ops). Discovery of promises: `GET /organ/open-promises` exports OPEN promises
a subject may see; `refresh_discovery` upserts them into the local cache,
stamping proximity from OUR contact row (proximity never travels outward).
Every record carries `organ_uid` (origin, stamped on creation, preserved
through relay hops); Protein's `organ_eq`/`organ_in` select "every record
belonging to organ X."

### First contact: QR, paste, and what dialing actually proves

First-contact key exchange, ranked strongest first, because only the
ACQUISITION of a NodeId is ever at risk — once held, connections to it cannot
be intercepted:

1. **QR code in person.** The local Organ renders its NodeId as a QR, the
   other scans it. The visual channel cannot be relayed and you can see who
   you are handing it to. QR also solves the blocked-mDNS case (guest wifi,
   hotels, corporate): embed NodeId AND current addresses and no discovery
   mechanism is needed at all for an in-person exchange.
2. **Paste it into a messaging app you already trust** (Signal, etc.).
   Equally strong: that channel is already authenticated to that human.
3. Discovery + conversation alone — weakest; a live relay passes it.
   Acceptable only because (1) and (2) cover the real flows.

**Pasted-contact uid reconciliation** was a BUG, not a refinement, and is
fixed (2026-08-04). `add-known-organ` never talks to anyone, so it could not
learn the other side's real Organ uid and invented `o-<node_id>`. But the wire
authenticates an inbound peer as `contact.record_uid` — that invented uid —
while their op batches carry their ACTUAL uid, so `batch_peer_mismatch`
refused every push: a contact added by paste could be reached and could never
sync. Silent, and only two people trying it would notice.
`organ_contact.pending_introduction` (migration 0045) marks a row added from a
code — an explicit column, because sniffing the derived shape would silently
replace a real uid that happened to look like one. `Wire::reconcile_pending`
runs at the top of every sync pass, BEFORE `push_outbox`: it dials each
pending contact on `lince/thread/1` (not the sync ALPN — the pending peer
holds no row for us, so their sync door is shut to this Cell by design), takes
their Introduction, retires the placeholder, and adopts them under the uid
they declare. Deleting the placeholder is purely local: `add_contact` writes
its Record with plain SQL rather than through the Record write path, so
nothing was ever logged to the op log or sent to any peer. The security
property is the root key — it was trust-on-first-use'd from the code, and that
is the only thing adding by code verifies, so if the Organ answering presents
a DIFFERENT root key this is not the peer the code was for: reconciliation
refuses, the row stays pending, and a human looks at it. Offline adding still
works: the row waits, and the surface says "not connected yet" rather than
showing it as an ordinary contact.

**Say what dialing actually proves.** Two paths end at `trust='known'`:
paste/scan a code (no network, trust-on-first-use on the root key), and dial
plus fetch an Introduction. The second does NOT prove more about WHO someone
is — if you were handed the wrong NodeId, the handshake authenticates the
wrong person flawlessly. What dialing adds is their true identity fields and
proof of reachability. Both rest on where the code came from, and the UI says
so rather than implying that having connected constitutes verification.

**Sand `media_capture` capability with backend QR decode** (2026-08-04).
`engine::pairing::decode_qr` (rqrr + `image`, restricted to the PNG/JPEG a
browser canvas actually emits) sits beside `qr_svg`; `POST /organ/qr-decode`
takes one frame and returns the decoded text, with "no code in this frame" as
a 200 carrying `null` — a scan loop must not read failures to know it is still
looking. Decoding belongs in the backend for the same reason rendering does
(no QR library under the sand CSP) plus one more: a decoder emits a PAIRING
CODE, so it is security-adjacent enough to want in one audited place instead
of in every sand that scans. The capability's shape is the part that matters:
the CHROME owns the camera, not the sand. A sand calls `H.scanCode()` and
receives a STRING; the host opens the stream, shows the preview, posts frames
to the decoder, and stops the tracks on every exit path. So `media_capture`
grants "read a code the user pointed at", never "watch the room" — enforced in
`widget-bridge.js`, gated on the card's declared permission plus a check that
the message came from that card's real iframe, exactly as `terminal_session`
is. Scanning FILLS THE FIELD and stops; a scan is a strong story about where a
code came from, but it is still a story, so the human presses Add. Two data
paths under one permission and they are not alike: QR needs a SINGLE FRAME
sent to the backend; audio/video calls in threads need a LIVE STREAM
peer-to-peer, which never routes through the backend at all — that stream path
belongs to the Communication sand (F2), not here.

Known gap, not fixed: `getUserMedia` needs a secure context, so scanning does
not work over plain HTTP to a LAN hostname — which is how a second device
reaches this Cell. The chrome says so specifically instead of failing as "no
camera". A hostname/TLS story is what actually closes it.

### The identity floor: root offline, operational keys online

BUILT 2026-08-03 (migrations 0043/0044, `store::roster`, `engine::roster`,
Organ sand Profile + Devices panels).

The problem, stated without flinching: if an attacker obtains the Organ
identity private key, they can sign a roster adding their own device, sign ops
as the owner, and BE that Organ to everyone holding the public key. There is
no central authority to report it to. Worse, the attacker can do exactly what
the victim can — both can announce "I was compromised, here is my new key" —
so contacts face a claim and a counter-claim with no referee. Any mechanism
that lets the owner recover is a mechanism the attacker can also attempt to
walk. No design removes that; designs only change who has to be fooled.

**Make the catastrophic case rare.** The Organ **root key** signs two things
only — the Cell roster and key successions — and lives OFFLINE: a hardware
token, or a printed or drawer-kept drive. It is on no running Cell, not the
laptop and not the VPS; using it is a deliberate, occasional act. Each Cell
holds an **operational key** used for everything routine, so compromise of a
device is compromise of one revocable credential, not of the identity. This is
the standard shape (TLS roots and intermediates, SSH CAs, PGP primary keys
with subkeys, Signal identity keys with prekeys). It also largely answers the
provider-access worry: a VPS snapshot yields an operational key the owner can
revoke, not the identity — the thing that could not be defended is no longer
there to steal. It retires the earlier "identity-signing Cells" flag, and with
it the whole laptop-versus-VPS argument about which machine may hold identity
material: once no running Cell holds the root, that comparison stops being
about the identity at all.

**The signed roster IS the certificate.** There is no separate certificate
object to define, sign, store, ship and validate. A roster entry already names
a Cell, its operational key and its NodeId, and the roster already carries a
monotonic version and an expiry; being listed in the current root-signed
roster IS what certifies an operational key, and being dropped from the next
one is what revokes it. One signed blob does membership, certification,
versioning and expiry together.

example:

```
roster (root-signed, monotonic version, not-after expiry)
  version:  7
  organ:    o-eduardo
  not_after: 2026-09-03T00:00:00Z
  members:
    - cell: c-laptop  op_key: ed25519:…  node_id: …  label: "laptop"
    - cell: c-phone   op_key: ed25519:…  node_id: …  label: "phone"
    - cell: c-vps     op_key: ed25519:…  node_id: …  label: "vps"  always_on: true
  sig: <root key over the above>

identity_succession  (old_key, new_key)   -- the chain a contact walks
```

Three things the implementation settled, beyond the design:

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
accepted in the same pass that learns it is dead. Revocation itself is a
roster version bump plus republish (monotonic counter, so an old roster cannot
be replayed to re-add a stolen device), and under root-offline that ALWAYS
requires the root — that is the correct cost and it is the point of the split,
but it must be stated plainly rather than discovered during a theft: revoking
a stolen device means going to the drawer. Two things keep that from being
painful. Roster entry EXPIRY means a stolen Cell loses authority on its own
even if the owner never reaches the root — self-limiting credentials doing the
work that urgency would otherwise have to. And the pre-signed revocation
certificate is meant to live offline WITH the root, so the drawer trip yields
both acts at once (see What is left).

Enrolment (migration 0044) issues a single-use, short-lived token —
`engine::roster::issue_enrolment_token` returns the plaintext once — served on
`lince/thread/1`, because a Cell being enrolled is by definition not yet a
member and cannot reach the sync door. Short-lived and single-use because this
grants membership in your identity, which is strictly more than a contact QR
grants.

Compatibility floor, in place because every one of these is cheap now and
brutal to retrofit once keys are in other people's hands: ALPN strings are
versioned (`lince/sync/1`, `lince/thread/1`, `lince/live/1`) so a protocol
change bumps to `/2` and both are served during transition; roster versioning
uses a monotonic counter so a contact accepts only a roster NEWER than the one
it holds; roster entries carry a not-after expiry; iroh is pinned to an exact
version, as Loro already is, because its API has broken across releases and an
unpinned bump is a silent protocol change between two Cells on different
builds; and the key-succession table (`identity_succession`, `record_succession`
/ `successions`) stores the chain even though the UI for it is still open.

#### Cell Record vs Organ Record: device vs profile

Each Cell's database holds TWO `kind=organ` Records, and this is where the
bugs will live:

- The **Cell Record** — this running instance, this laptop. What
  `organs::local()` returns (fixed slug). Its uid is stamped as
  `sync_op.actor_organ` on every op written here, which keeps ops from the
  laptop and the phone distinguishable and the `(actor_organ, hlc)` uniqueness
  intact.
- The **Organ Record** — the person across all their devices. Holds the
  published identity key and the Cell roster. What `record.organ_uid` points
  at, so a Record reads as coming from *you*, not from *your laptop*.

The op-log consequence dictates the shape: `idx_sync_op_identity` is UNIQUE on
`(actor_organ, hlc)` and an HLC is unique per actor, so the index IS the op uid
AND the import idempotency key. Three Cells appending under one shared uid
would be three independent `hlc::next()` clocks in one uniqueness domain — two
Cells could mint the same identity for different ops, and because import
dedupes on the same key, the collision does not merely fail a constraint, it
can swallow a remote op as already-seen. So Cells MUST NOT share a uid. With
per-Cell uids, Cells are ordinary full-trust contacts of each other over the
existing op log — no new sync path, no new machinery — so "edited on the phone
all day, walk in the door, laptop converges" is just sync, with the VPS as the
Cell that is always up so the other two never need be online at the same
moment.

Why NOT one shared node key across devices: iroh publishes a discovery record
mapping NodeId → current addresses, so two endpoints with the same NodeId each
overwrite the other's and a dialer reaches whichever wrote last. Worse, it
forces the Organ private key onto the VPS, where compromise is compromise of
the identity itself, unrevocable in isolation. With per-Cell keys a stolen VPS
costs one roster entry.

### Threads: reaching someone before you trust them

Settled 2026-08-03 after two reframes. A thread is how a stranger becomes a
contact — trust is established by talking, and only then does `trust` go to
`known`.

**A thread is not a subsystem. It is Records synced with exactly one peer.**
Sharing IS granting that peer sync access, and the same act shares any Record
with any contact — "I choose what to sync with whom, they agree, only we see
it". That axis is **individual replica**, as distinct from the whole-Organ
`sync_out`/`sync_in` feed. It is a genuinely new enforcement point, not "the
§12 visibility gate with a narrower selector" — §12 asks whether a contact may
see the feed at all, and this asks which Records leave the Cell for whom. Both
run; neither substitutes for the other.

Why this is a simplification rather than a new feature: it deletes the special
message-delivery path, reuses the op log and the outbox, and makes messaging a
*consequence* of sharing. Discovery demotes to ergonomics for creating a
thread and handing someone access to it. It is also deliberately orthogonal to
transport — a thread works over iroh or over signed HTTP.

The synced unit is a **Record per relationship**, and MANY THREADS live inside
it. Three levels, one grant:

example:

```
Record (kind='conversation', shared with one contact)   <- replica_root, the grant
  └─ threads   (Records, replica_root inherited)
       └─ messages (Records, replica_root inherited, ordered by created_hlc)

a `with` Assertion binds the conversation to the contact's Organ Record
```

Adding a second thread later — a different topic with the same person — needs
no new grant, no new pairing, no new sync setup, because it is inside a Record
already being synced. That is the whole reason to nest rather than make every
thread its own Record. The `with` Assertion is what §1/§3 already allow, so
there is still no new ACL table.

**The grant cascades via `replica_root`, a denormalized column** (migration
0042, `store::replica`, `engine::threads`, BUILT 2026-08-03). A grant per
message Record would be absurd and racy — the grant row would have to exist
before the peer could legitimately receive the message it describes — so a
grant on the conversation covers every Record inside it. An earlier draft did
that with a TRANSITIVE ASSERTION TRAVERSAL at three enforcement points; the
insight that replaced it is **traverse once at creation, not on every enqueue,
serve and import**. `record.replica_root TEXT NULL` names the Record whose
grants govern this one; NULL means "rides the ordinary feed", which is every
Record that exists today, so the migration was a no-op backfill. A grant is a
row `(root_record, contact_organ, …)` — one root, many contacts. The three
enforcement points then become the same indexed equality check instead of
three graph traversals that must agree: the outbox enqueue predicate (today
`INSERT…SELECT FROM organ_contact WHERE sync_out=1`, which now also admits "a
grant on this record's `replica_root` covers this contact"), feed-serve
filtering, and the import gate. No depth limit, no cycle handling, no
traversal denial-of-service, and no accidental oversharing through an
Assertion nobody thought of as a containment edge. The `sync_outbox` primary
key `(contact_organ, tbl, uid, field)` was already per-contact-per-record, so
individual replica fit the existing outbox with no schema change — what
changed is the selection, not the shape.

Three guards, without which this is a regression and not a simplification:
`replica_root` is LOCAL-ONLY and IMMUTABLE, never a settable synced field
(otherwise a contact sends a `set` moving a Record between roots and re-scopes
what gets shared with third parties); on import the root comes from the
CHANNEL, not the payload (ops arrive on a stream already scoped to a grant);
and ONE root per Record (a Record cannot belong to two roots; a root may be
granted to many contacts).

Two constraints the implementation forced, both worth keeping. **Stamped at
CREATION, not at grant time** — the draft had a walk that adopted an arbitrary
existing Record into a root, which opens a window: the Record already logged
ops with `replica_root = NULL`, those ops already rode the general feed, and
any `sync_in` contact gets them on their next catch-up. Adoption is therefore
deferred (see What is left). Creation-time inheritance covers conversation →
thread → message, which is the shape that exists; it is also what makes the
immutability claim true, and the denormalized `sync_op.replica_root` is only
safe because of it. **Import enforces immutability, it does not merely apply
the stamp** — three cases on arrival: the Record exists in THIS root → apply;
exists in a different root or on the general feed → quarantine, because that
is a grantee re-scoping one of your uids through a channel that does not
govern it; does not exist → create inside the channel root. Two bugs the tests
caught, recorded because both were silent: imported Records were not being
stamped at all (the receiver's copy would have ridden the RECEIVER's general
feed), and an Assertion's predicate Concept lives on the general feed, so a
grant-channel import hit a foreign-key failure that rejected the whole
conversation — the importer now inserts a Concept stub, since depending on
general-feed sync would break exactly the case that matters, a contact granted
one conversation and no feed at all.

**Messaging is NOT collab**, settled 2026-08-03, retiring the `threads`-Loro-Map
plan written a day earlier. Sending a message is appending a Record to a
thread; it is not two people typing into one string. So messages are ordinary
Records synced as ordinary `set` ops through individual replica, ordered by
HLC, and no Loro doc is involved. "Ordered by HLC" is `record.created_hlc`
(migration 0047, added 2026-08-04 when the conversation view was found
ordering by `created_at` — a local wall clock, so a peer whose clock is slow
sorts into the past forever and it reads as a rendering bug rather than the
clock problem it is). Denormalized onto the row for the same reason
`replica_root` is: written once at creation, never changed, so a copy cannot
drift. One stamp per record rather than per op, because `log_local` mints an
HLC per FIELD and "the record's HLC" would otherwise be several values. The
import path carries the ORIGIN's stamp — a fresh local one would silently make
it arrival order, the same bug by another route. Pinned by a test that skews
the receiver's clock backwards and asserts the order holds. Concurrent sends
do not conflict — two different Records, both arrive, both display. Editing a
sent message is LWW on that Record.

What that reframe deleted, all of it invented and none of it needed:
kind-dependent record-doc layout (`collab.rs` keeps its `doc.getText("body")`
assumption untouched); `record_doc.snapshot` as a place conversations leak,
since a thread has no doc; and unbounded doc growth for chat, and with it the
urgency behind shallow snapshots. A thread is rows, not a document. Efficiency
follows for free: rendering a thread is a SELECT of the last N messages on an
index over (thread, hlc), with older pages fetched on scroll — a ten-year
conversation costs the same to open as a new one.

Collab's role in messaging is exactly this small and no larger: if both
parties happen to OPEN THE SAME message Record, its body behaves like any
other collab-edited body — Loro merge, cursors, presence. Everything else
about a conversation is normal sync.

**Consequence to accept:** one Record, one grant, so ALL threads inside are
shared with that contact — you cannot share one thread and withhold another
from the same person. That is the right default for a per-relationship Record,
and the escape hatch if it is ever wrong is to put the private topic in a
different Record.

**Revocation level: at the conversation**, i.e. at the individually-synced
Record. The nested shape moved the grant one level above the thread, so
per-thread revocation was no longer expressible without giving up the property
that made nesting attractive. Resolved in favour of the simple workflow:
**reach closes when either party blocks the other, or when either party
deletes the individually-synced Record.** Deleting a single thread inside it
is then just deleting a Record, with no reach consequence — the conversation
is the unit of relationship and the unit of revocation. Symmetric by
construction: both sides hold the same switch and neither needs the other's
cooperation. Blocking (`trust='blocked'`, terminal per §2) closes everything
with that Organ; deleting the shared Record closes just that conversation. Two
switches, both local, both guaranteed — nothing here is a request the remote
may decline. Silencing a conversation does not unfriend: a known contact with
`sync_out`/`sync_in` still gets `lince/sync/1` and still syncs.

Reach is the individual-replica GRANT, and revoking it is what stops inbound
messages — refused at the import gate, not filtered in the UI. No time limits,
no expiring grants. **Deletion = local removal + explicit revocation of the
grant**, and conflating the two breaks in both directions: the peer keeps
pushing `set` ops for that uid, so a purely local delete leaves them writing
to a record that is gone (import must DROP those ops, never resurrect the
record); and `tombstone` is a synced op kind, so if deletion emitted one it
would delete THEIR copy of the conversation too, contradicting "both parties
keep a copy." Their copy survives, their ops stop being accepted, and nothing
is reached into on their Cell — exactly §12's honest split between revoke
(hard, local, guaranteed) and forget (a request the remote may honour).

Not Karma grants (K5.1). Those delegate one PERSON's authority to another and
are signed by the Person's key. Thread reach is an ORGAN-level question — who
may open a stream — resolved before any Action exists. Different axis,
different layer; do not fold them together.

**Invites** (migration 0046, DONE 2026-08-04). An invite is not a thread:
`kind='thread_invite'`, one pending per Organ, so a deleted thread cannot
become a spam channel. Two changes from the sketch. **Not an Assertion** — the
sender lives in a `thread_invite` table with `from_organ` UNIQUE, and that
constraint IS the one-pending rule, enforced in SQL rather than by a
check-then-insert, because an Organ is several Cells and a contact's laptop
and VPS can offer at the same moment. A `lince.invite` extension mirrors it
for display, the way `lince.pairing` mirrors the invite code onto the Organ
record. **Written with plain SQL, never `records::create`** — the Record write
path logs an op and enqueues it to every known contact, so creating an invite
the ordinary way would push "Bea is asking to talk to me" to everyone you
know; `organs::add_contact` already had this shape for the same reason, and a
test asserts the op log and the outbox both stay untouched. The grant row and
the invite are kept SEPARATE: `replica_grant` is the mechanism (it decides
whether ops are accepted), the invite is the surface (it is what a person
answers) — collapsing them would mean an offer could not be shown without
already having decided something. Two exits only, and no "dismiss": accepting
grants, declining REVOKES. Clearing an invite without answering would leave
the sender waiting on a reply that never comes while the one-per-Organ slot
stayed occupied, so they could not ask again either; declining frees both. A
repeat offer gets the same answer as a first one — telling a sender their
offer was dropped would tell them whether the last was declined or merely
unanswered, which is not theirs to know. `blocked` needed no new code:
`serve_connection` closes on it before the ALPN split.

**The board surface was restored 2026-08-04.** A first-contact request is
attention that must remain visible when the Conversations sand is closed, so
pending `thread_invite` Records are projected through `/host/notifications`. A
five-second LynxUI-style toast announces a new one, and the active
notification button keeps the board's base rail open until it is answered.
Accept/decline acknowledges the sender over the thread ALPN; accept then pulls
only the granted conversation root and opens it in the Record sand. The
Conversations sand mirrors the same queue, but is not the only place an invite
can be discovered. What is rendered is the Organ uid the CONNECTION proved —
there is no claimed display name on an invite at all; the sketch would have
shown one "marked as untrusted", and not having it is simpler and safer.

**Offline delivery** was VERIFIED 2026-08-04 and already worked — no announce
protocol was needed. `sync_outbox` holds the queued rows, a failed pass leaves
them queued, and the next pass that connects delivers them. The sketch
proposed "a Cell coming online announces itself to the contacts it KNOWS,
which wakes their queues"; that would only reduce latency from "their next
interval" to "immediately", because both sides dial anyway — the sender
retries in `push_outbox` and the receiver pulls in `pull_catch_up`. Pinned by
a test that sends while nobody is serving, asserts nothing arrived, then
starts serving and asserts it does. **The retry IS the delivery**, and if that
ever stops being true this breaks silently in the one case it exists for.
Opting out of being pinged is just moving that contact to `unknown` — no
separate setting.

Encryption of threads, revised 2026-08-02 and BOTH simpler and stronger than
the plan it replaced: the earlier static-X25519-DH-seal-each-body design had
no forward secrecy, so one long-term key stolen in two years would decrypt
every message ever recorded. Instead, threads are direct Cell-to-Cell over
iroh, and **iroh's QUIC/TLS 1.3 already provides authenticated, encrypted,
FORWARD-SECRET transport** — ephemeral session keys, discarded after use.
Message-layer sealing only buys something when a message passes through a
THIRD Organ; keep threads direct and it buys nothing while costing forward
secrecy. The always-on VPS Cell holds messages when a peer is offline, but
that Cell is the user's OWN Organ, not a third party. Consequence: no separate
X25519 key is needed at all, and no ed25519→montgomery conversion — one less
published key, one less primitive to get wrong. What was traded is stated
plainly under Decided against.

### Live mode

`live` is an in-memory session against a REMOTE Organ: zero local rows, the
remote is authoritative, and the sand renders data streamed into memory that
is never persisted locally. Logging into another Lince and editing its Records
— with full CRDT on text — IS live mode; "live login" is retired as a separate
phrase because it is the same thing. `replica` is a local persistent copy: ops
sync both ways, both sides store, checkpoints track what the other has seen.
An earlier pass redefined `live` as "hold the iroh connection open"; that was
a drift and is withdrawn — connection pinning is a transport tactic, not a
mode. The mode is about WHERE THE DATA LIVES.

**Live sessions ride iroh, on `lince/live/1`** (revised 2026-08-04, and the
revision removed a dependency rather than adding one). The earlier text said
live mode "needs a hostname, certs and a reverse proxy" because "browsers
speak HTTPS, not QUIC-to-a-NodeId" — the first half does not follow from the
second, because the browser never has to be the thing that crosses the
network. A guest's browser opens an ordinary websocket to its OWN Cell on
localhost — no certificate, no hostname, nothing to configure — and that Cell
relays the frames to the host Cell over iroh (`/live/{organ}/connect`,
`transport::live`). The only leg crossing a network is authenticated by KEY
rather than address, so there is no hostname to go stale and no certificate
bound to one, and QUIC migrates the path under a connection that stays open.
Change network mid-sentence and the session continues — which was the actual
requirement, and which TLS to a hostname would NOT have satisfied. `Session`
needed no changes: it was already transport-agnostic, so the QUIC driver is a
second driver of the same shape as `ws.rs`. `MAX_FRAMES_PER_CONNECTION`
deliberately does NOT apply — a live connection is handed off whole, because a
4096-frame cap would hang up on someone a few thousand keystrokes into a
sentence.

**There is ONE human reference: the Person** (migration 0050, 2026-08-07).
`app_user` and `Person` used to be two identities for one human, joined by
`app_user_person` — whose `user_id` was PRIMARY KEY and whose `person_uid` was
UNIQUE. Unique on both sides is a bijection, so the split was never modelling
two things. It also cost real behaviour: a transport session's `subject` came
to mean "numeric app user id" on the websocket driver and "Person uid" on the
iroh live driver, and neither value was correct for both consumers — a Person
uid works for `visible_targets` and fails `begin_action_intent_session`, a
numeric id the reverse. **That is why a live guest could read but never write.**
Migration 0011's stated reason (the split "prevents clients from claiming an
arbitrary Person uid") does not hold: what prevents that is the server
resolving identity from the authenticated session and never reading it from the
client frame, which is what the code did before and still does.

So a Person IS the human, and `person_credential` is merely a way to prove you
are one of them over HTTP — username, password hash, role. It is LOCAL AND
NEVER SYNCED: Person records travel to contacts, password hashes must not,
which is the whole reason it is a side table rather than columns on the record.
`organ_login` was already built this way, keyed straight to a Person; this
finishes the move it started. Consequences worth naming: `subject`, `actor`,
the JWT `sub` and `visible_targets`' argument are now all the same kind of
value; `AssignUserPerson` and the `user:assign_person` permission are deleted
because the state they configured is unrepresentable; and
`begin_action_intent_session` now requires only that the Person EXISTS, not
that they hold a password — a live guest has no credential by design, which is
exactly the case that was broken.

**You can see what you made** (`visible_targets`, 2026-08-07). Every read is
gated by that set, and it held only explicit grants plus public ones — so
turning auth on made a Cell look EMPTY to the very person using it: a record
created a second ago and shared with nobody was invisible to its own author.
Nobody chose that; it was the absence of a choice, and `--server` making login
mandatory turned it from a corner into the first thing you would hit. Creators
are now included intrinsically, the same way a Transfer's own parties always
were. Records committed with NO actor (made while the Cell ran with auth off)
deliberately stay invisible: on a personal Cell that later enables auth, showing
them is obviously right; on a shared one it would disclose everything predating
the first account — a real policy question, not one to answer silently.

**The board can now BE the guest** (2026-08-07). `/live/{organ}/connect` had
existed for a while as a route with no caller anywhere, while `transport.js`
hardcoded `window.location.host` — so the host half worked and nobody could
reach it. The mechanism is one line of routing rather than a second client:
the remote Cell speaks EXACTLY the frames our own does, so repointing the
board's single socket at the relay carries every subscription, Action, lane and
collab doc with it. `setLiveOrgan(uid)` closes and reconnects; consumers replay
through the same `onOpen` path a dropped link already used.

It is ONE value for the whole board, deliberately. Live mode means working
inside someone else's Cell, and a board where some panels were theirs and some
were yours would be a trap rather than a feature — including the Organ sand
itself, whose contact list becomes theirs, which is why the leave control reads
board-host state rather than anything that arrived over the wire. Two failures
were worth guarding by name: frames queued for our Cell are dropped on a switch
(flushing them into the next socket would apply a half-sent Action to a
different Organ's store, silently), and the uid is escaped into the path.

**A live guest acts, and the write lands on the host** as the Person their
login named. The guest walks the same path a browser walks — take the server's
challenge, prove possession of an Ed25519 key for its Person, send a signed
envelope — because `Session` refuses a plain `Act` from any authenticated
session. A test asserts the record exists on the HOST afterwards and that its
fact is attributed to the bound Person, not to the host Cell and not to the
guest's Organ.

**A login is a BINDING, not a credential** (`organ_login`, migration 0048). No
password: the handshake already proved which Organ is on the connection, with
a key rather than a secret someone could retype — adding a password would be a
second, weaker way in. What the login decides is which PERSON that Organ acts
as, and every read they make is then gated by that Person's visibility. So
granting one grants a named identity, not a door: a test asserts a fresh login
sees nothing until something is shared with that Person. Requires
`trust='known'` — reaching the thread door is not the same as being allowed
inside. Revoking is one row deleted: local, immediate, not a request the other
side may decline (§12). An ordinary HTTPS login would still need a hostname
and certs; that path is simply no longer the only one, and is not what the
workflow depends on.

The read-permission gate on collab is done: `may_read_record` gates both
`CollabJoin` and `CollabUpdate`, refusing with `collab_not_visible`.
`read`+`write` permission is exactly what enables CRDT editing — collab is not
a separate privilege, it is what having those permissions means. Presence is
done too: cursor position plus who it is, shown only to a viewer allowed to
know. `LaneEvent.from_subject` carries the sender, `spawn_lane_forwarder`
resolves it to a name only when the viewer may read that Person, and the
`record_editor` sand renders "someone" otherwise. The sand never decides whose
name it may show. Presence is ephemeral — lanes, never the op log.

Transport is orthogonal to the mode. A browser reaching a Cell uses HTTPS/WS;
a Cell reaching another Cell uses iroh. Both can serve a live session; neither
changes what `live` means.

**`lince --server` is a Cell that hands nobody a board** (2026-08-07,
`HttpServeMode::ApiOnly`). A Cell run as a service holds data and answers
authenticated clients, but the moment it also serves `/` any stranger who can
reach the port gets a working board backed by the server's own store and can
drop sands onto it. Server mode drops that whole surface — `/`, `/favicon.ico`,
every `/board/*` asset, `/sand/{*path}`, and `/static` + `/host/static` — while
keeping `/api/auth/login`, `/host/transport/ws`, and the `/organ/*` peer
endpoints. The UI routes live in ONE contiguous block behind the mode check, so
a board route added later cannot silently appear on a hardened box, and the
static tree is skipped before the `if static_dir.exists()` fork rather than
inside one arm of it — registered in both arms, it would otherwise survive.

Server mode FORCES local auth on — overriding a configured `enabled = false`,
per invocation and never written back to `lince.toml` — and this is the
load-bearing half rather than a convenience. Persisting it would strand the
operator who merely TRIES the flag: the run can still abort afterwards, and the
toggle would outlive it as a login wall with no account on an ordinary desktop
board. `authenticate_headers` is a no-op when `local_auth_required` is false,
so hiding the board while leaving `/host/transport/ws` reachable would
still leave any network peer an unauthenticated way to act on the store —
strictly worse than doing nothing, because it looks hardened. For the same
reason a server-mode Cell REFUSES TO START when the store has no admin and
there is no terminal to make one on, instead of booting a login wall with zero
accounts and reporting itself healthy; `--initial-admin-password-file` (or
`--initial-admin-password`, visible in `ps`) provisions it non-interactively
through the installer's existing staged-setup channel.

The two auth systems stay separate, and server mode touches only one. Local
users gate the HTTP surface. The iroh ALPNs authenticate CONTACTS by Organ key,
so an inbound live session from a contact arrives on `lince/live/1` and never
passes through HTTP at all — gating those behind local users would break peer
sync, the one reason to run a server. What server mode does drop is
`/live/{organ}/connect`, the GUEST half: it relays a LOCAL BROWSER out to a
contact, and a headless box has no such browser.

### Op log

The `sync_op` table exists, `seq` an AUTOINCREMENT rowid so pruned seqs never
return; the unique index on `(actor_organ, hlc)` IS the op uid and the import
idempotency check — no UUID column.

example:

```
sync_op(seq, tbl, uid, field, kind, value, hlc, actor_organ)
  UNIQUE (actor_organ, hlc)          -- op identity AND import idempotency

hlc (`nucleus::hlc`): one packed 64-bit INTEGER — 48 bits wall-clock ms + 16-bit logical
     counter; native int compare/sort/index. One clock per Cell, stamped on
     every local op, advanced past any imported HLC and past the log's max
     at boot.
```

Every syncable store writer logs its own ops where the SQL happens (record
fields, per-KEY extension diffs so two Cells editing one namespace never
clobber each other, assertion set/tombstone per uid, concept renames, facts as
kind `fact`), skipping silently when no local organ exists yet. Applying a
remote op appends it too (original HLC, local `seq`) so downstream contacts
can relay.

### Reactive deltas

Appending any op enqueues it to every `sync_out` contact in the same statement
flow (one `INSERT … SELECT … ON CONFLICT DO UPDATE`), so sync IS the write
path, not a pipeline beside it. The outbox is bounded: at most one queued op
per `(contact, tbl, uid, field)` — a newer `set` replaces the queued one, so a
burst of typing while a peer is offline queues one op, not thousands. The
boot-time sync runner wakes on any fact-bus event (250ms burst coalesce) and
drains immediately; relayed imports are never echoed to their source. Reactive
ops apply through the same import path as catch-up batches (signature,
quarantine, merge policy) — no trust shortcut.

### Catch-up reconciliation

`organ_contact.last_synced_seq` per pairing; the sync runner pulls ops after
the checkpoint per `sync_in` contact (default 500, cap 2000) — one indexed
rowid-range query, empty answer = converged = O(1). The checkpoint advances
only after the batch imports successfully, and the feed's `from_organ` must
match the contact. Per-contact `catchup_interval_secs` (default 30, clamped
5–300); `0` disables the pull cycle but keeps reactive deltas. The engine is
visibility-free: an authenticated contact gets the full feed; record hiding is
§12's serve-time filter, never state on the write path.

### Merge: Loro record-docs + the homebrew ledger

One Loro doc per record for its TEXT — `head` and `body` as Loro text
containers (character-level concurrent editing, `engine::collab` is the only
module that may import Loro). Scalar columns, extension namespaces (already
per-KEY ops), assertions, and concepts stay on the homebrew set/tombstone LWW
path — per-field ops already give per-key-map semantics, so the doc carries
only what benefits from a CRDT: text. Movable lists were considered and are
not needed — see §11b.
List scope is *visual ordering only*: a kanban move BETWEEN columns changes
quantity and/or @concept — ledger/assertion territory, never doc state.

Each `crdt` op's value is the cumulative tail since the last stored snapshot,
which makes bounded-outbox replacement lossless; compaction (≥100 ops or ≥256
KiB, `maybe_compact`) stores a full snapshot in `record_doc`. Docs are lazy
(snapshot + tail on first touch, LRU cap 64, zero memory for untouched
records). Doc seeding from pre-CRDT text uses a deterministic peer id derived
from (uid, head, body), so two Cells seeding the same replicated record
produce identical ops that dedupe instead of doubling the text. Materialized
read model, always: applying any text write or `crdt` op immediately writes
`record.head`/`record.body` back to SQLite; Protein, queries, File Sync, and
Archive read ONLY SQLite — if Loro vanished tomorrow the data is plain rows
and only concurrent merging degrades.

**Op retention** is built as an explicit operation (`store::sync_ops::prune`,
`Engine::prune_op_log`, migration 0049). The floor is
`organ_contact.peer_acked_seq` — how far each contact has received OUR log,
which is NOT `last_synced_seq` (our cursor into THEIRS; pruning against that
would delete ops a peer never saw, in exact proportion to how much they had
sent us). It advances from the two things that evidence durable receipt: a
peer's catch-up request carrying `after = X`, since they advance their own
checkpoint only after an import succeeds; and a push batch the peer accepted.
Never from the head of the batch just served — a peer that dies mid-import
still needs exactly those ops. Blocked contacts are excluded so a dead peer
cannot freeze retention forever, the floor only ever moves forward, and with no
synced contacts there is no floor and nothing is pruned.

**Only SUPERSEDED ops are prunable** — an op is droppable only when a NEWER op
exists for the same `(tbl, uid, field)` on the same channel. That one rule is
what makes **replica bootstrap** work with no bootstrap protocol at all: the
log always retains, for every live field, the op that established its current
value, so the surviving log IS current state plus recent history and a contact
added long after a prune builds a complete replica by replaying from zero like
anyone else. A record written once and never touched again is entirely current
state and nothing about it is prunable — correctly, because it is not history.
Growth is still bounded, since history is what accumulates: a field rewritten
ten thousand times keeps one op, so the log is O(live state), not O(edit
history).

It also keeps the LWW memory honest. Import asks "is this op older than what I
hold?" by reading the highest HLC for that field *out of this same log*
(`latest_hlc_for_field`). Pruning a field's newest op would erase that memory,
and a stale value arriving later would then look new and overwrite current
data. Keeping the tip is what makes "don't clobber local changes" true rather
than hoped for.

Two further exceptions survive below the floor: an op still referenced by
`sync_outbox` (deleting it turns a pending delivery into a silent no-op), and
**`crdt` ops, which are never pruned at all**. Each is the cumulative tail
since the last stored snapshot, and that snapshot lives in `record_doc` —
local state, not in the log — so a peer replaying from zero holds no snapshot
and dropping any crdt op would lose the text written before it. Compaction-gated
crdt pruning needs the serve path to ship `record_doc.snapshot` as part of a
bootstrap, which needs a synthesized op identity; `(actor_organ, hlc)` is the
unique index import dedupes on, so inventing one is not free. Deferred rather
than guessed at.

Deletes are tombstone ops, never row removal — a record tombstone freezes its
doc (`crdt` ops skip apply but still relay); undelete is a newer write.
Per-table application is one plain `match` in `import_op_batch` — facts: set
union with unchanged hash-chain + signature checks (quantity stays a fold,
structurally excluded from any doc); assertions: set/tombstone per uid, later
HLC wins; record text: `crdt` op → doc → materialize; other record fields +
extensions: per-field LWW; concepts: plain per-field LWW.

### File Sync (downstream consumer of the replica)

`Engine::file_sync_tick` mirrors every record whose origin is a given organ to
`{head}.md` (collisions disambiguated `{head} -- {uid}.md`) in a directory,
both ways; `spawn_configured_watchers` runs at boot per enabled organ (2s
tick). Selection is hardcoded to `organ_eq` for v1. `lince.file_sync`
(`enabled`, `path`) is configured per organ Record, so the local Organ mirrors
to `mydir/` while a replicated remote Organ mirrors to `work/` — organ sync
fills the replica, File Sync projects it to that organ's own directory.
Extensions are namespace-isolated (`record_extension` keyed by `(record_uid,
namespace)`, §1): one Cell writing `lince.file_sync` can never clobber another
namespace on the same record.

### Collab: Loro, and the client that finally runs

Three layers, none may leak into the others: (1) op sync — the sections above;
(2) the **Loro engine** — vendored library + `crdt` op relay + snapshot
compaction; (3) the **collab binding**, one reusable client element that gives
ANY sand surface multiplayer editing, with `record_editor` as the rich UI
built on top of it. Relation graphs, Kanban cards, notes, and table CRUD must
never own CRDT logic — they mount the binding (or `record_editor`) and pass a
Record context, nothing more. Layer 3 is still open; layers 1 and 2 are built.

"Instant save" is a UI behavior, not a mechanism: the binding commits through
the normal write path after a short debounce (~500ms of no keystrokes), the
fanout does the rest; no save button, no separate unsaved-state layer. Text
older than the debounce is always committed; the cosmetic gap (cursors, "who's
typing") is presence, ephemeral only.

**Vendoring Loro**: Rust `loro = "=1.13.9"` pinned in the workspace
`Cargo.toml`, all calls confined to `engine::collab` (yrs stays the named
fallback); browser `loro-crdt@1.13.9` (npm, SAME release as the crate)
vendored at `crates/web/src/sand/collab/vendor/` and served on the
always-registered routes `/board/vendor/loro-index.js`, `/loro_wasm.js`,
`/loro_wasm_bg.wasm` (siblings on purpose: the glue resolves the wasm relative
to `import.meta.url`) with the MIT license beside them at
`/board/vendor/loro.LICENSE.txt`. Upgrades bump both pins together.

**`crdt` op relay** (layer 2): local doc changes export as cumulative Loro
update tails and ride the op log as `crdt` ops (the bounded outbox replaces
per (contact, record) losslessly because each tail is a superset of the last);
remote `crdt` ops apply through `engine::collab`, then materialize head/body
to SQLite; a zero-delta Sync refresh fact per touched record wakes Protein and
collab sessions.

**Socket transport** for active docs, on the ONE existing WS: `collab_join`
(reply: `collab_state`, the doc's full snapshot), `collab_leave`,
`collab_update` (client Loro update bytes → engine merge → one cumulative
`crdt` op + materialize + refresh fact). Fan-out rides the fact bus: any fact
touching a joined record pushes `collab_change` with the merged snapshot — the
same signal covers a sibling session typing AND a peer Organ syncing in, and
client imports dedupe by version vector, so over-delivery is harmless. The
board bridge multiplexes per-record membership (`lince:collab-*` frames,
rejoin-with-snapshot on reconnect).

**The client collab layer, RUN at last** (2026-08-04). The `record_editor`
sand joins a Record's shared Loro document, edits `head`/`body` as ordinary
text, and sends a DELTA since its last send — not a snapshot per keystroke.
Both converge; only one stays cheap as the document grows, and the difference
is invisible until it is expensive. The vendored bundle is now EXECUTED in
tests (`crates/web/tests/collab_wasm.rs`, real node): it initializes, exposes
the `head`/`body` containers the engine materializes from, converges through
deltas, and — the one worth pinning — re-importing the server's echo of this
client's own work is a no-op rather than duplicated text. The Record sand's
body textarea binds directly to the record-doc over `H.collabJoin`/
`H.collabUpdate`: join-snapshot seeds a client LoroDoc, keystrokes fold in as
single-region diffs (200ms debounced send), remote snapshots merge
caret-preserving, and in-flight typing is folded before any remote reflect so
it is never clobbered.

**Presence moved INTO the binding** (2026-08-06), which is what makes it
reusable rather than a record-editor feature. `createCollabEditor` owns the
lane room, the throttled emit, peer expiry (a closed tab sends no goodbye),
and idle; `bindPresence` wires one input's selection to it, and `bindInputs`
does both at once *for a surface that binds head AND body* — `record_editor`
does, so presence follows the caret from its title into its body, while the
Record sand binds only the body because its head is an ordinary saved field
there rather than doc content. What travels is `{id, field, anchor, focus, idle}` — a selection RANGE,
because "they are about to replace this" is what a bare caret cannot say, and
`field` because every surface on one Record shares one lane room, so without it
a title caret would draw into a kanban card body at a meaningless offset.
Consumers render and decide nothing: `record_editor`, the Record sand body and
the kanban card body all take a resolved peer list. Idle dims rather than
disappears — someone who stepped away is still in the document.

Two bugs this closed, both silent. `identity` was resolved correctly by
`spawn_lane_forwarder` and then DROPPED at both bridge hops, so no sand could
ever have shown a name; `record_editor` compensated by rendering `from`, which
is a connection id, not a name. And the Record sand carried a second, parallel
collab implementation (its own doc, shadow text, single-region fold and send
cursor) — now deleted in favour of the shared element.

**Acks** (`ServerMessage::CollabAck`). A delta is exported relative to the
version the client believes the Cell holds; advancing that on SEND rather than
on confirmation means an update lost to a dropped socket is excluded from every
future export — the text stays on the author's screen and exists nowhere else,
which looks exactly like success. The client now tracks a confirmed version
separately from an optimistic one, and on reconnect the bridge clears pending
sends and tells each frame to re-export from its last ACKED version. The
rejoin snapshot alone could never fix this: it flows Cell → sand, and the lost
work is in the other direction. Re-sending something that did land is a no-op
(Loro dedupes by version vector), which is the safe direction to be wrong in.
Pinned by a test that types with the socket down and asserts the work arrives
after reconnect. Sends are debounced ~200ms and flushed on teardown, so closing
a card cannot drop the last keystrokes.

**One binding for every editable field** (`attachField`, 2026-08-07). A surface
names a `path` and gets back one object regardless of what kind of field it is:
`head`/`body` route through the record-doc (character-level CRDT merge, because
two people typing into one string is a real conflict with a real answer), while
`<namespace>.<key>` routes through the ordinary extension write — which is
already a per-key LWW op carrying its own HLC. A table cell should not have to
know which kind it holds; that is the whole point of the call. The returned
`kind` (`"crdt"` / `"lww"`) is there for a surface that wants to say so, not
for one that needs to branch. Only the edited KEY travels on the LWW path:
writing the whole namespace would clobber sibling keys another Cell changed
concurrently, which is exactly what per-key ops exist to prevent.

**Save state** is reported from the ack, not from the send: `onSaveState`
fires with the in-flight count, and "saved" means the Cell CONFIRMED the write.
A surface that renders "saved" when bytes reach the socket is asserting
something it cannot know. The Record sand shows it under the body.

NOT proven by those tests, and stated plainly rather than implied: that a sand
IFRAME may load the wasm. That depends on the frame's CSP and sandbox flags at
runtime and needs a browser. For the record, the live board serves NO CSP
header today (only the archive export sets one) and sand frames are
same-origin `srcdoc` with `allow-scripts allow-same-origin`, so the
same-origin ESM and wasm should load.

---

## 11b. Decided against

Kept in prose so nobody re-proposes them in three months. Each is a decision
with a date, not a task.

**At-rest encryption of record bodies and op values — DROPPED 2026-08-03.**
It was not free, and the honest accounting is that it bought little for a lot.
Encrypting at the field boundary above `log_set` would have covered both
plaintext copies cheaply (`record.head`/`body` and `sync_op.value`, the latter
because `records::log_set` calls `sync_ops::log_local` with the value inline),
but every READER then has to decrypt — query/projection, Protein, search,
export/archive, File Sync — and a missed one fails quietly as garbled text
rather than loudly as an error. What it defends is narrow: a database copied
out through a backup, a synced folder, or a disk pulled from a machine. It
does NOT defend against anyone executing as your user, who reads the key file
sitting beside the database. Full-disk encryption covers the same threat
better for no code at all. The cipher choice, had it happened, was
XChaCha20-Poly1305 with a 32-byte key in a file beside the database at mode
0600 — chosen because an OS keychain does not exist on a headless VPS,
deriving from a login password makes data unreadable whenever nobody is logged
in (breaking background sync, exactly wrong for an always-on Cell), and
SQLCipher encrypts everything including material that gains nothing from it.

State the loss plainly rather than pretending: **thread bodies sit in
PLAINTEXT SQLite on both ends.** That is a real regression against the
original "can we e2e encrypt it independent of lan?" ask — but that ask was
about the WIRE, and the wire IS delivered: iroh's QUIC/TLS 1.3, authenticated
and forward-secret. At-rest was an addition on top, not the request. It also
leaves one inconsistency standing: the VPS was called the least trusted
machine when arguing to split node key from identity key, and it now holds
plaintext conversations. If this is ever revived, `replica_root IS NOT NULL`
is the exact scope — the column already marks precisely the Records that would
need it, which is why it would be cheap to add later. (This supersedes the
earlier "at-rest encryption is an EARLY item" framing and the whole box of
sub-tasks under it; the one line that survived that box, a send queue that
flushes on next connect, is built — see Offline delivery.)

**The verification code — retired from all normal flows, 2026-08-02/03.** It
was built and it works: `engine::peers::verification_code` derives base32
(A-Z2-7) of the first 25 bits of `sha256(sorted both organs' pubkeys)` → 5
chars like `Y3HS4`; symmetric, deterministic, unit-tested, derived and never
transmitted. It only ever defended REMOTE first contact with no other trusted
channel. Under iroh the address IS the key, so there is no wire left to
substitute on; the residual threat is only MISDELIVERY — being handed the
wrong NodeId — and a conversational challenge does not defeat a live relay
that passes your "what did we do last Thursday?" to the real friend and the
answer back. A QR scanned in person does defeat it, completely, and so does
pasting the key into a chat app already authenticated to that human. Since
every flow this product has is one of those two, the code defends a case that
no longer occurs, and a security step users are taught to click past is worse
than no step. It stays in the tree as an optional "verify this contact" panel
for anyone pairing remotely; no normal flow shows it. What the 5 chars were
really reaching for — "which of these forty peers in a stadium is my friend" —
is DISAMBIGUATION, not security, and the honest fix is a short NodeId
fingerprint in the nearby list (see What is left).

**Time-locked key succession — CUT 2026-08-03.** A new root taking effect only
after a veto window requires peers to agree about time, requires the victim to
be online and watching during the window, and adds a second state machine to
the one part of the system that must never be subtly wrong. What it defends is
the case where the key was COPIED and the owner still holds it — precisely the
case the pre-signed revocation certificate handles, immediately and with no
clock assumptions. Near-zero security loss for a real drop in complexity.

**M-of-N social recovery — REJECTED 2026-08-03.** Every recovery path is also
an attack path, and a quorum scheme adds a second door that must be defended,
audited, and kept from becoming cheaper to walk than stealing the key. It buys
convenience in a rare event at the cost of permanent attack surface. What
remains is enough, because the root/operational split and succession chains
have already made the catastrophic case rare and visible: publish the
pre-signed revocation certificate (the old key is dead immediately, whatever
happens next), then re-establish through the channels that worked the first
time — QR in person, or a chat app already authenticated to that human.
Tedious, completely safe, and no new mechanism to attack. Existing threads
help more than they appear to: key theft is not the same as data theft, so an
announcement arriving inside a long shared thread from someone who knows its
history is strong evidence when the attacker took a key but not a database.
The honest summary: with the root offline, losing it requires physical access
to a drawer or a token — a threat model a person can actually reason about,
which is worth more than a clever protocol.

**One shared node key across devices — rejected 2026-08-02.** iroh publishes
NodeId → current addresses, so two endpoints with the same NodeId overwrite
each other's record and a dialer reaches whichever wrote last; and it forces
the Organ private key onto the VPS. See the identity floor above.

**A dual `url | node_id` address kind — rejected 2026-08-02.** That
scaffolding only existed to preserve contacts made before the refactor, and
local dev databases are expendable (standing "best schema over back-compat"
rule). A contact is reached by NodeId, full stop; the handful of existing ones
get re-paired.

**Skipping the op log for message Records — does not survive the reframe.** A
thread is a synced Record, so its ops must exist and must ship; an op that is
never written cannot sync.

**A Loro map for scalar and extension fields — rejected 2026-08-07.** The
obvious way to give a table cell live editing is to put its value in the
record-doc as a map key. Don't: those fields are ALREADY per-key LWW ops with
their own HLCs, merged on import by `latest_hlc_for_field`. Adding a Loro map
beside that creates a second authority over one value, and the only thing two
authorities can do is disagree — silently, since both would look correct in
isolation. Nothing is lost by declining: LWW is what a scalar wants, there is
no character-wise merge of the number 5, and the live-update half comes from
the ordinary Protein subscription a sand already holds. `attachField` therefore
routes map keys through the existing action (§11a).

**A bootstrap/snapshot wire protocol — not built, and no longer needed,
2026-08-07.** The plan was `pruned_through` plus gap detection plus
`FetchBootstrap`/`Snapshot` frames serving synthesized current-state ops. All of
it dissolved once retention became superseded-only: the log already retains
every live field's tip, so replaying from zero IS the bootstrap. The protocol
would also have had to invent `(actor_organ, hlc)` identities for ops that never
happened, and that pair is the unique index import dedupes on — a collision
there does not fail loudly, it swallows a real op as already-seen. Deleting the
plan removed a hazard rather than deferring one.

**Movable lists as a CRDT — NOT NEEDED, decided 2026-08-06.** The case for
them was concurrent kanban reordering, and kanban does not order cards that
way: card order comes from Protein, which is exact, total and one-directional,
so there is no per-card position for two people to move concurrently and
nothing for a CRDT to reconcile. Adding a movable list would introduce a second
source of truth for ordering whose only job would be to disagree with the
first. If some future surface genuinely needs hand-dragged order that survives
concurrent edits, this reopens — with the scope it always had, VISUAL ordering
only: a kanban move BETWEEN columns changes quantity and/or @concept, which is
ledger and assertion territory and never doc state.

**Store-and-forward through a THIRD Organ — not planned, and the only scenario
that would reintroduce message-layer sealing.** If it is ever wanted, sealing
returns as a real ratchet (`openmls` or an audited Double Ratchet crate),
never hand-rolled.

---

## 11c. What is left

Dependency-ordered within each group. Reasoning is kept inside each box on
purpose — the *why* is the part that is not recoverable from the code.

### Identity, roster, and publishing

- [ ] **pkarr publishing of the signed roster.** Without it, "one key is all
  they save" stays false: a contact learns roster v2 only by reaching a Cell
  listed in roster v1, so adding a laptop while the old Cells are off or lost
  strands the new one forever. Publish the SIGNED ROSTER under the Organ
  identity key via pkarr — it stores signed records addressed by an ed25519
  public key, which is exactly what the identity key is. Then identity key →
  current Cells resolves with no prior roster and the saved key is genuinely
  self-sufficient.
  What pkarr is, in one line: a phone book whose lookup key is your public
  key. You publish a small signed blob into the BitTorrent DHT; anyone knowing
  the public key fetches it and verifies the signature. Nothing to do with
  Lince Records — the "record" in "resource record" is a DNS-style entry. The
  size limit is the DHT's per-entry byte cap (mainline BEP44: 1000 bytes) and
  a roster fits with room to spare: a NodeId is 32 bytes, so five Cells plus a
  version counter, expiry and signature lands near 250 bytes. DHT entries
  expire in hours, so something must republish on a timer — a natural job for
  the always-on Cell, and a reason a laptop-only Organ should republish at
  every boot.
  **Republishing needs no private key.** It re-broadcasts an already-signed
  blob, so the VPS Cell can do it while holding no identity-signing material —
  do not let "the VPS republishes the roster" become "the VPS signs the
  roster" and quietly undo the key split.
- [ ] **Two tiers of publishing** — the resolution of "I want an add-me-in-Lince
  key without exposing my devices." The identity key itself is safe to publish
  anywhere: on its own it is an identifier and reveals nothing. The exposure is
  not in the key, it is in what the key RESOLVES TO.
  example:
  ```
  public tier   (the DHT record, readable by anyone holding the key)
      → the front-door Cell only: the always-on VPS. One address.
        A stranger who finds the key on a website learns that one
        machine exists and nothing else.

  contact tier  (shared over an already-authenticated connection)
      → the full roster, so contacts reach personal devices directly
        for speed instead of always paying the front-door hop.
  ```
  What the untiered version would leak to anyone holding the published key:
  how many devices the Organ has, each one's current IP, and which are online
  right now — which is to say whether the owner is home, travelling, or
  asleep. A daily-pattern leak to the entire internet, and the reason the
  tiers exist. A contact you already sync with necessarily learns which Cell
  it is talking to; that is unavoidable and harmless. The tiering is about
  non-contacts. "Never found" scopes to non-contacts only: personal Cells are
  not outbound-only, and contacts holding the roster do dial them directly.
  Cost to accept: if the front door is down, a stranger cannot reach the Organ
  at all. Existing contacts, holding the full roster, still can.
- [ ] **Front-door mechanics** — currently undefined and needed for "add me in
  Lince" to actually work. A stranger's invite arrives at the VPS, whose owner
  may be on a phone that is not in the public record and may be offline. The
  front door QUEUES the invite until a personal Cell syncs, reusing the
  offline send queue rather than forwarding live. The VPS holds no
  identity-signing material, so it cannot accept on the owner's behalf — it
  can only hold the request until a Cell that can decide sees it.
- [ ] **Dial policy: RACE all known Cells** and take the first that answers,
  with a preference order only as a tiebreak (prefer a LAN-local Cell for
  latency, the always-on one for bulk). No leader election — leaders exist for
  consensus, and an op log with CRDTs converges without one.
- [ ] **Key-succession chains shown to contacts.** The storage exists
  (`identity_succession`, `record_succession` / `successions`); what is left is
  the enforcement and the surface. Contacts store the full chain for each
  Organ, not just the current key, and a succession is accepted only if it
  chains from a key already held. Anything else is a loud, blocking warning
  that requires a human decision — NEVER a silent update. This is the cheap
  approximation of key transparency (CONIKS, Certificate Transparency), and it
  converts a silent takeover into a visible alarm. Build the signed succession
  record even if the UI lands later, so an Organ can rotate without every
  contact re-pairing.
- [ ] **Pre-signed revocation certificate**, generated at key creation and
  stored offline beside the root. It does not prove a new key is genuine, but
  it kills the old one immediately — damage limitation that works even when
  identity cannot yet be re-established. PGP has done this for decades and it
  costs nothing. Storing it WITH the root is what makes one drawer trip yield
  both revocation and re-establishment.
- [ ] **Serve BOTH ALPN versions during a transition.** The strings are
  versioned (`lince/sync/1`, `lince/thread/1`, `lince/live/1`) but `wire.rs`
  offers exactly those three and nothing else, so a bump to `/2` today would
  hard-cut every peer on the old build. What is missing is the transition
  path: accept `/1` and `/2` simultaneously for a release, so an old peer gets
  old behaviour rather than a broken half-upgrade. Cheap now, brutal once keys
  are in other people's hands.
- [ ] **Fail closed on the unknown**: an unrecognised op `kind`, grant version,
  or frame type quarantines rather than crashing or silently applying. A newer
  Organ syncing to an older one must degrade, never widen.
- [ ] **Nearby lists show a short NodeId fingerprint** beside the untrusted
  display name — disambiguation among many peers, explicitly not a security
  check, and impossible to spoof by choosing a name.
- [ ] **Label manual paste as TOFU in the UI.** Pasting a key in the Organ
  sand is trust-on-first-use on the identity key. Fine when the key came from
  somewhere you trust; say so rather than implying the typing verified
  anything.
- [ ] **Discovery UI in the Organ sand**: a nearby list of announced organs;
  selecting one runs the normal introduction flow and, on confirmation, adds
  it to contacts. Display names are untrusted labels — the UI must never
  present a name as identity. The pairing code is required on FIRST contact
  through discovery and skippable over an already-verified channel; what it
  defends is not key substitution on the wire (iroh retires that) but a relay
  attacker announcing their own NodeId under a friend's display name.
- [ ] **The accept-policy toggle must be legible.** Default is CLOSED, so
  publishing the key advertises reachability to people who already know you
  and grants nothing to anyone else. Turning it on is what opens the invite
  door, and the discovery UI must SAY so: the headline flow (meet a stranger
  on the LAN) needs the toggle on, and a nearby list that silently refuses
  everyone reads as broken.
- [ ] **State the privacy cost of publishing the key** — a PRIVACY issue, not
  a security one, and the distinction matters. Reaching a Cell across the
  internet works because discovery publishes NodeId → current addresses, so
  anyone holding the published key can resolve that Cell's current IP. Nobody
  thereby reads your data or forges your signature; what leaks is roughly
  WHERE you are (city-level geolocation) and WHEN you are online. For a key
  pasted on a personal website, that is a daily-pattern and approximate-home-
  location leak to anyone who looks. It is the same mechanism that makes
  beach-then-home work, so it is not separable — but it IS optional:
  relay-only mode publishes no direct addresses and peers see only the relay.

### Profile vs device surfaces

The Cell/Organ split earns its cost only if each side owns real things.

- [ ] **Organ Record = the public profile.** Display name, description,
  avatar, published identity key, the Cell roster, discovery preference
  (visible / relay-only / dark). This is what a contact saves, what a QR
  encodes, what goes on a website. It survives every device change.
- [ ] **Cell Record = this device.** Device label ("laptop", "phone", "vps"),
  its node key, whether it is always-on, and every local-only setting that has
  no business travelling: File Sync paths, storage config, local cache sizes.
  Never published except as a roster entry.
- [ ] **Audit every `organs::local()` caller.** The funnel is
  `store::organs::local()` (`organs.rs:76`, fixed slug) — today one row does
  both jobs, so every call site says `organs::local()` meaning "who am I" and
  "who authored this" interchangeably. After the split each one has to be
  re-read and assigned, and both ways of getting it wrong are bad: a site that
  means identity but keeps using the Cell uid makes one person look like three
  different Organs to everyone else; a site that means authorship but uses the
  Organ uid brings back the HLC collision that silently swallows remote ops.
  Two known examples of each meaning: `collab.rs` passes `&local.uid` as
  `actor_organ`, which means the CELL; `records.rs:121-131` stamps the origin
  through `set_organ_origin`, which means the ORGAN. Every `actor_organ` write
  site (they run through `store::sync_ops::append`) must mean the Cell, and
  every place reading `record.organ_uid` must mean the published Organ.
  Nothing else may assume the two are the same value. Audit, do not
  pattern-match. Note `record.organ_uid` is `Option<String>` and the origin is
  stamped by a separate call after insert, so the split lands on a field that
  can already be NULL — existing rows with no origin need a defined meaning
  BEFORE the audit lands, not after.
- [ ] **Enrolling a new device is pairing with YOURSELF**, and deserves its
  own flow rather than reusing contact pairing: an existing Cell shows a QR
  carrying its NodeId plus the single-use, short-lived enrolment token; the
  new Cell scans, connects, proves the token, and is signed into the roster.
  The token side is built; the flow and the QR are not.

### Individual replica and threads

- [ ] **Adopting an EXISTING Record into a replica root.** Deferred from the
  `replica_root` work because it opens a window: the Record already logged ops
  with `replica_root = NULL`, those ops already rode the general feed, and any
  `sync_in` contact gets them on their next catch-up. Needs a backfill
  decision and a UI that says plainly that already-sent ops cannot be un-sent.
  Sharing an arbitrary existing Record would make it its own root
  (`replica_root = own uid`) and stamp its subtree in ONE walk at that moment;
  new descendants inherit thereafter.
- [ ] **Bidirectional by agreement**: the sharer offers, the receiver accepts,
  and only then does the Record land in their Cell. Acceptance is what turns
  "you may see this" into "I keep a copy," and it is also what stops an Organ
  from pushing unwanted Records into someone's store.
- [ ] **Message Records must NOT ride the ordinary record sync feed.** They
  are delivered over the thread ALPN only. Otherwise a visibility bug in the
  normal feed leaks a private conversation to an unrelated contact, and even a
  sealed body would still expose who is talking to whom.
- [ ] **Key exchange IS the promotion step, and it happens inside the thread**:
  a "send my key" button posts the local Organ's identity key + NodeId as a
  message; receiving one offers "add as known Organ" with a name field the
  local user types (never the sender's claimed label). That single action
  writes the contact, adopts the key, and sets `trust='known'`. The Organ sand
  keeps the same thing by hand — paste a key, type a name — for contacts who
  never used a thread.
- [ ] **What remains after deletion is exactly one thing**: they may send an
  INVITE to open a new thread, one pending at a time, which lands in
  notifications. `trust='blocked'` drops invites too.
- [ ] **Conversation view in the Record sand**: thread list plus a message
  composer over the message Records. The existing text binding is the other
  branch of the same sand, untouched — a Record is either being talked in or
  being co-written, and the two views never contend. (A `conversation` sand
  exists; this is the Record-sand branch of it.)

### Transport cleanup

- [ ] **Retire `base_url` as a way to reach anyone** — settled 2026-08-05
  while simplifying the Organ sand. Registering an Organ by hostname is gone
  from the surface, because a URL matches nothing the transport does: pairing
  parses a NodeId, the outbox dials a NodeId, and inbound authorises by
  `contact_by_node_id`. What is left of `base_url` is a display string plus
  these callers, each of which must move to a NodeId dial before the column
  can go:
  - `web::presentation::http::transfer_delivery` — every envelope, receipt and
    pull request is an HTTP POST to `contact.base_url`. The largest of them;
    Transfer is the only subsystem still speaking HTTP peer-to-peer.
  - `engine::sync::Introduction.base_url` — carried in the introduction and
    written onto the contact by `adopt_introduction`. Harmless as a label,
    misleading as an address: it is the peer's view of itself, routinely a
    loopback URL.
  - `store::organs::Contact.base_url` (the `record.body` column) and
    `organ_contact.last_seen_addr`, which the retired HTTP runner ranked
    candidates from —
    keep it as a debugging breadcrumb or drop it; resolution is iroh's job.
  Until then the rule the sand already follows: a URL is something to show,
  never something to dial.
- [ ] **Transport reuse for reactive deltas**: when a live Protein WS to the
  contact is already open, reactive deltas ride it; the outbox drain is the
  fallback, not a second channel — the one-WS rule stays intact.
- [ ] **IPv6: prefer it wherever available.** NAT exists only because IPv4 ran
  out; with IPv6 every device can have a globally routable address, so there
  is no translation layer to defeat and direct connections succeed far more
  often. iroh already binds dual-stack and races v4/v6 paths, so the Lince-side
  work is only to not get in the way — bind both, publish v6 addresses in
  discovery, hardcode no v4 assumptions. The remaining gate is outside Lince
  entirely: the ISP must hand out IPv6 and the home router must have it
  enabled. Even then most routers keep a stateful inbound firewall, so hole
  punching is still needed — but punching a firewall pinhole is far more
  reliable than traversing address translation. IPv6 shrinks the relay's job;
  it does not remove it.

### The user's own infrastructure (opens the later work)

Everything after this improves once the user's always-on infrastructure
exists, which is why it comes first among the later items.

- [ ] **Run `iroh-relay` on the VPS.** It needs a public IP, a DNS name, and
  TLS (the relay speaks HTTPS/WebSocket to nodes); point the Cells at it as
  their configured relay instead of the default public ones. From then on the
  Organ's own machine carries its own connection metadata.
  What a relay actually does, since the mental model matters: both Cells hold
  a standing connection to it, so it is a mailbox that is always reachable. To
  reach B, A first sends through the relay — and immediately both sides start
  exchanging the addresses they observe and firing probe packets straight at
  each other. Those outbound probes punch a return path through each side's
  NAT or firewall, and when they meet, the connection UPGRADES to direct and
  the relay leaves the data path. The one correction to the simple model is
  the failure case: against a symmetric NAT or a strict firewall the punch
  never lands, and traffic keeps flowing through the relay for the life of the
  connection. It is a rendezvous AND a fallback, not only a rendezvous.
  On relay metadata: a relay cannot read anything (QUIC is encrypted end to
  end) but it observes that A dialed B at a given time. On the LAN no relay
  participates at all — mDNS finds the peer and the connection is direct, so
  the whole find-your-friend-in-a-room flow leaks nothing off the network.
  Running Lince on your own machine does NOT make you a relay: a relay must be
  publicly reachable at a stable address, which is exactly what a node behind
  NAT is not. Both VPS jobs coexist on one box — `iroh-relay` on its public
  address, and a Lince Cell that is a member of the Organ roster — as separate
  processes with separate ports and no interaction.
- [ ] **Self-host address publishing** (a pkarr/DNS publisher), so
  reachability does not depend on n0's infrastructure either.
- [ ] **Run a Cell on the VPS as a member of the Organ roster**: the always-on
  device that makes offline delivery work without either laptop being up.
- [ ] **Only then does relay-only mode cost nothing that matters** — the relay
  being depended on is the user's own. Depending on your own machine is not a
  dependency problem.
- [ ] **Discovery settings as ordinary config**, never a build flag: `local`
  controls mDNS advertising/listening on the LAN, independently from
  `internet`, which controls internet address publication (DHT + DNS). Both
  default ON, because a Cell that is not resolvable across the internet cannot
  serve the case that motivates the whole design — the VPS telling the phone
  about a change the laptop made. Off is the deliberate choice, not the
  default. Configured through `lince.discovery`, which already exists.
- [ ] **Live mode via iroh for hostname-less Cells**: your Cell fronts a
  remote Organ that has no public door. Your local Lince becomes the door to
  an Organ that has none of its own.

### Retention, audit, and modes

- [ ] **Put pruning on a schedule.** The reason it was manual is GONE:
  superseded-only retention means a from-zero replay is always complete, so
  there is no longer a contact that pruning can strand (§11a, and
  `replica_bootstrap.rs` pins it end to end). What remains is only the decision
  to run a destructive maintenance pass unattended, plus where it belongs — the
  sync runner's idle moment is the obvious place. Keep the dry-run mode either
  way; the report is computed from the same predicate as the delete, so it can
  never disagree with it.
- [ ] **Op kinds, written down as a closed set**, with snapshots explicitly
  not among them.
  example:
  ```
  set        field value
  tombstone  delete record / assertion / extension-key
  fact       existing signed fact rows, unchanged semantics — they join
             the log rather than a parallel channel
  crdt       a binary Loro update for one record-doc; commutes by
             construction, so ordering is irrelevant and idempotency is
             Loro's own dedupe plus the op identity

  NOT an op kind: snapshot. Bootstrap and visibility grants serve the
  current row state synthesized from the read model at serve time (each
  field carrying its stored HLC; record-docs as a Loro shallow snapshot),
  so the log holds only real writes and never bloats with copies of state.
  ```
- [ ] **Integrity audit — on-demand command, never a loop.** Checkpoints trust
  the peer's log, so a corrupted or buggy peer log is invisible to catch-up.
  `audit(organ)` walks both synced sets in uid order, streams
  `(uid, field_hlc_hash)` pages, and reports rows whose state disagrees
  despite equal checkpoints; repair reuses the normal import path.
- [ ] **Replica bootstrap and initial snapshot**, over iroh streams.
- [ ] **Branch on the `mode` column.** It exists and DEFAULTs to `'replica'`
  but no code reads it yet. Until then `sync_out=1` is mode-independent: the
  outbox drains to every non-blocked contact with the flag set, fact-bus-woken
  (~250ms coalesce), which is already live-ish in practice.
- [ ] **Organ sand controls the whole pairing per contact**: outgoing sync (my
  records go there), incoming sync (their records land here), both, or
  live-only — driven by the existing `sync_out`/`sync_in` flags plus mode.
- [ ] **Organ polling scheduler** (§2, tracked under Transfer T1).
- [ ] **Arbitrary configurable Protein filter for File Sync selection** —
  deferred, not wired to anything. Selection is hardcoded to `organ_eq`.

### Collab: the reusable binding and everything above it

- [ ] **Compaction = Loro shallow snapshot.** Today `maybe_compact` stores a
  FULL snapshot; shallow snapshots were deferred because a peer compacting at
  a divergent frontier could produce unimportable tails. Per record-doc,
  triggered by update count or byte threshold; store one snapshot, prune older
  `crdt` ops under the normal checkpoint-gated retention; loading a doc is
  snapshot + tail — never a history replay, so cost is O(current state), not
  O(edit history). Materialized text must be identical before and after. This
  matters only for genuinely long-lived collab documents, where human
  authorship bounds the size anyway — a thread is rows, not a document, so
  chat no longer creates the urgency it once did.
- [ ] **`record.<column>` as a bindable path.** `attachField` covers text and
  `<namespace>.<key>` (§11a); a scalar COLUMN — `slug`, `place_uid` — has no
  single-field action to drive, so it is the one path still unbound. It wants
  the same treatment as an extension key (per-field LWW through an ordinary
  action), not a Loro container — see §11b. The contract stays: fact-backed
  values (quantity) are structurally excluded; quantity displays update live
  because fact ops arrive on the same channel, not because the number is a
  CRDT.
- [ ] **`record_editor` sand** — the rich UI on top of the binding, standalone
  and embedded modes. Rich editing lives HERE, above the CRDT: the doc stores
  plain markdown text; slash commands are input affordances that insert
  markdown/block syntax at the caret (and `/slash` blocks stay a record_info
  product, K-plan unchanged); images are markdown links rendered at display
  time through the local `/host/media` pipeline; preview/rendering never
  writes. Because rich features are a layer over plain text, they need zero
  CRDT awareness and remote edits can never corrupt a block — worst case is
  concurrent text inside one block, which Loro text merges character-wise.
  - Inputs: `record_id`, `owner_organ_id`, `mode` (standalone/embedded),
    `field_policy` (`head_body`/`body_only`/future), inherited auth/session.
  - Rules: embedded mode never creates records or shows the record picker,
    edits only the concrete record it's given; ALL writes go through the
    binding; parents subscribe to editor events.
- [ ] **`Note` sand** (rename of the current markdown editor):
  - solo mode: a title-empty note is frontend-only (no `record` row); entering
    a title creates the record (title→`head`, markdown→`body`) and hands off
    editing to `record_editor`; a green status-ball picker (top right, like
    the document-reader pattern) lets the user bind to an existing record
    instead.
  - embedded mode: no status ball, no creation, no search — parent passes
    record context, Note renders `record_editor` for it.
  - naming: user-facing name stays `Note`; `record_editor` and the binding are
    internal; never say "CRDT" or "Loro" in normal UI labels.
- [ ] **Embed into existing sands**:
  - Relation: side panel embeds `record_editor` for the selected graph node;
    switching node rebinds/destroys the instance; Relation keeps its
    binary-assertion projection independent of editor state.
  - Kanban: focus-card body embeds `record_editor`; quick-card previews are
    read-only materialized text from SQLite (no doc load for closed cards — a
    200-card board costs zero Loro memory until a card opens).
  - Table: scalar cells may use the bare binding on `record.<column>` /
    `<namespace>.<key>`; `head`/`body` prefer embedding `record_editor`.
- [ ] **Delete/lifecycle rules**: a record tombstone freezes its doc — new
  `crdt` ops against it are rejected; undelete must land as a newer lifecycle
  op before edits resume; a title-less Note draft has no doc; a new record's
  doc initializes from its materialized columns. Today deletion is a hard
  tombstone with no `Undelete`/`Restore` action — that part is future work.
- [ ] **Test coverage** once the above exists: Note draft creates no record
  before a title; title creates the record and its doc; Note binds to an
  existing record via the picker; embedded editor cannot create/switch
  records; two bindings on the same record (record sand + kanban card)
  converge both ways; concurrent edits to different fields/keys both survive;
  local edits append
  `crdt` ops; applying a remote op materializes SQLite and never re-enqueues a
  loop; duplicate op identity is a no-op; a deleted record rejects `crdt` ops;
  shallow-snapshot compaction preserves materialized text and doc load never
  replays full history; slash-command insertion and image rendering survive a
  concurrent remote edit; socket subscribers receive local updates; the
  vendored `loro-wasm` asset ships its LICENSE/notice files and both pins are
  the same version.

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

# Nutrition Information Base

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

### Mirage: Lingua as a database language

A far-future, ground-up Lince could push this further than a projection.
Today the prelude is generated *from* the database and read back through
`EditRecordText`; a later rewrite could make `.lingua` the primary way to
program a Cell's data declaratively — a text form expressive enough to state
Concepts, Assertions, Records, and Rules directly, kept in lockstep with the
database rather than translated into and out of it, or possibly *being* the
database's own storage representation rather than a file synced against one.
That is a mirage for a future slopless Lince, not a direction for this
implementation — nothing in this document should be read as scoping current
work toward it.

### Current tooling: a real tree-sitter grammar via `rust-sitter`

Lince already has one working example of what a hard grammar definition for
a Lince DSL looks like end to end, built as a standalone exploration (not a
workspace member, not wired into any crate). It used `rust-sitter`: the
grammar is written as ordinary annotated Rust structs/enums (`Program`,
`Statement`, `Function`, `Block`, `Expression`, `Identifier`, with
`#[rust_sitter::leaf(...)]` marking literal/pattern tokens), and a `build.rs`
calling `rust_sitter_tool::build_parsers` generates and compiles a real
tree-sitter C parser at build time — no hand-written tokenizer or recursive
descent. `parse()` returns the typed Rust AST directly; a small
`format_program` walk over that AST gave a canonical formatter for free.

It was run and tested three ways: unit tests calling `parse()`/`format()`
directly (primitives, functions, syntax-error rejection, and every
`test/examples/*.lingua` fixture parsing without panicking); a CLI (`lingua
fmt [--stdin|--write|--check]`, `lingua parse <file>`) exercised by
`tests/cli.rs` spawning the built binary and checking stdout; and an LSP
server (`lingua lsp`, via `tower-lsp` over stdio) wired for
`textDocument/didOpen|didChange|didClose` and format-on-save, giving any
editor live diagnostics and formatting against the same grammar used for
parsing.

The takeaway for the current codebase: this approach is a good fit only
where a grammar needs to be hand-authored *and* editor tooling (highlighting,
incremental reparse, LSP) matters, and where the parser only needs to run on
a native host. It is not a fit for `karma/dsl.rs` or `nucleus/src/expr.rs` —
both are hand-written recursive-descent parsers, both already hardened to
this project's threat model (byte/token/nesting caps, exact byte/line/column
errors), and both compile to `wasm32-unknown-unknown` because `crates/web`
uses `nucleus` directly for client-side validation. A `rust-sitter` grammar's
generated C parser is compiled via `cc` for the host at build time, which is
not the same pipeline tree-sitter uses to target wasm (that goes through
emscripten and `web-tree-sitter`); nothing here confirms it would cross-compile
under `wasm32-unknown-unknown`, so it should not be assumed to work in the
sand.


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
