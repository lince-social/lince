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

## [ ] Karma — Lince's autonomic nervous system

Karma is the third primary pillar of Lince. **Records model the world;
Transfer coordinates intended change between people; Karma continuously
turns evidence into understanding, proposals, decisions, and permitted action.**
The Ledger, Lingua, Protein, Trust, and Actions are the common substrate through
which the three pillars remain one system.

**Naming decision.** Karma is the permanent public and code name for this
pillar. “Intelligence” was a temporary planning name and is not retained as an
alias, namespace, capability prefix, Protein source, database prefix, or sand.
The earlier condition → consequence feature becomes a legacy importer into the
new `karma` typed graph; it is not a second engine. “Orchestra” is likewise not
a product name: orchestration is one lens inside the Karma sand.

The three-pillar boundary should remain. Perception, Learning, Imagination,
Attention, Optimization, and Effects are facets of Karma because they
must share one causality, policy, and replay model. Memory/Ledger, Lingua, Trust/
Governance, and the Protein/Action control plane are equally fundamental
cross-cutting infrastructure, but promoting each to a competing behavioral
engine would recreate silos. If a future fourth pillar is ever needed, the test
is whether it owns an independent kind of truth—not whether it deserves a
screen or has many features.

It is Lince's always-on autonomic engine: sensing, schedules, derived values,
rules, workflows, pattern learning, forecasts, optimization, recommendations,
attention, simulation, and bounded effects. “Always on” means that a supervised
runtime starts with the Cell and keeps draining durable work. It does **not**
mean always interrupting, always connected, or always authorized to act. A
person can pause one program, one capability family, all external effects, or
the entire engine without losing evidence or queued work.

Karma is deliberately useful without an LLM. People and software agents
can install preset programs, visually author exact rules, use transparent
statistics and deterministic machine-learning algorithms, and run operations
research over their own goals. A model may be attached as an explicit Signal or
author an inert candidate, but model access is optional and grants no special
authority. The engine is the typed deterministic machinery, not a personality.

### Architectural corrections and boundaries

- **Karma's condition → consequence pair is too small as the destination
  abstraction.** It conflates recognition, inference, authority, and execution.
  Karma remains useful shipped machinery, but the destination is a versioned
  typed program graph with separate pure computation, candidate, policy, and
  effect stages.
- **A likelihood is not permission.** “Ana will probably buy apples this week,”
  “the estimate is well supported,” “buying is beneficial,” and “Lince may
  create or send a Transfer” are four different claims. Pattern probability,
  epistemic confidence, objective value, and authority are stored and evaluated
  separately.
- **A threshold is a routing policy, not truth.** Crossing it may surface,
  draft, ask, or act only according to an explicit policy with hysteresis,
  budgets, and authority. It never silently converts correlation into a fact.
- **Automatic promotion is allowed, but never magical.** A person may grant a
  narrow policy that turns a learned pattern into an active rule or draft. The
  generated revision must come from a reviewable template, pass Proof, run in
  shadow first if required, stay editable, and remain inside the grant that
  authorized promotion.
- **Rules are never set in stone.** A person or authorized agent can edit,
  pause, supersede, fork, or retire any program. Published revisions and past
  runs are immutable evidence; immutability protects history, not the current
  behavior. Editing creates a new revision and atomically changes which
  revision is active.
- **Optimization has no secret universal objective.** Scheduling, simplex/MILP,
  constraint solving, matching, routing, and other planners operate on explicit
  person-owned objectives and protected constraints. They return alternatives,
  sensitivity, infeasibility, and uncertainty—not a mysterious “best life.”
- **The deterministic boundary must be honest.** The external world is not
  reproducible: a sensor can drift, a person can change their mind, and an HTTP
  server can answer differently. Lince makes the *decision kernel*
  deterministic by first capturing those boundary results as ordered evidence.
  Replaying the same captured inputs must reproduce the same internal outputs;
  replay never secretly calls the outside world again.
- **Autonomy is actor-neutral and non-transitive.** Humans, local tools, and
  software agents use the same typed control Actions. A program has no identity
  or authority of its own; it acts as a named principal through a revocable
  delegation. A program may not enlarge its own delegation or pass it onward.

The complete loop is:

`Observation → Fact/evidence → context/features → recognition/model/rule →`
`forecast/optimizer → candidate → policy/authority → decision or intent →`
`typed Action/effect → receipt/Fact`

Imagination branches from the evidence/context boundary and runs the same
kernel against a snapshot. Attention is a delivery layer over durable
candidates and decisions, not a second truth store.

### Authoritative implementation sequence

**Phase labels are stable identities, not an order.** The build order is the
list below; `K6` means what `K6` has always meant regardless of when it runs.
An implementation must satisfy each phase's exit gate before exposing the next
phase *in build order* in the Karma sand. Later sections specify the destination
interfaces and behaviors. The UI/DSL examples appear early to freeze the
contract; they do not authorize implementing the frontend before the kernel,
persistence, and authority gates exist.

#### Build order — Economy first

Reordered 2026-07-25. The original order finished the entire backend safety
boundary before any product existed, which meant perception, learning, social
autonomy, workflows, and simulation testing all had to land before a single
person could use anything. Economy needs none of them: the doc's own reason for
choosing Economy as the integration proof is that it exercises exact values,
Records and Facts, units, Frequencies, occurrences, corrections, authority, and
explanations **without requiring an external effect**. So Economy moves up, and
everything perception-and-inference-shaped moves after it.

1. **K0 – K4** — done. Vocabulary, pure kernel, durable model, tickless
   scheduling, evaluator, inert candidates, candidate review.
2. **K5.1 – K5.2** — done. Signed delegation grants, budgets, durable inert
   intents.
3. **K11/E0.0 – E0.4 — the engine loop, closed, proven, and foldable.** E0.0
   makes the Ledger carry exact deltas; E0.1 opens the read side so a rule can
   pull a Record's quantity, its unit, and its other numeric values into
   evaluation; E0.2 gives arithmetic the multiplication and division a real rule
   needs; E0.3 lets an authorized intent change a Record's quantity or
   unit-valued field and compensate it; E0.4 runs that same loop on a virtual
   clock so a year or five years of it can be seen without waiting. Together
   these are the read → compute → write → fold loop the whole pillar exists to
   serve, and they land before any product is built on them.
4. **K11/E0 – E2** — the Economy vertical on that loop. E0 adds gain/loss
   events; E1 adds recurring plans, occurrences, and projection over the
   existing Frequencies; E2 ships the sand. This reaches the original K11 exit
   gate.
5. **K5.3 remainder** — the rest of execution: everything outside the
   reversible-local-data family, whose worker, lease, receipt, and emergency-stop
   machinery E0.3 already built and proved.
6. **K11/E3** — Fiote capture ergonomics for Economy.
7. **K12** — the Karma sand and its Flow Plane.
8. **K6** — Signals and integration adapters.
9. **K7** — learning, recommendations, and Attention.
10. **K8** — scoped Trust and Transfer autonomy.
11. **K9** — durable workflows, projections, and optimization.
12. **K10** — replay, shadowing, and Deterministic Simulation Testing.

**Why the engine loop comes before the product.** An earlier draft of this order
put execution after the Economy sand, on the reasoning that E1's `apply` route
is already gated by the K5.1/K5.2 grant boundary and that a person could resolve
due occurrences by hand until a worker existed. That is true but it is not the
point: it would have shipped a product on top of an engine whose central claim —
math rules that change Records over time — had never once been executed
end to end. E0.0–E0.3 close that loop first. Economy then becomes what it was
always meant to be, the *integration proof* of a working engine rather than the
thing that discovers the engine is not finished.

The restriction that makes this safe is capability family, not phase order.
E0.3 executes `LocalReversibleData` and nothing else: local, auditable on the
Record's own Fact chain, and reversible by an opposite exact delta. Every
external, social, or irreversible effect stays behind the same closed door it is
behind today.

**The cost of this reorder, stated plainly.** K10 moves to the end, so Economy
ships before deterministic simulation testing exists — and K10's exit gate is
what proves the determinism every earlier phase promised. Replay capsules,
frozen epochs, injected clocks, and the ordering proofs are all built and
tested, so the property is designed in and covered per-phase; what is missing
until K10 is the adversarial machinery that tries to *break* it. Accepted
deliberately: a product that exists is worth more than a proof about a product
that does not.

#### K0 — freeze vocabulary, invariants, and failure codes

Start by turning this document's types, states, Actions, Protein discriminators,
capability names, and error classes into Rust enums/newtypes with canonical
Serde behavior. This phase changes no runtime behavior. It prevents each later
module from inventing its own spelling for probability, time, occurrence,
candidate, authority denial, or uncertain effect.

**Exit gate:** golden serialization tests cover every public type; invalid
probability/decimal/time/unit/state/capability values fail at the boundary; and
the same fixture canonicalizes to the same bytes/hash on every supported
platform.

The K0 Rust boundary lives under `nucleus::karma`; later crates import
these definitions instead of restating strings. Keep focused modules for
`canonical`, `exact`, `time`, `reference`, `state`, `capability`, and `failure`.
This directory remains free of SQL, async runtimes, wall clocks, random uid
generation, platform locale, and unordered public collections. K0 freezes the
following wire decisions before any durable table is created:

- `FixedDecimal<S>` stores an `i128` mantissa at a compile-time scale, rejects
  precision loss and overflow, and serializes as a canonical decimal string
  with exactly `S` fractional digits. Negative zero is impossible. Arithmetic
  is checked; rounding is never implicit.
- `Probability` and `Confidence` are distinct parts-per-billion newtypes over
  `0..=1_000_000_000`. Their canonical JSON is a quoted decimal with exactly
  nine fractional digits. The later DSL may accept `0.72p` and `0.65c`, but
  parsing normalizes them before they enter an AST or hash.
- `DurationMs` is a signed integer millisecond count. `TimestampMs` stores a
  signed UTC millisecond and serializes only as normalized UTC RFC3339 with
  exactly three fractional digits and `Z`; an offset, omitted fraction, or
  finer fraction is not canonical input. Civil/calendar time is a separate K1
  type and never silently converts to elapsed duration.
- Enum values and Action tags use kebab-case; struct fields use snake_case.
  Public maps and sets use `BTreeMap`/`BTreeSet` or explicitly sort before they
  cross the boundary. Typed references preserve both kind and resolved uid;
  an optional display slug is metadata and never changes identity.
- Canonical JSON sorts every object key lexicographically, preserves array
  order, emits no whitespace, and rejects floating-point JSON numbers. Hashes
  are SHA-256 over a versioned, domain-separated prefix plus those bytes and
  are rendered `sha256:` followed by lowercase hexadecimal. Callers use a
  purpose-specific domain such as `karma.program-revision.v1`; hashes
  from different semantic object families cannot alias by accident.
- Boundary failures carry a stable machine `FailureCode`, a safe message, and
  an exact field path. Retry/operational classification remains typed; callers
  must not match human-readable text. New codes may be added, but an existing
  code cannot be repurposed.

The initial golden fixtures cover all K0 public enums and atoms as one object,
including a deliberately reverse-inserted map. Subsequent phases must extend
that fixture in the same change that adds a public type; an untested wire type
is an incomplete phase change.

**Implemented K0 foundation:** `nucleus::karma` now owns these focused
modules and their public re-exports. Its integration fixture covers every K0
enum variant plus exact values, typed references, capabilities, failure/path,
timestamp/duration, canonical hash, and reverse-inserted collections. Boundary
rejection tests and the fixed golden hash form the phase gate. This completion
does not freeze the vocabulary forever: when K1 or a later phase introduces a
new public wire type or enum value, that same change must extend and
deliberately re-freeze the K0 fixture.

#### K1 — pure Karma kernel and compiler

Implement the I/O-free core in `nucleus`: exact values, typed references,
Program AST, DSL parser/formatter, dependency graph, schedule math, gate/state
transitions, objective/model interfaces, Proof, replay types, and the Economy
signed-gain/loss/projection primitives specified in K11. Pure code
receives a frozen context and returns a trace plus candidates; it cannot access
SQLite, Tokio time, processes, network, devices, secrets, or global randomness.

**Exit gate:** DSL ↔ AST ↔ formatted DSL round-trips; type/unit/taint checking,
cycle detection, schedule/rephase math, deterministic ordering, and bounded
evaluation pass pure unit/property tests without opening a Store.

The first K1 slice extends `nucleus::karma` with `value`, `ast`, and
`proof`; it does not reuse the legacy `Expr`/`RuleDef` representation as the
new kernel. Those older types use `f64`, numeric Boolean truthiness, second
durations, implicit reads, and cycle-tolerant ordering, so they remain only an
eventual import source. The new slice uses these contracts:

- `ProgramAst` is the canonical, version-tagged revision payload. It contains
  purpose, parameters, stable node/output maps, and declared program outputs.
  Owner, live quantity, active revision, grants, and runtime state belong to
  the mutable Program handle or run epoch and therefore do not contaminate the
  immutable semantic hash.
- Node and parameter collections are keyed by `LocalId` in `BTreeMap`s. A node
  has explicit input bindings, output `PortContract`s, and a closed
  `NodeOperation` enum. Expressions can reference only names in that node's
  input-binding map; they cannot perform ambient Record, Protein, clock,
  randomness, secret, or network reads.
- The initial operation set is deliberately executable as pure semantics:
  trigger declarations, typed inputs, exact derivations, and an explicit delay
  state boundary. Later operation variants extend this same enum for gates,
  models, candidates, workflows, and intents; they do not introduce a generic
  JSON “operation config” escape hatch.
- `ValueType` and `LiteralValue` are closed, recursively typed enums. Decimal,
  quantity, and money literals retain exact scale; unit and currency identity
  are explicit. `datum<T>`, `estimate<T>`, collections, and references remain
  distinct types. Expression checking initially permits only operations whose
  result can be proven exactly; unsupported coercion is a Proof error rather
  than a runtime guess.
- A `PortContract` carries value type, sensitivity/taint class, and optional
  freshness. Source and destination value types must match exactly in this
  slice. Pure derivations cannot declare an output less sensitive than any
  consumed input. Declassification will be its own capability-checked node,
  not a flag on a wire.
- `Proof` is a deterministic value containing the revision hash, accepted/
  rejected status, stable topological order, and sorted typed issues with JSON
  pointer paths and related node ids. Missing nodes/ports, undeclared expression
  inputs, output/type mismatches, invalid references/contracts, and
  combinational cycles reject the revision. A cycle is legal only when cutting
  incoming update edges at an explicit delay/state boundary makes the graph
  acyclic; the delay contract records initialization, reset, late-event,
  migration, persistence, and simulation-clone behavior.

Text parsing is intentionally a later K1 compiler slice, after the graph,
schedule, and evaluator semantics it must project. First proving the JSON AST
prevents a convenient parser from becoming the accidental semantic model; the
DSL parser and formatter must later round-trip through these same types.

**Implemented K1 graph foundation:** `value`, `ast`, and `proof` now implement
the exact types, initial closed operation set, stable maps, state-boundary
contract, canonical revision hash, deterministic type/taint/reference checks,
and topological Proof described above. Golden fixtures cover every new public
enum variant. Tests prove ordinary cycles fail while feedback through an
explicit delay succeeds. This is the graph foundation, not the full K1 exit
gate: textual parsing/formatting, broader node families, replay capsules, and
Economy primitives remain later slices. Schedule/calendar math and the initial
evaluator are implemented in the following slices.

The first evaluator slice uses that graph without introducing ambient state.
`FrozenEvaluationContext` supplies exact boundary values by input/trigger node,
typed parameter overrides, and epoch-start delay state. `EvaluationLimits`
supplies deterministic fuel; one node visit and each visited expression consume
defined work units, so host speed and thread scheduling cannot change whether a
run exhausts its budget. The result contains the revision hash, declared
program outputs, stable node trace, fuel used, and proposed delay-state updates.

Delay evaluation is two-phase. During graph order every delay outputs its
epoch-start value (or declared initial value). After all combinational nodes
finish, the evaluator resolves each delay's update binding and stages the value
for the next epoch; it does not mutate the current context. This gives feedback
graphs synchronous read-old/write-next semantics independent of map or node
order. The evaluator never performs an Action, Store read, clock read, random
draw, candidate route, or external effect. A rejected Proof, missing frozen
input, runtime type mismatch, exact arithmetic overflow/divide-by-zero, invalid
state, or fuel exhaustion returns a typed deterministic error with node/path.

**Implemented K1 evaluator foundation:** `evaluate` now executes the initial
pure operation set from a frozen context with read-old/write-next delay state,
checked exact arithmetic, semantic reference identity, lazy typed branches,
stable trace order, and fuel/depth limits. Golden tests fix both successful
result bytes and evaluator vocabulary; failure tests cover missing/wrong inputs,
overflow, divide-by-zero, fuel/depth exhaustion, and successive feedback
epochs. This still produces no candidates, intents, Actions, Facts, or Store
writes; those require later node families and the K2/K5 authority boundaries.

**Implemented K1 calendar foundation:** `calendar` now owns canonical
`CivilDateTime`/`CivilTime`, validated timezone and tzdb revision atoms,
daily/weekly/monthly rules, invalid-month/gap/fold policy, typed boundary and
discontinuity results, and the pure `TimeZoneProvider` boundary. Resolution
checks provider identity and validates a previous boundary against the cadence
and provider before advancing. Fake-provider tests freeze gap skip/shift/pause,
fold-both order, weekly anchor arithmetic, monthly skip/clamp/pause, malformed
provider rejection, and the complete calendar wire hash. No host timezone,
clock, tzdb package, timer, or I/O is consulted.

**Implemented K1.4 Program DSL:** `nucleus::karma::dsl` now formats and parses
every current `ProgramAst`, type, literal, source, expression, state contract,
capability, and resolved reference. Canonical text sorts maps/sets and lexical
capability names, uses JSON string escaping, removes comments/whitespace, and
round-trips to the identical AST and Proof. Stable typed errors include exact
byte/line/column; source, token, string, and nesting limits are enforced before
storage. Comprehensive and exact-text tests cover all current variants,
reordering/comments/Unicode strings, duplicates, missing/unknown/trailing
syntax, noncanonical exact atoms, and every resource limit. DSL error vocabulary
is included in the K1 golden hash.

**Implemented K1.5 Frequency compiler/DSL:** `nucleus::karma::frequency` now
owns the separately versioned Frequency AST, duration/positive-integer
parameter definitions and bindings, effective-parameter compilation, typed
compile failures, and elapsed/calendar compiled union. `karma::dsl` formats and
parses its strict text. Compilation proves parameter domains keep interval and
timer fields valid, rejects unknown/type/range overrides, hashes revision and
effective values separately, and returns the existing concrete schedule types.
All calendar schedules now carry rephase policy. Exact-text, comments/order,
all-calendar-rule, override, cross-cadence, parameter-domain, compilation, and
complete wire-golden tests cover the boundary.

**Implemented K1.6 gate:** pure threshold/hysteresis, debounce, cooldown,
rate-limit, and candidate-route node families now have canonical AST/DSL forms,
Proof rules, frozen logical-time evaluation, separately typed staged control
state, exact boundary behavior, late-event failures, and sequence tests. The
evaluator still has no clock, timer, authority, or effect access. K1.7 pure
replay capsules and K1.8 Economy exact primitives finish the remaining K1
slices before K2 persistence.

#### K2 — durable model, Actions, and Protein

Add new migrations and schema-owned Rust sidecars for Program/revision,
parameter, Frequency/schedule, occurrence, run/trace, evidence/model checkpoint,
candidate/decision, grant/trust scope, workflow, intent/attempt/receipt, and
engine mode. Do not alter a past migration. Every mutable handle uses an
expected revision; immutable revisions/checkpoints use content hashes; every
semantic mutation appends or links the corresponding Fact in the same
transaction.

Implement the concrete Actions and `source:"karma"` Protein union next,
including capability booleans and stable blocking reasons. At this phase a user
can author, validate, store, diff, inspect, and activate definitions, but active
Programs still do not automatically run.

**Exit gate:** create/revise/fork/activate/pause/parameter/grant round-trips are
atomic, stale/replayed requests are deterministic, live invalidation works, and
Protein can reconstruct every stored object and causal link without reading
filesystem logs.

#### K3 — tickless shrink-to-fit deadlines and the occurrence sequencer

Replace the single coarse heartbeat as the owner of timed causality with the
deadline fabric specified below. One sequencer persists and orders Fact, timer,
Signal, sync, workflow, retry, and manual occurrences. It freezes an evaluation
epoch and enforces reaction-before-learning. This phase runs a minimal no-effect
Program to prove timing/order before adding broad Actions.

**Exit gate:** a Cell with `3ms`, `5h`, and monthly Frequencies arms each from
its own next deadline: the `3ms` path never scans/re-arms the `5h` or monthly
definitions, and the sparse deadlines still fire while the dense stream is
active. Adding/removing dense demand creates/retires only its runtime lane;
restart/catch-up, same-millisecond cursor order, generation invalidation, and
schedule batching replay exactly under DST.

#### K4 — evaluator, rules, derived values, and meta-control

Compile active revisions into an indexed dependency registry, evaluate impacted
Programs under the frozen epoch, persist traces/candidates, and follow local
reaction closure within fuel/fan-out limits. Implement pure/derived nodes,
stateful gates, reusable Senses, and narrow `tune/revise/activate/pause/resume`
meta-control. Definition/parameter changes always become later occurrences.

**Exit gate:** preset rules and reusable derived values work end-to-end;
conflicts/cycles/fuel failures are inspectable; and the 1d → 3d Frequency
example proves that a meta-rule cannot mutate its current run or bypass its
manage grant.

#### K5 — authority, budgets, intents, and effect workers

**K5.1 and K5.2 are done. K5.3 is split:** its worker, lease, attempt, receipt,
retry, compensation, and emergency-stop machinery is built in E0.3 restricted to
the reversible-local-data family, because the engine's read → compute → write
loop must close before a product is built on it. What remains under this label
is every capability family outside that one — decision and notification
plumbing, registered UI and controllers, commands, HTTP, devices, and social
effects — which runs after the Economy vertical. The exit gate below applies in
full to E0.3 for its family, and again here for the rest.

Implement the policy intersection and atomic budget reservation before enabling
any automatic mutation. Start with reversible local Actions, then basic
decision/notification intent plumbing, then registered UI/controllers,
commands/HTTP/devices, and finally social effects. K7 adds the Attention policy
and recommendation lifecycle on top of that plumbing. Intents are durable;
workers lease them; retries depend on typed idempotency; uncertain external
outcomes require reconciliation rather than being guessed successful.

**Exit gate:** deny-by-default holds at both evaluator and domain Action
boundaries; revocation races and concurrent budgets are safe; restart never
duplicates an idempotent intent; and emergency/stage-effects modes prevent all
still-preventable dispatch.

#### K6 — Signals and integration adapters

**Runs after the Economy vertical, K5.3, and K12.** Economy needs no external
observation, so nothing here blocks the first product.

Implement polling, push, streaming, command/query, model, and microcontroller
adapters behind one observation envelope and scheduler. Preserve raw capture,
then normalize/calibrate in pure nodes. Secrets remain opaque handles; adapter
health, lateness, quarantine, cost, and source consent are Protein-visible.

**Exit gate:** HTTP plus one buffered microcontroller source survive duplicate,
late, malformed, stale, disconnect, reboot, and secret-redaction tests; turning
off a Signal stops acquisition/use without deleting history.

#### K7 — learning, recommendations, and Attention

**Runs after K6.** Deliberately late: the reaction-before-learning law means
inference may never precede a working deterministic path, and Economy provides
the first real evidence for anything to learn from.

Implement eligible-evidence admission, deterministic decayed recurrence/cadence
as the first model, checkpoint rebuild, probability/confidence separation,
drift, threshold/hysteresis, candidate lifecycle, feedback, decisions, whispers,
and digest budgets. Learning runs after reaction work and cannot train on its
own generated output without later independent evidence.

**Exit gate:** the apple example moves from cold evidence to one explained
suggestion/draft with no self-training, no duplicate recommendation, correct
decay/rebuild, and no authority derived from probability.

#### K8 — scoped Trust and Transfer autonomy

**Runs after K7.** Economy is local by construction, so no automation touches
another person until this phase.

Add the concept/person/Organ/proximity-scoped automation Trust policy specified
below, then connect candidates to the existing revision-safe Transfer Actions
one stage at a time: local draft, publish/propose, negotiate, own agreement,
activation, own occurrence/confirmation, and local settlement. Probability,
counterparty evidence, visibility, trust scope, principal grant, budget, and
Transfer domain readiness are independent gates.

**Exit gate:** exact allow/deny precedence and list/proximity Boolean selectors
work for relayed and origin Organs; a high-probability apple recommendation
cannot automate an untrusted Organ; and every Transfer stage is proven both
denied-by-default and permitted within an exact expiring scope.

#### K9 — durable workflows, projections, and optimization

**Runs after K8.** Economy's recurrence is a plan plus occurrences, not a
multi-step workflow, so it needs nothing here.

Build workflow instances over the already-proven occurrence/intent machinery,
then expose Imagination projections and deterministic solver adapters. A
workflow coordinates existing typed Actions and compensation; a solver creates
ranked plans with explicit objectives/constraints and never receives an effect
channel.

**Exit gate:** waits/retries/cancellation/compensation survive reboot; the weekly
scheduler explains alternatives/infeasibility; and applying a plan invokes only
separately approved current Actions.

#### K10 — replay, shadowing, and Deterministic Simulation Testing

**Runs last, and this is the acknowledged cost of the Economy-first order.**
Determinism is designed in and covered phase by phase — replay capsules, frozen
epochs, injected clocks, restart and ordering proofs all exist — but the
adversarial machinery that tries to break it does not arrive until here.

Make production depend on injected clock/entropy/I/O ports, build multi-Cell
simulation, fault generation, reference models, invariant checking, trace
shrinking, replay-capsule export, and old/new revision differential runs. Use
the same evaluator/scheduler/policy code as production; do not maintain a toy
simulation implementation.

**Exit gate:** seeded failures replay and shrink; sampled production capsules
hash-match; multi-Cell convergence and revocation/dispatch crash boundaries are
covered; and unsupported/timeout Proof results remain `unknown`, never green.

#### K11 — first vertical workflow: Economy

**Runs now, immediately after K5.2.** The original text required K0–K10 first;
that requirement is withdrawn. What Economy actually depends on is the exact
kernel (K1), the durable model and Actions (K2), the occurrence and Frequency
machinery (K2–K3), the evaluator (K4), and the grant boundary (K5.1–K5.2) —
all of which exist and pass. It depends on no Signal, no model, no Trust scope,
no workflow engine, and no simulation harness. E0.4's projector is not that
harness: folding the loop forward on a virtual clock is a product capability
built from the production evaluator, while K10 is the adversarial machinery that
generates faults and hunts for invariant violations. The first needs only a
snapshot and a horizon; the second needs everything.

Implement the Economy reference workflow specified below. Its gain/loss event types,
event drafts, recurrence occurrences, projections, Actions, and Protein source
come before its interface. The first frontend delivered on this stack is the
renamed **Economy** sand, not a Karma demo and not the current unwired
Finance placeholder. Remove `sand.finance`/`sand/finance` when
`sand.economy`/`sand/economy` is registered; compatibility is deliberately not
preserved.

The public Economy value/Action fixtures participate in K0's vocabulary freeze,
and signed gain/loss/projection math is implemented/tested with K1's pure
kernel. Deferring the vertical workflow must not permit K11 to invent another
decimal, schedule, capability, or replay contract. E0–E1 finish the Economy
backend; only E2 starts the sand.

Economy is the integration proof because it exercises exact values, Records and
Facts, Lingua units/tags, reusable Frequencies, durable occurrences,
Imagination, evidence, corrections, authority, capture provenance, aggregates,
and explanations without requiring an external effect. Implement E0–E1 before
starting the sand, then E2 as the first viable human workflow. E3 adds the
future Fiote ergonomics without broadening Economy's domain.

**Exit gate:** through the real socket, a person creates individual gain/loss
events over selected resource Records, corrects/voids them without rewriting
history, defines
recurring gains/losses, resolves their due occurrences, and sees server-made
monthly gains, losses, net flow, tag/source profiles, resource trend, and
recurring projection series with drill-down. The same draft/correction Actions
accept a software-agent principal, but no Fiote/model inference can apply a
Fact without the same review or grant as a human client. Restart, stale edit,
duplicate request, unit separation, visibility, keyboard/screen-reader, and
projection explanation tests pass.

#### K12 — second vertical workflow: Karma Flow Plane

After the Economy exit gate, build the **Karma** sand as the second complete
workflow. Its default surface is the **Flow Plane**: a two-dimensional,
zoomable map of everything a Cell can observe and everything Karma could cause.
Library, Builder, Why, Learn, Imagine, Authority, Queue, and Health are lenses
over this same plane and the same Protein/Action contracts, not separate tools
with private state. Build accessible forms first and make graph/DSL lossless
alternate views of one canonical AST.

The plane must enumerate all declared and currently reachable source ports,
including Records/Facts, saved and inline Protein, parameters, Frequencies,
manual occurrences, sync arrivals, decisions, workflow wakes, model/forecast
outputs, Signals, APIs, files, processes, devices, and microcontrollers. A
source that is configured but unavailable remains visible with freshness,
visibility, capability, connector-health, and last-evidence state; absence must
not make a dependency disappear from the operator's mental model.

The plane must also enumerate every possible outcome path before it happens:
derived values, emitted Facts, recommendations, drafts, decisions, workflow
transitions, meta-control, and typed effect/Transfer Actions. Inactive, denied,
staged, budget-exhausted, missing-secret, untrusted-Organ, or otherwise blocked
paths remain drawn and name the exact gate. This is how a person can audit “all
possible effects” without granting those effects or waiting for a live run.

The canonical default layout uses time from left to right and stable causal/
resource lanes from top to bottom. A source observation, schedule boundary, or
state transition is a point; a freshness window, threshold band, hysteresis
band, allowed value range, schedule tolerance, wait, or Trust validity is a
range. Crossing/entering/leaving a range visibly routes a token to the next
typed node, where Records, candidates, commands, or Actions can enter a new
state. Other layouts may be offered, but layout is presentation metadata and
never changes graph semantics or a revision hash.

The Flow Plane has five composable views. **Definition** shows the complete
static graph and dormant branches. **Live** overlays latest values, occurrence
order, evaluated edges, queue state, and receipts. **Why** walks either
direction through exact evidence and authority provenance. **Imagine** runs
the production kernel on a frozen/branched world and visually separates
projected changes from Facts. **Authority** overlays taint, recipient, grant,
Trust scope, budget, expiry, and the first gate that would require escalation.
No overlay calculates policy or schedule truth in JavaScript.

Editing a node, edge, point, or range produces a typed graph-revision draft,
runs validation and Proof, and then invokes the ordinary revise Action with an
expected revision and idempotency key. Dragging nodes only writes personal
layout state. A breakpoint, run-once, simulation, activation, pause, candidate
response, grant change, retry, or compensation likewise invokes its typed
Action; the canvas never writes Store rows or dispatches an effect directly.

Large graphs use server-projected dependency slices, stable node/edge ids,
viewport virtualization, semantic zoom, and incremental live overlays. The
client may cache geometry but must retain causal data only to its Protein cursor
boundary. Split the sand into focused Rust `body/style/script` modules and
focused JS modules for bridge, plane, layout, inspectors, each lens, and
accessibility; do not create one monolithic Karma HTML/script.

**Exit gate:** through the real socket, the Karma sand renders every source and
potential effect for the acceptance fixture; a person authors the old
condition → consequence example as points/ranges, sees a live transition create
only its permitted candidate, replays it in Imagine, inspects its full Why and
Authority paths, and revises/pauses it without direct database access. Blocked
and dormant paths remain inspectable. Keyboard/screen-reader navigation, live
updates, stale edits, permissions, emergency controls, restart recovery, and a
large virtualized fixture pass. Only then follow with the remaining vertical
workflows.

### Economy — the first complete Karma workflow

**Product name and boundary.** Rename Finance to **Economy** because this
surface is a view of how selected Lince resources grow and shrink, not a broad
personal-accounting product. For now it has exactly one domain idea: an
individual or recurring **gain/loss** changes or is expected to change one
Record's quantity. Records/Ledger own the resource and actual change; Lingua
owns its concept/unit; Karma owns recurring occurrences and projections;
Protein computes the monthly view; and the sand sends typed Actions. Neither
the sand nor Fiote gets a private financial database or arithmetic path.

Do not add account ledgers, double-entry postings, reconciliation, budgets,
goals, debts, investments, bank imports, exchange-rate portfolios, shared-book
semantics, tax tooling, or other conventional finance-product features to this
plan. If one is requested later, design it then against the pillars rather than
preloading Economy with unused abstractions.

#### Standing Economy invariants

1. **A gain/loss is one signed resource delta.** The Action carries a positive
   magnitude and direction; the engine canonicalizes `gain` to a positive Fact
   delta and `loss` to a negative Fact delta on exactly one resource Record.
   Arbitrary client-provided signs are rejected.
2. **Values are exact and unit-bearing.** The event uses the resource Record's
   Lingua unit and the K0 fixed decimal quantity representation; incompatible
   resources/units are never silently summed or converted.
3. **Applied history is append-only.** Drafts are revisioned and editable. An
   applied mistake is changed by compensating its Fact and appending the
   replacement; “delete” compensates and voids. Original Facts, authorship, and
   causal order remain visible.
4. **Actual and expected are distinct.** An applied individual event is actual.
   A recurring-plan occurrence is expected until a person or authorized agent
   applies it. A projection renders expected change separately and never writes
   it into the resource quantity.
5. **Visibility gates precede totals.** Hidden events cannot leak through
   monthly totals, tag/source profiles, percentages, graph points, forecasts,
   explanations, or Fiote context.
6. **Source, tags, and capture origin remain separate.** `source` identifies or
   labels where the gain/loss came from; Record links/Lingua concepts classify
   it; `capture_origin` says manual, typed, voice, photo, or agent; cause links
   retain the plan occurrence, capture, Program, and Facts. The UI may group by
   any of them without merging their meanings.

#### Economy implementation sequence

This E-sequence is nested under K11. **E0.0 – E0.3 are not Economy work; they
are the engine loop itself**, discovered incomplete during the 2026-07-25 audit
and placed here because Economy is the first thing that needs it. As of that
audit the read → compute → write loop the whole pillar exists to serve is open
at both ends and lossy in the middle: no Program can read a Record's quantity,
exact values cannot be multiplied or divided, no Program can change a Record,
and the durable representation between them is a float. E0.0 fixes the
representation, E0.1 the read side, E0.2 the arithmetic, E0.3 the write side.
Only then does E0 begin the Economy domain. E0–E1 are backend-only and must pass
before E2 begins the sand. E3 later adds Fiote ergonomics without changing the
gain/loss model, and now runs after K5.3.

E1's three routes — `suggest`, `draft`, and explicitly granted `apply` — all
work once E0.3 lands, so a recurring plan can propose to a person or apply
itself within its grant and budget. A person can still resolve any due
occurrence by hand through the same apply Action, because the automatic path
deliberately calls exactly the Action a human form calls.

##### E0.0 — exact deltas in the Ledger

Found while auditing the engine on 2026-07-25, and a hard prerequisite for
every exactness claim E0 makes. The Karma kernel is exact end to end: no `f64`
appears anywhere in `evaluate.rs` or `value.rs`, and amounts are carried as
mantissa-and-scale decimals. The Ledger it must write into is not — `fact.delta`
and `record.quantity` are both `REAL`. A rule that computes an exact `1.15kg`
today would land as a float in the only place the result is durable, and E0's
"all sums are exact" exit condition cannot be met on that representation.

**Decided 2026-07-25: the Ledger is rebuilt exact, not patched exact.** The
earlier draft of this slice stacked nullable exact columns beside `fact.delta`,
defined an *exact-clean* rule for chains that mixed the two, and required a
signed reconciliation Fact per existing Record to migrate. All of that existed
only to protect databases that will be deleted. The licence to edit unreleased
migrations in place is hereby extended past the Karma migrations to the whole
schema: `fact` and `record` are rebuilt in `0001` itself, and the exact-clean
rule, the mixed-chain refusal, and the reconciliation Fact are **deleted from
this plan** rather than implemented. There is no legacy representation to
tolerate, so no code should be written to tolerate one.

- [x] `fact.delta` becomes an exact signed decimal — mantissa and scale — and the
  `REAL` column is removed, not kept beside it. Two representations of one
  quantity is precisely the ambiguity this rebuild exists to avoid. The exact
  pair is inside the hash preimage, so an amount is covered by the Fact's
  signature rather than annotated next to it.
- [x] **Exact decimals are strictly finer than the floats they replace, which is
  the whole point.** `0.1` is exactly `0.1` — a value `f64` cannot represent at
  all — and scale runs to 18 places, so any fractional precision a float could
  express is expressible here and reproducible. Nothing about quantities becomes
  coarser or integer-only; adding a third of a kilo, a price of `19.99`, or a
  rate of `0.0725` all stay exact, and stay exact after a thousand additions,
  which floats do not.
