# New-version capabilities and maneirisms

The living doc — manual, theory, and tracker in one — as a single checkbox
system. Checked = implemented and tested; unchecked = not yet. Ordered core
primitives first, board/sand infrastructure next, individual sands last;
within a section, checked items before unchecked. There is no separate
"tasks" list anymore: land something → check its box → compress its bullet
down to the maneirism (drop the history, keep the behavior you need to know
to not fight the system). This file is always the current truth, not a
changelog.

**Where we stand:** the legacy layer (`persistence`, the old karma/views/tui/
gui, the FullUi server) is gone. Everything runs on
`nucleus → store → engine → protein → transport`.

**Working conventions** (standing rules, not checkable work): work on `dev`,
no worktrees; `cargo check`, never a full build; narrowest per-crate tests;
never alter past migrations (new ones are fine); sands read Protein and write
Actions only; board chrome stays host state, never a Ledger fact; keep
vendored license/credit files when touching embed-honest sands (terminal,
freedoom, document viewer).

---

## [x] Wire protocol — how a sand talks to the Cell

- [x] One WebSocket (`/host/transport/ws`), multiplexed: Protein (reads) +
  Actions (writes) + ephemeral lanes (presence/cursors/events) + explicit
  host capabilities (e.g. a terminal PTY session) whose bytes don't belong in
  the Ledger.
- [x] Actions are JSON with a kebab-case `"action"` tag, snake_case
  everywhere else; Protein predicates/includes are snake_case too.
- [x] A subscription answers with a snapshot then re-executes and pushes on
  every relevant commit; invalidation is coarse-by-source — render
  idempotently, a sand may get refreshes it doesn't strictly need.
- [x] Action responses carry `created`, `facts` (what the Ledger committed,
  including any Karma cascade), and `warnings` (non-fatal advisories) — show
  warnings, never treat them as errors.
- [x] Ephemeral-lane and host-capability traffic (cursors, clicks, presence,
  PTY bytes) is never persisted; terminal PTYs are scoped to one connection
  and die with it.

## [x] Records and the Ledger — the ground truth

- [x] Everything is a record (tasks, rules, signals, transfers, decisions,
  organs, people, saved Proteins, threads, messages); every change is a
  hash-chained fact, signed when a signer is set.
- [x] Actions: `create-record`, `set-quantity`, `add-quantity`,
  `edit-record-text`, `set-slug`, `set-concept`, `set-unit`, `set-place`,
  `set-extension`, `activate`/`deactivate`, `delete-record`.
- [x] Undo = `compensate { fact }` — appends the inverse delta with
  `cause=compensation`; there is no destructive undo.
- [x] Any sand adds `include: { facts: { limit: N } }` for a free "why did
  this change" drawer (delta, at, cause_kind, cause, actor).
- [x] Checkpoints snapshot levels; compaction folds pre-checkpoint history
  into a cold, hash-anchored archive; retention horizon is per record-kind,
  no policy = keep forever.
- [x] `quantity` is a cache, the fact is truth — negative = Need, positive =
  Contribution, zero = peace; activation of rules/transfers/signals/sands is
  the same knob.
- [x] Deactivate and delete are different: `deactivate` (quantity → 0) keeps
  the record on every read surface (an honest zero-quantity column);
  `delete-record` HARD-tombstones it off every read path (Proteins,
  resolve, rule inputs, checkpoints) and frees its slug — the Ledger stays
  untouched, facts and hash chain remain, with a final zero-delta
  annotation recording the deletion.
- [x] Metadata edits drop zero-delta annotation facts so live subscriptions
  refresh.
- [x] Re-appending a fact whose uid already exists is a silent no-op (replay
  safety); slugs are optional `dot.case` local sugar, uids are identity —
  Actions accept either.

## [x] Lingua — concepts

- [x] `create-concept`, `adopt-concepts` (foreign concepts keep their uid and
  lineage, re-adoption is a no-op), `declare-equivalence` (cross-dialect
  same-ness).
- [x] The parent DAG powers widening (`concept_in: "food"` matches through
  the DAG) and dialect fallback (`nearest_ancestor_in` treats an unknown
  concept as its nearest known ancestor).
- [x] Unit conversion: one authoritative `concept_conversion` row per
  unordered pair, the reverse direction derived as `1/factor`, only honored
  within a shared ancestor dimension.
- [x] Concepts travel automatically inside sync packages — a record arrives
  already understandable.

## [x] Links — the graph

- [x] `add-link`, `remove-link`, `relink-order` (drag-reorder sugar);
  identity is the triple (from, kind, to) — the same two records carry many
  link kinds.
- [x] Adding an order-like link (`precedes`/`before`/`order`/descendants)
  that closes a loop succeeds but warns; non-order kinds never warn (mutual
  recipes are legal).
- [x] Protein `include: { links: { kinds, direction, depth } }` BFS-expands
  and stamps each link with its `hop`; `order: [{ topo: "before" }]` gives
  the focus queue.
- [x] Tags/clusters are links (`linked_to: { kind: "tag", to }`), composing
  with `any`/`not`/`all` for include+exclude filtering.

## [x] Promises — the social atom

- [x] `create-promise` (including OPEN Needs/Contributions with a known
  proposer and an unfilled counterparty),
  `promise-transition` (validated state machine: open → proposed → agreed →
  active → kept/broken/withdrawn), and standalone `edit-promise-delta`.
  Bundled promise edits use a complete signed Transfer revision and invalidate
  current agreement atomically.
- [x] Expiry is automatic each heartbeat: agreed/active past-window →
  broken (and enqueues an expiry decision); open/proposed past-window →
  withdrawn, quietly. Sands never write their own deadline logic.
- [x] Reservation: `reserve_from` per promise, else the bundle transfer's
  `reserve_default`, else `active`; `include: { availability: true }`
  returns `available` and `planned`.

## [x] Karma — automation

- [x] A rule is a record (condition/gate/carry/debounce sidecar) plus
  consequences, with full math conditions over live tokens: `@x`,
  `quantity()`, `sum[_pos/_neg](x, window)`, `freq()`, `signal()`, `value()`,
  `promise_state()`, `confidence()`, `projected()`, `hours_since_fact()`,
  `distance()`, `demand()` (`route_eta` parses and errors cleanly pending
  OSM data).
- [x] Consequences: `set_quantity`, `add_quantity`, `emit_promise`,
  `run_command`, `run_query`, `run_action`, `set_visibility`,
  `activate`/`deactivate` (works on rules too — quiet hours is just a rule
  turning another rule off), `ask`, `notify` (budgeted). The legacy
  `advance_transfer` consequence is recognized but cannot activate promises
  until Phase 4 provides occurrence-aware activation.
- [x] `create-rule`/`update-rule` reload the registry and return Proof
  warnings in `outcome.warnings` when a save closes a loop — save still
  succeeds, show the warning.
- [x] `create-signal` (command/http/sensor/query source on a schedule,
  samples land as facts and cascade like any change), `create-frequency`
  (day-of-week and catch-up supported).
- [x] Delivery is reactive (only rules reading a changed record
  re-evaluate; cascades cap at 256 per delivery, a runaway loop survives);
  `debounce` holds a rule for a period after it fires (in-memory, resets on
  reload); a rule with zero consequences is a named derived value, read via
  `value(@rules.x)`; effects run outside evaluation from a durable queue and
  log a zero-delta provenance fact.
- [ ] Karma divergence heuristic — a static Proof-loop refinement; the
  256-per-delivery cascade cap is today's runtime guard.
- Learned recurrence and Transfer-aware automation are centralized in Transfer
  Phase T5: manual Transfer semantics ship first, then Karma reuses their typed
  Actions without bypassing social authority.

## [x] Protein — the read contract

- [x] Six sources, one JSON shape: `record` (state vector, all
  predicates/includes), `promise` (`state_in`), `decision` (the open
  Decision Queue, never exported to remote subjects), `fact` (the Ledger
  itself — `at_since`, `cause_kind_eq`, `record_eq`, `concept_in`),
  `concept` (Lingua vocabulary), `transfer` (bundles with derived status,
  parties, promises, balance).
- [x] Predicates: `all/any/not`, `quantity_lt/lte/gt/gte/eq`, `uid_eq`,
  `kind_eq`, `slug_eq`, `concept_in` (DAG-aware), `linked_to`, `state_in`,
  `near`.
- [x] Includes: `facts` (provenance), `promises`, `links` (kinds, direction,
  depth + hop), `threads` (nested messages), `extension`, `availability`,
  `projection` (`{at:"+7d"}` folds agreed/active promise deltas — full rule
  simulation is the engine-side `project`/`snapshot` pair).
- [x] Aggregation (`sum`/`count` by concept/kind on records, by
  cause_kind/day/concept on facts) — the visibility gate applies BEFORE
  aggregation, hidden rows can't leak through sums.
- [x] Saved Proteins are records (`kind='protein'`) referenced by slug — the
  old "view" concept, done right.
- [x] Maneirisms: the wire `where` is a JSON array = implicit `all`;
  fact-source predicates don't nest (flat list) in v1; `at_since: "30d"`
  resolves against wall-clock now (use absolute RFC3339 for reproducible
  reads); remote subjects see only whole-row visibility grants — the
  Decision Queue and concept-level promises never leave a Cell through
  Protein.
- [ ] **Verifiable aggregates**: a `verified: true` Protein filter
  restricting an aggregate to signed facts only, so a number like "@maria's
  kept-promise ratio for @food, last 12 months" is *provable* to a
  counterparty without either side trusting the computing Cell (leans on
  Trust below). Opt-in only, between mutually-confiding organs — never a
  global or public score.

## [ ] Transfers — manual-first product, then Karma + recommendations

**Scope and architecture:** this is the one tracker for Transfer engine gaps,
network delivery, the Transfer sand, integrations, and acceptance. A Transfer
is a bundle of promises whose status is derived; the sand does not maintain a
second transfer model. It reads `source: "transfer"` plus narrowly scoped
record/promise/fact/thread Proteins and writes typed Actions over the shared
WebSocket. Board layout, selected transfer, filters, and panel state are host
state. Durable title, parties, promises, agreement, visibility, confirmations,
messages, settlement evidence, and behavior policies are Cell data. Cell-wide
defaults use the typed singleton `configuration` row; a transfer/tree override
uses typed transfer sidecar state, with precedence `transfer > Cell default >
code default`. Policies are read/written through Protein/Actions and never live
only in JavaScript or board state. The old Transfer sand is only a behavior
inventory: none of its HTTP endpoints, private DTOs, duplicated messages, event
log, visibility engine, or client-side projections return.

