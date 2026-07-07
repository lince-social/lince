# Fable Improvement Critique

This critique reviews the current `docs/fable-improvement.md` as a theory document.
It ignores whether the code happens to implement something differently. The question here is:
does the markdown's model explain itself without contradiction?

Status key:

- **Still holds:** the earlier critique remains valid.
- **Partially resolved:** new text helps, but the theory is still ambiguous.
- **New:** introduced or exposed by the latest edits.

---

## 1. "Everything is a Record" is still too broad

**Status:** Still holds.

### The tension

The root axiom still says:

```text
Everything is a Record. Every change is a Fact. Every intended change is a Promise.
```

But the document's own model still has first-class things that are not Records:

- `fact`
- `promise`
- `link`
- `concept`
- `place`
- `transfer_party`
- `transfer_agreement`
- `visibility_rule`
- `identity_key`

The conflict is still visible in the visibility section, where `target_uid` says it can
target "ANY record" and lists `concept`, even though concepts are modeled in their own table.

### Why it matters

"Everything is a Record" is the strongest phrase in the blueprint. It promises one
activation knob, one visibility model, one sync model, one search model, and one graph model.
If it is literal, the primitive tables should also be Records. If it is not literal, the
document needs a boundary between record-addressable objects and structural rows.

### Proposed repair

Replace the axiom with:

```text
Everything user-addressable is a Record.
Every quantity change is a Fact.
Every future, conditional, or social intended quantity change is a Promise.
```

Then add a rule near Part I:

```text
Recordhood rule:
If a thing needs activation, user-facing text, visibility as an object, publication,
or first-class search as an object, it is a Record. If it is structural machinery under
a Record, it is a primitive row. Primitive rows may be queryable through Protein, but
they do not carry `quantity`.
```

For visibility, pick one:

```sql
-- Direct visibility for both records and primitive rows:
target_kind TEXT NOT NULL,
target_uid  TEXT NOT NULL
```

or:

```sql
-- Simpler: primitives inherit visibility from owner records:
target_record_uid TEXT NOT NULL
```

The second option better matches the corrected doctrine.

---

## 2. "Every intended change is a Promise" still conflicts with Actions and Decisions

**Status:** Still holds.

### The tension

The document now leans even harder on Actions:

- sands write only through Actions
- decision options execute Action lists
- Karma can run actions/effects
- Fiote writes through Actions

Those are intended changes, but they are not all Promises. A Promise has future/social
semantics: state, party, window, condition, transfer membership, settlement.

### Why it matters

Without this distinction, the model cannot answer when an intended change should be:

- immediate
- previewable
- cancelable
- projected
- matched by Senses
- agreed by another party
- settled into real quantity facts

### Proposed repair

Define four intent objects:

```text
Action: immediate typed command.
Promise: future, conditional, social, or scheduled intended quantity delta.
Decision: pending human choice over Actions and/or Promises.
Effect: external IO or notification queued outside evaluation.
```

Then add the rule:

```text
Karma emits a Promise when the change should be previewable, cancelable, projected,
matched by Senses, or agreed with another party. Karma executes/enqueues an Action
when the user has already authorized immediate execution under the rule's scope.
```

---

## 3. "One write path" still says all state, but really covers quantity

**Status:** Still holds.

### The tension

Part 0 still says all state changes go through the fact appender. But the document contains
many state changes that cannot be represented as quantity facts alone:

- promise state transitions
- links
- concepts and equivalences
- transfer parties and agreements
- visibility rules
- text CRDT edits
- decision answers
- identity keys
- record metadata
- board/widget state in VII.4

### Why it matters

The good invariant is "one quantity write path." The document currently suggests "one
database write path," which is impossible given sidecars and host state.

### Proposed repair

Use two explicit invariants:

```text
One quantity write path:
Every `record.quantity` change goes through `append()` or `append_all()`.

One semantic write surface:
All non-quantity state changes go through Actions or named engine processes. Each such
change either is itself signed/provenanced or emits a zero-delta annotation Fact on the
nearest owning Record.
```

