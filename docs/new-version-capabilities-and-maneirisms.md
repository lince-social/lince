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
  including any Intelligence cascade), and `warnings` (non-fatal advisories) — show
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

## [ ] Intelligence — Lince's autonomic nervous system

Intelligence is the home of every continuing Lince behavior that a human or
Fiote does not perform one step at a time: sensing, schedules, derived values,
rules, workflows, recommendations, projections, attention, and bounded effects.
It is not a third actor and it does not own hidden authority. A human or Fiote
may create and manage its definitions, but the engine evaluates them under the
same visibility, permission, agreement, budget, and Ledger rules as every other
Action.
H: It can be useful to think that a good Lince is one with intelligence, rules, automations, to minimize the management of Life, to not need an llm behind recommendations and actions (if the user set it that way), possibly using them, but possibly only having preset rules made by agents and humans, and statistics and machine learning so Lince systems can slowly adjust and learn patterns from personal habits to interactions with other people. Leaving humans and agents to be creative and find the optimizations. I like operations research and the idea that the actors of Lince will want to use it to min-max (from simplex algorithms to more advanced stuff) their days, jobs and other things. I want Lince to have built in ways for actors (humans and agents) to access those optimizations, analysis inside Lince.

  1. Recurrence likelihood: Track how likely the local Person is to repeat an action,
     independently from trust in another Person. It may consider locally visible concepts,
     people, quantities, times, places, and preset context.
  2. Growth and decay formula: Repetition increases likelihood with diminishing returns, while
     time without repetition reduces it. Prior probability, growth, half-life, and thresholds
     are typed database policies.
  3. Trusted learning evidence: Learn only from actions authored by a Person or outcomes
     mutually confirmed by the involved People. Recommendations and automatically created
     drafts never train the model themselves.
  4. Suggestions and automatic drafts: Crossing the lower threshold creates one deduplicated
     recommendation with a visible explanation. Crossing a higher opt-in threshold may create a
     local unsigned draft, never send or agree to it.
  5. Karma-rule candidates: Repeated patterns may generate disabled automation candidates for
     explicit review. Candidates cannot impersonate another Person or automatically agree,
     confirm, or settle on their behalf.
  6. Explicit Karma actions: User-authored rules may perform simple local Transfer actions
     within approved recipients, visibility, budget, and evidence limits. Every execution
     remains idempotent, attributable, and visible.
  7. Persistent policy: Store thresholds, decay, scope, budgets, recipients, quiet times, and
     disabled patterns in typed database configuration. None of these decisions live only in
     frontend state.
  8. Recommendation safety proof: Verify recurrence growth, diminishing returns, time decay,
     deduplication, opt-out, privacy boundaries, and lack of self-training. Also prove that no
     recommendation or automation creates an unauthorized social commitment.

Legacy Karma is therefore not the whole feature. It is the deterministic
guarded-rule runtime inside Intelligence. Senses recognizes situations;
Imagination explores possible futures; recommendations propose; Attention
decides when a human choice is irreducible; workflows coordinate durable steps;
effects act. Together (maybe they need more features) they form one inspectable loop:

`Signals/Facts → Context → Senses/Rules/Projection → Candidates → Policy →`
`Decision or Effect → typed Action → Facts`

### Retired Karma vocabulary mapping

| Diary concept | Intelligence home | Refinement |
| --- | --- | --- |
| Karma | Program or deterministic rule node | The enabled behavior is a graph under policy, not a special table or opaque script. |
| Condition | Protein context + computation nodes | Inputs are named, typed, reusable, visibility-scoped, and traceable. |
| Operator | Gate | Boolean, comparison, threshold, and carry behavior are explicit instead of encoded by `=`/`=*`. |
| Consequence | Candidate outcome → policy → effect | Evaluation never smuggles authority into execution. |
| Delivery | Intelligence run and bounded agenda | Reactive evaluation, clock occurrences, cascades, and retries have identities and proofs. |
| Frequency | Schedule trigger | Timezone, calendar alignment, and missed-occurrence behavior are deliberate policy. |
| Command/query | Signal when reading; typed effect when acting | Inputs and side effects no longer hide inside arithmetic expressions. |
| Karma category | Program tag, owner, and execution scope | Selection and bulk pause/run are ordinary graph metadata. |
| Calendar/Graph/Karma Orchestra | Imagination + Orchestra | One model powers authoring, dependency views, run inspection, and branching futures. |
| Ask/Agent/Tinkerer | `suggest → draft → ask → act-within-budget` | Autonomy is an engine-enforced per-program policy, not an AI personality mode. |