Phases 0-9 deliberately ship the complete human-driven workflow first.
Transfer-aware Karma and learned recommendations are not removed: they are the
Phase 10 integration after the manual semantics and evidence are trustworthy.
An explicit user-authored rule may eventually automate permitted local Actions,
but no rule or recommendation can forge another person's agreement or claim
that a real-world event happened. External payment execution, carrier APIs,
legal-contract machinery, and Fiote are out; payment/contracts may travel as
links or messages. Promise time windows and calendar constraints are core even
though external calendar-provider integration is not.

### Authoritative implementation sequence

This sequence replaces the earlier T0-T5 grouping. Phases are dependency gates,
not parallel workstreams: do not expose a control from a later phase merely
because an older Action happens to exist. Each phase updates Action, storage,
Protein, live invalidation, sand behavior, and focused proof together. The next
phase starts only after the prior exit gate is true.

The existing implementation is useful groundwork, not proof that later
semantics are complete. In particular, the legacy aggregate confirmation and
settlement behavior must not be treated as the final occurrence model.

#### Phase 0 — semantic and authority foundation (complete)

**Prerequisite:** the Standing Transfer decisions below are frozen.

Implement this phase in the order shown: identity and authority, revision and
replay kernel, truthful participant/invitation storage, then boundary lockdown
and projection. A checked later item does not waive an unchecked earlier item.

- [x] Represent social actors as Person records and bind authenticated
  `app_user` identities to Persons explicitly. Never infer the binding from
  usernames, names, or slugs.
- [x] Require `transfer:create`/`transfer:update` plus creator/participant
  relationship, derive the viewer from the WebSocket subject, and preserve
  trusted local no-auth mode explicitly.
- [x] Provide typed, validated, atomic whole-draft creation with one signed Fact
  and no partial party/promise rows.
- [x] Add a persistent monotonically increasing Transfer revision. Every
  public-result mutation carries `expected_revision`; stale writes fail with
  the current revision before changing any row.
- [x] Add an idempotency key to every enabled multi-row Transfer mutation so
  retrying a committed request returns the original result without another
  revision or Fact.
- [x] Commit promise term mutation, revision increment, agreement invalidation,
  immutable revision snapshot, signed annotation Fact, and live
  invalidation in one transaction.
- [x] Model addressed people separately as invitations with
  `pending|accepted|rejected|withdrawn|expired`. Only the creator and accepted
  invitees are `transfer_party` participants; selecting a Person during
  creation must not bind them.
- [x] Keep invitation response mutations closed until the authenticated Actions
  in Phase 2 exist. The persistence API never accepts an acting identity; that
  authority must be derived by the engine, and creation can only address a
  pending invitation.
- [x] Bind agreement rows and promise term rows to the revision they authorize.
  A later revision cannot inherit an earlier signature accidentally.
- [x] Type every currently enabled Transfer value at the Action boundary:
  agreement/percentage, visibility/proximity, satiation, reservation point,
  finite non-zero delta, future window, parsed condition, and hierarchy
  references. Unavailable invitation/agreement/execution mutations fail with a
  stable phase code instead of reaching legacy stringly persistence.
- [x] Gate Protein capabilities conservatively: creation may be enabled now;
  edit/invite/review/commit/activate/confirm/settle remain false until their
  phase exit gate is satisfied.

**Exit gate:** an authenticated client cannot name another acting Person
(trusted local no-auth mode explicitly selects its Person); addressed people
are not participants before acceptance; stale/replayed/invalid mutations land
no partial rows; Protein exposes revision, viewer identity, capability, and
stable blocking reasons.

#### Phase 1 — truthful draft round-trip (complete)

**Prerequisite:** Phase 0 exit gate.

- [x] Use one revision-safe `revise-transfer-draft` Action, not independent
  field Actions, to edit title/slug, agreement settings,
  reservation/confirmation policy, visibility/proximity, parent/source, and the
  complete promise surface: Record, Person/OPEN slot, canonical delta/unit,
  window, condition, reservation point, location, and OPEN reuse policy. It
  carries the complete reviewed draft, `expected_revision`, and `request_id`.
- [x] Treat initial creation as immediate addressing: revision 1 creates pending
  invitations and makes the addressed proposal visible. There is no separate
  unsent frontend-only social draft.
- [x] Add revision-safe remove/withdraw operations for invitations and
  uncommitted promises. Preserve revision history and Facts; do not hard-delete
  evidence.
- [x] Make every time/window change invalidate revision-bound agreement levels.
  Invitation withdrawal also advances revision. Phase 2 acceptance must do the
  same; rejection and expiry remain signed lifecycle evidence without changing
  terms revision.
- [x] Require explicit review-and-adopt to turn a legacy revision-0 Transfer
  into revision 1. Never silently seal old partial terms on first edit.
- [x] Complete Transfer Protein predicates for revision, status, viewer role,
  invitation state, Person, Record/concept/unit, and window. Unsupported
  predicates return a typed error instead of matching everything.
- [x] Add deterministic server ordering for inbox/list queries and make every
  draft-side mutation publish live invalidation.
- [x] Project current terms, prior revision summary, signed change Fact,
  capabilities, blockers, hierarchy labels, and source Record labels.
- [x] Update the creation/edit sand to round-trip the server model without
  storing durable terms or arithmetic in browser/board state.
- [x] Add blank, Record-prefilled, multi-Record, and OPEN-promise draft entry
  points using the same Action contract.
- [x] Snapshot an explicit canonical unit on every promise instead of deriving
  signed meaning from the Record's mutable current unit.
- [x] Add an optional Transfer default location plus per-promise overrides;
  time windows and locations remain core and do not wait for providers.
- [x] Model an OPEN promise as a signed reusable template owned by its proposer
  with an unfilled counterparty, Record,
  delta/direction, unit, window, location, and `duplicate|consume` policy
  (`duplicate` default). Phase 2 claiming may copy or consume it and the other
  Person may change any copied term, but that refinement is a newly signed
  counteroffer linked to the source template.

Phase 1 implementation boundaries:

- `create-transfer-draft`, `revise-transfer-draft`, and
  `adopt-transfer-draft` share the same complete draft terms. Revision and
  adoption carry `expected_revision`/`request_id` as appropriate; omission of
  pending invitations or editable promises signs withdrawal rather than
  deleting rows. An empty reviewed promise set is the complete-draft withdraw
  operation and is never accepted for initial creation.
- The creator is an immutable Person marked as `kind=creator` in the signed
  party snapshot. Legacy adoption explicitly chooses and seals that marker;
  authenticated sessions must match their server-derived Person.
- The sand accepts `transferCreate` with `record` or `records` and optional
  `open=true`; blank, one-Record, multi-Record, and OPEN-prefilled paths all
  end in the same draft Action. Units come from the Concept Protein and the
  selected concept UID is copied into the signed promise snapshot.
- A stale editor may load the newer signed revision or explicitly replace its
  editable public terms. The UI never calls a complete-snapshot replacement a
  merge, and blocks replacement when a retained local promise is no longer
  draft-editable. Unchanged expired deadlines may round-trip; a changed or new
  deadline must be in the future.
- Transfer creation/revision Facts invalidate Protein subscriptions directly
  but deliberately do not enter the generic Karma cascade. Transfer-aware
  recurrence, recommendations, and automation remain Phase 10 work.
- Phase 1 edit/adopt capability is false once any promise has left the
  OPEN/proposed/agreed/withdrawn draft surface. Unsupported Transfer Protein
  predicates and ordering return stable `protein_*` wire error codes.

**Exit gate:** a creator can create, reload, edit, and withdraw a complete draft
through Actions/Protein; a second open sand converges; stale and replayed writes
behave deterministically.

#### Phase 2 — invitation and counteroffer (implemented; verification pending)

**Prerequisite:** Phase 1 exit gate.

Implementation order and contracts:

1. **Invitation lifecycle.** Add revision-safe address, accept, creator-withdraw,
   and reopen Actions. Reopen preserves the invitation UID, increments an
   attempt number, and appends a signed lifecycle event. Acceptance creates the
   addressed Person's level-0 participant row in the same transaction and
   advances the canonical terms revision. Rejection and automatic expiry append
   signed lifecycle events but do not change the terms revision or create a
   party. Every mutation is request-idempotent and actor-authorized.
2. **Canonical counteroffers.** An accepted participant may submit the same
   complete draft contract as the creator. The signed counteroffer immediately
   becomes the one current proposed revision; there are no branches. It carries
   `expected_revision`, exposes the signed changed-field diff, retains the
   immutable creator, cannot alter invitation lifecycle, and resets all
   revision-bound agreement levels.
3. **OPEN claim.** An OPEN template always names its proposer; only its
   counterparty is unfilled. Claiming it atomically creates the claimant's
   level-0 participant row and a concrete two-Person counteroffer with one
   promise per Person. A `duplicate` claim keeps the signed source template and
   creates a linked concrete pair; a `consume` claim closes the template and
   materializes the pair. The claimant may refine the copied public terms, but
   cannot silently reverse who proposed to give/receive: the two resulting
   deltas remain opposite and both People must sign the new agreement levels.
   Unknown non-contact People may enter only through a directly addressed
   invitation or a public OPEN promise.
4. **Negotiation surface.** Project invitation attempts/events, counteroffer
   evidence, OPEN-claim capability, and generic threads attached to the
   Transfer Record. The Transfer sand exposes inbox decisions, counteroffer
   editing, OPEN claim/refinement, and generic thread/message Actions; it does
   not introduce Transfer-private chat storage. Dirty local edits remain local
   when a live revision arrives until the Person explicitly reviews or replaces
   the newer complete revision.

- [x] Address Person invitations with expiry and explicit visibility; accept
  first, reject, withdraw, or reopen them.
- [x] Claim a proposer-owned OPEN slot into a concrete opposite-promise pair.
- [x] On acceptance, create the participant row and revision-bound party
  evidence atomically. Rejection/expiry never binds the Person.
- [x] Add full counteroffers with `expected_revision`, signed changed-term
  diff, deterministic agreement invalidation, and visible stale-write recovery.
- [x] Preserve OPEN proposer direction and materialize both proposer and
  claimant promises when the claimant signs refined terms.
