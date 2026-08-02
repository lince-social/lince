
## [ ] Transfers — manual-first product, then Intelligence

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
Transfer-aware rules and learned recommendations live under Intelligence and
remain gated on trustworthy manual semantics and evidence.
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
  but deliberately do not enter the generic rule cascade. Transfer-aware
  recurrence, recommendations, and automation remain Intelligence work.
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
  Revision and lifecycle Facts are published without Intelligence automation;
  heartbeat closes due invitations.
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

#### Phase 3 — signed agreement and commitment (complete)

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

- [x] Verify level 1 review and level 2 commitment signatures for one Person and
  one exact revision; sidecar state is a cache of immutable evidence.
- [x] Verify matched relevant levels are required before later stages unlock.
- [x] Verify `individual`, `full`, frozen `percentage` coalition, and
  named `dependency` gates without one Person's signature standing for
  another.
- [x] Verify only the agreeing Person's promises advance and expose who blocks each
  agreement path.
- [x] Verify affected agreements invalidate atomically for every public-result edit;
  private formula/notes/display changes do not invalidate public agreement.
- [x] Verify review/commit controls come only from server capabilities and wait for live
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
- `crates/engine/tests/transfer_agreement.rs` drives the current typed Actions
  with real Person signers and verifies adjacent/idempotent forward and backward
  transitions, immutable verifiable Facts, identity isolation, per-Person
  promise advancement, matched `individual` paths, `full` policy, frozen
  percentage coalitions, structured dependencies, public time invalidation,
  and private formula stability. Its draft fixtures also replay creation.
- `scripts/other/transfer-sand-selftest.sh` verifies that agreement controls are
  capability-derived, send the Person-scoped typed Action, remain visibly
  pending after Action acceptance, and clear only after pushed Protein evidence
  contains the request id. The tests exposed and fixed single-connection
  in-memory deadlocks in initial draft creation, replay, and whole-draft
  revision without changing their transaction semantics.
- The focused Phase 3 target passes all four policy/sensitivity scenarios, and
  the driven Chromium Transfer suite passes every overview, attachment,
  agreement-live-state, preset, and mobile assertion.

**Exit gate:** every agreement-policy matrix derives readiness from signed
revision-bound evidence, and no signature authorizes another Person or revision.

#### Phase 4 — availability, activation, and occurrence evidence (implemented; focused verification passing)

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

Implemented Phase 5 contracts (focused verification passing):

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

- [x] Add persistent `hosted` and `replicated` delivery modes, explicit
  recipients, visibility redaction, durable outbox retry, pull, freshness, and
  manual refresh.
- [x] Deliver invitations/proposals, acceptance, counteroffers, threads, and
  signed evidence with replay/idempotency/conflict handling.
- [x] Preserve origin signatures and field-level visibility; private formulas,
  notes, hidden Records, unrelated parties, and proof fields never leak.
- [x] Keep package receipt/seen evidence separate from fulfillment evidence.
- [x] Unknown contacts default to hosted/reference-only; replication is an
  explicit choice.
- [x] Complete the cross-Cell settlement handoff: origin-authored canonical
  slice proposal, isolated participant-side private Record application,
  durable Person-signed attestation return, and origin acceptance without
  disclosing the local Record, formula, or private quantity.

Implemented Phase 8 delivery foundation (workspace check complete):

- Migration `0024_transfer_delivery.sql` stores revisioned delivery policy,
  immutable outbox/pull attempts, hosted references, isolated replica history,
  remote commands, conflicts, receipts, revocation evidence, and settlement
  handoff state without promoting replicas into canonical Transfer sidecars.
- Recipient-specific envelopes and policy events are signed by the origin
  Organ. The projection keeps recipient-specific capabilities and action
  templates but recursively removes raw Fact, Action-intent, and signature
  fields. Private formulas, local Record quantities, and unrelated parties are
  not transportable in the envelope format.
- Exact browser-signed Action-intent bytes are carried inside Organ-authenticated
  remote commands. Invitation acceptance/rejection, counteroffers, agreement
  workflow, occurrence claims, and Transfer-scoped threads/messages execute at
  the origin; changed replays and stale revisions remain visible conflicts.
- Package receipts have a stable domain-separated recipient Organ signature in
  addition to the fresh nonce-bearing HTTP signature. Exact envelope retries
  therefore preserve receipt identity, and receipt/seen kinds remain separate
  from real-world fulfillment claims.
- Migration `0025_transfer_remote_settlement.sql` completes the settlement
  boundary without changing the committed Phase 8 migration. A Person-signed
  remote command creates one immutable origin handoff and reserves its reviewed
  canonical slice. The next recipient envelope carries an Organ-signed public
  handoff containing quantity/cursor commitments but no local Record or private
  formula data.
- The participant chooses a local Record and applies the configured formula in
  its own Cell. The private Fact and formula remain in
  `transfer_local_application`; a retryable outbox returns only a Person-signed
  attestation containing the canonical slice hash and formula commitment. The
  origin verifies the Person/Organ binding and signature before appending its
  Organ-signed canonical acceptance Fact, advancing the handoff, and delivering
  the resulting state. Pending slices reserve canonical quantity so concurrent
  requests cannot over-settle an occurrence.
- Protein and the sand expose reviewed cross-Cell slice creation, private local
  Record selection/application, pending/applied/accepted state, attestation
  delivery, and origin-side handoff evidence without presenting the handoff as
  a single cross-database transaction.
- `nix develop -c cargo check --workspace --all-targets` completes with warnings
  treated as errors. Tests were intentionally not run for this phase.

**Exit gate:** two Cells complete manual donation and sale with retry and no
duplicate effects, forged evidence, or hidden-field leakage.

#### Phase 9 — attachments and manual release proof

**Prerequisite:** Phase 8 exit gate.