- [x] `record.quantity` stays a cache, as its own comment already says, and is
  rebuilt to carry the exact pair too so a cached level never disagrees with its
  chain by a rounding step. Quantity truth remains the sum over the Fact chain.
- [x] Sum exact deltas in Rust as `i128` at a common scale, never in SQL, for
  the same reason budget quantities are summed that way: SQLite numeric affinity
  does not preserve exact decimals.
- [x] Rebuilding `0001` breaks every existing local database, deliberately. Say
  so in the implementation note: a stale `.db` must be deleted, not migrated.
  This licence ends the moment any migration reaches a real deployment.
- [x] Prove it: a gain and a loss whose float representation would drift
  (`0.1 + 0.2`) sum exactly; a thousand additions of `0.01` reach exactly `10`;
  and no `REAL` quantity column survives anywhere on the Fact/Record path.

**E0.0 exit:** an exact amount survives the round trip from kernel decimal to
Fact to aggregate without ever becoming a float, and there is no second
representation for it to become.

**Landed 2026-07-25.** `crates/store/tests/exact_ledger.rs` holds the six
proofs, including a `pragma_table_info` assertion so "no REAL survives" is
checked against the live schema rather than by reading the migration. What the
implementation settled beyond the plan text:

- **The kernel's decimal *is* the Ledger's decimal.** `nucleus::DecimalValue`
  is re-exported at the crate root and used by `Fact`/`NewFact` directly, so
  there is no conversion between an evaluation type and a storage type — which
  is what makes "survives the round trip" true by construction instead of by
  care.
- **Storage is `(mantissa TEXT, scale INTEGER)`.** The mantissa is TEXT because
  it is an `i128` and SQLite's INTEGER is 64-bit, which truncates at scale 18
  above ±9.22 units. A canonical mantissa has no leading zeros and no negative
  zero, so `mantissa != '0'` is an exact is-nonzero test and `mantissa LIKE '-%'`
  an exact is-negative test — the activation and debt predicates survive as
  string tests. `SUM()`/`delta > 0` in SQL do not survive: the window folds
  fetch and fold in Rust, bounded by the existing `(record_uid, at)` index.
- **Scale never needs to grow past 18.** Alignment takes `max(scale)`, not their
  sum, and every `DecimalValue` is `<= 18` by construction, so the anticipated
  "cache eventually needs scale 19" case cannot arise and needs no refusal rule.
  The cache absorbs the finest scale its facts declare.
- **The checkpoint payload was an exactness hole with no REAL column in it.**
  `{"level": q}` was a JSON float, and after compaction that payload *is* the
  record's level. It now carries canonical decimal text plus its scale.
- **A latent chain-determinism bug is fixed on the way.** The hash preimage
  interpolated an `f64` via `Display`, so `0.1 + 0.2` hashed as
  `0.30000000000000004` and chain bytes depended on the float formatter. The
  preimage is now `scale:canonical-text`, which also makes a declared `1.5` and
  a declared `1.50` different Facts — precision is signed, not annotated.
- **The lossy inbound door is one named function, not a `From` impl.**
  `NewFact::quantity_f64` / `exact::from_f64` mark every producer that still
  computes in floats (transfers, senses, the legacy rule fold). They are
  greppable, and that grep is E0.2/E0.3's worklist. `to_f64()` stays freely
  available outbound for display and the legacy projection: the exit condition
  is about the durable path, not about rendering.
- **Non-goals, decided rather than overlooked:** `promise.delta`,
  `link.quantity`, and `transfer_occurrence.quantity` stay `REAL`. E0.0 names
  only `fact.delta` and `record.quantity`; promises are E1's problem. Protein
  still emits quantities as JSON numbers so the sands keep working — E0.1 owns
  the exact read side.
- **Declared precision survives the sync wire, and that is now tested.**
  `Package.facts` is `Vec<Fact>`, so a delta crosses between Cells exact. It
  matters: a `1.50` that round-tripped through `f64` would come back as scale 1,
  hash as `1:1.5` instead of `2:1.50`, fail `verify_chain_step`, and be
  quarantined. The existing sync tests all use scale-0 values (`10`, `3`, `0`)
  and could not catch that, so `declared_precision_survives_the_sync_wire`
  syncs a trailing-zero decimal and asserts an empty quarantine.
- **`bump_quantity` is a read-modify-write, and it is safe for a sharper reason
  than "it runs in a transaction":** `facts::insert` runs first in that same
  transaction, so the write lock is already held before the SELECT. Under a
  DEFERRED begin, "inside a transaction" alone would not be enough.
- **Two SQL triggers referenced `fact.delta` and had to move with it**
  (`0020_transfer_settlement_corrections.sql`). The zero-evidence check became
  `delta_mantissa = '0'`; the compensation-matching check compares the exact
  pair against the transfer side's REAL by building `10^scale` as text, so it
  needs no SQLite math extension.

**History is not a second object.** Compaction today writes pre-checkpoint Facts
to a cold JSONL file, deletes them from `fact`, and anchors the file by hash. The
history therefore survives but stops being *readable* — and the instinct to fix
that by versioning the Record, or copying it per concept, would create a second
thing that can disagree with the chain. It is not needed: **a Fact already is the
history.** What compaction actually removes is queryability, and that is what
this slice restores.

- [ ] **Separate two properties the retention horizon currently conflates:**
  whether a Fact is still needed to compute the level, and whether a human can
  still read it. After a checkpoint the first is already false — the checkpoint
  carries those deltas — but today that also forces the second to be false,
  because compaction deletes. Split them: archived Facts move to a `fact_archive`
  table with identical columns rather than leaving the database. Level and sum
  queries never read it, because the checkpoint already accounts for it, so
  archived history is *unfolded by construction and not by a flag*. History
  queries union it when the window reaches back past the checkpoint. **Decided
  2026-07-25: the same database file, storage cost accepted.** Compaction then
  buys a smaller hot table and faster indexes rather than a smaller file — one
  file stays one backup, and history reads stay ordinary transactional queries
  instead of filesystem access.
- [ ] **The checkpoint is the past/present boundary, and nothing else needs to
  mark it.** Everything before a Record's last checkpoint is settled history:
  already folded, never re-summed, immutable. Everything after is live. This is
  structural and self-maintaining — no `is_history` column to set, no mode for a
  person to remember, and no way for the two to disagree.
- [ ] **Archive the classification with the Facts.** A `-10` whose concept
  sidecar was left behind is an unreadable number; history that cannot say *what*
  a movement was is not history. The same applies to the unit in force at the
  time, which the checkpoint records, so a later change to `record.unit_uid` does
  not silently reinterpret old amounts. Keep `prev_hash`/`hash`/`signature` intact
  so archived history stays verifiable rather than merely asserted.
- [ ] **Checkpoint on a cadence, not once.** A single checkpoint collapses
  everything before it to one level, so past *levels* become unanswerable at any
  finer grain. Writing one per period preserves the level series across
  compaction at that granularity, which is what E0's level-series query falls
  back to once a Record has been compacted.
- [ ] **Retention policy follows concepts, not `record.kind`.** The current table
  is keyed on kind, which cannot express "keep my grocery history for two years."
  Key it on concept with DAG inheritance — a policy on `@food` governs
  `@ice-cream` unless overridden — plus an explicit never-archive setting.
  Nearest-ancestor resolution already exists in `concepts::nearest_ancestor_in`.
- [ ] **Compaction must not silently change what rules compute.** A program
  reading `sum(@x, 90 days)` reads the `fact` table; archiving 90-day-old Facts
  changes its answer with no error and no trace. The effective horizon for a
  Record is therefore the configured horizon *or the longest lookback window any
  active program uses against it, whichever is longer*, and compaction refuses
  rather than truncating a window a live rule depends on.

##### E0.1 — the read side: record values, units, and conversion

Found in the 2026-07-25 audit. The engine's read side is declared but not
implemented, so today a Program can only compute over its parameters and its own
durable state, triggered by a Frequency. This slice makes every number a Record
carries available to a condition or a derived value.

- [x] **Resolve the boundary inputs.** `InputSource::RecordQuantity` exists in
  the AST, parses in the DSL, type-checks in Proof, and is covered by pure kernel
  tests with hand-supplied values — but the durable runtime never fills it.
  `evaluate_member` populates boundary values for `Trigger` nodes only, a single
  bool per trigger saying whether this occurrence matched its Frequency. A
  Program carrying a `record-quantity` input fails evaluation today with
  `MissingInput`. Implement the resolver for `RecordQuantity` and `SavedProtein`;
  leave `Signal`, `SecretMetadata`, and `CapturedFact` to K6, and make an
  unresolvable source a typed refusal rather than a missing value.
  **Landed for `RecordQuantity`** in `store::karma::runs::evaluate_member`, with
  `ProgramRunBlockCode::{RecordQuantityUnavailable, RecordUnitUntypable,
  InputSourceUnresolved}` as the typed refusals — a deleted Record blocks the
  run rather than reading as zero, because "the Record is gone" and "the Record
  holds nothing" are different facts. **`SavedProtein` did not land, and needs a
  seam rather than more code:** `protein` depends on `store`, so the runtime
  cannot call a Protein from where it resolves inputs. It needs a resolver
  injected by `engine`, which sits above both. Until that exists it blocks with
  `InputSourceUnresolved` rather than pretending.
- [x] Read the quantity **from the exact Fact chain that E0.0 establishes**, not
  from the `record.quantity` cache. Since E0.0 rebuilds the Ledger exact rather
  than patching it, every chain is exact by construction and there is no
  mixed-representation case to refuse. Freeze the value into the replay capsule
  like every other boundary value, so a run stays replayable and a later Fact
  cannot silently change what a past run saw. **`store::facts::level` folds the
  chain anchored on the last checkpoint that carries a level** — retention
  genuinely deletes archived Facts, so folding the surviving rows would
  under-report a compacted Record, and compaction's archive anchors are
  checkpoints too but carry `{archive, ...}` rather than a level, so they are
  skipped instead of read as zero. Freezing is automatic: the resolved value
  goes into `boundary_values`, which is exactly what `capture_evaluation_replay`
  captures.
- [~] **A Record's quantity arrives typed by its own unit.** A Record carries
  `unit_uid`; when it is set, the input resolves to `Quantity { amount, unit }`
  and the unit becomes part of the static type that Proof checks at every node
  boundary. When it is absent the input resolves to a plain `Decimal`. A Program
  must be able to state which of the two it expects and be refused at publish
  time if the Record disagrees — a rule that silently treats litres as kilograms
  is worse than a rule that will not compile. **The runtime half landed**
  (united Records resolve to `Quantity`, unitless to `Decimal`, an untypable
  unit blocks). **The publish-time refusal did not:** Proof is pure and cannot
  see a Record's unit, so the check belongs in the store's publish path
  alongside revision validation, not in `proof.rs`. Without it a unit mismatch
  surfaces at run time instead of at publish time — later than it should, but
  still a typed refusal rather than a wrong number.

  **Open task — publish-time unit refusal.** Where it goes: the Program
  create/revise path in `store::karma::programs`, which already validates a
  revision and can reach Records. What it does: for every `record-quantity`
  input, resolve the named Record's `unit_uid` and compare it against the unit
  the Program's port contract declares, refusing the publish when they
  disagree. Why it cannot live in Proof: `evaluate_program` and `proof.rs` are
  pure by design — no clock, no SQL — and a Record's unit is world state. Proof
  keeps checking that a `Quantity` is not used where a `Decimal` is expected;
  only the store can check that *this* Record's unit is the expected one. Doing
  this is what turns "a rule that treats litres as kilograms fails on its first
  run" into "it never publishes".
- [ ] **Read the other values a Record holds.** Add an input source for a
  numeric value inside a namespaced `record_extension`, addressed by namespace
  and field path, declaring both its value type and its unit. Missing field,
  wrong JSON type, and unparseable exact decimal are typed refusals, never a
  silent zero. This is what lets one rule read a Record's weight, its price, and
  its count together.
- [x] **Convert between units explicitly, and exactly.** Lingua already stores
  conversions — `concept_conversion` holds one factor row per unordered unit
  pair, derives the inverse at read time, and honors conversion only within a
  shared dimension — but the factor is `REAL`. Store the factor exactly as an
  integer numerator and denominator alongside the existing float, so `kg → g`
  is exact and `kg → lb` stays exact until something asks it to round. Expose
  conversion as an explicit operation with a declared result scale and rounding
  rule; never convert implicitly to make an expression type-check, because an
  implicit unit coercion is how a rule quietly computes the wrong number.
  **Deviation, decided 2026-07-25:** the plan said to store the ratio *alongside
  the existing float*; the float column was **removed** instead. Keeping both is
  the same two-representations-of-one-quantity ambiguity E0.0 spent a whole
  slice deleting, and there is no legacy database to protect. The legacy `f64`
  `convert()` now derives its factor from the exact ratio, so the two paths
  cannot disagree about what a conversion means. Inverting a rational is
  lossless, which is what makes the derived `b → a` direction exact too.

**E0.1 exit:** a Program reads one Record's unit-typed quantity, another
Record's extension value in a different unit of the same dimension, converts
one to the other explicitly, and compares them; the run replays identically
from its capsule; a cross-dimension conversion, a missing extension field, and
a unit mismatch each fail with their own code before anything is proposed.

##### E0.2 — arithmetic that can actually express a rule

- [ ] **Add multiplication and division for exact values.**
  `evaluate_integer_product` handles `I64` only: `Decimal`, `Quantity`, and
  `Money` multiply/divide fall through to an invariant error, so exact values can
  currently only be added and subtracted. Without this there is no percentage,
  rate, unit price, or split — and this document's own "shared percentage"
  example cannot be expressed.
- [~] Multiplication states its result scale explicitly and division states its
  rounding rule, because neither is closed over fixed-point decimals: no scale
  represents `1/3`. An implicit rounding mode is how exactness silently dies, so
  the rule is named in the AST, frozen in the revision hash, and shown in the
  Why lens. Half-up, half-even, toward zero, and away from zero are the frozen
  vocabulary. **The vocabulary and the arithmetic landed early with E0.1**,
  because unit conversion needs the identical rule and two rounding
  implementations would be one too many: `Rounding` plus
  `DecimalValue::{mul_ratio, div_exact}` returning `RoundedDecimal { value,
  exact }`. Every inexact operation reduces to one rational multiply, and the
  `exact` flag is how a discarded remainder gets *reported* rather than lost.
  **Still E0.2's:** wiring these into `evaluate_integer_product` so the AST can
  express them, and putting the rounding rule in the revision hash.
- [ ] Unit algebra is explicit, not inferred. Multiplying a `Quantity` by a
  plain `Decimal` keeps the unit — the percentage and rate cases, which is what
  most rules need. Multiplying two united quantities, or dividing one by another,
  produces a value whose unit the author must declare, and Proof refuses it
  otherwise rather than inventing `kg²` or silently dropping a dimension.
- [ ] Division by zero, scale overflow beyond `MAX_DECIMAL_SCALE` (18), and
  mantissa overflow are typed evaluation failures that keep their existing
  terminal-failure semantics, never a saturating or wrapped value.

**E0.2 exit:** `quantity(record:@apple) * 0.15` and `total / count` both
evaluate exactly with their declared scale and rounding; the rounding rule is
part of the revision hash, so changing it is a visible revision; a zero divisor
and a scale overflow each fail with their own code; and a rounded division
proves in test that the discarded remainder is reported, never silently lost.

##### E0.3 — the write side: closing the loop onto Records

The engine's whole purpose is that a rule reads the world, does arithmetic, and
changes it. E0.1 and E0.2 open the read and compute halves; this slice opens the
write half, which is the one thing Karma has never been allowed to do. It is
K5.3's machinery restricted to the reversible local data capability family, and
it is pulled ahead of the Economy domain work so the engine's central claim is
proven before a product is built on it.

Pulled forward as a whole, not as an apply-only shortcut. Leases and typed
idempotency are not polish on top of applying a change; they are what stops a
restart mid-apply from applying it twice. An `apply` path without them is the
duplication bug the K5 exit gate names, so the machinery arrives with the
capability it protects.