- [x] Preserve dirty local edits when a live revision arrives; show the
  intervening signed diff and require explicit reapply/edit.
- [x] Support unknown non-contact identities only when directly addressed or
  through a public OPEN proposal; they gain no ambient sync or contact entry.
- [x] Add generic negotiation threads/messages without a Transfer-private chat
  model.

Implemented architecture:

- Migration `0015_transfer_negotiation.sql` keeps one invitation UID across
  numbered attempts, stores append-only signed lifecycle-event references, and
  links duplicated promises to their OPEN source. Automatic expiry has no
  fabricated Person actor.
- Migration `0021_open_proposal_ownership.sql` makes OPEN ownership explicit:
  the signer is the proposer and only the counterparty is unfilled. Legacy
  ownerless OPEN rows are backfilled only when one creator is unambiguous;
  otherwise migration aborts instead of guessing an identity. Immutable claim
  pairs retain source, proposer, claimant, concrete promises, reuse policy,
  signed revision, request id, and time.
- Typed Actions enforce creator/addressee/participant authority, request replay
  before mutable-state validation, revision compare-and-swap, accept-first for
  addressed claimants, and public visibility for unknown OPEN claimants.
  Revision and lifecycle Facts are published without Karma; heartbeat closes
  due invitations.
- Protein projects current lifecycle state separately from historical signed
  revision terms, groups events by attempt, exposes source provenance and
  negotiation threads, and supplies authority-aware capabilities and blockers.
- The Transfer sand keeps the list as the big-picture surface and the selected
  Transfer as the high-control surface. It provides lifecycle decisions,
  complete canonical counteroffers, refine-and-sign OPEN claims, and generic
  thread/message controls. Dirty edits survive live updates until explicit
  review or replacement.

**Exit gate:** two identities can invite, accept, reject, counteroffer, hit a
stale precondition, recover, and converge on one visible revision.

#### Phase 3 — signed agreement and commitment (implemented; verification pending)

**Prerequisite:** Phase 2 exit gate.

Implementation order and contracts:

1. **Signed level transitions.** Replace the locked legacy mutation with a
   request-idempotent, revision-CAS Action authored by exactly one participant
   Person. Level `0` is no agreement, level `1` is **Checked · ready to
   agree**, and level `2` is **Agreed**. Ascending and descending transitions
   append immutable Facts; the mutable agreement row is only the current cache.
   A Person may move only their own level, one adjacent milestone at a time.
2. **Revision and promise sensitivity.** Every transition names the exact
   signed terms revision. Public-result revision changes reset the current
   cache to level 0 while prior evidence remains auditable. Reaching level 2
   advances only that Person's proposed promises; moving backward returns only
   that Person's still-unactivated agreed promises to proposed.
3. **Policy readiness.** Derive blockers and later-stage readiness per promise,
   Person, and Transfer. `individual` matches the relevant give/receive path;
   `full` requires every signed party; `percentage` atomically freezes the
   first committed quorum coalition for that revision; excluded parties cannot
   activate or settle. No signature counts for another Person.
4. **Structured dependencies.** Store dependency terms in the signed Transfer
   revision rather than frontend state. A promise-scoped dependency blocks only
   that promise; a Transfer-scoped dependency blocks every promise in the
   Transfer. Each explicitly names an upstream promise or Transfer and required
   state, defaulting to `kept`.
5. **Surfaces and negotiation authority.** Protein projects signed level
   history, frozen coalition, readiness, and exact blockers. The sand offers
   only server-authorized forward/back controls and waits for live projection.
   Generic negotiation threads remain readable under Transfer visibility, but
   writing is restricted to creator, accepted parties, and pending addressees.

- [ ] Verify level 1 review and level 2 commitment signatures for one Person and
  one exact revision; sidecar state is a cache of immutable evidence.
- [ ] Verify matched relevant levels are required before later stages unlock.
- [ ] Verify `individual`, `full`, frozen `percentage` coalition, and
  named `dependency` gates without one Person's signature standing for
  another.
- [ ] Verify only the agreeing Person's promises advance and expose who blocks each
  agreement path.
- [ ] Verify affected agreements invalidate atomically for every public-result edit;
  private formula/notes/display changes do not invalidate public agreement.
- [ ] Verify review/commit controls come only from server capabilities and wait for live
  Protein before showing success.

Implemented architecture:

- Migration `0016_transfer_agreement.sql` adds append-only agreement events,
  revision-frozen percentage coalitions, shared Transfer request-id collision
  guards, and structured revision-owned dependencies. Pre-Phase-3 unsigned
  agreement caches are reset instead of being treated as evidence.
- `set-transfer-agreement-level` derives the acting Person, accepts only
  adjacent transitions, verifies the exact revision Fact, requires that
  Person's installed signer, and commits the Fact, event, cache, owned promise
  state, and first percentage quorum atomically. Backward transitions retain
  history and revert only unactivated `agreed` promises.
- Agreement readiness is server-derived for each promise, Person, and Transfer.
  OPEN reusable templates are not executable commitment paths; claimed copies
  are. Transfer dependencies apply to all executable promises, while
  promise-scoped dependencies apply only to their named promise.
- Protein exposes immutable transition history, coalition membership,
  dependency state, exact blockers, and capability-gated forward/backward
  controls. Negotiation writes are limited to the creator, accepted parties,
  and non-expired pending addressees.

**Exit gate:** every agreement-policy matrix derives readiness from signed
revision-bound evidence, and no signature authorizes another Person or revision.

#### Phase 4 — availability, activation, and occurrence evidence (implemented; verification pending)

**Prerequisite:** Phase 3 exit gate.

Implementation order and contracts:

1. **Non-forgeable Person authorship.** Bind the active signing identity to the
   authenticated session and its mapped Person. Trusted local mode must
   explicitly select an available unlocked Person identity. A Cell/Organ key,
   app-user identifier, owner role, bulk command, or frontend-supplied Person
   UID cannot sign for that Person. Signer availability is explicit and
   fail-closed: Lince does not silently create a custodial Person key. Before
   settlement, authenticated WebSocket sessions use a server challenge and a
   client/OS-keystore-held Person key to sign the canonical Action intent,
   message/request id, and connection nonce. The server verifies the exact
   authenticated Person/key and rejects cross-session replay before executing
   the Action. Trusted local mode retains its explicit unlocked process signer.
2. **Directed occurrences and exchange paths.** Activating one executable
   promise creates one immutable directed occurrence with canonical subject,
   unit, quantity, giver, receiver, revision, window, and location. Exact
   opposite give/receive promises share an exchange-path UID; donations and
   unbalanced promises remain valid one-sided paths.
3. **Availability and private application policy.** Derive `actual`,
   `available`, `reserved`, `planned`, and `surplus` in the engine using
   compatible-unit conversion only. Persist reservation precedence and the
   receiving Person's private `application_formula` in Cell data; neither is
   frontend state and neither changes the signed public occurrence. The shared
   expression grammar exposes canonical incoming quantity as `incoming()`, so
   an override may be `incoming() * 2 + 1` without arbitrary JavaScript.
4. **Participant-scoped activation.** Activate only the acting Person's
   policy-ready promises for the exact current signed revision, one explicitly
   selected promise per idempotent Action. Bulk activation remains Phase 6
   labor-saving work. Agreement retraction remains signed evidence: it never
   rewrites an active/terminal occurrence, marks existing work disputed, and
   blocks only future activation.
5. **Role-specific conclusion evidence.** The occurrence giver may assert or
   correct delivery and the receiver may assert or correct receipt. Both are
   signed, request-idempotent Facts attached to that occurrence; mutual current
   claims derive **Confirmed conclusion**. Network delivery acknowledgement is
   a different transport concern.

- [x] Model each canonical give/receive occurrence explicitly before adding
  confirmation controls.
- [x] Project engine-derived `actual`, `available`, `reserved`, `planned`,
  and `surplus`, with compatible-unit conversion and explicit unknown units.
- [x] Implement reservation policy precedence
  `transfer > Cell default > code default`; incoming promises affect planned,
  not available, before settlement.
- [x] Add private deterministic `application_formula` using the shared pure
  expression grammar. Public canonical occurrence and private Record delta stay
  distinct.
- [x] Activate only policy-ready promises for the authenticated participant and
  exact revision.
- [x] Add giver and receiver claims for each occurrence. Delivery and receipt
  are role-specific, idempotent, actor-attributed, signed, and correctable.
- [x] Keep network delivery acknowledgement distinct from real-world
  fulfillment confirmation.

Implemented architecture:

- Migration `0017_transfer_occurrence.sql` adds immutable directed
  occurrences, revision-bound exchange paths, append-only activation and
  role-claim events, private receiver application policies, formula-hash audit
  events, a shared Phase-4 request-id namespace, and typed Cell reservation and
  application-formula defaults. Public occurrence evidence contains canonical
  terms and formula hashes, never the private formula text or local inventory
  values.
- `activate-transfer-occurrence` is singular and revision-CAS: it derives the
  authenticated Person, requires that exact Person's available signer, accepts
  only their current policy-ready `agreed` promise, resolves one unambiguous
  counterparty, and commits the signed Fact, occurrence, exchange path, and
  promise `active` cache atomically. Exact retries return the original
  occurrence; request-id reuse with changed intent fails.
- `set-transfer-occurrence-claim` appends signed giver-delivery or
  receiver-receipt assertions and retractions. Current booleans are projections
  of that history; mutual current claims derive **Confirmed conclusion**.
  Agreement retraction blocks later activation and annotates already-created
  occurrences as disputed without changing their evidence or active state.
- `set-transfer-occurrence-application-formula` stores receiver-only formula
  text locally and signs only its hash/version. Its validated numeric grammar
  permits finite constants, arithmetic, and zero-argument `incoming()` and is
  checked against the occurrence's canonical quantity; the sand never
  evaluates it. Resolution is occurrence override > Cell default > code
  `incoming()`. Protein exposes the text and derived local delta only to the
  receiving Person or trusted local session.
- Protein derives actual/available/reserved/planned/surplus with Lingua unit
  conversion and explicit unknown-unit buckets. Transport supplies the exact
  currently available session/process signer actor to Protein, so a public
  verification key alone never enables a signing control. Authenticated
  social writes use signed Action intents; trusted local mode uses the
  explicitly installed process signer. The Transfer sand exposes
  server-authorized activation per promise, exchange-path occurrence detail,
  role-specific claims/corrections and history, disputes, private formulas,
  and live-update waiting without maintaining durable workflow state.

