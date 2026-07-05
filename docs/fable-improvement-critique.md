# Fable Improvement Critique

This critique focuses only on internal theory consistency in `docs/fable-improvement.md`.
It deliberately ignores current implementation status. Each item names the tension, why it
matters, and a concrete repair that keeps the blueprint's spirit intact.

---

## 1. "Everything is a Record" is too broad

### The tension

The root axiom says:

> Everything is a Record. Every change is a Fact. Every intended change is a Promise.

But the document's own model has many first-class things that are not Records:

- `fact`
- `promise`
- `link`
- `concept`
- `place`
- `transfer_party`
- `transfer_agreement`
- `visibility_rule`
- `identity_key`

The conflict becomes explicit in Part XV, where visibility says `target_uid` can be any
record, including `concept`, even though concepts are their own table rather than records.

### Why it matters

The phrase "Everything is a Record" is doing important conceptual work: it promises one
activation knob, one visibility model, one sync model, one search surface, and one graph
surface. If taken literally, every sidecar primitive should be a Record. If not taken
literally, engineers need to know which things are record-addressable and which are lower
level machinery.

Without that boundary, later design decisions become ambiguous:

- Should concepts have `quantity`?
- Should facts be visible through `visibility_rule` directly, or only through their record?
- Can a link be activated/deactivated?
- Can a promise be searched as a Record, or only through Protein's `source: promise`?
- Can a visibility rule itself be synced, signed, or published as a Record?

### Proposed repair

Narrow the axiom:

> Every user-addressable, activatable, publishable object is a Record. Every quantity change is a Fact. Every future, conditional, or social intended quantity change is a Promise.

Then explicitly define the layers:

- **Record layer:** things with identity, text, quantity activation, visibility, sync presence, and user-facing lifecycle.
- **Primitive row layer:** facts, promises, links, concepts, places, keys, visibility rules, parties, agreements.
- **Sidecar layer:** rows that specialize a Record kind, such as rule, signal, decision, transfer, protein, sand.

Suggested addition near Part I:

```text
Recordhood rule:
If a thing needs activation, user-facing text, visibility as a subject/object, publication,
or first-class search as an object, it is a Record. If it is structural machinery under a
Record, it is a primitive row. Primitive rows still sync and may be visible through their
own source in Protein, but they do not carry `quantity`.
```

Then change the visibility wording from "ANY record (rule, transfer, plain, concept...)"
to one of these:

- `target_kind, target_uid` if visibility directly covers records and primitive rows.
- `target_record_uid` if primitive rows inherit visibility only through their owning record.

The second option is simpler and better aligned with the "everything user-facing is a
Record" correction.

---

## 2. "Every intended change is a Promise" conflicts with Actions and Decisions

### The tension

Actions are the typed write surface. Decisions execute Action lists. Karma consequences
can enqueue effects or create decisions. These are all intended changes, but not all are
Promises.

Meanwhile, Promise is defined as a future delta with window, party, state, condition, and
settlement semantics. That is narrower than "every intended change."

### Why it matters

This is not just wording. It affects what should be previewable, cancelable, simulated,
signed, synced, and trusted.

Examples:

- `set-slug` is an intended change, but making it a Promise would be awkward.
- `set-extension` is an intended change, but not necessarily a quantity delta.
- `create-concept` is an intended change, but has no obvious `delta`.
- `notify` is an intended effect, not a promise to change state.
- A decision option is an intended Action list, but the decision itself should not always
  become a bundle of Promises.

### Proposed repair

Separate intent types:

- **Action:** immediate typed command, may mutate sidecars or append facts.
- **Promise:** future, conditional, social, or scheduled intended quantity delta.
- **Decision:** pending human choice over one or more Actions and/or Promises.
- **Effect:** external IO or notification queued outside evaluation.

Refine the axiom:

> Every future, conditional, or social intended change is a Promise. Every immediate write is an Action. Every human choice is a Decision.

Then define when Karma should emit a Promise versus an Action:

```text
Karma emits a Promise when the change should be previewable, cancelable, projected, matched
by Senses, or agreed with another party. Karma emits/runs an Action when the user already
authorized immediate execution under the rule's scope and budget.
```

This preserves the core power of Promises without forcing every write into the Promise
state machine.

---

## 3. "One write path" says all state, but the mechanism covers quantity

### The tension

Part 0 says everything that changes state goes through the fact appender. But the blueprint
also has many non-quantity state changes:

- promise state transitions
- link creation/removal
- concept creation/adoption/equivalence
- transfer party and agreement changes
- visibility rule changes
- text CRDT updates
- decision answers
- identity keys
- record metadata changes such as slug, concept, unit, place, extensions

The appender only writes facts and bumps `record.quantity`.

### Why it matters

The single-write-path idea is excellent, but if stated too broadly it becomes false as
soon as sidecars are edited. The real invariant is not "all database mutations are facts."
The real invariant appears to be:

- All `record.quantity` mutations are Facts.
- All semantic mutations happen through Actions or controlled engine processes.
- Important non-quantity mutations leave provenance facts or signed state rows.

### Proposed repair

Rename the invariant:

> One quantity write path: every `record.quantity` change goes through `append()`.

Then add a second invariant:

> One semantic write surface: all non-quantity state changes go through Actions or named
engine processes, and each change either is itself signed/provenanced or emits a zero-delta
annotation fact on the nearest owning Record.

Suggested classification:

```text
Quantity state:
- record.quantity
- always changed by Facts through append/append_all

Semantic state:
- record head/body/slug/concept/unit/place
- sidecar rows
- promise state
- visibility rules
- transfer agreements
- links/concepts
- changed by Actions or engine processes
- provenance is either a zero-delta Fact or a signed row, depending on the primitive
```

This keeps the ledger meaningful without pretending every table is derived from facts.

---

## 4. Imported signed facts conflict with a local hash chain

### The tension

Sync says imported facts call `append` while preserving original author signatures.
Memory says each fact has `prev_hash`, `hash`, and `signature` in one chain per Cell.

If an imported fact keeps its original `hash` and `signature`, it cannot also have a new
local `prev_hash` in the receiving Cell's chain. If the receiving Cell reseals it into the
local chain, the original signature no longer verifies over the changed hash.

### Why it matters

This is a trust-model crack. The system needs both:

- origin authorship: who originally made the fact and signed it
- local receipt/order: when this Cell accepted it into its own ledger

Trying to store both in one hash/signature field will eventually force either broken
verification or lossy imports.

### Proposed repair

Use two layers:

```sql
fact (
  uid,
  record_uid,
  delta,
  at,
  actor_uid,
  cause_kind,
  cause_uid,
  payload,

  origin_prev_hash,
  origin_hash,
  origin_signature,
  origin_cell_uid,

  local_prev_hash,
  local_hash,
  local_signature,
  imported_at
)
```

Or keep `fact` as the origin object and add a receipt table:

```sql
fact_receipt (
  fact_uid TEXT PRIMARY KEY REFERENCES fact(uid),
  cell_uid TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  local_prev_hash TEXT NOT NULL,
  local_hash TEXT NOT NULL,
  local_signature TEXT
)
```

The receipt-table version is cleaner:

- origin fact remains immutable and verifiable
- local ledger remains append-only and ordered
- re-import is idempotent by `fact.uid`
- quarantine can hold origin packages whose signature fails

Update Sync language:

```text
Import preserves the origin fact hash/signature and appends a local receipt into this
Cell's chain. The quantity cache is bumped only once per origin fact uid.
```

---

## 5. Promise roles are overloaded

### The tension

Promise has:

- `record_uid`: local target record
- `concept_uid`: concept-level target for open/cross-organ promises
- `party_uid`: "who keeps it"
- `delta`