- [ ] **A Record's quantity may be defined by a Program.** `Total cost = Cost1 +
  Cost2` is the plain case the pillar has to serve, and it gets a first-class
  answer rather than being left to a person to wire by hand. A Record carries an
  optional binding to a Program revision and an output, in one of two modes the
  author picks per Record. **Computed:** the quantity *is* the program's output,
  resolved exactly on every read, with no Fact ever appended. It cannot drift
  from its inputs because it is never stored — change Cost1 and Total cost is
  already correct, with no occurrence, no intent, and no grant involved, because
  nothing was written. **Materialized:** a Frequency fires, the program runs, and
  an authorized intent writes the value onto the Record as an ordinary signed
  Fact, so the number is pinned in history and the Record accrues a real chain
  you can audit month by month. Both are the same program and the same
  arithmetic; the only difference is whether the value is remembered.
- [ ] **The two modes answer different questions, and choosing wrong is the
  common mistake.** Computed answers "what is my total cost" — always live, no
  history, free. Materialized answers "what was my total cost each month last
  year" — a real series, at the price of an occurrence, an intent, and a grant.
  A rolling balance that must accumulate (savings drawn down monthly) is
  necessarily materialized, because its next value depends on its previous one. A
  pure restatement of other Records (total cost) should default to computed.
- [ ] **A computed Record refuses conflicting writes.** Its quantity has exactly
  one author, its binding. A manual Fact, an `Action::AddQuantity`, or another
  program's intent targeting it is a typed refusal naming the binding, not a
  silent overwrite that would be erased on the next read. Rebinding or unbinding
  is an explicit revision, and unbinding freezes the last computed value into one
  signed Fact so the Record keeps a defined quantity.
- [ ] **Cycles are refused at Proof time, not discovered at runtime.** Bindings
  form a graph over Records; `Total cost` depending on a Record that depends back
  on it is rejected when the binding is authored, with the cycle named. Depth and
  fan-out are bounded by the same fuel the evaluator already meters.
- [ ] **Execute an authorized intent, for `LocalReversibleData` only.** A worker
  leases an intent, records a typed attempt, applies the change in one Store
  transaction with the intent's frozen idempotency key, and writes a receipt.
  The capability ceiling K5.2 already enforces twice stays exactly where it is:
  nothing outside this family becomes executable here.
- [ ] **The targets a rule may change.** A Record's plain quantity; a Record's
  unit-denominated quantity, where the intent's unit must equal the Record's
  `unit_uid` or carry an explicit E0.1 conversion, checked again at apply time
  against the live Record rather than trusted from the proposal; and a numeric
  value inside a namespaced `record_extension`. Each is a distinct capability
  with its own grant scope, so authority over a Record's weight is not authority
  over its price.
- [ ] Every applied change lands as an ordinary signed Fact on the Record's own
  chain with an exact delta from E0.0, caused by the intent. Karma gets no
  private write path: the Ledger stays the single quantity truth, and an
  automatic change is auditable by exactly the same means as a human one.
- [ ] **Compensation is real, not nominal.** Reversing an applied intent appends
  the opposite exact delta caused by the original, matching the existing
  correction semantics; a metadata write restores its previous frozen value.
  Reversibility is what makes this family safe to automate first.
- [ ] **A compensated intent keeps its budget consumed.** `holds_reservation()`
  currently returns false for `Compensated`, which would refund it, and a grant
  capped at ten intents could then apply-and-compensate forever without ever
  exhausting. A budget limits how much a delegation may *cause*, not how much of
  what it caused survives: an applied-then-reversed change touched a real Record
  and appended two real Facts. So the rule is that a reservation is consumed by
  having caused an effect, not by the effect persisting — `Compensated` becomes
  true, while `Failed` and `Cancelled` stay false because nothing happened.
  Revisit `DeadLetter` here too: it is false today, which is only correct if
  nothing was ever applied.
- [ ] Retries are driven by the frozen idempotency key, an uncertain outcome is
  reconciled against the Record's own chain rather than guessed, and emergency
  stop plus stage-effects mode prevent every dispatch that is still preventable.
- [ ] Revocation now has teeth: `cancel_for_grant_tx` selects only `authorized`
  intents today, which is correct while that is the only reservation-holding
  state. This slice widens it to every state that holds a reservation, or a
  revoked grant would leave leased work alive.

**E0.3 exit:** through the real socket, a Frequency fires, a Program reads a
Record's exact quantity, multiplies it by a granted percentage, and the
authorized intent changes that Record's quantity — with the Fact, the receipt,
and the policy proof all inspectable. A restart mid-apply applies it exactly
once. Revoking the grant mid-flight stops it. Compensating it returns the
Record to its previous exact value. Without a grant, nothing runs at all.

##### E0.4 — folding the loop forward: projection over Karma programs

**This slice exists because the loop is only half useful if it can only run at
the speed of real time.** The stated purpose of the pillar is to set rules with
frequencies and then see how quantities move over a year or five — to answer
"what happens to my savings" without waiting sixty months to find out. E0.0–E0.3
build the loop that runs once, on the real clock, against the real Ledger.
Running it against a virtual clock and a virtual Ledger is a separate slice, and
it was previously scattered between E1's fixed-occurrence fold and the K10
Imagination work. Neither covers it: E1 folds *declared recurring amounts*, not
computed ones, and K10 runs last.

- [ ] Project by substituting ports, never by writing a second evaluator.
  `evaluate_program` is already pure over `(ProgramAst, FrozenEvaluationContext,
  EvaluationLimits)` — it has no clock and no store. Only two things around it
  are production-bound: E0.1's store-backed boundary resolver and E0.3's intent
  applier. Projection replaces the first with a virtual quantity map and the
  second with a virtual fold. A rule that is wrong in projection is wrong in
  production, which is the entire value of the property.
- [ ] Enumerate occurrences ahead of `now` from the same Frequency machinery.
  `ScheduleSpec`/`ScheduleCursor`/`OccurrenceRange` already compute boundaries
  arithmetically from a cursor rather than by waiting; projection walks that
  forward to a horizon instead of to the present. Frozen timezone and tzdb rules
  apply unchanged, so a projection crossing a DST boundary lands where execution
  would have landed.
- [ ] Fold both binding modes from E0.3 in one timeline. A **computed** Record —
  `Monthly expenses = Rent + Utilities + Groceries` — is re-derived at every
  projected step from that step's projected inputs, never carried forward as a
  constant. A **materialized** Record — savings drawn down monthly — is folded
  occurrence by occurrence. They compose: raising projected rent in month 30
  changes projected monthly expenses in month 30, which changes the projected
  draw on savings from month 30 on. A projector that re-derives only at the start
  produces a plausible curve that is wrong everywhere after the first change.
- [ ] Fold classified promises alongside program occurrences. "In three months I
  expect +300 from a sale" is a `promise` — `record_uid`, `delta`, `window_end`,
  and the already-present `promise.concept_uid`, so a future annotation
  classifies through the same DAG as a past Fact and lands in the same E0 bucket.
  `build_snapshot` already folds these, but only in `Agreed`/`Active`, and a
  solo expectation created today starts at `Proposed` — so this needs a one-step
  path for an expectation with no counterparty, not a new primitive and not
  asking someone to agree with themselves.
- [ ] Return exact points, not floats. The projection carries E0.0 decimals and
  E0.1 units end to end, labels every point actual or projected, and names each
  projected point's cause — program revision hash, occurrence identity, and the
  intent shape it would have staged. A point nobody can trace to a rule and a
  moment is not a projection, it is a guess.
- [ ] **Exclusions are reported, never silent.** The legacy fold in
  `nucleus::imagination` quietly skips rules needing signals or sums, so a
  timeline can be confidently wrong. This one refuses that: any program that
  cannot be folded — an unresolvable input, a non-local capability, an external
  effect, a mixed inexact chain under E0.0's rule — appears in a named exclusion
  list attached to the result. A partial projection that does not say what it
  left out is worse than no projection.
- [ ] Projection writes nothing: no Fact, no intent, no candidate, no cursor
  advance, no grant consumption. It reads a snapshot and folds. Branching is
  mutating that snapshot — change a starting quantity, toggle a program, alter a
  rate — and folding again; comparing two timelines is the compare view. Applying
  anything a projection suggests is an ordinary reviewed Action.
- [ ] Bound it. Five years of a monthly Frequency is sixty evaluations per
  program, which is cheap, but a daily Frequency over the same horizon is not,
  and a program graph can be large. The horizon, total occurrence count, and
  total fuel are explicit limits, and exhausting one truncates the timeline with
  a stated reason rather than hanging or silently stopping early.
- [ ] Note the regression this prevents: `Engine::project` folds the legacy
  `registry.rules` today and is what powers `crossings_pass`. Once rules are
  imported to Karma as planned, that fold's input goes empty and the existing
  five-year projection quietly goes dark. This slice is what carries the
  capability across, and `crossings_pass` moves onto it.

**E0.4 exit:** a Rent Record and several cost Records feed a derived monthly
expenses rule; a Frequency-driven program draws that from savings each month;
projecting five years returns sixty exact points whose final value matches a
hand-computed decimal exactly. Raising rent and re-folding changes the curve.
The timeline is identical under a DST-crossing timezone and under a virtual
clock started at any instant. Nothing is written. And running the same program
for real across the same window produces the same numbers the projection gave —
because it is the same code.

##### E0 — classified movements over Records and Facts

**Classification attaches to the Fact, not the Record, and this is derived, not
chosen.** `record.concept_uid` says what a quantity *is of* — the money Record's
concept is money. It therefore cannot say what a *movement* was. Buying an ice
cream is a `-10` Fact on the money Record; the fact that it was a cost, and
specifically a food cost, is a property of that movement. Anyone later tempted to
"simplify" this into a Record per purchase should read this paragraph first: that
design forces every spend to invent a Record, and still cannot answer "what did I
spend in March" without a hand-maintained sum.

**The concept DAG is the tag system.** `concept` + `concept_parent` +
`descendants_including` already exist, are multilingual through `concept_name`,
and are hierarchical, so classifying a Fact `@ice-cream` makes it answer a query
for `@food` and `@cost` with no list to maintain. Do not add a parallel flat tag
field; a second classification axis is a second thing to disagree with the first.

**A concept may have many parents, and that already works.** `concept_parent` is
`UNIQUE(concept_uid, parent_uid)` — a many-to-many edge table, not a single
parent pointer. So `@food` can be a child of both `@substance` and `@cost` at
once, and querying `@cost` reaches every food. Nothing needs building for this;
it is the DAG doing what it was built for. Worth stating because a role like
`@cost` sitting above a kind like `@food` looks wrong under a strict taxonomy —
here it is fine, because direction is the delta's sign, so food you *sell*
appears as a positive movement under the same concept rather than needing a
second one.

- [x] **A Record carries many concepts, not one.** A toothbrush is a cost *and* a
  health item, and both must be queryable — as a recurring expense in Economy and
  as a view of hygiene items elsewhere. `record.concept_uid` is a single column
  and 57 call sites depend on it, including Transfer matching and sync, so it
  **stays** as the Record's *identity* concept — what the thing is. Add a
  `record_concept` join table for its additional classifications, and have every
  concept query read the union of both. This is additive: existing matching keeps
  working untouched, and it is semantically right rather than a compromise — a
  toothbrush *is* a toothbrush and *counts as* a cost and a health item.
  **Landed** in `0036_economy_classification.sql` with
  `economy::{add_record_concept, remove_record_concept, record_concepts,
  records_with_concept}`; the last expands down the DAG and unions both sources,
  so a Record classified either way appears exactly once.
- [ ] Classification therefore has two levels that must not be confused. A
  **Record's** concepts say what the thing is and counts as, and drive views like
  "everything I use for hygiene". A **Fact's** concept says what a particular
  movement was, and drives Economy's totals. Buying the toothbrush is one `-10`
  classified `@hygiene-purchase`; the toothbrush Record being `@health` is a
  separate, standing truth. A query may filter on either, and the sand must say
  which it used.

**Currency conversion is out of scope for now (decided 2026-07-25).** Sums stay
unit-separated and simply refuse to combine two currencies. Guard rail for
whoever gets there: `concept_conversion` holds one factor per pair with **no time
dimension**, which is correct forever for `kg → g` and wrong for money the moment
a rate moves — converting a 2020 expense at today's rate silently rewrites
history. Do not reuse that table for currencies; time-versioned rates are a
separate design.

- [ ] Freeze the movement classification and its exact magnitude, resource/unit
  reference, source, note, and occurrence/capture cause as revision types in
  `nucleus`. **Drop `EconomyDirection = Gain | Loss` and the separate `tags`
  field from the earlier draft of this slice.** Direction is the sign of the
  delta and nothing else: a refunded ice cream is classified `@cost` with a
  `+10` delta and must *reduce* total costs. A direction enum bucketed
  independently of the sign gets that backwards, and two classification axes that
  can disagree is a bug frozen into the vocabulary. This slice is unimplemented,
  so removing both is free now and expensive later.
- [x] Add classification as an append-only sidecar keyed on `fact_uid`, not a
  column on `fact`. `fact` is hash-chained and signed and E0.0 already restricts
  it to additive change; a `concept_uid` inside the preimage makes every existing
  Fact permanently unclassifiable, and one outside the preimage is unsigned
  mutable data masquerading as ledger truth. The repo already has the right shape
  in `fact_action_intent`. Structure it as the log-plus-projection idiom Karma
  uses for `karma_intent_event`/`karma_intent_state`, so **a classification is an
  assertion about a Fact, not part of it**: correcting a mistagged expense is a
  new assertion with an audit trail, never a compensating Fact over a typo.
- [ ] **Aggregation is a query over classified Facts, and never over the
  quantities of cost Records.** `Rent = 1000` is a standing parameter a rule
  reads; the monthly `-1000` on savings classified `@rent → @cost` is what a
  total sums. Stating the domain this precisely is what stops the aggregate and
  E0.3's computed `Total cost` Record from becoming two overlapping numbers on
  one screen. A computed Record remains useful as a *rule input*; it is not how
  totals are formed, which is precisely the hand-built sum this design removes.
- [ ] The query sums signed deltas whose classification descends from a chosen
  concept, bucketed by `fact.at`, over a *set* of resource Records selected by
  concept — money lives in checking, cash, and savings at once. Sums stay
  unit-separated. `fact.at` is occurred-at, not recorded-at, so backdating is
  ordinary and nothing may assume `at` is monotonic with chain order.
- [x] **Windows are arbitrary half-open instants, not trailing durations.**
  Every existing helper — `facts::sum_window`, `sum_pos_window`, `sum_neg_window`
  — takes `window_secs` back from `now`, which cannot express "10:23 on 1 Jan
  2020 until 00:00 on 2 Mar 2025". Add `[from, to)` variants taking two explicit
  instants. Half-open on purpose: adjacent periods must tile without a Fact
  landing in both. Positive and negative sums come back separately alongside the
  net, so inflow and outflow are visible without a second pass.
- [x] **Normalize `fact.at` to UTC `Z` on write, or range scans are silently
  wrong.** `at` is TEXT compared lexically, which only orders correctly when
  every timestamp shares one offset format. A Fact written with a `-03:00` offset
  sorts into the wrong place and drops out of windows it belongs to. Store
  normalized UTC and render local at the boundary. Audit existing rows as part of
  this slice; a wrong answer here looks exactly like a correct one.
- [x] **Levels come from the chain anchored on the last checkpoint, never from
  the quantity cache.** "At the end of last month I had 10, now I have 20" is a
  distinct query from a sum: level at instant `t` is the checkpoint at or before
  `t` plus every delta after it up to `t`. It cannot fold from zero, because
  retention genuinely deletes archived Facts — `archivable_before` and
  `delete_by_uids` compact them into a checkpoint level — so a naive full-chain
  sum silently under-reports on any compacted Record. `record.quantity` is the
  cache for *now* only and is never the answer for a past instant.
- [x] A level *series* is one ordered scan over the window producing a running
  balance, so the past line of the graph is a single query rather than one call
  per point. It is computed in occurred-at order, which is what makes a backdated
  Fact correctly reshape history behind it.
- [x] Anything whose concept does not descend from the configured root appears in
  an explicit unclassified bucket, never silently dropped — the same rule E0.4
  commits to for projection exclusions.

**E0 store layer landed 2026-07-25** in `crates/store/src/economy.rs`:
`classify_fact` / `fact_concept` / `classification_history` for the assertion
log and its projection; `movement_totals` and `movement_totals_by_concept` over
a `MovementWindow { record_uids, from, to, concept_uid }`; and `level_at` /
`level_series` for the balance questions. Notes on what the implementation
decided:

- **Totals read a *set* of Records**, because money lives in checking, cash and
  savings at once and a total that can only read one Record cannot answer "how
  much money do I have". Mixing units is refused rather than summed.
- **Gains, losses and net come back together** from one scan, so inflow and
  outflow are visible without a second pass — and because direction is the
  delta's sign, a refund classified `@cost` correctly *reduces* the cost total.
- **The concept filter expands down the DAG once**, before the query, so asking
  for `@cost` reaches every `@food` without the SQL knowing about hierarchy.
- **`fact.at` normalisation turned out to be a precision trap, not just a
  format one.** The obvious fix — store a fixed-width UTC millisecond string —
  would have silently broken `verify_chain_step`, because the hash preimage is
  rebuilt from the parsed `DateTime` and truncating the stored instant changes
  what a re-read Fact reconstructs. It is stored at nanosecond precision with a
  `Z` suffix: fixed width *and* lossless. Every comparison against `at` goes
  through the same `facts::instant` helper, since a `+00:00` cutoff against a
  `Z` row compares wrong in a way that looks exactly like a correct answer.
- **Still open in E0:** the classification log is append-only with an actor and
  timestamp but is *not* hash-chained or signed, unlike `fact`. That is
  deliberate — a classification is an assertion *about* the Ledger, not Ledger
  truth — but if classification ever becomes evidence rather than convenience,
  it needs the chain. The Economy event/draft sidecars and the schema-owned
  apply transaction are also not built.
- [ ] Add schema-owned sidecars for an Economy event and draft. Applying a
  draft in one Store transaction creates/revises the event Record, appends the
  signed Fact to the selected resource Record, links provenance/classification/
  source, and stores the idempotent Action result. It does not create accounts,
  categories, postings, or another balance truth.
- [ ] Implement create/read/revise/apply/void Actions with `request_id` and
  `expected_revision`. Revising an applied event compensates its old Fact and
  appends the replacement; voiding compensates it. A human form, CLI, Fiote, or
  other software agent uses the same `EconomyEventDraft` contract.
- [ ] Add the narrow `source:"economy"` Protein union for events, drafts, and
  overview/profile queries. All sums are exact, unit-separated, visibility-
  gated, server-bucketed, and drillable to the contributing events/Facts.

**E0 exit:** gain/loss by sign, a refund reducing its own category, re-tagging
with its audit trail, edit, void, stale/replayed Action, crash rollback,
visibility, unit mismatch, month boundary, unclassified bucket, and
descendant-concept aggregation tests pass. An arbitrary instant-to-instant
window returns the same total as the sum of its tiled sub-windows; a level query
against a compacted Record with archived Facts matches the same Record before
compaction; and a backdated Fact reshapes the level series behind it. "What did I spend in March" is
answered with no sum Record in existence. The resource Record's ordinary Fact
chain remains the only actual quantity truth.

##### E1 — recurring gain/loss plans, occurrences, and projection

- [ ] Add versioned `eplan:@slug` handles which reference a reusable
  Karma Frequency and an `EconomyEventDraft` template: direction,
  magnitude, resource, source, tags, note, start/end, missed/inactive-gap
  behavior, and route (`suggest`, `draft`, or explicitly granted `apply`).
- [ ] Materialize one idempotent `eocc` for each plan-revision/Frequency-
  occurrence identity. Its lifecycle is `planned → due → applied`, with
  explicit `skipped` and `cancelled` alternatives. Editing one occurrence
  creates an override; editing future instances creates a new plan revision.
- [ ] Build a pure projection which folds the resource's actual Facts and the
  selected unresolved recurring occurrences. It returns separately labeled
  actual resource points, expected points, recurring gains, recurring losses,
  recurring net, contributing ids, and exclusions; it writes no projected Fact.
  This is the *declared-amount* fold — a plan states its magnitude up front. It
  is built on E0.4's projector rather than beside it, so a timeline can mix
  recurring plans with computed program rules and stay one timeline; a Record
  whose future is partly a fixed rent and partly a rule-derived draw must not
  need two incompatible forecasts to be understood.

**E1 exit:** a monthly loss, weekly loss, recurring gain, skipped occurrence,
one-occurrence override, downtime catch-up, DST, and plan revision produce
stable nonduplicated occurrences and identical projections under production
and DST.

##### E2 — first viable Economy sand

- [ ] Delete the unwired Finance placeholder and register a real Economy sand
  at `sand.economy`. Split it into
  `sand/economy/{mod,body,style,script}.rs` plus focused
  `app/{bridge,state,event_editor,recurrence,dashboard,profile,graph,activity}.js`;
  do not grow a monolithic HTML or script.
- [ ] **One timeline for one resource: past, present, and future in a single
  view.** Selecting a resource shows actual Facts behind it, its current
  quantity, and its projected future ahead — where the past mixes rule-caused and
  hand-entered movements without distinguishing them structurally, and the future
  mixes E0.4 program folds with classified promises. This is the view the pillar
  exists to produce, and it is one query over one classification, not four
  panels stitched together.
- [ ] **Capture is one line, and relating it is not the person's job.** "ice
  cream, `@cost`, -10" is the whole interaction: pick a resource, a magnitude
  with sign, a concept, and a date. The sand never asks anyone to add it to a
  month's expenses, to a category total, or to a sum Record — those are E0
  queries over the classification, and they update because the Fact exists.
  A form that requires choosing which total to affect has reintroduced the
  bookkeeping this design removed.
- [ ] Authoring recurring rules lives here too, not only in the Karma sand. A
  recurring cost is a Frequency plus a magnitude plus a classification, stated in
  Economy's own words; the sand creates the underlying Program and Frequency and
  shows what will be written and under whose grant. Complex or multi-node rules
  hand off to K12's Flow Plane rather than growing a second rule editor.
- [ ] Deliver human-friendly CRUD for individual and recurring movements. Forms
  expose resource, exact signed magnitude/unit, occurred date, concept
  classification, source, note, and recurrence — no direction control, since the
  sign carries it and a refund is a positively-signed cost. “Edit” and “Delete”
  explain the compensation/replacement that will be appended, and re-tagging is
  offered as its own operation because it appends no Fact.
- [ ] Deliver monthly actual gains, actual losses, actual net, expected
  recurring gains/losses/net, resource actual/expected graph, concept/source
  profiles, recurring-occurrence inbox, and event/Fact drill-down. Every bucket
  drills to its contributing Facts and every projected point to the rule or
  promise that produced it, so a total is never a number nobody can explain.
  Unclassified movements are shown, not hidden. All totals, buckets,
  comparisons, and graph points come from Economy Protein; JS only formats and
  renders them.
- [ ] Add a driven browser selftest covering snapshot, every Action round-trip,
  live invalidation across two open sands, stale edit recovery, correction,
  recurrence, filtering, graph-to-event drill-down, keyboard operation, and
  accessible text/table alternatives for every chart.

**E2 exit:** this is the K11 exit gate and the first viable workflow on the
completed Karma backend.

##### E3 — future Fiote ergonomics

- [ ] Add `create-economy-capture`, extract, review, apply-to-draft, reject, and
  redact Actions. Typed shorthand is parsed by the pure compiler; voice
  transcription and photo/OCR/model recognition enter as captured Signal
  observations with adapter/version/hash, not privileged Fact writes.
- [ ] Extract only what the narrow event needs: gain/loss direction, magnitude,
  unit/resource candidate, occurred time, source label/Record, tags, and note
  describing recognized purchases or income. Store alternatives, confidence,
  and source spans/bounding boxes; do not grow receipt accounting, line-item
  inventory, or import subsystems inside Economy.
- [ ] Fiote produces the same `EconomyEventDraft` as the sand. It may apply the
  draft only through a current narrow grant over resource, direction, unit,
  magnitude/rate, capture kinds, evidence threshold, time window, and expiry;
  otherwise it leaves an editable draft for the person.

**E3 exit:** typing, speaking, and photographing the same gain/loss can produce
the same canonical draft, while malformed input, ambiguous resource/unit,
revoked permission, or model disagreement remains reviewable and changes no
resource Fact.

#### Canonical Economy objects, types, and short references

These are domain objects, not new silos. Every mutable handle is a Record with a
schema-owned sidecar and expected revision. Operational projections are query
results, not Records merely to make a chart convenient.

> **E0 supersedes the `direction` and `tags` vocabulary used throughout the rest
> of this Economy specification.** These tables predate the classification model
> and still speak of a gain/loss direction field and a separate tag list. Read
> both as one thing: a movement's sign carries its direction, and its concept
> classification carries what it was. Where a table below says `direction_in`,
> read a sign filter; where it says `tags`/`tag_in`, read a concept filter over
> the DAG including descendants. The reasoning is in E0 and the substitution is
> mechanical, so these tables are left as written rather than rewritten ahead of
> the implementation that will settle their exact field names.

| Reference / object | Durable meaning and data effect |
| --- | --- |
| `record:@cash` — Resource | Existing Record whose exact quantity and unit are changed by gains/losses. Economy creates no parallel balance. |
| `edraft:r_...` — Event draft | Mutable revisioned gain/loss proposal: resource, direction, positive magnitude, occurred time, source, tags, note, and capture/occurrence cause. It has no quantity effect. |
| `eevent:r_...` — Applied event | Gain/loss event linked to its signed resource Fact, draft revision, actor, source/tags, correction chain, and provenance. |
| `eplan:@rent.monthly` — Recurring plan | Mutable handle plus immutable revision containing Frequency reference, event template, start/end, missed/inactive-gap behavior, and route ceiling. |
| `eocc:r_...` — Plan occurrence | One expected boundary, optional override, lifecycle state, and applied-event link. It changes no quantity until the event Action succeeds. |
| `ecap:r_...` — Capture | Typed text/audio/photo observation, hashes/retention, recognizer versions, alternative extracted event fields, evidence spans, and review state. |
| Economy projection | Cursor-bound response containing actual gain/loss/net, expected recurring gain/loss/net, resource points, exclusions, and contributing ids. It is recomputed and never used as actual history. |

Keep the canonical source timestamps distinct:

| Time | Meaning |
| --- | --- |
| `occurred_at` | When the individual gain/loss happened; default monthly grouping basis. |
| `applied_at` | When Lince appended the signed resource Fact. |
| `intended_at` | Recurring occurrence boundary; never proof that the gain/loss happened. |
| `captured_at` | When typed text, voice, or photo evidence entered Lince. |

The default personal view uses `occurred_at` in the person's selected timezone
and half-open civil month `[month_start, next_month_start)`. The Protein request
must carry that timezone and resolved UTC boundaries so “July” is reproducible.
There is no alternate accounting basis. If an applied event's `occurred_at` is
wrong, correcting the event compensates and replaces it.

#### Gain/loss examples and correction behavior

An individual loss and gain each append exactly one resource Fact:

    loss 68.40 BRL on record:@cash
      occurred 2026-07-22T18:14:00-03:00
      source merchant:@market tags #food #household

    gain 5000.00 BRL on record:@cash
      occurred 2026-07-31 source merchant:@employer tags #salary

The first produces `-68.40 BRL`; the second produces `+5000.00 BRL`. If the
loss was really `63.40 BRL`, editing it appends `+68.40 BRL` compensation and
then `-63.40 BRL` replacement Facts linked to the original event. Monthly totals
use the net event revisions while the drill-down preserves the full Fact chain.

The complete Economy feature set for now is therefore:

- individual gain/loss CRUD over a selected resource Record;
- recurring gain/loss plans and editable individual occurrences;
- monthly actual gain, loss, and net totals;
- recurring expected gain, loss, and net totals;
- actual resource history plus expected recurring trend graph;
- profiles/drill-down grouped by source, tags, direction, recurrence, capture
  origin, and cause; and
- future typed/voice/photo Fiote capture into the same editable event draft.

#### Economy Protein contract

After E0, add one `source:"economy"` discriminated Protein union rather than
teaching the sand to join event sidecars and Facts. It is a gain/loss projection
over the ordinary Ledger, not a second resource truth. Unsupported predicates/
includes fail with a typed code; visibility is applied before sums/grouping.

| `object_kind` | Important predicates/includes |
| --- | --- |
| `event` / `draft` | `resource_eq/in`, `direction_in`, `occurred_window`, `source_eq/in`, `tag_in`, `capture_origin_in`, `plan_eq`, `text`; include Fact, correction chain, tags/source, occurrence/capture cause, capabilities |
| `plan` / `occurrence` | `plan_eq`, `resource_eq`, `direction_in`, `frequency_eq`, `intended_window`, `status_in`; include template, overrides, next boundary, applied event, capabilities |
| `overview` / `series` | exact civil window/timezone, resource set, unit, actual/expected inclusion, granularity; include gain/loss/net totals, resource points, profiles, exclusions, and drill-down ids |
| `capture` | origin, review state, captured window; include input metadata, field alternatives/confidence/spans, resulting draft, and capabilities |

The monthly dashboard request is explicit and reproducible:

    {
      "source": "economy",
      "where": [
        { "object_kind_eq": "overview" },
        { "window": {
          "start": "2026-07-01T03:00:00.000Z",
          "end": "2026-08-01T03:00:00.000Z",
          "civil": "2026-07",
          "timezone": "America/Sao_Paulo"
        }},
        { "resource_in": ["r_cash..."] },
        { "unit_eq": "c_brl..." }
      ],
      "include": {
        "summary": true,
        "series": { "granularity": "day", "actual": true, "expected": true },
        "profile": {
          "by": ["source", "tag", "direction", "capture_origin",
                 "recurrence"]
        },
        "drilldown_ids": true
      }
    }

The response supplies exact decimal strings. It returns actual gain, actual
loss, actual net, expected recurring gain/loss/net, and actual/expected resource
points separately. An empty or partially visible selection reports exclusions
rather than presenting hidden data as a confident complete total. The engine
buckets civil boundaries, and every total/profile/point links back to its
events, occurrences, and Facts.

#### Economy Actions and wire examples

All Actions derive the principal on the server, take `request_id`, and use
`expected_revision` for mutable handles/drafts. Suggested names are:

| Typed Action | Behavior |
| --- | --- |
| `create-economy-event-draft` / `revise-economy-event-draft` | Validate/canonicalize resource, direction, positive magnitude, time, source, tags, note, and cause into an inert draft. |
| `apply-economy-event` | Revalidate resource/unit/visibility/capability and atomically append the canonical signed Fact. |
| `correct-economy-event` / `void-economy-event` | Compensate the current Fact and optionally append a replacement; require a reason and expected current event revision. |
| `create-economy-plan` / `revise-economy-plan` / `activate-economy-plan-revision` | Bind a proven Frequency and event template; later revisions do not rewrite prior occurrences. |
| `override-economy-occurrence` / `resolve-economy-occurrence` | Edit one occurrence or apply/skip/cancel it. Applying uses the ordinary event validator. |
| `create-economy-capture` / `apply-economy-capture` / `reject-economy-capture` | Preserve typed/voice/photo field alternatives, then apply selected fields to an event draft; does not imply applying the event. |

A simple sand/Fiote-neutral draft Action is:

    {
      "action": "create-economy-event-draft",
      "request_id": "019c...",
      "resource_uid": "r_cash...",
      "direction": "loss",
      "magnitude": { "decimal": "68.40", "unit_uid": "c_brl..." },
      "occurred_at": "2026-07-22T18:14:00.000-03:00",
      "source": { "record_uid": "r_market..." },
      "tags": ["r_food...", "r_household..."],
      "capture_origin": "typed",
      "note": "fruit and vegetables"
    }

The engine validates that the magnitude is positive and matches the resource's
unit, derives `-68.40 BRL`, and returns a preview without changing quantity.
`apply-economy-event` references the draft revision and appends that one Fact. A
voice/photo/model adapter produces the same payload plus capture evidence and
confidence; it receives no alternate Action capable of bypassing review.

#### Economy shorthand and recurring DSL

Use readable words rather than accounting initials. This deterministic
shorthand is a capture language, not an executable shell:

    lose 68.40 BRL on record:@cash from merchant:@market
      on 2026-07-22T18:14-03:00 #household

    gain 5000.00 BRL on record:@cash from merchant:@employer
      on 2026-07-31 #work

`lose` and `gain` compile to the typed event draft. `spend`/`earn` may be input
sugar. `@spenditure`,
`@expenditure`, `@expense`, or a local word may be Lingua aliases/equivalences
for recognition, but the canonical direction is `loss`; `@income` maps to
`gain`. Missing resource, unit, magnitude, or ambiguous date yields a draft
question/error, never a guessed Fact.

A recurring plan reuses the Karma schedule language and adds one typed
Economy template rather than creating a second cron engine:

    frequency economy.rent.monthly {
      every calendar day 5 at 09:00 timezone America/Sao_Paulo
      timer { resolution 1s max_lateness 5m coalesce_window 0ms }
      missed latest
      inactive_gap skip_to_next_anchor
      rephase preserve_anchor
    }

    economy plan rent.monthly revision 1 {
      owner person:@ana
      on freq:@economy.rent.monthly
      expect lose 1800.00 BRL on record:@cash
      source merchant:@landlord tags #housing #rent
      route draft
    }

At the boundary the scheduler creates `eocc:@rent.monthly/<tick>` and a
forecast contribution. `route draft` may prepare a reviewed event. `route act`
must name a narrow Economy apply grant; without it, time never changes the
resource quantity by itself.

#### Economy sand information architecture

The default view shows a civil month selector, resource selector, unit/filter
scope, and exact cards for **gains**, **losses**, **net**, and **expected
recurring net**. Actual and expected values are never merged. Quick-add offers
Gain, Loss, and Recurring; the same editor handles create and correction.

The primary graph shows the resource's actual quantity history and separately
styled expected path from recurring occurrences. A companion flow graph shows
gain/loss/net by day, week, or month. Selecting a point opens its events/Facts.
A semantic table with identical points is mandatory for keyboard and screen-
reader use; color is not the only actual/expected distinction.

The profile groups by source, Record tags/Lingua concepts, direction,
recurring/individual, capture origin, and cause. Each group shows exact total,
event count, and its event ids. Clicking it issues a narrower server query; the
browser never derives a total from a partial local list.

The activity list visibly separates applied events, drafts/captures, and due
occurrences. The event drawer shows resource, direction, magnitude/unit, time,
source, tags, note, correction chain, capture/occurrence cause, and Facts. The
recurrence view supports next boundary, edit-one/edit-future, pause/end/skip,
and projected impact. There are no account, budget, debt, investment, import,
reconciliation, or tax pages in this scope.

#### Fiote and recognition control boundary

Fiote is an actor-neutral client of the same capture/draft contract. Its useful
future jobs are: parse “I spent 42 reais on lunch,” transcribe a voice note,
read the total/source/items description from a photo, resolve the target
resource and known source/tag slugs, and prepare or correct a matching recurring
occurrence. Each job returns gain/loss event field candidates, not an opaque
final mutation or a broader accounting object.

The canonical capture state is
`captured → extracted → needs_review|ready → applied|rejected`, with a separate
Fact-application state on the resulting draft. Every inferred field records its
recognizer/version, input hash, candidate value, `conf`, source span/bounding box, and
alternatives. A person can edit any field; that feedback may become eligible
learning evidence later but never rewrites the captured model output. Raw voice
or photos can be discarded after review according to retention while keeping a
hash and selected structured evidence.

Fiote may rank existing resource/source/tag Records, but it may not silently
create or select an ambiguous one. Its delegation is deliberately narrow: exact
principal, Program/revision, resource, direction, unit, per-event and period
magnitude, capture kinds, minimum evidence/confidence, time window, rate, and
expiry. A model score is not the grant. Revoking the grant before apply returns
the draft to review without losing it.

#### Economy-specific proof gates

- [ ] Gain always derives a positive delta and loss a negative delta; magnitude
  is positive/exact and must match the resource unit. No incompatible units are
  summed and no client sign can invert the declared direction.
- [ ] Editing/voiding an applied event preserves its original Fact and produces
  the correct resource quantity and monthly gain/loss/net under replay, sync
  duplication, stale requests, and crash at every transaction boundary.
- [ ] Recurring boundaries create expected occurrences, never actual Facts.
  Edit-one/edit-future, pause/end/skip, downtime, and DST neither lose nor
  duplicate an occurrence/event.
- [ ] Monthly overview uses explicit civil timezone/bounds; actual and expected
  remain separately queryable. Every total, profile group, and graph point
  drills to exactly its visible event/occurrence/Fact set.
- [ ] Manual form, deterministic shorthand, and future Fiote text/voice/photo
  converge on the same event-draft/apply validator. Ambiguous, low-confidence,
  malformed, or revoked cases change no resource Fact.
- [ ] Two open Economy sands converge live without clobbering a dirty draft;
  charts have equivalent tables, all interactions are keyboard accessible, and
  sensitive hidden entries cannot be inferred through grouping, prior-period
  comparison, projections, or small-cohort differencing.

### Low-level crate and module shape

Use one destination subsystem rather than continuing to expand the current
`karma.rs`, `signals.rs`, `senses.rs`, `effects.rs`, and `imagination.rs` into
parallel engines. During migration they may call the new modules as adapters;
after their behavior is covered, delete the duplicate paths.

| Crate | Intended modules and ownership |
| --- | --- |
| `nucleus` | `karma/{ids,value,ast,dsl,schedule,trace,policy,proof,model,workflow,simulation}.rs`: pure types, parsing, math, graph evaluation contracts, no I/O. |
| `store` | `karma/{programs,schedules,occurrences,runs,models,candidates,grants,trust_scopes,workflows,intents}.rs`: typed repositories and transaction helpers; each table is owned by a Rust row/input type. |
| `engine` | `karma/{supervisor,sequencer,scheduler,evaluator,learning,policy,proof,workflow,effect_worker,simulation}.rs`: orchestration and the only bridge between pure kernel, Store, and runtime ports. |
| `protein` | `karma.rs`: union source projection, predicates/includes, visibility/taint-before-aggregate, capability/blocking projection. |
| `transport` | Reuse the multiplexed protocol; add only typed Karma request/response/error payloads, never a second socket or private sand API. |
| `lince` | Start one Karma supervisor per writable Cell and own graceful shutdown; it contains no scheduling or rule semantics. |
| `web` | `sand/karma/{mod,body,style,script}.rs` plus `app/{bridge,state,library,builder,why,learn,imagine,authority,queue,health}.js`; host state stores layout only. |

Economy is a cross-pillar reference module, not a child hidden inside the
Karma evaluator. Keep manual gain/loss entry usable while Karma is
paused, and place code by responsibility:

| Crate | Economy module responsibility |
| --- | --- |
| `nucleus` | `economy/{ids,event,plan,capture,projection}.rs`: exact gain/loss types, sign/unit invariants, draft canonicalization, pure projection inputs/outputs. |
| `store` | `economy/{events,plans,occurrences,captures}.rs`: schema-owned row/input types and atomic event compensation/Fact-link helpers. |
| `engine` | `economy/{actions,recurrence,capture}.rs`: principal/capability checks, revision/idempotency, Fact append, Karma occurrence bridge, causal invalidation. |
| `protein` | `economy.rs`: typed union, visibility-before-event aggregation, exact civil buckets, profiles, projections, drill-down ids, capabilities/blockers. |
| `web` | `sand/economy/...` shape from E2. State holds filters/draft UI only; no durable event, recurrence cursor, total, or projection truth lives in JavaScript. |

The engine supervisor owns an injected `RuntimePorts` bundle: wall/virtual
clock, sleeper/wakeup, deterministic entropy, process, HTTP, filesystem,
device/UI controllers, and secret resolution. Pure nodes never receive that
bundle. Production and simulation differ by port implementation, not by
business logic.

The supervisor owns a small fixed set of long-lived tasks, not tasks proportional
to Program/Frequency count:

| Task | Responsibility |
| --- | --- |
| Deadline director | Own the durable registration mirror, tickless wheel, dynamic sparse/dense lane plan, and one-shot timer set; submit only due occurrence batches |
| Occurrence sequencer | Persist/deduplicate/order occurrences, freeze epoch, commit run order and reaction closure |
| Pure evaluator pool | Prefetch immutable context and evaluate graphs in parallel where safe; return deterministic results to sequencer |
| Learning worker | Consume completed eligible cursors behind reaction priority; commit checkpoints in cursor order |
| Effect worker pool | Lease intents by adapter/capability, recheck policy, dispatch, store attempts/receipts |
| Connector supervisor | Own active Signal adapter lifecycles and push captured observations into the occurrence path |
| Maintenance worker | Coarse repair/checkpoint/retention/replay audit; scheduled through the same deadline director |

A deadline lane is an in-memory timer/index partition, not a Rust thread or
Tokio task. The fixed deadline director may await many lane timer futures through
one ready set (or one `epoll`/`timerfd` adapter); only a lane whose one-shot timer
became ready is returned. Dense lanes may receive a dedicated runtime thread
only when an explicit platform/resource grant and measured load justify it.

Channels are bounded and carry stable uids/small commands, not giant snapshots.
On receipt a worker reloads authoritative state or uses the frozen immutable
epoch/context. Backpressure parks durable work; it never drops an occurrence
because an in-memory channel is full.

Active compiled definitions live in an immutable `Arc<CompiledEpoch>` containing
the revision/parameter/model/grant/Trust hashes and dependency index. Activation
or tuning commits Store state first, builds the next epoch, then publishes it
through a `tokio::sync::watch<Arc<CompiledEpoch>>` and enqueues its effective
occurrence. Runs clone one `Arc`, so no mutex is held across evaluation and no
mid-run edit is observable.

SQLite constraints, not process memory, guarantee correctness. Use unique keys
for source occurrence identity, `(program_revision, occurrence, correlation)`
run identity, schedule batch identity, Action request replay, candidate dedupe,
and intent idempotency. Never hold a database transaction while awaiting a
person, network, process, model, or device; commit intent first and reconcile
the receipt in a later transaction/occurrence.

### Legacy condition → consequence vocabulary mapping

The old names map to short, readable interface words. Three-to-eight character
terms are preferred over one-letter codes: a saved program should still be
understandable six months later. These words are DSL sugar over typed graph
nodes and Actions; the wire never executes text directly.

| Diary concept | Canonical type / short reference | DSL/interface | Durable data effect |
| --- | --- | --- | --- |
| Karma | Program, `prog:@slug` | `program`, `when`, `act` | Creates a Program Record and immutable revision; activation selects one revision. |
| Condition | Typed expression/recognizer, `node:@prog#rev/name` | `let`, `when`, `sense` | Pure by default; trace records inputs/result. A materialized value is an explicit Fact. |
| Operator | Gate node | `== != < <= > >=`, `and/or/not`, `crosses`, `enters`, `leaves` | Changes no domain data; records transition state when stateful. |
| Consequence | Candidate or intent | `recommend`, `draft`, `ask`, `act` | Creates an inert candidate/decision or an authorized intent; only the typed Action changes domain data. |
| Delivery | Occurrence + Run | `on fact`, `on every`, `on signal`, `run` | Appends occurrence/run/trace and any resulting candidates, intents, receipts, or Facts. |
| Frequency | Schedule, `freq:@slug` | `every 1d`, `at 08:00`, `after 250ms` | Stores schedule/anchor/cursor. A due boundary creates an occurrence; it does not edit a Record timestamp. |
| Sum | Aggregate/feature node | `sum`, `count`, `avg`, `rate`, `window` | Pure unless explicitly `emit`ted; the exact input Fact set stays explainable. |
| Command/query | Signal when reading; effect when acting | `input ... = signal`, `do command`, `do http` | Capture creates observation Facts; execution creates intent, attempts, receipt, then provenance Fact. |
| Karma category | Program tags/scope | `tags`, `purpose`, `scope` | Metadata revision/annotation only; tags do not confer permission. |
| Calendar/Graph/Karma Orchestra | Karma Flow Plane + Imagination + optimizer | `sim`, `project`, `solve`, graph editor | Creates simulation/analysis runs and candidates, never real-world state wholesale. |
| Ask/Agent/Tinkerer | Route policy | `observe`, `suggest`, `draft`, `ask`, `act` | Selects trace-only, recommendation, decision, or authorized intent. |
| Senses | Recognizer, `sense:@slug` | `sense name = ...` | Emits evidence-backed candidates; cannot write or contact by itself. |
| Learning/growth | Model, `model:@slug` | `learn ... using ...`, `predict` | Appends model checkpoint/update evidence; cannot mutate a Program directly. |
| Learned-rule promotion | Program revision candidate | `revise from template`, then `ask` or delegated `act` | Creates a proven/shadowed revision candidate; activation is a later occurrence. |
| Rule changing rule/Frequency | Meta-control candidate/intent | `tune`, `revise`, `pause`, `resume` | Appends parameter/revision/activation data effective only for later occurrences. |
| Recommendation | Candidate, `cand:r_...` | `recommend "..."` | Creates/updates one lifecycle-managed recommendation with evidence and preview. |
| Attention/whisper | Decision/delivery | `ask`, `whisper via ...` | Decision is durable; channel attempts are receipts. Delivery never answers it. |
| Imagination | Simulation run, `sim:r_...` | `project`, `branch`, `assert`, `sim` | Writes isolated run/trace/bookmarks only; applying uses separately reviewed Actions. |
| Workflow | Workflow instance, `flow:@slug` | `step`, `parallel`, `wait`, `retry`, `compensate` | Persists node position/waits/intents; domain changes still use Actions. |
| Optimization | Objective/solve run, `obj:@slug` | `solve`, `require`, `minimize`, `maximize` | Creates ranked plans and explanations; applying a plan is separate. |
| Authority | Delegation grant, `grant:@slug` | `require grant`, `budget` | Grant/narrow/revoke are signed Actions; no program can enlarge its own grant. |
| Automation Trust | Local counterparty scope, `trust:@slug` | `trust`, `allow/deny`, `any/all`, `ceiling` | Adds a concept/stage/person/Organ/proximity gate; never changes probability, visibility, or another Person's authority. |

### The Karma contract

- [ ] Karma reads Cell state through the same record/Ledger semantics
  that Protein exposes and changes state only through typed Actions. An
  automatic path is never a privileged write path.
- [x] Existing rules, signals, frequencies, match rules, and decisions are
  records; quantity is their activation knob and their changes remain visible
  in the Ledger.
- [ ] Make an **Karma program** the ergonomic unit a person manages: a
  named, versioned record whose linked graph declares triggers, Protein context,
  computations, policy, and outcomes. Rules, senses, schedules, recommendations,
  and workflows are program node kinds rather than separate automation silos.
- [ ] Every program declares an owner, purpose, data scope, authority ceiling,
  budgets, schedule/event triggers, failure policy, and enabled revision. No
  defaults may silently widen visibility or authority.
- [ ] Give each evaluation a durable `karma_run` identity with program
  revision, triggering occurrence, input cursor, logical clock/seed, node
  trace, candidates, policy decisions, intents, Actions/effects, resource cost,
  and final status. “Why did this happen?” and “what will retry?” must be
  ordinary reads, not logs an operator has to find on disk.
- [ ] Derive every effect idempotency key from the program revision, triggering
  occurrence, effect node, and correlation key. Retries may finish an intended
  action but never repeat it; changed definitions produce a new revision and a
  new proof boundary.
- [ ] Separate evaluation from effects. A run first computes a stable proposal;
  policy then permits, stages, asks, or rejects it; effect workers execute
  durable intents with leases, retry/backoff, timeout, and dead-letter state.
  Partial external failure never rolls back or hides committed Ledger Facts.
- [ ] Define deterministic agenda semantics for simultaneous rules: dependency
  order, explicit priority only where necessary, stable tie-breaking, atomic
  Action boundaries, and a recorded explanation of conflicts. An ordinary graph
  rejects combinational cycles; iteration is legal only inside an explicit
  bounded/convergent node or across a state/delay boundary.
- [ ] The effective program scope is the intersection of its declared input
  Protein, the owner's visibility at the run cursor, purpose/declassification
  policy, and the triggering principal's grant. Hidden data must not leak
  through features, aggregates, model parameters, explanations, or effects.
- [ ] Treat programs as replaceable definitions, never uneditable law. A
  revision can be cloned, changed, proven, simulated, shadowed, activated,
  rolled back by reselecting an earlier revision, paused, and retired. Existing
  run evidence is not rewritten.
- [ ] Programs may manage other programs or interface controllers only through
  separately granted typed capabilities. They may pause or tune within a grant;
  they may never grant themselves new data, authority, secrets, or budget.

### Canonical durable model

Everything durable in Karma remains a Record plus schema-owned typed
sidecar state, and every semantic transition appends a Fact. This does not mean
forcing an execution trace or sample into a record body. It means each object
has ordinary uid, origin, ownership, visibility, links, activation, and
provenance behavior.

| Object | Durable meaning |
| --- | --- |
| **Program** | Mutable handle people organize and activate. It names the owner, purpose, active revision, tags, and default operational policy. Its quantity is the universal on/off knob. |
| **Program revision** | Immutable, content-hashed typed graph plus declared inputs, outputs, parameters, objective, policy requirements, and failure/concurrency behavior. Slugs are resolved to uids when published. |
| **Frequency / Frequency revision** | A reusable mutable schedule handle plus immutable cadence/timer/catch-up policy. Quantity and active revision determine eligibility; an operational cursor/deadline exists only while at least one active Program, Signal poll, or workflow consumer references it. |
| **Trigger occurrence** | One durable reason work exists: Fact cursor, schedule boundary, signal sample, manual run, workflow wake-up, sync arrival, or retry. It carries logical time and deduplication identity. |
| **Run** | One evaluation of one revision against one frozen visible input cursor. It owns the node trace, proposals, policy results, resource use, and terminal state. |
| **Evidence set** | The exact Facts/observations and inclusion/exclusion reasons supporting a feature, pattern update, forecast, or recommendation. |
| **Model specification/checkpoint** | Versioned feature schema, deterministic algorithm and parameters, training cursor, learned state, validation metrics, drift state, and implementation hash. |
| **Candidate** | Inert proposed conclusion, plan, program revision, recommendation, decision, Action, or Transfer change. A candidate has no authority. |
| **Delegation grant** | A principal's signed, revocable capability envelope: program/revision, Action kinds, targets, recipients, value/rate limits, time/place/context, evidence requirements, expiry, and escalation rules. |
| **Action intent** | Authorized durable request awaiting an internal or external executor. It freezes the exact typed Action/effect, policy proof, idempotency key, deadline, and compensation metadata. |
| **Attempt/receipt** | Each lease, dispatch, response, timeout, retry, cancellation, external identifier, captured output hash, and eventual result. A receipt is evidence, not proof that an unobservable real-world claim is true. |
| **Workflow instance** | Durable node position, correlation key, child runs, waits, approvals, compensation stack, and cancellation state for long-running behavior. |

- [ ] Give every object a stable uid under the existing identity families and
  expose it through Protein. Karma objects that are Records retain
  `r_...`; Facts retain `f_...`; the typed object kind—not a new incompatible
  uid alphabet—distinguishes program/revision/run/model/candidate/grant/intent/
  receipt/workflow state.
- [ ] Store the complete revision and grant used by a run by hash/reference.
  Later edits or revocation never make an old explanation describe new policy.
- [ ] Separate definition status
  `draft → proven → shadow → active → superseded/retired` from run status
  `queued → evaluating → staged/waiting → executing → completed/failed/`
  `cancelled/dead-letter`. “Faulted” may automatically pause new occurrences
  without pretending the program's quantity was manually changed.
- [ ] Garbage collection may compact traces and model checkpoints only behind
  hash anchors and configured retention. Evidence needed for an active grant,
  unsettled Transfer, open decision, reproducible run, or audit hold stays hot.

### Interface types, references, and slugs

There are three identifier layers:

1. **UID** is canonical on the wire, in Facts, signatures, links, and stored
   graph revisions: `r_...`, `f_...`, `p_...`, `c_...`, and so on.
2. **Typed reference** is compact authoring syntax: `prog:@apple.restock` or
   `model:@apple.need`. Publishing resolves it to a uid and stores both uid and
   displayed slug. A later rename cannot change meaning.
3. **Bare `@slug`** is allowed only when the expected port type makes the kind
   unambiguous. Ambiguity is a compile error, never a “best match.”

Recommended short kinds:

| Short kind | Meaning | Example |
| --- | --- | --- |
| `prog` | Karma Program | `prog:@apple.restock` |
| `rev` | Immutable Program revision | `rev:@apple.restock#4` |
| `node` | Stable node inside a revision | `node:@apple.restock#4/shortage` |
| `sig` | Signal/source | `sig:@kitchen.scale` |
| `freq` | Reusable schedule | `freq:@daily` |
| `sense` | Pure recognizer | `sense:@pantry.shortage` |
| `view` | Saved Protein | `view:@nearby.apple.offers` |
| `model` | Learner/model specification | `model:@apple.need` |
| `obj` | Objective/optimizer specification | `obj:@week.balance` |
| `flow` | Workflow/subprogram | `flow:@apple.purchase` |
| `grant` | Delegation grant | `grant:@apple.autobuy` |
| `trust` | Local automation Trust scope | `trust:@apple.known_sellers` |
| `run` | Durable program run | `run:r_01...` |
| `cand` | Candidate/recommendation | `cand:r_01...` |
| `dec` | Decision | `dec:r_01...` |
| `intent` | Authorized intent | `intent:r_01...` |
| `receipt` | Effect attempt/result | `receipt:r_01...` |
| `sim` | Simulation/DST run | `sim:r_01...` |

User slugs remain `dot.case`, namespaced by kind rather than by awkward global
uniqueness. `prog:@daily` and `freq:@daily` may coexist. Program-local node and
parameter ids are lower `snake_case` (`next_window`, `min_confidence`) because
they appear as stable DSL fields and diff keys. User-facing heads remain free
text and may change without changing references.

The graph's value types are intentionally small and exact:

| DSL type | Wire/storage meaning | Examples |
| --- | --- | --- |
| `bool` | `true`/`false`; never numeric truthiness | `stock_low: bool` |
| `i64` | Signed integer | counts, sequence, retry number |
| `dec<S>` | Fixed-scale decimal with explicit rounding/overflow | `dec<4>` likelihood weights |
| `prob` | Fixed-point `[0,1]` probability | `0.72p` |
| `conf` | Fixed-point `[0,1]` evidence confidence | `0.65c` |
| `text` | Canonical Unicode string; never code or an implicit reference | labels, messages, exact source fields |
| `qty<U>` | Decimal quantity with Lingua unit/dimension | `2.5kg`, `3L` |
| `money<C>` | Decimal money in an explicit currency | `12.50 BRL` |
| `dur` | Signed integer milliseconds | `1ms`, `250ms`, `3d` |
| `at` | Internal signed UTC milliseconds; canonical wire is RFC3339 with `.sss` | `2026-07-21T08:00:00.125Z` |
| `civil` | Local calendar value plus timezone/calendar revision | `08:00 America/Sao_Paulo` |
| `win<T>` | Inclusive/exclusive typed interval | `[now, now + 7d)` |
| `ref<K>` | UID reference constrained to kind `K` | `ref<record>`, `ref<person>` |
| `list<T>` / `set<T>` / `map<K,V>` | Deterministically ordered collection | `set<ref<person>>` |
| `datum<T>` | `value`, `missing`, `stale`, `denied`, or `invalid` | a stale scale reading is not `0kg` |
| `estimate<T>` | value/range, uncertainty, support, model revision | forecast quantity/window |
| `candidate<A>` | Inert preview of Action/plan type `A` | `candidate<transfer.draft_local>` |
| `intent<A>` | Authorized durable execution request | `intent<record.set_quantity>` |

`prob` answers “how likely”; `conf` answers “how supported”; neither converts
implicitly to `bool`. A gate must compare both deliberately. Units never
coerce across dimensions, money never becomes a plain quantity, and missing/
stale/denied data must be handled before arithmetic.

#### Millisecond time contract

- [ ] Use signed integer UTC milliseconds as the first canonical schedule/run
  precision. DSL duration literals accept `ms`, and RFC3339 timestamps preserve
  exactly three fractional digits on the canonical wire. A monotonically
  increasing Cell cursor and occurrence uid break ties at the same millisecond.
- [ ] Millisecond precision is a semantic guarantee, not a promise that a
  general-purpose operating system wakes in one millisecond. A schedule records
  `intended_at`, `eligible_at`, `observed_at`, and lateness; a late worker runs
  the exact missed-occurrence policy rather than changing the intended time.
- [ ] A reusable schedule stores `anchor`, `interval/calendar rule`, `timezone`,
  `required_resolution`, `max_lateness`, `coalesce_window`, `missed`, `jitter`,
  `last_intended_at`, and `next_intended_at`. Calendar schedules and fixed
  elapsed durations are different types:
  `every calendar 1d at 08:00` is not `every elapsed 24h` across DST.
- [ ] Reserve finer-than-millisecond source timestamps as opaque source data if
  a device provides them; they may order samples inside an adapter, but no core
  rule depends on platform-specific nanosecond scheduling until the canonical
  type is deliberately upgraded.

The first pure schedule slice implements elapsed cadence before calendar
resolution. Its Rust contract is `nucleus::karma::schedule`:

- `ElapsedSchedule` is valid only with `interval_ms >= 1`, an exact anchor, and
  a `TimerPolicy` where resolution is at least `1ms` and coalescing never
  exceeds maximum lateness. It calculates boundaries by integer division from
  the anchor; it never adds an interval to observed wake time. The anchor is
  the cadence origin, not an implicit activation occurrence: activation at the
  anchor first schedules `anchor + interval`, and activation before the anchor
  schedules the anchor.
- `ScheduleCursor` stores the last consumed intended boundary and exact next
  boundary. Construction verifies both lie on the schedule lattice. A cursor
  cannot silently adopt a wall-clock instant or a boundary from another
  revision.
- `OccurrenceRange(first, interval, count)` is the reconstructible semantic
  batch. `ScheduleAdvance` records the full due range, emitted individual or
  coalesced range, explicit skipped range, late count/maximum lateness, next
  cursor, and whether the schedule must pause. A count is never zero and range
  arithmetic is checked for timestamp/`u64` overflow.