**Exit gate:** activation and confirmation cannot speak for another Person;
availability and private deltas come only from the engine; every occurrence is
independently auditable.

#### Phase 5 — partial settlement and correction

**Prerequisite:** Phase 4 exit gate.

Implementation order and contracts:

1. **Session-authored intent evidence.** Authenticated sockets first prove a
   client/OS-keystore Person key against the server challenge and their mapped
   Person. Every Action is signed over the exact action bytes, connection
   challenge, message id, and monotonic sequence. The verified intent is stored
   separately from the Ledger Fact signature and linked to resulting Facts; an
   Action signature is never mislabeled as a signature over a server-created
   Fact hash. Raw authenticated `act` messages fail closed. Trusted local mode
   retains its explicit process signer.
2. **Settlement eligibility and preview.** Settlement selects one occurrence,
   requires current mutual delivery/receipt claims, rejects disputed evidence,
   derives the authenticated Person from the session, and allows only the
   owner of that occurrence's source promise to change their own Record. The
   preview freezes canonical quantity, remaining quantity, owned target Record,
   effective formula/hash/version, and resulting local delta. A proposer-owned
   OPEN template without a concrete signed counterparty pair is never
   settleable.
3. **Append-only fulfillment slices.** Each idempotent settlement appends one
   immutable slice containing canonical quantity and private local delta. The
   sum of canonical slices derives remaining progress; a full sum moves only
   that promise to `kept`, while a smaller positive sum leaves it `active` and
   derives `partially_settled`. No slice may exceed the remaining amount.
4. **Correction and remainder policy.** Private quantity mistakes use the
   existing compensation Fact path with ownership/intent checks. Social
   corrections annotate the occurrence or create a linked reversing Transfer;
   they never rewrite settlement slices. Default remainder policy leaves the
   remainder visible. Opt-in may create an unsent local draft using the exact
   remainder, but cannot address, agree, activate, confirm, or settle it.
5. **Projection and irreversible review.** Protein derives occurrence,
   exchange-path, promise, and Transfer progress plus explicit partial,
   disputed, settled, and compensated states. The sand shows the exact preview
   and requires an explicit settle command, then waits for the pushed Protein
   snapshot. Network receipt remains unrelated.

Implemented Phase 5 contracts (verification still pending):

- Authenticated WebSocket clients register a client-held, non-exportable
  Ed25519 Person key against a server challenge. `signed_act` covers the exact
  Action JSON bytes, session/challenge, message id, and monotonic sequence.
  Verified Action-intent evidence lives in `signed_action_intent` and
  `fact_action_intent`; it is never copied into `fact.signature`.
- `settle-transfer-occurrence` accepts exactly one positive canonical slice and
  compare-and-set values from the private server preview: remaining quantity,
  local delta, application formula hash/version, and remainder policy. The
  engine re-derives all of them and permits only the concrete source-promise
  owner with current mutual confirmation and no dispute.
- `transfer_occurrence_settlement_slice` stores immutable public canonical
  progress alongside the private local application. One zero-delta Transfer
  Fact records shareable evidence and one owned-Record Fact applies the private
  delta atomically. A verified Action intent may authorize both Facts without
  fabricating either Fact-hash signature.
- Giver-owned promises apply cumulative `-incoming()`. Receiver-owned promises
  apply the private occurrence override or Cell formula, defaulting to
  `incoming()`. Each slice delta is the new cumulative formula result minus the
  sum already applied, so nonlinear formulas are independent of how work is
  partitioned.
- Transfer Protein projects public slice history and canonical progress to
  authorized viewers, but exposes local Record ids, formula text, application
  Facts, and local deltas only inside the source owner's Cell. A focused
  `transfer_settlement_preview` Protein recomputes arbitrary partial amounts on
  the server; the sand never evaluates private formulas in JavaScript.
- Remainder policy is persisted in the database (`visible` by default,
  `local_draft` opt-in). Settlement freezes the effective policy. The opt-in
  does not itself send or advance a remainder draft.
- Settlement application mistakes use
  `compensate-transfer-occurrence-settlement`, never generic compensation.
  Only the original slice owner can append the inverse private Record Fact,
  and each slice can be compensated once. Public fulfillment evidence remains
  intact.
- Giver and receiver may independently assert or retract a dispute through
  signed append-only events. Their latest assertions are combined with a
  separate system-dispute bit used for agreement/revision invalidation, so a
  participant cannot clear a system safety hold or impersonate its author.
- The remaining correction commands are explicit, signed, idempotent writes
  with an exact revision/state precondition. `create-transfer-remainder-draft`
  may be used only by the source-promise owner after a partial settlement with
  the persisted `local_draft` policy; it copies only the exact remaining
  canonical promise into a hidden, creator-only draft and never addresses,
  agrees, activates, confirms, or settles it. Its source occurrence and slice
  remain immutable lineage.
- `create-reversing-transfer-draft` is creator-authorized correction evidence.
  It creates a hidden linked Transfer whose promise reverses the selected
  occurrence's full canonical quantity, while retaining both Transfers and
  their revision Facts.
  The new Transfer begins as an unsigned-workflow draft: no counterparty is
  invited or agreed and no real-world occurrence is inferred.
- `reopen-transfer-promise` never mutates a closed promise back into service.
  The promise owner creates a successor promise in a new signed Transfer
  revision, linked to the predecessor and initialized as proposed (or OPEN
  only when the same proposer explicitly requests it). All agreement levels
  reset for the new revision and previous completion/broken evidence remains
  attached to the predecessor.

- [x] Settle only the authenticated Person's authorized local Records and only
  mutually confirmed occurrences; retry is a no-op.
- [x] Represent partial/multi-party progress normally without rolling back
  completed Facts.
- [x] Persist partial-remainder policy with `visible` as the default.
- [x] Let `local_draft` create an unsent local remainder draft; it must never
  address, agree, activate, confirm, or settle that draft automatically.
- [x] Add explicit withdrawn, cancelled/rejected, expired, broken, partially
  settled, disputed, compensated/reversed, and settled derived states.
- [x] Add owner-authorized private settlement compensation and signed
  participant dispute/retraction history without rewriting fulfillment.
- [x] Complete the remaining correction paths: broken remainder, linked
  reversing Transfer, and `reopen-promise` successor revision.
- [x] Add exact irreversible-step review showing canonical occurrences,
  confirmations, formulas, and local deltas before settlement.

**Exit gate:** partial work, replay, correction, and disputes remain append-only,
per-Person, idempotent, and visible without rewriting original evidence.

#### Phase 6 — hierarchy and bulk manual work

**Prerequisite:** Phase 5 exit gate.

Implementation order and contracts:

1. **Signed topology.** `parent_uid` is the single containment edge used for
   tree navigation and roll-up. Dependencies are separate directed execution
   gates between a downstream Transfer or promise and an upstream Transfer or
   promise. Both forms are part of the reviewed revision. Creation and every
   revision resolve all references and reject missing nodes, self-edges, parent
   cycles, and dependency cycles before any revision rows are committed.
2. **Derived readiness and roll-up.** Child status, policy, revision, evidence,
   and authority remain independent. A parent reports deterministic descendant
   counts, blockers, and remaining canonical quantities grouped by
   `(concept, unit)`; incompatible or unknown units are never summed. Parent
   readiness is a projection and never advances a child's agreement or work.
   Dependency order is stable by topology and UID, and a blocked node identifies
   the exact unsatisfied edge and upstream state.
3. **Reviewed source-group satiation.** `first_completes` applies only among
   Transfers that signed the same non-null source group and the same policy.
   The first fully settled sibling produces append-only winner/loser evidence
   and blocks future activation for losing siblings. It does not erase terms,
   retract another Person's agreement, or undo occurrences/work that already
   happened; already-started conflicts remain visible for manual correction.
4. **Atomic one-Person bulk confirmation.** A focused Protein preview expands
   an explicit tree/branch selection to the acting Person's currently missing
   role claim for each occurrence: delivery when they are giver, receipt when
   they are receiver. It never activates promises, settles Records, changes
   terms, or confirms the counterparty's role. The reviewed command carries
   the exact transfer revisions and occurrence claim-state tokens plus one
   request id. The store rechecks the whole selection in one transaction; any
   stale, missing, disputed, satiated, already-completed, or unauthorized item
   aborts the entire batch and returns item-specific blockers. A successful
   command emits one individually attributable claim event and Fact per item,
   all linked to the one signed Action intent. Exact replay is a no-op.
5. **Big-picture controls.** The sand renders the hierarchy before mutation,
   supports whole-tree, branch, and individual occurrence selection, and shows
   readiness, dependencies, grouped remainder, and the exact bulk preview.
   Submission requires explicit review acknowledgement and waits for the pushed
   Protein snapshot. Narrow layouts keep the tree visible and open one branch's
   details in a dismissible inspection surface.

- [x] Implement parent/child roll-up while each child retains its revision,
  policy, status, and evidence.
- [x] Add explicit Transfer/promise order and dependency DAGs, distinct from
  condition expressions; reject cycles.
- [x] Implement sibling `first_completes` satiation against the reviewed
  source group.
- [x] Add reviewed atomic bulk completion for one Person. It emits individually
  attributable role-claim evidence, never confirms for counterparties, and
  rejects the complete selection when any reviewed item changed.
- [x] Add tree selection, blocker/readiness projection, per-branch remainder,
  and narrow/mobile inspection controls.

Implemented Phase 6 architecture (workspace check complete):

- Migration `0022_transfer_hierarchy_bulk.sql` persists immutable source-group
  results/losers and reviewed bulk request/item lineage, protects the global
  Transfer request-id namespace in both directions, and adds database-level
  self-parent guards. Store transactions additionally validate the complete
  parent chain and global typed Transfer/promise dependency DAG.
- `complete-transfer-occurrence-claims-bulk` derives the acting Person from the
  authenticated session, verifies the canonical review token, preflights every
  `(occurrence, role)` pair, and commits all individual claim Facts/events or
  none. Local self-transfers may review both roles; no action may author a
  counterparty's role. Losing or late `first_completes` siblings are blocked at
  both engine and store activation boundaries.