- [x] Let a Transfer message explicitly reference existing document/receipt
  Records through the generic Record-assertion model. The Action validates every
  referenced Record before creating the message; Protein projects the explicit
  references; the sand can select and open them. This is not a new attachment
  entity, file store, payment object, or Transfer-private message model.
- [x] Add one executable release-proof entry point with a phase-indexed matrix
  of focused engine, Protein, and browser checks for every earlier phase gate.
  A missing check or failed command fails the proof instead of being recorded
  as an informal success.
- [x] Add explicit two-Cell proof scenarios for donation and sale, followed by
  assignment/group coordination, information/service, dependency, satiation,
  correction, and partial work. Retries, signatures, recipient redaction,
  private application state, and duplicate-effect rejection are acceptance
  assertions rather than visual inspection notes.
- [x] Keep external payments, carrier APIs, legal machinery, external calendar
  providers, calls, OSM routing, and Fiote out. A repository guard rejects
  these integration surfaces from the Transfer implementation; documents and
  receipts remain ordinary Records explicitly referenced by messages, and
  promise time constraints remain the only calendar behavior.

Phase 9 implementation:

- `create-message` and its origin-authoritative `create-transfer-message`
  counterpart accept a bounded, deduplicated `references` list. The engine
  resolves and validates the entire list before creating the message, stores
  ordinary `message @references [Record]` assertions, and permits a reference-only
  message without inventing an attachment or file entity.
- Protein nests the explicitly disclosed Record identity, label, kind, and body
  under its message. Transfer delivery therefore carries only references a
  participant deliberately placed in a shared thread; unrelated Records and
  private settlement state remain outside the recipient projection.
- The Transfer sand lists referenced Records on each message, opens them through
  the shared `recordClicked` lane, and lets the writer select existing plain
  Records while composing an origin-local message. A hosted remote view renders
  the explicitly disclosed Record snapshot but does not offer its unrelated
  local Records to an origin that cannot resolve them; cross-Cell file upload or
  generic Record sync remains out of scope. The browser harness asserts both
  the rendered navigation and typed Action payload.
- `scripts/other/transfer-phase9-release-proof.sh` is the single release entry
  point. `--automated` runs the warning-clean workspace check plus focused
  engine, Protein, sync, and browser checks; `--guard` rejects named payment,
  carrier, external-calendar, routing, call, legal-signing, and Fiote vendors
  from Transfer code; `--manual-matrix` prints the eight two-Cell scenarios.
- Full `--all` proof cannot pass silently: it requires
  `TRANSFER_PHASE9_MANUAL_RESULTS` with one `scenario|pass|evidence note` row for
  donation, sale, assignment/group coordination, information/service,
  dependency, satiation, correction, and partial work. Missing or failed rows
  fail the release command.
- The warning-clean full-workspace all-target check passes. The editable
  workspace archive export is reconnected, and legacy Phase 4/5,
  Transfer-source, expiry/reservation, and trust-ahead fixtures now use the
  revision/occurrence workflow or explicitly prove that deferred Intelligence
  cannot bypass it. The automated Phase 9 proof passes; the two-Cell manual matrix
  remains an explicit release step.

Verification repair order:

- [x] Reconnect the existing workspace-menu `.workspace.sand` export endpoint
  to `build_workspace_archive`. This editable bundle is distinct from the
  Archive sand's inert HTML capture: it preserves the selected workspace,
  layout, card state, and deduplicated sand packages for later import.
- [x] Preserve export when an installed package cannot be loaded by rebuilding
  a valid package from the card's embedded HTML and manifest hints. Export must
  remain authenticated wherever board state is authenticated and must reject
  unknown workspace ids without creating an empty archive.
- [x] Port Phase 4 confirmation fixtures from bundle-wide legacy commands to
  signed, revision-bound participant agreement, one directed occurrence, and
  giver-delivery/receiver-receipt claim evidence.
- [x] Port Phase 5 settlement fixtures to reviewed occurrence slices, including
  stale-preview rejection, partial progress, idempotent replay, private Record
  application, and compensation without rewriting public fulfillment.
- [x] Port the Transfer Protein status fixture and remaining reserve/agreement
  fixtures to typed draft creation. Do not unlock or restore `create-transfer`,
  `add-party`, `agree-transfer`, `activate-transfer`, `confirm-transfer`, or
  `settle-transfer`.

Verification results:

- `confirmations` covers role-specific claim authorship, mutual-claim gating,
  idempotent settlement, partial progress, stale previews, private
  compensation, visible remainder settlement, and generic Transfer threads.
- `transfer`, `expiry`, and `karma_effects` cover reviewed sale application,
  whole-draft agreement invalidation, persisted reservation precedence, OPEN
  proposer ownership, and fail-closed deferred rule activation.
- The Transfer Protein ladder now exercises `proposed`, `agreed`,
  `in_transfer`, `partially_settled`, and `settled`. This found and fixed an
  agreement-readiness projection bug that could overwrite terminal `settled`
  back to `agreed`.
- The `.workspace.sand` domain round trip, full workspace warning-clean check,
  and `transfer-phase9-release-proof.sh --automated` all pass. The release
  script selectors were updated so renamed tests cannot silently run zero
  cases.

**Exit gate:** warning-clean checks and focused/manual acceptance pass for every
gate, with licenses/notices preserved and no legacy Transfer model restored.

#### Phase 9.1 — workflow presets

**Prerequisite:** Phase 9 implementation. Presets are local composer prefills,
not permanent Transfer types: every result uses the ordinary Transfer draft
schema and typed Actions, remains fully editable, and follows the same review,
signature, invitation, agreement, confirmation, and settlement gates.

Implementation order and contracts:

1. Put one preset menu beside blank creation. Selecting a preset opens a compact
   editable setup followed by the existing signed review; blank creation keeps
   the full guided composer.
