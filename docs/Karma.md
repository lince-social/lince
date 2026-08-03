# Karma

Karma is the part of Lince that watches Records, does math on them, and changes
them — on a beat, or when something moves — with a person's permission and a
readable trail. Records model the world, Transfer coordinates change between
people, **Karma turns evidence into action.**

It is useful without an LLM. People author exact rules visually, use transparent
statistics and deterministic algorithms, and plan over their own goals. A model
may be attached as an explicit Signal or author an inert candidate; **model
access is optional and grants no special authority.** "Always on" means a
supervised runtime keeps draining durable work — never always interrupting,
always connected, or always authorized to act.

## How to read this file

Every feature is written the same way: **Purpose**, **How it works**,
**Interacts**, **Implementation** — a sentence or two each, three paragraphs at
the outside. Everything after that block is checkboxes.

**Prose is what exists. A checkbox is what is left.** When a task lands, delete
its box and fold the resulting behaviour into the prose above it — or keep it as
an `[x]` when it is a decision or a sharp edge a future reader would otherwise
fight. `[x]` is never an inventory of finished work.

Sections are in build order and **§1 Rule is the current goal, the sand first.**
Everything from §5 down is real intent, not yet due. §17 is demolition — the
apparatus §1 replaces. §18 is the list of things this must eventually be able
to do, and §19 is what "done" means.

## Standing rules — not checkable work

- Work on `dev`, no worktrees. `cargo check` and narrowest per-crate tests,
  never a full workspace build.
- Nothing domain-specific in the backend or the database. No money type: a gain
  is a positive delta, a currency is a Unit Record, "worth five of theirs" is a
  rule someone wrote. Economy is a preset of the Karma sand, not a sand.
- Sands read Protein and write typed Actions, nothing else. Board chrome is host
  state, never a Ledger Fact.
- Everything runs on `nucleus → store → engine → protein → transport`.
- Modelling vocabulary is `docs/Ontology.md`'s: Records, Assertions, Concepts,
  Lingua. Karma never invents a second one.
- Migrations that have not shipped get edited in place; local databases are
  expendable. Once one ships it is frozen.
- `crates/web/tests/sand_boundary.rs` fails if `economy`, `money`, `currency` or
  `finance` appears in backend code. Prose is exempt on purpose.
- One word in the UI: **Rule**. Not recurrence, not program, not occurrence.
  Karma is the pillar's name; a Rule is what you make.

---

# 0 — Exact numbers and frozen types

**Purpose:** Fix the small vocabulary every other section spends, so a number
means one thing everywhere and a wire format never drifts under a running Cell.

**How it works:** One decimal type on every durable path and no binary floats in
any decision. Probability and confidence are different types, because a
likelihood that something recurs and a confidence in an estimate are different
claims. Time atoms are signed integer milliseconds. There is no money type: a
currency is a Unit Record, so `12.50 @brl` is a `Quantity`.

**Interacts:** §1.2's arithmetic, §3's Ledger and §13's replay all depend on
these being exact and canonical.

**Implementation:** `nucleus::DecimalValue` stored as `(mantissa TEXT, scale
INTEGER)`, summed in Rust as `i128` at a common scale and never in SQL.
`fact.delta` and `record.quantity` are an exact pair with `REAL` removed. One
named lossy inbound door, `NewFact::quantity_f64`. Canonical JSON sorts object
keys and preserves array order; failures are a stable machine `FailureCode` plus
a safe message.

- [x] **Declared precision survives sync**, and the exact pair is inside the
      hash preimage, so two Cells agree on the bytes and not merely the value.
- [ ] **Every new public wire type extends the golden fixture in the same
      commit.** A type that ships without one is how the format drifts.
- [ ] **Exact multiply and divide.** Multiplication states its result scale,
      division states its rounding, scale overflow is a publish-time refusal and
      a zero divisor a runtime one. Unit algebra is explicit, never inferred.

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