- Transfer Protein attaches visible-only hierarchy paths, direct children,
  descendant status/readiness, explicit dependency order and blockers,
  authoritative source-group evidence, and remainder groups keyed by canonical
  concept/unit. `transfer_bulk_completion_preview` emits the exact action items
  and token accepted by the engine without exposing hidden branches.
- Sand modules `app/hierarchy.js` and `app/bulk.js` provide list/tree views,
  root and branch inspection, responsive remainder/blocker detail, per-Person
  selection, explicit irreversible-step acknowledgement, and pushed-Protein
  result tracking. No durable Transfer term or arithmetic is held in browser
  state.

**Exit gate:** a coordinated tree executes in deterministic order; bulk labor
saves clicks without weakening individual evidence or authority.

#### Phase 7 — complete local Transfer sand

**Prerequisite:** Phases 1-6 backend controls exist. UI is added incrementally
with each phase, but this is the complete local product gate.

Implementation order and contracts:

1. **Composable inbox facets.** A Transfer has one server-derived
   `primary_status` for sorting and labeling, but inbox membership is a set of
   independent server-derived flags. `mine` is an ownership scope; it may be
   combined with exactly one or several workflow facets. `awaiting_me`,
   `awaiting_others`, `active`, `completed`, `cancelled_or_broken`, and
   `discoverable_open` may overlap when the underlying evidence makes that
   truthful. Counts use the complete visible result, not the current search.
   Trusted local mode derives `mine` from the explicitly selected/installed
   Person signer; authenticated mode derives it from the session Person.
2. **Primary status and attention.** Precedence is deterministic and favors
   actionable exceptions: system dispute, participant dispute, broken/expired,
   cancelled/rejected/withdrawn, and partially settled. Satiated and fully
   completed results resolve before pending-work labels; otherwise the order is
   awaiting me, active, awaiting others, agreed, proposed, OPEN, inactive, and
   draft.
   Facets remain visible beside this label so one status never hides another
   person's pending work. The browser never reconstructs these memberships
   from button availability.
3. **Capability-complete detail.** Creation, revision, invitation,
   counteroffer, agreement movement, activation, role confirmation, private
   formula, settlement, dispute, compensation, and the available correction
   paths render only from server capabilities and blockers. Every mutation
   enters `signing`, then `awaiting_snapshot`; success is shown only after a
   newer matching Protein projection is observed. Stale or forbidden results
   keep the reviewed input visible and offer reload/review rather than retrying
   changed terms blindly.
4. **One evidence timeline and proof model.** Protein emits a compact,
   deterministic timeline ordered by `(occurred_at, uid)` across revisions,
   invitations, party/agreement changes, promise activation, role claims,
   settlements, expiry, disputes, compensation, source-group results, and
   corrections. Each item names its Record/Transfer target, Person author when
   present, Fact, request id, revision, proof state, and relevant linked ids.
   Proof state distinguishes a direct Fact signature, a verified signed Action
   intent authorizing the Fact, unsigned system/local evidence, and missing or
   invalid proof; the UI must not label all Facts as signed.
5. **Exact local disclosure.** The detail projection includes authorized
   visibility recipients and an exact field-level disclosure preview. Public
   canonical terms/evidence, participant-only negotiation, and Cell-private
   Record quantities/formulas are separate groups. A hidden recipient or
   private formula is never revealed merely to explain that something was
   redacted. Threads remain the generic attachment/conversation surface.
6. **Resilient interaction states.** The last pushed snapshot remains readable
   while offline, but all mutations and live previews are disabled. Loading,
   empty, filtered-empty, reconnecting, stale, forbidden, validation, partial
   projection/import, warning, retry, and awaiting-snapshot states have
   distinct messages. Keyboard focus returns predictably between inbox,
   detail, drawers, and composer; narrow screens show one surface at a time
   without hiding blocker or proof information.

- [x] Provide inbox partitions: mine, invited/awaiting me, awaiting others,
  active, completed, cancelled/broken, and discoverable OPEN proposals.
- [x] Complete create/edit/invite/counteroffer/agreement/activation/
  confirmation/settlement/correction controls from server capabilities.
- [x] Show one compact timeline across revisions, parties, promises,
  confirmations, settlements, expiry, correction, and exceptional states.
- [x] Show per-Record quantities/formulas, per-concept balance, source Facts,
  proof/signature state, hierarchy, and shared Record navigation.
- [x] Complete generic threads, history/proof drawer, visibility recipients,
  and exact disclosure preview.
- [x] Cover loading, empty, offline/reconnecting, stale, forbidden, validation,
  partial import, warning, retry, keyboard, and responsive states.
- [x] Never claim mutation success before the pushed Protein snapshot.

Implemented Phase 7 architecture (workspace check complete):

- Transfer Protein owns `primary_status`, composable `inbox_facets`, capability
  and blocker maps, exact action payloads, deterministic timeline entries,
  cryptographically distinguished proof states, visibility recipients, and
  disclosure groups. Public viewers receive redacted correction lineage and do
  not receive participant-only correction or successor proof events.
- Migration `0023_transfer_correction_lineage.sql` stores correction and promise
  successor lineage without overloading hierarchy or `first_completes` source
  groups. Remainder, reversal, and reopen commands use exact revision/state
  preconditions, a shared request-id namespace, signed evidence, and exact
  replay checks.
- The sand separates overview/list/tree work from detailed inspection. Focused
  inspection modules render the timeline, proof drawer, accounting/navigation,
  disclosure/threads, and reviewed correction controls. Mutation controls are
  disabled while stale or offline and remain pending until matching pushed
  evidence appears.
- `nix develop -c cargo check --workspace --all-targets` completes with warnings
  treated as errors. Tests were intentionally not run for this phase.

**Exit gate:** the complete local manual workflow is usable without legacy
Transfer endpoints/models, client authority, or client-maintained arithmetic.

#### Phase 8 — Cell-to-Cell social delivery

**Prerequisite:** the complete local evidence model through Phase 7. This uses
the existing Organ transport; it is not third-party integration.

Implementation order and contracts:

1. **Origin authority and delivery policy.** The Cell whose Organ UID is the
   Transfer Record's origin remains the only authority that may order canonical
   Transfer revisions or execute Transfer Actions. A recipient Cell never
   promotes a replica into local Transfer sidecars. Each explicit
   `(Transfer, recipient Person, recipient Organ)` delivery stores a revisioned
   `hosted` or `replicated` policy; unknown/reference-only recipients default to
   `hosted`, and replication requires an explicit signed choice.
2. **Recipient-specific envelopes.** Generic Sync `Package` is not used for
   Transfer delivery because it omits Transfer sidecars, links, and signed
   Action-intent proof. A versioned Transfer envelope binds the origin Organ,
   Transfer UID/revision, monotonic origin cursor, recipient, mode, disclosure
   manifest, recipient-redacted projection/events, included Facts, verified
   Action-intent evidence, actor keys, payload hash, and envelope UID. The
   receiver rejects wrong authority, wrong recipient, invalid proof, cursor
   conflicts, and changed replays.
3. **Hosted and replicated reads.** Hosted delivery persists a reference and
   freshness only; reads and signed mutations are served by the origin Cell.
   Replicated delivery additionally persists the verified redacted envelope in
   an isolated replica read model. Both modes still submit signed Actions to
   the origin; replica rows are never executable local commitments. Private
   formulas, notes, private Record quantities, unrelated parties/Records, and
   redacted proof fields never enter either payload.
4. **Durable push, pull, and conflict handling.** Transfer delivery has its own
   immutable, deduplicated outbox with attempts, exponential retry time, last
   error, and sent cursor. Push is checked against current delivery policy and
   visibility again at send time. Periodic/manual pull uses the recipient's
   last accepted cursor. A stale submitted Action is retained as a rejected
   local attempt with the authoritative revision and explicit refresh/review;
   it is never silently rebased or branched.
5. **Origin settlement and private local application.** A cross-Cell
   settlement cannot pretend to be one database transaction. The participant
   Cell applies only that Person's private Record delta and emits a signed
   application attestation bound to the occurrence, canonical slice, formula
   hash/version, and origin revision. The origin Cell verifies that attestation
   before appending public canonical settlement evidence. Pending, accepted,
   rejected, and compensated handoff states remain visible; neither Cell
   receives the private formula, private quantity, or unrelated Record data.
6. **Receipt versus fulfillment.** Receiving and seeing an envelope append
   Organ-authored package receipt evidence with envelope/cursor identity. These
   events have distinct names and storage from giver delivery and receiver
   receipt claims about real-world fulfillment. A package receipt cannot
   confirm an occurrence or settle a Record.
7. **Revocation without erasure.** Revocation is append-only origin evidence.
   It cancels queued future envelopes, blocks later push/pull and hosted reads,
   and remains visible to both sides. A recipient keeps already received signed
   evidence and its replica history; Lince never claims that disclosed bytes
   were erased.
8. **Sand surface.** Protein projects origin/replica authority, persistent
   mode, exact recipients/disclosure, queue state, attempts/errors, freshness,
   receipts, conflicts, revocation, and server capabilities/blockers. The sand
   offers reviewed mode selection, enqueue/retry/revoke/manual refresh, replica
   history, and conflict recovery while keeping package receipt visually and
   semantically separate from fulfillment.

- [ ] Add persistent `hosted` and `replicated` delivery modes, explicit
  recipients, visibility redaction, durable outbox retry, pull, freshness, and
  manual refresh.
- [ ] Deliver invitations/proposals, acceptance, counteroffers, threads, and
  signed evidence with replay/idempotency/conflict handling.
- [ ] Preserve origin signatures and field-level visibility; private formulas,
  notes, hidden Records, unrelated parties, and proof fields never leak.
- [ ] Keep package receipt/seen evidence separate from fulfillment evidence.
- [ ] Unknown contacts default to hosted/reference-only; replication is an
  explicit choice.

**Exit gate:** two Cells complete manual donation and sale with retry and no
duplicate effects, forged evidence, or hidden-field leakage.

#### Phase 9 — presets and manual release proof

**Prerequisite:** Phase 8 exit gate.

- [ ] Implement presets over the same schema in order: donation, sale,
  assignment, service/information, dependency tree, then manual ride/delivery.
- [ ] Prove focused engine/Protein/browser behavior at every earlier phase gate,
  not only at the end.
- [ ] Prove two-Cell donation and sale, then assignment/group coordination,
  information/service, dependency, satiation, correction, and partial work.