2. Prefill only semantic structure and conservative policy defaults. Records,
   People, quantities, time windows, and places remain explicit user choices;
   external `transferCreate` events may supply known local Record context.
   Their payload contract is `{ preset, record|records, invitee|invitees,
   creator?, quantity|quantities, head? }`; authenticated sessions ignore a
   supplied creator and continue deriving it from the WebSocket subject.
3. Use one directed promise for donation, service, and information. They default
   to an OPEN suggestion so a named counterparty is unnecessary; the creator may
   select a concrete party before submission.
4. Use two directed promises for sale: the creator gives the resource and
   receives the consideration. A sale requires one named invitee because a
   multi-term OPEN claim is not silently invented; consideration is an ordinary
   Record and never executes payment.
5. Assignment, group coordination, ride, and delivery require a named invitee.
   Group coordination starts with two editable responsibilities; ride and
   delivery expose only signed time/place fields and contain no routing,
   dispatch, carrier, map, or calendar-provider behavior.
6. Dependency plan prefills dependency agreement plus a structured Transfer
   dependency. The user must select an existing upstream Transfer before the
   draft can be signed.

- [x] Add an editable donation prefill over one directed promise.
- [x] Add an editable sale prefill over linked resource and consideration
  promises; consideration is Record evidence, never payment execution.
- [x] Add editable assignment and group-coordination prefills.
- [x] Add editable service and information prefills.
- [x] Add an editable dependency-tree prefill over structured dependencies.
- [x] Add editable manual ride and delivery prefills without routing, maps,
  carrier, dispatch, or external calendar integrations.

Phase 9.1 implementation:

- `app/presets.js` owns the transient preset catalog and role binding. Preset
  labels and helper-only role metadata are removed by the existing draft wire
  projection, so no preset type or state reaches storage.
- The toolbar menu opens donation, sale, assignment, group coordination,
  service, information, dependency plan, ride, and delivery drafts. Preset
  creation combines editable terms, People, promises, sharing, hierarchy,
  time/place, and dependencies on one setup screen, followed by the unchanged
  signed review screen; blank creation retains its five focused steps.
- The existing `transferCreate` event/lane accepts preset context from other
  sands. Known local Records, People, quantities, and title can arrive already
  filled, reducing common creation to setup review and submission without
  granting the calling sand authority to sign or choose an authenticated actor.
- Presets that need a concrete two-party workflow reject submission until an
  invitee is selected. Donation, service, and information remain valid OPEN
  offers. Every field can still be changed before review, including turning an
  OPEN promise into a concrete one or changing its direction.
- No remote document submission, payment execution, map/routing, dispatch,
  carrier, or external calendar integration was introduced. Remote document
  references retain the Phase 9 behavior and limitations.
- `nix develop -c cargo check --workspace --all-targets` completes warning-free.
- The driven Chromium harness verifies that every preset produces a validated
  ordinary `create-transfer-draft` Action with no persisted preset metadata and
  that preset entry opens the compact two-step composer. It also retains the
  Transfer overview, detail, message-reference, filter, and mobile assertions.
- Two-Cell OPEN discovery was found to still assume ownerless OPEN promises.
  Export and local matching now treat `party_uid` as the required proposer and
  leave only the counterparty role unfilled. The signed cross-Cell package test
  passes; the local matching predicate fix was made after the final bounded
  OPEN-donation test and remains queued for the next verification run.

**Exit gate:** every preset produces a valid ordinary `create-transfer-draft`
payload after its required local choices are made; switching fields remains
fully editable, and neither preset identity nor workflow state is persisted.

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


## OLD STUFF DOWN HERE 

# Theory

_Introduction_
A Transfer is a well structured process for Contributing to other's Needs and having yours met. It might be a way of exchanging physical resources, buying services, exchanging information, proposing the assignment of tasks... It has a few steps: a proposal, an agreement and an action. It is designed to work best with Record's quantities between different Lince Cells.

_Ideas_
I start with information, I say what I can Contribute to or Need, another party confirms that it wants to be part of this Transfer, make it real. We then agree on the specifics and do our part. A Transfer can start out only with the Contribution part, used as a brainstorming mechanism, a way to generate consensus: if we model a Need and all involved agree on one specific Contribution, the following actions and automation are an agreed upon plan. If I post a Need (make a Record public as a Need), anyone that wants to Contribute will give a different solution, suggesting to break it down into other Needs and helping with some of them.

_Automation_
We want to be able to automate the Transfers with built-in mechanisms Karma. To not have to think of a frequent purchase, to commit to it, to show first our Needs and Contributions to a certain Organ/Cells, then if they are not interacted with expand the visibility of them to other parties.

_POV_
I can create a Transfer for a Record as either a Need or a Contribution, might depend on how you see it, use it: let's say I want to play the Guitar live in front of an audience, that can be a Need to perform, or a Contribution as a service, paid or not. Your framing of it as either in the Need part or in the Contribution part will tell people in advance what is your goal. If nobody believes they Need to hear you play they wouldn't hire you, but if you say that you want to be heard playing they wouldn't mind letting you play at an event of theirs, contributing to your Need.

_Conditions_
We might only make a Transfer when we reach a certain state: if Record less than 7 then we buy Apples until we have 14 with a certain specific Lince Cell (known farmer). If we know that our 'Apple' Record's 'quantity' property diminishes by one every day, and we have 8, the day after tomorrow we will have 6, hitting the state to activate a Transfer proposal to buy more Apples. The farmer that wants to buy a new tool to produce more will see that they can't make a Transfer with a tool maker to buy that tool because they dont have the money yet. But because they see only the predictability that the transfer will happen after two days they can plan their business better, knowing that the chain of Transfers will happen 2 days from now.

That can extend to several different Transfers, many people agreeing to many things happening so they all happen. You might need 7 previous Transfers to meet Needs of your customers so they can Contribute to other Needs and buy your Apples. Knowing this you can take part in Contributing to those previous Needs so the flow of Contributions can continue flowing, unclogging the Transfers. You may call it Investing In Your Customers, or Just Helping.

