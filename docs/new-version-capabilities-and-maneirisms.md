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

- [x] `create-promise` (including OPEN, unfilled-party Needs/Contributions),
  `promise-transition` (validated state machine: open → proposed → agreed →
  active → kept/broken/withdrawn), `edit-promise-delta` (a counteroffer —
  resets every party's agreement).
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
  `advance_transfer` (never past policy), `activate`/`deactivate` (works on
  rules too — quiet hours is just a rule turning another rule off), `ask`,
  `notify` (budgeted).
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

Phases T0-T4 deliberately ship the complete human-driven workflow first.
Transfer-aware Karma and learned recommendations are not removed: they are the
final T5 integration after the manual semantics and evidence are trustworthy.
An explicit user-authored rule may eventually automate permitted local Actions,
but no rule or recommendation can forge another person's agreement or claim
that a real-world event happened. External payment execution, carrier APIs,
legal-contract machinery, and Fiote are out; payment/contracts may travel as
links or messages. Promise time windows and calendar constraints are core even
though external calendar-provider integration is not.

### Shipped contract to build on

- [x] Status is derived, never stored: `inactive → draft → proposed →
  agreed → in_transfer → settled`.
- [x] `create-transfer` (agreement `individual|full|percentage|dependency`,
  `satiation`, `reserve_default`, `require_confirmation`), `add-party`,
  `add-promise-to-transfer` (a condition string makes it a chain link or a
  spectator, the same grammar as Karma conditions).
- [x] `agree-transfer { level }` — level 2 also advances that party's
  bundled promises to agreed; editing any bundled promise drops everyone
  back to 0.
- [x] `activate-transfer` (within policy), then `settle-transfer` — the ONLY
  thing that mutates record quantities, idempotently, `cause=settlement`;
  chains fire, satiation withdraws sibling bundles.
- [x] `require_confirmation` gates settlement on both a `"delivery"` and a
  `"receipt"` confirmation annotation fact.
- [x] Advisory-only `balance`/`balanced` on every transfer row (per-concept
  promise sums — a trade sums to zero, a donation deliberately doesn't;
  never blocks).
- [x] Transfer chat is generic: `create-thread` + `create-message` on the
  transfer record, read back via `include: { threads }` — no
  transfer-specific message model.

### Phase T0 — decisions and contract hardening

- [x] The first decision pass is recorded in **Standing Transfer decisions**
  below; implementation must preserve those decisions instead of inventing UI
  policy.
- [ ] Make transfer options typed at the Action boundary and reject invalid
  combinations: agreement kind/percentage, agreement level, satiation,
  reservation point, confirmation kind, finite non-zero deltas, valid windows,
  and conditions that parse. A bad proposal must fail before partial rows land.
- [ ] Bind party-sensitive Actions to the authenticated viewer/Cell. A client
  must not impersonate another actor in `add-party`, `agree-transfer`,
  `confirm-transfer`, or `settle-transfer`; parties are Person records, while
  Organs only route/authenticate their signed operations. Define permissions
  for creator, invited person, participant, delegated bulk operator, and
  read-only observer.
- [ ] Make every multi-row Action transactional and concurrency-aware. Edits
  carry a revision/precondition, stale counteroffers fail visibly, agreement
  invalidation happens in the same transaction, and retries remain idempotent.
- [ ] Sign promise state transitions (facts already sign on the write path;
  populate and verify the transition signature/evidence rather than trusting
  an unsigned sidecar state).
- [ ] Make confirmation evidence role-specific and idempotent: delivery and
  receipt identify the confirming party, cannot both be satisfied by an
  unauthorized actor, expose who/when, and have a defined correction path.
- [ ] Define terminal and exceptional states in the derived model: withdrawn,
  expired, broken, rejected/cancelled, partially settled, disputed, and
  reversed/compensated. Do not flatten these into `draft` or `settled`.
- [ ] Add typed persistent policy Actions/Protein for Cell defaults and
  per-transfer/tree overrides: reservation, partial-remainder behavior, sync
  mode, visibility, hierarchy/satiation, bulk completion, recommendation
  thresholds, and any later automation authority. Policy changes carry facts
  when they alter a live transfer's meaning.

### Phase T1 — complete manual Action and Protein contract

- [ ] Expand `source: "transfer"` detail rows with every value the workflow
  needs: `reserve_default`, `require_confirmation`, source/sibling identity,
  promise windows/conditions/reservation source, confirmation evidence,
  settlement facts, creator/participant capabilities, and explicit blocking
  reasons. Keep status, balance, availability, and readiness engine-derived.
- [ ] Give the transfer source explicit status/party/concept/window predicates
  and deterministic sorting for the list/inbox views. Unsupported predicates
  must return an error instead of silently matching every transfer.
- [ ] Make each transfer-sidecar mutation announce a commit for live Protein
  subscribers (party/promise add or remove, edit/invalidation, agreement,
  activation, confirmation, settlement, cancellation). Two open sands must
  converge without a manual refresh or UI-fabricated state.
- [ ] Add typed edit Actions for transfer title/settings and the complete
  counteroffer surface (record, party, delta, window, condition/reservation),
  each invalidating the right agreements; changing only the delta is not a
  complete negotiation flow.
- [ ] Add invitation/acceptance: the creator may address an invitation to a
  Person, but that person is not a participating party until accepting or
  claiming an OPEN slot. Rejection/expiry is explicit and cannot bind them.
- [ ] Add safe remove/withdraw Actions for parties, promises, and whole
  transfers, with clear rules once another party has agreed or any promise has
  become active/kept. Ledger evidence stays append-only.
- [ ] Complete OPEN-promise claim → proposal and Senses decision "propose" →
  actual transfer proposal delivery to the selected remote Organ, without any
  Karma dependency.
- [ ] Finish cross-Cell proposal transport: visibility-gated export, remote
  acceptance/counteroffer, retry/idempotency, conflict handling, and live local
  refresh over the existing sync/outbox boundary, including negotiation threads
  and evidence. Wire the Organ polling scheduler needed to make delivery happen
  without a manual engine call.
- [ ] Keep network delivery acknowledgement distinct from business fulfillment:
  “package received/seen” must never satisfy “goods delivered/receipt
  confirmed,” even when both travel through the same sync package.
- [ ] Define and expose partial/multi-party settlement semantics. A participant
  settles only its authorized promises; the transfer shows per-party progress,
  each give/receive occurrence has claims from both sides, repeated settlement
  is a no-op, and remote facts retain original signatures. A local bulk action
  may mark selected occurrences/branches for one Person but emits individually
  attributable evidence and never speaks for the counterparties.
- [ ] Persist partial-fulfillment policy with `transfer > Cell default > code
  default` precedence. The conservative default records the partial occurrence
  and leaves the remainder visible; an opt-in policy may create a local draft
  correction/remainder Transfer, but never send, agree, or settle it silently.
- [ ] Add a manual correction path for mistakes after activation/settlement:
  withdraw/reject before activation; mark unperformed remainder broken after
  activation; compensate an erroneous private quantity fact; use a linked
  reversing/correction Transfer for a social obligation; and annotate disputes
  without rewriting the original evidence.
- [ ] Add `reopen-promise`: reopening keeps the same Transfer but creates a
  linked successor revision of the terminal/expired promise with a new window.
  The old promise and its signatures remain immutable and agreements reset.
- [ ] Add explicit transfer sequencing/order within chains, distinct from
  conditional promise expressions; validate cycles as warnings and show the
  engine-computed blocked-by/readiness state.
- [ ] Replace scalar `quantity_influence` with a private deterministic
  `application_formula` per promise/transfer, sharing the pure expression
  grammar with Karma (for example `incoming * 2 + 1`; a constant is valid).
  The public occurrence preserves what the giver actually gave; only the local
  record delta uses the formula. Formula changes do not rewrite public terms,
  and their availability/projection/audit effects remain idempotent.
- [ ] Expose `actual`, `available`, `reserved`, `planned`, and `surplus` with
  units/conversion rules from the engine. The sand must never reproduce this
  arithmetic, and an unknown/incompatible unit must be explicit rather than
  silently balanced.

### Phase T2 — Transfer sand essential workflow

- [x] Create a focused package under `crates/web/src/sand/transfer/`, split
  into small body/style/app modules. Register it as an official sand and remove
  or repair any catalog/group/selftest references to deleted legacy sands.
- [ ] Build list/inbox/detail navigation on live Protein: mine, awaiting me,
  awaiting others, active, completed, cancelled/broken, and discoverable
  proposals; search/filter/sort without copying durable data into host state.
- [ ] Build a guided create flow from blank state, an OPEN promise/discovery
  result, or one/more selected Records. Preview signed delta direction as
  Need/Contribution in plain language before the Action is sent.
- [ ] Build party and promise composition for N parties and N promises with
  record/concept/unit autocomplete, window, optional condition, availability,
  balance advisory, and precise inline validation. Donations remain valid when
  deliberately unbalanced.
- [ ] Build negotiation and counteroffers: show the current revision, changed
  terms, per-party milestone (`0` none/invalid, `1` signed agreement, `2`
  signed commitment/contract locked), mutual matched-level progress, who is
  blocking, and the fact that any public-result edit resets affected agreement
  before the user confirms it. Fulfillment confirmation is the later third
  milestone, not agreement level 3.
- [ ] Build execution controls from engine capabilities: activate, confirm
  each give/receive occurrence from both sides, settle/apply my authorized
  private part, bulk-complete a reviewed tree selection, cancel/withdraw, and
  retry. Dangerous or irreversible steps get a review dialog showing exact
  public occurrences and private record formulas/deltas; disabled controls
  state the backend-supplied reason.
- [ ] Show status and progress as one compact timeline derived from parties,
  promises, confirmations, settlement, expiry, and exception state. Never let
  optimistic UI claim agreement or settlement before the pushed Protein does.
- [ ] Show quantity impact by Record (`now`, `available`, `reserved`,
  `planned`, `surplus`, settlement delta/influence), per-concept balance, and
  source Fact links. Use the shared Record sand for full record inspection.
- [ ] Add negotiation threads using the generic thread/message contract:
  multiple threads, replies, timestamps/sender, search, `@slug` links, and
  permission-aware deletion, matching the Record sand rather than forking chat.
- [ ] Add a history/proof drawer backed by facts and transition evidence:
  proposal/edit/agreement invalidation/confirmation/settlement/compensation,
  actor and time, signature state, and Action warnings. It is an audit view,
  not a second custom event store.
- [ ] Add hidden/public/restricted visibility and explicit Organ recipients
  using the common visibility Actions. Show exactly what will be sent; blocked
  contacts cannot be selected, unknown non-contact identities may respond only
  to a directly addressed or public proposal, and private local formulas/notes
  do not leak into packages.
- [ ] Cover loading, unconfigured Protein, empty, offline/reconnecting, stale
  revision, forbidden, partial import, validation failure, warning, and retry
  states; preserve dirty forms across live snapshots and support keyboard and
  narrow/mobile layouts without overlapping controls.

#### Implemented slice T2a — truthful read surface (2026-07-18)

This is the first deliberately small workflow phase. It has no integration,
Karma, invitation, negotiation, or settlement mutation surface. Social writes
stay hidden until authenticated party capabilities and revision-bound Actions
exist; the sand does not present a client-supplied actor as authority.

- [x] Add one live `source: "transfer"` subscription and make Transfer facts
  invalidate that subscription. Selection never opens a parallel detail query
  or a client-maintained transfer model.
- [x] Expand the Transfer overview projection with visibility/proximity,
  parent/source identity, reservation and confirmation policy, reviewed and
  committed agreement counts, policy readiness, per-state progress,
  actor-attributed confirmation facts, Person names, and promise Record,
  concept, unit, quantity, window, condition, and reservation fields.
- [x] Derive and preserve `withdrawn`, `broken`, and `partially_settled` in the
  overview instead of flattening them into draft/settled.
- [x] Render the big-picture portfolio with total/attention/open/settled
  counts, high-detail transfer rows, agreement progress, parties, promise
  deltas, search, filter, and deterministic local presentation sorting.
- [x] Clicking a transfer opens the high-control inspection view: policy,
  agreement by Person, promises and deadlines, per-concept balance,
  confirmations, parent/source lineage, stable IDs, and navigation to the
  shared Record sand. On narrow screens it replaces the list; Escape/Back
  returns to the overview.
- [x] Keep only selected uid, filter, and sort in namespaced card state. No
  durable transfer fields or arithmetic are copied into board/browser state.
- [x] Make transfer creation fact-backed and annotate the existing
  add-party/add-promise/edit/agreement/activation sidecar mutations so live
  subscribers receive commit invalidation.
- [x] Enforce finite non-zero bundled deltas, Person-only party records, and
  party-to-transfer membership before agreement; level 2 advances only the
  selected Person's promises.
- [ ] Finish server-derived audience inbox partitions (`mine`, `awaiting me`,
  `awaiting others`, and discoverable) after viewer-to-Person authority is
  explicit. The current sand groups by workflow status only.
- [x] Add server-derived viewer capabilities and stable blocking reasons before
  enabling creation controls.
- [ ] Add settlement facts, availability, sibling/tree roll-up, and explicit
  server ordering before enabling negotiation/execution controls.

#### Implemented slice T2b — authorized atomic creation (2026-07-18)

This phase implements manual creation only. It adds no payment, calendar,
carrier, legal, contact-sync, Karma, recommendation, agreement, confirmation,
activation, or settlement integration/control.

- [x] Add an explicit one-to-one `app_user` to Person mapping in database
  state. Administrators assign it with `assign-user-person`, protected by
  `user:assign_person`; the Permissions sand exposes the assignment without
  inferring identity from a username, display name, or slug.
- [x] Derive Transfer authority from the authenticated WebSocket subject.
  Authenticated creation requires `transfer:create` and a mapped Person;
  mutation requires `transfer:update` plus creator/participant relationship.
  Local no-auth mode remains the explicitly trusted Cell mode.
- [x] Project one `transfer_context` row containing viewer identity, creation
  capability, and stable blocker codes, plus per-transfer capabilities,
  blocker codes, and the viewer's transfer-party uid. The sand filters the
  context row out of durable Transfer rows rather than inferring permission.
- [x] Add typed `create-transfer-draft` with agreement enum/percentage,
  sibling satiation, reservation point, confirmation policy, visibility and
  proximity, parent/source, Person parties, and promise Record/Person/delta/
  window/condition/reservation inputs.
- [x] Validate the complete draft before writing: strict slug/title, coherent
  percentage/proximity/satiation combinations, bounded unique Person parties,
  finite non-zero quantities, future RFC3339 windows, parsed condition
  expressions, dependency conditions, hierarchy Record kinds, and every
  promise Person belonging to the reviewed party set.
- [x] Commit the Transfer Record, sidecar settings, parties, promises,
  creator/public visibility grants, signed creation Fact, and quantity cache
  update in one SQLite transaction. After commit it enters the normal live
  Fact bus and reactive cascade path.
- [x] Automatically add the authenticated creator's mapped Person without
  accepting a client claim about who the creator represents. Hidden drafts
  remain visible to their creator and named participants.
- [x] Add a five-stage Transfer composer: terms, People, promises,
  sharing/hierarchy, and final review. It uses controlled Person/Record
  selectors and covers agreement policy, reservation, confirmation,
  give/receive direction, quantity, deadline, condition, visibility,
  proximity, parent Transfer, and source Record.
- [x] Review the exact signed delta twice: as the public transfer term and as
  the current private Record effect. They match in this phase because private
  `application_formula` is not implemented; the sand does not invent an
  inverse or silently apply a future formula.
- [x] Submit one typed Action and keep the composer pending until the created
  uid is present in the live Transfer Protein. If the live row arrives before
  the Action response, the already-projected row satisfies the same rule; no
  optimistic Transfer object is fabricated.
- [ ] Add Record/OPEN-promise cross-sand creation entry points and prefilled
  multi-Record drafts. This phase starts from the Transfer sand's blank state.
- [ ] Add place/location terms after a typed transfer-place contract exists;
  creation currently covers time windows and hierarchy but does not smuggle a
  client-only location into the reviewed result.
- [ ] Add revision-bound edit/counteroffer Actions before enabling agreement
  or execution controls. Confirmation and settlement capabilities remain
  false with explicit scope-not-modeled blockers.

### Phase T3 — deferred connected capabilities

No integration work is part of the current Transfer implementation. These
items remain organized here as later dependencies rather than being partially
wired into the simpler local workflows.

- [ ] Add cross-sand entry points instead of private duplicates: Record can
  start/inspect a transfer; Relations can drag/link a Contribution to a Need;
  Organ contacts can open received proposals; transfer rows emit scoped
  `recordClicked` for the shared Record sand.
- [ ] Build the transfer marketplace/discovery view from visibility-gated OPEN
  promises and cached remote proposals: concept/topic, direction, quantity,
  unit, window, place/proximity, confidence evidence, freshness, and source
  Organ. Claiming creates a proposal; browsing never mutates a Record.
- [ ] Add manual parent/child bundles, sibling satiation (`none` or
  `first_completes`), dependencies, and explicit ordering as a graph/list view.
  A whole transfer tree can be agreed/executed together; parent status is
  derived, children retain their own policy/evidence, and dependency/order
  cycles are hard errors because they cannot be executable plans.
- [ ] Support both persistent social delivery modes per contact/transfer:
  `hosted` opens the authoritative remote Transfer after login, while
  `replicated` imports signed events/data into this Cell and exposes conflicts.
  Unknown contacts default hosted/reference-only; replication requires an
  explicit choice. Both modes show freshness and offer manual refresh/retry.
- [ ] Complete the shared field-level visibility precedence (actor > role >
  Organ > public) with Transfer as its first demanding consumer, so a proposal
  may reveal its own label/description without leaking source Record identity,
  other parties, exact quantity, place, conditions, threads, or proof fields.
- [ ] Integrate projection/timeline once its transport verb exists so a user
  can inspect transfer effects and branch a hypothetical future. Projection is
  read-only until a separate explicit Action commits a real proposal.
- [ ] Integrate place/route/window comparison once offline OSM route support
  exists, enabling manual RIDE/DELIVERY proposals without embedding route math
  in the sand.
- [ ] Make promise start/end windows, deadlines, recurrence context, timezone,
  and overlap visible/editable in every relevant create/detail/tree view.
  External calendar linking may be generic record links/messages; core time
  constraints cannot wait for a calendar provider.
- [ ] Integrate the generic call sand once it exists: a participant may manually
  start/join a call scoped to the transfer and its parties. No Transfer-specific
  media stack and no Karma-triggered calling belongs in this workstream.
- [ ] Treat assignment, service, information, donation, sale, ride/delivery,
  and multi-party exchange as presets over the same promise bundle, not new
  schemas. Presets may choose labels/defaults but never change settlement law.
- [ ] Implement presets and acceptance in this order: donation, sale,
  assignment, then service/information, dependency tree, and ride/delivery.
  “Ship order” means implementation plus tests, not separate releases.
- [ ] Keep Transfer work/body/links/attachments in the shared Record surface
  and generic links/extensions; do not rebuild the deleted transfer-private
  work metadata, attachment, or comment systems.

### Phase T4 — manual workflow proof and release gate

- [ ] Engine tests cover policy matrices, invalid transitions/options,
  agreement invalidation, reservation/availability, confirmation authority,
  partial/multi-party settlement, expiry/broken/cancel/correction paths,
  chain ordering/cycles, application formulas, concurrency, and idempotency.
- [ ] Transport/Protein tests prove snapshots plus live invalidation, visibility
  redaction, Action warnings/errors, authenticated actor binding, retry/replay,
  and signed transition/settlement evidence.
- [ ] A driven Chromium selftest proves create → compose → counteroffer → agree
  → activate → confirm → settle → history/chat, plus offline, forbidden, stale
  edit, warning, cancellation, and responsive keyboard flows on the current
  bridge.
- [ ] Two real Cells prove DONATION and SALE end to end through their Transfer
  sands: publish/send, receive, counteroffer, both parties agree, confirmations,
  per-Cell settlement, signed fact sync, retry without duplication, and no
  hidden-field leakage.
- [ ] Acceptance presets prove manual assignment/group coordination,
  information/service exchange, dependency chain, and first-completes
  satiation. RIDE/DELIVERY proof waits only on the explicit OSM dependency.
- [ ] Manual-workflow definition of done: no legacy Transfer endpoint/model is
  restored; no transfer arithmetic or authority rule lives only in JavaScript;
  all durable writes are Actions, all live reads are Protein, warnings remain
  advisory, vendored assets carry licenses, `cargo check` is warning-clean,
  and focused Rust plus browser tests pass.

### Phase T5 — Karma + recurrence recommendation integration (last)

- [ ] Define a separate local `recurrence_likelihood` for “this Need/action is
  likely in this future window”; do not reuse counterparty `confidence`, which
  means kept-promise history. Pattern keys may include Action/preset, concepts,
  people/proximity class, quantity band, time/window, place, and selected
  context, but only from data visible to the local Cell.
- [ ] Implement deterministic diminishing-return evidence with time decay and
  user-tunable parameters. The starting candidate is
  `p = 1 - (1 - prior) * exp(-growth * Σ exp(-ln(2)*age/half_life))`, refined by
  recurrence/window fit; store prior, growth, half-life/decay, thresholds, and
  scope in typed database policy with per-pattern overrides. DST proves the
  exact formula and boundary behavior before it becomes product law.
- [ ] Learn only from human-authored or mutually confirmed outcome facts.
  Recommendations, generated drafts, and Karma-created drafts cannot feed their
  own likelihood until a real human/confirmed outcome occurs, preventing a
  self-reinforcing automation loop.
- [ ] Feed likelihood into Attention with evidence: at the configurable
  suggestion threshold (initial default `0.60`), create one deduplicated
  recommendation explaining the matched history, expected time, probability,
  and consequence of acting or ignoring it.
- [ ] At a higher opt-in auto-draft threshold (disabled by default), create or
  refresh a local Transfer draft/remainder draft. It is not sent, agreed,
  confirmed, or settled merely because probability crossed a threshold.
- [ ] Generate disabled Karma-rule drafts when a repeated pattern crosses its
  configured automation-candidate threshold. The user reviews its formula,
  scope, visibility, Action budget, recipients, decay, and expiry before
  enabling it; suppressing a pattern is durable.
- [ ] Make the simple Transfer Action vocabulary available to explicit Karma:
  create/fill a promise or draft, publish/send only within an approved
  visibility/recipient policy, activate only after matching signed commitment,
  and apply the local settlement formula only after the required dual
  fulfillment evidence. Karma can never agree or confirm for another person.
- [ ] Treat a fully explicit deterministic user rule as 100% user-side intent,
  distinct from learned probability. It still passes through engine authority,
  agreement, visibility, budget, idempotency, and confirmation gates.
- [ ] Surface suggestion and automation policies in the Transfer/Karma
  settings backed by database state: suggestion and auto-draft thresholds,
  growth, decay half-life, evidence horizon, allowed presets/people/Organs,
  action budget, quiet time, and per-pattern disable/override.
- [ ] Prove milk/pantry recurrence, repeated donation, recurring purchase,
  assignment, partial-remainder draft, evidence saturation, decay below
  threshold, deduplication, opt-out, no self-training, no private-data leak,
  and no automatic social commitment in engine plus two-Cell browser tests.

### Standing Transfer decisions (2026-07-18)

- Parties are Person records. Organs are identity transport, visibility, trust,
  and sync boundaries, never social parties. Another Person becomes a party
  only after accepting an invitation or claiming an OPEN promise.
- Each Cell settles only Records it owns. Public/shared evidence says which
  giver provided what canonical amount to which receiver; the receiving Cell's
  private `application_formula` decides its local Record delta. The default UI
  is “you give” = negative local delta and “you receive” = positive local delta.
- Agreement is mutual and revision-bound: level 1 is a meaningful signed
  agreement; level 2 is signed settlement/commitment of the contract terms;
  later dual-sided fulfillment confirmation establishes that an occurrence
  happened. Every agreement policy requires the relevant parties to match
  levels before its next stage unlocks.
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
  dates/windows, locations, confirmation requirements, dependencies/order, and
  any term affecting what someone gives or receives. Private application
  formulas, private notes, and local display choices do not. Visibility changes
  cannot retroactively retract data already delivered.
- Stale term writes are rejected, even when apparently non-conflicting. The UI
  preserves the draft and shows the intervening signed diff with one-click
  reapply/edit; the user must review and submit a new revision. Messages and
  unrelated private state may continue concurrently.
- Every give/receive occurrence is independently claimable and confirmable by
  its giver and receiver. Local completion claims may be unsigned but remain
  actor-attributed, hash-chained facts; mutual evidence confirms occurrence.
  Bulk completion is only an explicit labor-saving action for one Person and
  emits individual evidence across the reviewed tree selection.
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
  settled tree. (Shipped: Graph controls → Node gravity section; graph mode
  swaps the center-y force for the per-node weight pull, Trail mode unpins
  the rows and simulates only tree nodes with x anchored to the topo layer.)
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