- Missed policy is exact. `skip` emits only the newest due boundary when that
  boundary remains inside `max_lateness`, recording every older boundary as
  skipped; if even the newest is late, all are skipped. `coalesce` emits one
  occurrence carrying the complete range. `replay(max=N)` emits the oldest
  `min(N,count)` ticks in order and records the remaining suffix as overflow/
  skipped before advancing. `pause_on_lag` advances nothing when any due tick
  exceeds maximum lateness; an operator can therefore inspect the unchanged
  cursor before choosing recovery.
- `RationalRate` stores reduced integer numerator/denominator. Thus `3ms` is
  exactly `1000/3` semantic ticks per second and `5h` is exactly `1/18000`;
  admission never uses a rounded float or an arbitrary fast/slow threshold.
- Rephase returns a new immutable schedule plus cursor. `preserve_anchor`
  selects the first new-lattice boundary strictly after the change;
  `from_last_intended` anchors the new interval at the last consumed boundary;
  `from_change` anchors at the change and first fires one interval later; and
  `immediate_if_overdue` preserves the already-due intended boundary as the new
  anchor/next boundary, otherwise behaving as preserve-anchor. Rephase never
  reinterprets an already consumed occurrence.

Calendar cadence is a separate `nucleus::karma::calendar` contract. A
`CalendarSchedule` stores a local civil anchor, `daily-at`/`weekly-at`/
`monthly-at` rule, validated IANA timezone id, exact tzdb version and content
hash, DST gap/fold policies, timer/missed/inactive/overload policies, and no
host-derived state. Daily and weekly rules retain local clock time across UTC
offset changes. Monthly rules explicitly `skip`, `clamp-to-last-day`, or
`pause` when (for example) day 31 does not exist; they never roll silently into
the next month.

`TimeZoneProvider` is a pure injected lookup boundary. It advertises one
`TzdbRevision` and resolves `(timezone, CivilDateTime)` to exactly one instant,
a gap with its first valid instant after the gap, or a fold with two increasing
instants. Resolution fails closed if the provider's version/hash differs from
the schedule. The host timezone, current tzdb package, locale, and wall clock
are never implicit inputs; production loads a content-addressed provider while
replay loads the revision named by its capsule.

The production provider consumes canonical `karma.tzdb-artifact.v1` JSON. Its
top level contains a declared release version and a sorted timezone map; each
zone contains contiguous half-open UTC offset segments covering the complete
representable timeline. The first segment has no lower bound and the final
segment has no upper bound. Loading is bounded to 64 MiB, 4,096 zones, and
100,000 segments per zone, rejects offsets beyond 24 hours, UTC gaps/overlaps,
noncanonical bytes, and any artifact that maps one local instant to more than
two UTC instants. Its revision digest is the domain-separated hash of the
canonical semantic artifact, not a filename or host tzdb version string.
Engine checks file size before allocation, detects size change during the read,
and accepts the provider only when version and digest equal the pinned
`TzdbRevision`.

Local resolution subtracts each applicable fixed offset from the civil
millisecond value and validates the candidate against that segment's UTC
interval. Zero, one, and two candidates become a typed gap, unique instant, or
fold. Demand attestation derives a conservative rule-specific minimum: exact
daily/weekly lower spacing or a 28-day-per-month lower bound, minus the zone's
complete offset spread; `fold both` also admits the shortest backward-transition
width. This can over-reserve but never hides a faster possible occurrence. A
release/build tool may translate IANA source/TZif into this format, but runtime
semantics depend only on the frozen artifact bytes and are therefore replayable.

`GapPolicy` is `skip`, `shift-forward`, or `pause`; `FoldPolicy` is `first`,
`second`, `both`, or `pause`. Every generated `CalendarBoundary` retains the
requested local time, actual UTC `intended_at`, and resolution kind. With
`both`, the two fold instants are separate stable boundaries for the same civil
time; with `shift-forward`, the trace exposes that the UTC instant was shifted.
Skipped gaps and invalid month dates remain explicit generation outcomes, not
missing history. A deterministic search budget rejects a broken/malicious
provider that reports an unbounded run of gaps.

The recurrence iterator is arithmetic from the civil anchor: daily rules use
whole-date distance, weekly rules use an anchor Monday plus a non-empty ordered
weekday set, and monthly rules use checked absolute month indexes. It never
finds the next time by adding UTC `24h`, never asks the provider about dates
outside the selected cadence, and never consults every schedule on a fast
timer. Once resolved, its exact next UTC boundary enters the same tickless
deadline fabric as an elapsed Frequency.

**Implemented elapsed schedule foundation:** the validated schedule/cursor/
range/policy/rational-rate types and pure arithmetic above now live in
`nucleus::karma::schedule`. Golden and boundary tests cover the exact
`3ms + 5h` behavior, every missed/inactive/rephase policy, wake coalescing
windows, anchor preservation, Serde rejection, and timestamp/range overflow.
This is the semantic layer only; it creates no Tokio timer, lane, SQL row, or
polling loop.

**Implemented calendar schedule foundation:** the corresponding validated
calendar types and recurrence arithmetic now live in `nucleus::karma::calendar`.
Every boundary carries requested civil time, exact UTC intended time, and its
unique/gap-shift/fold identity; skipped and paused discontinuities are typed.
The injected provider is version/hash-pinned and previous-boundary validation
fails closed. `timezone_artifact` now supplies the bounded canonical production
provider and Engine's exact-revision file loader. Demand calibration/lane
planning and durable cursors/director are implemented in K2.3 below.

#### Tickless shrink-to-fit deadline fabric

Do **not** turn the shortest active Frequency into a global polling interval.
Do **not** generate Rust code, busy-loop, or spawn one Tokio task per Frequency.
Every timed object instead owns one calculated `next_due_at`; the runtime arms
one-shot timers and receives only the registrations that became due. Firing a
`3ms` Frequency calculates and registers its next `3ms` boundary. It does not
ask whether the `5h`, daily, or monthly Frequencies are ready.

Cadence and wake precision are separate. `every elapsed 5h` describes the
sequence of intended boundaries; `max_lateness 10ms` describes the requested
wake service. A five-hour schedule may demand millisecond accuracy at its one
boundary, while a one-second sampler may explicitly tolerate 100ms coalescing.
No architecture decision may classify a Frequency as “slow” merely because its
interval happens to exceed an arbitrary day/second threshold.

The deadline director owns all timed Karma work, not only Frequencies:

`Frequency | promise/invitation/decision expiry | Signal poll | workflow wake |`
`effect retry/lease timeout | retention/checkpoint maintenance`

Event-driven Fact/sync/push-Signal occurrences bypass the timer fabric and wake
the sequencer directly. Timed subsystems do not keep private polling loops.

##### Three-layer timer structure

Use three layers, each with a different correctness job:

1. `karma_deadline` is the durable, indexed registration set. It lets
   restart reconstruct all timers and is the recovery truth for what must wake.
2. A tickless hierarchical wheel/calendar queue is the in-memory directory.
   Distance-to-deadline selects an outer/inner index level, but every entry and
   every lane summary retains the exact minimum `due_at_ms`. Bucket width never
   rounds the runtime arm. Occupancy bitmaps/next-nonempty metadata skip empty
   buckets; there is no periodic wheel sweep. Far registrations are reindexed
   only when an already-required wake reaches their nearer horizon or when they
   become due; reindexing never creates an unnecessary earlier wake by itself.
3. A dynamic set of deadline lanes arms one one-shot runtime timer per lane.
   The central director awaits the ready-lane set; the runtime queues only the
   lane tokens whose timers expired. It does not linearly poll every lane or
   every registration.

The wheel is logically partitioned by lane: every `DeadlineKey` belongs to one
lane index, and reinsert/removal mutates only that lane's buckets and exact-min
summary. The director owns the lane directory, so a dense path does not acquire
or traverse a sparse lane merely to calculate its own next arm.

Keep the semantic key, admission demand, and operational plan as different
Rust types:

    struct DeadlineKey {
        kind: DeadlineKind,
        target_uid: DeadlineTargetUid,
        generation: u64,
        due_at_ms: TimestampMs,
        stable_priority: u16,
    }

    struct ScheduleDemand {
        semantic_rate: RationalRate,
        wake_rate_upper: RationalRate,
        required_resolution_ms: u32,
        max_lateness_ms: u32,
        scheduler_cpu_ns_per_second: u64,
        evaluator_fuel_per_second: u64,
        writes_per_second_upper: RationalRate,
        effects_per_second_upper: RationalRate,
    }

    struct DeadlineLanePlan {
        lane_uid: LaneUid,             // operational, not domain identity
        generation: u64,
        member_keys: Vec<DeadlineKey>, // stable-sorted
        reserved_wake_rate: RationalRate,
        reserved_evaluator_fuel: u64,
        next_arm_at_ms: TimestampMs,
    }

`DeadlineKey` and schedule cursor determine occurrences. `ScheduleDemand`
determines admission and lane isolation. `DeadlineLanePlan` is replaceable host
runtime state and may never appear in a Program revision hash or typed Action
precondition. `DeadlineTargetUid` is a validated enum/newtype over the existing
UID families; construction checks that `DeadlineKind::Frequency` names a
Frequency Record, an expiry names its Promise/Decision family, and so on.

The wheel's powers/radix are an implementation detail derived from the
platform's base timer resolution and supported horizon, not user-visible
Frequency classes. Bucket placement depends on distance to the next deadline,
not the schedule's name or an arbitrary `1s/1d` cutoff. Declared lateness/
coalescing policy may deliberately move an arm within its allowed window; the
index itself may not. Buckets are lookup acceleration, not permission to round
semantic time.

Start with one sparse lane. The deterministic lane planner splits out dense
streams when their admitted wake/evaluation utilization would consume the
sparse lane's configured capacity or lateness reserve. It packs demand in the
stable order `(required_resolution, utilization descending, target_uid)` into
lanes with explicit wake/evaluation budgets; it merges lanes again when demand
disappears. Thus the decision is based on measured/calibrated resource demand,
not “less than N milliseconds.” Lane assignment is operational and may differ
by Cell; it is recorded for diagnostics but is not part of the schedule's
semantic hash or replay result.

A lane is timer/index state, not necessarily a task. The production adapter may
implement the ready set with dormant Tokio `Sleep` futures, a runtime timer
wheel, or `timerfd` registrations behind one poller. A dormant five-hour timer
consumes bounded metadata but no five-hour thread and no repeated CPU. An
explicit best-effort real-time resource grant may give one dense lane a
dedicated runtime thread or short final spin; ordinary Lince never busy-spins
or claims hard real-time behavior on a general-purpose OS.

##### Concrete `3ms + 5h` behavior

Suppose `freq:@sensor.fast` has `interval=3ms` and next boundary
`09:00:00.003`, while `freq:@report` has `interval=5h` and next boundary
`14:00:00.000`:

    frequency sensor.fast {
      every elapsed 3ms
      anchor 2026-07-21T09:00:00.000Z
      timer {
        resolution 1ms
        max_lateness 1ms
        coalesce_window 0ms
      }
      missed replay max 64
      inactive_gap skip_to_next_anchor
      overload pause_and_ask
    }

    frequency report {
      every elapsed 5h
      anchor 2026-07-21T09:00:00.000Z
      timer {
        resolution 1ms
        max_lateness 20ms
        coalesce_window 0ms
      }
      missed coalesce
      inactive_gap skip_to_next_anchor
    }

Both ask for millisecond-granularity wake service at their own boundaries, but
only `sensor.fast` creates dense wake demand. Increasing `report`'s
`max_lateness`/`coalesce_window` could save an isolated OS wake when another
deadline is nearby; it would still retain `14:00:00.000` as `intended_at`.

1. Admission reserves roughly 333.334 timer firings/second plus the fast
   Program's evaluation/write cost. The planner normally gives this demand a
   dense lane. Its first one-shot timer is armed for `09:00:00.003`.
2. The five-hour registration remains in the sparse lane/outer wheel with an
   independent exact one-shot wake target. It is not queried from SQL, popped,
   compared for rule truth, re-armed, or otherwise processed every 3ms.
3. At each fast wake, the director drains only due keys from that dense lane,
   persists an occurrence or reconstructible batch, computes the next boundary
   directly from the anchor/cursor, and re-arms that lane. It never obtains the
   next boundary by repeatedly adding 3ms to wall-clock wake time.
4. At `14:00`, the runtime marks the sparse lane ready independently of the
   dense lane. The director drains all ready lanes, collects every key with
   `due_at_ms <= observed_now`, and stable-sorts before persistence. The report
   therefore cannot be starved or hidden by a continuous fast stream.
5. Pausing `sensor.fast` invalidates its generation and retires/merges its lane.
   The report's timer remains armed. If it is the only deadline, the process
   does no Frequency work until that one-shot timer or a schedule-change
   notification arrives.

If a calibrated host can safely pack both into one lane, the no-scan invariant
still holds: re-arming the inner `3ms` bucket does not visit the outer `5h`
bucket or evaluate its key. Lane splitting adds latency/resource isolation;
the tickless indexed registration—not the existence of two threads—is what
provides correctness.

This is “shrink to fit”: CPU wake rate follows the deadlines that actually need
that rate, and timer metadata remains dormant at each other deadline's natural
horizon. There is never one global `check_every = min(active_intervals)` loop.

The ready set is level-triggered by `lane_uid`, not an unbounded FIFO of wake
messages. Repeated expiration of one dense lane leaves one ready bit plus its
latest observed cutoff. The deadline director does no graph evaluation: in one
bounded pass it converts each ready schedule cursor into a durable occurrence
or arithmetic range batch, then hands work to the sequencer. This prevents 333
fast wake messages from sitting ahead of a sparse lane token. If semantic ticks
must later be expanded individually, that cost belongs to admitted evaluator
capacity and declared overload behavior; it cannot starve timer registration.

“Not unnoticed” means every intended boundary becomes exactly one durable
occurrence identity/range or an explicit `skipped/coalesced/paused` record. The
deadline row remains active until the same transaction advances its cursor and
stores that evidence; an OS wake followed by a crash cannot consume it. Wake,
occurrence persistence, graph completion, and external-effect completion are
separate latency measurements. A Program that requires evaluation before the
next boundary declares `must_finish_before_next true`; activation then needs a
conservative worst-case evaluator reservation, not merely enough timer wakes.

Lince cannot promise hard real-time physical actuation from a general-purpose
host. If a motor/interlock genuinely needs a 3ms closed loop, Karma
deploys a versioned, bounded controller rule to a capable microcontroller and
treats configuration/telemetry as typed effects/Signals; the device enforces
the loop locally. The Cell can still deterministically reason about, simulate,
authorize, and audit that controller without pretending network/OS latency is
real-time.

##### Runtime port and deterministic drain

The pure/runtime boundary should expose registrations rather than a polling
period:

    struct DeadlineArm {
        lane_uid: LaneUid,
        lane_generation: u64,
        wake_at_ms: TimestampMs,
        required_resolution_ms: u32,
    }

    trait DeadlinePort {
        fn replace_arm(&self, arm: DeadlineArm);
        fn disarm(&self, lane_uid: LaneUid, generation: u64);
        async fn next_ready(&self) -> DeadlineWake;
    }

    enum DeadlineWake {
        LaneReady { lane_uid: LaneUid, generation: u64,
                    observed_at_ms: TimestampMs },
        DirectoryChanged,
        ClockDiscontinuity,
        Shutdown,
    }

Production converts stored UTC targets to a monotonic one-shot sleep and emits
`ClockDiscontinuity` when wall-clock mapping changes; calendar schedules are
then recalculated from their frozen timezone/tzdb rules. Simulation registers
the same arms and advances virtual time directly to the next one. Schedule math
remains pure in `nucleus::karma::schedule`.

Runtime wake order is never semantic order. When one or more lane tokens are
ready, the director snapshots `observed_now`, drains all ready tokens without
blocking, asks only those lanes for keys due by that cutoff, and sorts them by:

`due_at_ms → deadline_kind_priority → target_uid → generation`

`deadline_kind_priority` is a versioned numeric enum, not map/hash iteration
order. At the same intended millisecond, process: `0` authority/trust/grant
expiry, `10` promise/invitation/decision expiry, `20` workflow wake, `30`
Frequency occurrence, `40` Signal poll, `50` effect lease/retry, then `90`
maintenance. A deadline created by work at that cursor is appended after the
current ordered set and receives a monotonically increasing occurrence
sequence; it cannot jump backward into an already processed priority. Changing
this order is a replay-breaking semantic version change and requires a new
simulation epoch, never an incidental refactor.

`DeadlineKey` contains only stable ids, exact deadline, kind, and generation.
The full typed specification/state stays in the Store/cache. A tune/deactivate
atomically increments the target generation, stores the new `next_due_at`, and
notifies the directory. A stale key/arm is discarded by generation. Boot loads
active indexed rows once and rebuilds the operational wheel/lanes; normal
firing never scans every Frequency or every source table.

Persist the cross-subsystem index in `karma_deadline` with
`deadline_kind, target_uid, generation, due_at_ms, required_resolution_ms,`
`max_lateness_ms, coalesce_key, stable_priority, active` and a unique
`(deadline_kind, target_uid)` plus due-time index. Creating/changing a promise
expiry, decision expiry, Signal poll, workflow wait, effect retry, or Frequency
updates its deadline registration in the same semantic transaction. The
deadline row is a materialized scheduling index, not new domain truth; the
subsystem's typed state remains authoritative. Operational lane/bucket ids are
not persisted as semantics and are rebuilt for the current host. A coarse
maintenance deadline may audit/rebuild the index, but normal firing does not
poll source tables.

The schedule sidecar is owned by a Rust type and uses typed columns/joins rather
than an opaque behavior blob. At minimum it stores:

`record_uid, active_revision_uid, parameter_revision, generation,`
`schedule_kind, anchor_ms, interval_ms/calendar_rule, timezone, tzdb_version,`
`required_resolution_ms, max_lateness_ms, coalesce_window_ms, missed_policy,`
`max_replay, inactive_gap_policy,`
`rephase_policy, last_intended_at_ms, next_intended_at_ms, failure_policy`

Index active rows by `(next_intended_at_ms, record_uid)`. Validate
`interval_ms >= 1`; fixed interval and calendar rule are mutually exclusive.
`coalesce_window_ms` may delay a runtime wake only within the declared
`max_lateness_ms`; it never changes `intended_at` or merges semantic occurrences.
Persist resolved active references in `karma_frequency_consumer` with
`frequency_uid, consumer_kind, consumer_uid, consumer_revision_uid,`
`effective_from_cursor`, unique across that tuple. Program, Signal, workflow,
and Frequency activation transactions update those rows; the transition from
zero to one active consumer upserts the deadline and one to zero removes it/
increments generation. This avoids firing unused reusable Frequencies without
recomputing consumer counts on every wake.

No-consumer time is not silently treated as scheduler failure. The Frequency
revision declares `inactive_gap skip_to_next_anchor` (default) or an explicit
bounded `inactive_gap replay_by_missed_policy`. On the zero-to-one consumer
transition the same transaction calculates the next cursor from the anchor and
that policy, so reactivation cannot unexpectedly replay months of dormant work.
The transaction that advances `last/next_intended_at` also inserts/deduplicates
the durable occurrence/batch, so a crash cannot lose a tick after advancing the
cursor or fire it twice after restart.

##### Dense occurrence batching

A `1ms` Frequency represents up to 1,000 semantic ticks per second. Writing one
schedule row and full trace envelope for every no-op tick would turn storage
overhead into the feature. The scheduler may therefore persist a deterministic
`OccurrenceBatch`:

    activation_hash
    batch_sequence
    emission                  # individual | coalesced
    first_schedule_ordinal
    range.first
    range.interval_ms
    range.count

Each semantic tick has a derived identity from
`(activation_hash, schedule_ordinal, intended_at)`. The activation hash already
commits the Frequency uid, immutable definition revision, complete effective
parameter map, activation generation/cause, and schedule. The ordinal is
relative to that schedule's frozen anchor, not to a host-wake batch, so two
different wake segmentations reproduce the same tick identity. The evaluator
processes ticks in ordinal order through bounded pages of at most 4,096 ticks;
the page cursor will be durable sequencer state rather than an allocation of an
unbounded range. A coalesced batch exposes exactly one aggregate occurrence and
cannot be expanded through the individual-tick API. Any tick that produces a candidate, intent, Fact,
failure, bookmark, state transition, or sampled full trace keeps its individual
run/evidence. Consecutive no-op ticks may share a compact trace summary because
the batch reconstructs them exactly. Batching is a storage representation, not
coalescing semantics: a Program asking for every tick still receives every tick
unless its declared missed/overload policy says otherwise.

The durable occurrence row stores the complete canonical occurrence envelope,
not merely a loosely typed advance fragment. SQL columns project cadence,
emission kind, first/last intended boundary, covered boundary count, and
semantic occurrence count; Store reload recomputes and compares every
projection and the content hash. The same fenced transaction advances the
cursor and inserts this immutable batch. Five overdue 3ms boundaries therefore
occupy one row while remaining exactly reconstructible as five identities. An
on-time tick is never speculatively grouped with a future boundary: future work
may still be paused or revised before it becomes due.

If the worker wakes late, pure schedule math computes how many intended ticks
exist between `last_intended_at` and `now`. Then the declared policy applies:

- `skip`: record skipped count/range and advance;
- `coalesce`: emit one occurrence containing missed count/range;
- `replay(max=N)`: emit up to `N` semantic ticks in order and record overflow;
- `pause_on_lag`: fault/pause before pretending late real-world actions are
  timely.

External/device/social effects should normally forbid unbounded replay. A
missed 1ms computation may be replayable; a missed motor command, notification,
or Transfer proposal is not repeated thousands of times without an exact
explicit policy.

##### Cost model and admission

Publishing a definition calculates a conservative fixed-point `ScheduleDemand`
from each active Frequency's rate, wake requirements, and dependency fan-out:

    semantic_ticks_per_second = sum(1000 / interval_ms)
    timer_wakes_per_second = sum(after explicitly allowed timer coalescing)
    scheduler_cpu_ns_per_second = sum(wake_rate * calibrated_fire_cost_ns)
    node_evaluations_per_second = sum(rate * impacted_node_count)
    estimated_writes_per_second = sum(rate * materializing_path_count)
    estimated_effects_per_second = sum(rate * effect_path_upper_bound)
    estimated_trace_bytes_per_second = rate * trace_policy_estimate

These are exact rational/fixed-point calculations internally; the displayed
decimal is not used for admission. `semantic_ticks_per_second` and
`timer_wakes_per_second` differ only when an explicit reconstructible batch or
wake-coalescing window permits it. Batching never lowers the semantic work
estimate unless the Program itself declares coalesced semantics.

Conditional gates may lower the displayed expected estimate, but admission uses
the safe upper bound unless the compiler can prove a tighter bound. For a
calendar rule, derive `rate` from its shortest possible interval over the
declared timezone/tzdb horizon, not its average interval; an unbounded or
unprovable rule is rejected until the owner supplies an enforceable rate cap.
The Karma sand shows both estimates, plus CPU/storage budget, retention growth,
lateness target, platform resolution, proposed lane plan, and whether the
Program contains external effects.

The low-level contract is `ScheduleWorkloadUpperBounds` (fuel, writes, effects,
and trace bytes per semantic tick), `SchedulerCalibration` (measured CPU
nanoseconds per wake), `ScheduleDemand` (exact reduced rational rates), and
`ScheduleDemandCapacity` (the Cell-wide hard ceilings). Calendar providers must
also return a conservative minimum-interval attestation bound to their pinned
tzdb artifact and the complete calendar schedule. Absence of that attestation
is `calendar_rate_unproven`, never an average-rate fallback. Every durable
cursor stores the canonical demand snapshot admitted with its activation;
loading recomputes it from the frozen schedule, workload, calibration, and
provider attestation before trusting the projection. Stable capacity failure
order is semantic ticks → timer wakes → scheduler CPU → evaluator fuel →
writes → effects → trace bytes, so the same snapshot always explains the same
first denial.

Resource gates are based on demand/capacity, never fixed interval bands:

| Gate | When it applies | Required authority/policy |
| --- | --- | --- |
| Baseline scheduling | Demand fits the Program's ordinary wake/evaluation/write budgets and Cell reserve | ordinary Program execution grant |
| Dense scheduling | Projected wake or scheduler/evaluator utilization exceeds the ordinary budget or requires lane isolation to meet existing lateness reserves | `karma.schedule.dense`, explicit rate/fuel/write budgets, activation load proof |
| Precision scheduling | Requested resolution/lateness is finer than the Cell's measured normal timer service, regardless of cadence | `karma.schedule.precision`, supported platform adapter, explicit best-effort downgrade or rejection |
| Effect-heavy scheduling | Upper-bound materialization/external-effect rate exceeds its ordinary capability budget | exact Action/effect grant, rate/value limits, idempotency and overload policy |

A `3ms` no-op recognizer and a five-hour workflow with ten thousand effects can
therefore hit different gates for different reasons. A `1ms` Frequency will
normally need dense plus precision authority, but `1ms` is not itself a magic
branch in the scheduler. The same demand formula applies to `2ms`, `37ms`,
`5h`, calendar schedules, and any later precision the canonical time type can
represent.

The Cell also has hard aggregate ceilings independent of individual grants.
Activation is rejected (or can remain shadow-only) if the sum of active upper
bounds exceeds them. Runtime meters actual wake lateness, evaluations, CPU/fuel,
writes, effects, and trace bytes. On overrun the predeclared policy is one of
`coalesce`, `drop_with_evidence`, `stage_effects`, or `pause_and_ask`; it never
silently changes the interval or omits ticks.

- [ ] Protein exposes lane count/kind, lane membership/reason, each lane's next
  arm, wheel/overflow occupancy, requested/effective resolution and lateness,
  estimated/actual wake/semantic/evaluation/write/effect rates, lateness
  percentiles, batch/skipped/coalesced counts, budget use, and pending
  split/merge. This is operational metadata, not a new scheduling truth.
- [ ] The Karma sand warns at authoring, proves at activation, and offers shadow load
  testing with effects disabled. A person choosing `1ms` sees the expected
  86,400,000 ticks/day and retention/effect implications before granting it.
- [ ] When dense demand pauses, its lane is disarmed and retired/merged without
  changing sparse arms. A Cell with only a `5h` Frequency performs no Frequency
  work between activation and its exact next one-shot deadline.

#### Mutation verbs

The DSL makes data-changing boundaries visually obvious:

| Verb | Meaning | Direct domain mutation? |
| --- | --- | --- |
| `let`, `sense`, `predict`, `project`, `solve` | Pure computation | No |
| `emit` | Materialize a typed derived observation/Fact | Yes, append-only and explicit |
| `recommend` | Create/update an inert candidate | No domain mutation |
| `draft` | Create an inert typed Action/Transfer/program draft | Draft data only |
| `ask` | Create a durable decision with Action previews | No until answered |
| `act` | Request a typed domain Action under a grant | Yes, through Action after authorization |
| `do` | Request an external/device/UI effect | Outside-world attempt plus receipt |
| `tune` | Change a declared parameter of another/named Program | Program parameter data; effective next occurrence |
| `revise` | Propose a new immutable graph revision | Definition data only; activation is separate |
| `pause` / `resume` | Change activation of a named Program | Program activation Fact; never deletes history |

There is no generic `set field` or `eval string`. Each `act`, `do`, `tune`, or
`revise` compiles to a typed candidate/Action with exact target, schema,
preconditions, grant requirements, and preview.

### Determinism — the replay contract

The guarantee is precise: **given the same replay capsule and ordered captured
inputs, the same engine must emit the same canonical node values, candidates,
policy decisions, Action intents, and unsigned Ledger payloads/content hashes.**
Captured signatures and receipts replay as their original bytes; a simulator
uses a fixture signer rather than requiring production secrets. This is the
useful meaning of deterministic “to the atom.” It does not promise that
rerunning an HTTP request or motor command changes the outside world in the same
way.

A replay capsule contains the starting checkpoint/hash-chain anchor, ordered
Fact and occurrence stream, program/model/grant revision hashes, engine/schema
build, Lingua/unit conversion revisions, timezone database version, virtual
clock, deterministic seed, solver/plugin hashes, captured Signal results, and
external receipts. It is exportable, inspectable, and sufficient to replay
without network, filesystem, wall clock, devices, or secrets.

- [ ] Put clock, scheduling, entropy, uid generation, filesystem, network,
  process execution, device I/O, and model calls behind injected runtime ports.
  Pure evaluation cannot call an ambient OS API. Production adapters capture a
  result; simulation adapters generate or replay one.
- [ ] Assign every accepted local Fact/occurrence a monotonically increasing
  Cell cursor in its commit transaction. Live behavior follows recorded arrival
  order. Sync packages may arrive in a different order on another Cell; replay
  reproduces each Cell's observed order rather than falsely claiming distributed
  simultaneity.
- [ ] Canonicalize maps, sets, strings, units, timestamps, and serialization.
  Sort unordered query results and graph edges explicitly. Stable ordering is
  dependency rank, declared priority, program uid, node id, then occurrence uid;
  CPU thread completion order never breaks a tie.
- [ ] Replace decision-critical binary floating point with canonical decimal,
  rational, integer base-unit, or specified fixed-point arithmetic. Probability
  scales, rounding mode, overflow, invalid values, and unit conversion are part
  of the type. `NaN`, infinities, locale parsing, and platform math must not
  enter a policy decision.
- [ ] Any stochastic algorithm receives a recorded seed and deterministic
  stream partition per node. Any solver declares its version, tolerances,
  variable ordering, timeout measured in deterministic work units, and stable
  tie-break. “First result returned by workers” is not a valid choice rule.
- [ ] Calendar evaluation freezes timezone and calendar-rule versions. A
  schedule records both the intended civil occurrence and resolved UTC instant,
  including daylight-saving gaps/folds and leap behavior.
- [ ] Content-address pure extensions (for example sandboxed WebAssembly), deny
  them clock/random/I/O, give them deterministic fuel and memory limits, and
  specify their numeric ABI. Native or remote opaque computation enters as a
  captured Signal instead.
- [ ] Treat nondeterministic AI/model output as an observation with model id,
  request hash, response hash, and capture time. Replay uses the captured
  response. A deterministic local model still pins weights, feature schema,
  runtime, tokenizer, numeric policy, and seed.
- [ ] Derive run, candidate, and intent ids from their semantic occurrence
  where practical. Generate any remaining ids/timestamps through the replay
  runtime so a replay does not manufacture different identities.
- [ ] Freeze the effective grant at proposal time for explanation, but recheck
  current revocation, budgets, target revision, and safety interlocks when an
  intent is claimed and immediately before irreversible dispatch. A revoked
  intent deterministically becomes denied/cancelled, not raced.
- [ ] Upgrades never reinterpret an old run silently. Replaying under the old
  engine is reproduction; replaying under a new engine is an explicit
  differential run whose changed candidates, facts, and effects are shown.

### The always-on occurrence kernel

One scheduler owns causality. Heartbeats, subscriptions, sync imports, workflow
wakes, and effect completions submit occurrences to it; they do not each grow a
private automation loop.

For each occurrence the engine:

1. persists/deduplicates it and assigns the Cell cursor;
2. freezes logical time, visible input cursor, active program/model revisions,
   and the triggering principal;
3. selects impacted programs from declared dependencies;
4. evaluates pure nodes in stable graph order, recording every substituted
   value, missing/stale input, branch, model output, and assertion;
5. materializes inert candidates, then evaluates objectives, policy, authority,
   taint/declassification, budgets, and conflicts;
6. atomically stores the run result and any permitted durable intents; and
7. lets separate workers claim intents, perform typed Actions/effects, append
   receipts/Facts, and thereby enqueue later occurrences.

This makes a run-to-completion agenda deterministic while still allowing
parallel prefetch and pure computation. Parallel work may improve latency but
cannot change commit order or selection.

#### Reaction-before-learning law

For one incoming change, Lince uses the definitions that were active **before
that change**. Existing behavior reacts first; learning and meta-rules adapt the
system afterward. This resolves the Human Note without allowing a newly learned
rule to reinterpret the very evidence that created it.

The sequencer maintains two ordered lanes per cursor:

1. **Reaction lane:** freeze the active Program revisions, parameters, grants,
   and model checkpoints; evaluate impacted rules/Senses/workflows; commit the
   run and authorized local Action intents. Synchronous local domain Actions may
   append child Facts, whose reaction occurrences also stay ahead of learning.
   External effects remain durable intents and their later receipts are new
   occurrences.
2. **Learning lane:** after the bounded reaction closure for that cursor,
   admit/reject eligible evidence, update models, detect patterns, and create
   recommendations or Program-revision/parameter candidates. Generated Facts
   and Actions are excluded as training evidence unless an independent outcome
   policy explicitly admits a later observation.
3. **Adaptation occurrence:** an approved or pre-delegated `tune`, `revise`,
   `activate`, `pause`, or generated-rule promotion commits as its own later
   occurrence. Its `effective_from_cursor` is strictly greater than the event
   that proposed it. No definition changes halfway through a run or cascade.

Thus an event at cursor 100 is handled by epoch 12 even if its evidence raises a
pattern above the activation threshold. If learning creates/activates epoch 13,
epoch 13 starts at cursor 101 or later. An explicit `replay from cursor 100
under rev 13` may compare or intentionally create a new compensating plan, but
it is a new visible occurrence—not retroactive history.

- [ ] Prioritize the reaction lane over background learning so a burst of model
  maintenance never makes obvious existing rules feel unresponsive. Bound the
  reaction closure and expose queue age; runaway cascades fault instead of
  starving all learning forever.
- [ ] Learning may compute in parallel from immutable snapshots, but checkpoint
  commits remain cursor-ordered. A run records `model_trained_through_cursor` so
  a person can see when a prediction was based on lagging learning state.
- [ ] Meta-rules may alter named Programs only through `tune`, `revise`,
  `activate`, `pause`, or `resume` and an `karma.manage` grant. They may
  not mutate in-memory nodes or schedule rows invisibly.
- [ ] Changing a schedule requires an explicit rephase policy:
  `preserve_anchor` (default), `from_last_intended`, `from_change`, or
  `immediate_if_overdue`. The run preview shows old/new next occurrences before
  the parameter Action commits.

- [ ] Run one elected occurrence sequencer per writable Cell. Multiple service
  processes may execute leased effects, but they may not race independent rule
  agendas against the same Ledger.
- [ ] Recover all nonterminal runs, workflows, and intents after restart.
  Persist debounce, cooldown, last-consumed occurrence, schedule cursor, rate
  budget, leases, and retry state; boot never treats forgotten memory as new
  permission to fire.
- [ ] Support deterministic trigger concurrency policies:
  `queue`, `drop`, `coalesce`, `latest`, and bounded `parallel`, with a
  required correlation key. Backpressure is visible as lag/parked work rather
  than silent loss.
- [ ] Bound evaluations by nodes, iterations, deterministic fuel, fan-out,
  candidate count, trace size, and declared cost. A budget violation faults the
  run, optionally pauses the program, and opens one deduplicated operational
  decision.
- [ ] Provide Cell modes: `normal`, `stage-effects` (evaluate and queue but
  dispatch nothing), `observe-only` (models/derivations continue, no action
  intents), and `emergency-stop` (no new runs/effects except recovery and
  inspection). Mode changes are durable, permissioned, and visible.
- [ ] Define overload priority without hiding starvation: safety/revocation and
  already-agreed Transfer deadlines first, then explicit user priority,
  ordinary workflows, learning maintenance, projections, and background
  analysis. Every delayed class exposes queue age and next eligibility.

### Program graph and authoring language

The canonical definition is one typed, versioned graph AST. The Karma sand's visual
graph, typed forms, JSON transport, and an expert textual DSL are lossless
projections of that AST; they are not separate execution languages. Node ids
remain stable across layout and label changes so diffs, state, and explanations
survive editing.

| Node family | Pure/durable role |
| --- | --- |
| Trigger | Fact/event, schedule, signal sample, threshold crossing, manual call, sync arrival, decision, workflow wake, or effect receipt. |
| Input | Saved/inline Protein, direct record/fact reference, parameter, secret reference metadata, or captured Signal. |
| Normalize/feature | Unit conversion, validation, window, aggregate, join, lag, rate, calendar/place feature, missing-data policy, and quality weighting. |
| Derive/recognize | Typed arithmetic/logic, stateful threshold with hysteresis, finite state recognizer, pattern model inference, or reusable Sense. |
| Project/analyze | Imagination branch, forecast, invariant, query, aggregate, optimizer, ranker, or sensitivity/infeasibility analysis. |
| Control | Gate, branch, merge, explicit bounded iteration, delay, debounce, cooldown, rate limit, transaction boundary, and assertion. |
| Workflow | Sequence/parallel, wait-until, approval, child program, retry, compensation, cancellation, and correlation. |
| Candidate/attention | Recommendation, decision, program revision, plan, report, whisper request, or digest item. |
| Intent/effect | Typed Action, Transfer Action, connector call, device/controller intent, notification, command, or HTTP request. |