The feature of Simulation is key for this. We might have a Karma to activate a Transfer proposal of donating all extra apples if we hit 20. So if we have 30 we activate that Transfer proposal and wait for someone to collect it. Until then, we see two numbers, the real number (30) and the number that exists in our Lince for our own use (20), the extra 10 will be donated so they are like free Contribution Apples, if we eat one, now there are 9 Contributions we can make, because our donations are on the surplus. Or we make a rule that the eating of the Apple will remove a number from our non donation apples, and the donation stays the same with the 10 Apples.

# Data Structure

The data structure is still being developed, dont mind it.

| Columns               | Data Type | User Input | Data Created |
| --------------------- | --------- | ---------- | ------------ |
| Id                    |           |            |              |
| Records Received      |           |            |              |
| Records Contributed   |           |            |              |
| Agreement             |           |            |              |
| Agreement Time        |           |            |              |
| Transfer Confirmation |           |            |              |
| Transfer Time         |           |            |              |

# Transfer

Main tracker for the Transfer feature.

## MVP Build Order

1. Transfer header with quantity and immutable agreement/settlement modes.
2. Transfer parties.
3. Transfer items with Transfer-specific title/description and source Record snapshots.
4. Transfer interactions and dependencies.
5. Transfer event log and validator.
6. Coordinator-backed event sync cursor for participating Cells.
7. Agreement levels with edit invalidation for connected items.
8. Messages.
9. Karma Transfer quantity tokens.
10. Transfer quantity influence facts.
11. Delivery/receipt confirmations.
12. Individual/full settlement.
13. Server-backed Transfer sand contract and typed backend actions.
14. Transfer sand list/detail/create/agreement UI.
15. Basic peer/contact table and Transfer discovery cache.
16. Optional SQL views or sand queries for richer quantity projections.
17. Visibility subjects, rules, and fields across all Transfer surfaces.

## Not First

- External integrations.
- Global reputation.
- Full peer-to-peer federation.
- Hardcoded expiration.
- Role-based agreement.
- Complex legal-contract language.
- Field-level visibility is intentionally last so the Transfer shape can settle first.

## Cross-File Map

- Product shape, core assumptions, agreement, events, messages, and the main checklist live in this file.
- Remaining reversal/dispute settlement work lives in [Simulation And Settlement](transfer-simulation-settlement.md).

## Core Model

Transfer is a protocol for making Lince Record quantity changes and Record relationships socially valid before they become final database changes.

Principles:

- Transfer deals only with Lince data for now.
- Quantity stays central and generic. Specialized units can be represented by metadata/extensions later.
- A Transfer is a structured promise before execution, and may group smaller item-level interactions.
- Records do not permanently become Needs or Contributions. Need, Contribution, support, task, information, and reservation are Transfer item roles.
- A Transfer item carries Transfer-specific title/description and may hide the private source Record fields.
- Parent Transfers group child Transfers without forcing one agreement or settlement policy on every child.
- Counteroffers are edits. Agreement returns only when the relevant parties accept the edited state again.
- Discovery must not mutate Records. It can suggest candidate assertions or create/edit Transfer proposals, but final Record quantity changes happen only through settlement.
- Transfer status should be derived from item, interaction, agreement, delivery, receipt, settlement, quantity influence, and event facts instead of maintained as a separate source of truth.
- Expiration is not hardcoded in Transfer for the first version; Karma can activate or neutralize Transfers by quantity.
- Role-based agreement, legal-contract language, external payments, delivery integrations, calendars, and external messaging are out of MVP scope.

Intended flow:

1. Someone creates a Transfer from one or more Records.
2. Transfer items describe what those Records mean in this Transfer.
3. Visibility decides which party or Organ can see which fields.
4. Parties edit the parts they are allowed to edit.
5. Connected edits invalidate earlier agreement for affected items/interactions.
6. Parties raise agreement levels when they accept the current visible state.
7. Satisfied agreement policy activates the relevant item, interaction, or group.
8. Transfer influence appears in simulation as plus/minus quantity.
9. Parties confirm delivery and receipt.
10. Settlement applies the actual Lince data changes.

## Implemented State

The structured backend (parties, items, interactions, agreement, settlement, visibility, events, messages, work metadata) is live. The old single-row adapter is gone. The Transfer sand is a server-backed widget with typed actions.

Key locations:

- Schema: `20260614133000_structured_transfer_model.sql`, `20260617120000_generic_work_metadata.sql`
- Domain enums: `crates/domain/src/clean/transfer.rs`
- Widget backend: `crates/web/src/application/transfer_widget.rs`
- Sand UI: `crates/web/src/sand/transfer/`

### Agreement Levels

| Level | Meaning                                             |
| ----- | --------------------------------------------------- |
| `0`   | No current agreement, or invalidated by an edit.    |
| `1`   | First agreement: the party reviewed and is aligned. |
| `2`   | Commitment threshold: the party accepts its part.   |

### Agreement Modes

| Mode         | Meaning                                                       |
| ------------ | ------------------------------------------------------------- |
| `individual` | Each party reaches level 2 independently (default).           |
| `full`       | All parties must reach level 2.                               |
| `percentage` | Ceil(n_parties × pct / 100) parties must reach level 2.       |
| `dependency` | Agreement propagates through dependent interaction agreement. |

### Reservation Policies

| Policy             | Meaning                                                                               |
| ------------------ | ------------------------------------------------------------------------------------- |
| `none`             | Never stage Transfer quantity changes. Only final settlement changes Record quantity. |
| `soft`             | Track proposal intent without reducing availability.                                  |
| `hard_on_proposal` | Reserve outgoing quantity when a proposal is created.                                 |
| `hard_on_consume`  | Reserve outgoing quantity when a proposal is duplicated/consumed.                     |
| `hard_on_lock`     | Reserve outgoing quantity when both sides lock agreement terms.                       |