## 1.1 — The sand

**Purpose:** Create a rule, read it, revise it, retire it, and see the Record it
acts on drawn through its past, present and declared future. This is the first
goal and everything else in the file waits on it.

**How it works:** The side panel leads with the builder — condition input,
threshold select, consequence picker — then Frequency CRUD, then a divider
saying *slop down here* with every older surface beneath it. Typing `record(`,
`freq(` or `@` autocompletes over every referenceable slug, and Tab inserts the
full reading form: `freq(@daily)` for a Frequency, `@apple` for a Record.

**The three parts show as three parts** — not a form of twelve equal fields.
A Frequency is created in its own section and shown in words ("every 1 day");
**a rule never defines one inline**, which is exactly what made them
un-reusable before.

**Interacts:** Protein for every read (`Source::Recurrence`, `Source::Frequency`,
`Source::Record`, `Source::Timeline`), typed Actions for every write. It owns no
primitive and must never cause a surface-named type in a backend crate.

**Implementation:** `crates/web/src/sand/karma/` — `body.rs`, `app/blocks.js`
(the pure completion grammar), `app/builder.js` (DOM and reuse banks),
`app/frequency.js`. `karma_sand_js.mjs` covers the grammar; `mod.rs` asserts the
panel ordering.

- [x] **A record is findable by head or slug and inserts at the caret.** The
      condition field is a plain `<input>`, not a contenteditable, because the
      caret *is* the interaction and `selectionStart` answers it exactly.
- [x] **Conditions and consequences are reusable across rules** with no new
      table — the banks are derived from the distinct condition strings and
      consequence shapes of existing rules, ranked by use count.
- [ ] **Clicking a record opens the record sand** as well as inserting it.
      Blocked: `handleShellAction` gates on `card.system && card.pinned`
      (`web/static/presentation/board/main.js:1576`) and karma is neither. Needs
      a narrowly-scoped open-record command rather than widening that gate.
- [ ] **Live parse feedback.** Parse on every keystroke, show `ConditionError`'s
      `Display` under the input, refuse save while it does not parse.
- [ ] **Calendar and graph from §1.5's projection** — beats ahead on a calendar,
      projected quantity on a line, both from the pure function, no new storage.
- [ ] **The sand resizes.** It is pinned to a maximum size today; it should grow
      and shrink.
- [ ] **A command palette** as operational sugar that never bypasses an Action.
- [ ] Still open from the older surface: monthly dashboards, source profiles,
      capture review.

## 1.2 — Condition

**Purpose:** Compute one exact number from named blocks, so that what a rule
watches is readable as arithmetic rather than assembled from form fields.

**How it works:** An expression over three kinds of block and nothing else.
`@slug` is a Record's quantity, and is also how a Command is read. `freq(@slug)`
is a Frequency's beat. `sum(@slug, 30d)` / `sum_pos` / `sum_neg` are windowed
totals. Anything else is refused at parse time and never defaulted to zero — a
typo must not read as "the stock is empty" and fire for the most alarming
possible reason.

**Interacts:** `Engine::rule_reads` turns a condition into the set of Records
whose movement wakes it, and already skips `freq` tokens so a rule is not woken
by the Frequency Record itself moving.

**Implementation:** `nucleus::karma::Condition` over `nucleus/src/expr.rs`,
resolved by `read_for_condition` in `crates/engine/src/actions.rs`. Exact
decimals end to end, no binary floating point in a decision. A Record's unit and
its other numbers are readable alongside its quantity, and a missing one is an
error rather than a silent zero.

- [ ] **At most one Frequency per condition, refused at parse time.**
      `Condition::parse` fails on a second `func == "freq"`. Parse-time, not
      runtime, so each rule belongs to exactly one timer task unambiguously.