### The Intelligence contract

- [ ] Intelligence reads Cell state through the same record/Ledger semantics
  that Protein exposes and changes state only through typed Actions. An
  automatic path is never a privileged write path.
- [x] Existing rules, signals, frequencies, match rules, and decisions are
  records; quantity is their activation knob and their changes remain visible
  in the Ledger.
- [ ] Make an **Intelligence program** the ergonomic unit a person manages: a
  named, versioned record whose linked graph declares triggers, Protein context,
  computations, policy, and outcomes. Rules, senses, schedules, recommendations,
  and workflows are program node kinds rather than separate automation silos.
- [ ] Every program declares an owner, purpose, data scope, authority ceiling,
  budgets, schedule/event triggers, failure policy, and enabled revision. No
  defaults may silently widen visibility or authority.
- [ ] Give each evaluation a durable `intelligence_run` identity with program
  revision, triggering Fact/timer, input cursor, clock/seed, node trace,
  candidates, policy decisions, Actions/effects, cost, and final status. “Why
  did this happen?” and “what will retry?” must be ordinary reads, not logs an
  operator has to find on disk.
- [ ] Derive every effect idempotency key from the program revision, triggering
  occurrence, and effect node. Retries may finish an intended action but never
  repeat it; changed definitions produce a new revision and new proof boundary.
- [ ] Separate evaluation from effects. A run first computes a stable proposal;
  policy then permits, stages, asks, or rejects it; effect workers execute
  durable intents with leases, retry/backoff, timeout, and dead-letter state.
  Partial external failure never rolls back or hides committed Ledger Facts.
- [ ] Define deterministic agenda semantics for simultaneous rules: dependency
  order, explicit priority only where necessary, stable tie-breaking, atomic
  Action boundaries, and a recorded explanation of conflicts. Programs may run
  to a bounded fixpoint; they may not depend on thread timing.

### Perception — signals, schedules, and context

- [x] `create-signal` represents command/http/sensor/query sources on a
  schedule. Samples land as Facts and cascade like any other change.
- [x] `create-frequency` supports day-of-week and catch-up behavior. Frequencies
  are reusable clocks, not record timestamp columns.
- [ ] Make every capture and integration source visible as an off-switchable
  signal record with freshness, last success/error, consent/visibility scope,
  sampling cost, and retention. Phone, scale, camera, microphone, filesystem,
  HTTP, query, model, and command inputs all obey this contract.
- [ ] AI enters only through a visible model Signal or through Fiote, never as
  a hidden reader or privileged writer.
- [ ] Replace ambiguous frequency catch-up with an explicit missed-occurrence
  policy: `skip`, `coalesce` (one run carrying the missed count), or bounded
  `replay`; expose start/end, timezone, weekdays, calendar interval, jitter, and
  whether calendar alignment happens before or after interval addition.
- [ ] Context is always a saved or inline Protein plus named derived values, not
  an ambient database capability. The run records the exact visible input set
  and freshness used, so a recommendation can explain missing or stale data.
- [ ] A **Sense** is a pure, named recognizer over current Facts, Signals,
  discovery data, and projected crossings. It emits evidence-backed candidates;
  it cannot write state or contact another Cell by itself.

### Deliberation — rules, derived values, and workflows

- [x] A deterministic rule (the subsystem historically called Karma) is a
  record with condition/gate/carry/debounce sidecar and consequences.
  Conditions support full math over live tokens: `@x`,
  `quantity()`, `sum[_pos/_neg](x, window)`, `freq()`, `signal()`, `value()`,
  `promise_state()`, `confidence()`, `projected()`, `hours_since_fact()`,
  `distance()`, and `demand()` (`route_eta` parses and errors cleanly pending
  OSM data).
- [x] Delivery is reactive: only rules reading changed records re-evaluate;
  cascades stop at 256 evaluations so a runaway loop survives for inspection.
  `debounce` temporarily holds a fired rule (currently in memory and reset on
  reload). A rule without consequences is a named derived value read through
  `value(@rules.x)`.