Each Transfer can override the default in `transfer_tree_config.reservation_policy`.

### Quantity Projections

| Column                                                    | Meaning                                            |
| --------------------------------------------------------- | -------------------------------------------------- |
| `record.quantity`                                         | Actual settled Record quantity.                    |
| `record_transfer_availability.proposed_outgoing_quantity` | Planned negative Transfer influence.               |
| `record_transfer_availability.proposed_incoming_quantity` | Planned positive Transfer influence.               |
| `record_transfer_availability.reserved_quantity`          | Active hard outgoing reservation.                  |
| `record_transfer_availability.reserved_incoming_quantity` | Active positive Transfer influence, informational. |
| `record_transfer_availability.available_quantity`         | Actual minus active hard outgoing reservation.     |
| `record_transfer_availability.planned_quantity`           | Simple projection: actual + incoming − outgoing.   |

```sql
SELECT record.*, availability.*
FROM record
LEFT JOIN record_transfer_availability availability ON availability.record_id = record.id;
```

### Karma

Transfer quantity is exposed to Karma with two equivalent token forms: `tq{id}` and `transfer-quantity-{id}`.

In a condition the token is replaced with the current Transfer quantity (0 if Transfer does not exist). In a consequence the token identifies which Transfer quantity receives the evaluated value. The `transfer-proximity-broadening-{transfer_id}` consequence widens restricted visibility by setting `max_visible_proximity`.

## Status

- [x] Transfer is treated as structured data, not a single immediate transaction.
- [x] Quantity stays central.
- [x] Records can participate in Transfers without becoming permanently Need or Contribution objects.
- [x] Transfer items carry their own title and description.
- [x] Transfer can be nested under a parent Transfer.
- [x] Visibility is first-class data.
- [x] A Lince Cell is modeled through the local Organ/contact model for Transfer networking.
- [x] Personal Organs can publish and consume p2p Transfer summaries.
- [x] Agreement is invalidated by edits to connected items.
- [x] Agreement policies are typed in Rust, not passed around as raw strings.
- [x] Event kinds are typed in Rust, not passed around as raw strings.
- [x] Event payloads are deserialized into typed Rust values at the boundary.
- [x] Karma only activates/deactivates preconfigured Transfers for now.
- [x] Transfer stores enough facts for SQL views and sands to project richer quantity views.
- [x] Simulation can store plus/minus influence facts.
- [x] Delivery and receipt confirmations are modeled separately.
- [x] Settlement is idempotent.
- [x] Transfer proposal data is separate from final Record quantity mutation.
- [x] Transfer history is append-only.
- [x] A coordinator event log can be mirrored by participating Cells.
- [x] Signed events are implemented for local Transfer actions and imported packages.
- [x] Discovery can cache public or permitted Transfer summaries.
- [x] A central or Organ server can introduce Cells to each other.
- [x] Direct Cell-to-Cell sync can happen after introduction.
- [x] The Transfer sand is a real server-backed workflow, not a placeholder.
- [x] The doc set is split into multiple focused files.

## Checklist

### Product Shape

- [x] Transfer is scoped to Lince data only for the first version.
- [x] No payment integration is assumed.
- [x] No delivery-provider integration is assumed.
- [x] No external messaging integration is assumed.
- [x] No calendar integration is assumed.
- [x] No legal-contract language is required for MVP.
- [x] The feature is described as a protocol for making Record changes socially valid.
- [x] The feature supports both personal and shared Organ use.
- [x] The feature supports one-off and grouped work.
- [x] The feature supports large subjects split into smaller child Transfers.

### Core Concepts

- [x] A Transfer is a structured promise before execution.
- [x] A Transfer can contain multiple interactions.
- [x] A Transfer can contain multiple items.
- [x] A Transfer item can represent a Need.
- [x] A Transfer item can represent a Contribution.
- [x] A Transfer item can represent support.
- [x] A Transfer item can represent a task.
- [x] A Transfer item can represent information.
- [x] A Transfer item can represent a reservation.
- [x] A parent Transfer can group child Transfers.
- [x] A parent Transfer can expose aggregate state.
- [x] Child Transfers can keep their own policies.
- [x] Child Transfers can have dependencies.

### Typed Options

- [x] Agreement type is a Rust enum.
- [x] Settlement mode is a Rust enum.
- [x] Agreement level is a Rust enum.
- [x] Transfer role is a Rust enum.
- [x] Transfer direction is a Rust enum.
- [x] Transfer interaction kind is a Rust enum.
- [x] Participation kind is a Rust enum.
- [x] Confirmation kind is a Rust enum.
- [x] Event kind is a Rust enum.
- [x] Storage strings are parsed into Rust types at the boundary.
- [x] Storage strings are serialized from Rust types at the boundary.
- [x] Raw `get("field")` access is avoided in the design.

### Visibility

- [x] Visibility is modeled with tables.
- [x] Visibility applies to Records.
- [x] Visibility applies to Transfers.
- [x] Visibility applies to Transfer items.
- [x] Visibility applies to Transfer events.
- [x] Visibility applies to fields, not only whole rows.
- [x] A subject can be a user.
- [x] A subject can be an Organ.
- [x] A subject can be public.
- [/] A party can see only the Transfer fields allowed for it.
- [/] A party can see a Transfer item title without seeing the source Record head.
- [/] A party can see a Transfer item description without seeing the source Record body.
- [/] Visibility can hide source Record identity.
- [/] Visibility can hide other parties.
- [/] Visibility can hide locations and quantities.

Field-level visibility design (when implemented):

Subjects: an Organ, a specific user within an Organ (by `actor_label`), a Transfer role (`contribution`, `need`, `support`), or `public`. Rules attach to a field path on a Transfer, item, or interaction row. If no rule exists, the field falls back to the whole-Transfer visibility policy.

Fields that can be scoped independently:

- `transfer.title`, `transfer.topic_text`
- `item.title`, `item.description` (visible without exposing source Record)
- `item.source_record_id` / `item.record_head_snapshot` (the Record identity)
- `item.quantity`
- `party.actor_label` / `party.public_key` (hide other parties from each other)
- `interaction.quantity`
- Locations and units carried in item metadata

A rule says "field X is visible to subject Y". Subjects are checked in order: exact actor_label match → role match → Organ match → public. The most specific matching rule wins. A field is hidden unless a matching rule grants access.

Package export applies field rules before serializing: hidden fields are omitted or replaced with a placeholder. The Transfer recipient cannot tell whether a field was intentionally hidden or simply absent.

### Agreement And Editing

- [x] Default agreement mode is individual.
- [x] Full agreement exists as an option.
- [x] Percentage agreement mode is stored as a fraction of parties (0–100).
- [x] Agreement validation branches on the configured AgreementType.
- [x] Agreement percentage threshold is stored per Transfer in `transfer_identity`.
- [/] Dependency agreement mode is wired to interaction dependency satisfaction. (Agreement is complete when the depended-on Transfer itself is agreed/settled, not when local parties sign. Requires querying `transfer_interaction` rows with `depends_on` kind and checking the referenced Transfer state.)
- [x] Agreement mode is displayed in the Transfer sand agreement section.
- [x] Agreement progress (N of M parties agreed) is displayed.
- [x] Editing a connected item invalidates earlier agreement.
- [x] Agreement level 0 means no current agreement.
- [x] Agreement level 1 means first review/align.
- [x] Agreement level 2 means commitment/activation threshold.
- [x] Agreement state is tracked per item or interaction.
- [/] Agreement state can also be derived for a parent Transfer.

### History And Events

- [x] Transfer events are append-only.
- [x] Event hashes can chain together.
- [x] Signed events are implemented for local Transfer actions and imported packages.
- [x] Event validation can be deterministic.
- [x] Each EventKind has a typed payload struct in Rust.
- [x] Event payload deserialization uses typed structs at the package import boundary.
- [x] Invalid event payloads at import are rejected with a validation error.
- [x] Messages are separate from generic comments.
- [x] Messages belong to a Transfer.
- [x] Messages can belong to a specific interaction.

### Messages

- [x] A unified `message` table replaces both `record_comment` and `transfer_message`.
- [x] Messages can reference a Record (replacing Kanban comments).
- [x] Messages can reference a Transfer.
- [x] Messages can reference a specific Transfer interaction.
- [x] Messages support threaded replies via `parent_message_id`.
- [x] Messages are soft-deletable.
- [x] Kanban uses the unified message table for Record comments.
- [x] Transfer packages include messages from the unified table.
- [x] Transfer package import writes messages into the unified table.
- [x] The Transfer sand can display messages in threaded order.
- [x] The Transfer sand can send new messages.
- [x] The Transfer sand can reply to a message.
- [x] The Transfer sand can delete own messages.

### Karma

- [x] Karma can turn a Transfer on by changing quantity.
- [x] Karma can turn a Transfer off by changing quantity.
- [x] Karma does not invent visibility.
- [x] Karma does not invent parties.
- [x] Karma does not silently settle a Transfer.
- [x] Karma-generated actions are bounded.

### Simulation

- [x] Transfer influence is modeled with plus/minus facts.
- [x] Actual quantity remains separate from projected quantity.
- [x] Proposed outgoing can be projected.
- [x] Proposed incoming can be projected.
- [x] Reserved outgoing can be projected for active hard local contribution reservations.
- [x] Reserved incoming can be projected.
- [x] Available can be projected for active hard local contribution reservations.
- [x] Planned can be projected with the simple formula.
- [ ] Surplus can be projected. (Surplus = quantity you have beyond what is already committed to hard reservations — the "safe to give away" figure. Formula: `record.quantity - reserved_quantity`. Distinct from `available_quantity` in that surplus could also account for confirmed incoming deliveries not yet settled.)
- [x] SQL views can explicitly join reservation availability.

### Settlement

- [x] Delivery confirmation is modeled.
- [x] Receipt confirmation is modeled.
- [x] Settlement is idempotent.
- [x] Individual settlement exists.
- [x] Full settlement exists for Transfers where both sides have local Records.
- [x] Settlement readiness includes structured interaction dependency state.
- [x] Settlement can apply Record quantity changes.
- [x] Settlement can consume reserved influence facts for the local Transfer path.

### Networking

- [x] A Cell can act as a p2p node through Organ contacts, package endpoints, polling, and outbox retry.
- [x] A node can publish visible Transfer summaries.
- [x] A node can cache public/permitted Transfer packages.
- [x] A node can keep discovery cache entries stale with source metadata.
- [x] A participating Cell can mirror a Transfer event log.
- [x] A participating Cell can track its last synced event.
- [x] A coordinator orders writes while replicas sync eventually.
- [x] A central or Organ server can introduce peers.
- [x] Direct peer sync can happen after introduction.
- [x] Peer discovery can be contact-list based.
- [x] Known peers can be auto-polled hourly by default.
- [x] Known peers can be manually polled when automatic polling is disabled.
- [x] A node can expose discoverable contacts with pagination and text search.
- [x] A discovered contact can be added locally as `unknown`.
- [x] Peer trust supports `unknown`, `known`, and `blocked`.
- [x] Blocked peers are rejected from receive, send, polling, and discovery surfaces.
- [x] Transfer topics/categories can be manual text input.
- [x] Public proposal ingress is integrated with unknown/known/blocked peer behavior.
- [x] Topic/category labels can be used by future discovery.
- [x] Structured package rows use stable UIDs for parties, items, and interactions.
- [x] Structured package import updates existing party/item/interaction rows by UID.
- [x] Structured package import preserves local-only rows when importing partial remote packages.
- [x] Structured package import has service coverage for idempotent UID-based updates.
- [x] Organ discovery is available through `/organs/discover`.
- [ ] Gossip cache is available as a long-term discovery helper.
- [ ] Delegated ask-around search is available with hop/TTL limits.
- [x] Event logs can later become signed.