Ports carry schema, unit/dimension, cardinality, visibility/taint, freshness, and
uncertainty. A connection that cannot prove compatibility is invalid; it does
not coerce strings at runtime. Missing, stale, denied, invalid, and unknown are
typed states distinct from numeric zero and Boolean false.

- [ ] Resolve authoring-time `@slug` sugar to uid plus displayed slug in the
  immutable revision. Rename never changes meaning; an unresolved reference
  blocks activation.
- [ ] Make every stateful node declare initialization, update event,
  persistence, reset/migration, late-event behavior, and whether simulation
  branches clone its state.
- [ ] Allow reusable subprograms with explicit typed parameters and outputs.
  Invocation freezes a revision; a template update never silently edits
  installed programs.
- [ ] Keep arbitrary shell/HTTP/model/device work out of expressions. A pure,
  content-addressed extension may calculate; an effect node may interact with
  the world; the graph makes the boundary visible.
- [ ] Compile legacy rule expressions to the graph as an import path only.
  New capability must not be constrained by `rq1`/`kd2` token compatibility.

#### DSL shape and short node vocabulary

The DSL is declarative and formatter-stable. Blocks describe a graph; they do
not execute top-to-bottom like a shell script. Data dependencies define order,
while workflow edges define durable sequencing. `#` begins a comment.

| Family | Preferred words | Meaning |
| --- | --- | --- |
| Metadata | `program owner purpose tags mode` | Identity and intent |
| Parameters | `param state` | Typed configurable or durable node state |
| Triggers | `on fact`, `on every`, `on at`, `on signal`, `on manual`, `on decision`, `on receipt` | Create occurrences |
| Timing | `every elapsed/calendar`, `anchor`, `resolution`, `max_lateness`, `coalesce_window`, `missed`, `inactive_gap`, `rephase` | Separate intended cadence from runtime wake service/catch-up |
| Inputs | `input`, `view`, `record`, `signal`, `secret` | Declare exact data dependencies |
| Features | `let`, `window`, `sum`, `count`, `avg`, `rate`, `lag`, `join`, `convert` | Pure preparation |
| Recognition | `sense`, `when`, `crosses`, `enters`, `leaves`, `holds` | Detect a situation/transition |
| Learning | `learn`, `predict`, `update`, `validate` | Versioned model operations |
| Futures | `project`, `branch`, `assert` | Imagination/proof |
| Optimization | `solve`, `require`, `prefer`, `minimize`, `maximize`, `tie_break` | Explicit objectives/constraints |
| Workflow | `step`, `parallel`, `wait`, `approve`, `retry`, `compensate`, `cancel` | Durable orchestration |
| Routing | `observe`, `recommend`, `draft`, `ask`, `act` | Autonomy ladder |
| Effects | `emit`, `do action`, `do transfer`, `do command`, `do http`, `do device`, `do ui` | Explicit mutation boundary |
| Meta-control | `tune`, `revise`, `pause`, `resume`, `run` | Manage named Karma objects |
| Policy | `scope`, `freshness`, `dedupe`, `budget`, `require grant`, `on denied`, `on stale`, `on failure` | Guard behavior |

The expression language supports typed literals, references, arithmetic,
comparison, Boolean operations, `if`/`match`, collection reducers, and pure
registered functions. It does not support reflection, dynamic field names,
unbounded loops, arbitrary recursion, shell interpolation, network calls, or
implicit reads. Explicit bounded `iterate max N until condition` is a graph node
with deterministic fuel and convergence trace.

A compact grammar sketch:

    definition   := program | frequency | signal | model | objective | trust
                  | grant_template
    program      := "program" slug "{" declaration* "}"
    frequency    := "frequency" slug "{" frequency_decl* "}"
    frequency_decl := param | cadence | anchor | timer_policy | missed_policy
                    | inactive_gap_policy | rephase_policy | overload_policy
    cadence      := "every" ("elapsed" duration | "calendar" calendar_rule)
    timer_policy := "timer" "{" ("resolution" duration)
                    ("max_lateness" duration) ("coalesce_window" duration) "}"
    declaration  := metadata | param | trigger | input | derive | model
                  | objective | route | workflow | policy
    trigger      := "on" trigger_kind trigger_policy*
    derive       := ("let" | "sense") local_id ":" type? "=" expression
    route        := "when" expression "{" outcome* "}"
    outcome      := recommend | draft | ask | act | emit | tune | revise
                  | pause | resume | external_effect
    reference    := kind ":@" dot_slug | "@" dot_slug
    duration     := integer ("ms" | "s" | "m" | "h" | "d" | "w")

##### K1.4 canonical Program DSL

The first parser/formatter slice deliberately covers every variant in the
current `ProgramAst` rather than pretending that later model/effect/workflow
nodes already exist. Its strict text starts with `karma 1;`, carries
`schema karma.program.v1;`, and spells out tags, capabilities, parameter
mutability, node bindings, port sensitivity/freshness, operations, state
contracts, and program outputs. Collections are formatter-sorted because the
AST uses ordered maps/sets. This form is a canonical projection, not the final
amount of text a person must type in the Karma sand.

Current node operations use these lossless forms:

    op trigger(manual, event);
    op input(record-quantity(ref(record, r_..., apple.stock)), value);
    op derive {
      low = binary(less, input(stock), input(threshold));
    }
    op delay(next, previous, i64(0),
      state(program, never, ignore, reset, clone));

Types and literals are explicit constructor expressions. Examples are
`decimal(3)`, `quantity(3, uid(unit, c_...))`, `datum(i64)`,
`decimal(3, "12.340")`, `prob("0.720000000")`, and
`datum(i64, missing)`. Published references always format as
`ref(kind, uid, display_slug_or_none)`; shorthand `@slug` is resolved before
canonical formatting and therefore cannot make a stored revision depend on a
future rename. Expressions use explicit `literal`, `input`, `unary`, `binary`,
and `if` constructors in this slice, avoiding precedence ambiguity. K1.5 adds
the separate Frequency definition grammar over elapsed/calendar schedule ASTs.

The lexer accepts Unicode only inside JSON-escaped strings; semantic tokens are
ASCII. `#` comments and insignificant whitespace are accepted but removed by
formatting. Parsing is deterministically bounded by source bytes, token count,
string bytes, and nesting depth. Errors contain a stable kind plus byte offset,
line, and column; unknown declarations, constructors, enum values, duplicate
ids, trailing input, and over-limit input fail instead of being ignored. The
only success criterion is:

`parse(format(ast)) == ast` and `format(parse(text)) == format(ast)`.

Ergonomic infix expressions, omitted ids/types, and short resolved-reference
syntax are a later normalization layer that must produce this same AST before
Proof or storage. The canonical parser never guesses a type, resolves a slug,
or accesses Records, Store, timezone, clock, locale, or network.

**K1.4 implementation status:** complete. `format_program` and `parse_program`
are pure functions in `nucleus::karma::dsl`; the formatter output itself is an
exact golden fixture and all current AST families round-trip losslessly. K1.5
applies the same boundary discipline to Frequency definitions rather than
combining two independently versioned AST families in one parser function.

##### K1.5 canonical Frequency AST and DSL

A Frequency has its own immutable `karma.frequency.v1` revision and hash. The
revision contains slug, non-empty purpose, tags, named schedule parameters,
cadence/anchor, timer service, missed/inactive/rephase/overload policies, and
calendar timezone/tzdb/gap/fold data where applicable. It never contains an
active flag, consumer list, current cursor, next deadline, lane, measured host
capacity, or wake result; those are mutable handle/runtime state.

Frequency parameters are deliberately smaller than Program values. A named
parameter is either a non-negative exact millisecond duration with inclusive
minimum/default/maximum, or a positive `u32` integer with inclusive bounds.
Schedule fields use a typed `literal(...)` or `parameter(local_id)` binding;
there is no expression evaluation, ambient parameter name, numeric coercion,
or arbitrary JSON. Anchors, timezone identity, tzdb identity, weekday sets,
day-of-month, and policy enums require a new revision rather than parameter
tuning because changing them can reinterpret civil identity or authority.

Compilation accepts an explicit map of overrides, rejects unknown names/type
mismatches/out-of-range values, fills every other value from the revision
default, and resolves bindings into a concrete `ElapsedSchedule` or
`CalendarSchedule`. The result carries the Frequency revision hash, complete
effective parameter map, and concrete schedule. It validates interval/timer
integer ranges and cross-field timer invariants after resolution. Store/runtime
code must persist the effective parameter hash with an activation epoch; it
must never compile from whatever mutable values happen to be visible halfway
through a run.

The strict canonical form begins `karma-frequency 1;` and formats, for example:

    frequency sensor.fast {
      schema karma.frequency.v1;
      purpose "Sample the local sensor";
      tags [sensor];
      param interval: duration default duration(3)
        range [duration(1), duration(1000)];
      every elapsed parameter(interval);
      anchor timestamp("2026-07-21T09:00:00.000Z");
      timer {
        resolution duration(1);
        max-lateness duration(1);
        coalesce-window duration(0);
      }
      missed replay(64);
      inactive-gap skip-to-next-anchor;
      rephase preserve-anchor;
      overload pause-and-ask;
    }

Calendar form replaces elapsed cadence/UTC anchor with a `daily`, `weekly`, or
`monthly` rule, canonical civil anchor, timezone, pinned tzdb version/hash, and
gap/fold policies. Weekly weekdays are sorted Monday through Sunday. Monthly
invalid-day behavior is explicit. Both calendar and elapsed schedules carry
the same rephase policy; rephase changes future boundaries and never rewrites a
consumed civil/UTC boundary. Parsing uses K1.4's bounded lexer/error contract,
and success requires AST/text round-trip plus identical compiled schedule and
revision hash for the same explicit parameter map.

**K1.5 implementation status:** complete. `FrequencyAst::compile` produces a
`CompiledFrequency` containing the immutable revision hash, effective-parameter
hash/map, and concrete schedule. `format_frequency`/`parse_frequency` are pure,
bounded, lossless projections sharing the K1.4 lexer/error contract. Frequency
wire types have their own golden fixture; calendar's golden fixture was
deliberately updated when rephase became part of its complete contract.

##### K1.6 deterministic gates, temporal controls, and candidate routing

K1.6 adds the stateful decision vocabulary needed to turn exact values into a
stable flow without granting the pure graph an effect channel. These nodes are
ordinary combinational graph nodes: their current input determines their
current output and their state update is written only after the occurrence.
They therefore do **not** break a graph cycle. Only an explicit `delay` is a
read-old/write-next cycle boundary. This distinction prevents an apparently
stateful threshold or debounce from concealing an instantaneous dependency
cycle.

`threshold` has one ordered scalar input and Boolean `active`, `entered`, and
`left` outputs. It stores only the old `active` bit. An `above` threshold enters
when `value >= enter` and leaves when `value <= exit`, with Proof requiring
`exit < enter`. A `below` threshold enters when `value <= enter` and leaves when
`value >= exit`, with Proof requiring `enter < exit`. Values in the open band
preserve the old state. `entered` and `left` are one-occurrence pulses, so an
oscillating measurement inside the band cannot repeatedly fire. Threshold
literals must have exactly the input type and that type must be an ordered
scalar; unit, currency, scale, and referenced kind are never coerced.

`debounce` has one Boolean input and `stable`, `entered`, and `left` Boolean
outputs. A changed input starts a candidate interval at the frozen logical
timestamp. It becomes stable only after the same value has continuously
remained pending for `for-at-least`; an exact-boundary timestamp qualifies, a
return to the stable value cancels the pending interval, and duration zero
promotes immediately. `cooldown` accepts a Boolean pulse only if no pulse was
previously accepted or `now - last_allowed_at >= cooldown`; false inputs never
consume the window. `rate-limit` similarly accepts at most `max` true pulses in
the half-open rolling interval `(now - window, now]`. An acceptance exactly one
window old has expired. The retained timestamp list is bounded by `max`, and
Proof requires non-negative debounce/cooldown durations, a strictly positive
rate window, and non-zero `max`.

All three temporal controls require `FrozenEvaluationContext.logical_at` and
keep their state in `control_state`, separate from `delay_state`. Every state
variant stores `last_observed_at` so logical time cannot silently move
backwards. Under `late_event: ignore`, the occurrence emits no new pulse or
acceptance and leaves state unchanged (`debounce.stable` still reports the old
stable value). Under `reject`, evaluation returns `non-monotonic-logical-time`.
`recompute` and `compensate` return `replay-required`: a single pure invocation
cannot reconstruct the intervening history, and the future K3 occurrence
runner must replay the captured ordered inputs before committing replacement
or compensating state. Absence of logical time returns `missing-logical-time`.
The complete typed state-before and staged-state-after values belong in the
trace and replay capsule.

`route-candidate` has a Boolean condition, one declared output, an explicit
route (`observe`, `recommend`, `draft`, `ask`, or `act`), a template slug, and
an ordered map from candidate field names to input bindings. Proof derives the
exact output type `datum<candidate(route, template, fields)>` and rejects
missing, extra, or differently typed fields. False produces a typed missing
datum. True produces a typed value datum containing the immutable candidate
payload. Every route is inert in K1.6: even `act` means “this candidate asks the
later policy/authority/effect pipeline to attempt acting”, never “perform an
effect now”. Candidate identity, persistence, deduplication, grants, intent
creation, and Actions remain K2/K5 concerns.

The canonical Program DSL uses these lossless forms:

    op threshold(value, active, entered, left, above,
      i64(10), i64(8), false,
      state(program, never, ignore, reset, clone));
    op debounce(input, stable, entered, left, duration(250), false,
      state(program, never, reject, reset, clone));
    op cooldown(input, allowed, duration(5000),
      state(program, never, reject, reset, clone));
    op rate-limit(input, allowed, 3, duration(60000),
      state(program, never, reject, reset, clone));
    op route-candidate(condition, proposal, recommend, buy.apple,
      {amount = amount, seller = seller});

Candidate port types and literals use
`candidate(recommend, buy.apple, {amount: quantity(3, uid(unit, ...))})`
and
`candidate(recommend, buy.apple, {amount = quantity(...), seller = ref(...)})`.
The field order is semantic and canonical because both maps use `BTreeMap`.
The Flow Plane projects threshold enter/exit values as two points joined by a
hysteresis band, temporal controls as annotated ranges, state transitions as
edge pulses, and candidate routes as terminal inert nodes. Thus the same AST
drives execution, Why traces, simulation, and the later 2D Karma sand; the UI
does not invent a second rule model.

**K1.6 implementation gate:** golden wire fixtures must cover every new enum,
state, type, literal, operation, error, and DSL constructor. Sequence tests
must prove exact-boundary threshold/debounce/cooldown/rate behavior, late-event
policy, read-old/write-next staging, candidate inertness, graph-cycle rules,
and byte-identical results for identical frozen contexts.

**K1.6 implementation status:** complete. `NodeOperation` and the canonical
Program DSL now include all five families. Proof rejects reversed/equal
hysteresis bands, wrong scalar/unit/port types, invalid durations/windows,
candidate schema drift, and cycles hidden behind a control node. Evaluation
validates persisted control-state invariants, uses only `logical_at`, stages
updates, and emits inert typed candidate data. Exact-boundary, late-event,
corrupt-state, deterministic-result, DSL round-trip, and wire-golden tests are
part of the Nucleus suite.

##### K1.7 portable pure-evaluation replay capsule

K1.7 captures the complete boundary of one **pure Program evaluation**. This is
the first executable layer of the larger replay contract, not a claim that K1
already captures occurrence streams, models, grants, signatures, effects, or
Store checkpoints. `EvaluationReplayCapsule` has its own
`karma.evaluation-replay-capsule.v1` schema and a separately explicit
`karma.evaluator.v1` semantic revision. It embeds the immutable `ProgramAst`,
the Program revision hash, complete `FrozenEvaluationContext`, exact
`EvaluationLimits`, the expected `EvaluationResult`, and that result's
domain-separated canonical hash. Embedding the Program makes the capsule
portable; the hash prevents the embedded definition from being silently
substituted.

The capsule is wrapped by `SealedEvaluationReplayCapsule`, whose canonical hash
covers every inner byte-equivalent field. Capture evaluates once through the
ordinary proven evaluator, records that exact result, and seals the capsule.
Replay performs checks in this order: outer seal, embedded Program revision,
stored expected-result hash, ordinary evaluation, then exact expected/actual
result equality and canonical result hash. Each failure has a stable typed
code; an evaluator failure remains nested as its original typed error. There is
no bypass that accepts stale inner hashes merely because a caller resealed the
outer wrapper. This seal is a content address, not an authenticity signature:
a caller may deliberately recapture fully changed inputs and output, but that
is a new capsule with a new hash. K2 associates ownership/signatures without
changing this content verification.

This first capsule intentionally contains values rather than pointers to live
Records or state rows. It can be serialized, moved, inspected, and replayed
with no Store, clock, timezone provider, filesystem, network, randomness,
device, or secret access. K2 persistence may content-address/deduplicate large
capsules, and K3/K10 will compose them into full occurrence/checkpoint capsules
with ordered Facts, schedules, models, policy/grant snapshots, captured ports,
and receipts. That storage optimization must preserve the same resolved
canonical content and verification order.

**K1.7 exit gate:** repeated replay and serde round-trip reproduce an exactly
equal `EvaluationResult`; mutation of the outer envelope, Program, frozen
context, or stored expected result fails at the corresponding typed boundary;
the schema, evaluator revision, error vocabulary, and a complete sealed capsule
have golden hashes.

**K1.7 implementation status:** complete. Capture and replay share the ordinary
Proof/evaluator path, embed the full pure boundary and expected trace, and use
domain-separated hashes for Program revision, evaluation result, and sealed
capsule. Tests cover repeated replay, serialization, outer mutation, resealed
context divergence, Program substitution, stale expected results, and nested
typed evaluator failure.

##### K1.8 exact Economy event and aggregation primitives

K1.8 freezes the pure value contract that E0 persistence and Actions must use.
`EconomyMagnitude` is an exact `DecimalValue` plus a `unit` reference; its
constructor and deserializer require a strictly positive mantissa and a real
`ReferenceKind::Unit`. Zero, negative magnitude, a client sign, float, currency
conversion, and an untyped unit string cannot enter the draft. Calling
`signed_delta(direction)` is the only conversion to `EconomySignedDelta`:
`gain` preserves the positive amount and `loss` checked-negates it. Thus clients
never submit a signed delta capable of contradicting direction.

`EconomyEventDraft` is an immutable `economy.event-draft.v1` value revision. It
contains one resource Record reference, direction, magnitude, normalized UTC
`occurred_at`, optional source, ordered tags, optional note, capture origin, and
optional cause. A source is either a Record reference or a bounded canonical
human label. A tag is a Record or Lingua Concept reference. A cause explicitly
names `occurrence`, `capture`, `program`, or `fact` and Proof-like validation
requires the corresponding Record/Program/Fact reference kind. Capture origin
is `manual`, `typed`, `voice`, `photo`, or `agent`; it is provenance, never
authority. Labels reject leading/trailing whitespace, controls, empty text, and
oversize bytes; notes are bounded and reject unsafe controls while retaining
intentional newlines/tabs. There is no implicit Unicode, locale, source, tag,
resource, unit, or timezone guess.

The draft constructor and deserializer run the same validation, and
`revision_hash()` domain-separates its canonical content. Mutable draft handles,
numeric expected revisions, principals, and idempotent request ids belong to K2
Store/Action rows; they are not smuggled into this immutable value. Likewise,
the draft has no event uid, applied Fact, correction chain, or quantity effect.

`EconomyFlowTotals` is the pure server-side fold primitive for one explicitly
selected scale/unit. It retains positive `gains`, positive `losses`, signed
`net = gains - losses`, and exact gain/loss/event counts. Adding an event
requires the exact same scale and unit and uses checked `i128` arithmetic.
Different resource units/scales are separate accumulator keys and a mismatch is
an error, never conversion or summation. Deserialization rechecks the algebraic
and count invariants, so a cached/transported projection cannot claim totals
that disagree with its components. Visibility filtering and civil-month
bucketing must happen before this fold in E0's query layer.

**K1.8 exit gate:** constructor and hostile-serde tests reject zero/negative
magnitudes, wrong reference kinds, malformed metadata, unit/scale mixing,
overflow, false total algebra, and count drift. Gain/loss signed deltas, draft
hashes, correction-ready metadata, exact totals, serde round-trips, and complete
wire vocabulary have golden fixtures. This completes K1; K2 may persist and
authorize these values but may not redefine their arithmetic.

**K1.8 implementation status:** complete. Economy magnitude, direction-bound
signed delta, source/tag/note/cause/capture provenance, immutable draft hashing,
and atomic exact flow totals are exported from `nucleus::karma::economy`.
Validated fields are private after construction; serde revalidates hostile
input; semantic duplicate tags and inconsistent transported totals fail closed.
K0–K1 are now complete.

##### K2.1 durable Program handles and revisions

The first K2 slice persists Programs before introducing automatic execution.
A Program handle is a `RecordKind::Program` Record plus one `karma_program`
sidecar. The handle owns mutable `handle_revision`, lifecycle status, immutable
head revision hash, optional active revision hash, optional owner-person
identity, and timestamps. `record.quantity` mirrors activation only (`0` or
`1`) for existing Record tooling; it is not the Program's semantic state and is
changed in the same transaction as the sidecar and evidence Fact.

`karma_program_revision` rows are immutable and content-addressed. Each stores
the canonical AST JSON, canonical Program DSL, complete Proof JSON/status, and
creation time. Loading re-deserializes all three, recomputes the Program hash,
reformats the DSL, and recomputes Proof; corruption is an error rather than an
accepted cached definition. Rejected Proof revisions may be stored as editable
draft heads but can never become active. Revising an active handle changes only
its head; activation of that new accepted revision is a separate expected-
revision mutation, so editing cannot silently replace the code currently used
by an occurrence.

Create, revise, activate, and pause repository commands take a globally unique
bounded `request_id`. `karma_program_request` stores a canonical payload hash,
action, expected/result handle revisions, selected definition hash, and linked
Fact. Exact replays return the prior result/Fact. Reuse of the same request id
with different payload is a conflict. Handle updates use one compare-and-swap
SQL statement; a miss returns the current revision without partially inserting
an immutable revision, request, Fact, or Record change.

Every committed command appends a typed `ProgramMutationEvidence` Fact in the
same SQLite transaction. Definition-only mutations use delta zero; first
activation uses `+1`, pause uses `-1`, and switching accepted active revisions
uses zero. The Fact freezes prior/new head and active hashes, request id, action,
handle revision, and actor; a signing callback may attach the current Trust
signature before commit. The request row links that Fact, making idempotent
results reconstructable after restart. Owner is optional in K2.1 and means
local/private by default; sharing and grants are added by later K2 slices.

**K2.1 exit gate:** migration from every prior schema succeeds; create/revise/
activate/pause survive reopen; rejected revisions store but cannot activate;
canonical rows reconstruct exactly; stale compare-and-swap and request replay
are deterministic; request collision, slug collision, cross-Program revision
activation, corrupt rows, and transaction failure leave no partial mutation;
Fact chain, Record activation, sidecar, and request result agree.

**K2.1 implementation status:** complete. Programs now use immutable canonical
revisions behind revisioned Record handles. Create, revise, activate, and pause
are atomic, compare-and-swap guarded, request-idempotent commands with durable
original result snapshots and signed optional `ProgramMutationEvidence` Facts.
Repository loads independently verify AST, DSL, Proof, hashes, ownership, and
activation invariants; reopen, corruption, rejected-proof, stale-write,
cross-handle, request-collision, and activation accounting tests freeze the
behavior. All Karma command families reserve request ids in the same immutable
global namespace before writing their family-specific result journal, so a
cross-family collision rolls back atomically. The complete mutation vocabulary
has a Nucleus golden fixture.

##### K2.2 durable Frequency handles, revisions, and activation epochs

A Frequency uses three deliberately separate durable identities. Its mutable
handle is a `RecordKind::Frequency` Record with a `karma_frequency` sidecar and
compare-and-swap `handle_revision`. A `karma_frequency_revision` is the
immutable, content-addressed authored schedule: canonical `FrequencyAst`, its
canonical DSL projection, and a default compilation. A
`karma_frequency_activation` is an immutable runtime epoch: it freezes the
chosen definition revision, complete effective parameter map (defaults
included), effective-parameter hash, compiled schedule, previous activation
hash, activating handle revision, cause, and logical activation time. Later
cursor and occurrence rows must name the activation hash, never merely the
mutable Frequency handle.

This split is required for deterministic parameter changes. Revising an active
Frequency changes only its head; the old revision and activation continue to
govern scheduling until an explicit activation. `set-parameters` compiles a
complete replacement override map against the currently active definition and
creates a new activation epoch. `reset-parameters` does the same with the
definition defaults. Neither command mutates the authored revision or an old
epoch. Activating a different revision also creates an epoch. A semantically
identical activation while already active is rejected as a no-op so every
committed handle revision has observable meaning. Pausing clears the handle's
active revision/epoch but retains its `latest_activation` chain pointer;
reactivation creates a new epoch linked across the pause. No old epoch is
erased, and history is reconstructed from immutable epochs and Facts.

The Store owns six request-idempotent compare-and-swap commands: `create`,
`revise`, `activate`, `set-parameters`, `reset-parameters`, and `pause`.
`activate` accepts a revision hash and a complete override map; parameter
commands require an active epoch and retain its definition revision. Every
request fingerprint includes action, Frequency uid, expected handle revision,
selected definition, complete overrides, owner, and actor as applicable.
`karma_frequency_request` retains the canonical original handle result and
linked evidence Fact, so replay after arbitrary later mutations returns the
original snapshot. A stale command writes no revision, epoch, request, Fact, or
Record change.

Definition insertion compiles with defaults before SQL is touched. Repository
loads independently deserialize the AST, parse and reformat the DSL, recompute
the revision hash, recompile the defaults, and compare the stored default
compilation byte-for-byte. Epoch loads recompile the named revision using the
stored effective map as explicit overrides and compare the parameter hash and
compiled schedule byte-for-byte. This detects database corruption and compiler
drift at the boundary. Calendar compilations retain the exact timezone
provider, version, and digest already embedded in the AST; later activation
admission must additionally prove that artifact is locally available before it
may arm a cursor.

Each committed command appends a typed `FrequencyMutationEvidence` Fact in the
same transaction. It freezes previous/new head, active definition, activation,
effective-parameter hashes, handle revisions, action, request, actor, and
logical activation time. Definition-only changes use Record/Fact delta zero;
first activation uses `+1`, switching definitions or parameters uses zero, and
pause uses `-1`. `record.quantity` remains only the existing activation mirror.
An activation epoch is configuration, not execution: K2.2 creates no timer,
thread, poll loop, cursor, or occurrence.

The later shrink-to-fit scheduler consumes active epoch rows through a change
feed keyed by activation hash and required timer resolution. A 3ms epoch can
therefore arm a high-resolution scheduling shard while an unrelated five-hour
epoch remains represented only by its own exact next deadline in a coarse
shard or operating-system timer. There is no global minimum interval, fixed
day/hour threshold, generated Rust loop, or scan of all Frequencies at the
fastest cadence. K2.2's immutable epoch boundary is what makes that dynamic
resource allocation safe to implement in K2.3.

**K2.2 exit gate:** all six commands survive reopen and replay exact original
results; definition and epoch canonical forms independently reconstruct;
revision activation, parameter replacement/reset, active-head divergence, and
pause preserve immutable history; stale writes, request collisions, cross-
Frequency revisions, no-op epochs, invalid overrides, corrupt DSL/AST/default
compilation/epoch compilation, and transaction failures leave no partial
state; Record quantity, handle, epoch chain, request, and Fact evidence agree.

**K2.2 implementation status:** complete. Frequency Records now expose durable
revisioned handles; authored definitions and effective activation epochs are
separate immutable content-addressed objects. Create, revise, activate,
set/reset parameters, and pause are atomic CAS/idempotent commands with exact
request-result replay and typed evidence Facts. Epoch payloads self-validate
their full effective parameter map/hash, and Store loads additionally recompile
the named definition. Program and Frequency commands share the global Karma
request-id namespace. Active edits, no-ops, invalid overrides, pause/reactivate
chain continuity, stale writes, cross-handle revisions, corruption, reopen,
Record quantity, and wire vocabulary are covered by deterministic tests. No
scheduler work happens in K2.2.

##### K2.3 durable cursors and a shrink-to-fit deadline dispatcher

K2.3 turns active Frequency epochs into exact cursor state without introducing
a global tick. One `karma_schedule_cursor` row exists per active activation
hash. It freezes the last intended boundary, next intended boundary, cursor
revision, lifecycle (`armed`, `leased`, `paused`, `superseded`, or `failed`),
lease fencing token/expiry, last occurrence sequence, and last error. Elapsed
cursors use the already-proven `ScheduleCursor`; calendar cursors retain their
requested civil boundary plus resolved instant/discontinuity evidence. Cursor
creation, replacement on a new activation, and pause/supersede are driven from
the Frequency mutation journal, not inferred by scanning all Records.

The in-process dispatcher is a deadline index, not a cadence loop. Its primary
key is `(next_wake_at, required_resolution_ms, activation_hash, cursor_revision)`.
An indexed min-heap provides the next host wake. Secondary resolution lanes are
created only for armed work and contain handles into that heap; they are
admission/accounting domains, never independent polling threads. A lane's
resolution is the minimum explicitly requested resolution among its members,
but each member retains its own absolute wake deadline. Adding a 3ms cadence
therefore arms its next exact boundary in a 1ms-capable lane; a five-hour
cadence retains one absolute five-hour deadline and is not visited on the 3ms
wake. Removing the last high-resolution member destroys that lane and releases
its host timer/resource grant.

There are no hard-coded daily, hourly, or millisecond buckets. Lane selection
uses an ordered set of resolutions actually present plus host capabilities and
Trust resource grants. The planner may coalesce only inside each occurrence's
explicit `[intended_at, intended_at + coalesce_window]`; it must never round a
deadline merely to fit a lane. If the platform timer cannot satisfy
`required_resolution` and `max_lateness`, activation admission follows the
Frequency's `OverloadPolicy`: reject, pause-and-ask, or use only a degradation
already bounded by an active grant. “Generate higher-frequency Rust checks” is
explicitly forbidden: runtime data adds/removes heap entries and host timer
registrations, not code or permanent loops.

At a host wake the dispatcher pops only entries whose arm window is reachable,
then asks the Store to claim each exact `(activation_hash, cursor_revision)`.
The claim transaction verifies the Frequency still names that activation,
advances `armed -> leased`, increments a monotonically increasing fencing
token, and sets a bounded lease expiry. Stale heap entries, superseded epochs,
and duplicate workers lose the compare-and-swap without an occurrence. After
pure schedule advancement, one transaction appends the occurrence intent and
new cursor state, then clears the lease. A crashed worker leaves no ambiguous
commit: an expired lease can be reclaimed with a higher fencing token, while a
committed occurrence's unique `(activation_hash, sequence)` prevents replayed
side effects.

The planner is pure and clockless. It accepts an ordered snapshot of armed
entries, host timer capabilities, the active resource grant, exact persisted
per-entry demand, and exact aggregate capacity; it returns ordered
admissions/rejections and a deadline index whose minimum is the next host-timer
request. Logical `now` belongs to cursor advancement and persisted admission
diagnostics, not resource arithmetic. The async runner uses an injected
`DeadlineClock`: its production Tokio adapter waits against a monotonic instant,
projects that elapsed duration onto the wall-clock observation, and reports a
typed `ClockDiscontinuity` if the two differ beyond an explicit tolerance.
The director rebuilds once at that boundary. Wall-clock jumps, suspend/resume,
and restarts are therefore handled by the Frequency's missed and inactive-gap
policies during advancement, never by assuming a loop ran while the process
slept. Deterministic simulation supplies the same port with a manual clock and
advances directly to the next deadline without real sleeping.

The runtime builds that plan only at boot or after an explicit directory-change
notification. A normal firing removes one due entry, completes its fenced Store
transaction, and reinserts only the returned next revision into its existing
admitted lane. It must not requery or replan unrelated registrations. If a
claim is already leased by another worker, the contender records the exact
lease expiry as an operational recovery arm; at that one instant it fences the
expired lease and rebuilds once. This is not a retry interval. A successful
mutation publishes a lossless watch revision after commit, so activation,
parameter, pause, provider, capability, and grant changes cannot be missed
between directory snapshots.

Frequency activation is a control-plane transaction, not a raw Store call from
an interface. The Engine prepares the exact candidate epoch/cursor, plans the
candidate together with all currently armed work under one host-capability and
resource-grant snapshot, then commits only the admitted result. A
`reject_activation` failure leaves no active epoch or cursor. `pause_and_ask`
may commit an explicitly paused cursor with typed admission evidence;
`degrade_within_grant` may commit only the precise bounded degradation returned
by the planner. The same Engine boundary publishes the mutation Fact,
materializes/supersedes the cursor, and increments the directory-change
revision. Direct Store functions remain persistence primitives for recovery and
tests, not the human/agent Action contract.

**K2.3 exit gate:** pure planning proves that 3ms and five-hour entries retain
independent deadlines and that dispatcher work is proportional to due/changed
entries, not fastest cadence times all schedules. Heap insertion/removal,
coalescing, host capability admission, stale entries, lease fencing/expiry,
restart, pause, activation replacement, duplicate workers, missed policies,
overflow, and occurrence uniqueness are deterministic. A restart integration
test must reopen SQLite, rebuild the heap from one indexed armed-cursor query,
and produce the same next wake and occurrence sequence without scanning on any
periodic tick.

**K2.3 implementation status:** complete; the durable tickless kernel, exact
resource admission, clock boundary, and Engine control path are complete.
Nucleus has a pure dynamically-laned deadline planner, lazy-fenced min-heap,
elapsed and pinned-provider calendar advancement, content-addressed occurrence
envelopes, and reduced-rational `ScheduleDemand`. Demand separately reserves
semantic ticks, timer wakes, calibrated scheduler CPU, evaluator fuel, writes,
effects, and trace bytes per second; arithmetic overflow fails closed. Calendar
demand requires the pinned provider artifact to attest a conservative minimum
interval for the complete rule, rather than guessing from labels such as daily
or monthly.

Store persists exact elapsed/calendar cursors, their canonical demand,
admitted lane resolution/degradation/time, typed calendar or admission pauses,
indexed armed deadlines, fenced expiring leases, and immutable occurrence
advances. A cursor without a matching durable admission record is deliberately
unclaimable. Admitted activation/retuning installs and supersedes its cursor in
the same transaction; `reject_activation` rolls the whole command back,
`pause_and_ask` stores a typed paused cursor, a new candidate cannot evict an
incumbent, and pause atomically supersedes its active cursor.

The one Cell-wide Engine director admits elapsed and every registered tzdb
revision under one grant, rebuilds only at boot, a lossless change wake, lease
recovery, or a typed clock discontinuity, rearms only the completed cursor during
normal operation, and uses persisted lease expiry as an exact crash-recovery
wake. Its `DeadlineClock` port has a Tokio monotonic/wall mapping and supports
manual deterministic clocks. Tests cover the 3ms/five-hour exact rational sum,
independent virtual-time firing at precisely 3ms, a dormant five-hour wait,
typed discontinuity wakes, runtime dense/sparse no-reread invariant, lane
destruction, stale heap entries, activation replacement, automatic live wake,
atomic admission rejection/pause, calendar completion, early/duplicate claims,
abandoned-lease restart recovery, stale-worker loss, unique sequences, and exact
Store reopen reconstruction.

The production timezone provider now loads a bounded canonical transition
artifact only at its exact content address, resolves gaps/folds without host
state, and supplies conservative schedule-specific rate attestation. Multiple
dedicated lane arms remain a measured host-backend optimization: the current
one exact minimum one-shot still visits only due heap entries and never scans
sparse schedules at a dense cadence. Dedicated arms must stay behind the same
interface and may be added only where latency/energy measurements justify them;
they are not part of the semantic K2.3 exit gate.

##### K2.4 typed Karma Actions and Protein projection

K2.4 exposes the K2 repository through the same human/agent boundary used by
the rest of Lince. The typed Action vocabulary is
`create-karma-program`, `revise-karma-program`, `activate-karma-program`,
`pause-karma-program`, `create-karma-frequency`, `revise-karma-frequency`,
`activate-karma-frequency`, `set-karma-frequency-parameters`,
`reset-karma-frequency-parameters`, and `pause-karma-frequency`. Create carries
the complete typed AST and optional owner; revise carries the uid, expected
handle revision, and replacement AST; activation carries the selected content
hash and expected handle revision; parameter Actions carry the expected active
revision hash and complete override map. Every payload carries a required
idempotent `request_id`. The authenticated Action/session actor is the sole
authorship source and is passed into mutation evidence; payloads do not carry a
second spoofable actor field.

Program Actions call the same CAS/idempotency Store commands proven by K2.1.
Frequency Actions call the K2.2/K2.3 Engine control plane, never legacy
`store::freqs`. Activation and retuning require an installed immutable
`KarmaDeadlineDirectorConfig` snapshot so host timer capabilities, aggregate
grant/capacity, workload calibration, clock, and pinned timezone providers are
the exact values used by atomic admission. Absence of runtime configuration is
a typed fail-closed error. A committed mutation returns its object uid and Fact;
an identical request replay returns the same object without republishing the
Fact, and a stale expected revision becomes `karma_stale_handle_revision` with
the current revision in the safe message.