- [x] `create-rule`/`update-rule` reload the registry and return Proof-loop
  warnings in `outcome.warnings`; saving succeeds and the interface must show
  the warning.
- [ ] Persist debounce/cooldown and last-consumed occurrence so restarts cannot
  double-fire or accidentally re-arm one-shot behavior.
- [ ] Make conditions and derived values reusable graph nodes. Composition uses
  typed references and explicit Boolean/math gates, allowing arbitrary chains
  without returning to opaque `rq1`/`kd2` token strings.
- [ ] Add durable workflow nodes for multi-step orchestration: state machine,
  sequence/parallel, wait-until, branch, approval, retry, compensation, and
  child-program invocation. Workflows coordinate typed Actions; they do not
  create a second implementation of domain behavior.
- [ ] Give a workflow an explicit concurrency policy (`queue`, `drop`,
  `coalesce`, or bounded parallel), correlation key, timeout, and cancellation
  semantics. Long-running work resumes from its durable node state after boot.
- [ ] Add Proof analysis for dependency cycles, contradictory writers,
  unreachable nodes, unsafe external effects, authority escalation, dead ends,
  non-convergence, and likely divergence. The runtime cascade cap remains the
  final guard, not the design tool.

### Recommendations and learning

- [x] A match rule is a record (`create-match-rule { watch_concept,
  max_proximity, min_confidence, auto }`) and activates/deactivates like any
  rule. `max_proximity` is a hard ceiling; matching never auto-expands.
- [x] Each heartbeat, `senses_pass` joins local OPEN promises with the discovery
  cache using sign-opposite deltas, Lingua-aligned concepts, overlapping windows,
  and a confidence floor, then places ranked drafts in the Decision Queue.
- [x] Deterministic `confidence(@p)` is the counterparty's Laplace-smoothed
  kept-promise ratio; `demand(@concept)` is the current hour's share of trailing
  30-day activity for that concept. Neither is a global reputation score.
- [ ] Refine counterparty confidence with window tightness and dispute evidence
  while keeping the raw, explainable ingredients visible.
- [ ] Define local `recurrence_likelihood` separately from counterparty
  confidence, keyed only by locally visible action/preset, concepts, people,
  quantity/time/place, and deliberately selected context.
- [ ] Use deterministic diminishing-return evidence with configurable prior,
  growth, time-decay/half-life, suggestion threshold, and a higher,
  disabled-by-default auto-draft threshold.
- [ ] Learn only from human-authored or mutually confirmed outcome Facts.
  Recommendations, dismissed candidates, generated drafts, Fiote output, and
  automated Actions never become positive training evidence merely because the
  system produced them.
- [ ] Emit one deduplicated recommendation with evidence, confidence, expected
  benefit/cost, uncertainty, alternatives, authority required, and a preview of
  affected records. The person can accept once, edit, snooze, mute the pattern,
  or turn it into a disabled program candidate.
- [ ] Recommendation policies support balancing activities across a week,
  detecting useful habits/purchases, matching Needs and Contributions, and
  proposing workflow simplifications without inventing a universal objective
  for “optimization.” The objective and protected constraints belong to the
  person.
- [ ] A model or Fiote may author the same inert candidate format, but cannot
  bypass evidence display, visibility, review, budgets, or the program's
  authority ceiling. “Suggested by a model” is provenance, not permission.
- [ ] Prove recurrence saturation/decay, deduplication, negative feedback,
  opt-out, no self-training, no private leak, and no automatic social
  commitment.

### Attention and whispers — the interruption contract

- [x] `source: decision, live: true` is the Decision Queue and `decide` is the
  answer. Deterministic feeders cover broken promises (`expiry`), rule `ask`
  consequences (`ask`), Senses matches (`draft`), and projected crossings
  (`crossing`, a one-week horizon, deduped per record).
- [x] `decide { decision, answer }` closes a decision through the Ledger. When
  the chosen option carries an Action, deciding executes it for one-tap flows
  such as “yes → set quantity.”
- [x] Decisions with `expires_at` auto-close as `expired`; each sweep deduplicates
  by `(subject, kind)`, so one unresolved situation asks once.
- [x] The daily notification budget (`configuration.attention_budget_per_day`,
  default 12) is hard. Excess notification effects finish as `parked:digest`;
  they are deferred, not lost.