### Visibility V1

- [x] Transfer visibility defaults to hidden.
- [x] Transfer visibility mode is exclusive: `hidden`, `public`, or `restricted`.
- [x] Whole-Transfer package export is the v1 visibility boundary.
- [x] Package export checks the requesting Organ before sending a Transfer package.
- [x] Public visibility allows public package discovery/export.
- [x] Restricted visibility supports explicit Organ allow rules.
- [x] Restricted visibility supports `max_visible_proximity`.
- [x] Blocked Organs cannot receive visible Transfer packages.
- [x] Manual send to an Organ ensures that Organ can view the Transfer.
- [x] Organ proximity is stored as a numeric Organ property.
- [x] The Transfer sand can edit Organ proximity.
- [x] The Transfer sand can edit whole-Transfer visibility.
- [x] The Transfer sand can choose hidden/public/restricted visibility.
- [x] The Transfer sand can choose restricted Organs and a max proximity threshold.
- [x] Received package state is stored locally.
- [x] Seen package state is stored locally when a user opens Transfer detail.
- [x] Receipt configuration exists for received receipts, seen receipts, and anonymous package viewing.
- [x] Anonymous package viewing avoids generating received/seen state.
- [x] Received/seen package facts can become signed outbound Transfer events.
- [x] Karma consequences can widen restricted visibility with `transfer-proximity-broadening-{transfer_id}`.
- [x] Offer ordering sends eligible Transfers to closer Organs first without exposing local proximity externally.
- [x] Visibility-aware projection sharing is default-off and gated by configuration.

### Transfer Sand

- [x] The Transfer sand uses server-backed widget actions while remaining an official local widget.
- [x] The Transfer sand declares the permissions it needs.
- [x] The Transfer sand has a dedicated runtime contract.
- [x] The Transfer sand has typed backend actions.
- [x] The Transfer sand can list Transfer summaries.
- [x] The Transfer sand can load one Transfer detail.
- [x] The Transfer sand can create a Transfer.
- [x] The Transfer sand can create child Transfers.
- [x] The Transfer sand can add and edit Transfer items.
- [x] The Transfer sand can delete Transfer items.
- [x] The Transfer sand can assert a relationship from a Transfer item to a source Record.
- [x] The Transfer sand can show Transfer-specific item title and description separately from source Record fields.
- [x] The Transfer sand can configure parties.
- [/] The Transfer sand can configure field-level visibility.
- [x] The Transfer sand can show item interactions.
- [x] The Transfer sand can show dependencies.
- [x] The Transfer sand can show agreement state.
- [x] The Transfer sand can let permitted parties agree.
- [x] The Transfer sand invalidates agreement through backend rules after connected edits.
- [x] The Transfer sand can send and display Transfer messages.
- [x] The Transfer sand can display messages in threaded order.
- [x] The Transfer sand can show append-only Transfer history.
- [x] The Transfer sand can show delivery confirmation state.
- [x] The Transfer sand can show receipt confirmation state.
- [x] The Transfer sand can request settlement.
- [x] The Transfer sand can show quantity influence facts when they exist. (Per Transfer, shows which local Records will be affected and by how much — `proposed_incoming`, `proposed_outgoing`, `planned_quantity` from `record_transfer_availability` — before settlement actually runs.)
- [x] The Transfer backend projection exposes Transfer-level work metadata when it exists.
- [x] The Transfer sand can edit Transfer work metadata.
- [x] The Transfer sand can show and edit item work metadata.
- [x] The Transfer sand can show and edit interaction work metadata.
- [x] The Transfer sand can configure whole-Transfer visibility.

### Multi-Item And Interaction CRUD

- [x] The Transfer sand can create new items with full fields (role, title, description, source Record assertion, quantity, unit).
- [x] The Transfer sand can edit all fields of an existing item.
- [x] The Transfer sand can delete items.
- [x] The Transfer sand can create new interactions (from/to item, kind, direction, dependency kind, quantity).
- [x] The Transfer sand can edit existing interactions.
- [x] The Transfer sand can delete interactions.
- [x] Backend actions exist for create/edit/delete of structured items.
- [x] Backend actions exist for create/edit/delete of structured interactions.

### Roadmap

- [x] The doc is split into multiple files.
- [x] The main file is a tracker.
- [x] The main file has many checkboxes.
- [x] The plan can grow without becoming one monolith.
- [x] The schema and Rust models have a structured backend implementation.
- [x] Generic Kanban work metadata can attach to Transfers.
- [x] Generic Kanban work metadata can attach to structured Transfer items.
- [x] Backend Transfer summary/list projection reads from structured parties/items/agreements.
- [x] The local create/edit/agreement/inactivation action surface writes structured rows first.
- [x] Transfer package import/export uses structured rows without the old `TransferItemPackage` fallback.
- [x] Delivery, receipt, and settlement write/check structured confirmation and settlement rows.
- [x] The legacy `transfer_item` table is removed from the Rust schema and dropped by migration.
- [x] Work metadata owner kind for structured Transfer items no longer uses the legacy `transfer_item` name.
- [x] The UI action surface has native multi-item and interaction creation/editing.
- [x] Explicit reservation projection is available through `record_transfer_availability`.
- [x] Visibility-aware package export filtering is implemented for whole-Transfer visibility.
- [x] The networking protocol carries Transfer packages over structured Transfer data.

### Multi-Party Settlement

- [x] Schema supports N parties per role in `transfer_party` and `transfer_structured_item`.
- [x] `settle-all-local` action settles all structured items owned by local parties in one call.
- [x] Settlement delta uses interaction quantities when defined, falls back to item quantity.
- [x] Many-to-many fulfillment: one contribution item can relate to multiple need items via interaction quantity routing.
- [x] Settlement event payload records party_id, record_id, and delta per item.
- [x] "Settle my parts" button in Transfer sand items section.