But worked transfers need more roles than that. In a sale, one person's account decreases,
another person's account increases, one person gives the bike, another receives it. The
single `party_uid` cannot clearly represent obligor, beneficiary, owner of the target
record, and visible counterparty.

### Why it matters

Transfer settlement, matching, balance checks, visibility, and trust all depend on role
clarity.

Questions the current model does not answer cleanly:

- Who is obligated to keep the promise?
- Whose record is changed?
- Who receives the benefit?
- Who is allowed to see the private source record?
- Whose kept/broken history feeds confidence?
- Who can settle or dispute the promise?

### Proposed repair

Split promise roles:

```sql
promise (
  uid,
  target_record_uid,
  target_concept_uid,
  delta,
  obligor_uid,      -- who is expected to perform/keep it
  beneficiary_uid,  -- who receives value, optional
  owner_uid,        -- owner of target record when relevant, optional/derived
  transfer_uid,
  state,
  condition,
  window_start,
  window_end,
  reserve_from,
  signature
)
```

For local-only promises, `target_record_uid` may imply owner. For cross-organ open promises,
`target_concept_uid` plus place/window/details lets Senses match without revealing private
records.

Then describe common shapes:

```text
Need publication:
- target_concept_uid=@apple
- delta=-3
- obligor_uid=NULL
- beneficiary_uid=@ana
- state=open

Contribution offer:
- target_concept_uid=@apple
- delta=+3
- obligor_uid=@bruno
- beneficiary_uid=NULL or matched later
- state=open

Settlement against local inventory:
- target_record_uid=@apples.stock
- delta=-3
- obligor_uid=@ana
- beneficiary_uid=@bruno
```

This makes Senses, Transfer, and Trust much less ambiguous.

---

## 6. Promise/place logistics assumes fields that do not exist

### The tension

Place says `record.place_uid` and promise windows together give logistics: a delivery is a
promise with a window and two places. Senses scores promises using `a.place` and `b.place`.

But Promise has no place fields. Concept-level open promises may not have a record either,
so they cannot inherit `record.place_uid`.

### Why it matters

Transport, delivery, ride matching, neighborhood visibility, Senses scoring, and route
overlap depend on promise-level place. A record's place is not enough:

- The source place and destination place may differ.
- A record may live at home, but delivery may happen at work.
- A ride promise needs origin and destination.
- An open concept-level promise needs location without exposing a private record.

### Proposed repair

Add place roles to Promise, either directly:

```sql
promise (
  ...
  from_place_uid TEXT REFERENCES place(uid),
  to_place_uid   TEXT REFERENCES place(uid),
  at_place_uid   TEXT REFERENCES place(uid)
)
```

Or use typed links:

```text
promise @from_place place
promise @to_place place
promise @at_place place
```

Direct fields are better for core logistics and Senses performance. Typed links are more
general but make the core matcher harder.

Recommended compromise:

- Direct `from_place_uid` and `to_place_uid` on Promise.
- `at_place_uid` can be represented by setting both equal or allowing either from/to to be null.
- Record place remains the default when promise place is omitted.

Add fallback semantics:

```text
Promise place resolution:
1. use promise from/to place when present
2. otherwise inherit target record place
3. otherwise match only on concept/window/trust, with place score unknown
```

---

## 7. Transfer settlement says "only Record mutation" but mutates promises

### The tension

The settlement section is titled "idempotent, the only Record mutation." The pseudocode
appends facts, sets promises to `Kept`, triggers conditional promises, and withdraws
siblings.

That is not wrong behavior, but the heading is imprecise. It is not the only mutation in
settlement; it is the only record quantity mutation.

### Why it matters

This matters because the settlement code is a high-trust path. It needs to be very clear
which writes are allowed:

- quantity facts
- promise state transitions
- annotation facts for confirmation
- conditional promise activation
- sibling withdrawal from satiation
- maybe transfer agreement/status derivation

If the invariant is phrased incorrectly, future contributors may either over-restrict
settlement or accidentally bypass the intended provenance model.

### Proposed repair

Rename VIII.3:

```text
Settlement: idempotent, the only quantity mutation
```

Then define the allowed settlement write set:

```text
Settlement may only:
1. append settlement facts for due active promises
2. transition those promises active -> kept
3. write settlement confirmation annotation facts when required
4. activate conditional downstream promises whose conditions now hold
5. withdraw sibling duplicated transfers/promises under satiation policy

Settlement may not:
1. create new parties
2. invent new promises
3. alter transfer economics
4. bypass agreement policy
5. mutate record.quantity except through append_all
```

This keeps the "settlement is narrow" doctrine but makes it operational.

---

## 8. Effect identity is muddy

### The tension

Part VI says rule/signal/effect are records with sidecars. But the schema has `effect_queue`
as a queue table, not an effect record sidecar. Consequence execution says command/query/action
results are logged as facts on the effect's record, but no effect record exists in the schema.

### Why it matters

Effects need provenance, retries, status, permissions, and audit. There are two coherent
models:

1. Effects are records: reusable named external side effects with activation, visibility,
   and configuration.
2. Effects are queue entries: one-off execution jobs caused by rules/actions.

The current text mixes both.

### Proposed repair

Pick one primary model.

Recommended model:

- **Effect definitions are Records** when they are reusable configured capabilities.
- **Effect queue entries are execution attempts** generated by rule/action firing.

Schema sketch:

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

Then clarify logging:

```text
If an effect definition record exists, execution results are annotation facts on that
effect record and include `origin_uid`. If the effect is inline, results are annotation
facts on the originating rule/action subject.
```

This also makes device/channel notification config fit naturally as either effect records
or device records referenced by notify effects.

---

## 9. Messages are a forced core addition but lack a primitive

### The tension

The Window says triage forced exactly four core additions, including
messages-attach-to-anything. Transfer also depends on `messages where subject = t_uid`.
But there is no message schema, no record kind, no Protein source/include, and no interaction
section for messages.

### Why it matters

Messages touch several sensitive areas:

- visibility
- sync
- threading
- attachments/media
- retention
- moderation/blocking
- transfer negotiation
- social posts/comments
- call invites

If messages are core, they need a minimal primitive. If they are just records, that needs
to be said explicitly.

### Proposed repair

Add a short Part for Messages, or fold into XV/VII with a schema.

Recommended primitive:

```sql
CREATE TABLE message (
  uid TEXT PRIMARY KEY,
  subject_uid TEXT NOT NULL,      -- record or primitive subject, depending visibility model
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

Message doctrine:

```text
Messages attach to any user-addressable Record by default. They inherit subject visibility
unless a stricter message-level visibility rule exists. Message edits are text CRDT updates
or append-only revisions, but they do not change record.quantity. Calls are messages/events
plus ephemeral lanes for live signaling.
```

Protein:

```json
{ "include": { "messages": { "limit": 50 } } }
```

Action:

```text
message: post-message, edit-message, delete-message
```

This makes chat/calls/social/transfer negotiation depend on one primitive instead of hidden
assumptions.

---

## 10. Concept fallback semantics are vague

### The tension

Lingua says an engine that does not know `@blocks-softly` treats it as parent `@blocks`.
But concepts are rows in the local database and concept packages preserve uid/lineage. It is
unclear what "does not know" means:

- the concept row is absent
- the concept row exists but the instinct function is unknown
- the concept is known but not trusted/adopted
- the concept has multiple parents
- the parent chain is absent or partially imported

### Why it matters

Fallback semantics affect Senses matching, Protein filters, rule evaluation, and package
import. A vague fallback can cause false matches, surprising automation, or silent semantic
downgrades.

### Proposed repair

Define four states:

```text
known: concept row exists and is adopted
carried: concept row exists only because an imported package brought it
unknown: uid/name appears but no row exists
unimplemented: concept row exists, but local engine has no function for its instinct
```

Then define fallback:

```text
Fallback applies only to known/carried concepts with an imported parent chain. It is allowed
for matching and display, but not for executing instinct functions unless the parent function
explicitly accepts the child. Unknown concepts do not fallback; they are unresolved.
```

For multiple parents:

```text
Fallback may widen to any parent for search/matching. For rule execution, ambiguity is a
save-time error unless the rule names the parent explicitly.
```

For adoption:

```text
Import can carry unknown concepts as inert rows. They become adopted only after user approval
or a trust policy. Senses may score carried concepts lower than adopted concepts.
```

This preserves Lingua's social/forkable model while avoiding silent behavior changes.

---

## 11. Visibility defaults need a subject/object model

### The tension

Visibility says default hidden and most-specific rule wins, with subject kinds actor, role,
organ, public, fiote. But the rest of the blueprint has multiple possible subjects and
targets:

- local user
- local Cell
- remote Organ
- person record
- Fiote agent
- sandbox host
- public
- role

Targets can be records, facts, promises, messages, primitive rows, fields, attachments, or
included rows.

### Why it matters

Visibility is the single read gate. If subject/target semantics are loose, package export,
remote Protein, Senses discovery, and sandbox access will disagree.

### Proposed repair

Define visibility as a request context:

```text
VisibilitySubject =
  LocalUser(actor_uid)
  RemoteOrgan(organ_uid)
  RemoteActor(actor_uid, via_organ_uid)
  Sandbox(sand_uid)
  Fiote(agent_uid)
  Public
