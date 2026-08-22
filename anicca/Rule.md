1 — Rule (@rule: 0, is #idea, #instinct, #part-of @karma, #done) { r_K99HSKDH25E128SY5JFMB0F73Y

# 1 — Rule

**Purpose:** A Rule is the whole of Karma's authoring surface. It is three parts
and nothing else, and it is one object so a person never has to learn where the
schedule went.

**How it works:** The Condition computes a number from blocks referenced by
slug. The Threshold decides whether that number means *fire*. The Consequence
happens and receives the number the Condition carried.

```
-1 * freq(@daily) + @apple    →  !=0  →  add-quantity @pear
└──────── Condition ────────┘    Thresh.   └── Consequence ──┘
```

**Interacts:** Reads Record quantities and Frequencies; writes through Actions,
which append Facts, which wake other Rules — that chain is deliberate and is how
one Rule's consequence feeds another's condition.

**Implementation:** `nucleus::karma::{Condition, Gate, Carry, Consequence}` are
the four types and are already right — `Gate` *is* the Threshold in the drawing,
`Carry` *is* "the value carried over by the condition". Storage is
`0038_recurrence.sql` + `store/src/recurrence.rs`; evaluation is
`Engine::react_to` in `crates/engine/src/actions.rs`, with a
`MAX_REACTIONS_PER_CHANGE` cascade guard.

## Later, still Rule

**Purpose:** Rule capabilities that are real intent but do not block the rename
and rewire above. They are parked here rather than in §1 so §1's list stays the
work of this week.

# 2 — Command

**Purpose:** Let something outside the Cell — an API request, a person, a
device — become an input a rule can condition on, and let a rule run a shell
Command as a consequence. Both halves are useful and both are missing.

**How it works:** A Command is a Record. An inbound request appends a Fact
carrying a number, which is a Record moving, so it wakes the rules that read it
through the ordinary change path. No new reading form, no signal table. For
one-shot semantics, give the rule a consequence that sets the Command back to
zero.

**Interacts:** Read side is §1.2's `@slug`. Write side is §1.4's shell
consequence. Anything richer is §7 Signals, which is the same idea with an
adapter contract and health in front of it.

**Implementation:** none yet.

# 3 — Entries and classification

**Purpose:** Individual changes a person types, classified so they can be
totalled without a second ledger.

**How it works:** One-line capture, correction and voiding, plus re-tagging.
Classification attaches to the Fact rather than the Record and is derived, so a
Record's meaning can change without rewriting its history. Aggregation is a
query over classified Facts, never over a stored running total.

**Interacts:** Everything a rule reads is a Fact an entry produced. The Ledger
carries exact `DecimalValue` deltas; history stays queryable after compaction.

**Implementation:** `0036_classification.sql`, `0037_entries.sql`,
`store/src/entries.rs`, `store/src/ledger.rs`, the `CaptureEntry` /
`ReviseEntry` / `VoidEntry` Actions, `Source::Entry`. Shipped and tested.

# 4 — Timeline, calendar and graph

**Purpose:** Draw one Record through settled past, current position and declared
future, so a person can see what a rule will do before it does it.

**How it works:** Past is a query over Facts. Future is §1.5's Frequency run
forward, with each projected beat evaluated against the rule's condition. One
evaluator serves both — projection substitutes ports rather than running a
second interpreter.

**Interacts:** The graph and the calendar in §1.1 are two renderings of this one
query. §13's replay and simulation are the same kernel with a different clock.

**Implementation:** `Source::Timeline`, shipped for the past and present halves.

# 5 — Authority

**Purpose:** Nothing automatic acts without a named, revocable permission, and
every automatic change is attributable to a person.

**How it works:** A grant is a typed record signed by the delegating Person,
scoping a capability family, a selector, a budget and a window. A rule that
wants to act proposes a durable intent; a worker leases it, applies it, records
a typed attempt, and appends an ordinary signed Fact. Revocation cancels
unclaimed work immediately.

**Interacts:** Gates §1.4's consequences whenever they leave the Cell, §6's
effects, and §11's Transfer driving. A likelihood is never permission.

**Implementation:** `0034_karma_grants.sql`, `0035_karma_intents.sql`,
`store::karma::intents`. Durable but inert — **the effect worker does not
exist**, so today "apply this" is a person pressing apply, and the sand's inbox
is the real mechanism rather than a placeholder for one.

# 6 — Effects

**Purpose:** Let a rule reach outside the Cell — a shell Command, an HTTP call,
a device, the interface itself — without any of that becoming a special case in
the kernel.

**How it works:** Every effect is a durable typed intent carrying target, exact
payload, idempotency key and deadline. A microcontroller is not a category: it
is HTTP. Interface control goes through registered host Actions, and durable
desired interface state is a Record, not a transient command.

**Interacts:** Every effect passes §5's grant. §13's simulation replaces every
adapter with a deterministic double.

**Implementation:** the intent table exists; no adapter does.

# 7 — Signals

**Purpose:** Turn the outside world into evidence a rule can read, with the
honesty that it came from outside.

**How it works:** Every capture and integration source is an off-switchable
Signal record. Push, polling and streaming share one adapter contract. Raw
capture is preserved separately from normalized evidence, so a normalization bug
is recoverable.

**Interacts:** Signals feed §1.2's conditions and §8's Senses. AI enters here
and only here — as a visible captured model Signal or an ordinary inert
candidate, never as a privileged path.

**Implementation:** partial; the observation envelope is not defined.

# 8 — Senses

**Purpose:** A pure named recognizer over current Facts, Signals and discovery
data that emits evidence-backed candidates.

**How it works:** A Sense proposes; a person decides. **It cannot write state or
contact another Cell by itself** — that restriction is the whole definition, and
a recognizer that could act would be a rule. In the authoring vocabulary it is
`sense:@slug`, written `sense name = …`, and it appears in a rule as an input,
never as an outcome.

**Interacts:** Its output routes through §9's recommendation contract and §10's
attention policy. **A likelihood is not permission, and a threshold is a routing
policy rather than a truth** — a Sense only has to know it produces evidence.
`confidence(@p)` is a counterparty's Laplace-smoothed kept-promise ratio and
`demand(@concept)` is the current hour's share of trailing 30-day activity —
**neither is a global reputation score**, both are local, purpose-specific, and
computed from visible signed history.

**Implementation:** `create-match-rule { watch_concept, max_proximity,
min_confidence, auto }` is a Record and activates through its quantity;
`max_proximity` is a hard ceiling matching never expands past. `senses_pass`
joins local OPEN promises against the discovery cache each heartbeat using
sign-opposite deltas, aligned meanings, overlapping windows and a confidence
floor, and places ranked drafts in the Decision Queue
(`crates/store/src/senses.rs`).

# 9 — Learning and recommendations

**Purpose:** Notice a pattern a person has not written down, and offer it as
something they can accept, edit or refuse — never as something that happened.

**How it works:** Evidence is admitted by a versioned policy, learned into a
deterministic model, and turned into one lifecycle-managed recommendation per
subject. **A learner never mutates a rule or its own evidence policy.** A high
score never skips a route: routing is policy, not a consequence of confidence.

**Interacts:** Reads §8's Senses and §3's classified Facts; writes only inert
candidates that §10 delivers and a person accepts. Reaction always takes
priority over background learning, which may compute in parallel from immutable
snapshots.

**Implementation:** none. The first model is specified but unbuilt.

# 10 — Attention

**Purpose:** Ask a person something without pestering them.

**How it works:** A decision is durable work requiring a choice; a whisper is its
calm delivery. A decision freezes the question, subject, evidence and options at
the moment it is raised, so answering it later answers the same question.
Ranking is by explicit user priority, urgency, confidence and safety.

**Interacts:** Delivery routes through device Records. Feedback is a first-class
result that §9 learns from.

**Implementation:** the Decision Queue exists and `senses_pass` fills it.

# 11 — Automation Trust

**Purpose:** Say how far automation may go with a given counterparty, in tiers
rather than a boolean.

**How it works:** A higher tier includes willingness for lower stages and
confers none of their authority by itself. The scope is an immutable revision
holding an owner, a selector AST and a ceiling; an explicit deny list is
evaluated first and always vetoes. Transfer parties remain People — an Organ
selector says which Cell, never who.

**Interacts:** Gates §5's grants when the counterparty is another Cell, and
drives Transfer stage by stage.

**Implementation:** none. Transfer's side of it is `docs/Transfer.md`.

# 12 — Workflows and optimization

**Purpose:** Multi-step durable work, and choosing between plans rather than
computing one.

**How it works:** A workflow is a state machine with a declared concurrency
policy and cooperative, observable cancellation. Optimization takes an objective
specification, runs a deterministic solver, and returns a plan *set* with
binding constraints shown.

**Interacts:** Composes with §1's rules and §5's intents. Transaction boundaries
stay narrow — compatible local Actions may commit together, nothing else.

**Implementation:** none.

# 13 — Imagination

**Purpose:** Four products from one kernel — replay a past run, project a
future, simulate a hypothetical, and prove an invariant.

**How it works:** Replay reproduces a past run from captured inputs. Projection
folds rules forward. Simulation runs on an isolated snapshot with a virtual
clock and mocked adapters. Proof has three honest result classes: proved within
a stated bound, counterexample found, or unknown.

**Interacts:** Shares §4's evaluator and §1.5's Cadence exactly. Anything that
would need a second interpreter is a design error.

**Implementation:** the replay capsule is partially specified; nothing runs.

# 14 — Runtime operations

**Purpose:** Make the engine's condition legible, and make degradation explicit
rather than mysterious.

**How it works:** Health is data published through Protein, not log lines. Pause
states are distinguished by cause — user pause, policy denial, rule fault,
connector outage — because they need different answers. An unexpected invariant
violation enters stage-effects rather than continuing.

**Interacts:** §1.5's timer tasks and §5's effect worker both report here.

**Implementation:** none.

## What is left

### 1 — Rule

- [ ] **Rename `recurrence` → `rule` everywhere.** Table, `recurrence_revision`
      → `rule_revision`, `recurrence_skip` → `rule_skip`,
      `store/src/recurrence.rs` → `store/src/rule.rs`, every Action name. Edit
      `0038_recurrence.sql` in place. `rule_skip` keys on
      `(rule_uid, frequency_uid, beat_at)` — a skip is per-beat, like a firing.
- [ ] **Drop `cadence_json` and `anchor_at` from the rule.** They belong to the
      Frequency now. A rule's schedule is whatever `freq(@x)` it reads; a rule
      reading none has no schedule at all.
- [ ] **Two triggers, one evaluator.** A condition holding a Frequency is
      evaluated *only* on its beat. A condition holding none is evaluated *only*
      when a Record or Command it reads changes — extend `Engine::react_to` to
      skip rules carrying a `freq` token, keeping the cascade guard. Both paths
      call one `evaluate_rule(rule, now, cause)` so a beat and a change cannot
      drift.
- [ ] **One target Record per rule** (`record_uid`); consequences apply to it.
      Apple/Pear is already expressible — target `@pear`, condition reads
      `@apple`.

### 1.6 — Later, still Rule

- [ ] **Rules take named parameters** with declared types and ranges, so tuning
      one number is not a revision of the whole rule and a rule can be installed
      as a template and then adjusted.
- [ ] **A rule is reusable inside another rule** through explicit typed inputs
      and outputs, so a shared piece of arithmetic is written once.
- [ ] **A text form of a rule**, round-tripping rule → text → rule without
      semantic loss, so a rule is diffable, shareable and reviewable outside the
      sand. The visual builder stays the primary surface.
- [ ] **A rule may keep durable state across firings** — a counter, a last
      value, a cooldown's clock. It is an immutable event chain with one
      projection, not a mutable field, and §1.3's temporal controls depend on it.
- [ ] **A rule declares what happens when it is triggered while still running**
      — queue, drop, or restart. Deterministic, declared, never emergent.
- [ ] **Simultaneous rules run in a deterministic order** — dependency order
      first, a stable tiebreak after — and conflicting writes resolve by a
      declared policy rather than by whichever landed first.

- [ ] **Add `RecordKind::Command`** and the ingress Action that appends its Fact.
- [ ] **Commands declare** executable identity and hash, typed arguments (no
      string interpolation), working directory, environment allow-list, timeout
      and captured output.
- [ ] **An inbound HTTP request maps to one Command Fact**, authenticated,
      idempotent by request id, and refused rather than queued when unauthorized.

- [ ] **Crash atomicity is not met, and this is known.** The engine appends the
      Fact and its classification in separate statements; a crash between them
      leaves an unclassified Fact.
- [ ] **Archive instead of delete.** Pre-checkpoint Facts move to
      `fact_archive` carrying the classification and unit in force at the time.
      The checkpoint is the past/present boundary — no `is_history` column.
- [ ] **Compaction refuses rather than truncating a window a live rule reads.**
- [ ] **Exact deltas on `promise.delta` and `link.quantity`**, deliberately not
      done yet.
- [ ] **A shorthand capture language**, deterministic and readable, never an
      executable shell: `lose 68.40 @brl on record:@cash from merchant:@market
      on 2026-07-22 #household`. The unit is an ordinary Record reference, which
      is why `@brl` and `@kg` are the same kind of token. A missing target,
      unit or magnitude, or an ambiguous date, yields a question — never a
      guessed Fact.
- [ ] **Retention is keyed on meaning with DAG inheritance**, not on
      `record.kind`, and is purpose- and sensitivity-aware.

- [ ] **Projection writes nothing** — no Fact, no intent, no candidate, no
      cursor advance. Prove it with a test that counts rows before and after.
- [ ] **Return exact points, not floats**, carrying decimals and units end to
      end.
- [ ] **Exclusions are reported, never silent.** A rule that could not be
      projected says so on the timeline.
- [ ] **Fold classified promises alongside rule beats**, so "in three weeks"
      includes what was promised as well as what is scheduled.

- [ ] **Build the effect worker.** Lease an intent, record a typed attempt,
      apply the change, land a signed Fact, release the lease. Retries driven by
      the frozen idempotency key.
- [ ] **Three distinct targets, three distinct capabilities** — a Record's plain
      quantity, a Transfer stage, an external effect. Never one
      `transfer:automatic` boolean.
- [ ] **Reserve on authorization, reconcile on receipt, release on failure.** A
      compensated intent keeps its budget consumed.
- [ ] **Compensation is real, not nominal.** Reversing an applied intent appends
      the reversing Fact; it does not delete the original.
- [ ] **Attribute every automatic Action to both the real principal and the
      rule**, and enforce at the engine and Action boundary, never in a sand.
- [ ] **Freeze the effective grant at proposal time** for explanation, and
      recheck it at apply time.
- [ ] **Role/permission grants themselves become Karma-drivable.** Assigning a
      role to a Person, or granting/revoking a permission, is today only a
      direct auth-table mutation (Interface.md's permissions sand). Once the
      effect worker exists, a Rule's Consequence should be able to do the same
      thing a person does by hand today — grant, revoke, or reassign a role —
      under the same signed-grant/Authority discipline as any other
      Consequence, never a bypass of it. Not started; the direct auth-table
      mutation path stays the only one until this lands.

- [ ] **HTTP connectors declare** method, host and path policy, request and
      response schema, retry policy, and reference secrets by opaque
      capability-bound handle — never by value in a rule.
- [ ] **Device controllers expose** typed commands and state plus physical
      safety limits that a rule cannot argue past.
- [ ] **Distinguish durable desired interface state from a one-off command.**
- [ ] **Every adapter has a simulation double** built at the same time, not
      after.

- [ ] **One observation envelope:** `source_uid, source_revision, observed_at,
      received_at, payload, signature`.
- [ ] **Deactivating a Signal stops new acquisition and downstream triggers**
      without deleting what it already produced.
- [ ] **Source health is data, not logs:** last scheduled, attempted and
      successful acquisition, adapter version, next retry — readable through
      Protein, and usable in a rule.
- [ ] **Event-time nodes declare a late-data watermark and a correction policy.**
- [ ] **Recognition has one capture lifecycle:** `captured → extracted →
      needs_review | ready → applied | rejected`. Every inferred field records
      its recognizer and version, input hash, candidate value, confidence,
      source span and alternatives. A person may edit any field; that edit never
      rewrites the captured model output. Raw audio and photos can be discarded
      after review while keeping the hash and the structured evidence.

- [ ] **Split the name.** Recurrence probability, estimate confidence,
      counterparty evidence, expected utility and authority eligibility are five
      different claims and `confidence()` currently blurs them.
- [ ] **Refine counterparty evidence by meaning, role and window**, and show
      kept, broken and outstanding counts beside the number.

- [ ] **A deterministic decayed Beta/cadence model** as the first and only
      learner, stored as typed policy rather than frontend state: prior,
      evidence window, decay, update rule.
- [ ] **Separate opportunity and exposure from positive, negative and censored
      outcomes.** An absent Fact is not a negative one.
- [ ] **Persist each update** with the prior checkpoint hash and the admitted
      and rejected evidence, so any number can be explained backwards.
- [ ] **Model lifecycle:** `cold → learning → calibrated → drifting → retired`,
      with a deterministic train/validation split and declared candidate
      features — no combinatorial context mining.
- [ ] **Recommendation states:** `open, accepted, accepted-edited, dismissed,
      snoozed, muted, expired`, with typed contextual feedback.
- [ ] **Detect action/recommendation loops** — if accepting a suggestion creates
      the evidence that regenerates it, stop and say so.
- [ ] **Explanations at several depths:** one sentence, substituted values, full
      evidence.

- [ ] **Idempotent whisper uid with per-channel delivery state**, so a retry is
      not a second interruption.
- [ ] **Explicit escalation:** retry a channel, change channel, notify another
      person — each a declared step, never an emergent one.
- [ ] **Attention policy reserves capacity** for safety and expiring decisions,
      and persists quiet hours, recipients and per-channel thresholds as durable
      policy rather than as a device setting.
- [ ] **No coercive ranking, synthetic urgency, dark patterns, or hiding the
      "do nothing" option.** This is a build constraint, not a sentiment.

- [ ] **Persist as a Record** (`kind=automation_trust_scope`, quantity is the
      tier) with an immutable revision chain, and extend `source:"karma"` with
      `object_kind="trust_scope"`.
- [ ] **Policy evaluation returns a structured proof, never just `false`** —
      which clause matched, and what would have to change.
- [ ] **Never use locally inferred counterparty probability as their consent.**
- [ ] **Autonomy is chosen per step** — "always ask before publishing" is a
      different setting from "always ask before settling".
- [ ] **Keep payment execution separate from Transfer settlement.**

- [ ] **Workflow nodes:** state machine, sequence, parallel, wait-until,
      timeout, compensate.
- [ ] **Deterministic solver adapters** — linear, mixed-integer, constraint —
      translating Records, Facts, assertions, Promises, availability and time
      windows into decision variables.
- [ ] **Distinguish forecast from plan from schedule.** A forecast estimates
      what will happen; a plan chooses; a schedule commits.
- [ ] **Reoptimization preserves stability** through explicit change penalties,
      so a small input change does not reshuffle a person's week.

- [ ] **Put clock, scheduling, entropy, uid generation, filesystem, network and
      model access behind injected ports.** Nothing else makes replay possible.
- [ ] **Canonicalize maps, sets, strings, units, timestamps and serialization**,
      and give every stochastic algorithm a recorded seed.
- [ ] **Nondeterministic model output is an observation** with model id, request
      and response recorded — never a decision.
- [ ] **Upgrades never reinterpret an old run silently.**
- [ ] **Deterministic fault injection and shrinking** for time jumps, crashes,
      duplicate delivery and clock skew, preserving the violated invariant.
- [ ] **Shadow mode** runs a candidate revision beside the active one without
      effects.

- [ ] **Publish engine health through Protein:** mode, active schema, task
      liveness, queue depth, oldest unclaimed work.
- [ ] **Type failures** as invalid definition, missing or stale or denied data,
      adapter error, budget exhausted, timeout.
- [ ] **Cell modes:** `normal`, `stage-effects`, `observe-only`,
      `emergency-stop` — each surviving reboot.
- [ ] **Enforce CPU, memory, storage, I/O and notification limits** per rule.
} r_K99HSKDH25E128SY5JFMB0F73Y