- [ ] **A condition that reads nothing is refused.** No blocks means no trigger.
- [x] **`freq(@daily)` is the token form and the grammar does not change.** It
      already parses in `nucleus/src/expr.rs`; the `@freq(dai`+Tab typing
      experience is the editor's job. If the namespaced spelling still reads
      better later it is a lexer change plus a rewrite of every stored
      `condition_src` — decide then, not now.
- [ ] **Drop `value(@slug)` and `derived_value`.** Reading another rule's number
      without running it needs recursion, a depth cap, cycle detection and
      transitive wake-up that `reads()` cannot express. Chaining through
      committed Facts already does the useful half.
- [ ] **Refuse a unit mismatch at publish time**, not at first run. Conversion is
      exact and explicit, never an implicit coercion to make an expression
      type-check.
- [ ] **Persist rounding notes** so a Why lens can show them.
- [ ] **Read a number out of a namespaced Record extension**, so a rule can use
      a Record's weight, price and count and not only its quantity. A missing
      one is an error, never a silent zero.
- [ ] **A saved Protein view is a block**, reducing to exactly one number from
      exactly one row. A view that fails to execute blocks its rule as a
      refusal, never as an error to be logged. The legacy engine had this; §1.2
      currently does not.
- [ ] **A deleted Record blocks the rule; it does not read as zero.**

## 1.3 — Threshold

**Purpose:** Decide whether the number the Condition produced means *fire*, kept
deliberately small so the interesting part stays in the arithmetic.

**How it works:** *only non-zero*, *anything*, or a comparison against a number.
That is the whole vocabulary. When it passes, the Condition's value is carried
into the Consequence.

**Interacts:** Nothing but the Condition above it and the Consequence below it.

**Implementation:** `nucleus::karma::Gate` and `nucleus::karma::Carry`, built and
tested. The sand renders it as one select.

- [ ] **Specify each gate's transition behaviour** — edge or level trigger,
      hysteresis, and what a re-fire on an unchanged reading means. A threshold
      exposes *entered* and *left* as two distinct events, not one boolean.
- [ ] **Debounce:** a reading must hold for a declared span before it counts, so
      a value flickering across the line does not fire ten times.
- [ ] **Cooldown:** after firing, refuse for a declared span.
- [ ] **Rate limit:** at most `max` firings in a rolling window.
      These three are temporal controls on the Threshold, not new rule kinds,
      and each needs its state persisted so a restart does not forget a
      cooldown.

## 1.4 — Consequence

**Purpose:** The thing that actually happens, receiving the number the Condition
carried.

**How it works:** An ordered, non-empty list — capture an entry, set or add a
quantity, assert or retract an ordinary Record assertion, or set an identity
assertion. A list rather than one, because `@wip → @done` is one intention and
must be one rule.

**Interacts:** A consequence appends an ordinary signed Fact on the target
Record, which is a Record moving, which wakes any rule reading it. Anything
leaving the Cell goes through §5's grant and §6's intent instead of firing
directly.

**Implementation:** `nucleus::karma::Consequences`, stored as
`consequences_json`, each entry dispatching to the typed Action that already
existed. Assertion semantics are `docs/Ontology.md`'s.

- [x] **`set-quantity` is not a delta.** Assigning `-1` is not a movement of
      `-1`, and applying a beat writes one entry marking it done which never
      carries the value as well — letting it would move an `add-quantity` rule's
      figure twice.
- [x] **Changing an assertion moves a card between Kanban columns**, since
      columns bucket by value, range or assertion. Removing a Record's identity
      assertion stays refused.
- [ ] **Shell Command as a consequence** — declared executable identity and
      hash, typed arguments, no string interpolation, timeout, output capture.
- [ ] **A computed Record refuses conflicting writes.** If a Record's quantity
      is defined by a rule, a manual write is a refusal with a reason, not a
      silent overwrite.