The read half is `source:"karma"`, a heterogeneous, deterministic Protein
union. Rows use `object_kind` for `program`, `program_revision`, `frequency`,
`frequency_revision`, `frequency_activation`, `schedule_cursor`, and
`schedule_occurrence`. Immutable rows expose canonical AST/DSL/hash or
epoch/batch data, and cursor rows expose exact demand, admission diagnostics,
lease state, and next intended boundary. In K2.4, handle capability booleans
mean only that the durable object state makes an Action structurally
submittable; stable blockers explain states such as `program_not_active` or
`head_already_active`, and Frequency rows explicitly state
`requires_runtime_admission:true`. They do not predict admission or grant
authority. The Action boundary recomputes runtime admission, and K5 will add
principal/grant-specific capability projection without weakening that check.
Filters never infer an Action: the UI copies a provided typed Action template,
adds a new request id, and submits it through the ordinary Action path. K2.4 is
complete only when Protein can reconstruct every durable K2 object and causal
hash link, unsupported predicates fail with stable codes, and remote visibility
is deny-by-default until K5's fine-grained Karma grants exist.

**K2.4 implementation status:** the Program/Frequency mutation and read slices
are complete. All ten typed Actions route through Engine to the CAS/idempotent
K2 repositories; Frequency activation/retuning additionally crosses K2.3
admission and cannot create a legacy `frequency` row. Identical request replay
does not republish evidence, stale handles return
`karma_stale_handle_revision`, and missing runtime configuration returns
`karma_runtime_unconfigured`. `source:"karma"` locally projects all seven
object families in stable order, emits action templates and durable blockers,
supports recursive `all`/`any`/`not` plus kind/uid/slug/status filters and
bounded ordering/limit, rejects unrelated predicates/includes/aggregates with
stable codes, and returns no rows to remote subjects. Integration tests traverse
Action → admitted cursor → occurrence batch → Protein without touching the
legacy timer table.

##### K3.1 generic occurrence ingress and Cell ordering

K3 begins by separating a semantic occurrence from the order in which one Cell
accepted it. `KarmaOccurrenceEnvelope` is immutable and content-addressed. Its
source union starts with `schedule-tick` (one segmentation-independent
`SemanticScheduleTick`) and `schedule-coalesced` (one explicit aggregate over
an `OccurrenceBatch`); later variants add Fact, Signal, sync, workflow, receipt,
and manual evidence without changing schedule identity. The envelope freezes
`logical_at`, source identity, optional causal parent occurrence, and the typed
source payload. Its hash excludes Cell sequence and receipt time: importing the
same evidence twice therefore deduplicates even if it arrives through different
threads or after restart.

Store owns one transactional Cell sequence counter. Ingress first checks the
source-kind/source-identity uniqueness boundary; an identical canonical
envelope returns the original row, while the same source identity with changed
payload is a protocol conflict. Only a genuinely new envelope increments the
counter and receives the next positive `cell_sequence`. The row projects source
kind, source identity, logical instant, and parent hash from canonical JSON and
revalidates every projection plus content hash on load. Rows and assigned
sequences are immutable. This recorded sequence is the authoritative replay
order for genuinely concurrent external arrival; deterministic internal
producers must submit their already-sorted identities in one transaction.

Schedule occurrence batches are not themselves silently treated as Program
runs. K3.2 adds a durable expansion cursor keyed by the immutable schedule
occurrence hash. `individual` batches emit their semantic ticks in schedule
ordinal order through bounded pages; `coalesced` batches emit one aggregate
occurrence and never individual ticks. Advancement of that expansion cursor and
generic occurrence insertion share a transaction. A crash can repeat the page
request but cannot skip or duplicate a tick. Elapsed and calendar sources have
separate typed tick and coalesced payloads; every source identity is derived
from the semantic boundary or aggregate, never its page, wake, or arrival
metadata.

Expansion is cooperative work, not another polling timer. Runtime configuration
sets two non-zero bounds: semantic items per page (hard-capped by the wire
protocol at 4096) and source batches per recovery turn. A deadline completion
attempts one page immediately. Boot/rebuild performs one recovery turn, and the
director processes further turns only while a persisted incomplete cursor
exists and no deadline is currently due, yielding between turns. Consequently
a five-hour schedule does not create millisecond polling, while a large replay
cannot monopolize the same director that protects a three-millisecond deadline.
The durable pending predicate, rather than a guessed wall-clock interval,
decides whether expansion work exists.

The occurrence sequencer then
leases strictly by `cell_sequence`; Program selection freezes the active
revision set before evaluation, records one run per `(occurrence, program
revision)`, and completes reaction work before any learning occurrence.

**K3.1 implementation status:** complete. Canonical occurrence wire/hash tests
and SQLite insert/replay/collision/reopen tests protect the immutable Cell
sequence contract. It performs no Program evaluation or Action.

**K3.2 implementation status:** complete. Durable cursor and atomic
elapsed/calendar expansion, paged individual and single aggregate behavior,
always-on cooperative recovery, and restart proof are implemented. Deadline
work is prioritized without polling, while expansion and Program turns share
background progress fairly.

##### K3.3 frozen Program epochs and no-effect runs

The occurrence processor owns a single durable `next_cell_sequence`; it never
selects a later occurrence while an earlier one is incomplete. On first seeing
an occurrence it opens a short transaction, snapshots every active
`(program_uid, revision_hash)` in UID order, content-addresses that immutable
selection as a Program epoch, and commits it before evaluation. Later Program
activation, revision, or pause therefore cannot change which revision the
occurrence saw. A page cursor within the epoch advances atomically with each
immutable run, and epoch completion advances the Cell occurrence cursor. Empty
epochs are valid and advance without manufacturing a run.

Every epoch member receives exactly one typed terminal run: `succeeded`,
`not-applicable`, `blocked`, or `evaluation-failed`. For schedule occurrences,
Frequency trigger nodes receive frozen boolean pulses after resolving the
activation to its immutable Frequency UID; the Program runs when at least one
such trigger matches, while all other trigger nodes receive `false`. A Program
without a matching trigger is durably not-applicable rather than silently
absent. Until durable Program state lands, any delay/control-state operation is
blocked before evaluation so a state transition can never be calculated and
then discarded. Missing adapters and deterministic evaluator failures are
terminal, inspectable results for that Program and do not poison later members
or occurrences.

A successful K3.3 run stores the sealed pure-evaluation replay capsule already
defined in K1.7, including the exact Program AST, frozen context, limits, trace,
outputs, fuel and hashes. The run hash excludes persistence time and includes
the Cell sequence, occurrence, frozen epoch, Program identity/revision and
outcome. No candidate is authorized, no state update is applied, and no effect
or Action is executed in this phase. Runtime turns bound both occurrences and
Program members; durable demand drives continuation without a heartbeat.

**K3.3 implementation status:** complete for the no-effect slice. Typed epoch
and run wires, migration, store processor, Frequency trigger projection,
stateful fail-closed gate, replay capsules, Protein rows, and cooperative
director turns are implemented. Tests freeze two revisions, activate a third
mid-epoch, and obtain exactly one replayable success plus one explicit
not-applicable terminal run.

##### K3.4 restart and ordering proof

The release proof interrupts processing after one member of a multi-member
epoch, closes the database, reopens it, changes the current active Program set,
and resumes. The old occurrence must finish its previously frozen members
before the next `cell_sequence` can freeze a new epoch; the next occurrence must
see the new active set. Persisted runs must order lexicographically by
`(cell_sequence, member_ordinal)`, keep one row per occurrence/revision, and
retain identical hashes after reopen and replay verification. Evaluation
failure and not-applicable are terminal for ordering purposes, while storage or
integrity failure stops advancement.

Because K3 has no reaction producer yet, a no-effect run must not append a
child occurrence, Fact, candidate, intent, Action, receipt, or transfer. The
proof snapshots those counts around processing. This negative assertion is
important: later K4/K5 phases must add each reaction through an explicit
outbox/ingress boundary rather than gaining mutation as an accidental evaluator
side effect.

**K3.4 implementation status:** complete. The file-backed release proof resumes
a partially processed epoch, preserves the old revision set, gives the next
occurrence the newly active set, verifies strict Cell/member order and stable
run hashes after a second reopen, and proves Fact/occurrence/intent/transfer
counts do not change. A deterministic division-by-zero run is terminal and the
following Cell sequence still completes.

##### K4.1 durable synchronous Program state

K4 first removes K3's stateful fail-closed gate; it does not start with
candidates or effects. Each frozen Program epoch member includes the Program's
activation handle revision as well as its content revision. This is the stable
activation-generation token needed by `on-program-activation` reset policy and
cannot be inferred later from the mutable handle.

Program-persistent delay/control state uses an immutable event chain plus one
CAS projection per `(program_uid, node_id)`. An event records state revision,
previous event hash, source run hash, definition revision, activation handle
revision, optional reset reason, and either a typed delay/control value or an
explicit reset tombstone. The current projection repeats and revalidates those
fields for efficient context assembly. Event hashes exclude database time.
Run insertion, every state event/projection CAS, the epoch member cursor, and
the Cell processing cursor commit in one transaction. A run can therefore
never become visible without its synchronous read-old/write-next state, nor can
state advance for a run that is retried.

Before evaluation, the runner compares each current node state with its frozen
member. `on-program-activation` resets on activation-generation change;
`on-revision-change` resets on definition change. Otherwise revision changes
follow `migration`: `reset` starts from the node's declared initial state,
`require-explicit` produces a terminal blocked run, and
`compatible-type-only` carries state only when the node operation/state kind
and exact value type remain compatible. `never` and `manual` preserve state
subject to that migration check. Reset and a newly staged value may be recorded
in one event carrying the reset reason, so audit history shows that evaluation
read the initial state. A reset with no staged update remains an explicit
tombstone rather than resurrecting old state later.

This slice supports `persistence:program`. Workflow and model-checkpoint state
remain terminal blocked outcomes until their owning runtimes can supply the
correct scope key; silently treating them as Program state would merge
independent workflows/models. Late-event `ignore`/`reject`/replay-required
semantics remain those of the pure evaluator because the durable context now
supplies its exact previous state and logical timestamp.

**K4.1 implementation status:** complete for Program persistence. Cooldown state
is proven across three occurrences and a database reopen, including exact
boundary acceptance and a three-event hash chain. A new activation generation
resets the window and records `program-activation` on the state event. Run,
state events/projections, epoch cursor, and Cell cursor are one transaction;
other persistence scopes block explicitly.

##### K4.2 inert candidate materialization

After a successful evaluation, the runner scans stable node-trace order and
port-name order for value-bearing candidate datums. Each becomes a typed,
content-addressed proposal keyed by source run, node and output port, carrying
the occurrence, Program revision, route, template and exact ordered fields.
Missing candidate datums create no row. Duplicate `(run,node,port)` production
is an integrity conflict, never last-write-wins.

Candidate insertion shares the run/state transaction. Its initial lifecycle is
always `proposed`, including route `act`: route expresses desired downstream
handling, not authority. K4.2 has no candidate mutation, decision, grant,
intent, Action, or effect worker. Protein exposes the immutable proposal so the
Karma sand can render the first real terminal flow node; K4.3 adds reviewed
human/agent controls and K5 decides whether any accepted proposal may become an
intent.

**K4.2 implementation status:** complete. Trace scanning materializes
value-bearing candidate datums in stable node/port order. An `act`-routed test
produces one `proposed` row sharing the run transaction and proves that Facts,
signed intents, transfers, and child occurrences remain unchanged.

##### K4.3 candidate review control

Candidate review is an event-sourced CAS handle independent from the immutable
proposal. `respond-karma-candidate` requires a globally idempotent request id,
candidate hash, expected state revision, and one typed response: `accept`,
`dismiss`, or `snooze(until)`. The actor is always taken from the authenticated
Engine session, never trusted from the payload. Snooze must name a canonical
future logical instant. Reusing a request id with different content conflicts;
stale state returns the current revision without appending anything.

Each committed response appends an immutable candidate-state event, advances
the small current projection by CAS, and appends a zero-delta audit Fact to the
owning Program in the same transaction. Review is deliberately reversible:
later responses may move an accepted, dismissed, or snoozed candidate again,
because no user/agent decision is treated as read-only history; the event chain
retains every change. Repeating the same status is allowed only through exact
request replay, avoiding meaningless new revisions.

Protein candidate rows expose current state revision, status, snooze instant,
actor/event provenance, capability booleans, blockers, and complete Action
templates. Accepting an `act` route still creates no intent. K5 alone may
translate a reviewed candidate into separately authorized work after grant and
budget checks.

**K4.3 implementation status:** complete for accept/dismiss/snooze review.
Candidate state and immutable response events, global request replay, stale CAS,
reversible transitions, typed actor validation and attribution, owning-Program
audit Facts, typed Engine Action, and Protein capabilities/templates are
implemented. Tests accept, dismiss, and snooze the same `act` proposal, prove
exact replay publishes no second Fact, prove a stale response appends nothing,
and keep occurrence/intent/transfer counts unchanged. No candidate-to-intent
path exists before K5.

##### K5.1 signed delegation-grant boundary

The first K5 slice establishes authority as durable data without yet creating
an intent or executing an effect. A delegation grant is a Record handle with an
immutable, content-hashed revision. Its principal is always the authenticated
Person whose installed key signs the revision; it is not accepted as an Action
payload field. Creation is disabled, activation is a separate expected-revision
Action, and revocation clears the active revision immediately. A revoked handle
cannot be resurrected; creating a replacement makes renewed consent explicit.

`DelegationGrantSpec` scopes one named Program, either its exact revision or
whichever revision is active at the later authority check. It contains a
non-empty capability set, an explicit candidate-template scope (`any` or a
non-empty exact set), an explicit target scope (`any` or a non-empty typed exact
set), purpose, and a `[valid_from, expires_at)` interval. Target atoms retain
their semantic kind—Record, concept, Person, Organ, place, or controller—so a
matching string in the wrong namespace cannot authorize an Action. Grant
management capabilities cannot themselves be delegated through these grants;
human/session authorization for grant creation and narrowing remains a
separate boundary and delegation is therefore non-transitive in K5.1.

The only revision mutation in this slice is `narrow-karma-grant`. The pure
comparator must prove the replacement is a subset: capabilities and exact sets
may only lose members, `any` may become an exact set, an any-active Program
revision may become one exact revision, `valid_from` may move later, and expiry
may move earlier. A different exact Program revision, changed Program or
principal, newly added capability/target/template, longer validity, or mixed
narrow-and-widen edit is rejected. If the handle is active, a proven narrowing
becomes active atomically; no old wider revision remains live between commits.

Authority evaluation receives a frozen typed request containing principal,
Program and revision, candidate template, capability, optional typed target,
and logical instant. It evaluates one explicitly named active grant revision;
the engine never unions all matching grants. The result is a structured list of
stable denial reasons (missing/inactive/revoked, not-yet-valid/expired,
principal/Program/revision/template/capability/target mismatch). Absence of a
grant or any mismatch is denial. Both candidate policy and the eventual domain
Action boundary will invoke this same evaluator; K5.1 exposes it and its proof,
but deliberately has no candidate-to-intent bridge.

Persistence uses `karma_grant`, `karma_grant_revision`, and globally
idempotent request rows. The revision row stores the principal key id and
detached signature over the revision hash; the lifecycle Fact is signed by the
same Person in the same transaction. Handles use CAS revisions, immutable
revision rows reject update/delete, and request replay must reproduce the exact
stored handle and Fact. Protein exposes grant handles and revisions, current
signature provenance, capabilities/blockers, and ready-to-fill narrow,
activate, and revoke Action templates. Budget limits, reservations, Automation
Trust, intents, and workers remain later K5/K8 layers and cannot be inferred
from the presence of an active K5.1 grant.

**K5.1 implementation status:** complete. The pure authority kernel, the
append-only `karma_grant` persistence, the typed Engine Actions, and the Protein
projection are implemented and tested.

Three decisions were settled while landing it. First, the principal is derived
from the installed signing key, never from a payload: the store only accepts a
revision whose signature names the principal, so the Engine resolves the Person
from `trust::Signer` and refuses when no key is installed. An authenticated
session bound to a different Person is refused rather than allowed to borrow the
Cell's key. Second, a grant Record's quantity tracks *live* authority the way a
Program's quantity tracks live activation: the Fact delta follows the
transition, so revoking a draft that never authorized anything moves nothing.
Third, grant Actions are gated on the existing `karma:create` and `karma:update`
permissions; no new permission key was added, because the separateness the
contract demands already comes from key-derived principals plus the kernel's
refusal to place `KarmaGrantNarrow`/`KarmaGrantWiden` in any spec.

Tests prove the create → activate → narrow → revoke lifecycle with authority
read back from storage at every step, that narrowing an active grant swaps head
and active in one commit while narrowing a draft moves the head without ever
making it live, that a revoked handle can be neither narrowed nor
reactivated, that exact replay publishes no second Fact, that a stale
expectation writes nothing and returns the shared
`karma_stale_handle_revision` conflict, that revision and request rows reject
update and delete, that an unsigned or wrongly signed mutation is refused, and
that every authority dimension denies on its own alongside
missing/inactive/revoked. One test asserts the whole lifecycle leaves the
candidate, occurrence, run, and transfer tables untouched: authority exists and
still causes nothing. Protein exposes `grant` and `grant_revision` rows with
signature provenance, capabilities/blockers, ready-to-fill narrow/activate/
revoke templates, and an explicit `authorizes_effects: false`. There is
deliberately no widen Action, no candidate-to-intent bridge, and no worker.

Two conditions found while landing this slice, both pre-existing and left
untouched: `promise_lifecycle_through_actions` fails on the transfer WIP's
"trusted local transfer action requires an acting Person" check, and the debug
build of the Action dispatcher needs more than the default 2 MiB test stack,
now raised in `.cargo/config.toml`.

##### K5.2 budgeted authority and inert durable intents

The second K5 slice makes authority *finite* and gives accepted work somewhere to
live, while still executing nothing. A grant gains a budget; accepting a reviewed
`act` candidate becomes the one way an intent is born; and an intent is a durable,
authorized, frozen request that no worker may yet claim.

A budget belongs to the grant revision, because a budget is part of what was
consented to. `GrantBudget` carries an optional lifetime intent cap, an optional
`per_window` count over a fixed duration, and an optional total quantity limit
with its unit. Absent means unlimited, so the narrowing comparator treats `None`
as the widest value: a replacement may lower any limit or add one where none
existed, and may never raise or remove one. Windows are tumbling and anchored at
`valid_from`, so the window a given instant falls in is a pure function of the
revision and replays exactly.

Consumption is counted per grant *handle*, never per revision. If narrowing
reset consumption, narrowing would become a way to refill a spent budget —
an escalation disguised as a restriction. The intent rows are themselves the
consumption ledger: a budget check counts and sums the grant's intents that
still hold their reservation, inside the same transaction that inserts the new
one, so no separate mutable counter can drift from the evidence. Cancelling an
intent releases its reservation.

An intent freezes what was authorized: the source candidate, the exact grant
handle and revision that permitted it, the Program and Program revision, the
capability, the typed target, the candidate template, the frozen typed Action
payload, an idempotency key, a deadline, and the full policy proof (the
authority decision plus the budget snapshot at reservation time). It is
content-addressed and immutable. Its status in this slice is only `authorized`
or `cancelled`; there is no `denied` row, because a denial refuses the whole
acceptance instead of recording a dead intent, and there is no lease, attempt,
receipt, or executed state, because nothing may run yet.

The intent's lifecycle is stored the way Program state and candidate review
already are: one immutable, per-intent hash-chained transition log plus one
current projection that must match its head. A transition names the durable
request that caused it rather than a Fact, because one cause legitimately moves
many intents — revoking a grant cancels everything it authorized — and the Fact
for that cause is reachable through the request rather than copied onto each
row. Cancellation is therefore written after the causing request row exists, so
no transition can cite a cause that was not recorded first.

Which states reserve budget is a fact the database states once. A seeded status
table carries `holds_reservation` and every budget query joins it instead of
naming statuses, so E0.3's `leased`, `dispatching`, and `uncertain` begin
counting against a budget by being seeded rather than by an edit to five `WHERE`
clauses — the omission that would otherwise let a leased intent's reservation be
spent twice. Only the statuses a phase can actually reach are seeded, so a state
this slice must not produce cannot be written at all, and a test walks the table
against the kernel enum so the two can never drift.

Accepting a candidate and authorizing its intent are one commit. The
`respond-karma-candidate` Action gains an optional `authorizing_grant_uid`.
Accepting an `act` candidate without naming a grant keeps K4.3 behavior exactly:
the candidate is accepted and no intent exists. Naming a grant makes the same
transaction re-evaluate the live grant, reserve budget, and create the intent —
and if the grant denies, the budget is exhausted, or the candidate is not an
`act` route, the whole Action fails and nothing changes. A person asking for
authorized work never silently gets an accepted candidate with no authority
behind it. Exactly one grant is named, never a union of matching grants.

Revocation is deterministic rather than raced. Revoking a grant cancels its
still-authorized intents in the same transaction and releases their budget, so
no intent can outlive the consent that created it. Creating an intent appends
its own Fact; cancelling one does not, because the revocation that caused it
already appends a signed lifecycle Fact naming the grant, and every intent it
cancelled is derivable from that. When E0.3 lets a single intent end on its own,
that transition needs its own Fact.

**K5.2 implementation status:** complete for budgets, the authorization kernel,
persistence, the accept-time bridge, and revocation cancellation. Leases,
attempts, receipts, retries, compensation, emergency stop, and any execution at
all move to **E0.3**, which builds that machinery for the reversible-local-data
family so the engine's read → compute → write loop closes before the Economy
product is built on it; everything outside that family stays under K5.3.

Decisions settled while landing it. The intent's amount and target are read from
the stored proposal, never supplied by the caller — a client that could name the
amount could understate it and spend a budget it was never given — and a
proposal carrying two quantity or two reference fields is refused rather than
disambiguated by guessing. `IntentStatus` was already frozen in `state.rs` with
the full lifecycle, so K5.2 reuses it and writes only `authorized` and
`cancelled`; `holds_reservation` states the accounting rule once over the whole
vocabulary, so E0.3's execution states inherit it. K5.2 maps only templates whose
capability is in the `LocalReversibleData` family, checked twice, so a later
template cannot quietly reach further. An unbudgeted grant is omitted from the
wire entirely, which keeps K5.1's golden authority hash and lets revisions
stored before budgets existed still verify.

Tests prove that accepting without a grant stays inert exactly as K4.3 left it;
that naming a grant authorizes one intent, reserves its budget, freezes the
proof, and lands the intent's own Fact in the same commit; that an exhausted cap
refuses the whole acceptance and leaves the candidate `proposed`; that revoking
a grant cancels its intents and releases their budget; that a replayed
acceptance reports the same intent rather than minting a second, while the same
request id against a different grant is refused; and that a draft grant or one
scoped to another template authorizes nothing. The kernel separately proves
budget narrowing in every dimension, deterministic tumbling windows, each budget
denial on its own, and that a denied decision can never be hashed into an
intent.

One K5.1 bug surfaced and was fixed here: `karma_grant_revision` keyed rows by
content hash alone, so two grants carrying byte-identical consent collided — and
the contract's own "creating a replacement makes renewed consent explicit" path
was therefore broken for identical terms. The revision hash identifies consent
*content*; the stored revision is now identified by `(grant_uid, revision_hash)`,
and revision lookups are scoped by grant.

**Karma migrations are edited in place until one ships.** None of 0026–0035 has
ever been committed, so the right schema is written directly rather than stacked
behind corrective migrations that would exist only to fix mistakes no released
database ever saw. The cost is that a local database built from an earlier run
of this branch must be deleted rather than migrated; fresh and in-memory
databases are unaffected. This standing licence ends the moment a Karma
migration reaches a real deployment.

Every declaration has a stable node id. If omitted, the formatter derives it
from the left-hand name and freezes it on first publish. Moving a visual node,
renaming its display label, or reformatting text does not change the semantic
hash. Changing an expression, type, dependency, effect, policy requirement, or
stable id does.

The text is compiled before storage. For example:

    let stock_low: bool = quantity(record:@apple) < 1kg

becomes a typed AST similar to:

    {
      "id": "stock_low",
      "kind": "compare",
      "op": "lt",
      "left": {
        "kind": "record_quantity",
        "record_uid": "r_apple...",
        "type": "qty",
        "unit_uid": "c_kilogram..."
      },
      "right": {
        "kind": "literal",
        "type": "qty",
        "unit_uid": "c_kilogram...",
        "decimal": "1.000"
      }
    }

The AST—not the source string—is the canonical revision payload. The stored DSL
is its human-readable projection, and the visual editor reads/writes the same
AST. Unknown fields or node kinds fail validation instead of being ignored.

An expert textual projection for the apple example could look like this:

    program household.apple.restock revision 4 {
      owner person:@ana
      purpose "Keep the family pantry supplied with apples"
      mode active

      on fact(concept_in: @apple) coalesce by household every 5m
      on schedule @hourly

      input pantry: datum<qty<kg>> = view:@family.apple.inventory freshness 2h
      input outcomes: list<fact> = view:@family.apple.confirmed_consumption window 180d
      input offers: list<transfer> = protein {
        source: transfer,
        open: true,
        concept_in: @apple,
        near: { place: @home, max: 5km }
      }

      learn need: model<recurrence> = recurrence.beta_cadence {
        evidence: outcomes,
        prior: beta(1, 1),
        half_life: 90d,
        cadence: weekly(local_tz),
        min_effective_samples: 5
      }

      let shortage: bool = project.quantity(pantry, at: need.next_window.end) < 0kg
      solve seller: estimate<ref<transfer>> from offers lexicographic {
        require compatible_unit && window_overlap && route_eta <= 25m
        minimize expected_total_cost
        minimize route_eta
        tie_break transfer.uid
      }

      when shortage && need.probability >= 0.72p && need.confidence >= 0.65c {
        recommend "Apples are likely needed this week" dedupe need.pattern_window
        preview transfer.draft_local from seller quantity need.expected_quantity
      }

      when shortage && need.probability >= 0.90p && need.confidence >= 0.85c {
        draft transfer.draft_local from seller quantity need.expected_quantity
        require trust:@apple.known_sellers at draft
        require grant:@apple.local_drafts
      }
    }

The first branch explains and suggests. The second may create an editable local
draft only if the seller matches the named concept/counterparty Trust scope and
that exact Program grant exists. Publishing the proposal, addressing the seller,
agreeing, confirming, or settling are different Trust ceilings/capabilities and
would need explicit nodes and grants. No model or LLM is necessary: the
recurrence model, inventory projection, offer query, and optimizer are
deterministic.
Here `offers` is the visibility-gated Protein view over local and permitted
discovery-cache OPEN Contributions from other Cells; source freshness,
signature status, proximity ceiling, unit, window, and missing route data remain
visible inputs rather than hidden ranking behavior.

### Protein and Action interface

Karma adds one discriminated Protein source rather than one unrelated
query language per feature:

    {
      "source": "karma",
      "where": [
        { "object_kind_in": ["program", "run", "candidate"] },
        { "program_eq": "r_program..." },
        { "status_in": ["active", "queued", "open"] }
      ],
      "order": [
        { "field": "at", "direction": "desc" },
        { "field": "uid", "direction": "asc" }
      ],
      "include": {
        "definition": true,
        "capabilities": true,
        "latest_run": true
      }
    }

`object_kind` selects the typed union. Stable common fields are `uid`, `kind`,
`program_uid`, `revision_uid`, `owner_uid`, `status`, `quantity`, `at`,
`cursor`, `cause`, and `visibility`; kind-specific data lives under a typed
field matching the discriminator. Unsupported predicates/includes are errors,
not ignored filters.

Recommended predicates and includes:

| Interface | Fields |
| --- | --- |
| Program/revision | `object_kind_in`, `program_eq`, `revision_eq`, `owner_eq`, `tag_in`, `status_in`, `active`, `purpose_eq`; include `definition`, `parameters`, `proof`, `diff`, `dependencies`, `capabilities` |
| Occurrence/run | `trigger_kind_in`, `cursor_gte/lte`, `at_since`, `cause_eq`, `status_in`; include `trace`, `inputs`, `candidates`, `policy`, `intents`, `receipts`, `cost`, `replay_capsule` |
| Model/evidence | `model_eq`, `trained_through_cursor_gte`, `drift_state_in`; include `spec`, `checkpoint`, `eligible_evidence`, `rejected_evidence`, `metrics`, `recommendations` |
| Candidate/decision | `candidate_kind_in`, `subject_eq`, `live`, `expires_before`; include `evidence`, `preview`, `alternatives`, `authority_required`, `capabilities` |
| Grant/intent/receipt | `principal_eq`, `capability_in`, `target_eq`, `status_in`, `deadline_before`; include `scope`, `budget`, `policy_proof`, `attempts`, `receipt`, `compensation` |

“Why did this happen?” is one query, not a log hunt:

    {
      "source": "karma",
      "where": [
        { "object_kind_in": ["run"] },
        { "uid_eq": "r_run..." }
      ],
      "include": {
        "trace": true,
        "inputs": { "facts": true, "exclusions": true },
        "model": true,
        "policy": true,
        "intents": { "receipts": true },
        "causal_chain": true
      }
    }

All mutations keep the existing wire mannerism: kebab-case `action` tag,
snake_case fields, engine-derived viewer/principal, `request_id` for replay
safety, and `expected_revision` for mutable handles.

| Typed Action | Main effect |
| --- | --- |
| `validate-karma-definition` | Parse/type-check/canonicalize DSL or AST and return Proof without storing or executing it. |
| `create-karma-program` | Create disabled Program Record plus immutable revision 1 and Proof result. |
| `fork-karma-program` | Create a disabled local Program/revision from a named revision/template, preserving lineage but no grant/live state. |
| `revise-karma-program` | Compile/validate AST and append a new immutable revision; does not activate it. |
| `activate-karma-revision` | Select a proven revision, set/keep Program active, and enqueue an activation occurrence. |
| `create-karma-frequency` / `revise-karma-frequency` | Create a disabled Frequency handle plus immutable schedule revision, or append a later revision; neither arms a timer. |
| `activate-karma-frequency-revision` | Prove/admit one revision, select it, advance generation, and arm its exact next deadline only when an active Program/Signal/workflow consumer exists. |
| `set-karma-parameter` | Append one typed Program/Frequency/model parameter version inside its declared range; effective next occurrence. |
| `reset-karma-parameter` | Return a parameter to its revision-declared default as another parameter version. |
| `deactivate` / `activate` | Pause/resume the Program's universal quantity knob through the existing Fact path. |
| `retire-karma-program` | Deactivate and mark the mutable Program handle retired; revisions/runs remain readable and it may be forked. |
| `run-karma-program` | Enqueue a manual occurrence against current or named revision; `dry_run` forbids effects. |
| `replay-karma-run` | Reproduce or differentially replay one run from its capsule without production effects. |
| `rebuild-karma-model` / `disable-karma-model` | Deterministically rebuild allowed evidence or stop inference/updates while retaining checkpoints. |
| `create-karma-grant` / `narrow-karma-grant` / `revoke-karma-grant` | Change signed delegation data; programs cannot call the widening form for themselves. |
| `create-automation-trust-scope` / `revise-automation-trust-scope` / `activate-automation-trust-revision` | Create/revise/select a local concept/stage/counterparty scope; creation is disabled and widening never activates by implication. |
| `respond-karma-candidate` | Accept/edit/dismiss/snooze/mute a candidate; accepting revalidates and invokes its typed preview Action. |
| existing `decide` | Answer one current decision option with stale-world revalidation; Karma does not create a second decision action. |
| `control-karma-workflow` | Cancel/resume/retry/skip only transitions allowed by the Workflow definition and current capability. |
| `control-karma-intent` | Stage/cancel/retry/reconcile/compensate an intent according to capability and current state. |
| `simulate-karma-program` | Create an isolated replay/projection/scenario/DST run; never applies results wholesale. |
| `import-karma-template` | Validate dependencies/licenses/signature and create an inert local Program/revision with no data binding or grant. |

Creating a program:

    {
      "action": "create-karma-program",
      "request_id": "create-apple-restock-1",
      "slug": "household.apple.restock",
      "head": "Apple restock",
      "purpose": "Keep the family pantry supplied with apples",
      "dsl": "program household.apple.restock { ... }"
    }

The engine parses the DSL, resolves typed refs, validates types/units/graph,
canonicalizes the AST, calculates its hash, and atomically creates:

- a Program Record (`kind=karma_program`, quantity `0`);
- revision 1 (`kind=karma_revision`) containing canonical AST/hash;
- `revision-of` and `owned-by` links;
- a Proof result linked to the revision; and
- one annotation Fact caused by the authenticated actor/request.

The owner/principal is derived from the authenticated session or a separately
verified delegation; the payload cannot impersonate an `owner_uid`. No schedule
is armed, model trained, grant created, or effect allowed merely by creation.

Activating it:

    {
      "action": "activate-karma-revision",
      "request_id": "activate-apple-restock-4",
      "program_uid": "r_program...",
      "revision_uid": "r_revision_4...",
      "expected_program_revision": 3
    }

The transaction re-runs Proof against current schemas/capabilities, selects
revision 4, advances the Program handle revision, moves quantity from `0` to
`1` if needed, appends activation/quantity Facts, and enqueues one activation
occurrence. Missing grants do not necessarily block activation: the Program can
run in observe/suggest mode while gated effect nodes report `authority_missing`.

Creating the dense example Frequency is also an inert, typed definition:

    {
      "action": "create-karma-frequency",
      "request_id": "create-sensor-fast-1",
      "slug": "sensor.fast",
      "cadence": {
        "kind": "elapsed",
        "interval_ms": 3,
        "anchor": "2026-07-21T09:00:00.000Z"
      },
      "timer_policy": {
        "required_resolution_ms": 1,
        "max_lateness_ms": 1,
        "coalesce_window_ms": 0
      },
      "missed_policy": { "kind": "replay", "max": 64 },
      "inactive_gap_policy": "skip_to_next_anchor",
      "overload_policy": "pause_and_ask"
    }

Creation atomically appends a Frequency Record at quantity `0`, immutable
revision 1, Proof/load estimate, and annotation Fact. It creates no
`karma_deadline` row and no runtime timer. Activating the revision first
checks dense/precision capacity and selects it. The dependency registry then
arms exactly one deadline registration if at least one active Program, Signal
poll, or workflow consumes the Frequency; ten consumers referencing it still
share one schedule occurrence. When the last consumer pauses, the transaction
invalidates/removes the registration and notifies the director without
disabling or deleting the reusable Frequency definition.

Tuning a declared parameter:

    {
      "action": "set-karma-parameter",
      "request_id": "slow-reminder-after-recovery-1",
      "target_kind": "frequency",
      "target_uid": "r_reminder_tick...",
      "parameter": "interval",
      "value": { "type": "dur", "milliseconds": 259200000 },
      "rephase": "preserve_anchor",
      "expected_parameter_revision": 7,
      "cause_run_uid": "r_run..."
    }

This appends parameter revision 8 on the target and an annotation Fact, recalculates
`next_intended_at` from the declared rephase policy, and enqueues a
parameter-changed occurrence. The run that requested the tune finishes under
parameter revision 7; no current occurrence is reinterpreted.

A narrow grant is similarly explicit:

    {
      "action": "create-karma-grant",
      "request_id": "grant-apple-local-drafts-1",
      "slug": "apple.local_drafts",
      "program_uid": "r_program...",
      "capabilities": ["transfer.draft_local"],
      "trust_scope_uid": "r_apple_known_sellers...",
      "concept_uids": ["c_apple..."],
      "quantity_limit": { "value": "5.000", "unit_uid": "c_kilogram..." },
      "per_window": { "count": 1, "duration_ms": 604800000 },
      "expires_at": "2026-12-31T23:59:59.999Z"
    }

It binds the grant to the authenticated principal, then creates a signed Grant
Record and Fact. It does not activate the Program or retroactively authorize
existing candidates. At intent claim time the engine rechecks the live grant
and atomically reserves its count/quantity budget.

Accepting one recommendation is also revision-safe:

    {
      "action": "respond-karma-candidate",
      "request_id": "accept-apple-draft-1",
      "candidate_uid": "r_candidate...",
      "expected_candidate_revision": 2,
      "response": "accepted",
      "edited_preview": null
    }

The engine freezes the response Fact, re-runs current visibility, target
revision, policy, grant, budget, and Action validation, then closes the
candidate and creates/executes only the preview's typed intent. If the offer,
price, window, seller, evidence, grant, or budget changed, the candidate becomes
`stale` with a new diff; “accepted” does not force an obsolete Action through.

#### Data-transition rules