Then classify state:

```text
Ledger state:
- record.quantity
- fact log

Semantic engine state:
- promise states
- links/concepts/visibility/agreements
- record metadata
- effect and decision sidecars

Host presentation state:
- board camera/workspaces/layout/widgetState
- never Ledger truth
```

VII.4 already starts making this host-state distinction; Part 0 should adopt the same
language so the doctrine is consistent.

---

## 4. Imported signed facts vs local hash chain is only partially resolved

**Status:** Partially resolved.

### What improved

The build order now says Trust has a "two-layer tamper model":

```text
chain guards content->hash, signature guards hash->author
```

That helps clarify hash vs signature.

### What still does not hold

Sync still says imported facts are brought in through `append` while preserving origin
signatures. Memory still models one `prev_hash`, one `hash`, and one `signature` on the fact.

If a remote fact keeps its origin `hash` and `signature`, it cannot also be resealed into
the receiver's local chain by changing `prev_hash`. If it is resealed, the original signature
no longer verifies.

The newer "two-layer tamper model" does not yet define origin fact identity vs local receipt
identity.

### Proposed repair

Keep the origin fact immutable and add a local receipt:

```sql
fact (
  uid TEXT PRIMARY KEY,
  record_uid TEXT NOT NULL,
  delta REAL NOT NULL,
  at TEXT NOT NULL,
  actor_uid TEXT,
  cause_kind TEXT NOT NULL,
  cause_uid TEXT,
  payload TEXT,
  origin_prev_hash TEXT NOT NULL,
  origin_hash TEXT NOT NULL,
  origin_signature TEXT
);

fact_receipt (
  fact_uid TEXT PRIMARY KEY REFERENCES fact(uid),
  cell_uid TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  local_prev_hash TEXT NOT NULL,
  local_hash TEXT NOT NULL,
  local_signature TEXT
);
```

Then change Sync wording:

```text
Import preserves the origin fact hash/signature and appends a local receipt into this
Cell's chain. The quantity cache is bumped only once per origin fact uid.
```

---

## 5. Promise roles remain overloaded

**Status:** Still holds.

### The tension

Promise still has one role field:

```text
party_uid -- who keeps it; NULL = OPEN slot
```

But Transfer examples need several roles:

- who owns the target record
- who is obligated to perform
- who receives the value
- who may settle
- whose trust history feeds confidence
- who can see hidden source details

The `SALE` example demonstrates this: money, bike, giver, receiver, and record owner are
not all the same concept.

### Why it matters

Transfer settlement, matching, balance checks, trust, and visibility all depend on role
clarity. A single `party_uid` cannot carry all of that without conventions hidden outside
the schema.

### Proposed repair

Split roles:

```sql
promise (
  uid TEXT PRIMARY KEY,
  target_record_uid TEXT,
  target_concept_uid TEXT,
  delta REAL NOT NULL,
  obligor_uid TEXT,      -- expected performer/keeper
  beneficiary_uid TEXT,  -- expected receiver/beneficiary
  owner_uid TEXT,        -- target record owner when not derivable
  transfer_uid TEXT,
  state TEXT NOT NULL,
  condition TEXT,
  window_start TEXT,
  window_end TEXT,
  reserve_from TEXT,
  signature TEXT
)
```

Then describe canonical shapes:

```text
Open Need:
- target_concept_uid=@apple
- delta=-3
- obligor_uid=NULL
- beneficiary_uid=@ana
- state=open

Open Contribution:
- target_concept_uid=@apple
- delta=+3
- obligor_uid=@bruno
- beneficiary_uid=NULL
- state=open

Local settlement:
- target_record_uid=@apples.stock
- delta=-3
- obligor_uid=@ana
- beneficiary_uid=@bruno
```

---

## 6. Promise/place logistics still assumes fields that do not exist

**Status:** Still holds; now more urgent because Place and Senses are marked further along.

### The tension