- [ ] **Cycles are refused at proof time**, not discovered at runtime.
- [ ] **A consequence has a route: none, propose, or apply.** *Propose* puts a
      draft in front of a person; *apply* must name a narrow grant (§5).
      Without one, time never changes a quantity by itself. The route is a field
      on the consequence, not a different kind of rule.
- [ ] **Ask, notify and emit-promise** as consequences, so a rule can raise a
      question (§10) or open a Promise instead of only moving a number.

## 1.5 — Frequency

**Purpose:** Repeating time, as a named object a condition can reference. It is
three things and no more: **you set it** ("every day"), **it fires** the
conditions that read it when the time comes, and **it projects** — running it
forward without firing anything is the calendar and the graph. Nothing further
is needed for calendar or recurrence features.

**How it works:** A Frequency is a slug and a step — `uid, slug, every,
anchor_at`, where `every` is a `CadenceStep`. It is a block *inside* a
condition, never a property of the Rule.

**Interacts:** `nucleus::karma::Cadence` enumerates its beats purely —
`between(anchor, from, to)`, `next_on_or_after`, `civil_at(anchor, index)` — with
no database and no cursor. **Firing a rule and projecting it forward are the
same call**; only running the consequences differs. That is why the calendar and
the graph need no storage.

**Implementation:** `0039_frequency.sql`, `store/src/frequency.rs`,
`CreateFrequency`/`DeleteFrequency`, `Source::Frequency`, and `freq(@x)`
resolving the table in `read_for_condition`. `Cadence` is untouched and stays
that way.

- [x] **A step is a sum of components** applied largest unit first — years and
      months against the anchor's own day-of-month (a too-short month clamps or
      skips per `invalid_day`), then weeks through milliseconds as an exact
      duration, optionally rolled forward onto an allowed weekday. So `1 month +
      1 day + 1 second + 10 milliseconds, then forward to Friday` is one step,
      and every component is typeable in the sand.
- [x] **The slug is the only way in.** Charset validated, zero step refused,
      creation idempotent by `request_id`, deletion refused while a rule's
      `condition_src` still names it.
- [x] **`freq(@x)` reads the table with `(since, at]` edges**, falling back to
      the old rule-carried rhythm so rules written before it keep working.
- [ ] **One tokio task per distinct Frequency read by an active rule.** Not one
      loop over all frequencies, and not one task per frequency that merely
      exists. Each task: `next_on_or_after(now)`, `sleep_until` it, wake,
      evaluate every rule whose condition reads it, repeat. A `1s` and a `1d`
      frequency then cost one wake each per their own period, which is the
      entire point.
- [ ] **Lifecycle.** Spawn when a rule reading it becomes active; abort when the
      last one stops or the step changes, then respawn. `JoinHandle`s in a map
      keyed by frequency uid on the Engine.
- [ ] **Missed beats on boot.** `between(anchor, last_fired, now)` gives them;
      fire those, then enter the sleep loop. Safe every boot because firing is
      keyed `<rule_uid>:<frequency_uid>:<beat_at>` on `entry_revision`'s
      existing `UNIQUE(request_id)`.
- [ ] **`freq(@x)` becomes a beat indicator** — 1 on the beat, 0 otherwise —
      once the timer tasks land, and the `since` plumbing goes with it.
- [ ] **Civil resolution at fire time.** Wire the read path to a timezone
      provider so a rule that must land on a local Friday does.
- [ ] **Projection is bounded.** Five years of a monthly Frequency is sixty
      evaluations; a `1s` Frequency over the same span is not a query. Refuse
      with the count rather than hanging.
- [ ] **The missed policy is declared, not assumed** — fire every missed beat,
      coalesce them into one, take only the latest, or skip to the next anchor.
      A Cell asleep three weeks owes three Mondays only if it said so.
- [ ] **Changing a step declares its rephase policy** — keep the anchor, or
      re-anchor from now. Silently doing either is a bug in one direction or the
      other.