- [ ] A **decision** is durable work requiring a choice; a **whisper** is its
  calm, context-aware delivery. Whispers never become a second queue and never
  execute Actions: they link to a decision, recommendation, run, or changed
  fact and disappear without losing the underlying item.
- [ ] Rank attention by explicit user priority, urgency/window, confidence,
  reversibility, cost of delay, and interruption cost. Low-value items collect
  into summaries; urgency does not manufacture authority.
- [ ] Route through device records to inbox, digest, desktop toast, mobile push,
  sound, or text; support per-program/per-source channel controls, quiet hours,
  location/context eligibility, accessible presentation, and “show why now.”
- [ ] Feedback is a first-class result (`accepted`, `edited`, `dismissed`,
  `snoozed`, `muted`, `wrong-context`) used to tune local delivery and pattern
  policy without rewriting historical evidence.

### Effects, authority, and social safety

- [x] Rule consequences include `set_quantity`, `add_quantity`, `emit_promise`,
  `run_command`, `run_query`, `run_action`, `set_visibility`,
  `activate`/`deactivate` (including another rule), `ask`, and budgeted `notify`.
  Effects run outside evaluation from a durable queue and append zero-delta
  provenance Facts.
- [x] Transfer automation fails closed against the old activation path; current
  manual revision, agreement, occurrence, confirmation, and settlement gates
  remain authoritative.
- [ ] Classify every outcome before it can run: pure derivation, reversible
  local Action, external effect, social draft, or social commitment. Policy may
  grant lower classes independently; crossing a class always requires explicit
  authority.
- [ ] Simple Transfer Actions may be automated only inside approved visibility,
  recipient, value, rate, idempotency, agreement, occurrence, and evidence
  gates. No program, recommender, model, or Fiote may agree, confirm, claim,
  settle, publish to a new audience, or speak as another Person.
- [ ] UI/device effects use durable typed action intents with target, nonce,
  claim lease, and completed/failed evidence. A rule can request that a bound
  call controller open or close a room; it cannot send a vague command or drive
  an arbitrary sand directly.
- [ ] Commands and HTTP effects declare executable/host, arguments/schema,
  secrets, working scope, timeout, network/filesystem capability, and output
  capture. Interfaces expose live execution and failure like a workflow run;
  simulation never performs the real external effect.
- [ ] Persist scopes, budgets, recipients, quiet time, thresholds, decay,
  forbidden Action kinds, and pattern overrides as typed policy. Enforcement is
  in the engine, never only in Orchestra or an agent prompt.

### Imagination, simulation, and proof before action

- [x] `Engine::project(now, until)` folds promises and rules on a virtual clock
  with Signals frozen. `Engine::snapshot(now)` creates mutable input for
  toggle/clear/re-fold/diff, so branching futures already exist as an internal
  engine call.
- [ ] Expose project/snapshot through a typed transport verb so a sand can scrub
  and branch a future: change starting quantities, toggle a program, clear a
  promise, alter time, re-fold, and compare timelines without touching the real
  Ledger.
- [ ] Simulation operates on an isolated snapshot with a virtual clock and
  mocked signals/effects. Its seed, inputs, event script, stopping/bookmark
  conditions, and engine version make every run replayable; “apply” means
  separately reviewing ordinary typed Actions, never committing a simulated
  state wholesale.
- [ ] Let people define invariants and questions: can this state be reached,
  do these programs conflict, will a quantity cross a boundary, does the graph
  settle, can an effect repeat, and what changes if this promise disappears?
  Proof results link to the exact program revisions and counterexample trace.
- [ ] Add deterministic generated scenarios and fault injection for time jumps,
  restart, delayed/failed effects, duplicate Facts, reordered sync, and missing
  Signals. This is both product Imagination and a test architecture for the
  autonomous runtime.
- [ ] Build calendar/time-budget and graph/state-space projections from the same
  simulator: time on one axis, quantities/ranges on another, rule-active regions,
  consequence arrows, dependency/supply-chain paths, uncertainty bands, and
  real-vs-projected values.

### Orchestra — one control room, not a programming language exam

- [ ] Build Intelligence Orchestra on the shared canvas: overview all programs,
  filter/group by type, owner, scope, state, or tag, and zoom from the whole
  dependency graph into one node's configuration and run history.