- [ ] Keep external payments, carrier APIs, legal machinery, external calendar
  providers, calls, OSM routing, and Fiote out. Documents/receipts may be linked
  through Records/messages; time constraints are already core.

**Exit gate:** warning-clean checks and focused/manual acceptance pass for every
gate, with licenses/notices preserved and no legacy Transfer model restored.

#### Phase 10 — Karma and recurrence recommendations (last)

**Prerequisite:** the entire manual evidence model and two-Cell proof.

- [ ] Define local `recurrence_likelihood` separately from counterparty
  confidence, keyed only by locally visible action/preset, concepts, people,
  quantity/time/place, and selected context.
- [ ] Implement deterministic diminishing-return evidence with time decay,
  configurable prior/growth/half-life, suggestion threshold, and higher
  disabled-by-default auto-draft threshold.
- [ ] Learn only from human-authored or mutually confirmed outcome Facts;
  recommendations and generated drafts do not train themselves.
- [ ] Create deduplicated explanations/recommendations at the suggestion
  threshold and local drafts only at an explicit opt-in threshold.
- [ ] Generate disabled Karma-rule candidates for review; no learned or explicit
  rule may agree, confirm, or settle for another Person.
- [ ] Make simple Transfer Actions available to explicit Karma only within
  approved visibility, recipient, budget, idempotency, agreement, and evidence
  gates.
- [ ] Persist thresholds, decay, scope, budgets, recipients, quiet time, and
  pattern disable/override in typed database policy.
- [ ] Prove recurrence, saturation/decay, deduplication, opt-out, no
  self-training, no private leak, and no automatic social commitment.

### Existing implementation evidence (not phase gates)

- The live Transfer sand provides portfolio/detail inspection, local
  search/filter/sort, a five-step blank creation composer, and Record navigation.
- `source: "transfer"` exposes revision, accepted parties, pending/terminal
  invitations, promises, balance/confirmation data, viewer context,
  capabilities, and blocker codes, but predicates, ordering, revision history,
  occurrence evidence, and the full exceptional vocabulary remain incomplete.
- `create-transfer-draft` validates and atomically commits the current draft,
  one creator participant, pending invitees, promises, visibility, request-key
  replay protection, and the signed canonical revision-1 Fact.
- Authenticated users map explicitly to Person records and the Permissions sand
  manages that mapping.
- Current creation inserts exactly one explicit/derived creator participant and
  stores every other selected Person as a pending invitation in revision 1.
- Legacy add/edit/agree/activate/confirm/settle Actions are compatibility
  groundwork, not authorization to expose later-phase controls.
### Standing Transfer decisions (2026-07-18)

- Parties are Person records. Organs are identity transport, visibility, trust,
  and sync boundaries, never social parties. Another Person becomes a party
  only after accepting an invitation or claiming an OPEN promise.
- Each Cell settles only Records it owns. Public/shared evidence says which
  giver provided what canonical amount to which receiver; the receiving Cell's
  private `application_formula` decides its local Record delta. The default UI
  is “you give” = negative local delta and “you receive” = positive local delta.
- Agreement is mutual, revision-bound, and presented as human milestones:
  level 1 is **Checked · ready to agree**, level 2 is **Agreed**, and later
  dual-sided fulfillment evidence is **Confirmed conclusion**. The final label
  is derived occurrence evidence, not an agreement level a person can assert
  for someone else. Every agreement policy requires the relevant parties to
  match levels before its next stage unlocks.
- `individual` unlocks each mutually matched give/receive path independently;
  `full` requires every party in the revision to match; `percentage` freezes
  and unlocks only its signed quorum coalition; `dependency` adds named
  upstream promise/Transfer state gates. No policy makes one person's signature
  stand in for another's.
- Under percentage agreement, reaching quorum freezes a committed coalition
  for that revision. Non-signers and their promises are excluded and cannot be
  activated or settled; changing percentage/coalition is a terms edit that
  invalidates affected signatures. Dependency gates may target a promise or a
  Transfer and explicitly name the required upstream state, defaulting to
  `kept` when unspecified.
- Public-result edits invalidate agreement: people, canonical quantities/units,
  dates/windows (including time-only changes), locations, confirmation
  requirements, dependencies/order, and
  any term affecting what someone gives or receives. Private application
  formulas, private notes, and local display choices do not. Visibility changes
  cannot retroactively retract data already delivered.
- Stale term writes are rejected, even when apparently non-conflicting. The UI
  preserves the draft and shows the intervening signed diff with one-click
  reapply/edit; the user must review and submit a new revision. Messages and
  unrelated private state may continue concurrently.
- Creating a Transfer immediately addresses and reveals revision 1 to its
  pending invitees. Invitation acceptance and creator withdrawal alter the
  signed terms revision; rejection and expiry are signed lifecycle evidence
  but do not churn the terms revision.
- Every promise snapshots its canonical unit. A Transfer may provide a default
  location and each promise may override it. Neither signed unit nor location
  is reinterpreted when its referenced Record later changes.
- An OPEN promise is a signed suggestion/template owned by the proposing
  Person, with only the counterparty left open, and reuse policy
  `duplicate|consume`, defaulting to `duplicate`. A claimant may refine every
  copied term, but the result is a new signed two-Person counteroffer retaining
  source lineage; consuming prevents later claims while duplicating leaves the
  source OPEN. The proposal may say either “I offer to give this” or “I ask to
  receive this” without naming a counterparty. Claiming creates opposite
  concrete promises for proposer and claimant; agreement levels then require
  both People to accept the refined terms. The OPEN template itself remains
  non-executable discovery data. Settlement applies each Person's own signed
  promise Record; it does
  not invent a separate private target Record for the counterparty-free
  template.
- Every give/receive occurrence is independently claimable and confirmable by
  its giver and receiver. Activation, retraction, delivery, receipt, and
  correction evidence is signed by exactly that Person's available identity;
  mutual current evidence confirms occurrence. Bulk completion is only an
  explicit labor-saving action for one Person and emits separate evidence for
  that Person across the reviewed tree selection. It cannot sign for another
  party.
- Partial fulfillment is normal progress and never rolls back completed facts.
  The remainder stays visible; policy may create a local draft remainder
  Transfer, never silently send or agree it. An accidental private mutation is
  corrected by compensation; a social correction uses a linked reversing
  Transfer. Disputes annotate evidence rather than rewriting it.
- Expiry means a promise passed its end window: open/proposed becomes withdrawn,
  agreed/active becomes broken. “Reopen” keeps the Transfer identity but creates
  a linked successor promise revision, preserving the terminal promise and all
  signatures.
- Reservation defaults to `none`; incoming promises affect `planned`, never
  `available`, until locally settled. Different concepts may be exchanged;
  balance stays advisory per concept/dimension, Lingua converts only compatible
  units, and incomparable buckets never silently combine.
- User-facing exceptional states are derived and distinct: cancelled/rejected
  before activation, expired/withdrawn, broken, partially settled, disputed,
  settled, and compensated/reversed. Only immutable occurrences are terminal;
  the enclosing Transfer may continue through successor/remainder promises.
- Visibility defaults hidden. A user explicitly addresses Persons/Organs,
  expands by proximity, or makes an OPEN promise public. Unknown non-contact
  people may negotiate when directly addressed or through public discovery;
  they are not forced into contacts, receive no ambient sync, and remain
  blockable.
- Social delivery is selectable and persistent: hosted mode uses an
  authoritative remote view; replicated mode imports signed events/data into
  the Cell. Push while connected plus durable outbox retry and periodic pull is
  the normal cadence, with manual refresh/retry and visible freshness. Unknown
  contacts default to hosted/reference-only.
- Transfer hierarchy is required: parent/child trees group Transfers that must
  happen together, with derived roll-up, retained child evidence/policies, and
  executable dependency/order DAGs. Cycles are rejected rather than warned.
- The Transfer sand opens on the inbox/list and big-picture tree status;
  creation retains a compact list summary and gives high control. Preset build
  order is donation, sale, assignment, then broader workflows.
- Threads/messages are the integration point for external payment receipts,
  contract documents, and other linked records; Lince does not execute payments
  or provide legal-contract machinery. Promise time constraints are core.
  Carrier APIs are deferred external delivery integrations; manual delivery and
  ride Transfers are not deferred.
- Shared threads are visible to participating parties by default; private
  threads use explicit visibility. Sent messages are immutable social evidence;
  correction is a reply, while permission-gated deletion leaves Ledger
  evidence/tombstone. A never-shared draft may be hard-deleted; after invitation
  or publication it is cancelled/tombstoned instead.

## [x] Attention — the Decision Queue ("what should I do next")

- [x] `source: decision, live: true` is the queue; `decide` is the answer.
  Four deterministic feeders run inside the heartbeat, no UI required:
  broken promises (`kind: "expiry"`), Karma `ask` consequences (`"ask"`),
  Senses matches (`"draft"`), projected crossings (`"crossing"`, a week
  horizon, per-record deduped).
- [x] `decide { decision, answer }` closes it through the Ledger; if the
  chosen option carries an `action` field, deciding EXECUTES it — one-tap
  flows like "yes → set-quantity".
- [x] Decisions with `expires_at` auto-close as `"expired"`; every sweep
  dedups by `(subject, kind)` — one situation asks exactly once while its
  decision stays open.
- [x] Notify budget (`configuration.attention_budget_per_day`, default 12)
  is hard — over-budget notify effects complete as `parked:digest` instead
  of interrupting; nothing is lost.
- [ ] Notify platform channels: device records (`kind='device'`) routing to
  desktop toast / mobile push / sound / text digest, per-source on/off and
  quiet hours, plus a digest sand reading the parked rows.
- [ ] Inward capture sources: every capture source (phone, scale, camera,
  mic) as a visible, off-switchable signal-record; AI only ever as a Signal
  implementation, never hidden.

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
  `lince.file_sync` extension, edited from the **Organ** sand (below).
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

## [ ] Senses + Imagination — the recommendation engine

- [x] A match rule is a record (`create-match-rule { watch_concept,
  max_proximity, min_confidence, auto }`), activates/deactivates like any
  rule; `max_proximity` is a hard ceiling — matching never auto-expands.
- [x] Each heartbeat, `senses_pass` joins your OPEN promises against the
  discovery cache (sign-opposite deltas, Lingua-aligned concepts, window
  overlap, confidence floor) into ranked drafts straight into the Decision
  Queue.