- [ ] **Admission: a Frequency the Cell cannot serve is refused at activation,
      not discovered at runtime.** A `1ms` beat is a real request and a real
      cost; say no with the arithmetic, and offer a shadow run.
- [ ] **Reuse is the point** — ten rules reading one Frequency wake from one
      timer, not ten.

## 1.6 — Later, still Rule

**Purpose:** Rule capabilities that are real intent but do not block the rename
and rewire above. They are parked here rather than in §1 so §1's list stays the
work of this week.

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

- [ ] **Add `RecordKind::Command`** and the ingress Action that appends its Fact.
- [ ] **Commands declare** executable identity and hash, typed arguments (no
      string interpolation), working directory, environment allow-list, timeout
      and captured output.
- [ ] **An inbound HTTP request maps to one Command Fact**, authenticated,
      idempotent by request id, and refused rather than queued when unauthorized.

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

- [ ] **Projection writes nothing** — no Fact, no intent, no candidate, no
      cursor advance. Prove it with a test that counts rows before and after.
- [ ] **Return exact points, not floats**, carrying decimals and units end to
      end.
- [ ] **Exclusions are reported, never silent.** A rule that could not be
      projected says so on the timeline.
- [ ] **Fold classified promises alongside rule beats**, so "in three weeks"
      includes what was promised as well as what is scheduled.

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

- [ ] **HTTP connectors declare** method, host and path policy, request and
      response schema, retry policy, and reference secrets by opaque
      capability-bound handle — never by value in a rule.
- [ ] **Device controllers expose** typed commands and state plus physical
      safety limits that a rule cannot argue past.
- [ ] **Distinguish durable desired interface state from a one-off command.**
- [ ] **Every adapter has a simulation double** built at the same time, not
      after.

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
- [ ] **Fiote is a client of that contract, not a privileged path.** Parse "I spent 42 reais on lunch", transcribe a note, read a receipt, rank existing Records — always returning field candidates, never a final mutation, and never silently creating an ambiguous Record. **A model score is not a grant.** Deferred, but the boundary is fixed now so it stays narrow. Be able to run an model to look at your DNA and change it to fit your needs:
      - [ ] Creating components for the frontend.
      - [ ] Suggesting Karma, or more Lince ways of doing things.
      - [ ] Doing imperative changes like: change this, start a call with someone, i did this task...

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

- [ ] **Split the name.** Recurrence probability, estimate confidence,
      counterparty evidence, expected utility and authority eligibility are five
      different claims and `confidence()` currently blurs them.
- [ ] **Refine counterparty evidence by meaning, role and window**, and show
      kept, broken and outstanding counts beside the number.

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

# 10 — Attention

**Purpose:** Ask a person something without pestering them.

**How it works:** A decision is durable work requiring a choice; a whisper is its
calm delivery. A decision freezes the question, subject, evidence and options at
the moment it is raised, so answering it later answers the same question.
Ranking is by explicit user priority, urgency, confidence and safety.

**Interacts:** Delivery routes through device Records. Feedback is a first-class
result that §9 learns from.

**Implementation:** the Decision Queue exists and `senses_pass` fills it.

- [ ] **Idempotent whisper uid with per-channel delivery state**, so a retry is
      not a second interruption.
- [ ] **Explicit escalation:** retry a channel, change channel, notify another
      person — each a declared step, never an emergent one.
- [ ] **Attention policy reserves capacity** for safety and expiring decisions,
      and persists quiet hours, recipients and per-channel thresholds as durable
      policy rather than as a device setting.
- [ ] **No coercive ranking, synthetic urgency, dark patterns, or hiding the
      "do nothing" option.** This is a build constraint, not a sentiment.

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

- [ ] **Persist as a Record** (`kind=automation_trust_scope`, quantity is the
      tier) with an immutable revision chain, and extend `source:"karma"` with
      `object_kind="trust_scope"`.
- [ ] **Policy evaluation returns a structured proof, never just `false`** —
      which clause matched, and what would have to change.