- [ ] The primary authoring path is names, concepts, selectors, typed ports,
  forms, and connections. Raw condition syntax remains an inspectable expert
  escape hatch, never required for ordinary habits, schedules, recommendations,
  or workflows.
- [ ] Render conditions/senses as inputs inside a program boundary and outcomes
  outside it, with directional connections, freely rearrangeable circle/line/
  graph layouts, reusable subgraphs, and supply-chain links across permitted
  Organs. Preserve licenses/notices for any vendored graph/physics library.
- [ ] Dry-run one node or whole program against current or simulated input,
  animate the evaluated path, show each substituted value/gate/carry, preview
  writes and external effects, and allow breakpoints before an effect.
- [ ] Surface loop/conflict/authority Proof on edit and save; compare revisions,
  publish/rollback by selecting the active revision, pause immediately, inspect
  queued/running/dead effects, retry safely, and compensate reversible Actions.
- [ ] Provide installable templates as ordinary disabled program graphs: habit,
  recurring task, inventory threshold, birthday reminder, recurring Transfer
  draft, call intent, monthly recap, and command flow. Installation grants no
  data scope or authority until the person reviews them.


### Intelligence acceptance workflows

- [ ] Todo/knowledge base: a habit re-arms daily and completing it posts a
  causal Fact.
- [ ] Recurring tasks: a monthly schedule fires exactly once under normal time
  and obeys its chosen catch-up policy after downtime.
- [ ] n8n-style command flow: a signal → rule/workflow → leased effect graph is
  built visually, dry-run, executed, inspected, and safely retried.
- [ ] CRM/people: a birthday whisper arrives at the chosen moment and an
  interaction report is one aggregate Protein.
- [ ] Personal finance: “rent leaves you short on the 5th unless X settles” is
  explained from the projection and opens the relevant evidence.
- [ ] Calendar/time budgeting: the projected week renders and moving a promise
  recomputes it without storing a duplicate calendar truth.
- [ ] Health/IoT: a scale posts weight Facts, a streak program reacts, and the
  source off-switch stops new sampling and effects.
- [ ] Transfer recurrence: repeated, confirmed behavior produces one explained
  recommendation, then an editable disabled rule or local draft—never an
  agreement or settlement.
- [ ] Neighborhood matching: a scoped match rule and visibility grant produce a
  draft in Attention after polling, without widening proximity.
- [ ] Chat/calls: “when Transfer Y reaches agreed, ask controller X to open the
  room” uses a typed, single-claim action intent.
- [ ] Games/THE Game: records provide state and an Intelligence program provides
  the inspectable rulebook without a special game automation core.
- [ ] Garden/farm and inventory/production: watering and threshold programs
  derive work/Needs, projections distinguish actual/available/planned, and
  settlement remains the only quantity truth.
- [ ] Monthly recap: a program selects the month's Facts, drafts the recap, and
  links its evidence without training on its own output.

### Fiote manages Intelligence; it does not become the engine
For now dont plan anything around Fiote, we don't know how its going to exactly access what features and how, pretend it doesnt exist.

- [ ] Fiote follows the shared autonomy ladder
  `observe → suggest → draft → ask → act-within-budget`. It reads only
  Protein-visible data granted to `@fiote`,
  writes only typed Actions, and receives no ambient database, command, secret,
  or network access.
- [ ] Fiote may explain runs, diagnose a failed workflow, construct or edit an
  inactive program, simulate it, and present the diff. Activation and authority
  escalation remain explicit human decisions unless an existing policy already
  grants that exact bounded change.
- [ ] Fiote Actions carry `cause=fiote`, the user's signature plus a visible
  delegation marker, program/conversation provenance, and engine-enforced daily,
  value, recipient, and Action-kind budgets. Compensation remains available for
  reversible changes.

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
- [x] Events are scoped to a grouped sand's innermost group; ungrouped
  sources broadcast board-wide; cross-session mirroring rides lane rooms,
  never persisted.