| Event | Data appended/changed | What does **not** happen |
| --- | --- | --- |
| Definition created/revised | Program handle or new immutable revision, links, Proof, annotation Fact | No activation, grant, model training, or domain effect |
| Program activated/paused | Active revision pointer and/or quantity Fact, activation occurrence | No deletion of revisions/runs |
| Frequency activated/tuned or gains/loses its first/last consumer | Revision/parameter pointer, generation/cursor, exact deadline upsert/removal, annotation Fact | No polling loop/task, no rescheduling or due-check of unrelated Frequencies |
| Fact/schedule/signal arrives | Trigger occurrence with cursor/time/source | No rule runs before the occurrence is durable |
| Program evaluates | Run, trace, frozen input/effective-policy references, candidates/intents | Pure nodes do not mutate domain Records |
| Learner updates | Evidence admission decisions, checkpoint/model Fact, metrics | No direct rule/authority change |
| Recommendation routes | Candidate or Decision Record and evidence links | No Action until accepted or independently authorized |
| Internal Action succeeds | Ordinary domain Facts plus intent receipt/provenance | No alternate privileged Karma write path |
| External effect completes | Attempt/receipt and provenance Fact; qualifying observation may arrive separately | Receipt alone does not assert an unobserved physical/social outcome |
| Grant revoked | Revocation Fact, cancellation/denial of preventable intents | Past Facts/effects are not erased |

### Human-facing Karma interfaces

Karma is one sand with several lenses over the same Protein/Actions:

| Lens | Primary job |
| --- | --- |
| **Library** | Programs/templates, active/paused/faulted state, owner, purpose, next occurrence, latest result |
| **Builder** | Form/graph/DSL synchronized editor, typed ports, parameters, Proof, revision diff |
| **Why** | Causal run trace, substituted values, evidence, model output, policy/grant, intent/receipt, resulting Facts |
| **Learn** | Pattern hypotheses, admitted/rejected evidence, probability/confidence/cadence, drift, thresholds, feedback |
| **Imagine** | Replay/project/branch/DST, invariants, future timeline, plan comparison, apply-as-Actions preview |
| **Authority** | Grants plus Automation Trust scopes: concept/stage, People/Organs/proximity selectors, thresholds, budgets, expiry, capability matrix, revoke/narrow controls |
| **Queue** | Occurrences, runs, workflows, candidates, decisions, staged/retrying/uncertain/dead intents |
| **Health** | Sequencer/connector/device/model lag, failures, replay audits, engine mode/emergency controls |

The Builder begins with ordinary-language templates and forms, not a blank
programming screen. A “Recurring task” form asks *what, when, missed-occurrence
policy, and route*. An “Inventory threshold” form asks *record/concept, unit,
threshold/hysteresis, forecast horizon, and suggest/draft/ask/act*. Switching to
graph or DSL shows exactly what the form generated.

The command palette provides short operational sugar; it never bypasses Actions:

| Command | Action/query |
| --- | --- |
| `i new` | Open template/form and eventually `create-karma-program` |
| `i edit @apple.restock` | Open Builder on active revision |
| `i on @apple.restock` / `i off @apple.restock` | Activate/resume or pause with preview |
| `i run @apple.restock` | Enqueue manual run |
| `i sim @apple.restock +30d` | Create 30-day projection |
| `i why run:r_...` | Query full causal chain |
| `i tune @reminder interval=3d` | Preview typed parameter Action |
| `i grant @apple.restock` | Open capability-scoped grant editor; never “grant all” silently |
| `i trust @apple.restock` | Open concept/counterparty Trust selector and show matching offers/stage ceilings |
| `i revoke grant:@apple.autobuy` | Preview affected queued work then revoke |
| `i queue` / `i learn` / `i health` | Open corresponding Protein lens |
| `i stop effects` | Enter `stage-effects` after showing disposition |

Every compact command expands to a readable confirmation/diff when it changes
authority, social state, external state, an active revision, or more than its
declared low-risk local scope.

### Concrete program and data-change examples

These examples are interface specifications, not promises that the current
Karma parser already accepts them.

#### Preset rule — re-arm a daily task

    program habit.meditate {
      owner person:@ana
      purpose "Make meditation a daily Need until completed"
      param reminder_time: civil = 07:00 America/Sao_Paulo

      on every calendar 1d at reminder_time missed coalesce id daily_tick
      input task: ref<record> = record:@meditate

      when quantity(task) == 0 {
        act record.set_quantity {
          record: task,
          quantity: -1
        }
        require grant:@habit.local_records
      }
    }

If the task is already negative, the run records `false` and changes nothing.
If it is zero, the due schedule creates an occurrence; the run creates an
authorized `record.set_quantity` intent; the normal Action appends the quantity
Fact and changes the cached Record quantity to `-1`. Completing the task later
sets it to zero through the ordinary user Action. The program never owns a
private “completed” Boolean.

#### Meta-rule — change one day to three days

    frequency recovery.reminder_tick {
      param interval: dur = 1d range [1ms, 30d]

      every elapsed interval
        anchor 2026-07-21T09:00:00.000Z
        rephase preserve_anchor
        missed coalesce

      timer {
        resolution 1ms
        max_lateness 30s
        coalesce_window 5s
      }
    }

    program recovery.reminder {
      owner person:@ana
      purpose "Ask for a recovery check at the current interval"

      on frequency freq:@recovery.reminder_tick id reminder_tick

      ask "How is recovery today?" dedupe reminder_tick.intended_at
    }

    program recovery.adapt_frequency {
      owner person:@ana
      purpose "Ask less often after recovery has remained stable"

      on fact(record_eq: record:@recovery.score) queue id score_changed
      input scores: list<fact> = facts(record:@recovery.score, window: 14d)
      sense stable: bool = all(scores.last(7d), value >= 8)

      when enters(stable) {
        tune freq:@recovery.reminder_tick param interval = 3d
          rephase preserve_anchor
        require grant:@recovery.manage_reminder
      }

      when leaves(stable) {
        tune freq:@recovery.reminder_tick param interval = 1d
          rephase preserve_anchor
        require grant:@recovery.manage_reminder
      }
    }

`grant:@recovery.manage_reminder` permits only
`karma.manage.parameter` on Frequency `recovery.reminder_tick`, parameter
`interval`, range `[1d, 3d]`; it grants no edits, activation, data scope, or
effects.

Suppose a score Fact arrives at cursor 200 and makes `stable` enter true:

1. cursor 200 is evaluated with the Frequency's parameter revision 7 (`1d`);
2. the meta-rule produces an authorized tune intent;
3. `set-karma-parameter` commits cursor 201, parameter revision 8
   (`3d`), and recomputes the next intended boundary from the original anchor;
4. learning for cursor 200 runs after its existing reaction work and cannot
   change either run retroactively; and
5. later schedule occurrences use revision 8. Any schedule occurrence already
   ordered before cursor 201 uses revision 7.

The timestamps have millisecond precision; the cursors establish deterministic
order when the score and a timer boundary share the same millisecond.

#### Learned apple recurrence — old rules first, learning second

Assume `model:@apple.need` is at checkpoint 12 with probability `0.69p`. A
mutually confirmed apple-consumption Fact arrives at cursor 500:

1. the reaction lane runs `prog:@household.apple.restock` against checkpoint
   12. It may update inventory/projected shortage, but the `0.72p` suggestion
   gate remains false;
2. after the reaction closure, the learning lane admits the consumption Fact
   and creates checkpoint 13 with probability `0.74p` and its new confidence;
3. crossing the route threshold creates a pattern-threshold occurrence at a
   later cursor, which evaluates the Program against checkpoint 13 and creates
   one recommendation; and
4. if the higher draft threshold and grant later pass, the candidate is a local
   Transfer draft. Publishing, inviting, agreeing, confirming, and settling
   still require their independent capabilities.

The one consumption event is not handled under a rule it created. If Ana wants
to compare the alternate behavior, the Karma sand offers “replay cursor 500 under
checkpoint 13” as simulation; applying any difference is a new Action.

#### Microcontroller Signal and actuator
H: on the whole microcontroller part, if microcontrollers can be calleable as http and send http requests, we just need the rules/commands to be able to receive requests and put the value they received on a record quantity (or body, extension...). I believe that would be all, no need to do microcontroller specific stuff, just have like an open port for a record/records or give the microcontroller a valid key to make requests and thats it, we dont even know the http request coming in is from microcontroller, if we call an endpoint, we can be calling a microcontroller. Way simpler than what you have probably cooked out, please simplify this part.

    signal garden.soil_moisture {
      adapter mqtt:@garden.controller
      schema qty<percent>
      source_clock device
      sequence monotonic
      freshness 5m
      calibration @soil.sensor.v2
      quarantine outside [0%, 100%]
    }

    program garden.water_bed_1 {
      owner person:@ana
      purpose "Water bed 1 when verified moisture remains low"

      on signal sig:@garden.soil_moisture coalesce by bed_id every 500ms
      input moisture: datum<qty<percent>> = signal sig:@garden.soil_moisture
      sense dry: bool = moisture < 22% holds 10m

      when enters(dry) {
        do device:@garden.controller water {
          valve: "bed-1",
          duration: 20s
        }
        require grant:@garden.water_bed_1
        on stale ask "Soil sensor is stale; watering was not started"
      }
    }

Each packet appends a raw observation Fact on the Signal Record. Calibration
creates a derived value in the run (or an explicit derived Fact if configured).
The `holds` node persists transition state. When it enters true, policy checks
the device, bed, duration/rate budget, freshness, and interlocks before creating
an intent. The controller acknowledgement becomes a receipt; it proves the
command was acknowledged, not that water physically flowed. A later moisture
observation or flow sensor may independently prove outcome and train a model.

#### Shared percentage without importing authority

    program household.apple.peer_context {
      owner person:@ana
      purpose "Use opted-in family cadence as weak planning context"

      input family_rate: datum<aggregate<prob>> = published @family.apple.weekly {
        require signed
        require denominator >= 5
        freshness 14d
      }

      let peer_prior: prob = family_rate.value
      learn need = recurrence.beta_cadence {
        local_evidence: view:@family.apple.confirmed_consumption,
        external_prior: peer_prior weight 0.20,
        never_train_remote: true
      }

      recommend "Family apple demand usually rises around this week"
        when need.probability >= 0.72p && need.confidence >= 0.65c
    }

The imported product records numerator, denominator, cohort/window/method,
signature coverage, visibility, and freshness. It can influence a declared
prior but cannot install the publisher's rule, reveal hidden members, grant
Transfer authority, or become a Fact that Ana herself needs apples.

#### Interface control without arbitrary sand control

    program transfer.open_call_room {
      owner person:@ana
      purpose "Offer the call room when this Transfer becomes agreed"

      on fact(transfer_eq: transfer:@band.rehearsal) id transfer_changed
      when enters(transfer_state(transfer:@band.rehearsal) == agreed) {
        do ui:@ana.phone present {
          surface: "call-room",
          subject: transfer:@band.rehearsal,
          mode: "offer"
        }
        require grant:@ui.call_offer
      }
    }

This creates a presentation intent and device receipt. It may show/focus an
“Open room” control, but cannot click agreement, answer a Decision, execute
arbitrary JavaScript, hide warnings, or claim Ana joined. Opening the room is a
separate bound controller Action; joining/attendance is later evidence.

#### Explicit optimization and applying a plan

    program week.balance {
      owner person:@ana
      purpose "Propose a feasible week with protected sleep and commitments"

      on every calendar 1d at 18:00 America/Sao_Paulo
      input work = view:@week.open_work
      input promises = view:@week.agreed_promises
      input travel = view:@week.travel_estimates freshness 6h

      solve plan: list<schedule_plan> {
        require sleep >= 8h each_day
        require all(promises.windows)
        require no_overlap
        minimize overdue_penalty
        minimize schedule_change_from_current
        prefer deep_work in [09:00, 12:00]
        tie_break task.uid
        return 3
      }

      ask "Choose a proposed week" options plan
        preview action:@calendar.apply_plan
    }

The solver run stores variables, constraints, objective values, alternatives,
slack/infeasibility, and deterministic tie-break. It creates a Decision with
three exact Action previews. Nothing reserves time until Ana selects an option
and the current world/revision revalidates.

### Perception — signals, schedules, and context

Perception is the boundary that turns nondeterministic outside input into typed,
ordered, replayable evidence. Implement it after the adaptive scheduler and
occurrence kernel: adapters capture; pure nodes validate/normalize; Programs
consume only the captured envelope. No connector gets to call rule evaluation
or domain storage directly.

- [x] `create-signal` represents command/http/sensor/query sources on a
  schedule. Samples land as Facts and cascade like any other change.
- [x] `create-frequency` supports day-of-week and catch-up behavior. Frequencies
  are reusable clocks, not record timestamp columns.
- [ ] Make every capture and integration source visible as an off-switchable
  Signal record with adapter/revision, schema, freshness, last success/error,
  health, consent/visibility/purpose scope, sampling cost, rate limit, and
  retention. Phone, scale, camera, microphone, filesystem, database, HTTP,
  webhook, message bus, local model, remote model, and command inputs all obey
  this contract.
- [ ] Use one observation envelope:
  `source_uid, source_revision, source_sequence, schema_uid, value, unit,`
  `effective_at, observed_at, received_at, place, quality, uncertainty,`
  `actor/signature, capture_hash`. Source time and Cell receipt time are never
  conflated. Duplicate sequence/hash is idempotent; a correction references the
  prior observation instead of rewriting it.
- [ ] Separate raw capture from normalized evidence. Preserve the signed/raw
  value according to retention, then derive calibrated units, validation,
  quality, and semantic concept through versioned pure nodes. A changed
  calibration can re-derive history without pretending the sensor originally
  emitted the corrected value.
- [ ] Make push, polling, streaming, and interrupt-driven microcontrollers use
  the same adapter contract. Devices declare identity/key, firmware/schema
  revision, monotonic sequence, clock quality, calibration, expected cadence,
  offline buffering, maximum age, and safe backpressure. Malformed or
  out-of-range samples land in quarantine with a visible reason.
- [ ] A Signal's activation stops new acquisition and downstream trigger
  creation; it does not erase previous observations. Revoking camera,
  microphone, location, health, or other sensitive consent also prevents new
  use by runs, not merely new sampling.
- [ ] AI enters only through a visible captured model Signal or as an ordinary
  candidate author. It never has an ambient reader, hidden prompt-side data, or
  privileged writer. Prompt/request inputs obey Protein visibility and purpose
  scope; secrets and unrelated context do not enter the trace.
- [ ] Replace ambiguous frequency catch-up with an explicit missed-occurrence
  policy: `skip`, `coalesce` (one run carrying the missed count), or bounded
  `replay`; expose start/end, timezone, weekdays, calendar interval, jitter, and
  whether calendar alignment happens before or after interval addition. Jitter
  is derived from the schedule/occurrence seed, never ambient randomness.
- [ ] Let event-time nodes declare their late-data watermark and correction
  behavior: ignore for live action but include in later analysis, recompute an
  open window, compensate a reversible result, or ask. Historical evidence does
  not silently cause a present-tense actuator or social effect.
- [ ] Context is always a saved or inline Protein plus named derived values, not
  an ambient database capability. The run records the exact visible input set
  and why each row was included/excluded, plus freshness and quality used, so a
  recommendation can explain missing, denied, invalid, or stale data.
- [ ] A **Sense** is a pure, named recognizer over current Facts, Signals,
  discovery data, and projected crossings. It emits evidence-backed candidates;
  it cannot write state or contact another Cell by itself.
- [ ] Define source health as data, not just logs: last scheduled/attempted/
  successful sample, lag, consecutive errors, clock drift, dropped/quarantined
  count, adapter version, and next retry. Health can feed an operational Sense
  without recursively treating its own alarm as healthy input.
- [ ] Connectors reference secrets by opaque capability-bound handle. Program
  exports, traces, errors, notifications, and synced records never serialize
  secret values. A simulator receives a fixture, not the production secret.

### Deliberation — rules, derived values, and workflows

Deliberation is the deterministic preset-behavior layer: it evaluates the graph
that was active at the occurrence epoch and produces traces/candidates/intents.
Build pure expressions and stateful gates first, then meta-control, then durable
workflow nodes; never implement workflow/domain behavior in a parallel action
path.

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
- [ ] Specify every gate's transition behavior: edge/level trigger, enter/exit
  thresholds, hysteresis, hold duration, cooldown, once-per-window, reset, and
  unknown/stale input policy. “True on every refresh” must never accidentally
  mean “repeat an effect forever.”
- [ ] Separate a durable derived Fact from a virtual derived value. Virtual
  values are recomputed/read through the run; materialization is an explicit
  node with provenance, retention, unit, and correction semantics.
- [ ] Add durable workflow nodes for multi-step orchestration: state machine,
  sequence/parallel, wait-until, branch, approval, retry, compensation, and
  child-program invocation. Workflows coordinate typed Actions; they do not
  create a second implementation of domain behavior.
- [ ] Give a workflow an explicit concurrency policy (`queue`, `drop`,
  `coalesce`, `latest`, or bounded parallel), correlation key, timeout,
  cancellation semantics, and parent/child ownership. Long-running work resumes
  from its durable node state after boot.
- [ ] Make cancellation cooperative and observable. It prevents unclaimed
  intents, requests cancellation from claimed adapters, waits or times out
  according to policy, and runs only declared compensations. It does not claim
  an irreversible external effect was undone.
- [ ] Define transaction boundaries narrowly: compatible local Actions may
  commit atomically through one domain Action; external or social multi-step
  work is a saga with receipts and compensation. A workflow cannot hold a
  database transaction while waiting for a person, network, or device.
- [ ] Add Proof analysis for dependency cycles, contradictory writers,
  unreachable nodes, unsafe external effects, authority escalation, dead ends,
  non-convergence, fan-out explosion, stale/missing paths, unit/schema mismatch,
  privacy declassification, and likely divergence. The runtime cascade cap
  remains the final guard, not the design tool.
- [ ] Conflicting candidates are resolved by declared policy—reject all,
  priority, merge with a typed commutative reducer, serialize, or ask. Arrival
  or thread order never silently chooses a writer. The rejected alternatives
  remain in the run explanation.
- [ ] Support parameter records separately from graph revisions when safe.
  Tuning a threshold within its declared typed range appends evidence without
  rewriting topology; changing types, inputs, effects, authority requirements,
  or allowed range requires a new program revision.

### Analysis, forecasting, and optimization

Analysis is pure Karma: it can inspect, aggregate, forecast, search, and
compare without receiving action authority. Optimization turns explicit goals
and constraints into ranked plan candidates. Execution is still a separate
policy decision.

- [ ] Define an **objective specification** with owner, purpose, decision
  variables, units/domains, hard constraints, soft penalties, objective order
  (lexicographic, weighted, Pareto, or satisfice), planning horizon,
  uncertainty treatment, and tie-break. Missing objectives never default to
  “maximize activity,” money, quantity, or engagement.
- [ ] Provide deterministic adapters for linear/simplex, mixed-integer,
  constraint/scheduling, min-cost flow/matching, routing, and simulation-based
  search as needs justify them. Each adapter uses the same typed solver contract
  and can be replaced without changing program/effect semantics.
- [ ] Translate Records, Facts, Links, Promises, availability, time windows,
  places, units, skills, budgets, and user constraints into solver variables
  through explicit feature nodes. The mapping and every approximation appear
  in the run, rather than living in a sand.
- [ ] Return a plan set—not only one answer—with objective values, binding
  constraints, slack, sensitivity/range, assumptions, uncertainty, excluded
  alternatives, and deterministic infeasibility explanation or smallest known
  conflicting constraint set.
- [ ] Distinguish forecasts from plans. A forecast estimates what may happen
  under stated assumptions; a plan selects intended actions under objectives;
  a schedule reserves time/resources only when a separate typed Action says so.
- [ ] Support robust/scenario planning over captured distributions and
  Imagination branches. A plan states which uncertainty it tolerates and which
  future observation should trigger replanning; it never hides a point estimate
  behind an exact-looking answer.
- [ ] Reoptimization preserves stability by explicit change penalties and
  frozen commitments. It may not churn a person's day or revise an agreed
  Transfer merely because a marginally better solution appeared.
- [ ] Multi-person optimization uses only shared/visible constraints and
  objectives. It produces a proposal each party can inspect; it cannot infer a
  hidden preference, expose another person's private constraint, or treat one
  Cell's optimum as agreement.
- [ ] Make analysis callable through Protein/Actions and reusable by humans,
  agents, programs, and sands: validate, solve, explain, compare, cancel, and
  pin a result as a candidate. Solvers never get an implicit effect channel.

### Recommendations and learning

Learning is the lower-priority adaptation lane, not the rule runtime. It admits
only explicitly eligible evidence, updates versioned checkpoints, and proposes
probabilities/patterns; routing, Trust, and authority decide what those outputs
may become. Begin with the transparent recurrence model before adding advanced
learners so every later algorithm inherits the same evidence/rebuild contract.

- [x] A match rule is a record (`create-match-rule { watch_concept,
  max_proximity, min_confidence, auto }`) and activates/deactivates like any
  rule. `max_proximity` is a hard ceiling; matching never auto-expands.
- [x] Each heartbeat, `senses_pass` joins local OPEN promises with the discovery
  cache using sign-opposite deltas, Lingua-aligned concepts, overlapping windows,
  and a confidence floor, then places ranked drafts in the Decision Queue.
- [x] Deterministic `confidence(@p)` is the counterparty's Laplace-smoothed
  kept-promise ratio; `demand(@concept)` is the current hour's share of trailing
  30-day activity for that concept. Neither is a global reputation score.

The shipped `confidence(@p)` is a useful ingredient but its name is too broad
for the destination. Karma keeps at least these quantities distinct:

| Quantity | Question |
| --- | --- |
| Recurrence probability | How likely is this event/action within this context and horizon? |
| Estimate confidence | How much eligible evidence supports that probability and how wide is its uncertainty? |
| Counterparty evidence | What visible signed outcomes exist for this Person, concept, window, and role? |
| Expected utility/cost | Under this person's stated objective, how good is a candidate and compared with what? |
| Authority eligibility | Does a current grant allow the proposed Action now? |

- [ ] Define a **pattern hypothesis** by event schema, principal/household,
  concept hierarchy, direction and quantity band, counterpart role, place
  region, calendar/cadence bucket, prerequisite context, horizon, and feature
  revision. Similarity/generalization is explicit; the learner never silently
  widens from “green apples from this store” to all food or all people.
- [ ] Separate an **opportunity/exposure** from positive, negative, and censored
  evidence. An observed purchase/meal/completion can be positive; a deliberately
  skipped eligible opportunity can be negative; “there is no Fact” is unknown
  unless the program proves the opportunity was observable. Outages, hidden
  data, and periods before a source existed are censored, not failure.
- [ ] Learn only from evidence admitted by a versioned evidence policy:
  human-authored Facts, independently sensed outcomes, signed/mutually confirmed
  occurrences, and deliberately labeled feedback. A recommendation, generated
  draft, program-created task, model/agent text, or automated Action never
  becomes positive evidence merely because the system produced it. A later
  independently observed outcome may train the model on its own merit.
- [ ] Define local `recurrence_likelihood` separately from counterparty
  evidence, keyed only by locally visible/purpose-permitted context. Keep
  purchase need, consumption cadence, seller reliability, price forecast, and
  Transfer agreement likelihood as different models that a program may compose.

The first transparent recurrence model should be a deterministic decayed
Beta/cadence model, not a vague “growth factor.” For eligible evidence `i`:

    weight_i = decay(age_i, configured_half_life, decay_version)
    alpha = prior_alpha + sum(weight_i * positive_i)
    beta  = prior_beta  + sum(weight_i * negative_i)
    recurrence_probability = alpha / (alpha + beta)

`decay` is a specified fixed-point lookup/algorithm so it replays identically.
Each new event changes the posterior less as supported evidence accumulates;
old evidence loses influence according to the half-life. Confidence is reported
separately from probability using effective sample weight and a versioned
credible/uncertainty interval. Cadence uses deterministic eligible-time buckets
or a discrete time-to-event hazard, so “usually Saturday morning” and “about
every eight days” can coexist without confusing frequency with certainty.

- [ ] Store prior, evidence query/policy, positive/negative definitions,
  half-life, cadence/timezone, feature buckets, minimum effective sample weight,
  probability and confidence thresholds, enter/exit hysteresis, mute/snooze,
  drift policy, and model version as typed policy—not frontend state.
- [ ] Persist each model update with prior checkpoint hash, admitted/rejected
  evidence ids and reasons, logical evaluation time, resulting sufficient
  statistics, metrics, and new checkpoint hash. Checkpoints are caches:
  replaying eligible evidence is the truth and must reconstruct them.
- [ ] Avoid combinatorial context mining by declaring candidate feature
  templates and resource/privacy budgets. New pattern discovery emits a
  hypothesis with multiple-testing/validation information; it does not create a
  million invisible rules or search sensitive attributes by default.
- [ ] Split evidence into deterministic train/validation horizons or
  forward-chaining windows. Report calibration, false-positive/negative cost,
  support, drift, and baseline comparison before a learned policy can graduate
  from watching to suggestion or autonomy.
- [ ] Model lifecycle is
  `cold → learning → calibrated → drifting → stale/disabled`. Insufficient,
  stale, shifted, or contradictory data lowers confidence and autonomy. A
  threshold crossing uses hysteresis/minimum duration so values near the line
  do not chatter.
- [ ] Offer a registry of deterministic learner types: decayed count/Beta,
  cadence/hazard, moving quantile, seasonal baseline, anomaly/change detector,
  regression/classification, and later deterministic seeded advanced models.
  Every type publishes its feature contract, limitations, update rule, metrics,
  memory/fuel bounds, and explanation strategy.
- [ ] Never let a learner mutate a program graph or its own feature/evidence
  scope directly. It emits parameters or a program-revision candidate. Automatic
  promotion requires a pre-authorized template and scope, Proof, validation,
  optional shadow duration, rollback condition, and a grant that explicitly
  includes activation.
- [ ] Refine counterparty evidence by concept/role/window and show kept, broken,
  disputed, late, partial, missing, and verification counts directly. Any
  smoothed estimate is local decision support, not a global reputation score,
  identity label, or fact about a person's character.

### Recommendation contract

A recommendation is a durable, inert interface between inference and choice.
It must be deduplicated, updateable as evidence changes, and independently
revalidated when accepted. Implement this lifecycle before whispers or
automatic drafts so Attention never becomes the only place a candidate exists.

- [ ] Emit one lifecycle-managed recommendation per
  `(pattern/objective, subject, horizon, candidate-kind)`, with deduplication
  and update-in-place evidence. It carries claim, evidence, model revisions,
  probability, confidence/uncertainty, expected benefit/cost, alternatives,
  freshness, required authority, expiry, and an exact preview/diff.
- [ ] Recommendation states are `open, accepted, accepted-edited, dismissed,
  snoozed, muted, obsolete, expired`. New evidence may update an open item but
  cannot resurrect a muted pattern or replace a person's edited choice.
- [ ] Routes are policy:
  `observe-only → log`,
  `suggest → recommendation`,
  `draft → inert Action/Transfer/program candidate`,
  `ask → durable decision`, and
  `act → authorized intent`.
  Probability/confidence thresholds and authority checks are required at every
  transition; a high score does not skip a route.
- [ ] Feedback is typed and contextual: correct, incorrect, wrong time/place/
  quantity/person, already done, not useful, too frequent, accepted unchanged,
  accepted edited, snoozed, or mute. It may update delivery/pattern models under
  their evidence policy while preserving the original recommendation and
  response.
- [ ] Detect action/recommendation loops: if accepting a suggestion creates the
  only evidence that makes it more likely, mark the path endogenous and exclude
  or separately measure it. Compare against a no-intervention baseline where
  feasible.
- [ ] Support explanations at several depths: one sentence, substituted values,
  evidence timeline, model card/uncertainty, objective/alternatives, policy
  decision, and counterfactual (“without Tuesday's consumption Fact, this would
  remain below the suggestion threshold”).
- [ ] A human, software agent, imported template, or model may author the same
  inert candidate format. Authorship is provenance, not permission; none bypass
  visibility, evidence display, Proof, budgets, or the principal's grant.

### Shared and collective Karma

A person may choose to expose records, evidence, percentages, model summaries,
or programs. Collective Karma is composition of explicitly published
evidence—not a central brain and not a loophole around Protein visibility.

- [ ] Publish one of four typed products: visible raw evidence; a signed
  aggregate; a model/forecast card with stated inputs; or a Karma
  program template. Each carries owner/origin, purpose/terms, audience,
  visibility, time window, concept/unit scope, method/revision, freshness,
  lineage/hash, signature, and revocation/expiry.
- [ ] A percentage always includes numerator, denominator, eligibility/cohort
  definition, excluded/missing count, time window, unit/concept, method, and
  signature/verification coverage. “80% of people do X” without those fields is
  invalid input, not Karma.
- [ ] Apply visibility before aggregation and track input taint through derived
  outputs. An output cannot be published more broadly than its inputs unless an
  explicit declassification policy proves an allowed aggregate. Small cohorts,
  repeated queries, joins, and differencing attacks obey minimum-group/query
  budgets or optional deterministic privacy mechanisms.
- [ ] Allow a Cell to combine local evidence with permitted remote raw facts or
  signed aggregates using declared weighting, provenance, freshness, and trust
  policy. Remote claims remain inputs with uncertainty; they do not become local
  Facts about an unseen person merely because they are signed.
- [ ] Sharing a live rule/program means sharing a content-hashed definition or
  template, never its secrets, private inputs, model checkpoint, grant, or
  authority. Installation creates an inactive local revision whose references,
  data scope, budgets, and effects must be rebound and proven.
- [ ] Support family/team cooperative patterns only over scopes each
  participant granted. A household need can use Ana's and her mother's visible
  pantry evidence, while explanations and outgoing Transfers reveal no more
  than their grants permit.
- [ ] Revocation stops future export/use and cancels eligible queued work; it
  cannot erase signed data already shared. Retention and redistribution terms
  remain visible, and downstream models mark revoked/unavailable provenance
  rather than laundering it.
- [ ] Prove recurrence saturation/decay, cadence, confidence separation,
  deduplication, negative/censored evidence, feedback, opt-out, drift,
  no self-training, no private leak, no reputation laundering, and no
  unauthorized automatic commitment.

### Attention and whispers — the interruption contract

Attention schedules human interruption after a candidate/decision already
exists. It ranks and delivers; it does not recompute the decision, gain
authority, or hide parked work. Implement inbox/digest truth first and device
channels afterward, with one cross-device acknowledgement identity.

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
- [ ] A decision freezes the question, subject, evidence/run, options and exact
  Action previews, required principal, default/no-answer behavior, deadline,
  reversibility, and current-revision preconditions. Answering after the world
  changed either revalidates or returns a stale decision; it never executes an
  obsolete preview.
- [ ] Rank attention by explicit user priority, urgency/window, confidence,
  safety, reversibility, cost of delay, interruption cost, and recent delivery
  load. The formula and tie-break are inspectable. Low-value items collect into
  summaries; urgency does not manufacture authority.
- [ ] Route through device records to inbox, digest, desktop toast, mobile push,
  sound, or text; support per-program/per-source channel controls, quiet hours,
  location/context eligibility, accessible presentation, and “show why now.”
- [ ] Delivery has an idempotent whisper uid and per-channel attempts/receipts.
  Opening, acknowledging, dismissing, or answering on one device converges on
  the durable item and suppresses redundant channels according to policy.
- [ ] Escalation is explicit: retry a channel, change channel, notify another
  delegated recipient, or expire. No program infers permission to contact a
  family member/employer merely because the primary person did not answer.
- [ ] Attention policy reserves capacity for safety and expiring commitments,
  caps every source/program, supports “never interrupt for this,” and shows
  which items were parked by budget. Digest generation summarizes links; it
  does not replace or mutate the underlying decisions.
- [ ] Feedback is a first-class result (`accepted`, `edited`, `dismissed`,
  `snoozed`, `muted`, `wrong-context`) used to tune local delivery and pattern
  policy without rewriting historical evidence.
- [ ] Explanations and controls remain available without coercive ranking,
  synthetic urgency, dark patterns, or hiding the “do nothing/mute/pause”
  option. Accessibility and quiet-time constraints are hard policy.

### Effects, authority, and social safety

Effects are the only bridge from a pure run/candidate to mutation or the outside
world. Policy first creates an authorized durable intent; a worker later claims
and executes it through the normal typed Action/adapter. Implementing any
automatic write before grants, budget reservation, revocation recheck, and
idempotent intent identity would create a second privileged system.

- [x] Rule consequences include `set_quantity`, `add_quantity`, `emit_promise`,
  `run_command`, `run_query`, `run_action`, `set_visibility`,
  `activate`/`deactivate` (including another rule), `ask`, and budgeted `notify`.
  Effects run outside evaluation from a durable queue and append zero-delta
  provenance Facts.
- [x] Transfer automation fails closed against the old activation path; current
  manual revision, agreement, occurrence, confirmation, and settlement gates
  remain authoritative.

The old fail-closed behavior remains correct until the capability system exists.
The destination, however, does not hard-code “a machine may never commit” or
“automation may do everything.” A person can deliberately delegate even
high-impact actions on **their own** behalf within exact limits. The engine
preserves that freedom while making escalation explicit, narrow, revocable,
rechecked, and attributable.

| Capability family | Default route | Examples |
| --- | --- | --- |
| Pure read/derive/analyze | evaluate within data scope | Protein, feature, projection, solver, report |
| Local reversible data | suggest/ask until granted | set/add quantity, link, local metadata, create task |
| Attention/presentation | budgeted delivery | decision, digest, toast, focus a permitted interface |
| Program/meta-control | ask; narrow grants allowed | tune parameter, pause program, activate proven revision |
| External resource | staged; bound adapter grant | HTTP, command, filesystem, network, payment/device controller |
| Private Transfer preparation | suggest/draft | local draft, projection, rank possible counterparties |
| Social publication/negotiation | ask; exact grant allowed | publish OPEN offer, invite, counteroffer, message, visibility |
| Own social commitment/evidence | explicit high-authority grant | agree own revision, claim own occurrence, confirm own side, settle owned Record |
| Irreversible/safety-critical | manual or dedicated interlocked grant | door/vehicle/medical/industrial actuation, destructive command |

The effective authority for an intent is the intersection of:

`program requirements ∩ principal delegation ∩ actor permissions ∩`
`visible/purpose-allowed data ∩ applicable Automation Trust scope ∩`
`current domain capability ∩ budgets ∩`
`target revision/preconditions ∩ safety interlocks`

Any missing term denies or stages the intent. A recommendation score, model
confidence, owner role, template signature, or past successful run cannot
replace one of these terms.

Grant and Automation Trust overlap intentionally as defense in depth but answer
different questions. The grant says “this Program may perform these Action
kinds for me within these budgets”; the Trust scope says “these counterparties/
Organs/proximities are acceptable for this concept and stage at these evidence
thresholds.” The effective result is their intersection, never their union.

- [ ] Make grants typed records signed by the delegating Person. At minimum they
  scope principal, program and optionally exact revision/template, capability
  and Action kinds, target records/concepts/places/controllers, recipients/
  Organs/proximity, quantity/value/currency, per-run/day/window rate, valid
  time/context, evidence quality, allowed visibility, reversibility, approval
  threshold, and expiry.
- [ ] A program cannot create, widen, renew, transfer, or choose the principal
  of its own grant. Grant management is a separately permissioned typed Action.
  Delegation is non-transitive unless the original grant names an exact
  subdelegation, which is itself visible and revocable.
- [ ] Persist scopes, budgets, recipients, quiet time, thresholds, model/
  evidence restrictions, forbidden Actions, and pattern overrides as typed
  policy. Enforcement is in the engine and Action/domain boundary, never only
  in the Karma sand, another sand, connector, or agent prompt.
- [ ] Attribute every automatic Action to both the real principal and
  `program/revision/run/intent`, with `cause=karma`. The program never
  becomes a Person, signs as another Person, or obscures which delegation was
  consumed.
- [ ] Reserve budget when an intent is authorized, reconcile it on receipt, and
  release it on denial/cancellation according to typed policy. Concurrent runs
  cannot each see the full remaining budget and overspend it.
- [ ] Revocation and emergency-stop prevent unclaimed work immediately and are
  rechecked before dispatch. Already committed local Facts remain; an already
  dispatched external action gets an honest receipt/uncertain state and any
  declared compensation.

### Scoped automation Trust — who may enter an automatic Transfer

Probability is evidence about what may be needed; it is not trust in a seller
and not authority to transact. Add a local **Automation Trust scope** between
recommendation policy and a Transfer intent. It answers:

> For this principal, Program/purpose, concept/direction, and Transfer stage,
> which People and origin/delivery Organs may participate, at what proximity,
> thresholds, terms, and limits?

The existing contact state remains the coarse first gate: `blocked` always
denies import, discovery, suggestion delivery, and automation; `known` merely
permits ordinary interaction and never implies automation. Automation Trust is
finer, local, concept-specific, and unpublished by default. It is not a global
reputation level and cannot grant authority to the counterparty.

Keep these gates separate and conjunctive:

`visible offer ∩ recurrence probability/confidence ∩ objective/terms ∩`
`Automation Trust scope ∩ principal Program grant/budgets ∩ Transfer domain`
`revision/agreement/occurrence readiness`