Place says `record.place_uid` plus promise windows are enough for logistics, and Senses
scores promises with `a.place`/`b.place`. But Promise has no place fields. Concept-level
open promises may not have a `record_uid`, so they cannot inherit a record place either.

### Why it matters

Transport, ride matching, delivery, public/restricted proximity, and route overlap all need
promise-level places.

A record place is not enough:

- source and destination may differ
- delivery may happen away from the inventory's home
- ride promises need origin and destination
- open concept promises need location without revealing a private record

### Proposed repair

Add Promise place roles:

```sql
promise (
  ...
  from_place_uid TEXT REFERENCES place(uid),
  to_place_uid   TEXT REFERENCES place(uid),
  at_place_uid   TEXT REFERENCES place(uid)
)
```

Define resolution:

```text
Promise place resolution:
1. use promise from/to/at place when present
2. otherwise inherit target record place
3. otherwise the place score is unknown and Senses may match only on concept/window/trust
```

For a one-place event, use `at_place_uid` or set `from_place_uid = to_place_uid`.

---

## 7. Transfer settlement still says "only Record mutation" but mutates promises

**Status:** Still holds; sharper because VIII.3 and Stage 4 are now checked.

### The tension

VIII.3 is now checked and still titled:

```text
Settlement (idempotent, the only Record mutation)
```

The pseudocode appends facts, sets promises to `Kept`, triggers conditional promises, and
withdraws siblings. Stage 4 also says "settlement as the only Record mutation."

The intended invariant seems to be "only record quantity mutation," not "only mutation."

### Why it matters

Settlement is the highest-trust write path. It should state exactly which non-quantity
writes are allowed, because the pseudocode explicitly performs them.

### Proposed repair

Rename:

```text
Settlement: idempotent, the only quantity mutation
```

Then state:

```text
Settlement may only:
1. append settlement facts for due active promises
2. transition those promises active -> kept
3. write confirmation annotation facts when required
4. activate conditional downstream promises whose conditions now hold
5. withdraw sibling duplicated promises/transfers under satiation policy

Settlement may not:
1. create parties
2. invent new economics
3. bypass agreement policy
4. mutate record.quantity except through append_all
```

---

## 8. Effect identity remains muddy

**Status:** Still holds.

### The tension

VI.1 says "rule/signal/effect are records with sidecars," but the schema has only
`effect_queue`. Consequence execution says command/query/action results are logged as facts
on "the effect's record," but no effect record sidecar is defined.

### Why it matters

Effects need a clear identity for:

- permissions
- retries
- status
- provenance
- visibility
- notification/channel config

The document currently mixes two different models: reusable effect definitions and one-off
queue entries.

### Proposed repair

Make both explicit:

```sql
CREATE TABLE effect (
  record_uid TEXT PRIMARY KEY REFERENCES record(uid),
  kind TEXT NOT NULL,        -- command | notify | http | action
  config TEXT NOT NULL       -- JSON
);

CREATE TABLE effect_queue (
  uid TEXT PRIMARY KEY,
  effect_uid TEXT REFERENCES effect(record_uid),
  origin_uid TEXT,
  payload TEXT NOT NULL,
  status TEXT NOT NULL,
  attempts INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  result TEXT
);
```

Then define logging:

```text
If an effect definition record exists, execution results are annotation facts on the
effect record and include `origin_uid`. If the effect is inline, execution results are
annotation facts on the originating rule/action subject.
```

---

## 9. Messages are still a required core primitive without a model

**Status:** Still holds; stronger after VII.4.

### The tension

Messages appear in several places:

- Transfer chat: `messages where subject = t_uid`
- Kanban comments via the message model
- Window case 6 chat/calls
- "messages-attach-to-anything" as one of four forced core additions
- "chat = comments = negotiation" in the standing law

But there is still no message schema, Action set, Protein include/source, visibility rule,
sync behavior, or interaction section.

### Why it matters

Messages are not just UI text. They touch:

- transfer negotiation
- comments
- social posts
- call invites
- threading
- edits/deletes
- visibility
- sync
- signatures
- attachments
- retention

### Proposed repair

Add a small Message part:

```sql
CREATE TABLE message (
  uid TEXT PRIMARY KEY,
  subject_uid TEXT NOT NULL,
  author_uid TEXT,
  body TEXT NOT NULL,
  reply_to_uid TEXT REFERENCES message(uid),
  created_at TEXT NOT NULL,
  edited_at TEXT,
  deleted_at TEXT,
  signature TEXT
);
CREATE INDEX idx_message_subject ON message(subject_uid, created_at);
```

Doctrine:

```text
Messages attach to user-addressable Records by default. They inherit subject visibility
unless a stricter message-level rule exists. Message edits are append-only revisions or
text CRDT updates; they never change record.quantity. Calls are messages/events plus
ephemeral lanes for live signaling.
```

Protein and Actions:

```json
{ "include": { "messages": { "limit": 50 } } }
```

```text
message: post-message, edit-message, delete-message
```

---

## 10. Concept fallback semantics are still vague

**Status:** Still holds.

### The tension

Lingua says an engine that does not know `@blocks-softly` treats it as parent `@blocks`.
But "does not know" can mean several things:

- the row is absent
- the row exists but is not adopted
- the row exists but is untrusted
- the row exists but its instinct function is unimplemented
- the parent chain is missing
- there are multiple parents

### Why it matters

Fallback affects Senses, Protein filters, rule saves, rule evaluation, package import, and
adoption. Silent fallback can create surprising automation.

### Proposed repair

Define concept states:

```text
known: row exists and is adopted
carried: row exists only because an imported package brought it
unknown: uid/name appears but no row exists
unimplemented: row exists, but local engine has no function for its instinct
```

Define fallback:

```text
Fallback applies only to known/carried concepts with an imported parent chain. It is allowed
for search, matching, and display. It is not allowed for instinct execution unless the parent
function explicitly accepts the child. Unknown concepts do not fallback.
```

Multiple parents:

```text
Search/matching may widen to any parent. Rule execution must be unambiguous; otherwise save
is rejected unless the rule names the parent explicitly.
```

---

## 11. Visibility defaults still need a subject/object model

**Status:** Partially resolved.

### What improved

Stage 3 now claims a single visibility gate, `execute_for`, and visibility-filtered export
is mentioned later in the build order.

### What still does not hold

The theory section still has only:

```text
subject_kind: organ | actor | role | public | fiote
target_uid: ANY record
field
grant
```

It does not define request contexts or target inheritance. VII.4 also adds sands as active
callers over the WebSocket, but the visibility subject model does not say whether a sand is
its own subject, runs as local user, or has per-sand capabilities.

### Proposed repair

Define request subjects:

```text
VisibilitySubject =
  LocalUser(actor_uid)
  RemoteOrgan(organ_uid)
  RemoteActor(actor_uid, via_organ_uid)
  Sandbox(sand_uid, acting_actor_uid)
  Fiote(agent_uid, acting_actor_uid)
  Public
```

Define inheritance:

```text
Record visibility is primary.
Facts inherit from fact.record_uid.
Promises inherit from target record, transfer, or concept publication rule.
Links inherit from both endpoint records unless explicitly published as part of a package.
Messages inherit from their subject.
Attachments inherit from owner row.
Primitive rows not covered above inherit from their owning Record.
```

Define conflict order:

```text
field override > row override > owner inheritance
actor > role > organ > public
explicit hidden beats visible at the same specificity
default hidden for non-local subjects
local owner can read unless local privacy mode locks it
```

---

## 12. Trust/Transfer dependency order is improved but still misleading

**Status:** Partially resolved.

### What improved

Trust is now marked done, with signatures and import verification described in Stage 7.

### What still does not hold

Build order still places Stage 4 Transfer before Stage 7 Trust, while Stage 4 says two-Cell
donations and sales are usable/tested. Transfer theory depends on signed promises,
settlement facts, authorship-preserving sync, and confidence/trust.