- [x] `Engine::project(now, until)` folds promises + rules forward on a
  virtual clock (signals frozen); `Engine::snapshot(now)` hands the mutable
  input to toggle/clear/re-fold/diff — the scrubbable, branching future is
  an engine call today; no sand renders it yet (see the Timeline transport
  verb below).
- [x] Deterministic numbers, no ML: `confidence(@p)` = the party's
  Laplace-smoothed kept-ratio; `demand(@concept)` = the current hour's share
  of trailing-30d activity on that concept.
- Senses proposal delivery is tracked with the complete OPEN-promise flow in
  Transfer Phase T1.
- The generic recurrence-likelihood/recommendation engine and its first
  Transfer/Karma consumer are tracked together in Transfer Phase T5; it remains
  distinct from counterparty kept-promise `confidence`.
- [ ] Confidence refinements: a window-tightness factor and a dispute
  penalty on top of the Laplace kept-ratio.
- [ ] **Timeline transport verb**: `Engine::project`/`Engine::snapshot`
  already exist and work, but only from inside the engine — there is no
  WebSocket verb exposing them, so no sand can call them. The one genuinely
  new endpoint a Timeline sand needs: expose `project`/`snapshot` over the
  transport so a sand can render the projected future and let a user branch
  it (toggle a rule, clear a promise, re-fold, diff two timelines) live.

## [x] Trust

- [x] Every locally-authored fact is signed on the write path; imported
  facts keep their ORIGIN signature so downstream Cells can still verify the
  original author — a two-layer tamper model (the hash chain guards
  content, the signature guards authorship).
- [x] Compaction archives stay verifiable file-side; the anchor fact makes
  the file tamper-evident from inside the Ledger.
- [x] No reputation scores, ever — kept/broken history is queryable raw
  material, never a score.
- Field-level grants and most-specific-wins precedence are tracked in Transfer
  Phase T3, with Transfer as the first consumer of this shared Trust behavior;
  today grants remain whole-row only.

---

## [x] Board and sand infrastructure — the shipped web surface

- [x] One WebSocket (`/host/transport/ws`) shared by the unified bridge and
  the Data panel; the bridge speaks both the legacy nested-payload chrome
  shape and the current flat `frame.js` shape, routing by subscription id
  and lane room (ids never collide across consumers).
- [x] Sands are Rust-canonical: each official sand is a self-contained
  `.html` via `include_str!`, registered in `OFFICIAL_WIDGETS`; groups ship
  as `.lince` workspace archives; the catalog peeks content so a group
  archive is never mis-parsed as a single sand, and a group entry replaces a
  same-named single sand.
- [x] Groups nest: `BoardCard.group_ids` (outer → inner) is authoritative;
  disbanding an outer group preserves inner ones; adding a catalog group
  re-homes to a fresh inner id each time, so repeated adds are independent.
- [x] Events are scoped to a grouped sand's innermost group (a kanban's
  `recordClicked` drives only its own Record, not another group's);
  ungrouped sources broadcast board-wide; cross-session mirroring rides lane
  rooms, never persisted.
- [x] Per-card host state flows both ways (`H.getCardState()`/
  `H.onCardState`/`H.patchCardState`) — any sand persists UI prefs without
  touching the Ledger; board chrome itself (pan/zoom/workspaces/position/
  size/pin/z-index/grouping/edit mode) is ALWAYS host state, never a Ledger
  fact.
- [x] The Data panel is the one place Protein gets configured (source,
  filters, sort, limit, includes) per card — sands ship with NO default
  driving Protein; an unconfigured card shows an explicit "pick a Protein"
  prompt instead of silently dumping every record. The builder autocompletes
  link-kind inputs from a `concept` source subscription; "All records"
  drives an explicit `{source:"record"}`, distinct from "unconfigured." The
  links include is MULTI-KIND ("+ kind" rows, `"*"` = every kind, both AST
  spellings round-trip) — one Protein pulls several link types and the
  relations graph draws parallel kinds between the same two nodes as
  fanned-out bent lines.
- [x] The shared slash-block editor (`window.LinceBodyEditor`) is used by
  every sand that touches record bodies: `/` opens a Notion-like block
  palette (headings, image placeholder, checkbox), `@` opens the record
  picker; the body stays canonical markdown, checkboxes toggle by original
  line index, `@slug` chips navigate and become real `references` links on
  save. Optional — a sand without it degrades to a plain textarea.
- [x] Local images: the editor's "/image" block picks/uploads a file (native
  OS dialog first, browser `<input type=file>` fallback), sniffs bytes
  against a raster allowlist, and stores under an opaque generated name —
  there is still no route serving an arbitrary disk path.
- [x] Action `warnings` reach sands end-to-end (bridge → `frame.js` → amber
  sand status), never surfaced as errors.
- [x] Record deletion is permission-gated (`record:delete` vs
  `record:delete_own` + creator match) at the one `DeleteRecord` action —
  since threads/messages are themselves records, this single gate covers
  all three; viewer identity (`H.getViewer()`/`H.onViewer`) flows to every
  sand so delete controls can show/hide correctly, though the engine gate
  (not the UI hint) is what actually enforces it.
- [x] The permission/role/user system is Protein(`source:"auth"`) + five
  gated Actions (`create-role`, `create-user`, `assign-role`,
  `grant-permission`, `revoke-permission`) — a plain CRUD sand on top, no
  different in kind from any other sand; auth-table mutations emit no
  facts, so the sand re-subscribes after every mutation instead of relying
  on live invalidation.
- [ ] Per-sand capability/permission model before imported sands can write
  arbitrary Actions (today any sand can call any Action — fine for official
  sands, needed before running imported ones freely); sand provenance
  `cause=sand:<uid>`.
- [ ] Blanket read/write permission enforcement across every OTHER Protein
  source and Action (today only `delete-record` and the five auth actions
  are gated) — sequenced after more of the role-management UI exists.
- [ ] `.lince` GROUP drag/drop import: client routing still checks the
  `.group.sand` extension — route by content instead, like the catalog
  does.
- [ ] Host-state sync for board presentation state across devices.
- [ ] Package import/publish subsystem on the new record/package model.

## [ ] Sands — the individual surfaces

- [x] **Table** — the rebuild template: cell edits map by column to typed
  Actions.
- [x] **Todo** — focus queue + `set-quantity` undo/redo.
- [x] **Kanban** — quantity-lane board (fully overridable lanes), per-lane
  collapse/resize; full column system (create/rename/delete/reorder/hide,
  bucket by value/range/concept, shareable presets, optional per-column
  color wash); three body view modes (head/compact/full) with per-card/
  column/board override; writes records four ways (checkbox toggle, column
  move, in-place body edit, bulk delete with confirm); card click opens the
  grouped Record, "+" opens Record's creation mode; badges assignee/parent
  via a side concept-name subscription; three-state live dot.
- [x] **Relations** (ships as the group with Record) — d3 force graph with
  physics sliders, golden-angle layout, zoom/pan/fit, directed arrows,
  edge-kind labels; Shift+drag adds a link (optimistic), edge-click selects,
  the header chip's ✕ removes; **Trail mode** lays a root's forward
  link-tree out topologically with a Done/Undo promotion cascade over
  shared status presets (quantity or concept buckets, e.g.
  `@todo/@next/@wip/@done` — the SAME vocabulary a kanban column can use);
  resizable controls panel, link keybinds (Delete unlinks, Ctrl+Z undoes a
  session-local stack); link-kind inputs autocomplete from concepts.
- [x] **Relations node gravity (tree weight refactor)** — one extra physics
  variable on top of the existing forces: each node gets a *weight* from its
  depth in the selected link tree — the root is heaviest (or lightest, when
  inverted) and each hop toward the leaves gets lighter; nodes not connected
  to the root weigh the same as leaf nodes. A vertical gravity bias then
  pulls heavy nodes down and lets light ones float up, so the tree settles
  into root-down/leaves-up (or inverted, root-up/leaves-down) while charge,
  link, collision, and center forces keep working unchanged. Configurable
  per sand: gravity direction toggle (root sinks / root floats), strength
  slider, and which link kind/root defines the tree (reuse the Trail mode
  root picker). Applies in both graph mode and Trail mode — in Trail mode it
  replaces/augments the fixed topological ranks with the same simulated
  gravity so the done/next/ahead coloring stays readable on a physically
  settled tree. (Shipped: Graph controls → Node gravity section. Weight maps
  to BUOYANCY — a constant per-node vertical acceleration, not a target
  line, so nodes keep falling until their link tethers them and the tree
  hangs like a mobile; the center forces stay on for cohesion while
  charge/link soften. Graph mode simulates all nodes this way; Trail mode
  unpins the rows and simulates only tree nodes, x anchored to the topo
  layer.)
- [x] **Record** (formerly "record_info" — the sole markdown editor,
  viewer, and creator for a record, and the home for every other
  per-record concern) — the get view IS the edit view (head/slug/quantity/
  body writable, Save writes only what changed, a dirty form is never
  clobbered by live updates); Zero (`deactivate`) and Delete
  (`delete-record`, permission-gated) are separate buttons; creation mode
  shows the same fields empty, Create + focuses the new record; carries the
  shared slash-block editor (headings/images/checkboxes/`@slug`, the same
  palette everywhere in a body); collapsible sections for **Work**
  (start/due dates, estimate, worklogs with play/pause, on the `work`
  record extension, offline-queued writes), **Assignees** (`assigned-to`
  links), **Links** (every hop-1 link either direction, kind+target inputs,
  both autocompleted — a document/URL just lives as a link or inline media
  in the body, no separate resource/attachment concept), and **Threads**
  (a real multi-thread system — a tab per thread, search
  filters which tabs list without hiding messages, each message shows
  timestamp + sender, `@slug` in a post becomes a real link, delete
  controls per permission). Reusable — any sand drives it via a scoped
  `recordClicked`/`recordCreate`; no sand keeps a private record sidepanel.
  Full real-time collaborative editing is blocked on the CRDT text relay
  above.
- [x] **Organ** — Protein list of `kind=organ` records (this Cell + its
  contacts); selecting one shows/edits its `lince.file_sync` extension
  (enabled, disk path) via `set-extension` — File Sync to disk as a
  first-class per-organ feature. Deliberately thin: no trust/proximity/
  introduce/block/quarantine UI yet (that's the separate **Organ contacts
  manager** item below); the dormant, unregistered pre-Protein
  `organ_management` sand was left in place rather than adapted.