- [ ] **Never use locally inferred counterparty probability as their consent.**
- [ ] **Autonomy is chosen per step** — "always ask before publishing" is a
      different setting from "always ask before settling".
- [ ] **Keep payment execution separate from Transfer settlement.**

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

- [ ] **Workflow nodes:** state machine, sequence, parallel, wait-until,
      timeout, compensate.
- [ ] **Deterministic solver adapters** — linear, mixed-integer, constraint —
      translating Records, Facts, assertions, Promises, availability and time
      windows into decision variables.
- [ ] **Distinguish forecast from plan from schedule.** A forecast estimates
      what will happen; a plan chooses; a schedule commits.
- [ ] **Reoptimization preserves stability** through explicit change penalties,
      so a small input change does not reshuffle a person's week.

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

# 14 — Runtime operations

**Purpose:** Make the engine's condition legible, and make degradation explicit
rather than mysterious.

**How it works:** Health is data published through Protein, not log lines. Pause
states are distinguished by cause — user pause, policy denial, rule fault,
connector outage — because they need different answers. An unexpected invariant
violation enters stage-effects rather than continuing.

**Interacts:** §1.5's timer tasks and §5's effect worker both report here.

**Implementation:** none.

- [ ] **Publish engine health through Protein:** mode, active schema, task
      liveness, queue depth, oldest unclaimed work.
- [ ] **Type failures** as invalid definition, missing or stale or denied data,
      adapter error, budget exhausted, timeout.
- [ ] **Cell modes:** `normal`, `stage-effects`, `observe-only`,
      `emergency-stop` — each surviving reboot.
- [ ] **Enforce CPU, memory, storage, I/O and notification limits** per rule.

# 15 — Shared and collective Karma

**Purpose:** Let a household, a team or a neighbourhood compute something
together without any of them handing over their raw history.

**How it works:** A Cell publishes one of four typed products: visible raw
evidence, a signed aggregate, a content-hashed rule definition, or a
recommendation. Visibility applies *before* aggregation and taint is tracked
through every derived value.

**Interacts:** Rides §11's trust scopes and Transfer's existing visibility.

**Implementation:** none.

- [ ] **A percentage always includes numerator, denominator and cohort
      definition.** A bare percentage is not a shareable product.
- [ ] **Sharing a live rule means sharing a content-hashed definition**, not a
      running process.
- [ ] **Revocation stops future export and use and cancels eligible queued
      work.**

# 16 — The Flow Plane

**Purpose:** One zoomable map of everything a Cell can observe and everything it
can do, so a person can see causality instead of inferring it from a list.

**How it works:** Time runs left to right, causal and resource lanes top to
bottom, with five composable views over the same graph — definition, live,
history, projection, simulation. Editing produces a typed revision draft that
validates before it activates.

**Interacts:** This is the Karma sand's map view, built *after* §1.1's rule CRUD.
It is not a second sand.

**Implementation:** `crates/web/src/sand/karma/app/canvas.js` and `graph.js` are
the beginnings.

- [ ] **Enumerate every declared source port and every reachable outcome path**
      before it happens.
- [ ] **Make every "why" navigable in both directions** — changed Fact → the run
      that changed it → the rule → the grant, and back down.
- [ ] **Make data scope and authority visible on the graph** — taint paths and
      ceilings drawn, not documented.
- [ ] **Dry-run one node or a whole rule** against current or simulated inputs.
- [ ] **Store canvas layout as host state**, never as a Ledger Fact.
- [ ] **Large graphs** use server-projected dependency slices and stable ids.
- [ ] **Installable templates are ordinary disabled rules**, not a second format.
- [ ] **Every control is also a typed Action**, and Protein exposes capability
      booleans with stable blocking reasons beside every one of them, so an
      agent can do exactly what a person can do and nothing more.
- [ ] **Keyboard and screen-reader reachable**, authoring and emergency controls
      alike.