If the build order is chronological, Transfer cannot be complete before basic Trust. If it
is dependency order plus later retroactive status, the section should say so.

### Proposed repair

Split Trust:

```text
Trust A: identity and signature substrate
- key table
- sign facts/promises
- verify imported rows
- quarantine invalid packages
- required before cross-Cell Transfer

Trust B: verifiable aggregates and social trust views
- kept ratios
- verified aggregate Proteins
- leaderboard sands
- deferred
```

Then move Trust A before cross-Cell Transfer in the build order, or explicitly say:

```text
Stage 4 local Transfer is usable without Trust A. Cross-Cell Transfer acceptance requires
Trust A, implemented in Stage 7.
```

---

## 13. Checkbox semantics now conflict with local unchecked requirements

**Status:** New.

### The tension

Several parent sections are checked while their local required bullets remain unchecked.
Examples:

- Part VIII Transfer is checked, but VIII.1 status derivation, edit invalidation, balance
  check, and messages are unchecked.
- VIII.3 Settlement is checked, but delivery/receipt confirmations and Karma settlement
  policy bullets are unchecked.
- IX.1 Place is checked, but map data, exposure, logistics, and future instinct bullets
  are unchecked.
- XI.1 Keys and signatures is checked, but all listed key/signature/import bullets are
  unchecked.
- XII.1 The fold is checked, but frozen signals, branching, and threshold extraction remain
  unchecked.
- Stage 3 is checked as done while Part VII still leaves sources, aggregates, live, saved
  Proteins, and visibility unchecked.

### Why it matters

The document says every part and section title carries a checkbox and should be checked
when implemented and verified. Parent checks now mean either:

- all child requirements are complete, or
- the implementation has progressed elsewhere and local bullets are stale.

Both cannot be true.

### Proposed repair

Define checkbox semantics:

```text
Parent checkbox:
- [ ] no acceptance slice complete
- [/] usable slice exists, but listed child requirements remain
- [x] every child requirement in the section is implemented and verified
```

Then change current parent sections with unchecked children to `[/]`, or mark the children
with explicit "superseded by Stage N / implemented as X" notes.

This is not cosmetic. The blueprint is supposed to be executable; checkboxes are part of
the execution model.

---

## 14. Stage status and normative section status disagree

**Status:** New.

### The tension

Stage 3 says Protein includes facts/promises/links/availability, aggregates, saved Proteins,
single visibility gate, place `near`, JSON wire, and canned queues are done. But Part VII
still marks several of those as pending or partially pending in the normative checklist.

Stage 5 says confidence and `confidence()`/`projected()` Karma tokens are done. But Part XII
still leaves Confidence unchecked and says exposed tokens are unchecked.

Stage 7 says every fact is signed on the write path and import verification exists. But
Part XI's bullets for Ed25519, signing, import verification, and automatic verification are
unchecked.

### Why it matters

The build order is now doing status reporting that overrides the local theory sections. A
reader cannot tell which source is authoritative.

### Proposed repair

Pick one:

1. Local section checklists are authoritative. Stage bullets summarize them.
2. Stage bullets are authoritative. Local section bullets must be updated or explicitly
   labeled "remaining theory / not current status."

Recommended:

```text
The section checklists are normative. Stage XVII may summarize progress but must not mark
capabilities done unless their owning section is checked or marked superseded.
```

---

## 15. Sand migration adds a capability/security gap

**Status:** New.

### The tension

VII.4 says every sand will speak only Protein and Actions over the WebSocket. It also keeps
sand import/publish and the widget bridge. But there is no theory for sand identity,
capabilities, or per-sand permissions over the Action catalog.

If a sand can send arbitrary Actions as the local actor, imported sands become a write
capability problem.

### Why it matters

The old table CRUD path going away is good, but replacing it with full Action access is
only safe if the host mediates permissions. This affects:

- imported `.html` packages
- published sands
- widget bridge APIs
- package trust
- Action budgets
- visibility subject for reads
- destructive or high-trust Actions such as transfer settlement, visibility changes, and
  package install

### Proposed repair

Add a Sand Capability Model:

```text
Every sand runs as VisibilitySubject::Sandbox(sand_uid, acting_actor_uid).
The host grants each sand a capability set:
- read Proteins by saved slug or inline shape
- allowed Action kinds
- allowed target scopes
- allowed ephemeral lane rooms
- network/embed permissions
- package resource permissions
```

Example:

```sql
CREATE TABLE sand_capability (
  sand_uid TEXT NOT NULL,
  kind TEXT NOT NULL,        -- protein | action | lane | resource | network
  scope TEXT NOT NULL,       -- JSON or structured selector
  grant TEXT NOT NULL,
  UNIQUE(sand_uid, kind, scope)
);
```

Action execution should receive both `actor_uid` and `sand_uid`, and provenance should record
both:

```text
actor=@ana, cause=sand:<sand_uid>, action=set-quantity
```

---

## 16. Board/widget state needs an explicit owner and sync rule

**Status:** New.

### The tension

VII.4 correctly says board chrome is frontend-only presentation state, not Ledger truth.
But it also says existing board features are carried over verbatim through board-state store
and host `widgetState`.

The theory does not say:

- whether board state syncs between devices
- whether board state is local-only
- whether it is visible/publishable
- whether it participates in backup/export
- whether it is per user, per Cell, per workspace, or per Organ
- whether old board state is hand-migrated with data

### Why it matters

Board layout is not domain truth, but it is still user data. Losing it or syncing it
unexpectedly would be bad. It also affects sand import/publish and workspaces.

### Proposed repair

Add a host-state doctrine:

```text
Host state is local user interface state, not Ledger truth. It is stored under the local
Cell profile, may be backed up, and syncs only through explicit UI-profile sync. It is never
visible to remote Organs unless exported as a package or workspace.
```

Define ownership:

```text
BoardWorkspace: owner_actor_uid, cell_uid
BoardCard/widgetState: workspace_uid, sand_uid
BoardCamera/edit mode: actor-local, not shared by default
```

This keeps the "presentation state is not the Ledger" rule while treating UI state as real
data with lifecycle.

---

## 17. `source: timeline` is mentioned but absent from Protein sources

**Status:** New.

### The tension

Part XII says Protein exposes Imagination as `include: projection` and `source: timeline`.
Part VII's source list is:

```text
record | promise | fact | concept | decision | transfer
```

No `timeline` source appears there.

### Why it matters

Projection can be either:

- an include attached to records/promises
- its own queryable source for timeline views

Both are useful, but the Protein contract needs to name both if both exist.

### Proposed repair

Add `timeline` to Protein sources:

```text
source: record | promise | fact | concept | decision | transfer | timeline
```

Define shape:

```json
{
  "source": "timeline",
  "where": { "all": [
    { "record_in": ["@checking", "@rent"] },
    { "between": ["at", "now", "+30d"] }
  ]},
  "include": { "cause": true, "confidence": true }
}
```

Or remove `source: timeline` from Part XII and keep projection only as an include.

---

## 18. Senses as "kind='rule' variant 'sense'" lacks a schema home

**Status:** New.

### The tension

The Senses matcher says:

```text
MatchRule itself is a record (kind='rule' variant 'sense')
```

But the Record kind list has `rule`, not `sense`, and the rule schema does not define a
variant/subtype field. Senses rules are not ordinary Karma rules either: they have
`watch`, `max_proximity`, `min_confidence`, and `auto`.

### Why it matters

Senses needs activation, scheduling, publication, and visibility like rules, but its config
is not a Karma condition/consequence pipeline. Hiding it as a "variant" without schema will
make Protein, Actions, and package import ambiguous.

### Proposed repair

Either add a sidecar:

```sql
CREATE TABLE sense_rule (
  record_uid TEXT PRIMARY KEY REFERENCES record(uid), -- kind='rule' or kind='sense'
  watch TEXT NOT NULL,
  max_proximity INTEGER NOT NULL,
  min_confidence REAL NOT NULL,
  auto TEXT NOT NULL
);
```

Or add `kind='sense'` to Record kinds.

Recommended:

```text
Keep `kind='rule'` for Karma rules only. Add `kind='sense'` for Senses match rules.
Both are activatable Records, but they have different sidecars and engines.
```

---

## 19. Transfer derived status uses states not defined in Promise or Transfer schema

**Status:** New.

### The tension

Transfer status is derived as:

```text
draft -> proposed -> agreed -> in_transfer -> settled
inactive when quantity=0
```

But Promise states are:

```text
open | proposed | agreed | active | kept | broken | withdrawn
```

There is no `draft` promise state, no `in_transfer` promise state, and no explicit transfer
state column because status is derived. The derivation rules are not defined.

### Why it matters

Transfer status drives UI, decisions, visibility, agreement requests, settlement availability,
and Karma `advance_transfer`. If it is derived, the derivation must be deterministic.

### Proposed repair

Define transfer status as a pure function:

```text
inactive:
  transfer record quantity == 0

draft:
  no parties have been notified OR all bundled promises are local draft promises

proposed:
  at least one party exists and at least one party agreement level < 2

agreed:
  agreement policy satisfied and no bundled promise is active/kept/broken

in_transfer:
  at least one bundled promise is active and not all due promises are terminal

settled:
  all required bundled promises are kept, withdrawn by satiation, or otherwise terminal

broken:
  any required active promise is broken and policy does not allow ignoring it
```

If `draft` is meaningful before proposal, add either:

- a transfer visibility/proposal flag, or
- a promise state `draft`, or
- a rule that `quantity=0` + parties absent means draft.

---

## 20. "The engine is the only writer" conflicts with host-state and package operations

**Status:** New/refined from item 3.

### The tension

Part 0 says `engine` is the only writer. VII.4 says board chrome and widget state live in
host state and are preserved. Publish/import package flows also operate around sand assets,
manifest validation, and catalog pickup.

Those are writes, but not necessarily engine writes.

### Why it matters

There are now at least three write domains:

- engine database writes
- host UI state writes
- package/resource filesystem or catalog writes

They should not all be governed by the same "engine only writer" sentence.

### Proposed repair

Define write domains:

```text
Engine writer:
  writes the Cell semantic database and Ledger.

Host writer:
  writes local presentation state, workspace layout, widgetState, and local UI preferences.

Package writer:
  writes package resources/catalog entries through the package installer/publisher.
```

Then state:

```text
Sands cannot write any domain directly. They request engine writes via Actions, host writes
via host-control APIs, and package writes via package Actions that require explicit
capabilities.
```

---

## Recommended doctrine patch

The root doctrine should become:

```text
Everything user-addressable is a Record.
Every quantity change is a Fact.
Every future, conditional, or social intended quantity change is a Promise.
Every immediate semantic write is an Action.
Every pending human choice is a Decision.
The rest is choreography, and Protein is how the dance is seen.
```

Core invariants:

```text
1. `record.quantity` has exactly one writer: `append` / `append_all`.
2. All semantic database writes go through Actions or named engine processes.
3. Host presentation state is real user data, but not Ledger truth.
4. Primitive rows inherit visibility from their owning Record unless explicitly overridden.
5. Remote facts preserve origin signatures and receive local import receipts.
6. Promises name roles explicitly: target, obligor, beneficiary, places, window.
7. Imported sands have explicit capabilities; they do not inherit arbitrary Action power.
```

Highest-priority fixes to the markdown:

1. Fix the root axiom and one-write-path wording.
2. Define Promise roles and Promise place.
3. Define Message as a primitive.
4. Define sand capabilities.
5. Normalize checkbox semantics so parent and child statuses agree.
6. Add `timeline` or remove `source: timeline`.
7. Split Sense rules out of Karma rule schema.