### Transfer Chains (Karma Sequence)

Transfer chains are private, Organ-local chain edges that connect the settlement of one Transfer to the contribution input of another. A receives from B (Transfer 1). B privately connects Transfer 1's settlement to their contribution in Transfer 2 (B→C). Neither A nor C sees that chain edge. No anonymous data inside Transfer events or packages.

- [x] `transfer_chain_link` table stores cross-Transfer flow connections (organ-local, never in packages).
- [x] Chain edges support `constant` or `percentage` amount formulas.
- [x] `add-chain-edge` / `remove-chain-edge` backend actions.
- [x] Settling a Transfer triggers pending chain edges: computes delta and marks downstream item as funded.
- [x] Chain-edge state tracks: pending → triggered → canceled.
- [x] Transfer sand shows a chain-edge section (upstream and downstream views, private to the local Organ).

### Spectator Watching

A spectator watches a **source Transfer** (by `source_transfer_uid`) and a **role** (contribution or need). When ANY Transfer derived from that source has the matching role settle, all active spectators are triggered.

- [x] `transfer_spectator` table stores watches (organ-local, never in packages).
- [x] Spectator watches a `watched_source_transfer_uid` + `watched_role`, not a specific Transfer instance.
- [x] Triggering the wrong duplicate still satisfies spectators (watches point to source, not instance).
- [x] Spectator can watch contribution role (supply materializes) or need role (need met).
- [x] Spectator applies `constant` or `percentage` delta to a local Record when triggered.
- [x] `add-spectator` / `remove-spectator` backend actions.
- [x] Spectators are triggered after `settle-all-local` runs.
- [x] Transfer sand shows "Watching" section (private to local organ).

### Satiation Policy

When a Transfer or spectator settles, a satiation policy can auto-cancel sibling Transfers from the same source.

- [x] `transfer_satiation_policy` column on `configuration` for global default.
- [x] `satiation_policy` column on `transfer_identity` for per-Transfer override (NULL = inherit).
- [x] Policy `none`: all duplicates proceed independently.
- [x] Policy `first_completes`: when any duplicate settles, inactivate non-settled siblings.
- [x] Satiation runs after `settle-all-local` and after spectator trigger.
- [x] `set-satiation-policy` backend action.
- [x] Transfer sand shows satiation policy picker with inherit/none/first_completes options.

### Backend Features Without Transfer Sand UI

Backend actions and capabilities that exist but have no UI surface in the Transfer sand yet.

- [x] `set-transfer-reservation-policy` action exists but the Transfer sand has no control for it. The five reservation policies (`none`, `soft`, `hard_on_proposal`, `hard_on_consume`, `hard_on_lock`) can be set per Transfer but only via direct API calls. → Added reservation policy dropdown in the Transfer tree section.
- [x] Transfer tree sync mode (`live` vs `snapshot`) can be configured per tree node but the Transfer sand only shows one-time sync buttons, not the configured mode or a toggle. → Configured sync mode dropdown already shown alongside "Sync now" button.
- [x] Transfer tree branch mode (`inherit`, `duplicated`, `greedy`) is displayed in the Transfer sand tree section but the `greedy` option is not explained and the effect of changing it mid-tree is not surfaced. → Added `title` attributes to branch mode options explaining what each does.
- [x] Transfer dependency interactions (`depends_on` kind with `must_agree`, `must_activate`, `must_deliver`, `must_receive`, `must_settle` dependency kinds) are stored but the Transfer sand does not show whether the dependency is currently blocking progress or satisfied. → Interactions with unresolved dependency kinds now show a yellow "blocking" badge and a left border.
- [x] The global Transfer satiation policy default in `configuration.transfer_satiation_policy` has no UI in the Transfer sand configuration section. → Added global satiation policy select in the Network drawer; populated from `snapshot.globalSatiationPolicy`.
- [x] `surplus_quantity` (actual minus hard reserved) is not computed or surfaced anywhere in the UI. → `surplus_quantity = actual − reserved − proposed_outgoing` now computed in `load_influence_facts` and displayed per-Record in the Quantity influence section.

Trust is important in workflows of Transfer, when we interact with other parties how do we know we can Trust them? Trust in Lince is done by verifyiable facts that happened when this party interacted with other parties. If you consider the parties they interacted with as Trustworthy, when both parties can agree something was done, you can maybe trust them a little more. For automation this is crucial, being able to trust will bring more agility to interactions.

> Extracted from `docs/Karma.md` on 2026-07-29. Automation Trust scopes
> (Karma phase K8) stay in that doc — they gate automation, not authorship.

## [x] Trust

- [x] Every locally-authored fact is signed on the write path; imported
  facts keep their ORIGIN signature so downstream Cells can still verify the
  original author — a two-layer tamper model (the hash chain guards
  content, the signature guards authorship).
- [x] Compaction archives stay verifiable file-side; the anchor fact makes
  the file tamper-evident from inside the Ledger.
- [x] No universal/global/public reputation score, ever. Kept/broken signed
  history is the shareable raw material; Karma may derive a local,
  purpose-specific estimate for one concept/role/window, but must show its
  ingredients and must not publish it as a fact about a Person's character.
- [ ] Karma phase K8 adds local Automation Trust scopes above coarse
  `known|blocked`: exact concept/direction and stage ceilings, Person/origin-
  Organ/via-Organ/proximity selectors, probability/confidence thresholds,
  limits, expiry, and deny-first Proof. This policy gates automation only; it
  is not reputation, visibility, consent, or a Program grant.
- Field-level grants and most-specific-wins precedence are tracked in Transfer
  Phase T3, with Transfer as the first consumer of this shared Trust behavior;
  today grants remain whole-row only.