- [x] (2026-07-19) Kanban, Relations, and Communication no longer ship as a
  GROUP bundled with their own Record sand — every board already has exactly
  one pinned Record (`shell-record`, bottom-right corner, icon by default),
  so bundling a second one per sand was redundant and, worse, its group
  scoping meant a grouped kanban's `recordClicked` never reached the pinned
  one. These three now ship as plain single `.html` packages (ungrouped),
  so their board-wide `recordClicked`/`recordCreate` reaches the pinned
  Record directly. The generic group-archive machinery (`.lince` workspace
  archives, `is_group` catalog entries, drag-drop import) stays for
  user-authored/imported groups — only the three OFFICIAL auto-grouped
  catalog entries were removed. Kanban's default add-to-board size also grew
  (`initial_width`/`initial_height` 6×6, up from 7×5 pre-clamp) since it's no
  longer sharing space with a bundled Record card.
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
- [ ] **Finance / statistics dashboard** — `source: fact` aggregates (`sum`
  by `cause_kind`/`day`, `concept_in: "money"`), drill-down via `record_eq`.
- [ ] **Pantry / inventory dashboard** — `availability` + `projection`
  includes render "8 now, 5 available, 2 by Friday" with no math in the
  sand; crossing decisions already surface "you run out Thursday."
- [ ] **Organ contacts manager** — list contacts with trust/proximity/sync
  policy, introduce via URL, block button, quarantine viewer.
- [ ] **Todo polish**: create-task UI/Action, richer history backed by
  compensation/facts, live-update proof beyond the stubbed bridge, plus the
  old table sand's deferred keyboard-grid navigation, helix mode, and
  concept/unit inline editors.
- [ ] Every ported data-plane sand needs a driven chromium selftest
  (snapshot + Action round-trip + live update); two stale scripts
  (`board-selftest.sh`, `table-sand-selftest.sh`) still reference deleted
  crates/files and need rewriting on the current architecture.

## [ ] Future Instincts and product surfaces

- [ ] OSM place data: a local offline extract, local geocoding, `route_eta`,
  polygon `within`, a `route(a,b)` include (`distance`/`near` already
  live).
- [ ] Currency conversion over a Lingua `@money` dimension.
- [ ] Storage-engine independence (AniccaDB): swapping out `store` must not
  change one character of the Protein/Action contract.
- [ ] New product surfaces beyond the Transfer workstream: route/ride
  planning, calls, calendar/time budgeting, finance projections, social feed.
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

- [ ] Relation trail mode, re-proven on a live server (proven on the old
  core pre-purge; the driven stub selftest already covers the behavior,
  this is the real-socket run)
- Transfer workflows (DONATION, SALE, assignment/group coordination,
  dependency chains, and later RIDE/DELIVERY) are centralized in Transfer
  Phase T4 so their implementation and proof cannot drift apart.
- [ ] Real-time collab docs (two Cells, one body, cursors on lanes, Ledger
  shows only text_edit annotations)
- [ ] Social network (federated feed from two organs, visibility respected
  — zero new core)
- [ ] World statistics (need-mountains aggregate from N organs, nothing
  hidden leaks)
- [ ] AI conversation sand (zero core changes)
- [ ] Education (imported Relation trail shows per-student progression)

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
- [x] Time is deliberately NOT a record column: automated timing = Intelligence
  schedules/Frequencies; declarative time (what strangers match on) lives on promise
  windows.
- [x] Board chrome is frontend state; sand data is Protein/Actions.

## The theory (from the retired blueprint)

**The refounding, three sentences:**

> **Everything is a Record. Every change is a Fact. Every intended change is a Promise.**

**The pillar map:** Record (state) · Memory/Ledger (facts) · Lingua (shared
concepts; Instinct tier = concepts with engine functions) · Intelligence
(Signals → Context → Senses/Rules/Imagination → Recommendation/Attention →
Policy → Effects) · Transfer (promise bundles under agreement + visibility) ·
Trust (verifiable signed deltas) · Protein (declarative reads) & Actions (typed
writes) · Fiote (optional agent managing the same Intelligence knobs).

**The placement rule (the Window):** the core owns what must be computed,
verified, or agreed across Organs; interfaces own what is seen; embed
honestly what the world already built well. Altitude ladder: Primitive →
Pillar engine → Instinct → Lingua concept/unit → fds sidecar → Sand →
Embedded foreign app.

**Non-negotiables:** quantity stays central (negative = Need, positive =
Contribution, zero = peace); quantity-as-activation on everything; full math
in Intelligence rule conditions (tokens substituted, then evaluated); compatibility
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