A failure in any gate denies or routes to Attention with its own reason. Raising
probability cannot compensate for an untrusted Organ; putting an Organ on an
allowlist cannot compensate for insufficient evidence or grant; and a grant
cannot make an invisible or stale offer visible/current.

#### Automation tiers

Use a stage ceiling rather than one `trusted=true` Boolean:

| Tier | Highest behavior the Trust scope is willing to consider | Still required |
| --- | --- | --- |
| `observe` | Read/compare visible evidence | visibility/purpose |
| `suggest` | Show recommendation involving the counterparty | recommendation policy |
| `draft` | Create a private local Transfer draft | `transfer.draft_local` grant |
| `propose` | Publish/address/send proposal | `transfer.publish/propose` grant |
| `negotiate` | Claim/counter/revise inside terms | `transfer.negotiate_own` grant |
| `commit` | Agree/activate the principal's own side | exact high-authority agreement/activation grant |
| `settle` | Claim/confirm own occurrence and settle owned Record | independent evidence, confirmation/settlement grants, domain readiness |

Higher tiers include willingness for lower stages but confer none of their
capabilities. The effective stage is the minimum of Trust ceiling, grant
capability ceiling, current policy route, and Transfer domain capability.

#### Typed scope and selector

An immutable `AutomationTrustScopeRevision` contains:

    owner_person_uid
    optional_program_uid / optional_program_revision_uid
    purpose
    concept_uid + include_descendants
    direction: buy | sell | give | receive | any
    stage_ceiling: AutomationTier
    counterparty_selector: SelectorExpr
    per_stage_probability_confidence
    allowed_units / quantity_range / value_range / currency
    allowed_places / windows / weekdays
    rate_and_aggregate_budgets
    required_counterparty_evidence
    valid_from / expires_at

The selector AST is deliberately explicit:

    enum SelectorExpr {
        Any(Vec<SelectorExpr>),
        All(Vec<SelectorExpr>),
        Not(Box<SelectorExpr>),
        PersonIn(Set<PersonUid>),
        OriginOrganIn(Set<OrganUid>),
        ViaOrganIn(Set<OrganUid>),
        ProximityAtMost(u32),
    }

There is no ambiguous “list plus proximity” behavior. `any { organ list;
proximity <= 2 }` means either condition; `all { organ list; proximity <= 2 }`
means both. Empty `any` is false, empty `all` is true only inside a scope that
also names a positive selector, and a Trust scope with no positive counterparty
selector cannot activate above `suggest`.

An explicit deny list is evaluated before the positive expression and always
vetoes it. Then the exact Trust revision referenced by the Program/grant is
evaluated; Lince does not merge every matching allow rule from the database and
guess precedence. Multiple scopes require an explicit `any/all` composition in
the Program policy. This keeps “why was this seller allowed?” mechanically
answerable.

Transfer parties remain People. An Organ selector says which Cell identity may
originate or carry the automated relationship; it does not trust every Person
inside that Organ or sign for them. A Person selector may narrow the party
inside allowed Organs. For relayed discovery, `origin_organ` is the record's
preserved lineage and `via_organ` is the delivery contact; a policy can require
either or both. Proximity is the evaluating Cell's local contact value, never a
remote self-asserted number, and no rule automatically broadens its maximum.

#### DSL example — probability plus allowed Organs/proximity

    trust apple.known_sellers {
      owner person:@ana
      purpose "Allow bounded apple restock automation"
      concept exact concept:@apple
      direction buy

      deny {
        origin_organ in [organ:@blocked.market]
        person in [person:@seller.with.dispute]
      }

      counterparty any {
        origin_organ in [organ:@family.coop, organ:@neighborhood.market]
        all {
          proximity <= 2
          via_organ in [organ:@trusted.relay]
        }
      }

      stage suggest require probability >= 0.70p confidence >= 0.60c
      stage draft   require probability >= 0.85p confidence >= 0.75c
      stage propose require probability >= 0.92p confidence >= 0.85c
      ceiling propose

      quantity in [0.5kg, 5kg]
      value <= 50 BRL per 7d
      window local [07:00, 20:00]
      expires 2026-12-31T23:59:59.999Z
    }

    program household.apple.restock {
      # ...evidence, projection, and offer ranking from the earlier example...

      when shortage && need.probability >= 0.85p
                    && need.confidence >= 0.75c {
        draft transfer.draft_local from seller
        require trust:@apple.known_sellers at draft
        require grant:@apple.local_drafts
      }

      when shortage && need.probability >= 0.92p
                    && need.confidence >= 0.85c {
        act transfer.publish_proposal from seller
        require trust:@apple.known_sellers at propose
        require grant:@apple.proposals
      }
    }

If an OPEN apple offer comes from `organ:@random.shop` at proximity 4, a `0.99p`
need still cannot draft/propose it. If it comes from the family co-op, the first
`any` arm matches. If it comes through the trusted relay at proximity 2, the
second `all` arm matches. The blocked market is denied even if another arm would
allow it.

The corresponding creation Action is typed rather than a free-form policy
string:

    {
      "action": "create-automation-trust-scope",
      "request_id": "create-apple-known-sellers-1",
      "slug": "apple.known_sellers",
      "purpose": "Allow bounded apple restock automation",
      "program_uid": "r_apple_restock...",
      "concept_uid": "c_apple...",
      "include_descendants": false,
      "direction": "buy",
      "stage_ceiling": "propose",
      "selector": {
        "deny": {
          "origin_organ_uids": ["r_blocked_market..."],
          "person_uids": ["r_disputed_seller..."]
        },
        "allow": {
          "op": "any",
          "items": [
            {
              "op": "origin_organ_in",
              "organ_uids": ["r_family_coop...", "r_neighborhood_market..."]
            },
            {
              "op": "all",
              "items": [
                { "op": "proximity_at_most", "value": 2 },
                { "op": "via_organ_in", "organ_uids": ["r_trusted_relay..."] }
              ]
            }
          ]
        }
      },
      "stage_thresholds": {
        "draft": { "probability": "0.8500", "confidence": "0.7500" },
        "propose": { "probability": "0.9200", "confidence": "0.8500" }
      },
      "max_quantity": { "decimal": "5.000", "unit_uid": "c_kilogram..." },
      "max_value": { "decimal": "50.00", "currency": "BRL", "window_ms": 604800000 },
      "expires_at": "2026-12-31T23:59:59.999Z"
    }

Creation derives the principal and creates a disabled scope handle plus revision
1 and Proof. A separate `activate-automation-trust-revision` selects it after
showing which active Programs/grants could begin matching. Later revisions keep
the same handle, never edit revision 1, and take effect only from a later
occurrence cursor.

#### Persistence, Actions, Protein, and evaluation

Store each Trust scope as a Record (`kind=automation_trust_scope`, quantity is
activation) with immutable revision sidecars and typed selector member tables.
Use integer fixed-point columns for probability/confidence and canonical
quantity/value/unit fields. Suggested sidecars are:

- `automation_trust_scope_revision` for owner/program/purpose/concept/direction/
  stage/threshold/limits/validity and content hash;
- `automation_trust_selector_node` for normalized `any/all/not/atom` tree and
  stable node order; and
- `automation_trust_selector_member` for Person/Organ sets with
  `allow|deny` and `origin|via|person` roles.

The create/revise Action derives the principal, validates every referenced
Person/Organ/concept, canonicalizes the selector, appends a new immutable
revision and Fact, and never activates a widened revision by implication.
Activation uses the normal Program-like revision selection plus quantity knob.
Narrowing may be immediate; widening requires the same explicit authority and
preview as a new Transfer grant.

Extend `source:"karma"` with `object_kind="trust_scope"`, predicates
`concept_in`, `program_eq`, `stage_ceiling_gte`, `person_eq`,
`origin_organ_eq`, `via_organ_eq`, `max_proximity_lte`, `active`, and
`expires_before`; includes expose normalized selector, thresholds/limits,
referencing Programs/grants, current capabilities, and recent allow/deny traces.

Policy evaluation returns a structured proof, never just `false`:

    concept: pass exact @apple
    direction: pass buy
    probability: pass 0.93p >= 0.92p
    confidence: pass 0.87c >= 0.85c
    deny_selector: pass no deny matched
    positive_selector: pass origin_organ @family.coop
    stage_ceiling: pass propose
    quantity/value/window: pass
    grant: pass @apple.proposals, 1/1 weekly reservation
    transfer_domain: pass expected revision 8

The exact Trust/grant revisions are frozen into the candidate explanation and
rechecked live before intent dispatch. A later block, expiry, proximity change,
Trust revision, offer revision, or budget use deterministically denies/stales
the intent.

### Karma controlling Transfer

Every Transfer mutation remains the typed, revision-safe, idempotent domain
Action described in `docs/Transfer.md`. Karma never edits Transfer
tables, invents signatures, bypasses agreement/occurrence gates, or maintains a
second Transfer state machine. It may control every legitimate stage of a
principal's side when that exact capability has been delegated:

| Stage | Capability and non-negotiable gate |
| --- | --- |
| Observe/project/match | `transfer.read/project`; visibility applies before matching, scoring, aggregate, and explanation. |
| Create a private local draft | `transfer.draft_local`; matching Trust scope at `draft`, then freezes source evidence and expected value/window but contacts nobody. |
| Publish OPEN/address people | `transfer.publish/propose`; matching Trust scope at `propose` plus separate recipient, audience, concept, value, rate, and expiry grant. Publication is a social effect, not “just a draft.” |
| Claim an OPEN promise/counteroffer | `transfer.negotiate_own`; matching Trust scope at `negotiate`, exact current revision, allowed counterparties/terms, stale-write rejection, and signed principal attribution. |
| Revise terms | `transfer.revise_own`; only fields and ranges in grant. Normal domain semantics invalidate agreement; Karma cannot preserve stale consent. |
| Review/agree own side | `transfer.agree_own`; Trust ceiling `commit` plus explicit high-authority delegation naming agreement policy, counterparty/cohort, concept/value bounds, window, evidence, and grant expiry. It can never sign another party's level. |
| Activate/reserve own contribution | `transfer.activate_own`; current revision agreement and availability/reservation policy must already permit it. Budget reservation is atomic. |
| Claim delivery/receipt/occurrence | `transfer.claim_occurrence_own`; Trust ceiling `settle`, only the principal's statement, tied to qualifying independent evidence or an explicitly allowed manual/external source. A program's own intent is not proof it happened. |
| Confirm own side | `transfer.confirm_own`; current occurrence, confirmation policy, evidence source/quality, and principal grant. It never confirms what the counterparty must attest. |
| Settle an owned Record | `transfer.settle_local`; only after domain readiness, expected revision, idempotency, local ownership, application formula, and quantity/value budgets pass. Settlement still creates the ordinary signed Facts. |
| Withdraw/cancel/dispute/correct | Separate `transfer.withdraw_own/cancel_own/dispute_own/correct_own`; terminal evidence is never rewritten. Compensation or reversing/successor Transfer remains explicit. |
| Expand visibility/proximity | `transfer.declassify`; never implied by propose/agree. Exact fields/audience and privacy budget are reviewed independently. |
| Create remainder/successor/dependency | `transfer.draft_local` by default; publication/agreement follows the same later gates and cannot inherit authority accidentally. |

- [ ] Encode these as capability families rather than one
  `transfer:automatic` Boolean. Grants can allow drafts but forbid publication,
  allow a weekly purchase from named sellers but forbid new recipients, or
  allow settlement only from a bound scale/scanner confirmation.
- [ ] Freeze the proposed canonical Transfer revision and preview at policy
  time, then send `expected_revision` and request/idempotency key through the
  normal Action. A stale counteroffer, changed price, recipient, unit, window,
  location, visibility, agreement, or evidence returns to policy/attention.
- [ ] Never use locally inferred counterparty probability as their consent.
  Each Person or their explicitly delegated program acts only for their own
  identity. Cross-Cell automation composes through signed proposals and
  responses, not shared hidden authority.
- [ ] Let policies choose autonomy per step:
  “always ask before publishing,” “auto-counter within 5% and these sellers,”
  “auto-agree this exact recurring revision,” or “settle after both signed
  scanner receipts.” A human can override, pause, narrow, or revoke at any time.
- [ ] Keep financial/payment execution separate from Transfer settlement. A
  payment connector is another high-authority external effect with its own
  receipt and reconciliation; a successful payment receipt may be evidence for
  a Transfer policy but does not silently settle Records.

### External, device, and interface effects

External effects extend the same intent/receipt state machine with adapter-
specific schemas and safety. Add each adapter family only after its capability,
idempotency/uncertainty, secret redaction, simulation fixture, and manual
reconciliation behavior are specified; “generic command” is not a substitute
for a typed device or UI controller.

- [ ] All effects use durable typed action intents with target, exact payload,
  schema/revision, nonce/idempotency key, principal/grant, preconditions,
  deadline, lease, retry class, expected receipt, capture/redaction, and
  compensation/uncertainty behavior. “Run this string somewhere” is not a safe
  destination contract.
- [ ] Commands declare executable identity/hash, typed arguments (no implicit
  shell unless explicitly granted), environment allowlist, secret handles,
  working-directory/filesystem roots, stdin/stdout schemas, timeout, process/
  CPU/memory limits, and network capability. Shell interpolation is visible
  high-risk behavior, not sugar.
- [ ] HTTP/connectors declare method, host/path policy, request/response schema,
  auth secret handle, redirect/DNS policy, body limits, timeout/retry semantics,
  rate/budget, idempotency support, and redacted capture. A retry is automatic
  only when the adapter's operation is proven idempotent or carries a remote
  idempotency key.
- [ ] Device/actuator controllers expose typed commands and state, physical
  bounds, interlocks, heartbeat/failsafe, manual override, acknowledgement vs
  observed outcome, and safe shutdown. Opening a call room, watering a garden,
  or moving a motor are distinct registered capabilities, never arbitrary bytes
  sent to a sand or microcontroller.
- [ ] Interface control goes through registered host/controller Actions such as
  `ui.present`, `ui.navigate`, `ui.focus`, `ui.layout.apply`, or a typed
  domain controller. Programs cannot execute arbitrary DOM/JavaScript, forge
  user input, hide permission/audit controls, dismiss a decision as the person,
  or mutate board chrome through the Ledger.
- [ ] Distinguish durable desired interface state (a record/policy another
  device can reproduce) from ephemeral presentation intent (focus this record
  now). Each device binding can accept, adapt, or deny presentation under local
  accessibility, safety, interruption, and foreground-control policy.
- [ ] Simulation replaces every external/device/interface adapter with a
  deterministic model or scripted fixture. It records the hypothetical intent
  and receipt; it never performs the production effect.

### Imagination, simulation, and proof before action

Imagination is not a forked rule engine. It supplies a snapshot, virtual ports,
and event/fault script to the same scheduler/evaluator/policy code, then stores
isolated traces and comparisons. Implement replay first, projection/branching
second, and generated DST/model checking after the replay capsule is sufficient.

- [x] `Engine::project(now, until)` folds promises and rules on a virtual clock
  with Signals frozen. `Engine::snapshot(now)` creates mutable input for
  toggle/clear/re-fold/diff, so branching futures already exist as an internal
  engine call. **Legacy scope:** it folds the legacy `registry.rules` and f64
  quantities, and silently skips rules needing signals or sums. E0.4 rebuilds
  this over Karma programs with exact decimals and reported exclusions; this
  entry stays checked only until the rule import lands, at which point its input
  goes empty.
- [ ] Expose project/snapshot through a typed transport verb so a sand can scrub
  and branch a future: change starting quantities, toggle a program, clear a
  promise, alter time, re-fold, and compare timelines without touching the real
  Ledger. Ships against E0.4's projector, not the legacy fold.
- [ ] Simulation operates on an isolated snapshot with a virtual clock and
  mocked signals/effects. Its seed, inputs, event script, stopping/bookmark
  conditions, replay capsule, and engine version make every run reproducible;
  “apply” means separately reviewing ordinary typed Actions, never committing a
  simulated state wholesale or reusing simulated receipts as real evidence.
- [ ] Distinguish four products built on one kernel:
  **replay** reproduces a past run from captured inputs;
  **projection** folds one stated future;
  **scenario/planning** compares deliberate branches and uncertainty; and
  **DST** generates event/fault schedules to search for invariant violations.
  The UI and test runner differ, but the execution semantics do not.
- [ ] Let people define invariants and questions: can this state be reached,
  do these programs conflict, will a quantity cross a boundary, does the graph
  settle, can an effect repeat, and what changes if this promise disappears?
  Proof results link to the exact program revisions and counterexample trace.
- [ ] Add deterministic generated scenarios and fault injection for time jumps,
  DST gaps/folds, restart/crash at every durable boundary, delayed/failed/
  duplicate/uncertain effects, duplicate Facts, stale decisions, grant
  revocation races, exhausted budgets, reordered sync arrival, partitions,
  corrupt/quarantined inputs, missing/stale Signals, device disconnect, and
  model drift. This is both product Imagination and the test architecture for
  the autonomous runtime.
- [ ] Drive generated runs from a named workload distribution over programs,
  Records, Transfers, people, time, signals, actions, faults, and operator
  choices. Record the root seed and split seed per generator/node so failures
  replay when generation is parallelized.
- [ ] Add deterministic shrinking/minimization of a failing trace while
  preserving the violated invariant, and emit a portable replay capsule plus a
  readable causal counterexample. A seed without the engine/program/model
  hashes and captured fixtures is not a complete reproduction.
- [ ] Maintain small independent reference models for foundational invariants
  where practical: Ledger/quantity conservation and compensation, schedule
  occurrence, grant/budget consumption, exactly-once intent identity, workflow
  state, and Transfer readiness. Differentially compare production kernel,
  reference fold, and upgrade versions.
- [ ] Let program authors declare assumptions, controllable variables,
  distributions/ranges, invariants, bookmarks, stopping conditions, maximum
  logical time/events/fuel, and effect fixtures. An unconstrained scenario
  cannot accidentally read production secrets or call production adapters.
- [ ] Simulate multiple Cells with independent occurrence cursors, clocks,
  visibility, grants, outboxes, partitions, and delivery schedules. Assertions
  distinguish per-Cell deterministic replay from convergence properties that
  should hold after all permitted messages arrive.
- [ ] Proof has three honest result classes: proved within a stated finite/
  symbolic domain, no counterexample found under stated exploration, or
  counterexample found. Timeouts and unsupported nodes are “unknown,” never a
  green check.
- [ ] Build calendar/time-budget and graph/state-space projections from the same
  simulator: time on one axis, quantities/ranges on another, rule-active regions,
  consequence arrows, dependency/supply-chain paths, uncertainty bands, and
  real-vs-projected values.
- [ ] Shadow mode runs a candidate revision beside the active one against live
  captured evidence, blocks all effects, and compares candidates/intents,
  resource cost, false alarms, and policy outcomes. Promotion criteria and
  rollback triggers are stored before the shadow begins.
- [ ] A continuous forecast is a cache linked to its starting cursor,
  assumptions, revisions, and generated time. New evidence marks it stale and
  queues recomputation; it is never mistaken for a promised or settled Fact.

### Karma Flow Plane — one control room, not a programming language exam

The Karma sand is the second vertical workflow and a projection of the contracts
above. Forms, Flow Plane, DSL, Why, Learn, Imagine, Authority, Queue, and Health are
lenses over the same Program/Protein/Action model. Do not put durable policy,
schedule math, model updates, authority, or effect retry logic in JavaScript.

- [ ] Build the Karma Flow Plane on the shared canvas: overview all programs,
  filter/group by type, owner, scope, state, or tag, and zoom from the whole
  dependency graph into one node's configuration, evidence lineage, model,
  authority, run history, workflow instances, and effect health.
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
  writes and external effects, compare active/candidate output, and allow
  breakpoints before an effect.
- [ ] Surface loop/conflict/authority Proof on edit and save; compare revisions,
  publish/rollback by selecting the active revision, pause immediately, inspect
  queued/running/dead effects, retry safely, and compensate reversible Actions.
- [ ] Make data scope and authority visible on the graph: taint paths, hidden/
  missing inputs, declassifications, grant boundaries, remaining budgets,
  recipients, values, expiry, and the exact node that first requires escalation.
  Activation never bundles an unread permission dialog into a generic “enable.”
- [ ] Give learning its own inspectable surface: hypotheses, eligible/rejected
  evidence, probability vs confidence, cadence, thresholds/hysteresis, model
  checkpoints, validation/calibration, drift, recommendation feedback, and a
  “forget/rebuild from allowed evidence” operation.
- [ ] Give operations a queue/run surface: occurrence lag, sequencer status,
  paused/faulted programs, nonterminal workflows, staged/leased/retrying/
  uncertain/dead intents, connector/device health, budgets, and replay capsule
  export. Never require filesystem log access for normal recovery.
- [ ] Make every “why” navigable in both directions: changed Fact → occurrence
  → run → node/evidence/model → candidate → grant/policy → intent/receipt →
  resulting Fact, and a result back to every program that consumed it.
- [ ] Provide global and scoped controls for normal/stage-effects/observe-only/
  emergency-stop, pause/resume, cancel, retry, compensate, mute, and revoke.
  Controls show what happens to already queued, leased, dispatched, and waiting
  work before confirmation.
- [ ] Provide installable templates as ordinary disabled program graphs: habit,
  recurring task, inventory threshold, birthday reminder, recurring Transfer
  draft/negotiation, call intent, sensor/actuator loop, optimizer, monthly recap,
  and command flow. Installation grants no data scope, secret, budget, connector,
  controller, or authority until the person reviews and binds them.
- [ ] Store canvas layout and personal display preferences as host state while
  program semantics, parameters, scopes, grants, and revision selection remain
  Cell data. Rearranging nodes must not create a new semantic revision.
- [ ] All authoring, trace, simulation, and emergency controls are keyboard and
  screen-reader reachable. Color/animation never carries the only explanation
  of state, confidence, authority, or failure.

### Control contract for humans and software agents

Every control available in the Karma sand is also a typed Action, and every durable
result is readable through Protein. This is how a human, CLI, sand, script, or
authorized software agent can control **every** Karma feature without
receiving database access or a private backdoor.

- [ ] Provide typed Actions for program create/fork/revise/validate/prove,
  parameter tune/reset, simulate/shadow/compare, activate/select-revision,
  pause/resume/retire, run-once/replay, model rebuild/disable, recommendation
  feedback, candidate approve/reject/edit, decision answer, grant create/narrow/
  revoke, workflow cancel, and intent stage/cancel/retry/compensate.
- [ ] Every mutating Action carries viewer/principal derived by the engine,
  expected revision where applicable, request/idempotency key, reason, and
  cause/provenance. A UI or agent cannot name a more powerful actor in its
  payload.
- [ ] Protein exposes capability booleans and stable blocking reasons beside
  each program, revision, candidate, grant, decision, workflow, and intent.
  Interfaces render those capabilities; they do not duplicate the permission
  calculation.
- [ ] A software agent reads only explicitly granted Protein scopes, proposes
  the same inert graph/Action candidates, and invokes the same Actions as a
  human tool. Model/agent reasoning may be opaque, but the candidate diff,
  engine Proof, policy, principal, and resulting effects remain exact.
- [ ] Editing by an agent never activates by implication. A grant may
  separately allow activation of proven revisions matching an exact template/
  scope and shadow criteria; otherwise activation is a durable human decision.
- [ ] A meta-program may tune, pause, resume, or select revisions of named
  programs only under `karma.manage` with field/range/state limits. It
  cannot edit its own grant, change owner, bind secrets/connectors, waive Proof,
  broaden visibility, or suppress its audit trail.
- [ ] Export/import uses content-hashed revision/template packages with schema,
  Lingua dependencies, extension hashes, license/notices, and signatures.
  Evidence, secrets, grants, model checkpoints, and live state are excluded
  unless independently and explicitly selected.

### Runtime operations and failure semantics

An always-on autonomous engine needs operability as part of its data model.
Queue lag, scheduler mode/cost, Program/model/connector health, denials,
uncertain effects, replay audits, and recovery controls must be readable and
actionable without shell access. Implement health projections alongside each
phase rather than adding metrics after autonomy ships.

- [ ] Publish engine health through Protein: mode, active build/schema, leader/
  sequencer lease, last cursor, deadline lane plans/arms/earliest deadline,
  active/estimated/actual semantic and wake rates and budget, queue depth/oldest
  age by class, runs per state, effect worker health, schedule lag, model
  backlog, storage pressure, and last successful checkpoint/replay audit.
- [ ] Type failures as invalid definition/input, missing/stale/denied data,
  Proof rejection, policy/authority denial, conflict/stale revision, budget/
  fuel exhaustion, adapter unavailable, retryable/terminal/uncertain effect,
  invariant violation, or engine fault. Retry policy follows type, not string
  matching.
- [ ] An unexpected invariant violation enters stage-effects or emergency-stop
  according to configured severity, preserves the replay capsule, stops related
  dispatch, and opens one high-priority operational decision. It never catches
  an error and silently continues acting.
- [ ] Separate user pause, policy denial, program fault, connector outage, and
  global stop so recovery cannot confuse “operator said no” with “try again.”
  Resume shows the occurrences/intents that will become eligible.
- [ ] Enforce CPU/fuel, memory, trace, storage, I/O, network, notification,
  Action, value, and candidate/fan-out quotas per run/program/principal/Cell.
  Maintenance and safety controls retain reserved capacity under overload.
- [ ] Make trace/evidence retention purpose- and sensitivity-aware. Redaction
  produces a new view, not a modified Fact; secret values and unnecessary raw
  personal data never enter general traces in the first place.
- [ ] Periodically replay sampled completed runs from their capsules and compare
  hashes. A mismatch is a determinism incident with engine/revision diff, not an
  ignorable test flake.


### Karma acceptance and proof gates

The architecture is not complete when the happy-path UI works. These are
cross-cutting engine exit gates:

- [ ] A replay capsule produces byte-identical canonical runs, candidates,
  policy decisions, intents, unsigned Fact payloads/content hashes, and captured
  signature/receipt bytes across repeated runs and different host thread
  schedules.
- [ ] DST crashes at every persistence/lease/dispatch/receipt boundary; restart
  loses no accepted occurrence, repeats no intended idempotent effect, resumes
  workflows, and exposes uncertain non-idempotent effects for reconciliation.
- [ ] Duplicate Facts, samples, sync packages, occurrences, decisions, Action
  requests, and effect receipts are idempotent; recorded alternate arrival
  order is replayable and convergence assertions hold where specified.
- [ ] Simultaneous conflicting writers resolve by declared deterministic policy
  and preserve rejected alternatives/explanation; no thread race selects one.
- [ ] Revoking/narrowing a grant while runs are evaluating, staged, leased, or
  about to dispatch prevents every still-preventable effect. Budget reservation
  is atomic under concurrent runs.
- [ ] Visibility/purpose taint applies before input, feature, aggregate, model
  update, explanation, recommendation, optimizer, notification, and external
  effect. Small-cohort/differencing tests reveal nothing outside policy.
- [ ] Decimal/fixed-point quantities, probabilities, decay, conversions,
  schedules/timezones, solvers, seeded algorithms, and pure extensions replay
  identically on supported platforms.
- [ ] DSL → canonical AST → visual graph → DSL round-trips without semantic
  drift. Layout/display-label changes preserve the revision hash; type, node,
  dependency, expression, policy, or effect changes produce a new hash.
- [ ] Millisecond schedule boundaries preserve exact `intended_at` and stable
  cursor ordering when Facts/timers share a millisecond. Late wake-up follows
  skip/coalesce/replay policy and never rewrites intended time.
- [ ] With simultaneous `3ms`, `5h`, daily, and monthly Frequencies, tracing
  proves that a fast wake reads/drains/re-arms only its due dense lane. Sparse
  registrations receive no SQL query, due-check, heap pop, or timer re-arm from
  the `3ms` path, yet still produce their occurrence at the exact intended
  boundary. With only `5h`, the director performs no Frequency work between
  activation and its one-shot wake.
- [ ] Lane assignment contains no fixed cadence classes. Deterministic demand/
  capacity tests split, pack, and merge the same schedules in stable uid order;
  changing host capacity may change only the recorded operational lane plan,
  never semantic occurrence ids/results.
- [ ] One reusable Frequency referenced by ten active consumers has one cursor/
  deadline and fans one occurrence out deterministically. Removing the last
  consumer disarms it; adding the first follows the exact `inactive_gap` policy
  and never surprises the owner with implicit dormant-history replay.
- [ ] A `1ms` Frequency is denied unless its computed demand fits aggregate Cell
  capacity, Program wake/evaluation/write/effect budgets, required dense/
  precision capabilities, and a declared overload policy. The Karma sand displays
  1,000 ticks/second and 86,400,000 ticks/day plus estimated retention before
  activation.
- [ ] Dense `OccurrenceBatch` replay yields the same semantic tick ids,
  state transitions, candidates, intents, and Facts as individual scheduling;
  compacting no-op traces never coalesces requested semantics.
- [ ] For each evidence cursor, all already-active reaction work precedes its
  learning update. A threshold-crossing model update or meta-rule creates a
  later occurrence and cannot change the revision/parameter/checkpoint used to
  process its own evidence.
- [ ] A meta-rule changes `freq:@recovery.reminder_tick` from `1d` to `3d`
  only through a range-scoped grant and parameter Action. All four rephase
  policies produce their specified next boundary, survive restart, and replay.
- [ ] The recurrence reference model proves prior behavior, diminishing update
  influence, half-life decay, cadence, hysteresis, confidence/support,
  negative/censored evidence, drift, rebuild, feedback, deduplication, and
  exclusion of endogenous self-training.
- [ ] A learned pattern can remain observed, create one explained suggestion,
  create an editable draft, or promote a template revision only according to
  its route/grant/shadow policy. No probability value manufactures authority.
- [ ] Every Transfer lifecycle capability is tested both denied-by-default and
  permitted inside an exact delegation. Automation signs only its principal's
  side, respects current revision/domain gates, never treats prediction as
  consent/evidence, and cannot widen visibility through another capability.
- [ ] Automation Trust selectors prove exact Person, origin Organ, via Organ,
  proximity, `any/all/not`, deny-first, concept/direction, stage ceiling,
  threshold, limit, expiry, and blocked-contact behavior. The full structured
  allow/deny proof is available through Protein.
- [ ] A `0.99p` apple need cannot draft/propose an offer outside
  `trust:@apple.known_sellers`; allowlisted Organs and proximity arms behave
  exactly as declared; Trust/grant revocation before dispatch prevents the
  effect; and no Organ scope acts as consent for a Person.
- [ ] Commands, HTTP, models, UI controllers, and microcontrollers prove schema,
  secret redaction, capability scoping, timeouts, retry/idempotency, receipts,
  uncertainty, interlocks, simulation substitution, and manual override.
- [ ] Human UI, CLI, and software agent can perform the same authorized program,
  simulation, candidate, decision, grant, workflow, and intent operations
  through Actions/Protein; none has a hidden database or effect path.
- [ ] Emergency-stop, observe-only, and stage-effects survive reboot; queue/
  workflow/effect disposition is explained before resume and normal inspection
  works without filesystem logs.

Vertical workflows prove that the pieces compose:

- [ ] Economy is the first vertical proof after K0–K10: individual and recurring
  resource gains/losses, correction/void, due-occurrence resolution, exact
  monthly gain/loss/net, tag/source profile, actual/expected resource graph, and
  event/Fact drill-down all run through the real Economy sand. Typed, voice, and
  photo capture later produce the same inert event draft without a privileged
  Fiote path.
- [ ] Todo/knowledge base: a habit re-arms daily and completing it posts a
  causal Fact; missing a day is negative evidence only if the opportunity policy
  says completion was observable.
- [ ] Recurring tasks: a monthly schedule fires exactly once under normal time
  and obeys its chosen catch-up policy after downtime and daylight-saving
  transitions.
- [ ] Adaptive Frequency: a recovery rule tunes another reusable Frequency from
  daily to every three days after seven stable observations, then restores it
  when stability leaves; no same-occurrence or mid-cascade definition change is
  possible.
- [ ] n8n-style command flow: a signal → rule/workflow → leased effect graph is
  built visually, dry-run, executed, inspected, and safely retried.
- [ ] CRM/people: a birthday whisper arrives at the chosen moment and an
  interaction report is one aggregate Protein.
- [ ] Calendar/time budgeting: the projected week renders and moving a promise
  recomputes it without storing a duplicate calendar truth.
- [ ] Health/IoT: a scale posts weight Facts, a streak program reacts, and the
  source off-switch stops new sampling/use/effects; calibration, clock drift,
  malformed data, offline buffering, and actuator interlock are visible.
- [ ] Apple/pantry recurrence: confirmed family consumption grows a decayed
  cadence model; projected shortage plus visible nearby OPEN offers yields one
  explained ranked recommendation around the learned window. A matching
  concept/counterparty Trust scope plus grant may create a local draft;
  publishing/agreement/settlement each require their own Trust ceiling and
  capability.
- [ ] Delegated recurring Transfer: a person explicitly grants one named apple
  program value/quantity/seller/window limits and binds the grant to a Trust
  scope for proposal, own agreement, evidence-qualified confirmation, and local
  settlement. It runs end-to-end, while an unlisted/distant/blocked Organ,
  changed seller/price/revision, exhausted budget, missing evidence, or
  Trust/grant revocation returns to Attention without partial authority.
- [ ] Neighborhood matching: a scoped match rule and visibility grant produce a
  draft in Attention after polling, without widening proximity.
- [ ] Shared family pattern: two People publish permitted pantry evidence and a
  signed percentage/aggregate with exact denominator and window; the consuming
  Cell uses it without exposing hidden members or importing anyone's authority.
- [ ] Chat/calls: “when Transfer Y reaches agreed, ask controller X to open the
  room” uses a typed, single-claim action intent.
- [ ] Interface policy: a program may present/focus a relevant Record on one
  bound device inside attention/accessibility policy, but cannot click agreement,
  forge input, hide warnings, or take over an unbound sand.
- [ ] Games/THE Game: records provide state and a Karma program provides
  the inspectable rulebook without a special game automation core.
- [ ] Garden/farm and inventory/production: watering and threshold programs
  derive work/Needs, ingest moisture/controller receipts, honor physical
  interlocks, projections distinguish actual/available/planned, and settlement
  remains the only quantity truth.
- [ ] Operations research: a week scheduler combines tasks, promises, travel,
  energy preferences, protected time, and hard commitments; it returns multiple
  plans, constraint/slack and infeasibility explanation, and applies only the
  separately approved schedule Actions.
- [ ] Monthly recap: a program selects the month's Facts, drafts the recap, and
  links its evidence without training on its own output.

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
- [ ] **Economy** — the first K11 vertical workflow specified in E0–E3. Replace
  the unwired Finance placeholder with individual/recurring resource gain/loss
  CRUD, monthly gain/loss/net, tag/source profiles, actual/expected trends,
  future Fiote capture review, and event/Fact drill-down over
  `source:"economy"`.
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
  planning, calls, calendar/time budgeting, simple Economy projections,
  social feed.
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
- [x] Current heartbeat order: promise expiry → decision expiry → timers →
  signal sampling → effects (budgeted notify) → senses pass → crossings pass.
  Karma phase K3 replaces the polling heartbeat as timer owner with the
  tickless deadline director/sequencer while preserving explicit stable
  priority.
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
- [x] Time is deliberately NOT a record column: automated timing = Karma
  schedules/Frequencies; declarative time (what strangers match on) lives on promise
  windows.
- [x] Board chrome is frontend state; sand data is Protein/Actions.

## The theory (from the retired blueprint)

**The refounding, three sentences:**

> **Everything is a Record. Every change is a Fact. Every intended change is a Promise.**

**The pillar map:** Record (state) · Memory/Ledger (facts) · Lingua (shared
concepts; Instinct tier = concepts with engine functions) · Karma
(Signals → Context → Senses/Rules/Imagination → Recommendation/Attention →
Policy → Effects) · Transfer (promise bundles under agreement + visibility) ·
Trust (verifiable signed deltas) · Protein (declarative reads) & Actions (typed
writes). Humans and software agents manage the same Karma knobs through
those shared read/write contracts.

**The placement rule (the Window):** the core owns what must be computed,
verified, or agreed across Organs; interfaces own what is seen; embed
honestly what the world already built well. Altitude ladder: Primitive →
Pillar engine → Instinct → Lingua concept/unit → fds sidecar → Sand →
Embedded foreign app.

**Non-negotiables:** quantity stays central (negative = Need, positive =
Contribution, zero = peace); quantity-as-activation on everything; full math
in typed Karma computation graphs; compatibility fully ignored —
greenfield build, old data ported by hand; local-first; no global reputation
score, ever.

**Storage:** SQL is SQLite dialect; Protein is the abstraction that later
permits AniccaDB — replacing `store` must not change one character of the
Protein/Action contract.

**The Window's standing law:** apps are projections of one organism. Economy is
the gain/loss and recurring projection of selected resources (units + exact
Facts + Frequencies + Imagination); inventory and pantry may reuse those core
resource primitives without becoming Economy features. Chat = comments =
negotiation (messages on a shared object); profiles = catalogs = libraries
(published records behind visibility). When a new workflow
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