# 17 — Demolition

**Purpose:** §1 replaces a second, larger rule engine built for the same job. It
is listed here because deleting it is real work with a real order, not because
any of it is worth keeping.

Establish by test coverage first, then delete with its tests, one deletion per
commit.

- [ ] **Delete `fire_due_rules` from `heartbeat`** (`engine/src/lib.rs:460`).
      This is the live 60-second sweep, running in production through
      `web/src/lib.rs:726` (`HEARTBEAT_PERIOD_SECS = 60`). The heartbeat keeps
      promise expiry, decisions and effects; it stops being how rules run.
      `RECURRENCE_CATCH_UP_DAYS` goes with it.
- [ ] **Delete the old Frequency apparatus.** `0027_karma_frequencies.sql` is
      three tables — `karma_frequency`, `karma_frequency_revision`,
      `karma_frequency_activation` — with revision hashes, activation epochs,
      effective-parameter hashes and immutability triggers, for an object whose
      content is "every 1 day". `0039_frequency.sql` already replaced it.
- [ ] **Delete the occurrence machinery:** `0028_karma_schedule_cursors.sql`,
      `0029_karma_occurrence_ingress.sql`, and the cursor, dispatcher, epoch,
      deadline-fabric, dense-batching and admission code that serves them. §1.5
      needs no cursor because `Cadence` is pure.
- [ ] **Migrations `0026`–`0035` all fold away** into what §1 keeps.
- [ ] **Delete the second schedule vocabulary.** `ElapsedSchedule`,
      `CalendarSchedule` and the legacy `store::freqs` are three spellings of
      `Cadence`.
- [ ] **Reduce `nucleus/src/karma/` to six files:** `condition.rs`,
      `consequence.rs`, `cadence.rs`, `exact.rs`, `value.rs`, `time.rs`.
      Everything else there — `ast.rs`, `dispatcher.rs`, `dsl.rs`, `durable.rs`,
      `evaluate.rs`, `execution.rs`, `occurrence.rs`, `proof.rs`, `replay.rs`,
      `schedule.rs`, `state.rs`, `frequency.rs` and the rest — is unreachable
      once §1 lands.
- [ ] **Do not build a legacy import path.** The local database was deleted
      deliberately so this would be free.

# 18 — What this must be able to do

**Purpose:** The list of things a person should be able to build with the
sections above. It exists so a redesign that simplifies the machinery cannot
quietly drop a capability — if one of these stops being expressible, the
simplification went too far.

Each is one vertical someone actually wants, and each is a test as much as a
feature. None require domain-specific backend code; they differ only in which
Records exist and what they are called. **These are bullets, not checkboxes:
they are what to test §1–§16 against, not work to schedule on their own.**

- **Economy** — one-off and recurring gains and losses, categories,
      budgets as queries, currency as a Unit Record.
- **Pantry / inventory** — consumption grows a decayed estimate, a
      threshold proposes a restock.
- **Habit** — a daily beat re-arms a task; completing it is an ordinary
      user Action and the rule never owns a private "completed" flag.
- **Todo and knowledge base** — completing a task posts a Fact another rule
      reads.
- **Recurring tasks** — a monthly beat fires exactly once, including after
      a three-week outage.
- **Adaptive Frequency** — a rule tunes another rule's Frequency inside a
      declared range, under a grant that permits only that parameter.
- **Command flow** — a Signal triggers a rule which runs a leased effect
      chain, the n8n-shaped case.
- **CRM** — a birthday whisper arrives at the chosen moment.
- **Calendar and time budgeting** — the projected week renders, and moving
      one thing re-projects the rest.
- **Health and IoT** — a scale posts Facts, a streak rule reacts, a device
      effect fires under its own capability.
- **Delegated recurring Transfer** — one named rule may drive one Transfer
      to one counterparty, and nothing else.
- **Neighborhood matching** — a scoped match rule plus a visibility grant
      produce a draft, never an action.