- [x] **Roles & Permissions** — role cards with one checkbox per catalog
  permission, a users table with a role select, "+ New role"/"+ New user"
  forms; a Forbidden response reverts the optimistic toggle inline, no
  second enforcement layer.
- [x] **Document Viewer** (embed-honest) — PDF/EPUB/image rendering, opaque
  authenticated media paths, no Protein/Action/lane/proxy of its own.
- [x] **Freedoom** (embed-honest) — local wasm/WAD, true solo mode, no Cell
  data plane.
- [x] **Lince Logo LED** (embed-honest) — selected visual mode as host
  state.
- [x] **Ghostty Terminal** (embed-honest) — explicit `terminal_session`
  frame API over the one transport socket, connection-scoped local PTYs, no
  separate socket.
- [ ] **Home manager / dashboard** — aggregate Proteins + Action writes, no
  new backend needed.
- [ ] **Karma Orchestra v2** — rule CRUD (`create-rule`/`update-rule`) with
  live Proof-loop warnings surfaced on save, derived values readable via
  `value()`, signal/frequency creation, a DepGraph render off the rule
  records.
- [ ] **Finance / statistics dashboard** — `source: fact` aggregates (`sum`
  by `cause_kind`/`day`, `concept_in: "money"`), drill-down via `record_eq`.
- [ ] **Pantry / inventory dashboard** — `availability` + `projection`
  includes render "8 now, 5 available, 2 by Friday" with no math in the
  sand; crossing decisions already surface "you run out Thursday."
- [ ] **Timeline sand** — renders `Engine::project` output as scrubbable,
  branching points; blocked on the Timeline transport verb above.
- [ ] **Organ contacts manager** — list contacts with trust/proximity/sync
  policy, introduce via URL, block button, quarantine viewer.
- [ ] **Neighborhood matching settings** — `create-match-rule` + visibility
  grants ("publish this Need to @neighborhood"); results land in the Inbox
  as drafts once the polling scheduler is wired.
- [ ] **Todo polish**: create-task UI/Action, richer history backed by
  compensation/facts, live-update proof beyond the stubbed bridge, plus the
  old table sand's deferred keyboard-grid navigation, helix mode, and
  concept/unit inline editors.
- [ ] Every ported data-plane sand needs a driven chromium selftest
  (snapshot + Action round-trip + live update); two stale scripts
  (`board-selftest.sh`, `table-sand-selftest.sh`) still reference deleted
  crates/files and need rewriting on the current architecture.
- [ ] Broader browser selftests: pan, zoom, grouping, resize, pin,
  workspaces, import, publish, sand-to-sand events.
- [ ] Frontend polish/redesign, after the data plane settles.

## [ ] Future Instincts and product surfaces

- [ ] OSM place data: a local offline extract, local geocoding, `route_eta`,
  polygon `within`, a `route(a,b)` include (`distance`/`near` already
  live).
- [ ] Duration/calendar math Instinct (Frequency is the proto-Instinct);
  currency conversion over a Lingua `@money` dimension.
- [ ] Storage-engine independence (AniccaDB): swapping out `store` must not
  change one character of the Protein/Action contract.
- [ ] New product surfaces beyond the Transfer workstream: route/ride
  planning, calls, calendar/time budgeting, finance projections, social feed.
- [ ] **Fiote** (deferred by decision) — the optional operator, autonomy
  ladder `observe → suggest → draft → act-within-budget`; reads only
  through Protein against `@fiote`-visible data, writes only through
  Actions with `cause=fiote` signed under the user's key with an agent
  marker (delegation visible, inspectable, reversible via compensation);
  budgets (max actions/day, max promise value, forbidden Action kinds —
  e.g. never `settle-transfer`) enforced by the engine, not the prompt.

## [ ] Parked (needs the user)

- [ ] Worklogs as time-concept delta facts on the Ledger instead of the
  `work` record extension — only if preferred over the current, already-
  shipped extension approach; would be an extension → facts migration.
- [ ] The trailed-off "make sure the sands can also show a …" thought (best
  guess: a preview/collapsed sand state) — needs the user to finish the
  sentence.

## [ ] Acceptance workflows (the Window)

Proof goal: each workflow runs end-to-end on the new core through a real
sand, checked off here when proven.

- [x] Focus queue
- [x] Personal finance's projected-crossing engine (the sand itself isn't
  built yet)
- [ ] Relation trail mode, re-proven on a live server (proven on the old
  core pre-purge; the driven stub selftest already covers the behavior,
  this is the real-socket run)
- [ ] Todo/knowledge base (habit re-arms daily; done posts a fact with
  cause)
- [ ] Recurring tasks (monthly rule fires exactly once; catch-up works)
- Transfer workflows (DONATION, SALE, assignment/group coordination,
  dependency chains, and later RIDE/DELIVERY) are centralized in Transfer
  Phase T4 so their implementation and proof cannot drift apart.
- [ ] Chat & calls ("call the parties when the Transfer reaches agreed" as
  a rule; AV embedded)
- [ ] Real-time collab docs (two Cells, one body, cursors on lanes, Ledger
  shows only text_edit annotations)
- [ ] Social network (federated feed from two organs, visibility respected
  — zero new core)
- [ ] n8n-style command flows (signal→rule→effect chain built visually in
  Orchestra, and runs)
- [ ] CRM/people (birthday whisper fires; interaction report = one
  aggregate Protein)
- [ ] Personal finance sand ("rent leaves you short on the 5th unless X
  settles" rendered from the crossing engine)
- [ ] Inventory & production (`derive_needs(@cake, 20)` shopping list; the
  PRODUCTION chain relays settlement)
- [ ] World statistics (need-mountains aggregate from N organs, nothing
  hidden leaks)
- [ ] AI conversation sand (zero core changes)
- [ ] Calendar & time budgeting (projected week renders; moving a promise
  recomputes)
- [ ] Health & IoT (scale posts weight facts; streak rule; off-switch stops
  it)
- [ ] Games (chess on fds state; embedded engines; THE Game reads records
  with Karma as rulebook)
- [ ] Education (imported Relation trail shows per-student progression)
- [ ] Garden & farm (plant records + watering rules; scales into
  production)
- [ ] Monthly recaps ("this month in this Cell" generates itself from a
  rule)

---

## [x] Cross-cutting maneirisms cheat-sheet

- [x] Everything is a record; activation is quantity; delete is deactivate
  (and hard-delete is a separate, further step).
- [x] The fact is the truth, quantity is the cache; undo is compensation.
- [x] Metadata/state changes announce themselves as zero-delta annotation
  facts.
- [x] Warnings are advice (cycles, Proof loops), never rejections.
- [x] Heartbeat order: promise expiry → decision expiry → timers → signal
  sampling → effects (budgeted notify) → senses pass → crossings pass.
- [x] One situation, one open decision (dedup by subject+kind).
- [x] Uids are identity everywhere, across Cells; slugs are local sugar and
  get dropped on collision at import.
- [x] Visibility is default-hidden, whole-row, enforced in exactly one
  place — and applied before aggregation.
- [x] Blocked organs are rejected at every door (import, discovery,
  outbox).
- [x] Conventions: uids are prefixed ULIDs (`r_/f_/p_/l_/c_/t_` = record,
  fact, promise, link, concept, transfer); slugs are `dot.case`; timestamps
  RFC3339; durations `90s`/`2h`/`30d`; `@slug` in conditions is sugar for
  `quantity(@slug)`.
- [x] Time is deliberately NOT a record column: record timing = Karma +
  Frequency; declarative time (what strangers match on) lives on promise
  windows.
- [x] Board chrome is frontend state; sand data is Protein/Actions.

## The theory (from the retired blueprint)

**The refounding, three sentences:**

> **Everything is a Record. Every change is a Fact. Every intended change is a Promise.**

**The pillar map:** Record (state) · Memory/Ledger (facts) · Lingua (shared
concepts; Instinct tier = concepts with engine functions) · Karma (Signals →
Rules → Effects) · Transfer (promise bundles under agreement + visibility) ·
Senses (matching open promises, proximity-scoped) · Trust (verifiable signed
deltas) · Imagination (state(t) projection + confidence) · Protein
(declarative reads) & Actions (typed writes) · Attention (the Decision
Queue) · Fiote (optional agent operating the same knobs).

**The placement rule (the Window):** the core owns what must be computed,
verified, or agreed across Organs; interfaces own what is seen; embed
honestly what the world already built well. Altitude ladder: Primitive →
Pillar engine → Instinct → Lingua concept/unit → fds sidecar → Sand →
Embedded foreign app.

**Non-negotiables:** quantity stays central (negative = Need, positive =
Contribution, zero = peace); quantity-as-activation on everything; full math
in Karma conditions (tokens substituted, then evaluated); compatibility
fully ignored — greenfield build, old data ported by hand; local-first; no
global reputation score, ever.

**Storage:** SQL is SQLite dialect; Protein is the abstraction that later
permits AniccaDB — replacing `store` must not change one character of the
Protein/Action contract.

**The Window's standing law:** apps are projections of one organism. Finance
= inventory = pantry (units + facts + promises + Imagination); chat =
comments = negotiation (messages on a shared object); profiles = catalogs =
libraries (published records behind visibility). When a new workflow
arrives, triage it against the primitives; if it doesn't decompose, the
missing piece is named by what resists — that is how the next abstraction
gets deduced instead of appended.

**The north star:** Ana wakes. No dashboard. The kitchen scale posts a fact;
beans cross their threshold; a promise to the roaster activates under a rule
she approved months ago; two Cells settle Saturday pickup. One whisper on
the walk to work — a nod, two promises change state. Work is an Organ; the
standup is a view nobody fills in. A second whisper near the market — her
mother's pantry Need, published to family only, met on the way home. In the
evening she scrubs the timeline out of curiosity: rent fine, a bar's event
Organ bit on her guitar Need, the tomatoes surplus in nine days and the
donation rule is staged. The Lincegoshi grows fat and luminous and
dissipates. Under four minutes of managing life, all of it decisions only a
human could make.

That is the Death of Lince: management time asymptotically approaching the
irreducible minimum — the moments of actual human choice. More needs met,
more transactions peer-to-peer, more donations, more efficiency: the dance
of the world, made executable. Everything is a Record. Every change is a
Fact. Every intended change is a Promise. The rest is choreography — and
Protein is how the dance is seen.