```

Define target inheritance:

```text
Record visibility is primary.
Primitive rows inherit from their owning Record unless explicitly overridden.
Facts inherit from fact.record_uid.
Promises inherit from target record, transfer, or concept publication rule.
Messages inherit from subject record.
Attachments inherit from their owner row.
```

Define most-specific ordering with examples:

```text
field override > row override > owner record inheritance
actor > role > organ > public
explicit hidden beats visible at same specificity
default hidden for non-local subjects
local owner can always read unless explicitly locked by local privacy mode
```

This turns the visibility doctrine into an enforceable policy.

---

## 12. Dependency order puts Trust after Transfer, but Transfer theory depends on Trust

### The tension

Build order puts Transfer at Stage 4 and Trust at Stage 7. But Transfer's theory says signed
promises and settlement facts are the verifiable good, settlement facts are signed, and Senses
uses confidence/trust scoring. The Donation/Sale acceptance also mentions a second Cell, where
authorship and import verification matter.

### Why it matters

If Stage 4 Transfer is expected to work across two Cells, it needs some minimal trust/key model.
If Trust is deferred to Stage 7, Stage 4 can only be local or explicitly unverified.

### Proposed repair

Split Trust into two layers:

```text
Trust A: identity and signature substrate
- key table
- sign facts/promises
- verify imported rows
- quarantine invalid packages
- required before cross-Cell Transfer

Trust B: verifiable aggregates and reputation-like views
- kept ratios
- leaderboards
- verified aggregate Protein filters
- Stage 7
```

Then update build order:

- Stage 4 Transfer can include local transfer and unsigned test transfer.
- Cross-Cell donation/sale requires Trust A.
- Stage 7 remains advanced trust analytics, not basic signatures.

This removes a dependency contradiction without pulling reputation work earlier.

---

## Recommended doctrine patch

A concise replacement for the root doctrine could be:

```text
Everything user-addressable is a Record.
Every quantity change is a Fact.
Every future, conditional, or social intended quantity change is a Promise.
Every immediate write is an Action.
Every human choice is a Decision.
```

And the core invariants:

```text
1. `record.quantity` has exactly one writer: `append`.
2. All non-quantity semantic writes go through Actions or named engine processes.
3. Primitive rows inherit visibility from their owning Record unless explicitly overridden.
4. Remote facts preserve origin signatures and receive local import receipts.
5. Promises name roles explicitly: target, obligor, beneficiary, places, window.
```

These changes keep the blueprint's main architecture while removing the places where the
theory currently over-promises or compresses different kinds of state into one word.