- **Shared family pattern** — two People publish permitted evidence and get
      a joint aggregate neither could compute alone.
- **Chat and calls** — "when this Transfer reaches agreed, ask the
      controller to open a channel".
- **Interface policy** — a rule may present or focus a Record on a chosen
      surface.
- **Games** — Records hold state, a rule holds the loop.
- **Garden and farm** — watering and threshold rules over device readings.
- **Operations research** — a week scheduler combining tasks, promises,
      travel and constraints into a plan set.
- **Monthly recap** — a rule selects the month's Facts and drafts the
      summary for review.

# 19 — Proof gates

**Purpose:** What "built" means, stated once, so a feature is not called done
because it demonstrated once by hand.

These are cross-cutting: each one spans several sections and none is satisfied
by a unit test on a single function. **Bullets, not checkboxes** — they are the
bar each section's own boxes have to clear.

- **Replay is byte-identical.** The same capsule produces the same run,
      candidates and effects, twice.
- **No crash loses work.** Interrupt at every persistence, lease and
      dispatch boundary; nothing is lost and nothing is applied twice.
- **Duplicates are absorbed.** Facts, samples, sync packages, beats,
      decisions and Actions all deduplicate on their declared key.
- **Revocation has teeth mid-flight.** Narrowing a grant while runs are
      staged or leased stops them, and says so.
- **Exactness holds end to end** — decimals, probabilities, decay,
      conversions and rounding, with no float on any durable path.
- **Simultaneous `3ms`, `5h`, daily and monthly Frequencies** each keep
      their own beat, and a slow one does not starve a fast one or vice versa.
- **Millisecond boundaries** preserve exact intended times.
- **A learned pattern can stay observed, become one explained suggestion,
      and be refused** — without ever acting on its own.
- **Every Transfer capability is tested denied-by-default and granted.**
- **Human, CLI and agent perform the same authorized rule identically**,
      through the same Actions.
- **Emergency-stop, observe-only and stage-effects survive reboot.**
- **Every vertical in §18 runs end to end** on a fresh Cell.

---

## Appendix A — where the code goes

`nucleus` holds pure types and arithmetic. `store` holds tables and queries and
nothing that decides. `engine` holds evaluation, reaction, timers and Actions.
`protein` holds reads. `transport` and `web` hold surfaces. The supervisor owns
an injected `RuntimePorts` bundle — clock, scheduler, entropy, uid, network — so
§13's replay is possible at all.

- [ ] **A small fixed set of long-lived tasks**, plus §1.5's one per active
      Frequency, and nothing proportional to rule count.
- [ ] **Channels are bounded** and carry stable uids and small commands, never
      large payloads.
- [ ] **SQLite constraints, not process memory, guarantee correctness.** Unique
      keys and transactions, so a restart cannot double-apply.

## Appendix B — the old vocabulary, mapped

| Old word | Now |
| --- | --- |
| Program, graph, node | Rule |
| Recurrence, `karma_frequency`, `ElapsedSchedule`, `CalendarSchedule`, `freqs` | Frequency + `Cadence` |
| Occurrence, cursor, tick | A beat, computed on demand |
| Recognizer | Condition (§1.2), or a Sense (§8) when it only proposes |
| Gate | Threshold (§1.3) |
| Carry | the value a Condition carries |
| Economy | a preset of the Karma sand |
| Rebirth | retired, unused |

This file absorbed `Sand: Karma.md`, `Central: Rule.md`, `Central: Command.md`
and `Central: Senses.md` on 2026-08-01 — all four deleted, history in git. It is
now the whole of Karma: Rule, Command, Senses, Trust and the sand all live here.
Siblings it leans on: `docs/Ontology.md` for the modelling vocabulary,
`docs/Tool.md` for Protein and Actions, `docs/Transfer.md` for the counterparty
half of §11, `docs/Simulation.md` for §13.

