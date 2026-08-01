# Karma

Karma is the part of Lince that watches Records, does math on them, and changes
them on a schedule or when something happens — with a person's permission and a
readable trail. Records model the world, Transfer coordinates change between
people, **Karma turns evidence into action.**

It is useful without an LLM. People install preset programs, author exact rules
visually, use transparent statistics and deterministic algorithms, and run
planning over their own goals. A model may be attached as an explicit Signal or
author an inert candidate; **model access is optional and grants no special
authority.** The engine is typed deterministic machinery, not a personality.

"Always on" means a supervised runtime starts with the Cell and keeps draining
durable work. It does **not** mean always interrupting, always connected, or
always authorized to act. A person can pause one program, one capability family,
all external effects, or the whole engine without losing evidence or queued work.

## How to read this file

Everything is a task. **Blocks are in build order** — finish one before starting
the next, and each is meant to be built once and never reopened. A `(K5.2)` tag
maps a task to the old phase label so commit history stays findable. `[x]` is
built and tested; `[ ]` is not. Where something is half-built, it is split into
a `[x]` for the part that works and a `[ ]` naming exactly what is missing —
never one ambiguous box.

Everything runs on `nucleus → store → engine → protein → transport`.

**Standing rules, not checkable work:** work on `dev`, no worktrees;
`cargo check`, never a full build; narrowest per-crate tests; never alter a past
migration once it ships; sands read Protein and write Actions only; board chrome
stays host state, never a Ledger fact; keep vendored license/credit files when
touching embed-honest sands.

**Time appears in two blocks and that is deliberate.** Block 1 is time as a
*declaration* — one `Cadence` covering appointments, recurrence, bounds and
weekday landing. Block 11 is time as *execution* — timezones at fire time, the
deadline fabric, missed-wake policy. It is one primitive authored once; the
split exists because a declaration needs no permission and firing needs a grant,
a worker and a run identity, which are blocks 6 and 7.

---

## Decided 2026-07-30 — one schedule, one rule, no legacy

Three names for one idea and two rule engines is the last big incoherence, and
it is the reason a cadence authored in the sand is invisible to a rule. The
local database was deleted deliberately to make this free: **there is nothing to
migrate, so build the right shape rather than a compatible one.**

### One word for repeating time: Cadence

Today there are five spellings. `Cadence` (step + `land_on` + `invalid_day` +
`bound`), `ElapsedSchedule` (a millisecond lattice), `CalendarSchedule` (a
Cadence plus zone and policy), `karma_frequency` (the Program engine's handle),
and the legacy `store::freqs` that `freq()` actually reads.

**Verified 2026-07-31, before deleting anything.** The three-stores claim was
read off tests and never confirmed end to end. It holds, and is sharper than
stated: `freq(@x)` resolves *only* from `injected.freq`, which `tick()` fills
from one table. So the three were genuinely disjoint — not partly joined — and
a rule could never see a cadence the sand wrote. Also settled: `ElapsedSchedule`
anchors on a `TimestampMs`, every test anchor is a plain UTC instant, and
`boundary_strictly_after` is pure millisecond division. **Nothing in the elapsed
case needs instant-ness that a UTC-interpreted civil anchor cannot express**, so
the merge below is real rather than a shape that only looks mergeable.

- [x] **`Cadence` is the only schedule shape the timer wheel speaks.** The
  legacy `FrequencySpec` — `seconds` + `days` + `months` + `day_of_week` +
  `finish_at` — is deleted, and `nucleus::Frequency` is a `Cadence` plus the
  three things a Cadence deliberately does not carry: the `anchor_at` that
  phases it, the durable `fired_through` cursor, and `catch_up`.
  - **`freq(@x)` now reads a Cadence**, so compound steps, weekday landing and
    bounds are reachable from a rule for the first time. `-1 * freq(@daily-7am)`
    still fires — the same test proves both.
  - **The cursor is durable, and its lower edge is inclusive.** `fired_through`
    is where the unfired window *starts*, so a boundary landing exactly on it is
    still owed; firing sets it one millisecond past what it delivered. A process
    restarting mid-window neither re-fires what it delivered nor swallows what
    it owed. Reading the edge as exclusive is the one way to get this wrong —
    it adds a second millisecond and skips a date.
  - **`finish_at` was a column beside the schedule; the bound is now inside it.**
    "Stops after N" and "stops on a date" are one field, so they cannot disagree.
  - **Behaviour deliberately dropped: `day_of_week` filtered occurrences away.**
    A Monday-only daily rule counted zero on a Tuesday. `land_on` *rolls forward*
    instead, which is what a person means by "then move it to a Monday". Nothing
    depended on the filter.
  - **Due-ness left SQL.** `next_at <= ?` became "read every enabled timer and
    ask its Cadence", because a pre-filter would be cadence arithmetic written a
    second time in a second language, free to disagree with the first.
- [ ] **Still two runtimes underneath: `ElapsedSchedule` and `CalendarSchedule`.**
  `CalendarSchedule` is *already* `Cadence` + timezone + policies — the target
  shape with the zone made mandatory. Merging them is therefore the same job as
  the durable-handle merge below, not a warm-up for it: `resolve_calendar_cursor`
  needs a no-zone path that skips gap/fold and does the millisecond lattice, and
  the anchor becomes a civil reading interpreted as UTC when the zone is absent.
  This is kernel code carrying canonical hashes, so it moves golden values.
- [ ] **`Frequency` is the only durable schedule handle** — a `Cadence`, an
  optional timezone with pinned tzdb, and execution policy (timer, missed,
  inactive-gap, rephase, overload). **A schedule with no zone is not a different
  type; it is a Frequency whose zone is absent.** `store::freqs` speaks Cadence
  now, but it is still a different table from `karma_frequency`; one of the two
  has to go.
- [ ] **Delete the words that were never separate concepts.** In prose and in
  code, "frequency", "cadence", "schedule" and "recurrence" mean one thing. Pick
  `Cadence` for the shape and `Frequency` for the handle, and use nothing else.

### One rule object

`recurrence` (the sand's rules: a Cadence plus an amount and a target) and
`RuleDef` (the legacy engine: condition, gate, carry, consequences) are two
halves of the same object that never met.

- [x] **A rule is: when, if, then.** *When* is its Cadence. *If* is a condition
  over Record readings, with a gate and a carry — `condition_src`, `gate` and
  `carry` on the rule, all NULL together for an unconditional one. *Then* is its
  consequences. A rule with no condition is a pure schedule, which is what every
  rule was before this.
  - **The condition is asked at fire time**, against the world as it stands, so
    "every day, but only when stock is low" means what it says.
  - **A blocked gate is neither an error nor a skip.** The rule looked and
    decided not to act, so the date stays unapplied and is asked again next
    beat — the answer can change without the rule changing. A skip, by
    contrast, is a person's decision and is remembered.
  - **The carry is what the consequence receives.** This is the piece the new
    graph had no equivalent of, and it is what makes `-1 * freq(@daily) →
    set_quantity` expressible: fire *because* stock is 8, write -1 anyway.
  - Refused at write time, not at 3am inside a heartbeat: an unreadable
    condition never stores, and a gate with no condition is rejected rather
    than dropped — dropping it turns a rule that fires *sometimes* into one
    that fires *always*.
- [x] **A fact trigger exists, and it is not a second kind of *when*.** A rule
  is evaluated when a Record it reads changes, and its cadence bounds how often
  that may act. So "purely reactive" is a rule whose cadence is fine-grained,
  not a rule with a different trigger — one shape, two ways of being woken.
- [ ] **Delete `recurrence` as a separate table** and express the sand's
  recurring rules as this one object. The apply/skip inbox stays — it is what a
  rule does when nothing is authorized to act for you.
- [x] **Full CRUD: a rule can be deleted.** Pausing was the only way to stop
  one, so a finished rule sat in the list forever wearing a badge; retiring it
  as a third state has the same problem one word along. `DeleteRecurrence`
  removes the rule, its revision log and its skips. **It does not touch the
  Ledger** — dates it already applied are ordinary entries, because the rule
  proposed those changes and never owned them. What a delete removes is the
  rule's future, which is all a rule ever holds.

### Delete the legacy engine, port its capability

- [x] **Arithmetic and the readings a rule actually needs are ported** onto
  exact decimals: `quantity()` (and the bare `@slug` sugar for it), `signal()`,
  `sum()` / `sum_pos()` / `sum_neg()` over a window, the full arithmetic,
  comparisons and short-circuiting booleans. A window is *required* where it is
  meaningless without one, and an unknown reading is refused rather than
  defaulted to zero — a zero would let a typo read as "the stock is empty" and
  fire a rule for the most alarming possible reason.
- [x] **The rest of the vocabulary is ported.** `freq()`, `value()`,
  `promise_state()`, `hours_since_fact()`, `confidence()`, `demand()`,
  `projected()` and `distance()` all answer in `read_for_condition`. Only
  `route_eta()` is still missing, and it is waiting on local map data rather
  than on this work.
  - **`freq(@x)` is the merge, made visible.** It counts the boundaries the
    rule on `@x` produced in the stretch this evaluation speaks for. So a
    schedule is a *term in the arithmetic* rather than a second kind of
    trigger: `-1 * freq(@payday)` is worth zero on six days and -1 on the
    seventh, and the ordinary `!=0` gate turns a rule that is looked at daily
    into one that acts weekly.
  - **The stretch is derived, never stored.** One evaluation reads back to the
    rule's own previous instant (`Cadence::preceding`), so consecutive
    evaluations tile the timeline exactly — nothing counted twice, nothing
    skipped, and a Cell that slept counts every rhythm it missed once each.
    There is no cursor to advance, so no ordering hazard between the reactive
    path, the heartbeat and the projection.
  - **A measurement stays a measurement.** `distance()`, `confidence()`,
    `demand()` and `hours_since_fact()` are approximate at the source and cross
    into exact decimals through one named bridge; nothing the Ledger owns does.
- [x] **Gate and carry are ported, on exact decimals.** `nucleus::karma::
  condition` — gate `!=0` / `always` / `<n` / `<=n` / `>n` / `>=n` / `==n`, carry
  `value` / `one` / `const:N`, and an evaluator over the *same* parsed
  expression tree the legacy engine uses, walked with `DecimalValue` instead of
  `f64`.
  - **The parser now keeps a literal as its source text** rather than an `f64`,
    which is what makes exactness reachable at all: `0.1 + 0.2` is `0.3`, not
    `0.30000000000000004`. A rule that runs daily accumulated that error forever.
  - **One grammar, two evaluators.** The legacy `f64` path and the exact path
    read the same tree, so the port could not drift into a second dialect.
  - **Division is the one inexact operation** and says so: rounded half away
    from zero at a defined scale, once, rather than drifting. Division by zero
    is refused rather than infinite.
- [x] **The consequences are ported.** `emit-promise`, `ask`, `notify`,
  `run-command`, `run-query`, `run-action`, `set-visibility` and
  `advance-transfer` joined the six that already existed, on the one rule
  object. `activate`/`deactivate` are not variants: quantity is the universal
  enable, so they are `set-quantity 1` and `set-quantity 0` and always were.
  - **Everything outward is committed, not run.** Each lands as an obligation,
    a question or a queued effect, and a separate worker carries it out. A rule
    that shelled out mid-evaluation could change the world and then have its
    own transaction rolled back, and would leave nowhere to check a grant —
    by then it has already happened.
  - **`advance-transfer` died on purpose**, as the note here said it must.
    Transfer automation is meant to fail closed and Transfer/Karma is parked;
    porting it would have shipped parked behaviour behind a green test.
  - **Carry reaches every consequence that takes a figure.** `set-quantity` and
    `add-quantity` hold `Option<DecimalValue>`: a written number wins, and
    `None` means "the number the condition worked out". That is what finally
    makes `-1 * freq(@payday) → set-quantity` sayable end to end — the sentence
    this pillar was measured against. A rule whose condition is only a gate
    keeps its literal, so "when stock is low, set it to 10" still sets 10
    rather than being handed the stock level.
  - **The marker entry no longer takes the carry unless the rule captures.**
    Applying a date always writes one entry to mark it done; letting a carry
    into that entry for an `add-quantity` rule moved the figure twice, once
    through the marker and once through the consequence.
- [x] **The consequences that never existed: concepts.** `nucleus::karma::
  Consequences` is an ordered non-empty list of `capture-entry`, `set-quantity`,
  `add-quantity`, `set-concept`, `add-concept`, `remove-concept`, each
  dispatching to the typed Action that already existed. A rule stores it as
  `consequences_json`, replacing the single `amount` + `concept_uid` pair that
  was the first caller's shape leaking into the model. Removing a Record's
  identity concept stays refused. A rule can now move a card between kanban
  columns, since columns bucket by value, range or concept.
  - **A list, not one**, because `@wip → @done` is one intention and must be one
    rule, or a reader has to know two rules are secretly joined.
  - **`SetQuantity` is not a delta.** Assigning `-1` is not a movement of `-1`,
    so it is excluded from `declared_delta()` and never folded into a timeline;
    a projection can only sum what is summable.
  - **The idempotency guard moved to the front of apply.** The entry carrying
    the occurrence's request id was enough while a rule could only capture — the
    UNIQUE index refused the second one. It is not enough once a rule can add a
    quantity or toggle a concept, because those run *before* the capture is
    refused. Caught by a test that doubled a `+5` to `+10`.
  - **Every apply writes exactly one entry, even at zero.** That entry is the
    only record the date ran. Without it a concept-only rule leaves no trace,
    reads as due forever, and re-applies on every press.
  - **The marker entry carries an amount only for `capture-entry`** — using the
    declared delta would move an `add-quantity` rule's amount twice, once
    through the marker and once through its own consequence.
- [x] **Author the non-capture consequences in the sand.** The rule form has a
  *Then* block in two independent halves — a number and a concept — because the
  useful rules are pairs, and splitting "add 1 and mark it @done" across two
  rules hides that they are joined.
  - The number half offers capture / add / **set** / nothing. *Nothing* is what
    makes a pure reclassification sayable; without it, moving a card would have
    to invent an amount.
  - The concept half offers add / remove / **move** / replace-all. *Move* emits
    the remove-then-add pair in one rule, which is what makes a column change a
    move rather than a moment in both columns at once.
  - **Rows without a figure lead with what the rule does.** A `0` there would
    be a claim that the rule moves nothing, rather than the absence of a number.
    The same applies to a due date: no amount means no override box, since the
    apply path would have nowhere to put the number.
  - A delete control per rule, saying on the button what a delete keeps.
- [x] **A declined date is visible and reversible.** `UnskipRecurrenceOccurrence`
  was a third Action the sand never called, and skipped dates were filtered out
  of the list entirely — so a mis-click could not be undone, and the record of
  the decision was invisible, which undoes the reason skipping exists at all
  ("decided against" and "nobody has looked yet" must not read the same).
  Skipped dates now list, muted, offering *Undo skip* alone: leaving *Apply*
  beside one would let a skip be overridden without ever being withdrawn.
  - **Skip became the primary control** the moment the heartbeat started
    applying due dates on its own. Declining ahead of the beat is how a person
    says "not this one"; *Apply* now means "run it early".
- [x] **The sand can revise a rule, so CRUD is finally all four letters.**
  `ReviseRecurrence` existed as an Action that nothing called: create, read and
  delete were reachable, and the only way to fix a wrong amount was to delete
  the rule — losing its identity and its explanation of the entries it had
  already produced.
  - **The same form does both jobs**, prefilled and relabelled. A separate edit
    surface is how a rule ends up with a shape only one of the two can express.
  - **The revision is quoted back** (`expected_revision`), so a form left open
    while the rule changed underneath is refused rather than winning by being
    slower.
  - **The Record select is disabled while editing.** A revision carries no
    target, so a rule cannot move to another Record; leaving the control live
    would be an offer the Action cannot honour.
  - **A stored consequence names its concept by uid**, because the Action
    resolved it on save. The form speaks it back as the name its author typed —
    prefilling the uid would show an identifier where someone wrote "rent".
- [x] **The JS that builds and re-reads a consequence list is tested for real.**
  `crates/web/tests/karma_sand_js.mjs` runs in Node against the actual module —
  authoring each consequence shape, refusing the empty and half-named ones, and
  the round trip that breaks silently: open an untouched rule, save, get the
  same rule. The sharp case is the column move, which must read back as one
  *move* rather than two unrelated concept consequences, and the concept uid a
  stored rule carries must be spoken back as the name its author typed.
  - A Rust test (`karma_sand_js.rs`) runs it so it rides the normal suite, and
    **skips loudly** when no `node` is on PATH — a missing engine is a property
    of the machine, not of the code, but a green run must never be mistaken for
    a checked one.
  - Verified in both directions: the harness was made to fail on purpose and
    the Rust wrapper reported it. A test that cannot fail proves nothing.
- [x] **The delivery behavior is ported, and debounce turned out to be the
  cadence.** A change to a Record makes every rule reading it *look*; the
  rule's own cadence still decides whether it may *act*. Both paths therefore
  apply the same occurrence — the latest date the rule produced — through the
  same Action with the same idempotency key.
  - That single decision buys three things at once. A reactive firing and a
    scheduled one cannot double-apply each other. A rule acts at most once per
    period, which *is* the debounce, now declared in the same place as
    everything else about the rule and durable because it is derived from the
    Ledger rather than held in memory.
  - **A blocked gate does not spend the date**, so a rule fires the instant the
    world makes its condition true rather than waiting for the next beat.
  - **A rule's own firing must not re-enter the reactive path.** The marker
    entry is not committed yet, so a reaction there would find the date unspent
    and apply it twice; chains are followed afterwards, from outside, where
    idempotency holds. Bounded at 256 evaluations per change.
  - **The guard belongs on the apply, not on any one caller.** It was first put
    on the heartbeat, which left a date applied *by hand* from the inbox moving
    the Record twice — the path a person actually watches was the broken one.
- [ ] **Reaction re-reads every rule on every committed Fact.** `react_to` loads
  all rules and resolves each condition's tokens per change, on the write path.
  Fine at ten rules; a sync batch against fifty will feel it. An index rebuilt
  on rule CRUD is the fix, and it is a cache — so it is worth doing only once
  the shape has stopped moving.
- [x] **`nucleus::rule`, `store::rules`, `store::freqs`, `engine::karma`,
  `nucleus::frequency` and the `rule` / `rule_consequence` / `frequency` tables
  are deleted.** No import path, no adapter, no compatibility shim. ~2,900 lines
  gone, and `parse_duration` — the one thing in there that was neither a rule
  nor a schedule — moved next to the lexer that produces the token.

### Non-negotiable: a rule fires itself

**This is the point of the whole pillar.** A rule that waits for a click every
week is a to-do list with extra steps — the person is still the scheduler, which
is the job they asked the software to take. The canonical case, and the one to
build against:

> A habit Record sits at `-1` (a Need). I tick it and it goes to `0` (done).
> **Every week it returns to `-1` by itself.** I never touch the schedule again.

Manual apply was framed here as "what makes this shippable". That framing was
wrong and is retired: manual apply is the **fallback for what a person must
approve**, not the destination. Anything a person already authorized by
declaring the rule must happen without them.

- [x] **The heartbeat is started.** `Engine::run(period_secs)` existed — a
  daemon that ticks and drives the wheel — with **no caller anywhere in the
  codebase**: the web server built the Engine and never started the loop, so the
  Cell had a pulse it never took. Started after the organ signer, so the first
  beat can attest what it commits. Period is 60s.
- [x] **Due occurrences apply themselves.** `fire_due_rules` drains every date
  that fell due and applies it through the **same Action** a person presses, so
  an automatic change is auditable by identical means to a manual one and never
  gets a private write path. Runs after the timer wheel on each beat, so a rule
  reading a Record a Frequency just moved sees the new value this beat.
  - **No actor is recorded, deliberately.** Nobody pressed it. Naming a person
    would be a false attribution in the Ledger; the rule's declaration is the
    authority, and that is a different claim.
  - **A paused rule fires nothing**, including dates that already fell due.
    Pausing means "stop acting for me", and a pause that only hid the future
    while the wheel kept firing is the most surprising reading of the word.
  - **A refused consequence does not stop the wheel** for every other rule — a
    rule whose concept or Record vanished stays due and says so on the surface.
  - **The idempotency guard already makes this safe.** Apply is keyed on
    `<recurrence_uid>:<due_at>` and refuses a repeat before running any
    consequence. So the wheel and a person can both press the same date and the
    second one does nothing. This was built for retries; it is what lets
    automatic and manual firing coexist without a lease.
- [x] **Declaring the rule is the authorization**, for consequences that touch
  only the author's own Records — quantity and concept changes. Requiring a
  second consent every week means the declaration meant nothing. **This does not
  extend to effects that leave the Cell**: notify, run_command, transfer and
  anything reaching another Organ still need a grant, and still fail closed.
  That boundary is what makes automatic firing shippable ahead of the full
  authority machinery, rather than blocked behind it.
- [x] **A Cell asleep for three weeks wakes owing three Mondays, and owes them.**
  Apply every missed date rather than collapsing to the latest. Three missed
  rents *are* three rents, and a habit re-armed three times ends at the same
  `-1` it would have reached once — so applying each is right in the
  accumulating case and harmless in the state-assertion case, while collapsing
  is wrong in the first. Each date also needs its own marker entry or it stays
  due forever and re-fires on every tick, which is the failure collapsing
  quietly creates.
  - **Bound the burst instead.** The danger is not a missed month, it is a
    millisecond rule asleep for a day — tens of millions of occurrences. Cap
    what one tick applies per rule and carry the rest to the next tick, so a
    fast rule cannot starve the heartbeat or the Ledger.
- [x] **Say plainly that delivery resolution is the tick period.** A cadence can
  be declared in milliseconds; a polling heartbeat cannot deliver one on time.
  Weekly habits do not care and work today; sub-second delivery is the
  **tickless deadline fabric** in block 11, and until it exists the honest
  statement is "declared in ms, delivered on the tick". Do not let a 3ms
  *declaration* imply a 3ms *delivery*.
- [ ] **Then: one rule object** (below), so a rule authored in the sand is
  visible to the wheel rather than living in a table only the inbox reads. The
  steps above make sand rules fire; this makes them first-class to everything
  else — `freq()`, conditions, and rules that reference each other.

---

## 0. Exact numbers and frozen vocabulary

Why first: every later block stores, hashes, or signs a number. If the number
representation changes afterwards, every hash and every test fixture is invalid.

- [x] **One decimal type, no floats on any durable path (K0).**
  `FixedDecimal<S>` = `i128` mantissa + compile-time scale, max scale 18.
  Checked arithmetic, no implicit rounding, negative zero impossible.
  Serializes as a decimal string with exactly `S` fractional digits.
- [x] **Probability and confidence are different types (K0).** Both
  parts-per-billion over `0..=1_000_000_000`, nine fractional digits on the
  wire. Neither converts to `bool`. `0.72p` / `0.65c` are DSL sugar normalized
  before hashing. They answer different questions — how likely vs how
  well-supported — and a gate must compare both deliberately.
- [x] **Time atoms (K0).** `DurationMs` = signed integer milliseconds.
  `TimestampMs` = signed UTC millisecond, serialized only as RFC3339 with
  exactly three fractional digits and `Z`. Civil time is a separate type and
  never silently becomes an elapsed duration.
- [x] **No money type.** A currency is a Unit Record, so `12.50 @brl` and
  `2.5kg` are the same shape. Converting between units is a rule, not a kernel
  feature. Deleting `Money` is what made `Quantity ÷ Quantity` expressible.
- [x] **Canonical bytes (K0).** JSON sorts object keys, preserves array order,
  no whitespace, rejects float numbers. Hash = SHA-256 over a domain-separated
  prefix (`karma.program-revision.v1`) so two object families can't alias.
  Enums/Actions kebab-case, fields snake_case, maps are `BTreeMap`.
- [x] **Failures are typed (K0).** Stable machine `FailureCode` + safe message +
  field path. Callers never match on text. Codes may be added, never repurposed.
- [x] **`nucleus::karma` has no I/O.** No SQL, async runtime, wall clock, random
  uid, platform locale, or unordered public collection.
- [ ] **Every new public wire type extends the golden fixture in the same
  change.** An untested wire type is an incomplete change.

### The Ledger carries exact deltas (E0.0)

Why: the kernel is exact but `fact.delta` and `record.quantity` were `REAL`, so
an exact `1.15kg` became a float in the only place it was durable.

- [x] **`fact.delta` and `record.quantity` are exact pairs, `REAL` removed** —
  rebuilt in migration `0001`, not stacked beside the old column. Two
  representations of one quantity is the ambiguity being deleted. A stale local
  `.db` must be deleted, not migrated; this licence ends at first deployment.
- [x] **The kernel's decimal *is* the Ledger's decimal.** `nucleus::DecimalValue`
  is re-exported at the crate root and used by `Fact`/`NewFact` directly, so
  there is no conversion between an evaluation type and a storage type — which
  is what makes "survives the round trip" true by construction rather than by
  care. `parse_inferred` and `aligned_add` are its parsing and addition entry
  points; `MAX_DECIMAL_SCALE` is 18.
- [x] **Stored as `(mantissa TEXT, scale INTEGER)`.** TEXT because the mantissa
  is `i128` and SQLite INTEGER is 64-bit, which truncates above ±9.22 at scale
  18. Canonical mantissa has no leading zeros and no negative zero, so
  `mantissa != '0'` is an exact is-nonzero test and `LIKE '-%'` an exact
  is-negative test.
- [x] **Sum in Rust as `i128` at a common scale, never in SQL** — SQLite numeric
  affinity does not preserve exact decimals. Alignment takes `max(scale)`, not
  the sum, so scale never needs to exceed 18.
- [x] **The exact pair is inside the hash preimage** (`scale:canonical-text`),
  so an amount is covered by the Fact's signature. Consequence: a declared `1.5`
  and a declared `1.50` are different Facts — precision is signed, not
  annotated. This also fixed a chain-determinism bug where the preimage
  interpolated an `f64` and `0.1 + 0.2` hashed as `0.30000000000000004`.
- [x] **Checkpoint payloads carry canonical decimal text plus scale.**
  `{"level": q}` was a JSON float, and after compaction that payload *is* the
  record's level.
- [x] **One named lossy inbound door**, `NewFact::quantity_f64` /
  `exact::from_f64`, marking every producer still computing in floats. Greppable
  on purpose. `to_f64()` stays free for display.
- [x] **Declared precision survives sync.** `Package.facts` is `Vec<Fact>`, so a
  delta crosses between Cells exact. A `1.50` round-tripped through `f64` would
  return as scale 1, hash `1:1.5` instead of `2:1.50`, fail `verify_chain_step`
  and be quarantined. The existing sync tests all used scale-0 values and could
  not catch that, so `declared_precision_survives_the_sync_wire` syncs a
  trailing-zero decimal and asserts an empty quarantine.
- [x] **Proven against the live schema, not the migration text.**
  `crates/store/tests/exact_ledger.rs` holds the six proofs including a
  `pragma_table_info` assertion for "no `REAL` survives".
- [x] **Two SQL triggers referenced `fact.delta` and moved with it**
  (`0020_transfer_settlement_corrections.sql`): the zero-evidence check became
  `delta_mantissa = '0'`, and the compensation-matching check compares the exact
  pair against the transfer side's `REAL` by building `10^scale` as text, so it
  needs no SQLite math extension.
- [x] **`bump_quantity` is safe because `facts::insert` runs first in the same
  transaction**, so the write lock is held before the SELECT. Under a DEFERRED
  begin, "inside a transaction" alone would not be enough.
- [ ] **Not done, deliberately:** `promise.delta`, `link.quantity`,
  `transfer_occurrence.quantity` stay `REAL`. Protein still emits quantities as
  JSON numbers so sands keep working.

### History stays queryable after compaction (E0.0)

Why: compaction deletes pre-checkpoint Facts and anchors them in a cold file, so
history survives but stops being readable. A Fact already *is* the history —
don't version the Record or copy it per concept.

- [ ] **Archive instead of delete.** Pre-checkpoint Facts move to `fact_archive`
  (identical columns, same database file). Level and sum queries never read it
  because the checkpoint already accounts for those deltas — unfolded by
  construction, not by a flag. History queries union it when the window reaches
  past the checkpoint.
- [ ] **The checkpoint is the past/present boundary.** No `is_history` column,
  no mode to remember, no way for two markers to disagree.
- [ ] **Archive the classification and the unit in force with the Facts.** A
  `-10` whose concept was left behind is an unreadable number. Keep
  `prev_hash`/`hash`/`signature` so archived history stays verifiable.
- [ ] **Checkpoint on a cadence, not once** — one checkpoint collapses everything
  before it to a single level, making past levels unanswerable at finer grain.
- [ ] **Retention keyed on concept with DAG inheritance**, not `record.kind`,
  plus an explicit never-archive setting. A policy on `@food` governs
  `@ice-cream` unless overridden. Today's kind-keyed table cannot express "keep
  my grocery history for two years".
- [ ] **Compaction refuses rather than truncating a window a live rule reads.**
  Effective horizon = configured horizon or the longest lookback any active
  program uses against that Record, whichever is longer. Otherwise archiving
  90-day-old Facts silently changes what `sum(@x, 90 days)` returns.

---

## 1. Time, declared

Why: one primitive for every repeating or dated thing in Lince — an appointment,
a reminder, rent, a backup, a promise window. There was a second schedule type;
it existed only because a read path couldn't reach the scheduler's timezone
registry, which is a crate-graph reason, not a real one. Firing on time is
block 11, after authority, because firing needs permission and a declaration
does not.

- [x] **`Cadence` is the only schedule.** Shape:
  - `CadenceStep` summing years, months, weeks, days, hours, minutes, seconds,
    milliseconds — a *sum*, so `1 month + 1 day + 10ms` is one rule.
  - optional `land_on` weekday set, applied after the step, never folded back
    into its phase.
  - `invalid_day` = `clamp | skip | pause`, for a month too short for the
    anchor's day.
  - `bound` = `unbounded | count | until`.
- [x] **A one-off is `count: 1`, not a second type.** "This happens on the 14th"
  produces its anchor and retires. A promise, a dated reminder, a one-off
  transfer and a standing order are the same object with different bounds, and
  nothing downstream needs to know which it holds.
- [x] **An empty step is legal only with `count: 1`.** Otherwise the rule could
  reach a second date with no way to get there. A bound of zero is refused.
- [x] **A count is of occurrences produced, not indices tried.** Under `skip`, a
  February that yields nothing must not spend one of twelve payments. See
  `Cadence::ordinal_of`.
- [x] **`until` is exclusive**, so rules tile end to end without a date landing
  in both. The bound is measured on the landed instant, not the one before
  landing.
- [x] **No second place to say when a rule stops.** The `recurrence` table has no
  `ends_at` column; the bound inside `cadence_json` is the only answer.
- [x] **Derivation returns `Derived { dates, truncated }`** — a millisecond-step
  rule always yields a prefix, and saying so is the contract's job, not the
  surface's.
- [x] **Deliberately lost:** several weekdays inside a multi-week cycle is no
  longer one rule, because landing rolls forward and yields one instant per
  step. `every 1 day landing on [mon,wed,fri]` covers the common case;
  "Monday and Wednesday every fortnight" is two rules with two anchors.
- [x] **Consequence is not part of the schedule.** A schedule saying rent is due
  does nothing; a rule that pays rent spends authority. Declare / propose /
  apply is a property of whatever *binds* a schedule to an action.
- [ ] **Publish-time refusal for one-shot misuse** and the remaining
  `suggest`/`draft`/`apply` route split (E1) — see block 6.
---

## 2. Reading the world

Why: until this lands a rule can only compute over its own parameters. This is
what lets a rule read a Record's quantity, its unit, and its other numbers.

- [x] **Resolve `InputSource::RecordQuantity` and `SavedProtein` inputs (E0.1),
  in `store::karma::runs::evaluate_member`.** The AST, DSL and Proof already had
  them; the runtime never filled them — `evaluate_member` populated boundary
  values for `Trigger` nodes only, a single bool per trigger saying whether the
  occurrence matched its Frequency — so a program using one failed with
  `MissingInput`. `Signal`, `SecretMetadata` and `CapturedFact` stay unresolved
  until block 13 and refuse by name.
- [x] **A deleted Record blocks the run, it does not read as zero.** "The Record
  is gone" and "the Record holds nothing" are different facts. Typed refusals:
  `RecordQuantityUnavailable`, `RecordUnitUntypable`, `InputSourceUnresolved`.
- [x] **`SavedProtein` resolves through a seam, not a dependency cycle.**
  `protein` is built on `store`, so `store` declares the hole
  (`karma::runs::ExternalInputResolver::saved_protein`) and `engine` fills it
  with `karma_runtime::SavedProteinInputs`. With no resolver supplied it blocks
  by name.
- [x] **A saved view must reduce to exactly one number** — one row holding a
  bare number or an object with a single numeric field. Several numeric columns
  is ambiguous, and guessing which one the author meant is how a rule computes
  against the wrong column.
- [x] **A view that fails to execute is a refusal, not an error.** It blocks its
  own program by name so one broken saved query cannot stop the processing turn.
- [x] **Read the quantity from the exact Fact chain, not the `record.quantity`
  cache.** `store::facts::level` folds anchored on the last checkpoint carrying
  a level — retention really deletes archived Facts, so folding only surviving
  rows under-reports a compacted Record. Compaction's archive anchors are
  checkpoints too but carry `{archive, ...}` rather than a level, so they are
  skipped rather than read as zero.
- [x] **Boundary values are frozen into the replay capsule automatically** — the
  resolved value goes into `boundary_values`, which is what
  `capture_evaluation_replay` captures. A later Fact cannot change what a past
  run saw.
- [x] **A Record's quantity arrives typed by its own unit.** `unit_uid` set →
  `Quantity { amount, unit }`; absent → plain `Decimal`; an untypable unit
  blocks.
- [ ] **Refuse a unit mismatch at publish time, not at first run (E0.1, open).**
  Where: the Program create/revise path in `store::karma::programs`, which
  already validates a revision and can reach Records. What: for every
  `record-quantity` input, compare the named Record's `unit_uid` against the
  unit the port contract declares and refuse the publish when they disagree.
  Why not in Proof: `evaluate_program` and `proof.rs` are pure — no clock, no
  SQL — and a Record's unit is world state. This turns "a rule that treats
  litres as kilograms fails on its first run" into "it never publishes".
- [ ] **Read numeric values inside a namespaced `record_extension` (E0.1),**
  addressed by namespace and field path, declaring value type and unit. Missing
  field, wrong JSON type, and unparseable decimal are typed refusals, never a
  silent zero. This is what lets one rule read a Record's weight, price and
  count together.
- [x] **Unit conversion is exact and explicit.** Full spec in
  `docs/Central: Lingua.md`. What this block needs: a conversion is exact, and
  asking for one is visible in the program — never an implicit coercion applied
  to make an expression type-check.
---

## 3. Arithmetic a real rule can express

Why: before this, exact values could only be added and subtracted — no
percentage, rate, unit price or split.

- [x] **Multiply and divide for `Decimal` and `Quantity` (E0.2).**
  `evaluate_integer_product` handled `I64` only; everything else fell through to
  an invariant error.
- [x] **Multiplication states its result scale, division states its rounding
  rule.** Neither is closed over fixed-point decimals — no scale represents
  `1/3`. The rule is named in the AST and frozen in the revision hash, so
  changing it is a visible revision. Vocabulary: half-up, half-even, toward
  zero, away from zero. The API is `Rounding` plus
  `DecimalValue::{mul_ratio, div_exact}` returning
  `RoundedDecimal { value, exact }` — every inexact operation reduces to one
  rational multiply, and the `exact` flag is how a discarded remainder gets
  *reported* rather than lost.
- [x] **One optional field, `ExpressionAst::Binary.precision`, and Proof
  enforces it as an `iff`** — present exactly for multiply/divide where either
  side is exact, absent everywhere else. The forbidding half matters:
  `precision` is inside the revision hash, so a stray one on `And` behaves
  identically but hashes differently, giving one program two identities and
  orphaning state keyed on the old hash. `ProofIssueCode::InvalidPrecision`
  covers all four ways to get it wrong — missing, stray, scale past 18, declared
  unit where the value already has one.
- [x] **Scale overflow is a publish-time refusal; a zero divisor is a runtime
  failure.** The declared scale is static so Proof rejects `scale > 18` before
  the program can run; a divisor depends on the values a run sees. Runtime keeps
  `DivisionByZero` and `ArithmeticOverflow`.
- [x] **Unit algebra is explicit, never inferred.** `Quantity × Decimal` keeps
  the unit (the percentage and rate cases). Multiplying two united quantities,
  or dividing one by another, produces a value whose unit the author must
  declare; Proof refuses otherwise rather than inventing `kg²` or dropping a
  dimension. `DeclaredUnit::{Dimensionless, Unit}` — "this ratio has no unit" is
  a statement the author makes, because `total ÷ budget` is a real, common rule.
- [x] **There is one dimensioned type, not two.** `Money` sat beside `Quantity`
  with an identical shape and a narrower algebra. Deleting it removed a second
  spelling of one idea and made unit conversion expressible: `Quantity ÷
  Quantity` with a declared result unit is exactly the rate `USD ÷ EUR` used to
  be refused for wanting.
- [x] **`mul_exact` is a sibling of `div_exact`, not a call into `mul_ratio`.**
  Routing a product through `mul_ratio` sets the denominator to `10^other.scale`
  then re-inflates the numerator by the same power, so multiplying two scale-9
  values at scale 18 multiplies by `10^18` and divides it straight back out,
  overflowing `i128` on perfectly representable values. `mul_exact` cancels
  those powers of ten into the operands first, and only by the factors of ten
  they actually contain — dropping a non-zero digit would discard a remainder
  that still decides the rounding.
- [x] **Known limit, conservative on purpose:** two operands with no trailing
  zeros whose raw mantissa product exceeds `i128` return `None` even when the
  result at the declared scale would fit. A typed failure, never a wrong answer.
  Lifting it needs a 256-bit intermediate.
- [x] **Exactly one rounding implementation** — `round_ratio(numer, denom,
  scale, rounding)`; `mul_ratio`, `mul_exact` and `div_exact` all reduce to it.
  Two copies would drift precisely at the tie cases nobody tests.
- [x] **`exact_literal` mirrors `infer_exact_product` arm for arm, including
  order**, and they are commented as a pair. `ensure_type` checks scale on every
  node output, so a disagreement between Proof's inferred type and the runtime
  value surfaces as a `RuntimeTypeMismatch` mid-evaluation — a publish-time
  error arriving at the wrong moment and nearly unreadable.
- [x] **Rounding is reported per run, not per site.**
  `EvaluationResult.rounding: Vec<RoundingNote>` carries node, operator, scale,
  rule and result. Empty means the run was exact throughout.
- [ ] **Persist rounding notes so a Why lens can show them.** The store's run
  record has no column yet, so they satisfy the kernel exit criterion but reach
  no reader.
---

## 4. Rules as a typed graph

Why: a rule is not a string of conditions. It is a versioned graph whose pure
computation, candidate, policy and effect stages are separate objects, so
"recognising a situation", "deciding it matters", "being allowed to act" and
"acting" can never collapse into one another.

Four claims this block keeps apart, because conflating them is the original sin
the old condition → consequence pair committed:

- **A likelihood is not permission.** "Ana will probably buy apples", "the
  estimate is well supported", "buying is beneficial" and "Lince may send a
  Transfer" are four different claims, stored and evaluated separately.
- **A threshold is a routing policy, not a truth.** Crossing it may surface,
  draft, ask or act only under an explicit policy with hysteresis, budgets and
  authority. It never converts correlation into a fact.
- **Automatic promotion is allowed but never magical.** A learned pattern may
  become an active rule under a narrow grant, from a reviewable template, past
  Proof, in shadow first if required, still editable, inside the grant.
- **Rules are never set in stone.** Anyone authorized can edit, pause,
  supersede, fork or retire any program. Published revisions and past runs are
  immutable evidence; immutability protects history, not current behavior.

The complete loop: `Observation → Fact/evidence → context/features →
recognition/model/rule → forecast/optimizer → candidate → policy/authority →
decision or intent → typed Action/effect → receipt/Fact`.

### The pure kernel

- [x] **`nucleus::karma` is I/O-free (K1).** A pure node receives a frozen
  context and returns a trace plus candidates. No SQLite, Tokio time, processes,
  network, devices, secrets or global randomness.
- [x] **`ProgramAst` is the canonical version-tagged revision payload** —
  purpose, parameters, stable node/output maps, declared outputs. Owner, live
  quantity, active revision, grants and runtime state belong to the mutable
  handle or run epoch and must not contaminate the immutable semantic hash.
- [x] **Nodes keyed by `LocalId` in `BTreeMap`s**, with explicit input bindings,
  output `PortContract`s, and a **closed** `NodeOperation` enum. Expressions
  reference only names in that node's binding map — no ambient Record, Protein,
  clock, randomness, secret or network read. Later families extend the same
  enum; there is deliberately no generic JSON "operation config" escape hatch.
- [x] **`PortContract` carries value type, sensitivity/taint class, optional
  freshness.** Source and destination types must match exactly. A pure
  derivation cannot declare an output less sensitive than any consumed input.
  Declassification is its own capability-checked node, never a flag on a wire.
- [x] **`Proof` is a deterministic value** — revision hash, accepted/rejected,
  stable topological order, sorted typed issues with JSON pointer paths and node
  ids. Missing nodes/ports, undeclared expression inputs, type mismatches,
  invalid references and combinational cycles reject the revision.
- [x] **A cycle is legal only through an explicit delay/state boundary** that
  makes the graph acyclic when incoming update edges are cut. The delay contract
  records initialization, reset, late-event, migration, persistence and
  simulation-clone behavior.
- [x] **Text parsing came after the graph, deliberately.** Proving the JSON AST
  first stops a convenient parser becoming the accidental semantic model. The
  DSL round-trips through these same types.
- [x] **`FrozenEvaluationContext` + `EvaluationLimits`.** Exact boundary values
  by input/trigger node, typed parameter overrides, epoch-start delay state, and
  deterministic fuel — one node visit and each visited expression consume
  defined work units, so host speed and thread scheduling cannot change whether
  a run exhausts its budget.
- [x] **Delay evaluation is two-phase.** In graph order every delay outputs its
  epoch-start value; after all combinational nodes finish, the evaluator
  resolves each delay's update binding and stages it for the next epoch without
  mutating the current context. Feedback graphs get read-old/write-next
  semantics independent of map or node order.
- [x] **Typed deterministic failures with node/path:** rejected Proof, missing
  frozen input, runtime type mismatch, exact overflow, divide-by-zero, invalid
  state, fuel exhaustion.
- [x] **Landed:** `value`/`ast`/`proof` (graph, closed op set, canonical revision
  hash, taint/reference checks, topological Proof); `evaluate` (read-old/write-
  next delay state, checked exact arithmetic, lazy typed branches, stable trace
  order, fuel/depth limits); `calendar`; `dsl` (K1.4); `frequency` (K1.5); the
  K1.6 gate families; K1.7 replay capsules; K1.8 exact primitives.
- [x] **The legacy `Expr`/`RuleDef` types are never the new kernel** — they use
  `f64`, numeric Boolean truthiness, second durations, implicit reads and
  cycle-tolerant ordering. **Superseded 2026-07-30:** they are not an import
  source either. Their *capability* is ported by hand onto exact values (see the
  decision at the top) and then they are deleted.

### Node families and ports

| Family | Role |
| --- | --- |
| Trigger | Fact/event, schedule, signal sample, threshold crossing, manual call, sync arrival, decision, workflow wake, effect receipt |
| Input | Saved/inline Protein, direct record/fact reference, parameter, secret reference metadata, captured Signal |
| Normalize/feature | Unit conversion, validation, window, aggregate, join, lag, rate, calendar/place feature, missing-data policy, quality weighting |
| Derive/recognize | Typed arithmetic/logic, stateful threshold with hysteresis, finite state recognizer, pattern model inference, reusable Sense |
| Project/analyze | Imagination branch, forecast, invariant, query, aggregate, optimizer, ranker, sensitivity/infeasibility |
| Control | Gate, branch, merge, bounded iteration, delay, debounce, cooldown, rate limit, transaction boundary, assertion |
| Workflow | Sequence/parallel, wait-until, approval, child program, retry, compensation, cancellation, correlation |
| Candidate/attention | Recommendation, decision, program revision, plan, report, whisper request, digest item |
| Intent/effect | Typed Action, Transfer Action, connector call, device/controller intent, notification, command, HTTP request |

- [x] **Ports carry schema, unit/dimension, cardinality, visibility/taint,
  freshness and uncertainty.** A connection that cannot prove compatibility is
  invalid; nothing coerces strings at runtime. Missing, stale, denied, invalid
  and unknown are typed states distinct from numeric zero and Boolean false.
- [x] **Gate semantics are exact (K1.6).** `threshold` has one ordered scalar
  input and Boolean `active`/`entered`/`left` outputs, storing only the old
  `active` bit. `above` enters at `value >= enter`, leaves at `value <= exit`,
  Proof requiring `exit < enter`; `below` is the mirror with `enter < exit`.
  Values in the open band preserve state. `entered`/`left` are one-occurrence
  pulses, so an oscillating measurement inside the band cannot repeatedly fire.
  Threshold literals must have exactly the input type and it must be an ordered
  scalar — unit, scale and referenced kind are never coerced.
- [x] **`debounce`** has one Boolean input and `stable`/`entered`/`left` outputs.
  A changed input starts a candidate interval at the frozen logical timestamp
  and becomes stable only after the same value has remained pending for
  `for-at-least`. An exact-boundary timestamp qualifies; a return to the stable
  value cancels the pending interval; duration zero promotes immediately.
- [x] **`cooldown`** accepts a Boolean pulse only if none was accepted before or
  `now - last_allowed_at >= cooldown`. **False inputs never consume the window.**
- [x] **`rate-limit`** accepts at most `max` true pulses in the half-open rolling
  interval `(now - window, now]` — an acceptance exactly one window old has
  expired. The retained timestamp list is bounded by `max`. Proof requires
  non-negative debounce/cooldown durations, a strictly positive rate window, and
  non-zero `max`.
- [x] **These stateful nodes do NOT break a graph cycle.** Their current input
  determines their current output; the state update is written only after the
  occurrence. **Only an explicit `delay` is a read-old/write-next cycle
  boundary.** Without this rule an apparently stateful threshold or debounce
  could conceal an instantaneous dependency cycle.
- [x] **All three temporal controls require
  `FrozenEvaluationContext.logical_at`** and keep state in `control_state`,
  separate from `delay_state`. Every state variant stores `last_observed_at` so
  logical time cannot silently move backwards. Under `late_event: ignore` the
  occurrence emits no new pulse and leaves state unchanged (`debounce.stable`
  still reports the old value); under `reject` evaluation returns
  `non-monotonic-logical-time`; `recompute` and `compensate` return
  `replay-required`, because a single pure invocation cannot reconstruct the
  intervening history and the occurrence runner must replay the captured ordered
  inputs before committing replacement or compensating state. Absent logical
  time returns `missing-logical-time`. The complete typed state-before and
  staged-state-after values belong in the trace and replay capsule.
- [x] **`route-candidate`** takes a Boolean condition, one declared output, an
  explicit route (`observe|recommend|draft|ask|act`), a template slug, and an
  ordered map from candidate field names to input bindings. Proof derives the
  exact output type `datum<candidate(route, template, fields)>` and rejects
  missing, extra or differently typed fields. False produces a typed *missing*
  datum; true produces a typed value datum carrying the immutable payload.
  **Every route is inert here — even `act` means "this candidate asks the later
  policy/authority/effect pipeline to attempt acting", never "perform an effect
  now".** Field order is semantic and canonical because both maps are
  `BTreeMap`s.
- [x] **The Flow Plane projects these directly:** threshold enter/exit as two
  points joined by a hysteresis band, temporal controls as annotated ranges,
  state transitions as edge pulses, candidate routes as terminal inert nodes.
  **One AST drives execution, Why traces, simulation and the sand — the UI does
  not invent a second rule model.**
- [ ] **Resolve `@slug` sugar to uid plus displayed slug in the immutable
  revision.** Rename never changes meaning; an unresolved reference blocks
  activation.
- [ ] **Every stateful node declares** initialization, update event,
  persistence, reset/migration, late-event behavior, and whether simulation
  branches clone its state.
- [ ] **Reusable subprograms with explicit typed parameters and outputs.**
  Invocation freezes a revision; a template update never silently edits
  installed programs.
- [ ] **Keep shell/HTTP/model/device work out of expressions.** A pure
  content-addressed extension may calculate; an effect node may touch the world;
  the graph makes the boundary visible.
- [ ] **Specify every gate's transition behavior**: edge/level trigger,
  enter/exit thresholds, hysteresis, hold duration, cooldown, once-per-window,
  reset, unknown/stale input policy. "True on every refresh" must never
  accidentally mean "repeat an effect forever".
- [ ] **Separate a durable derived Fact from a virtual derived value.** Virtual
  values are recomputed through the run; materialization is an explicit node
  with provenance, retention, unit and correction semantics.
- [ ] **Make conditions and derived values reusable graph nodes** composed
  through typed references and explicit gates, so arbitrary chains never return
  to opaque `rq1`/`kd2` token strings.
- [ ] **Support parameter records separately from graph revisions.** Tuning a
  threshold inside its declared range appends evidence without rewriting
  topology; changing types, inputs, effects, authority requirements or the
  allowed range requires a new revision.

### The authoring language

The DSL is declarative and formatter-stable. Blocks describe a graph; they do
not execute top to bottom. Data dependencies define order, workflow edges define
durable sequencing, `#` begins a comment. The visual graph, typed forms, JSON
transport and text DSL are **lossless projections of one AST**, not separate
execution languages. Node ids stay stable across layout and label changes so
diffs, state and explanations survive editing.

| Family | Words |
| --- | --- |
| Metadata | `program owner purpose tags mode` |
| Parameters | `param state` |
| Triggers | `on fact`, `on every`, `on at`, `on signal`, `on manual`, `on decision`, `on receipt` |
| Timing | `every elapsed/calendar`, `anchor`, `resolution`, `max_lateness`, `coalesce_window`, `missed`, `inactive_gap`, `rephase` |
| Inputs | `input`, `view`, `record`, `signal`, `secret` |
| Features | `let`, `window`, `sum`, `count`, `avg`, `rate`, `lag`, `join`, `convert` |
| Recognition | `sense`, `when`, `crosses`, `enters`, `leaves`, `holds` |
| Learning | `learn`, `predict`, `update`, `validate` |
| Futures | `project`, `branch`, `assert` |
| Optimization | `solve`, `require`, `prefer`, `minimize`, `maximize`, `tie_break` |
| Workflow | `step`, `parallel`, `wait`, `approve`, `retry`, `compensate`, `cancel` |
| Routing | `observe`, `recommend`, `draft`, `ask`, `act` |
| Effects | `emit`, `do action`, `do transfer`, `do command`, `do http`, `do device`, `do ui` |
| Meta-control | `tune`, `revise`, `pause`, `resume`, `run` |
| Policy | `scope`, `freshness`, `dedupe`, `budget`, `require grant`, `on denied`, `on stale`, `on failure` |

**Mutation verbs make the data-changing boundary visible:**

| Verb | Mutates? |
| --- | --- |
| `let`, `sense`, `predict`, `project`, `solve` | No — pure |
| `emit` | Yes — append-only typed derived Fact |
| `recommend` | No — inert candidate |
| `draft` | Draft data only |
| `ask` | No, until answered |
| `act` | Yes — through a typed Action, after authorization |
| `do` | Outside-world attempt plus receipt |
| `tune` | Program parameter data, effective next occurrence |
| `revise` | Definition data only; activation is separate |
| `pause` / `resume` | Activation Fact; never deletes history |

- [x] **No generic `set field` or `eval string`.** Each `act`, `do`, `tune`,
  `revise` compiles to a typed candidate/Action with exact target, schema,
  preconditions, grant requirements and preview.
- [x] **Every declaration has a stable node id.** If omitted, the formatter
  derives it from the left-hand name and freezes it on first publish. **Moving a
  visual node, renaming its display label or reformatting text does not change
  the semantic hash.** Changing an expression, type, dependency, effect, policy
  requirement or stable id does.
- [x] **The AST — not the source string — is the canonical revision payload.**
  Text is compiled before storage: `let stock_low: bool = quantity(record:@apple)
  < 1kg` becomes a typed compare node whose operands carry resolved
  `record_uid`, `type`, `unit_uid` and canonical decimal text. The stored DSL is
  a human-readable projection and the visual editor reads and writes the same
  AST. **Unknown fields or node kinds fail validation instead of being
  ignored.**
- [x] **The expression language has no reflection, dynamic field names,
  unbounded loops, arbitrary recursion, shell interpolation, network calls or
  implicit reads.** Bounded `iterate max N until condition` is a graph node with
  deterministic fuel and a convergence trace.
- [x] **K1.4 strict text** starts `karma 1;`, carries `schema karma.program.v1;`,
  spells out tags, capabilities, parameter mutability, node bindings, port
  sensitivity/freshness, operations, state contracts and outputs. Collections
  are formatter-sorted because the AST uses ordered maps. Errors carry exact
  byte/line/column; source, token, string and nesting limits are enforced before
  storage. This canonical projection is not the amount of text a person types in
  the sand.
- [x] **K1.5 Frequency AST is separately versioned** (`karma.frequency.v1`,
  canonical text starting `karma-frequency 1;`). The revision holds slug,
  non-empty purpose, tags, named schedule parameters, cadence/anchor, timer
  service, missed/inactive/rephase/overload policies, and calendar
  timezone/tzdb/gap/fold data. **It never holds an active flag, consumer list,
  cursor, next deadline, lane, measured host capacity or wake result** — those
  are mutable handle and runtime state.
- [x] **Frequency parameters are deliberately smaller than Program values.** A
  named parameter is either a non-negative exact millisecond duration with
  inclusive min/default/max, or a positive `u32` with inclusive bounds. Schedule
  fields use a typed `literal(...)` or `parameter(local_id)` binding — **no
  expression evaluation, ambient parameter name, numeric coercion or arbitrary
  JSON.** Anchors, timezone identity, tzdb identity, weekday sets, day-of-month
  and policy enums require a **new revision** rather than parameter tuning,
  because changing them can reinterpret civil identity or authority.
- [x] **Compilation takes an explicit override map**, rejects unknown
  names/type mismatches/out-of-range values, fills the rest from revision
  defaults, and resolves bindings into a concrete `ElapsedSchedule` or
  `CalendarSchedule`. `CompiledFrequency` carries the revision hash, complete
  effective parameter map and concrete schedule; interval/timer integer ranges
  and cross-field timer invariants are validated after resolution. **Store and
  runtime persist the effective-parameter hash with an activation epoch and
  never compile from whatever mutable values happen to be visible halfway
  through a run.**
- [x] **Weekly weekdays sort Monday through Sunday; monthly invalid-day behavior
  is explicit.** Both calendar and elapsed schedules carry the same rephase
  policy, which changes future boundaries and **never rewrites a consumed
  boundary.** Success requires AST↔text round-trip plus an identical compiled
  schedule and revision hash for the same explicit parameter map.

### The replay capsule (K1.7)

- [x] **`EvaluationReplayCapsule` captures the complete boundary of one pure
  evaluation** — this is the first executable layer of the replay contract, not
  a claim that occurrence streams, models, grants, signatures, effects or Store
  checkpoints are captured yet. It has its own
  `karma.evaluation-replay-capsule.v1` schema plus a separately explicit
  `karma.evaluator.v1` semantic revision, and embeds the immutable `ProgramAst`,
  the Program revision hash, the complete frozen context, exact limits, the
  expected `EvaluationResult`, and that result's domain-separated hash.
  **Embedding the Program makes the capsule portable; the hash stops the
  embedded definition being silently substituted.**
- [x] **`SealedEvaluationReplayCapsule` wraps it**, its canonical hash covering
  every inner byte-equivalent field. Replay checks in this exact order: outer
  seal → embedded Program revision → stored expected-result hash → ordinary
  evaluation → exact expected/actual equality and canonical result hash. Each
  failure has a stable typed code; an evaluator failure stays nested as its
  original typed error. **There is no bypass that accepts stale inner hashes
  because a caller resealed the outer wrapper.**
- [x] **The seal is a content address, not an authenticity signature.** A caller
  may deliberately recapture fully changed inputs and output — that is a new
  capsule with a new hash. Ownership and signatures attach later without
  changing this content verification.
- [x] **The capsule contains values, not pointers** to live Records or state
  rows, so it serializes, moves, inspects and replays with **no Store, clock,
  timezone provider, filesystem, network, randomness, device or secret access.**
- [ ] **Later phases compose capsules into full occurrence/checkpoint capsules**
  with ordered Facts, schedules, models, policy/grant snapshots, captured ports
  and receipts. Persistence may content-address and deduplicate large capsules,
  but must preserve the same resolved canonical content and verification order.
- [ ] **Do not build a legacy import path.** Superseded 2026-07-30: the database
  was deleted, so there is nothing to import. Port the capability by hand and
  delete `nucleus::rule`. New capability must not be constrained by `rq1`/`kd2`
  token compatibility.

### The occurrence kernel — one scheduler owns causality

Heartbeats, subscriptions, sync imports, workflow wakes and effect completions
submit occurrences to it. None of them grows a private automation loop.

- [x] **Per occurrence, in order:** persist/deduplicate and assign the Cell
  cursor → freeze logical time, visible input cursor, active program/model
  revisions and triggering principal → select impacted programs from declared
  dependencies → evaluate pure nodes in stable graph order, recording every
  substituted value, missing/stale input, branch, model output and assertion →
  materialize inert candidates, then evaluate objectives, policy, authority,
  taint, budgets and conflicts → atomically store the run and any permitted
  intents → let separate workers claim intents, act, append receipts/Facts, and
  thereby enqueue later occurrences.
- [x] **Parallel work may improve latency but cannot change commit order or
  selection.**
- [ ] **One elected occurrence sequencer per writable Cell.** Multiple processes
  may execute leased effects; they may not race independent rule agendas against
  the same Ledger.
- [ ] **Recover all nonterminal runs, workflows and intents after restart.**
  Persist debounce, cooldown, last-consumed occurrence, schedule cursor, rate
  budget, leases and retry state. Boot never treats forgotten memory as new
  permission to fire.
- [ ] **Deterministic trigger concurrency policies:** `queue`, `drop`,
  `coalesce`, `latest`, bounded `parallel`, with a required correlation key.
  Backpressure is visible as lag or parked work, never silent loss.
- [ ] **Bound evaluations** by nodes, iterations, fuel, fan-out, candidate count,
  trace size and declared cost. A violation faults the run, optionally pauses
  the program, and opens one deduplicated operational decision.
- [ ] **Cell modes:** `normal`, `stage-effects` (evaluate and queue, dispatch
  nothing), `observe-only` (derivations continue, no action intents),
  `emergency-stop` (no new runs or effects except recovery and inspection).
  Durable, permissioned, visible.
- [ ] **Overload priority without hiding starvation:** safety/revocation and
  already-agreed Transfer deadlines first, then explicit user priority, ordinary
  workflows, learning maintenance, projections, background analysis. Every
  delayed class exposes queue age and next eligibility.
- [ ] **Deterministic agenda for simultaneous rules:** dependency order,
  explicit priority only where necessary, stable tie-breaking, atomic Action
  boundaries, and a recorded explanation of conflicts.
- [ ] **Conflicting candidates resolve by declared policy** — reject all,
  priority, merge with a typed commutative reducer, serialize, or ask. Arrival
  or thread order never silently chooses a writer, and rejected alternatives
  stay in the run explanation.
- [ ] **Proof analysis for** dependency cycles, contradictory writers,
  unreachable nodes, unsafe external effects, authority escalation, dead ends,
  non-convergence, fan-out explosion, stale/missing paths, unit/schema mismatch,
  privacy declassification and likely divergence. The runtime cascade cap is the
  final guard, not the design tool.
- [x] **Warnings are advice, never rejections** (cycles, Proof loops).
  `create-rule`/`update-rule` reload the registry and return warnings in
  `outcome.warnings`; saving succeeds and the interface must show the warning.
- [x] **Delivery is reactive:** only rules reading changed records re-evaluate,
  and cascades stop at 256 evaluations so a runaway loop survives for
  inspection. A rule without consequences is a named derived value read through
  `value(@rules.x)`.
- [ ] **Persist debounce/cooldown and last-consumed occurrence** so restarts
  cannot double-fire or re-arm one-shot behavior. Debounce is in-memory today
  and resets on reload.

### Reaction before learning

For one incoming change, Lince uses the definitions active **before that
change**. Existing behavior reacts first; learning adapts afterwards. This stops
a newly learned rule reinterpreting the very evidence that created it.

- [x] **Two ordered lanes per cursor.** *Reaction:* freeze active revisions,
  parameters, grants and checkpoints; evaluate impacted rules/Senses/workflows;
  commit the run and authorized local intents. Synchronous local Actions may
  append child Facts whose reaction occurrences also stay ahead of learning;
  external effects remain durable intents whose receipts are new occurrences.
  *Learning:* after the bounded reaction closure, admit/reject evidence, update
  models, detect patterns, create recommendations or revision candidates.
- [x] **Adaptation is its own later occurrence.** An approved or pre-delegated
  `tune`, `revise`, `activate`, `pause` or promotion commits with
  `effective_from_cursor` strictly greater than the event that proposed it. **No
  definition changes halfway through a run or cascade.** An event at cursor 100
  is handled by epoch 12 even if its evidence raises a pattern above threshold;
  epoch 13 starts at cursor 101 or later. `replay from cursor 100 under rev 13`
  is a new visible occurrence, not retroactive history.
- [ ] **Prioritize reaction over background learning** so a burst of model
  maintenance never makes obvious rules feel unresponsive. Bound the reaction
  closure and expose queue age; runaway cascades fault instead of starving
  learning forever.
- [ ] **Learning may compute in parallel from immutable snapshots, but
  checkpoint commits stay cursor-ordered.** A run records
  `model_trained_through_cursor` so a person can see when a prediction used
  lagging state.
- [ ] **Meta-rules alter named Programs only through `tune`/`revise`/`activate`/
  `pause`/`resume` under an `karma.manage` grant.** Never by mutating in-memory
  nodes or schedule rows.
- [ ] **Changing a schedule requires an explicit rephase policy** —
  `preserve_anchor` (default), `from_last_intended`, `from_change`,
  `immediate_if_overdue` — and the run preview shows old and new next
  occurrences before the parameter Action commits.

### The replay contract

**The guarantee:** given the same replay capsule and ordered captured inputs,
the same engine emits the same canonical node values, candidates, policy
decisions, intents, and unsigned Ledger payloads/content hashes. Captured
signatures and receipts replay as their original bytes; a simulator uses a
fixture signer rather than production secrets. It does **not** promise that
rerunning an HTTP request or motor command changes the world the same way.

A capsule contains: starting checkpoint/hash-chain anchor, ordered Fact and
occurrence stream, program/model/grant revision hashes, engine and schema build,
Lingua/unit conversion revisions, tzdb version, virtual clock, deterministic
seed, solver/plugin hashes, captured Signal results, external receipts. It
replays with no network, filesystem, wall clock, device or secret.

- [ ] **Put clock, scheduling, entropy, uid generation, filesystem, network,
  process execution, device I/O and model calls behind injected runtime ports.**
  Pure evaluation cannot call an ambient OS API. Production adapters capture a
  result; simulation adapters generate or replay one.
- [ ] **Every accepted local Fact/occurrence gets a monotonically increasing
  Cell cursor in its commit transaction.** Live behavior follows recorded
  arrival order. Sync packages may arrive in a different order elsewhere; replay
  reproduces each Cell's observed order rather than claiming distributed
  simultaneity.
- [ ] **Canonicalize maps, sets, strings, units, timestamps, serialization.**
  Sort unordered query results and graph edges explicitly. Stable ordering is
  dependency rank → declared priority → program uid → node id → occurrence uid.
  Thread completion order never breaks a tie.
- [ ] **No binary floating point in a decision.** Canonical decimal, rational,
  integer base-unit or specified fixed-point only. Probability scale, rounding
  mode, overflow, invalid values and unit conversion are part of the type.
  `NaN`, infinities, locale parsing and platform math must not enter a policy
  decision.
- [ ] **Any stochastic algorithm receives a recorded seed and a deterministic
  stream partition per node.** Any solver declares version, tolerances, variable
  ordering, timeout in deterministic work units, and a stable tie-break. "First
  result returned by workers" is not a valid choice rule.
- [ ] **Content-address pure extensions** (e.g. sandboxed WebAssembly), deny
  them clock/random/I/O, give them deterministic fuel and memory limits, specify
  their numeric ABI. Native or remote opaque computation enters as a captured
  Signal instead.
- [ ] **Nondeterministic model output is an observation** with model id, request
  hash, response hash and capture time; replay uses the captured response. A
  deterministic local model still pins weights, feature schema, runtime,
  tokenizer, numeric policy and seed.
- [ ] **Derive run, candidate and intent ids from their semantic occurrence**
  where practical, and generate any remaining ids/timestamps through the replay
  runtime so replay does not manufacture different identities.
- [ ] **Freeze the effective grant at proposal time for explanation, but recheck
  revocation, budgets, target revision and interlocks when an intent is claimed
  and again immediately before irreversible dispatch.** A revoked intent
  deterministically becomes denied/cancelled, never raced.
- [ ] **Upgrades never reinterpret an old run silently.** Replaying under the old
  engine is reproduction; replaying under a new one is an explicit differential
  run whose changed candidates, facts and effects are shown.
---

## 5. Rule CRUD — storing, versioning and reading rules

Why: a person must be able to write a rule, look at it, change it, turn it off,
and see why it did what it did — without database access. At the end of this
block a rule can be authored, validated, stored, diffed, inspected and
activated, but an active Program still does not run automatically.

### What is durable, and what each object means

Everything durable is a Record plus schema-owned typed sidecar state, and every
semantic transition appends a Fact. That does not mean forcing an execution
trace into a record body — it means each object has ordinary uid, origin,
ownership, visibility, links, activation and provenance behavior.

| Object | Meaning |
| --- | --- |
| **Program** | Mutable handle people organize and activate: owner, purpose, active revision, tags, default policy. Its quantity is the on/off knob. |
| **Program revision** | Immutable content-hashed typed graph plus declared inputs, outputs, parameters, objective, policy requirements, failure/concurrency behavior. Slugs resolve to uids at publish. |
| **Frequency / revision** | Reusable mutable schedule handle plus immutable cadence/timer/catch-up policy. A cursor exists only while some active Program, Signal poll or workflow references it. |
| **Trigger occurrence** | One durable reason work exists — Fact cursor, schedule boundary, signal sample, manual run, workflow wake, sync arrival, retry — carrying logical time and dedup identity. |
| **Run** | One evaluation of one revision against one frozen visible input cursor: node trace, proposals, policy results, resource use, terminal state. |
| **Evidence set** | The exact Facts/observations plus inclusion and exclusion reasons behind a feature, pattern update, forecast or recommendation. |
| **Model spec/checkpoint** | Versioned feature schema, deterministic algorithm and parameters, training cursor, learned state, validation metrics, drift state, implementation hash. |
| **Candidate** | Inert proposed conclusion, plan, revision, recommendation, decision, Action or Transfer change. **A candidate has no authority.** |
| **Delegation grant** | A principal's signed revocable capability envelope (block 7). |
| **Action intent** | Authorized durable request awaiting an executor, freezing the exact Action, policy proof, idempotency key, deadline and compensation metadata. |
| **Attempt/receipt** | Each lease, dispatch, response, timeout, retry, cancellation, external id, captured output hash and result. **A receipt is evidence, not proof that an unobservable real-world claim is true.** |
| **Workflow instance** | Durable node position, correlation key, child runs, waits, approvals, compensation stack, cancellation state. |

- [ ] **Every object gets a stable uid under the existing families** and is
  exposed through Protein. Karma objects that are Records keep `r_...`, Facts
  keep `f_...`; the typed object *kind* distinguishes them, never a new uid
  alphabet.
- [ ] **Store the complete revision and grant used by a run, by hash.** Later
  edits or revocation must never make an old explanation describe new policy.
- [ ] **Definition status and run status are separate.** Definition:
  `draft → proven → shadow → active → superseded/retired`. Run:
  `queued → evaluating → staged/waiting → executing →
  completed/failed/cancelled/dead-letter`. "Faulted" may pause new occurrences
  without pretending the quantity was manually changed.
- [ ] **Garbage collection may compact traces and checkpoints only behind hash
  anchors and configured retention.** Evidence needed for an active grant,
  unsettled Transfer, open decision, reproducible run or audit hold stays hot.
- [ ] **Every program declares** owner, purpose, data scope, authority ceiling,
  budgets, triggers, failure policy and enabled revision. No default may
  silently widen visibility or authority.
- [ ] **Every evaluation gets a durable `karma_run` identity** — revision,
  triggering occurrence, input cursor, logical clock/seed, node trace,
  candidates, policy decisions, intents, Actions, resource cost, final status.
  "Why did this happen?" and "what will retry?" are ordinary reads, not logs on
  disk.
- [ ] **Effective program scope is an intersection**: declared input Protein ∩
  owner's visibility at the run cursor ∩ purpose/declassification policy ∩ the
  triggering principal's grant. Hidden data must not leak through features,
  aggregates, model parameters, explanations or effects.

### Identifiers

Three layers: **UID** is canonical on the wire, in Facts, signatures, links and
stored revisions. **Typed reference** is compact authoring syntax
(`prog:@apple.restock`) resolved to a uid at publish, storing both uid and
displayed slug, so a rename cannot change meaning. **Bare `@slug`** is allowed
only where the expected port type makes the kind unambiguous — ambiguity is a
compile error, never a best match.

`prog` Program · `rev` revision · `node` node in a revision · `sig` Signal ·
`freq` Frequency · `sense` recognizer · `view` saved Protein · `model` ·
`obj` objective · `flow` workflow · `grant` · `trust` scope · `run` · `cand` ·
`dec` decision · `intent` · `receipt` · `sim`.

- [x] **Slugs are `dot.case`, namespaced by kind**, so `prog:@daily` and
  `freq:@daily` coexist. Program-local node and parameter ids are lower
  `snake_case` because they appear as stable DSL fields and diff keys.
  User-facing heads are free text and may change without changing references.

### Program persistence (K2.1)

- [x] **Handle = `RecordKind::Program` Record + one `karma_program` sidecar**,
  owning mutable `handle_revision`, lifecycle status, immutable head revision
  hash, optional active revision hash, optional owner, timestamps.
  `record.quantity` mirrors activation only (`0`/`1`) for existing Record
  tooling — it is **not** the Program's semantic state — and changes in the same
  transaction as the sidecar and evidence Fact.
- [x] **`karma_program_revision` rows are immutable and content-addressed**,
  storing canonical AST JSON, canonical DSL, complete Proof JSON/status and
  creation time. Loading re-deserializes all three, recomputes the hash,
  reformats the DSL and recomputes Proof; corruption is an error, never an
  accepted cached definition.
- [x] **A rejected-Proof revision may be stored as an editable draft head but can
  never become active.** Revising an active handle changes only its head;
  activating is a separate expected-revision mutation, so editing cannot
  silently replace the code an occurrence is using.
- [x] **Commands take a globally unique bounded `request_id`.**
  `karma_program_request` stores canonical payload hash, action, expected/result
  handle revisions, selected definition hash and linked Fact. Exact replays
  return the prior result and Fact; the same id with a different payload is a
  conflict. Handle updates are one compare-and-swap statement — a miss returns
  the current revision without partially inserting anything.
- [x] **All Karma command families reserve request ids in one immutable global
  namespace before writing their family-specific journal**, so a cross-family
  collision rolls back atomically.
- [x] **Every committed command appends a typed `ProgramMutationEvidence` Fact in
  the same transaction**, freezing prior/new head and active hashes, request id,
  action, handle revision and actor. Definition-only mutations use delta zero,
  first activation `+1`, pause `-1`, switching accepted active revisions zero. A
  signing callback may attach the Trust signature before commit.

### Frequency persistence (K2.2)

- [x] **Three deliberately separate identities.** Mutable **handle**
  (`RecordKind::Frequency` Record + `karma_frequency` sidecar + CAS
  `handle_revision`). Immutable **revision** (canonical `FrequencyAst`, its DSL
  projection, a default compilation). Immutable **activation epoch** (chosen
  definition revision, complete effective parameter map including defaults,
  effective-parameter hash, compiled schedule, previous activation hash,
  activating handle revision, cause, logical activation time). Cursors and
  occurrences name the **activation hash**, never the mutable handle.
- [x] **Why the split: deterministic parameter changes.** Revising an active
  Frequency changes only its head; the old revision and activation keep
  governing until an explicit activation. `set-parameters` compiles a complete
  replacement override map against the active definition and creates a new
  epoch; `reset-parameters` does the same with defaults. Neither mutates the
  authored revision or an old epoch.
- [x] **A semantically identical activation while already active is rejected as a
  no-op**, so every committed handle revision has observable meaning.
- [x] **Pausing clears the active revision/epoch but keeps `latest_activation`**,
  so reactivation creates a new epoch linked across the pause. No epoch is
  erased; history reconstructs from immutable epochs and Facts.
- [x] **Six request-idempotent CAS commands:** create, revise, activate,
  set-parameters, reset-parameters, pause. Each request fingerprint includes
  action, uid, expected handle revision, selected definition, complete
  overrides, owner and actor. `karma_frequency_request` retains the canonical
  original result and Fact, so replay after arbitrary later mutations returns
  the original snapshot.
- [x] **Loads independently verify everything.** Definition insertion compiles
  with defaults before SQL is touched. Repository loads deserialize the AST,
  parse and reformat the DSL, recompute the revision hash, recompile defaults
  and compare byte-for-byte. Epoch loads recompile the named revision using the
  stored effective map as explicit overrides and compare parameter hash and
  compiled schedule byte-for-byte. This catches database corruption **and
  compiler drift** at the boundary.
- [x] **An activation epoch is configuration, not execution.** K2.2 creates no
  timer, thread, poll loop, cursor or occurrence.

### Cursors and the dispatcher (K2.3)

- [x] **One `karma_schedule_cursor` per active activation hash**, freezing last
  and next intended boundary, cursor revision, lifecycle (`armed`, `leased`,
  `paused`, `superseded`, `failed`), lease fencing token and expiry, last
  occurrence sequence, last error. Elapsed cursors use `ScheduleCursor`;
  calendar cursors retain requested civil boundary plus resolved
  instant/discontinuity evidence. Cursor creation, replacement and
  pause/supersede are driven from the Frequency mutation journal, never inferred
  by scanning Records.
- [x] **Claim is fenced.** At a wake the dispatcher pops only entries whose arm
  window is reachable, then asks the Store to claim each exact
  `(activation_hash, cursor_revision)`. The claim verifies the Frequency still
  names that activation, advances `armed → leased`, increments a monotonic
  fencing token, sets a bounded lease expiry. Stale heap entries, superseded
  epochs and duplicate workers lose the CAS without an occurrence. After pure
  advancement, one transaction appends the occurrence and new cursor state and
  clears the lease. A crashed worker leaves no ambiguous commit: an expired
  lease is reclaimable with a higher token, and the unique
  `(activation_hash, sequence)` prevents replayed side effects.
- [x] **The planner is pure and clockless.** It takes an ordered snapshot of
  armed entries, host timer capabilities, the active resource grant, exact
  persisted per-entry demand and exact aggregate capacity, and returns ordered
  admissions/rejections plus a deadline index whose minimum is the next host
  timer request. Logical `now` belongs to cursor advancement and diagnostics,
  not resource arithmetic.
- [x] **Injected `DeadlineClock`.** The Tokio adapter waits against a monotonic
  instant, projects elapsed duration onto the wall-clock observation, and
  reports a typed `ClockDiscontinuity` beyond an explicit tolerance; the
  director rebuilds once at that boundary. Wall-clock jumps, suspend/resume and
  restarts are handled by the missed and inactive-gap policies during
  advancement, **never by assuming a loop ran while the process slept**.
  Simulation supplies the same port with a manual clock.
- [x] **Replan only at boot or an explicit directory-change notification.** A
  normal firing removes one due entry, completes its fenced transaction, and
  reinserts only the returned next revision into its existing admitted lane — it
  must not requery or replan unrelated registrations. A contender losing to a
  live lease records the exact lease expiry as a recovery arm and rebuilds once
  at that instant; **this is not a retry interval.** A successful mutation
  publishes a lossless watch revision after commit, so activation, parameter,
  pause, provider, capability and grant changes cannot be missed between
  snapshots.
- [x] **Activation is a control-plane transaction, not a raw Store call.** The
  Engine prepares the candidate epoch/cursor, plans it together with all
  currently armed work under one host-capability and grant snapshot, and commits
  only the admitted result. `reject_activation` leaves no epoch or cursor;
  `pause_and_ask` may commit an explicitly paused cursor with typed admission
  evidence; `degrade_within_grant` may commit only the precise bounded
  degradation the planner returned. A cursor without a matching durable
  admission record is deliberately unclaimable, and a new candidate cannot evict
  an incumbent. Direct Store functions stay persistence primitives for recovery
  and tests, not the human/agent contract.
- [x] **"Generate higher-frequency Rust checks" is explicitly forbidden.**
  Runtime data adds and removes heap entries and host timer registrations, not
  code or permanent loops.
- [ ] **Multiple dedicated lane arms stay a measured optimization**, behind the
  same interface, added only where latency/energy measurements justify them. Not
  part of the semantic exit gate.

### The typed Actions and the read side (K2.4)

`create-karma-program`, `revise-karma-program`, `activate-karma-program`,
`pause-karma-program`, `create-karma-frequency`, `revise-karma-frequency`,
`activate-karma-frequency`, `set-karma-frequency-parameters`,
`reset-karma-frequency-parameters`, `pause-karma-frequency`.

Later blocks add: `validate-karma-definition` (parse/type-check/canonicalize and
return Proof without storing or executing), `fork-karma-program`,
`set-karma-parameter` / `reset-karma-parameter`, `retire-karma-program`,
`run-karma-program` (with `dry_run` forbidding effects), `replay-karma-run`,
`rebuild-karma-model` / `disable-karma-model`, grant create/narrow/revoke,
automation-trust scope create/revise/activate, `respond-karma-candidate`,
`control-karma-workflow`, `control-karma-intent`, `simulate-karma-program`,
`import-karma-template`. Answering a decision reuses the existing `decide` —
Karma does not create a second decision action.

- [x] **Wire mannerisms:** kebab-case `action` tag, snake_case fields,
  engine-derived viewer/principal, `request_id` for replay safety,
  `expected_revision` for mutable handles.
- [x] **The authenticated session actor is the sole authorship source.** Payloads
  carry no second spoofable actor field, and a UI or agent cannot name a more
  powerful actor.
- [x] **Activation requires an installed immutable
  `KarmaDeadlineDirectorConfig`** so host timer capabilities, aggregate
  grant/capacity, calibration, clock and pinned providers are the exact values
  admission used. Absence is a typed fail-closed `karma_runtime_unconfigured`.
- [x] **A committed mutation returns its uid and Fact; an identical replay
  returns the same object without republishing the Fact**; a stale expected
  revision returns `karma_stale_handle_revision` carrying the current revision.
- [x] **`source:"karma"` is one heterogeneous deterministic union**, not a query
  language per feature. `object_kind` selects `program`, `program_revision`,
  `frequency`, `frequency_revision`, `frequency_activation`, `schedule_cursor`,
  `schedule_occurrence`. Common fields: `uid`, `kind`, `program_uid`,
  `revision_uid`, `owner_uid`, `status`, `quantity`, `at`, `cursor`, `cause`,
  `visibility`. **Unsupported predicates and includes are errors, not ignored
  filters.**
- [x] **Capability booleans mean "structurally submittable", not "will be
  allowed".** Stable blockers explain states like `program_not_active` or
  `head_already_active`; Frequency rows state `requires_runtime_admission:true`.
  The Action boundary recomputes admission regardless. Block 7 adds
  principal/grant-specific projection without weakening that check.
- [x] **Filters never infer an Action.** The UI copies a provided typed Action
  template, adds a new request id, and submits it through the ordinary path.
- [x] **Remote visibility is deny-by-default** until fine-grained Karma grants
  exist.
- [ ] **"Why did this happen?" is one query, not a log hunt** — one run uid with
  `include: { trace, inputs{facts,exclusions}, model, policy,
  intents{receipts}, causal_chain }`.
- [ ] **Predicates and includes per interface:** Program/revision
  (`program_eq`, `revision_eq`, `owner_eq`, `tag_in`, `status_in`, `active`,
  `purpose_eq`; include `definition`, `parameters`, `proof`, `diff`,
  `dependencies`, `capabilities`); Occurrence/run (`trigger_kind_in`,
  `cursor_gte/lte`, `at_since`, `cause_eq`, `status_in`; include `trace`,
  `inputs`, `candidates`, `policy`, `intents`, `receipts`, `cost`,
  `replay_capsule`); Model/evidence (`model_eq`,
  `trained_through_cursor_gte`, `drift_state_in`; include `spec`, `checkpoint`,
  `eligible_evidence`, `rejected_evidence`, `metrics`, `recommendations`);
  Candidate/decision (`candidate_kind_in`, `subject_eq`, `live`,
  `expires_before`; include `evidence`, `preview`, `alternatives`,
  `authority_required`, `capabilities`); Grant/intent/receipt (`principal_eq`,
  `capability_in`, `target_eq`, `status_in`, `deadline_before`; include `scope`,
  `budget`, `policy_proof`, `attempts`, `receipt`, `compensation`).

### What each event may and may not do

| Event | Appends | Does **not** happen |
| --- | --- | --- |
| Definition created/revised | Handle or new immutable revision, links, Proof, annotation Fact | No activation, grant, training or domain effect |
| Program activated/paused | Active revision pointer and/or quantity Fact, activation occurrence | No deletion of revisions or runs |
| Frequency activated/tuned, or gains/loses first/last consumer | Revision/parameter pointer, generation/cursor, exact deadline upsert/removal, annotation Fact | No polling loop, no rescheduling or due-check of unrelated Frequencies |
| Fact/schedule/signal arrives | Trigger occurrence with cursor/time/source | No rule runs before the occurrence is durable |
| Program evaluates | Run, trace, frozen input and policy references, candidates/intents | Pure nodes do not mutate domain Records |
| Learner updates | Evidence admission decisions, checkpoint/model Fact, metrics | No direct rule or authority change |
| Recommendation routes | Candidate or Decision Record and evidence links | No Action until accepted or independently authorized |
| Internal Action succeeds | Ordinary domain Facts plus intent receipt/provenance | No alternate privileged Karma write path |
| External effect completes | Attempt/receipt and provenance Fact | A receipt alone does not assert an unobserved outcome |
| Grant revoked | Revocation Fact, cancellation of preventable intents | Past Facts and effects are not erased |

**Karma migrations are edited in place until one ships.** None of `0026`–`0035`
has reached a deployment; this licence ends the moment one does.
---

## 6. Running a program, and writing the result back

Why: everything before this computes. This is where a rule actually changes a
Record — the one thing Karma had never been allowed to do — and it is built as
one closed loop: occurrence → frozen epoch → run → candidate → review → intent →
worker → Fact.

The whole block is restricted to **one capability family, `LocalReversibleData`**:
local, auditable on the Record's own Fact chain, reversible by an opposite exact
delta. Every external, social or irreversible effect stays behind the closed
door until block 15 and later. That restriction, not phase order, is what makes
writing safe to enable here.

### Occurrence ingress and Cell ordering (K3.1–K3.2)

- [x] **`KarmaOccurrenceEnvelope` is immutable and content-addressed**, freezing
  `logical_at`, source identity, optional causal parent, and the typed payload.
  **Its hash excludes Cell sequence and receipt time**, so importing the same
  evidence twice deduplicates even across threads or a restart. Sources start
  with `schedule-tick` and `schedule-coalesced`; Fact, Signal, sync, workflow,
  receipt and manual variants are added later without changing schedule identity.
- [x] **One transactional Cell sequence counter.** Ingress checks the
  source-kind/identity uniqueness boundary first: an identical canonical
  envelope returns the original row; the same source identity with a changed
  payload is a protocol conflict. Only a genuinely new envelope increments the
  counter. Rows and assigned sequences are immutable, and the row revalidates
  every projection plus content hash on load.
- [x] **The recorded sequence is the authoritative replay order for genuinely
  concurrent external arrival.** Deterministic internal producers must submit
  their already-sorted identities in one transaction.
- [x] **Batch expansion is a durable cursor, not a second timer.** Keyed by the
  immutable schedule occurrence hash: `individual` batches emit ticks in
  ordinal order through bounded pages, `coalesced` batches emit one aggregate
  and never individual ticks. Cursor advancement and generic occurrence
  insertion share a transaction, so a crash can repeat a page request but cannot
  skip or duplicate a tick. Every source identity derives from the semantic
  boundary or aggregate — never a page, wake or arrival metadata.
- [x] **Expansion is cooperative work.** Two non-zero configured bounds: semantic
  items per page (wire-capped at 4,096) and source batches per recovery turn. A
  deadline completion attempts one page immediately; boot performs one recovery
  turn; the director processes further turns only while a persisted incomplete
  cursor exists and no deadline is due, yielding between turns. So a five-hour
  schedule creates no millisecond polling, and a large replay cannot monopolize
  the director protecting a 3ms deadline. **The durable pending predicate, not a
  guessed interval, decides whether work exists.**

### Frozen epochs and terminal runs (K3.3–K3.4)

- [x] **One durable `next_cell_sequence`; never select a later occurrence while
  an earlier one is incomplete.** On first seeing an occurrence, snapshot every
  active `(program_uid, revision_hash)` in uid order, content-address that
  immutable selection as a **Program epoch**, and commit it before evaluation.
  Later activation, revision or pause cannot change which revision the
  occurrence saw. A page cursor inside the epoch advances atomically with each
  immutable run; epoch completion advances the Cell cursor. **Empty epochs are
  valid** and advance without manufacturing a run.
- [x] **Every epoch member gets exactly one typed terminal run:** `succeeded`,
  `not-applicable`, `blocked`, `evaluation-failed`. A Program without a matching
  trigger is durably **not-applicable**, never silently absent. For schedule
  occurrences, Frequency trigger nodes get frozen boolean pulses after resolving
  the activation to its immutable Frequency uid; the Program runs when at least
  one matches and all other triggers receive `false`.
- [x] **Missing adapters and deterministic evaluator failures are terminal and
  inspectable for that Program**, and do not poison later members or
  occurrences.
- [x] **A successful run stores the sealed pure-evaluation replay capsule**
  (K1.7): exact AST, frozen context, limits, trace, outputs, fuel, hashes. The
  run hash excludes persistence time and includes Cell sequence, occurrence,
  frozen epoch, Program identity/revision and outcome.
- [x] **Restart proof.** Interrupt after one member of a multi-member epoch,
  close and reopen the database, change the active Program set, resume: the old
  occurrence finishes its previously frozen members before the next
  `cell_sequence` freezes a new epoch, and the next occurrence sees the new
  active set. Runs order lexicographically by `(cell_sequence, member_ordinal)`,
  one row per occurrence/revision, identical hashes after reopen.
- [x] **The negative assertion matters:** a no-effect run appends no child
  occurrence, Fact, candidate, intent, Action, receipt or transfer, and the
  proof snapshots those counts around processing. Later phases must add each
  reaction through an explicit outbox/ingress boundary rather than gaining
  mutation as an accidental evaluator side effect.

### Durable program state (K4.1)

- [x] **Each frozen epoch member carries the Program's activation handle
  revision as well as its content revision** — the stable activation-generation
  token `on-program-activation` reset needs, which cannot be inferred later from
  the mutable handle.
- [x] **State is an immutable event chain plus one CAS projection per
  `(program_uid, node_id)`.** An event records state revision, previous event
  hash, source run hash, definition revision, activation handle revision,
  optional reset reason, and either a typed value or an explicit reset
  tombstone. Event hashes exclude database time.
- [x] **Run insertion, every state event/projection CAS, the epoch member cursor
  and the Cell cursor commit in one transaction.** A run can never become
  visible without its synchronous read-old/write-next state, nor can state
  advance for a run that is retried.
- [x] **Migration policy is explicit.** `on-program-activation` resets on
  activation-generation change; `on-revision-change` resets on definition
  change; otherwise `migration` decides — `reset` starts from the declared
  initial state, `require-explicit` produces a terminal blocked run,
  `compatible-type-only` carries state only when node operation/state kind and
  exact value type stay compatible. `never` and `manual` preserve state subject
  to that check.
- [x] **A reset plus a newly staged value may be one event carrying the reset
  reason**, so audit history shows evaluation read the initial state. A reset
  with no staged update stays an explicit tombstone rather than resurrecting old
  state later.
- [x] **Only `persistence:program` is supported.** Workflow and model-checkpoint
  state are terminal blocked outcomes until their runtimes can supply the
  correct scope key; treating them as Program state would merge independent
  workflows and models.

### Candidates and review (K4.2–K4.3)

- [x] **After a successful evaluation, scan stable node-trace and port-name
  order for value-bearing candidate datums.** Each becomes a typed
  content-addressed proposal keyed by source run, node and output port, carrying
  occurrence, revision, route, template and exact ordered fields. Missing datums
  create no row. **A duplicate `(run, node, port)` is an integrity conflict,
  never last-write-wins.**
- [x] **Initial lifecycle is always `proposed`, including route `act`.** Route
  expresses desired downstream handling, not authority.
- [x] **Review is an event-sourced CAS handle independent of the immutable
  proposal.** `respond-karma-candidate` requires a globally idempotent request
  id, candidate hash, expected state revision, and one typed response —
  `accept`, `dismiss`, `snooze(until)` with a canonical future logical instant.
  The actor comes from the authenticated session, never the payload.
- [x] **Each committed response appends an immutable state event, advances the
  projection by CAS, and appends a zero-delta audit Fact to the owning Program**,
  all in one transaction.
- [x] **Review is deliberately reversible.** Later responses may move an
  accepted, dismissed or snoozed candidate again — no user or agent decision is
  read-only history — and the event chain retains every change. Repeating the
  same status is allowed only through exact request replay, avoiding meaningless
  revisions.
- [x] **Accepting an `act` route still creates no intent.** Only block 7's grant
  and budget checks may turn a reviewed candidate into authorized work.

### Rules that define a Record's quantity (E0.3)

- [ ] **A Record may be bound to a Program revision and output, in one of two
  modes the author picks.** **Computed:** the quantity *is* the program's output,
  resolved exactly on every read, no Fact ever appended — it cannot drift from
  its inputs because it is never stored, so changing `Cost1` makes `Total cost`
  already correct with no occurrence, intent or grant involved.
  **Materialized:** a Frequency fires, the program runs, and an authorized intent
  writes the value as an ordinary signed Fact, so the number is pinned in
  history and the Record accrues an auditable chain.
- [ ] **The two answer different questions, and choosing wrong is the common
  mistake.** Computed answers "what is my total cost" — live, no history, free.
  Materialized answers "what was my total cost each month last year" — a real
  series, at the price of an occurrence, an intent and a grant. A rolling balance
  that accumulates (savings drawn down monthly) is **necessarily** materialized,
  because its next value depends on its previous one. A pure restatement of other
  Records should default to computed.
- [ ] **A computed Record refuses conflicting writes.** Its quantity has exactly
  one author, its binding. A manual Fact, an `AddQuantity`, or another program's
  intent targeting it is a typed refusal naming the binding — not a silent
  overwrite erased on the next read. Rebinding or unbinding is an explicit
  revision, and unbinding freezes the last computed value into one signed Fact so
  the Record keeps a defined quantity.
- [ ] **Cycles are refused at Proof time, not discovered at runtime.** Bindings
  form a graph over Records; `Total cost` depending on a Record that depends back
  on it is rejected when authored, with the cycle named. Depth and fan-out are
  bounded by the same fuel the evaluator already meters.

### Executing an authorized intent (E0.3)

Pulled forward as a whole, not as an apply-only shortcut: leases and typed
idempotency are not polish on top of applying a change, they are what stops a
restart mid-apply from applying it twice.

- [ ] **A worker leases an intent, records a typed attempt, applies the change in
  one Store transaction with the intent's frozen idempotency key, and writes a
  receipt.** The capability ceiling stays exactly where it is; nothing outside
  `LocalReversibleData` becomes executable here.
- [ ] **Three distinct targets, three distinct capabilities.** A Record's plain
  quantity; a Record's unit-denominated quantity, where the intent's unit must
  equal `unit_uid` or carry an explicit conversion **rechecked at apply time
  against the live Record** rather than trusted from the proposal; and a numeric
  value inside a namespaced `record_extension`. Authority over a Record's weight
  is not authority over its price.
- [ ] **Every applied change lands as an ordinary signed Fact on the Record's own
  chain** with an exact delta, caused by the intent. **Karma gets no private
  write path** — the Ledger stays the single quantity truth and an automatic
  change is auditable by exactly the same means as a human one.
- [ ] **Compensation is real, not nominal.** Reversing an applied intent appends
  the opposite exact delta caused by the original; a metadata write restores its
  previous frozen value. Reversibility is what makes this family safe to
  automate first.
- [ ] **A compensated intent keeps its budget consumed.** `holds_reservation()`
  returns false for `Compensated` today, which refunds it — so a grant capped at
  ten intents could apply-and-compensate forever without exhausting. A budget
  limits how much a delegation may *cause*, not how much of what it caused
  survives: an applied-then-reversed change touched a real Record and appended
  two real Facts. `Compensated` becomes true; `Failed` and `Cancelled` stay false
  because nothing happened. Revisit `DeadLetter`, which is false today and only
  correct if nothing was ever applied.
- [ ] **Retries are driven by the frozen idempotency key.** An uncertain outcome
  is reconciled against the Record's own chain rather than guessed. Emergency
  stop and stage-effects mode prevent every dispatch that is still preventable.
- [ ] **Revocation gets teeth.** `cancel_for_grant_tx` selects only `authorized`
  intents today, correct only while that is the sole reservation-holding state.
  Widen it to every state holding a reservation, or a revoked grant leaves leased
  work alive.

**Exit:** through the real socket, a Frequency fires, a Program reads a Record's
exact quantity, multiplies it by a granted percentage, and the authorized intent
changes that Record's quantity — Fact, receipt and policy proof all inspectable.
A restart mid-apply applies it exactly once. Revoking the grant mid-flight stops
it. Compensating returns the Record to its previous exact value. Without a
grant, nothing runs at all.
---

## 7. Authority — what a rule is allowed to do

Why: a program has no identity and no authority of its own. It acts as a named
person through a **revocable delegation**, and it can never enlarge that
delegation or pass it on. This block is what stands between "the rule decided to
do something" and "the rule did it".

**Authority is actor-neutral.** Humans, local tools and software agents use the
same typed Actions. Authorship is provenance, not permission.

**The effective authority for an intent is an intersection**, and any missing
term denies or stages it:

    program requirements ∩ principal delegation ∩ actor permissions ∩
    visible/purpose-allowed data ∩ applicable Automation Trust scope ∩
    current domain capability ∩ budgets ∩
    target revision/preconditions ∩ safety interlocks

A recommendation score, model confidence, owner role, template signature or past
successful run **cannot replace one of these terms.**

### Capability families and their default route

| Family | Default | Examples |
| --- | --- | --- |
| Pure read/derive/analyze | evaluate within data scope | Protein, feature, projection, solver, report |
| Local reversible data | suggest/ask until granted | set/add quantity, link, local metadata, create task |
| Attention/presentation | budgeted delivery | decision, digest, toast, focus a permitted interface |
| Program/meta-control | ask; narrow grants allowed | tune parameter, pause program, activate proven revision |
| External resource | staged; bound adapter grant | HTTP, command, filesystem, network, payment/device controller |
| Private Transfer preparation | suggest/draft | local draft, projection, rank counterparties |
| Social publication/negotiation | ask; exact grant allowed | publish OPEN offer, invite, counteroffer, message, visibility |
| Own social commitment/evidence | explicit high-authority grant | agree own revision, claim own occurrence, confirm own side, settle owned Record |
| Irreversible/safety-critical | manual or dedicated interlocked grant | door/vehicle/medical/industrial actuation, destructive command |

The destination does **not** hard-code "a machine may never commit" or
"automation may do everything". A person may deliberately delegate even
high-impact actions **on their own behalf** within exact limits; the engine makes
escalation explicit, narrow, revocable, rechecked and attributable.

### The grant (K5.1)

- [x] **A grant is a Record handle plus an immutable content-hashed revision.**
  Its principal is always the authenticated Person whose installed key signs the
  revision — **never an Action payload field.** Creation is disabled; activation
  is a separate expected-revision Action; revocation clears the active revision
  immediately. A revoked handle cannot be resurrected, so creating a replacement
  makes renewed consent explicit.
- [x] **`DelegationGrantSpec` scopes one named Program**, either an exact
  revision or whichever is active at the later check, plus a non-empty capability
  set, an explicit candidate-template scope (`any` or a non-empty exact set), an
  explicit target scope (`any` or a non-empty typed exact set), purpose, and a
  `[valid_from, expires_at)` interval.
- [x] **Target atoms keep their semantic kind** — Record, concept, Person, Organ,
  place, controller — so a matching string in the wrong namespace cannot
  authorize an Action.
- [x] **Delegation is non-transitive.** Grant-management capabilities cannot be
  delegated through a grant; authorizing grant creation and narrowing stays a
  separate human/session boundary. A program cannot create, widen, renew or
  choose the principal of its own grant.
- [x] **The only revision mutation is `narrow-karma-grant`, and the comparator
  must prove a subset.** Capabilities and exact sets may only lose members; `any`
  may become an exact set; an any-active Program revision may become one exact
  revision; `valid_from` may move later; expiry may move earlier. A different
  exact revision, a changed Program or principal, a newly added
  capability/target/template, longer validity, or a mixed narrow-and-widen edit
  is rejected. **There is deliberately no widen Action.**
- [x] **A proven narrowing of an active handle swaps head and active in one
  commit**, so no wider revision stays live between commits. Narrowing a draft
  moves the head without ever making it live.
- [x] **Authority evaluation takes a frozen typed request** — principal, Program
  and revision, candidate template, capability, optional typed target, logical
  instant — and evaluates **one explicitly named active grant revision.** The
  engine never unions all matching grants. The result is a structured list of
  stable denial reasons; absence of a grant or any mismatch is denial. Both
  candidate policy and the domain Action boundary invoke this same evaluator.
- [x] **The principal comes from the installed signing key.** The store accepts
  only a revision whose signature names the principal; the Engine resolves the
  Person from `trust::Signer` and refuses when no key is installed. An
  authenticated session bound to a different Person is refused rather than
  allowed to borrow the Cell's key.
- [x] **A grant Record's quantity tracks live authority** the way a Program's
  tracks live activation — the Fact delta follows the transition, so revoking a
  draft that never authorized anything moves nothing.
- [x] **Grant Actions reuse `karma:create` / `karma:update`.** No new permission
  key: the separateness the contract demands already comes from key-derived
  principals plus the kernel refusing to place `KarmaGrantNarrow`/`Widen` in any
  spec.
- [x] **Fixed while landing K5.2:** `karma_grant_revision` keyed rows by content
  hash alone, so two grants with byte-identical consent collided — breaking the
  contract's own "a replacement makes renewed consent explicit" path for
  identical terms. Revisions are now identified by `(grant_uid, revision_hash)`
  and lookups are scoped by grant.

### Budgets (K5.2)

A budget says how much a delegation may cause before it must be renewed.

- [x] **The budget lives on the grant revision, because it is part of what was
  consented to.** Optional lifetime intent cap, optional count per fixed-length
  window, optional total quantity limit with its unit. **Absent means
  unlimited**, so narrowing treats absent as the widest value: a replacement may
  lower a limit or add one, never raise or remove one.
- [x] **Windows are fixed-length and start at `valid_from`**, so which window an
  instant falls in is a pure function of the revision and replays exactly.
- [x] **Consumption is counted per grant *handle*, never per revision.** If
  narrowing reset consumption, narrowing would be a way to refill a spent budget
  — an escalation disguised as a restriction.
- [x] **The intent rows are the consumption ledger.** A budget check counts and
  sums the grant's intents still holding a reservation, inside the same
  transaction that inserts the new one, so no separate mutable counter can drift
  from the evidence. Cancelling an intent releases its reservation.
- [x] **Which states reserve budget is stated once, in the database.** A seeded
  status table carries `holds_reservation` and every budget query joins it
  instead of naming statuses — so `leased`, `dispatching` and `uncertain` start
  counting by being seeded, not by editing five `WHERE` clauses, which is the
  omission that would let a leased intent's reservation be spent twice. Only
  statuses a phase can reach are seeded, and a test walks the table against the
  kernel enum so the two cannot drift.
- [x] **An unbudgeted grant is omitted from the wire entirely**, which keeps the
  K5.1 golden authority hash valid and lets revisions stored before budgets
  existed still verify.
- [ ] **Reserve on authorization, reconcile on receipt, release on
  denial/cancellation.** Concurrent runs must not each see the full remaining
  budget and overspend it.

### The intent (K5.2)

- [x] **An intent freezes what was authorized:** source candidate, exact grant
  handle and revision, Program and revision, capability, typed target, candidate
  template, frozen typed Action payload, idempotency key, deadline, and the full
  policy proof (authority decision plus budget snapshot at reservation time).
  Content-addressed and immutable.
- [x] **Amount and target are read from the stored proposal, never supplied by
  the caller.** A client that could name the amount could understate it and spend
  a budget it was never given. A proposal carrying two quantity or two reference
  fields is refused rather than disambiguated by guessing.
- [x] **There is no `denied` row.** A denial refuses the whole acceptance instead
  of recording a dead intent.
- [x] **Lifecycle is an immutable per-intent hash-chained transition log plus one
  current projection matching its head.** A transition names the durable
  *request* that caused it rather than a Fact, because one cause legitimately
  moves many intents — revoking a grant cancels everything it authorized — and
  the Fact is reachable through the request rather than copied onto each row.
  Cancellation is written after the causing request row exists, so no transition
  can cite a cause that was not recorded first.
- [x] **Accepting a candidate and authorizing its intent are one commit.**
  `respond-karma-candidate` gains an optional `authorizing_grant_uid`. Accepting
  an `act` candidate **without** naming a grant keeps the candidate inert.
  Naming one makes the same transaction re-evaluate the live grant, reserve
  budget and create the intent — and if the grant denies, the budget is
  exhausted, or the route is not `act`, **the whole Action fails and nothing
  changes.** A person asking for authorized work never silently gets an accepted
  candidate with no authority behind it. Exactly one grant is named, never a
  union.
- [x] **Only `LocalReversibleData` templates are mapped, checked twice**, so a
  later template cannot quietly reach further.
- [x] **Revocation is deterministic, not raced.** Revoking a grant cancels its
  still-authorized intents in the same transaction and releases their budget, so
  no intent outlives the consent that created it. Creating an intent appends its
  own Fact; cancelling one does not, because the revocation already appends a
  signed lifecycle Fact naming the grant and every cancelled intent is derivable
  from it. When a single intent can end on its own, that transition needs its own
  Fact.

### Attribution and enforcement

- [ ] **Attribute every automatic Action to both the real principal and
  `program/revision/run/intent`, with `cause=karma`.** The program never becomes
  a Person, signs as another Person, or obscures which delegation was consumed.
- [ ] **Enforcement is in the engine and the Action/domain boundary** — never
  only in the Karma sand, another sand, a connector, or an agent prompt.
- [ ] **Persist scopes, budgets, recipients, quiet time, thresholds,
  model/evidence restrictions, forbidden Actions and pattern overrides as typed
  policy**, not frontend state.
- [ ] **Revocation and emergency-stop prevent unclaimed work immediately and are
  rechecked before dispatch.** Already committed local Facts remain; an already
  dispatched external action gets an honest receipt or uncertain state plus any
  declared compensation.
- [ ] **Grants are typed records signed by the delegating Person**, scoping at
  minimum: principal, program and optionally exact revision/template, capability
  and Action kinds, target records/concepts/places/controllers,
  recipients/Organs/proximity, quantity/value and its unit, per-run/day/window
  rate, valid time/context, evidence quality, allowed visibility, reversibility,
  approval threshold, expiry.

**Exit:** deny-by-default holds at both the evaluator and the domain Action
boundary; revocation races and concurrent budgets are safe; restart never
duplicates an idempotent intent; emergency and stage-effects modes prevent all
still-preventable dispatch.
---

## 8. Entries and classification — individual changes a person types

Why: someone needs to record "ice cream, food, -10" and be able to fix it later.
The number lives on the Ledger; what the change *was for* is a separate
assertion about that Fact.

**Naming, settled 2026-07-26.** This was built with Economy in its names — a
`store::economy` module, an `economy_event` table, a private `Source::Economy`
Protein union, a 655-line `nucleus::karma::economy` in the kernel. The giveaway
was that the private Protein source existed only because generic Fact
aggregation was too weak: it summed with `f64`, undoing exactness, and could not
group by what a change was for. Routing around a weak primitive instead of
strengthening it is how a domain silo starts. Everything is now domain-neutral,
and one query answers "what did I spend on food in March", "how much flour did I
use", and "how many hours went to this project".

| Was | Is |
| --- | --- |
| `store::economy` | `store::ledger` |
| `store::economy_events` | `store::entries` |
| `economy_event` / `_revision` | `entry` / `entry_revision` |
| `0036_economy_classification.sql` | `0036_classification.sql` |
| `0037_economy_events.sql` | `0037_entries.sql` |
| `Action::CaptureMovement` | `Action::CaptureEntry` |
| `ReviseMovement` / `VoidMovement` | `ReviseEntry` / `VoidEntry` |
| `Source::Economy` | deleted — folded into `Source::Fact` aggregation |
| `Predicate::ResourceConceptIn` | `Predicate::ClassifiedIn` |
| `nucleus::karma::economy` | deleted, 655 lines, unreferenced |

The kernel module encoded exactly what this design rejects: `EconomyDirection =
Gain | Loss` (direction is the delta's sign) and a `tags` field parallel to the
concept DAG.

### Classification

- [x] **Classification attaches to the Fact, not the Record, and it is derived.**
  `record.concept_uid` says what a quantity *is of* — the flour Record's concept
  is flour — so it cannot say what a *movement* was. Baking bread is a `-500g`
  Fact on the flour Record; that the flour went to `@bread` rather than `@cake`
  is a property of that movement. Anyone tempted to simplify this into a Record
  per movement: that design forces every use to invent a Record and still cannot
  answer "how much flour went to bread in March" without a hand-maintained sum.
- [x] **Append-only sidecar keyed on `fact_uid`, never a column on `fact`.**
  `fact` is hash-chained and signed; a `concept_uid` inside the preimage makes
  every existing Fact permanently unclassifiable, and one outside the preimage
  is unsigned mutable data masquerading as ledger truth. Structured as the
  log-plus-projection idiom (`karma_intent_event`/`karma_intent_state`), so
  correcting a mistagged expense is a new assertion with an audit trail, never a
  compensating Fact over a typo.
- [x] **Drop `direction` as a field.** Direction is the sign of the delta and
  nothing else: a refunded ice cream is classified `@cost` with a `+10` delta
  and must *reduce* total costs. A direction enum bucketed independently of the
  sign gets that backwards, and two axes that can disagree is a bug frozen into
  the vocabulary.
- [x] **Two predicates because there are two axes.** `resource_concept_in`
  selects the Records whose levels moved; `concept_in` filters what the
  movements were. "How much flour went to bread" is `@flour` resources and
  `@bread` movements; a single filter quietly answers a different question. The
  context
  row echoes both, so "which axis produced these numbers" is on the wire.
- [x] **The store surface this landed as:** `add_record_concept`,
  `remove_record_concept`, `record_concepts`, `records_with_concept` (expands
  down the DAG and unions both axes, so a Record classified either way appears
  exactly once), `classify_fact`, `classification_history`, `level_at`,
  `level_series`, `movement_totals_by_concept`, `archivable_before`,
  `delete_by_uids`, and `[from, to)` variants of `facts::sum_window` /
  `sum_pos_window` / `sum_neg_window`. The sidecar shape follows the existing
  `fact_action_intent` pattern.
- [ ] **Hash-chain and sign the classification log if it ever becomes evidence.**
  Today it is append-only with actor and timestamp but unsigned — deliberate,
  because a classification is an assertion *about* the Ledger rather than Ledger
  truth.

### Aggregation

- [x] **Generic Fact aggregation, not a private source.** `GroupBy::Total` and
  `GroupBy::Classification`; `Predicate::ClassifiedIn` and `Predicate::AtBefore`;
  exact text sums with gains/losses/net/count; buckets keyed by `(group, unit)`
  so litres never join kilograms; a named `(unclassified)` bucket; `concept_in`
  on the Fact source reading what a Record *counts as*. No `f64` in either
  aggregate path.
- [x] **Windows are arbitrary half-open instants, not trailing durations.**
  Existing helpers took `window_secs` back from now, which cannot express "10:23
  on 1 Jan 2020 until 00:00 on 2 Mar 2025". Half-open so adjacent periods tile
  without a Fact landing in both. Positive and negative sums come back
  separately alongside the net.
- [x] **Normalize `fact.at` to UTC `Z` on write.** `at` is TEXT compared
  lexically; a `+00:00` cutoff against a `Z` row compares wrong in a way that
  looks exactly like a correct answer. Everything goes through `facts::instant`.
- [x] **`fact.at` is occurred-at, not recorded-at**, so backdating is ordinary
  and nothing may assume `at` is monotonic with chain order.
- [x] **Totals read a *set* of Records** selected by concept — flour lives in
  the pantry, the shelf and the freezer at once.
- [x] **Unit mixing is separated, not refused.** `movement_totals` rejects a
  Record set spanning two units, which is right as an invariant and wrong as an
  answer — "some of it is in kilos and some in bags" is a real situation. Protein
  groups
  resolved Records by unit *before* totalling, so each call sums one unit by
  construction. **"No unit" is one of the separated units**, reported as
  `unit_uid: null`, not a wildcard merged into whatever else is present.
- [x] **An unscoped or windowless query is refused**, as is a reversed window
  and a window bound that fails to parse. Totalling every Record would add hours
  to kilograms to litres; a total with no window is not an answer; falling
  through to "no filter" would answer a different window and look correct doing
  it.
- [x] **The unclassified bucket is always emitted, even at zero.** One that
  disappears when empty is indistinguishable from one never computed, and
  "everything is categorised" is the claim a person needs before trusting a
  total.
- [x] **Exact values cross the wire as canonical text, not JSON numbers.** A
  `-1020.25` becoming an IEEE double at the boundary would undo exactness at the
  one place it is hardest to notice.
- [x] **Remote subjects see nothing.** Whole-row visibility grants cannot express
  "you may see the sum but not its parts", and a partial total is worse than no
  answer because it looks complete. Revisit when fine-grained grants exist.
- [ ] **Aggregation is a query over classified Facts, never over the quantities
  of cost Records.** `Rent = 1000` is a standing parameter a rule reads; the
  monthly `-1000` classified `@rent → @cost` is what a total sums. Stating this
  precisely is what stops the aggregate and a computed `Total cost` Record from
  becoming two overlapping numbers on one screen.

### Capture, correction, void

- [x] **`CaptureEntry { target, amount, concept, note, at }`** — one Action, not
  two. A form that first made you choose which total to affect reintroduces the
  bookkeeping this design removes. The amount is exact decimal *text* parsed
  straight into a decimal, so `-10.50` never becomes a float on the way to a
  signed Fact. `at` accepts a backdated instant.
- [x] **`ClassifyFact { fact, concept, note }`** re-asserts what a recorded
  movement was. It never touches the Fact: nothing moved, only our account of what it meant. Concepts resolve through the DAG, so `@food`
  classifies a movement a later `@cost` query finds.
- [x] **`ClassifyRecord` / `UnclassifyRecord`** for a Record's additional
  concepts. `UnclassifyRecord` refuses to remove the **identity** concept —
  Transfer matching and sync resolve through it, so removing it would look like
  a tag edit and behave like a deletion. Both append a zero-delta annotation
  Fact, which is also what makes live subscriptions refresh.
- [x] **`entry` is where a typo goes.** A captured movement was a signed Fact
  plus a classification assertion, neither of which can change, so "I typed 15
  instead of 150" had nowhere to live. `capture-entry` returns its entry uid;
  `revise-entry` and `void-entry` edit it.
- [x] **The entry does not store the concept.** `fact_concept` owns it; a second
  copy would disagree the instant a Fact was re-tagged. Readers join through
  `fact_uid`. So `revise` handles amount, instant and note only — changing *what
  a movement was* is `classify-fact`.
- [x] **The replay guard runs before any Ledger work.** `append` commits its own
  transaction, so a replay caught at insert time would already have appended a
  duplicate compensating Fact — the amount handed back twice, with an error
  afterwards that cannot put it back. All three Actions check
  `entries::replayed` as their first statement. The `request_id` UNIQUE index is
  the backstop, not the mechanism.
- [x] **Corrections carry their classification onto both new Facts.** Otherwise
  fixing an amount silently drops the movement out of its category and leaves
  the category showing the original wrong number. Voiding does the same.
- [x] **A note-only edit appends nothing** but bumps the revision and writes an
  audit row, and the entry keeps pointing at the Fact carrying its amount.
  "Did the amount move" is compared *numerically* — `DecimalValue` equality
  includes scale, so re-typing `-15` as `-15.00` would otherwise append a
  compensating pair netting to zero.
- [x] **Voiding leaves `fact_uid` pointing at the compensated Fact**, because
  that is still the movement the entry describes. The compensation is a separate
  Ledger entry, not a replacement.
- [x] **Voiding retracts from the period that claimed it; a refund is a new
  capture today.** Voiding means the change never happened, so it nets the
  original period to zero. A purchase that really happened and was later
  refunded is a different thing. Conflating them would either rewrite a closed
  month or leave a phantom in it.
- [x] **Backdating a correction does not rewrite history.** The compensation
  lands at the *original* instant and the replacement at the new one, so the old
  month keeps both halves and nets to zero while the new month carries the
  movement. A month total that changed retroactively with no trace would be
  indistinguishable from a bug.
- [x] **Generic `compensate` is refused on a Fact owned by an entry**, the same
  guard Transfer settlements use. Otherwise the amount comes back while the entry
  still reads `applied`.
- [ ] **Crash atomicity is not met, and this is known.** The engine appends Facts
  and *then* records the sidecar, following the Transfer-settlement precedent,
  because only the engine can seal and sign a Fact. A failure between the two
  leaves a real Fact with no entry — readable as an unmanaged movement, but not
  atomic. Closing it needs `append` to accept a caller's transaction, which is a
  change to the write path.

### Recurring entries

- [x] **Occurrences are derived, never materialized (E1).** Due dates are a pure
  function of cadence and anchor, so storing them creates a second copy of a
  derivable fact plus a cursor to keep in sync. Tables are `recurrence` /
  `recurrence_revision` / `recurrence_skip`.
- [x] **Only two things cannot be derived, and only one needed a table.**
  *Applied* is an entry whose `request_id` is `<recurrence_uid>:<due_at>` —
  `entry_revision.request_id` was already UNIQUE, so applying a date twice is
  impossible with no new state at all, and a retry returns the first entry
  rather than moving the quantity again. *Skipped* needs `recurrence_skip`,
  because "declined" and "nobody has looked yet" must not read the same.
- [x] **Applying routes through the ordinary `capture-entry` path**, so a
  rule-applied change is indistinguishable from a hand-typed one afterwards and
  is revisable and voidable like any other entry.
- [x] **A date the rule does not produce is refused**, so "apply" cannot
  degenerate into a capture wearing a rule's name.
- [x] **Revising a rule keeps its anchor by default.** Silently re-anchoring to
  now would shift every future date of a rule whose author only changed the
  amount.
- [x] **One occurrence may be applied at a different amount** — the bill that
  came in higher — without editing the rule. The derived occurrence then reports
  what actually moved rather than the rule's standing figure.
- [x] **Pausing hides the future and keeps the past.** A paused rule still
  explains the entries it already produced.
- [x] **Signed authorship is not forwarded** from `apply-recurrence-occurrence`
  into the nested capture. That evidence attested applying an occurrence, not
  capturing an entry, and forwarding it would let one signature stand for an
  action shape its signer never saw.
- [x] **Downtime catch-up needs no worker, because occurrences are derived.**
  A date nobody answered while the app was closed is not lost state to replay —
  it is recomputed from the cadence and reads as `due` the moment someone looks.
  Close the app for a week and the week's dates are waiting.
- [x] **The inbox window is finite, and now says so.** It looks 60 days back; a
  rule ignored longer than that loses its oldest dates from the list without
  being applied *or* skipped. Nothing is corrupted — they stay derivable — but a
  surface that quietly stops offering them reads as an obligation that resolved
  itself. **The sand now names its own lookback instead of inheriting Protein's
  default**, because a list cannot describe a window it did not choose, and says
  "dates before X are not listed" whenever something past is still unanswered.
  - This is the third way this list can be a prefix, after a rule that repeats
    faster than the page can hold and a page showing the first N of M. All three
    share one line, because they are one question: *is this everything?*
---

## 9. Past, present and declared future

Why: the point of setting rules with frequencies is to answer "what happens to
my savings over five years" without waiting sixty months. This is the loop from
blocks 2–8 run on a virtual clock against a virtual Ledger.

### The timeline query

- [x] **`Source::Timeline` — one classified quantity axis through time**:
  settled past, the position now, and the declared future in a single query. One
  source rather than three because stitching them client-side means **adding
  exact decimals in JavaScript**, and the running cumulative is precisely the
  number that must not be computed there. Every value leaves as exact decimal
  text, including the cumulative.
- [x] **The future is declared, never invented** — derived recurrence dates plus
  outstanding promises. Nothing extrapolates from the past; nothing writes a
  projected Fact.
- [x] **An applied date is counted once, as history**, and excluded from the
  expected half. Otherwise every rule-driven month reads as twice its cost.
- [x] **The line opens at `opening`** (everything the concept did before the
  window), so a cumulative never restarts at zero and draws a position nobody
  was ever in.
- [x] **Two units under one concept are two lines, never one sum.**
- [x] **Every declared point drills to the rule or promise that produced it.** A
  number on a chart is never one nobody can explain.
- [x] **Points carry their `origin`.** Promise deltas are `REAL` in schema 0001
  and converted on the way out, so a promise-derived point is only as exact as
  that column ever was — a reader is never left guessing which kind it holds.

### Folding programs forward (E0.4)

Why separate from the timeline: the timeline folds *declared* amounts. This
folds *computed* ones — rules whose output depends on other rules.

- [ ] **Project by substituting ports, never by writing a second evaluator.**
  `evaluate_program` is already pure over `(ProgramAst, FrozenEvaluationContext,
  EvaluationLimits)` — no clock, no store. Only two things around it are
  production-bound: the store-backed boundary resolver (block 2) and the intent
  applier (block 6). Projection replaces the first with a virtual quantity map
  and the second with a virtual fold. A rule that is wrong in projection is
  wrong in production, which is the entire value of the property.
- [ ] **Enumerate future occurrences from the same schedule machinery**, walking
  the cursor forward to a horizon instead of to the present. Frozen timezone and
  tzdb rules apply unchanged, so a projection crossing DST lands where execution
  would have landed.
- [ ] **Fold both binding modes in one timeline.** A **computed** Record
  (`Monthly expenses = Rent + Utilities + Groceries`) is re-derived at every
  projected step from that step's projected inputs, never carried forward as a
  constant. A **materialized** Record (savings drawn down monthly) is folded
  occurrence by occurrence. They compose: raising projected rent in month 30
  changes projected expenses in month 30, which changes the projected draw on
  savings from month 30 on. A projector that re-derives only at the start
  produces a plausible curve that is wrong everywhere after the first change.
- [ ] **Fold classified promises alongside program occurrences.** "In three
  months I expect +300 from a sale" is a promise — `record_uid`, `delta`,
  `window_end`, `promise.concept_uid` — so a future annotation classifies
  through the same DAG as a past Fact. `build_snapshot` already folds these but
  only in `Agreed`/`Active`, and a solo expectation created today starts at
  `Proposed`. Needs a one-step path for an expectation with no counterparty —
  not a new primitive, and not asking someone to agree with themselves.
- [ ] **Return exact points, not floats**, carrying decimals and units end to
  end, each labelled actual or projected and naming its cause: program revision
  hash, occurrence identity, and the intent shape it would have staged.
- [ ] **Exclusions are reported, never silent.** The legacy fold in
  `nucleus::imagination` quietly skips rules needing signals or sums, so a
  timeline can be confidently wrong. Any program that cannot be folded — an
  unresolvable input, a non-local capability, an external effect — appears in a
  named exclusion list attached to the result.
- [ ] **Projection writes nothing:** no Fact, intent, candidate, cursor advance
  or grant consumption. Branching is mutating the snapshot — change a starting
  quantity, toggle a program, alter a rate — and folding again. Comparing two
  timelines is the compare view. Applying anything is an ordinary reviewed
  Action.
- [ ] **Bound it.** Five years of a monthly Frequency is sixty evaluations; a
  daily one over the same horizon is not, and a program graph can be large.
  Horizon, total occurrence count and total fuel are explicit limits, and
  exhausting one truncates with a stated reason rather than hanging.
- [ ] **Carry the legacy capability across before it goes dark.**
  `Engine::project` folds `registry.rules` today and powers `crossings_pass`.
  Once rules are imported to Karma that fold's input goes empty and the existing
  five-year projection silently stops working. `crossings_pass` moves onto this.

**Exit:** a Rent Record and several cost Records feed a derived monthly expenses
rule; a Frequency-driven program draws that from savings each month; projecting
five years returns sixty exact points whose final value matches a hand-computed
decimal exactly. Raising rent and re-folding changes the curve. The timeline is
identical under a DST-crossing timezone and under a virtual clock started at any
instant. Nothing is written. Running the same program for real across the same
window produces the same numbers — because it is the same code.
---

## 10. The Karma sand

Why: this is the point of everything above — a person opens a browser and does
full CRUD over the rules that change their Records, enters individual changes by
hand, and sees the graph of their Records through past, present and declared
future. It is the **integration proof** that the loop works, not a new layer.

**There is no Economy sand and there will not be one.** Economy is what this
surface looks like when the rules in it are about a balance, exactly as a pantry
is what it looks like when they are about flour — a preset of Records and
concepts shipped as data in someone's own Cell. Viewing past/present/future,
CRUDing rules, and entering individual changes is what the Karma sand does for
*any* domain. The surface was renamed twice for the same mistake (Finance →
Economy → Karma): naming it after the first thing people did with it.

**A sand is a bundle of HTML and JavaScript that reads through Protein and
writes through Actions.** Never a layer, never a phase, never something the
backend has heard of.

- [x] **Enforced, not asserted.** `crates/web/tests/sand_boundary.rs` fails if
  `economy`, `money`, `currency` or `finance` appears as a module, type, table,
  column or literal in `nucleus`, `store`, `engine`, `protein`, `transport` or
  `lince`. Comments explaining *why* a domain name was rejected are exempt. The
  list is more than one word because the pressure that produced `store::economy`
  is the pressure that produced a `Money` type beside an identical `Quantity`.
- [x] **Every test in nucleus, store, engine, protein and lince-web passes**
  (500, 2026-07-31). The senses and transfer failures carried as "pre-existing"
  for weeks were three test setups missing a Person and one assertion comparing
  a normalised permission list as an ordered vector. Neither was a product
  defect, and both were cheap once looked at rather than routed around.
- [x] **Shipped 2026-07-26**, renamed `sand.karma` on 2026-07-28. One-line
  capture, correction/void/re-tag, recurring rules with a compound cadence and
  an apply/skip inbox, and a concept timeline spanning settled past, current
  position and declared future. It reads `Source::Entry`, `Source::Recurrence`
  and `Source::Timeline` — there is no `source:"economy"`.
- [ ] **Still open:** monthly dashboards, source profiles, Fiote capture review,
  and the driven chromium selftest.

Full surface specification, gates and information architecture:
**`docs/Sand: Karma.md`**.

### The lenses

One sand, several lenses over the same Protein and Actions — not separate tools
with private state.

| Lens | Job |
| --- | --- |
| **Library** | Programs/templates, active/paused/faulted state, owner, purpose, next occurrence, latest result |
| **Builder** | Form/graph/DSL synchronized editor, typed ports, parameters, Proof, revision diff |
| **Why** | Causal run trace, substituted values, evidence, model output, policy/grant, intent/receipt, resulting Facts |
| **Learn** | Pattern hypotheses, admitted/rejected evidence, probability/confidence/cadence, drift, thresholds, feedback |
| **Imagine** | Replay/project/branch/DST, invariants, future timeline, plan comparison, apply-as-Actions preview |
| **Authority** | Grants plus Trust scopes: concept/stage, People/Organs/proximity selectors, thresholds, budgets, expiry, capability matrix, revoke/narrow |
| **Queue** | Occurrences, runs, workflows, candidates, decisions, staged/retrying/uncertain/dead intents |
| **Health** | Sequencer/connector/device/model lag, failures, replay audits, engine mode and emergency controls |

- [ ] **The Builder starts with ordinary-language templates and forms, not a
  blank programming screen.** "Recurring task" asks *what, when,
  missed-occurrence policy, route*. "Inventory threshold" asks *record/concept,
  unit, threshold/hysteresis, forecast horizon, and suggest/draft/ask/act*.
  Switching to graph or DSL shows exactly what the form generated.
- [ ] **Do not put durable policy, schedule math, model updates, authority or
  effect retry logic in JavaScript.**
- [ ] **A command palette as operational sugar that never bypasses Actions:**
  `i new`, `i edit @x`, `i on/off @x`, `i run @x`, `i sim @x +30d`,
  `i why run:r_…`, `i tune @reminder interval=3d`, `i grant @x` (never "grant
  all" silently), `i trust @x`, `i revoke grant:@y` (preview affected queued
  work first), `i queue` / `i learn` / `i health`, `i stop effects`. Every
  compact command expands to a readable confirmation and diff when it changes
  authority, social state, external state, an active revision, or more than its
  declared low-risk local scope.

### The Flow Plane

Built after the rule-CRUD surface is proven. It is the sand's map view, not a
second sand: **one control room, not a programming language exam.**

- [ ] **A two-dimensional zoomable map of everything a Cell can observe and
  everything Karma could cause.** Overview all programs, filter/group by type,
  owner, scope, state or tag, and zoom from the whole dependency graph into one
  node's configuration, evidence lineage, model, authority, run history,
  workflow instances and effect health.
- [ ] **Enumerate every declared and currently reachable source port** —
  Records/Facts, saved and inline Protein, parameters, Frequencies, manual
  occurrences, sync arrivals, decisions, workflow wakes, model/forecast outputs,
  Signals, APIs, files, processes, devices, microcontrollers. **A source that is
  configured but unavailable stays visible** with freshness, visibility,
  capability, connector health and last-evidence state. Absence must not make a
  dependency disappear from the operator's mental model.
- [ ] **Enumerate every possible outcome path before it happens** — derived
  values, emitted Facts, recommendations, drafts, decisions, workflow
  transitions, meta-control, typed effect/Transfer Actions. **Inactive, denied,
  staged, budget-exhausted, missing-secret, untrusted-Organ or otherwise blocked
  paths stay drawn and name the exact gate.** This is how a person audits "all
  possible effects" without granting them or waiting for a live run.
- [ ] **Default layout: time left to right, stable causal/resource lanes top to
  bottom.** A source observation, schedule boundary or state transition is a
  point; a freshness window, threshold band, hysteresis band, allowed range,
  schedule tolerance, wait or Trust validity is a range. Crossing, entering or
  leaving a range visibly routes a token to the next typed node. **Layout is
  presentation metadata and never changes graph semantics or a revision hash.**
- [ ] **Five composable views.** *Definition:* the complete static graph
  including dormant branches. *Live:* latest values, occurrence order, evaluated
  edges, queue state, receipts. *Why:* walk either direction through exact
  evidence and authority provenance. *Imagine:* the production kernel on a
  frozen or branched world, visually separating projected changes from Facts.
  *Authority:* taint, recipient, grant, Trust scope, budget, expiry, and the
  first gate that would require escalation. **No overlay computes policy or
  schedule truth in JavaScript.**
- [ ] **Editing produces a typed graph-revision draft**, runs validation and
  Proof, then invokes the ordinary revise Action with an expected revision and
  idempotency key. Dragging nodes writes only personal layout state. A
  breakpoint, run-once, simulation, activation, pause, candidate response, grant
  change, retry or compensation likewise invokes its typed Action — **the canvas
  never writes Store rows or dispatches an effect directly.**
- [ ] **Dry-run one node or a whole program** against current or simulated
  input: animate the evaluated path, show each substituted value, gate and
  carry, preview writes and external effects, compare active against candidate
  output, and allow breakpoints before an effect.
- [ ] **Surface loop/conflict/authority Proof on edit and save.** Compare
  revisions, publish or roll back by selecting the active revision, pause
  immediately, inspect queued/running/dead effects, retry safely, compensate
  reversible Actions.
- [ ] **Make data scope and authority visible on the graph** — taint paths,
  hidden/missing inputs, declassifications, grant boundaries, remaining budgets,
  recipients, values, expiry, and the exact node that first requires escalation.
  **Activation never bundles an unread permission dialog into a generic
  "enable".**
- [ ] **Give learning its own inspectable surface:** hypotheses,
  eligible/rejected evidence, probability vs confidence, cadence,
  thresholds/hysteresis, checkpoints, validation/calibration, drift,
  recommendation feedback, and a "forget/rebuild from allowed evidence"
  operation.
- [ ] **Give operations a queue/run surface:** occurrence lag, sequencer status,
  paused/faulted programs, nonterminal workflows, staged/leased/retrying/
  uncertain/dead intents, connector and device health, budgets, replay capsule
  export. **Never require filesystem log access for normal recovery.**
- [ ] **Make every "why" navigable in both directions:** changed Fact →
  occurrence → run → node/evidence/model → candidate → grant/policy →
  intent/receipt → resulting Fact, and a result back to every program that
  consumed it.
- [ ] **Global and scoped controls** for normal / stage-effects / observe-only /
  emergency-stop, pause/resume, cancel, retry, compensate, mute, revoke.
  Controls show what happens to already queued, leased, dispatched and waiting
  work **before** confirmation.
- [ ] **Installable templates are ordinary disabled program graphs:** habit,
  recurring task, inventory threshold, birthday reminder, recurring Transfer
  draft/negotiation, call intent, sensor/actuator loop, optimizer, monthly
  recap, command flow. **Installation grants no data scope, secret, budget,
  connector, controller or authority** until the person reviews and binds them.
- [ ] **Store canvas layout and personal display preferences as host state**
  while program semantics, parameters, scopes, grants and revision selection
  stay Cell data. Rearranging nodes must not create a semantic revision.
- [ ] **Large graphs** use server-projected dependency slices, stable node/edge
  ids, viewport virtualization, semantic zoom and incremental live overlays. The
  client may cache geometry but retains causal data only to its Protein cursor
  boundary.
- [ ] **Split the sand into focused Rust `body/style/script` modules and focused
  JS modules** for bridge, plane, layout, inspectors, each lens and
  accessibility. Do not create one monolithic Karma HTML/script.
- [ ] **All authoring, trace, simulation and emergency controls are keyboard and
  screen-reader reachable.** Color and animation never carry the only
  explanation of state, confidence, authority or failure.

### Control contract — anything a human can do, an agent can do

- [ ] **Every control in the sand is also a typed Action, and every durable
  result is readable through Protein.** That is how a human, CLI, sand, script
  or authorized agent controls **every** Karma feature without database access
  or a private backdoor.
- [ ] **Protein exposes capability booleans and stable blocking reasons** beside
  each program, revision, candidate, grant, decision, workflow and intent.
  Interfaces render those capabilities; **they do not duplicate the permission
  calculation.**
- [ ] **An agent reads only explicitly granted Protein scopes**, proposes the
  same inert candidates, and invokes the same Actions as a human tool.
  Model reasoning may be opaque; the candidate diff, Proof, policy, principal
  and resulting effects stay exact.
- [ ] **Editing by an agent never activates by implication.** A grant may
  separately allow activation of proven revisions matching an exact
  template/scope and shadow criteria; otherwise activation is a durable human
  decision.
- [ ] **A meta-program may tune, pause, resume or select revisions of named
  programs only under `karma.manage`** with field/range/state limits. It cannot
  edit its own grant, change owner, bind secrets/connectors, waive Proof,
  broaden visibility, or suppress its audit trail.
- [ ] **Export/import uses content-hashed revision/template packages** with
  schema, Lingua dependencies, extension hashes, license/notices and signatures.
  Evidence, secrets, grants, model checkpoints and live state are excluded
  unless independently and explicitly selected.

**Exit:** through the real socket, the sand renders every source and potential
effect for the acceptance fixture; a person authors the old condition →
consequence example as points and ranges, sees a live transition create only its
permitted candidate, replays it in Imagine, inspects its full Why and Authority
paths, and revises or pauses it without direct database access. Blocked and
dormant paths stay inspectable. Keyboard/screen-reader navigation, live updates,
stale edits, permissions, emergency controls, restart recovery and a large
virtualized fixture all pass.
---

## 11. Time, executing

Why here and not block 1: a declaration needs no permission. *Firing* needs a
grant, a worker and a run identity, so it comes after authority. Same one
`Cadence` from block 1 — this adds only what exists because something will fire:
which zone the wall clock belongs to, what a DST gap or fold means, and what to
do about a wake-up that was missed.

### Civil resolution (the zone half)

- [x] **`CalendarSchedule` = one `Cadence` + timezone + pinned tzdb revision +
  gap/fold/timer/missed/inactive/rephase/overload policy.** It carries no time
  arithmetic of its own. Both this and the provider-free UTC path call one
  generator, `Cadence::civil_at`, so a rule cannot mean one thing on screen and
  another in the runtime.
- [x] **`TimeZoneProvider` is a pure injected boundary.** It advertises one
  `TzdbRevision` and resolves `(timezone, CivilDateTime)` to exactly one
  instant, a gap with its first valid instant after the gap, or a fold with two
  increasing instants. Resolution fails closed if the provider's version/hash
  differs from the schedule's. Host timezone, current tzdb package, locale and
  wall clock are never implicit inputs. Production loads a content-addressed
  provider; replay loads the revision named by its capsule.
- [x] **The production artifact is canonical `karma.tzdb-artifact.v1` JSON** —
  declared release version plus a sorted timezone map; each zone holds
  contiguous half-open UTC offset segments covering the whole representable
  timeline, first segment unbounded below, last unbounded above. Bounded to
  64 MiB, 4,096 zones, 100,000 segments per zone. Rejects offsets beyond 24
  hours, UTC gaps/overlaps, noncanonical bytes, and any artifact mapping one
  local instant to more than two UTC instants. Revision digest is the
  domain-separated hash of the canonical semantic artifact, never a filename or
  host tzdb version string. Engine checks file size before allocation, detects
  size change during the read, and accepts only on exact version+digest match.
- [x] **`GapPolicy` = `skip | shift-forward | pause`; `FoldPolicy` = `first |
  second | both | pause`.** Every `CalendarBoundary` keeps the requested local
  time, the actual UTC `intended_at`, and its resolution kind. With `both` the
  two fold instants are separate stable boundaries for the same civil time; with
  `shift-forward` the trace shows the UTC instant was shifted. Skipped gaps and
  invalid month dates are explicit generation outcomes, not missing history.
- [x] **A deterministic search budget rejects a broken or malicious provider**
  reporting an unbounded run of gaps.
- [x] **Demand attestation is conservative.** Derived from exact daily/weekly
  lower spacing or a 28-day-per-month lower bound, minus the zone's complete
  offset spread; `fold both` also admits the shortest backward-transition width.
  May over-reserve, never hides a faster possible occurrence.
- [ ] **Wire the recurrence read path to a provider** so a rule that must land on
  a local wall clock across a DST transition can. Today that path is
  provider-free UTC, where DST does not apply.

### The elapsed lattice

> **Superseded 2026-07-30 — this is not a second type.** `CadenceStep` already
> carries milliseconds, so a zoneless millisecond schedule is a `Cadence` with a
> millisecond step. `ElapsedSchedule` is deleted and the arithmetic below moves
> onto `Cadence`, keeping every property it proves. The bullets stay because
> each one is a rule the merged generator must still obey.

- [x] **`ElapsedSchedule`** — `interval_ms >= 1`, an exact anchor, and a
  `TimerPolicy` where resolution is at least `1ms` and coalescing never exceeds
  maximum lateness. Boundaries come from integer division from the anchor; it
  **never adds an interval to observed wake time**. Activation at the anchor
  first schedules `anchor + interval`; activation before the anchor schedules
  the anchor.
- [x] **`ScheduleCursor`** stores the last consumed intended boundary and the
  exact next one, verifying both lie on the schedule lattice. A cursor cannot
  adopt a wall-clock instant or a boundary from another revision.
- [x] **`OccurrenceRange(first, interval, count)`** is the reconstructible
  semantic batch; `ScheduleAdvance` records due range, emitted range, skipped
  range, late count/max lateness, next cursor, and whether to pause. Count is
  never zero; range arithmetic is checked for timestamp/`u64` overflow.
- [x] **`RationalRate` stores reduced integer numerator/denominator**, so `3ms`
  is exactly `1000/3` ticks per second and `5h` exactly `1/18000`. Admission
  never uses a rounded float or an arbitrary fast/slow threshold.
- [x] **Rephase returns a new immutable schedule plus cursor**, never
  reinterpreting a consumed occurrence. `preserve_anchor` takes the first
  new-lattice boundary strictly after the change; `from_last_intended` anchors at
  the last consumed boundary; `from_change` anchors at the change and first fires
  one interval later; `immediate_if_overdue` keeps the already-due boundary as
  the new anchor, otherwise behaving as preserve-anchor.

### Missed-occurrence policy

- [x] **Four policies, exactly specified.** `skip` emits only the newest due
  boundary if it is still inside `max_lateness`, recording every older one as
  skipped; if even the newest is late, all are skipped. `coalesce` emits one
  occurrence carrying the complete range. `replay(max=N)` emits the oldest
  `min(N,count)` ticks in order and records the remaining suffix as
  overflow/skipped. `pause_on_lag` advances nothing when any due tick exceeds
  max lateness, so an operator can inspect the unchanged cursor before choosing
  recovery.
- [x] **`intended_at` is never rewritten by lateness.** A schedule records
  `intended_at`, `eligible_at`, `observed_at` and lateness; a late worker runs
  the missed policy rather than moving the intended time.
- [ ] **External, device and social effects normally forbid unbounded replay.** A
  missed 1ms computation may be replayable; a missed motor command,
  notification, or Transfer proposal is not repeated thousands of times without
  an exact explicit policy.
- [x] **No-consumer time is not scheduler failure.** A Frequency revision
  declares `inactive_gap skip_to_next_anchor` (default) or a bounded
  `replay_by_missed_policy`. On the zero-to-one consumer transition the same
  transaction computes the next cursor from the anchor and that policy, so
  reactivation cannot replay months of dormant work.

### The tickless deadline fabric

Why: the naive version turns the shortest active Frequency into a global polling
interval. A `3ms` sensor must not make the monthly report get checked 333 times
a second.

- [x] **Do not** turn the shortest interval into a poll loop, generate Rust code,
  busy-loop, or spawn one Tokio task per Frequency. Every timed object owns one
  calculated `next_due_at`; the runtime arms one-shot timers and receives only
  registrations that became due. Firing a `3ms` Frequency computes and registers
  its next `3ms` boundary and does not ask whether the `5h`, daily or monthly
  ones are ready. There is never a global `check_every = min(active_intervals)`.
- [x] **Three layers, kept as different Rust types.** `DeadlineKey` (kind,
  target uid, generation, exact `due_at_ms`, stable priority) is semantic.
  `ScheduleDemand` (rational rates, resolution, lateness, CPU/fuel/write/effect
  upper bounds) drives admission. `DeadlineLanePlan` (lane uid, generation,
  stable-sorted members, reserved rates, next arm) is replaceable host state and
  **may never appear in a revision hash or Action precondition**.
- [x] **Bucket width never rounds the runtime arm.** Distance-to-deadline picks
  an index level, but every entry and lane summary retains the exact minimum
  `due_at_ms`. Occupancy bitmaps skip empty buckets; there is no periodic sweep.
  Far registrations are reindexed only when an already-required wake reaches
  their horizon, and reindexing never creates an earlier wake by itself.
  Buckets are lookup acceleration, not permission to round semantic time.
- [x] **Lanes are timer/index partitions, not threads.** The wheel is logically
  partitioned by lane so a dense path never traverses a sparse lane to compute
  its own arm. The production adapter may use dormant Tokio `Sleep`, a runtime
  timer wheel, or `timerfd` behind one poller. A dormant five-hour timer costs
  bounded metadata, no thread, no repeated CPU.
- [x] **Lane planning is measured, not named.** Start with one sparse lane. The
  planner splits out a dense stream when its admitted utilization would consume
  the sparse lane's capacity or lateness reserve, packing demand in stable order
  `(required_resolution, utilization desc, target_uid)`, and merges lanes again
  when demand disappears. No fixed cadence classes, no `1s/1d` cutoff. Lane
  assignment is operational, recorded for diagnostics, and not part of the
  semantic hash or replay result.
- [x] **The ready set is level-triggered by `lane_uid`**, not an unbounded FIFO.
  Repeated expiry of one dense lane leaves one ready bit plus its latest
  observed cutoff, so 333 fast wake messages cannot sit ahead of a sparse token.
- [x] **The director does no graph evaluation.** In one bounded pass it converts
  each ready cursor into a durable occurrence or arithmetic range batch and
  hands work to the sequencer.
- [x] **Runtime wake order is never semantic order.** On ready, snapshot
  `observed_now`, drain all ready tokens without blocking, ask only those lanes
  for keys due by that cutoff, and sort by
  `due_at_ms → deadline_kind_priority → target_uid → generation`.
  `deadline_kind_priority` is a versioned numeric enum, never map iteration
  order: `0` authority/trust/grant expiry, `10` promise/invitation/decision
  expiry, `20` workflow wake, `30` Frequency occurrence, `40` Signal poll, `50`
  effect lease/retry, `90` maintenance. A deadline created by work at that
  cursor is appended after the current ordered set with a monotonically
  increasing sequence and cannot jump backward into an already-processed
  priority. **Changing this order is a replay-breaking semantic version change**
  requiring a new simulation epoch, never an incidental refactor.
- [x] **Generation discards stale keys.** A tune or deactivate atomically
  increments the target generation, stores the new `next_due_at`, and notifies
  the directory. Boot loads active indexed rows once and rebuilds the wheel;
  normal firing never scans every Frequency or every source table.
- [x] **`karma_deadline` is a materialized index, not new truth.** Columns:
  `deadline_kind, target_uid, generation, due_at_ms, required_resolution_ms,
  max_lateness_ms, coalesce_key, stable_priority, active`, unique on
  `(deadline_kind, target_uid)` plus a due-time index. Creating or changing a
  promise expiry, decision expiry, Signal poll, workflow wait, effect retry or
  Frequency updates its registration in the same semantic transaction. The
  subsystem's typed state stays authoritative. A coarse maintenance deadline may
  audit/rebuild the index; normal firing does not poll source tables.
- [x] **`karma_frequency_consumer`** (`frequency_uid, consumer_kind,
  consumer_uid, consumer_revision_uid, effective_from_cursor`, unique across the
  tuple) tracks who references a reusable Frequency. Zero-to-one upserts the
  deadline; one-to-zero removes it and increments generation. Ten consumers
  share one cursor, one deadline, one occurrence fanned out deterministically.
  This avoids firing unused Frequencies without recounting consumers every wake.
- [x] **"Not unnoticed" is exact:** every intended boundary becomes exactly one
  durable occurrence identity/range or an explicit `skipped/coalesced/paused`
  record. The deadline row stays active until the same transaction advances its
  cursor and stores that evidence, so an OS wake followed by a crash cannot
  consume it.
- [x] **`must_finish_before_next`** — a Program needing evaluation before its
  next boundary declares it, and activation then needs a conservative worst-case
  evaluator reservation, not merely enough timer wakes.

**The runtime port exposes registrations, not a period:**

    struct DeadlineArm { lane_uid, lane_generation, wake_at_ms,
                         required_resolution_ms }
    trait DeadlinePort {
        fn replace_arm(&self, arm: DeadlineArm);
        fn disarm(&self, lane_uid: LaneUid, generation: u64);
        async fn next_ready(&self) -> DeadlineWake;
    }
    enum DeadlineWake { LaneReady { lane_uid, generation, observed_at_ms },
                        DirectoryChanged, ClockDiscontinuity, Shutdown }

Production converts stored UTC targets to a monotonic one-shot sleep and emits
`ClockDiscontinuity` when wall-clock mapping changes, after which calendar
schedules recalculate from their frozen tzdb rules. Simulation registers the
same arms and advances virtual time to the next one. Schedule math stays pure.

### Dense occurrence batching

Why: a `1ms` Frequency is up to 1,000 semantic ticks per second. One schedule row
and full trace per no-op tick makes storage overhead the feature.

- [x] **`OccurrenceBatch`** = `activation_hash, batch_sequence, emission
  (individual|coalesced), first_schedule_ordinal, range.{first, interval_ms,
  count}`.
- [x] **Tick identity is derived, not allocated:** `(activation_hash,
  schedule_ordinal, intended_at)`. The activation hash commits the Frequency
  uid, immutable revision, effective parameter map, activation generation/cause
  and schedule. The ordinal is relative to the frozen anchor, **not** to a host
  wake batch, so two different wake segmentations reproduce the same identities.
- [x] **Bounded pages of at most 4,096 ticks**, processed in ordinal order, with
  the page cursor as durable sequencer state rather than an unbounded range
  allocation.
- [x] **Batching is storage representation, not coalescing semantics.** A
  Program asking for every tick still gets every tick unless its declared
  missed/overload policy says otherwise. A coalesced batch exposes exactly one
  aggregate occurrence and cannot be expanded through the individual-tick API.
- [x] **Any tick producing a candidate, intent, Fact, failure, bookmark, state
  transition or sampled trace keeps its individual run.** Consecutive no-op ticks
  may share a compact trace summary because the batch reconstructs them exactly.
- [x] **The durable row stores the complete canonical envelope**, not a loose
  advance fragment. SQL columns project cadence, emission kind, first/last
  boundary, covered boundary count and semantic count; reload recomputes and
  compares every projection and the content hash. Five overdue 3ms boundaries
  occupy one row and remain exactly reconstructible as five identities.
- [x] **An on-time tick is never speculatively grouped with a future boundary** —
  future work may still be paused or revised before it becomes due.

### Admission — you cannot activate what the Cell cannot serve

- [x] **Publishing computes a conservative fixed-point `ScheduleDemand`:**
  semantic ticks/s, timer wakes/s, scheduler CPU ns/s, node evaluations/s,
  writes/s, effects/s, trace bytes/s. Exact rational internally; the displayed
  decimal is never used for admission. Ticks and wakes differ only where an
  explicit reconstructible batch or coalescing window permits it, and batching
  never lowers the semantic estimate unless the Program declares coalesced
  semantics.
- [x] **Admission uses the safe upper bound.** Conditional gates may lower the
  *displayed* estimate; only a compiler-proven tighter bound lowers the admitted
  one. A calendar rule derives its rate from the **shortest possible** interval
  over the declared tzdb horizon, not the average. An unbounded or unprovable
  rule is rejected until the owner supplies an enforceable rate cap. Missing
  provider attestation is `calendar_rate_unproven`, never an average fallback.
- [x] **Stable capacity failure order** — semantic ticks → timer wakes →
  scheduler CPU → evaluator fuel → writes → effects → trace bytes — so the same
  snapshot always explains the same first denial.
- [x] **Every durable cursor stores the demand snapshot admitted with its
  activation**, and loading recomputes it from the frozen schedule, workload,
  calibration and attestation before trusting the projection.
- [x] **Gates are demand-based, never interval bands.** Baseline: fits ordinary
  budgets. **Dense** (`karma.schedule.dense`): projected utilization exceeds the
  ordinary budget or needs lane isolation. **Precision**
  (`karma.schedule.precision`): requested resolution/lateness finer than the
  Cell's measured normal timer service, regardless of cadence. **Effect-heavy**:
  upper-bound materialization/external-effect rate exceeds its capability
  budget. A `3ms` no-op recognizer and a five-hour workflow with ten thousand
  effects hit different gates for different reasons. `1ms` is not a magic branch.
- [x] **Hard aggregate Cell ceilings independent of individual grants.**
  Activation is rejected or held shadow-only if the sum of active upper bounds
  exceeds them. Runtime meters actual lateness, evaluations, CPU/fuel, writes,
  effects and trace bytes; on overrun the predeclared policy is `coalesce`,
  `drop_with_evidence`, `stage_effects` or `pause_and_ask` — never silently
  changing the interval or omitting ticks.
- [ ] **Protein exposes the operational picture:** lane count/kind, membership
  and reason, each lane's next arm, wheel/overflow occupancy,
  requested/effective resolution and lateness, estimated vs actual
  wake/semantic/evaluation/write/effect rates, lateness percentiles,
  batch/skipped/coalesced counts, budget use, pending split/merge.
- [ ] **The sand warns at authoring and proves at activation**, offering shadow
  load testing with effects disabled. A person choosing `1ms` sees 1,000
  ticks/second and 86,400,000 ticks/day plus retention and effect implications
  before granting it.
- [ ] **A paused dense lane is disarmed and retired/merged without touching
  sparse arms.** A Cell with only a `5h` Frequency does no Frequency work between
  activation and its exact next one-shot deadline.

**Honest limit:** Lince cannot promise hard real-time actuation from a
general-purpose host. If a motor or interlock genuinely needs a 3ms closed loop,
Karma deploys a versioned bounded controller to a capable microcontroller and
treats configuration and telemetry as typed effects and Signals; the device
enforces the loop locally. The Cell can still reason about, simulate, authorize
and audit that controller without pretending network and OS latency are
real-time. Ordinary Lince never busy-spins; an explicit best-effort real-time
grant may give one dense lane a dedicated thread or short final spin.

**Exit:** a Cell with `3ms`, `5h` and monthly Frequencies arms each from its own
next deadline. The `3ms` path never scans or re-arms the others; sparse
deadlines still fire while the dense stream runs. Adding or removing dense
demand creates and retires only its runtime lane. Restart, catch-up,
same-millisecond cursor order, generation invalidation and schedule batching all
replay exactly under DST.
---

## 12. External, device and interface effects

Why: block 6 opened the write path for one safe family — local, reversible,
auditable. This opens the rest: commands, HTTP, devices, and controlling an
interface. Same intent/receipt state machine, plus adapter-specific schema and
safety. **Add each adapter family only after its capability,
idempotency/uncertainty, secret redaction, simulation fixture and manual
reconciliation behavior are specified.** "Generic command" is not a substitute
for a typed device or UI controller.

- [x] **Already shipped as rule consequences:** `set_quantity`, `add_quantity`,
  `emit_promise`, `run_command`, `run_query`, `run_action`, `set_visibility`,
  `activate`/`deactivate` (including another rule), `ask`, and budgeted
  `notify`. Effects run outside evaluation from a durable queue and append
  zero-delta provenance Facts.
- [ ] **All effects use durable typed intents** carrying target, exact payload,
  schema/revision, nonce/idempotency key, principal/grant, preconditions,
  deadline, lease, retry class, expected receipt, capture/redaction, and
  compensation/uncertainty behavior. **"Run this string somewhere" is not a safe
  destination contract.**
- [ ] **Commands declare** executable identity/hash, typed arguments (no
  implicit shell unless explicitly granted), environment allowlist, secret
  handles, working-directory/filesystem roots, stdin/stdout schemas, timeout,
  process/CPU/memory limits, and network capability. **Shell interpolation is
  visible high-risk behavior, not sugar.**
- [ ] **HTTP/connectors declare** method, host/path policy, request/response
  schema, auth secret handle, redirect/DNS policy, body limits, timeout/retry
  semantics, rate/budget, idempotency support, and redacted capture. **A retry
  is automatic only when the operation is proven idempotent or carries a remote
  idempotency key.**
- [ ] **Device/actuator controllers expose** typed commands and state, physical
  bounds, interlocks, heartbeat/failsafe, manual override, acknowledgement vs
  observed outcome, and safe shutdown. Opening a call room, watering a garden
  and moving a motor are **distinct registered capabilities**, never arbitrary
  bytes sent to a sand or device.
- [ ] **A microcontroller is just HTTP — do not build a microcontroller
  subsystem** (your note, 2026-07-29, replacing an earlier device-specific
  design). Outbound, calling a board is an ordinary `do http` effect; we cannot
  tell whether the endpoint is a microcontroller or a web service, and we do not
  need to. Inbound, a board holding a valid key makes an ordinary authenticated
  request that writes the value it carries onto a Record's quantity, body or
  extension. What that needs is: an inbound route that accepts a value for a
  named Record under a key, the same typed-write path everything else uses, and
  ordinary rate/quarantine limits. Firmware revisions, monotonic device
  sequence, clock-quality declarations, offline buffering and a bespoke adapter
  contract are **not** part of this — a device that wants those can send them as
  ordinary fields.
- [ ] **Interface control goes through registered host/controller Actions** such
  as `ui.present`, `ui.navigate`, `ui.focus`, `ui.layout.apply`, or a typed
  domain controller. **Programs cannot execute arbitrary DOM/JavaScript, forge
  user input, hide permission or audit controls, dismiss a decision as the
  person, or mutate board chrome through the Ledger.**
- [ ] **Distinguish durable desired interface state** (a record/policy another
  device can reproduce) **from ephemeral presentation intent** (focus this
  record now). Each device binding may accept, adapt or deny presentation under
  local accessibility, safety, interruption and foreground-control policy.
- [ ] **Simulation replaces every external, device and interface adapter** with a
  deterministic model or scripted fixture, recording the hypothetical intent and
  receipt and never performing the production effect.
- [ ] **Decision and notification plumbing, registered UI and controllers,
  commands, HTTP, devices, then social effects — in that order.** The block 7
  exit gate applies again in full to each family.
---

## 13. Signals — turning the outside world into evidence

Why: everything so far reads Records. This is the boundary that turns
nondeterministic outside input — a scale, a camera, an HTTP endpoint, a
microcontroller, a model — into typed, ordered, replayable evidence. Adapters
capture; pure nodes validate and normalize; Programs consume only the captured
envelope. **No connector may call rule evaluation or domain storage directly.**

Nothing before this block needs it, which is why it comes after the first
product.

- [x] **`create-signal`** represents command/http/sensor/query sources on a
  schedule. Samples land as Facts and cascade like any other change.
- [x] **`create-frequency`** supports day-of-week and catch-up behavior.
  Frequencies are reusable clocks, not record timestamp columns.
- [ ] **Every capture and integration source is an off-switchable Signal
  record** carrying adapter/revision, schema, freshness, last success/error,
  health, consent/visibility/purpose scope, sampling cost, rate limit and
  retention. Phone, scale, camera, microphone, filesystem, database, HTTP,
  webhook, message bus, local model, remote model and command inputs all obey
  this contract.
- [ ] **One observation envelope:** `source_uid, source_revision,
  source_sequence, schema_uid, value, unit, effective_at, observed_at,
  received_at, place, quality, uncertainty, actor/signature, capture_hash`.
  **Source time and Cell receipt time are never conflated.** A duplicate
  sequence or hash is idempotent; a correction references the prior observation
  instead of rewriting it.
- [ ] **Separate raw capture from normalized evidence.** Preserve the signed raw
  value per retention, then derive calibrated units, validation, quality and
  semantic concept through versioned pure nodes — so a changed calibration can
  re-derive history without pretending the sensor originally emitted the
  corrected value.
- [ ] **Push, polling and streaming use one adapter contract**, declaring
  identity/key, schema, calibration, expected cadence, maximum age and safe
  backpressure. Malformed or out-of-range samples land in quarantine with a
  visible reason.
- [ ] **A microcontroller is not a special case — it is HTTP** (your note,
  2026-07-29). An inbound authenticated request carrying a value for a named
  Record is the whole contract; there is no device-specific adapter family, no
  firmware/sequence/clock-quality declaration, and no offline-buffer protocol.
  A board that wants to send those sends them as ordinary fields. Outbound is
  `do http` (block 12).
- [ ] **Deactivating a Signal stops new acquisition and downstream triggers; it
  does not erase previous observations.** Revoking camera, microphone, location
  or health consent also prevents new *use* by runs, not merely new sampling.
- [ ] **AI enters only as a visible captured model Signal or an ordinary
  candidate author.** No ambient reader, no hidden prompt-side data, no
  privileged writer. Prompt inputs obey Protein visibility and purpose scope;
  secrets and unrelated context do not enter the trace.
- [ ] **Replace ambiguous catch-up with the explicit missed policy** from block
  11 (`skip`, `coalesce`, bounded `replay`), exposing start/end, timezone,
  weekdays, calendar interval, jitter, and whether calendar alignment happens
  before or after interval addition. **Jitter derives from the
  schedule/occurrence seed, never ambient randomness.**
- [ ] **Event-time nodes declare their late-data watermark and correction
  behavior:** ignore for live action but include in later analysis, recompute an
  open window, compensate a reversible result, or ask. Historical evidence never
  silently causes a present-tense actuator or social effect.
- [ ] **Context is always a saved or inline Protein plus named derived values**,
  never an ambient database capability. The run records the exact visible input
  set, why each row was included or excluded, and the freshness and quality
  used, so a recommendation can explain missing, denied, invalid or stale data.
- [ ] **Source health is data, not logs:** last scheduled/attempted/successful
  sample, lag, consecutive errors, clock drift, dropped/quarantined count,
  adapter version, next retry. Health can feed an operational Sense without
  recursively treating its own alarm as healthy input.
- [ ] **Connectors reference secrets by opaque capability-bound handle.** Program
  exports, traces, errors, notifications and synced records never serialize
  secret values. A simulator receives a fixture, not the production secret.

**Exit:** HTTP plus one buffered microcontroller source survive duplicate, late,
malformed, stale, disconnect, reboot and secret-redaction tests; turning off a
Signal stops acquisition and use without deleting history.
---

## 14. Learning and recommendations

Why: this is the lower-priority adaptation lane, not the rule runtime. It admits
only explicitly eligible evidence, updates versioned checkpoints, and proposes
probabilities and patterns. **Routing, Trust and authority decide what those
outputs may become.** It runs late on purpose: reaction-before-learning means
inference may never precede a working deterministic path, and the first real
product provides the first real evidence.

Start with the transparent recurrence model before any advanced learner, so
every later algorithm inherits the same evidence and rebuild contract.

### Keeping the quantities apart

| Quantity | Question |
| --- | --- |
| Recurrence probability | How likely is this event/action within this context and horizon? |
| Estimate confidence | How much eligible evidence supports that probability, and how wide is its uncertainty? |
| Counterparty evidence | What visible signed outcomes exist for this Person, concept, window and role? |
| Expected utility/cost | Under this person's stated objective, how good is a candidate, compared with what? |
| Authority eligibility | Does a current grant allow the proposed Action now? |

The shipped `confidence(@p)` (see `docs/Central: Senses.md`) is a useful
ingredient whose name is too broad for this.

### Evidence

- [ ] **Define a pattern hypothesis** by event schema, principal/household,
  concept hierarchy, direction and quantity band, counterpart role, place
  region, calendar/cadence bucket, prerequisite context, horizon and feature
  revision. **Similarity and generalization are explicit** — the learner never
  silently widens from "green apples from this store" to all food or all people.
- [ ] **Separate opportunity/exposure from positive, negative and censored
  evidence.** An observed purchase can be positive; a deliberately skipped
  eligible opportunity can be negative; **"there is no Fact" is unknown** unless
  the program proves the opportunity was observable. Outages, hidden data and
  periods before a source existed are censored, not failure.
- [ ] **Learn only from evidence a versioned policy admits:** human-authored
  Facts, independently sensed outcomes, signed or mutually confirmed
  occurrences, deliberately labeled feedback. **A recommendation, generated
  draft, program-created task, model text or automated Action never becomes
  positive evidence merely because the system produced it.** A later
  independently observed outcome may train the model on its own merit.
- [ ] **Keep local `recurrence_likelihood` separate from counterparty
  evidence**, keyed only by locally visible, purpose-permitted context. Purchase
  need, consumption cadence, seller reliability, price forecast and Transfer
  agreement likelihood are different models a program may compose.

### The first model

- [ ] **A deterministic decayed Beta/cadence model, not a vague "growth
  factor".** For eligible evidence `i`:

      weight_i = decay(age_i, configured_half_life, decay_version)
      alpha    = prior_alpha + sum(weight_i * positive_i)
      beta     = prior_beta  + sum(weight_i * negative_i)
      recurrence_probability = alpha / (alpha + beta)

  `decay` is a specified fixed-point lookup so it replays identically. Each new
  event changes the posterior less as evidence accumulates; old evidence loses
  influence by half-life. **Confidence is reported separately from probability**
  using effective sample weight and a versioned credible interval. Cadence uses
  deterministic eligible-time buckets or a discrete time-to-event hazard, so
  "usually Saturday morning" and "about every eight days" coexist without
  confusing frequency with certainty.
- [ ] **Store as typed policy, not frontend state:** prior, evidence
  query/policy, positive/negative definitions, half-life, cadence/timezone,
  feature buckets, minimum effective sample weight, probability and confidence
  thresholds, enter/exit hysteresis, mute/snooze, drift policy, model version.
- [ ] **Persist each update** with prior checkpoint hash, admitted and rejected
  evidence ids and reasons, logical evaluation time, resulting sufficient
  statistics, metrics and new checkpoint hash. **Checkpoints are caches:
  replaying eligible evidence is the truth and must reconstruct them.**
- [ ] **Avoid combinatorial context mining** by declaring candidate feature
  templates and resource/privacy budgets. New pattern discovery emits a
  hypothesis with multiple-testing information; it does not create a million
  invisible rules or search sensitive attributes by default.
- [ ] **Split evidence into deterministic train/validation horizons.** Report
  calibration, false-positive/negative cost, support, drift and baseline
  comparison before a learned policy graduates from watching to suggestion or
  autonomy.
- [ ] **Model lifecycle:** `cold → learning → calibrated → drifting →
  stale/disabled`. Insufficient, stale, shifted or contradictory data lowers
  confidence and autonomy. Threshold crossings use hysteresis and minimum
  duration so values near the line do not chatter.
- [ ] **A registry of deterministic learner types:** decayed count/Beta,
  cadence/hazard, moving quantile, seasonal baseline, anomaly/change detector,
  regression/classification, later seeded advanced models. Each publishes its
  feature contract, limitations, update rule, metrics, memory/fuel bounds and
  explanation strategy.
- [ ] **A learner never mutates a program graph or its own feature/evidence
  scope.** It emits parameters or a revision candidate. Automatic promotion
  requires a pre-authorized template and scope, Proof, validation, optional
  shadow duration, rollback condition, and a grant that explicitly includes
  activation.
- [ ] **Refine counterparty evidence by concept/role/window** and show kept,
  broken, disputed, late, partial, missing and verification counts directly. Any
  smoothed estimate is local decision support, **not a global reputation score,
  identity label, or fact about a person's character.**

### The recommendation contract

A recommendation is a durable, inert interface between inference and choice.
Build this lifecycle **before** whispers or automatic drafts, so Attention never
becomes the only place a candidate exists.

- [ ] **One lifecycle-managed recommendation per `(pattern/objective, subject,
  horizon, candidate-kind)`**, deduplicated and updated in place as evidence
  changes. It carries claim, evidence, model revisions, probability,
  confidence/uncertainty, expected benefit/cost, alternatives, freshness,
  required authority, expiry, and an exact preview/diff.
- [ ] **States:** `open, accepted, accepted-edited, dismissed, snoozed, muted,
  obsolete, expired`. New evidence may update an open item but **cannot
  resurrect a muted pattern or replace a person's edited choice.**
- [ ] **Routes are policy, and a high score never skips one:**
  `observe-only → log`, `suggest → recommendation`, `draft → inert candidate`,
  `ask → durable decision`, `act → authorized intent`. Probability/confidence
  thresholds and authority checks are required at every transition.
- [ ] **Feedback is typed and contextual:** correct, incorrect, wrong
  time/place/quantity/person, already done, not useful, too frequent, accepted
  unchanged, accepted edited, snoozed, mute. It may update delivery and pattern
  models under their evidence policy while preserving the original
  recommendation and response.
- [ ] **Detect action/recommendation loops.** If accepting a suggestion creates
  the only evidence that makes it more likely, mark the path endogenous and
  exclude or separately measure it. Compare against a no-intervention baseline
  where feasible.
- [ ] **Explanations at several depths:** one sentence, substituted values,
  evidence timeline, model card/uncertainty, objective/alternatives, policy
  decision, and counterfactual ("without Tuesday's consumption Fact, this would
  stay below the suggestion threshold").
- [ ] **A human, agent, imported template or model may author the same inert
  candidate format.** Authorship is provenance, not permission; none bypass
  visibility, evidence display, Proof, budgets or the principal's grant.

**Exit:** the apple example moves from cold evidence to one explained
suggestion or draft with no self-training, no duplicate recommendation, correct
decay and rebuild, and no authority derived from probability. The recurrence
reference model separately proves prior behavior, diminishing update influence,
half-life decay, cadence, hysteresis, confidence/support separation,
negative/censored evidence, feedback, deduplication, drift, rebuild, and
exclusion of endogenous self-training.
---

## 15. Attention — asking a person without pestering them

Why: Attention schedules human interruption **after** a candidate or decision
already exists. It ranks and delivers. It does not recompute the decision, gain
authority, or hide parked work. Build the inbox and digest truth first, device
channels afterwards, with one cross-device acknowledgement identity.

- [x] **The Decision Queue is `source: decision, live: true`, and `decide` is the
  answer.** Deterministic feeders cover broken promises (`expiry`), rule `ask`
  consequences (`ask`), Senses matches (`draft`), and projected crossings
  (`crossing`, one-week horizon, deduped per record).
- [x] **`decide { decision, answer }` closes a decision through the Ledger.**
  When the chosen option carries an Action, deciding executes it, for one-tap
  flows such as "yes → set quantity".
- [x] **Decisions with `expires_at` auto-close as `expired`**, and each sweep
  deduplicates by `(subject, kind)` — one unresolved situation asks once.
- [x] **The daily notification budget is hard**
  (`configuration.attention_budget_per_day`, default 12). Excess notification
  effects finish as `parked:digest`: **deferred, not lost.**
- [ ] **A decision is durable work requiring a choice; a whisper is its calm,
  context-aware delivery.** Whispers never become a second queue and never
  execute Actions — they link to a decision, recommendation, run or changed fact
  and disappear without losing the underlying item.
- [ ] **A decision freezes** the question, subject, evidence/run, options and
  exact Action previews, required principal, default/no-answer behavior,
  deadline, reversibility and current-revision preconditions. **Answering after
  the world changed either revalidates or returns a stale decision; it never
  executes an obsolete preview.**
- [ ] **Rank by explicit user priority, urgency/window, confidence, safety,
  reversibility, cost of delay, interruption cost and recent delivery load.**
  The formula and tie-break are inspectable. Low-value items collect into
  summaries. **Urgency does not manufacture authority.**
- [ ] **Route through device records** to inbox, digest, desktop toast, mobile
  push, sound or text, with per-program/per-source channel controls, quiet
  hours, location/context eligibility, accessible presentation, and "show why
  now".
- [ ] **Delivery has an idempotent whisper uid and per-channel
  attempts/receipts.** Opening, acknowledging, dismissing or answering on one
  device converges on the durable item and suppresses redundant channels per
  policy.
- [ ] **Escalation is explicit:** retry a channel, change channel, notify another
  delegated recipient, or expire. **No program infers permission to contact a
  family member or employer merely because the primary person did not answer.**
- [ ] **Attention policy reserves capacity for safety and expiring
  commitments**, caps every source and program, supports "never interrupt for
  this", and shows which items were parked by budget. Digest generation
  summarizes links; it does not replace or mutate the underlying decisions.
- [ ] **Feedback is a first-class result** (`accepted`, `edited`, `dismissed`,
  `snoozed`, `muted`, `wrong-context`) used to tune local delivery and pattern
  policy without rewriting historical evidence.
- [ ] **No coercive ranking, synthetic urgency, dark patterns, or hiding the "do
  nothing / mute / pause" option.** Explanations and controls stay available.
  Accessibility and quiet-time constraints are hard policy.
---

## 16. Automation Trust and driving Transfer

Why: up to here everything Karma does is local. This is the first block where
automation touches another person, and it needs a gate the grant cannot express.
A grant says *"this Program may perform these Action kinds for me within these
budgets"*. A Trust scope says *"these counterparties, Organs and proximities are
acceptable for this concept and stage, at these evidence thresholds"*. **The
effective result is their intersection, never their union.**

**Probability is evidence about what may be needed. It is not trust in a seller
and not authority to transact.**

The gates stay separate and conjunctive:

    visible offer ∩ recurrence probability/confidence ∩ objective/terms ∩
    Automation Trust scope ∩ principal grant/budgets ∩
    Transfer domain revision/agreement/occurrence readiness

Raising probability cannot compensate for an untrusted Organ; allowlisting an
Organ cannot compensate for insufficient evidence or grant; a grant cannot make
an invisible or stale offer visible or current. A failure in any gate denies or
routes to Attention **with its own reason**.

Existing contact state remains the coarse first gate: `blocked` always denies
import, discovery, suggestion delivery and automation; `known` merely permits
ordinary interaction and never implies automation. Automation Trust is finer,
local, concept-specific and unpublished by default. It is **not** a reputation
level and cannot grant authority to the counterparty.

### Tiers, not a boolean

| Tier | Highest behavior the scope will consider | Still required |
| --- | --- | --- |
| `observe` | Read/compare visible evidence | visibility/purpose |
| `suggest` | Show a recommendation involving the counterparty | recommendation policy |
| `draft` | Create a private local Transfer draft | `transfer.draft_local` |
| `propose` | Publish/address/send a proposal | `transfer.publish/propose` |
| `negotiate` | Claim/counter/revise inside terms | `transfer.negotiate_own` |
| `commit` | Agree/activate the principal's own side | exact high-authority agreement/activation grant |
| `settle` | Claim/confirm own occurrence, settle owned Record | independent evidence, confirmation/settlement grants, domain readiness |

- [ ] **Higher tiers include willingness for lower stages but confer none of
  their capabilities.** The effective stage is the minimum of Trust ceiling,
  grant capability ceiling, current policy route, and Transfer domain
  capability.

### The scope and its selector

- [ ] **`AutomationTrustScopeRevision` is immutable** and contains: owner person,
  optional program/revision, purpose, concept + include-descendants, direction
  (`buy|sell|give|receive|any`), stage ceiling, counterparty selector, per-stage
  probability/confidence, allowed units, quantity and value ranges, allowed
  places/windows/weekdays, rate and aggregate budgets, required counterparty
  evidence, `valid_from` / `expires_at`.
- [ ] **The selector AST is explicit — there is no ambiguous "list plus
  proximity".**

      enum SelectorExpr {
          Any(Vec<SelectorExpr>), All(Vec<SelectorExpr>), Not(Box<SelectorExpr>),
          PersonIn(Set<PersonUid>), OriginOrganIn(Set<OrganUid>),
          ViaOrganIn(Set<OrganUid>), ProximityAtMost(u32),
      }

  `any { organ list; proximity <= 2 }` means either; `all { … }` means both.
  Empty `any` is false; empty `all` is true only inside a scope that also names
  a positive selector; **a scope with no positive counterparty selector cannot
  activate above `suggest`.**
- [ ] **An explicit deny list is evaluated first and always vetoes.** Then the
  exact Trust revision the Program or grant references is evaluated. **Lince
  does not merge every matching allow rule and guess precedence** — multiple
  scopes require an explicit `any/all` composition in the Program policy. This
  keeps "why was this seller allowed?" mechanically answerable.
- [ ] **Transfer parties remain People.** An Organ selector says which Cell
  identity may originate or carry the relationship; it does not trust every
  Person inside that Organ or sign for them. A Person selector may narrow the
  party inside allowed Organs. For relayed discovery, `origin_organ` is the
  record's preserved lineage and `via_organ` the delivery contact; a policy may
  require either or both. **Proximity is the evaluating Cell's local contact
  value, never a remote self-asserted number**, and no rule automatically
  broadens its maximum.
- [ ] **Persistence:** a Record (`kind=automation_trust_scope`, quantity is
  activation) plus `automation_trust_scope_revision` (owner/program/purpose/
  concept/direction/stage/threshold/limits/validity + content hash),
  `automation_trust_selector_node` (normalized `any/all/not/atom` tree, stable
  node order), and `automation_trust_selector_member` (Person/Organ sets with
  `allow|deny` and `origin|via|person` roles). Integer fixed-point columns for
  probability/confidence; canonical quantity/value/unit fields.
- [ ] **Create/revise derives the principal, validates every referenced
  Person/Organ/concept, canonicalizes the selector, appends an immutable
  revision and Fact, and never activates a widened revision by implication.**
  Narrowing may be immediate; widening needs the same explicit authority and
  preview as a new Transfer grant.
- [ ] **Extend `source:"karma"` with `object_kind="trust_scope"`** and predicates
  `concept_in`, `program_eq`, `stage_ceiling_gte`, `person_eq`,
  `origin_organ_eq`, `via_organ_eq`, `max_proximity_lte`, `active`,
  `expires_before`; includes expose the normalized selector, thresholds/limits,
  referencing Programs and grants, current capabilities, and recent allow/deny
  traces.
- [ ] **Policy evaluation returns a structured proof, never just `false`** — a
  per-dimension pass/fail list (concept, direction, probability, confidence,
  deny selector, positive selector, stage ceiling, quantity/value/window, grant
  reservation, transfer domain revision). The exact Trust and grant revisions
  are frozen into the candidate explanation and **rechecked live before intent
  dispatch**; a later block, expiry, proximity change, Trust revision, offer
  revision or budget use deterministically denies or stales the intent.

### Driving Transfer, stage by stage

Every Transfer mutation stays the typed, revision-safe, idempotent domain Action
in `docs/Central: Transfer.md`. **Karma never edits Transfer tables, invents
signatures, bypasses agreement or occurrence gates, or keeps a second Transfer
state machine.** It may control every legitimate stage of a principal's own side
when that exact capability is delegated.

| Stage | Capability and non-negotiable gate |
| --- | --- |
| Observe/project/match | `transfer.read/project`; visibility applies before matching, scoring, aggregation and explanation |
| Private local draft | `transfer.draft_local`; Trust at `draft`; freezes source evidence and expected value/window but contacts nobody |
| Publish OPEN / address people | `transfer.publish/propose`; Trust at `propose` plus separate recipient, audience, concept, value, rate and expiry grant. **Publication is a social effect, not "just a draft"** |
| Claim an OPEN promise / counteroffer | `transfer.negotiate_own`; Trust at `negotiate`, exact current revision, allowed counterparties/terms, stale-write rejection, signed principal attribution |
| Revise terms | `transfer.revise_own`; only fields and ranges in the grant. Normal domain semantics invalidate agreement — **Karma cannot preserve stale consent** |
| Review/agree own side | `transfer.agree_own`; Trust ceiling `commit` plus explicit high-authority delegation naming agreement policy, counterparty/cohort, concept/value bounds, window, evidence and expiry. **It can never sign another party's level** |
| Activate/reserve own contribution | `transfer.activate_own`; current-revision agreement and availability/reservation policy must already permit it. Budget reservation is atomic |
| Claim delivery/receipt/occurrence | `transfer.claim_occurrence_own`; Trust ceiling `settle`, only the principal's statement, tied to qualifying independent evidence or an explicitly allowed manual source. **A program's own intent is not proof it happened** |
| Confirm own side | `transfer.confirm_own`; current occurrence, confirmation policy, evidence source/quality, principal grant. **It never confirms what the counterparty must attest** |
| Settle an owned Record | `transfer.settle_local`; only after domain readiness, expected revision, idempotency, local ownership, application formula and quantity/value budgets pass. Settlement still creates ordinary signed Facts |
| Withdraw/cancel/dispute/correct | Separate `transfer.withdraw_own` / `cancel_own` / `dispute_own` / `correct_own`; terminal evidence is never rewritten |
| Expand visibility/proximity | `transfer.declassify`; never implied by propose or agree. Exact fields/audience and privacy budget reviewed independently |
| Remainder/successor/dependency | `transfer.draft_local` by default; later gates apply and cannot inherit authority accidentally |

- [ ] **Encode these as capability families, not one `transfer:automatic`
  boolean.** Grants can allow drafts but forbid publication, allow a weekly
  purchase from named sellers but forbid new recipients, or allow settlement
  only from a bound scale or scanner confirmation.
- [ ] **Freeze the proposed canonical revision and preview at policy time**, then
  send `expected_revision` and an idempotency key through the normal Action. A
  stale counteroffer, changed price, recipient, unit, window, location,
  visibility, agreement or evidence returns to policy and Attention.
- [ ] **Never use locally inferred counterparty probability as their consent.**
  Each Person or their explicitly delegated program acts only for their own
  identity. Cross-Cell automation composes through signed proposals and
  responses, not shared hidden authority.
- [ ] **Autonomy is chosen per step** — "always ask before publishing",
  "auto-counter within 5% and these sellers", "auto-agree this exact recurring
  revision", "settle after both signed scanner receipts". A human can override,
  pause, narrow or revoke at any time.
- [ ] **Keep payment execution separate from Transfer settlement.** A payment
  connector is another high-authority external effect with its own receipt and
  reconciliation; a successful payment receipt may be evidence for a Transfer
  policy but **does not silently settle Records.**
- [x] **Transfer automation fails closed against the old activation path**, and
  current manual revision, agreement, occurrence, confirmation and settlement
  gates remain authoritative until the capability system exists.

**Exit:** exact allow/deny precedence and list/proximity boolean selectors work
for relayed and origin Organs; a `0.99p` apple need cannot draft or propose
outside `trust:@apple.known_sellers`; allowlisted Organs and proximity arms
behave exactly as declared; Trust or grant revocation before dispatch prevents
the effect; **no Organ scope acts as consent for a Person**; and every Transfer
stage is proven both denied-by-default and permitted inside an exact expiring
scope.
---

## 17. Workflows and optimization

Why: some work is multi-step and long-running (wait for approval, retry, then
compensate), and some work is a search over a person's own goals. Both are built
on the occurrence and intent machinery already proven, and neither gets a second
implementation of domain behavior.

### Durable workflows

- [ ] **Add workflow nodes:** state machine, sequence/parallel, wait-until,
  branch, approval, retry, compensation, child-program invocation. **A workflow
  coordinates typed Actions; it does not reimplement domain behavior.**
- [ ] **Every workflow declares** a concurrency policy (`queue`, `drop`,
  `coalesce`, `latest`, or bounded parallel), correlation key, timeout,
  cancellation semantics, and parent/child ownership. Long-running work resumes
  from durable node state after boot.
- [ ] **Cancellation is cooperative and observable.** It prevents unclaimed
  intents, requests cancellation from claimed adapters, waits or times out per
  policy, and runs only declared compensations. **It does not claim an
  irreversible external effect was undone.**
- [ ] **Transaction boundaries are narrow.** Compatible local Actions may commit
  atomically through one domain Action; external or social multi-step work is a
  saga with receipts and compensation. **A workflow cannot hold a database
  transaction while waiting for a person, network or device.**

### Optimization

Analysis is pure Karma: it can inspect, aggregate, forecast, search and compare
**without receiving action authority.** Execution stays a separate policy
decision.

- [ ] **An objective specification** names owner, purpose, decision variables,
  units/domains, hard constraints, soft penalties, objective order
  (lexicographic, weighted, Pareto or satisfice), planning horizon, uncertainty
  treatment and tie-break. **Missing objectives never default to "maximize
  activity", quantity or engagement.**
- [ ] **Deterministic solver adapters** for linear/simplex, mixed-integer,
  constraint/scheduling, min-cost flow/matching, routing and simulation-based
  search, added as needs justify them. One typed solver contract, so an adapter
  can be replaced without changing program or effect semantics.
- [ ] **Translate Records, Facts, Links, Promises, availability, time windows,
  places, units, skills, budgets and user constraints into solver variables
  through explicit feature nodes.** The mapping and every approximation appear
  in the run, not in a sand.
- [ ] **Return a plan set, not one answer** — objective values, binding
  constraints, slack, sensitivity range, assumptions, uncertainty, excluded
  alternatives, and a deterministic infeasibility explanation or the smallest
  known conflicting constraint set.
- [ ] **Distinguish forecast from plan from schedule.** A forecast estimates what
  may happen under stated assumptions; a plan selects intended actions under
  objectives; **a schedule reserves time or resources only when a separate typed
  Action says so.**
- [ ] **Support robust/scenario planning** over captured distributions and
  Imagination branches. A plan states which uncertainty it tolerates and which
  future observation should trigger replanning; it never hides a point estimate
  behind an exact-looking answer.
- [ ] **Reoptimization preserves stability** through explicit change penalties
  and frozen commitments. It may not churn a person's day or revise an agreed
  Transfer merely because a marginally better solution appeared.
- [ ] **Multi-person optimization uses only shared, visible constraints and
  objectives.** It produces a proposal each party can inspect; it cannot infer a
  hidden preference, expose another person's private constraint, or treat one
  Cell's optimum as agreement.
- [ ] **Analysis is callable through Protein and Actions** and reusable by
  humans, agents, programs and sands: validate, solve, explain, compare, cancel,
  pin a result as a candidate. **Solvers never get an implicit effect channel.**

**Exit:** waits, retries, cancellation and compensation survive reboot; the
weekly scheduler explains alternatives and infeasibility; applying a plan
invokes only separately approved current Actions.
---

## 18. Imagination, replay and simulation testing

Why: Imagination is not a forked rule engine. It supplies a snapshot, virtual
ports and an event/fault script to **the same** scheduler, evaluator and policy
code, then stores isolated traces and comparisons. Build replay first,
projection and branching second, generated fault testing last.

**This runs last, and that is the acknowledged cost of shipping a product
first.** Determinism is designed in and covered block by block — replay
capsules, frozen epochs, injected clocks, restart and ordering proofs all exist.
What does not arrive until here is the adversarial machinery that tries to break
it. Accepted deliberately: a product that exists is worth more than a proof
about a product that does not.

### Four products, one kernel

- [ ] **Replay** reproduces a past run from captured inputs. **Projection** folds
  one stated future. **Scenario/planning** compares deliberate branches and
  uncertainty. **DST** generates event and fault schedules searching for
  invariant violations. The UI and test runner differ; **the execution semantics
  do not.**
- [x] **`Engine::project(now, until)` folds promises and rules on a virtual clock
  with Signals frozen; `Engine::snapshot(now)` creates mutable input for
  toggle/clear/re-fold/diff**, so branching futures already exist as an internal
  call. **Legacy scope:** it folds `registry.rules` and `f64` quantities and
  silently skips rules needing signals or sums. Block 9 rebuilds this over Karma
  programs with exact decimals and reported exclusions; this stays checked only
  until the rule import lands, at which point its input goes empty.
- [ ] **Expose project/snapshot through a typed transport verb** so a sand can
  scrub and branch a future — change starting quantities, toggle a program,
  clear a promise, alter time, re-fold, compare timelines — without touching the
  real Ledger. Ships against block 9's projector, not the legacy fold.
- [ ] **Simulation runs on an isolated snapshot with a virtual clock and mocked
  signals/effects.** Its seed, inputs, event script, stopping/bookmark
  conditions, replay capsule and engine version make every run reproducible.
  **"Apply" means separately reviewing ordinary typed Actions** — never
  committing simulated state wholesale or reusing simulated receipts as real
  evidence.
- [ ] **Simulation replaces every external, device and interface adapter with a
  deterministic model or scripted fixture**, recording the hypothetical intent
  and receipt. It never performs the production effect.

### Asking questions of a future

- [ ] **People define invariants and questions:** can this state be reached, do
  these programs conflict, will a quantity cross a boundary, does the graph
  settle, can an effect repeat, what changes if this promise disappears? Proof
  results link to the exact revisions and counterexample trace.
- [ ] **Proof has three honest result classes:** proved within a stated
  finite/symbolic domain, no counterexample found under stated exploration, or
  counterexample found. **Timeouts and unsupported nodes are "unknown", never a
  green check.**
- [ ] **Program authors declare** assumptions, controllable variables,
  distributions/ranges, invariants, bookmarks, stopping conditions, maximum
  logical time/events/fuel, and effect fixtures. An unconstrained scenario
  cannot accidentally read production secrets or call production adapters.
- [ ] **Build calendar/time-budget and graph/state-space projections from the
  same simulator** — time on one axis, quantities/ranges on another,
  rule-active regions, consequence arrows, dependency/supply-chain paths,
  uncertainty bands, real-vs-projected values.
- [ ] **A continuous forecast is a cache** linked to its starting cursor,
  assumptions, revisions and generation time. New evidence marks it stale and
  queues recomputation; **it is never mistaken for a promised or settled Fact.**

### Fault generation and shrinking

- [ ] **Deterministic generated scenarios and fault injection** for time jumps,
  DST gaps/folds, restart/crash at every durable boundary, delayed/failed/
  duplicate/uncertain effects, duplicate Facts, stale decisions, grant
  revocation races, exhausted budgets, reordered sync arrival, partitions,
  corrupt/quarantined inputs, missing/stale Signals, device disconnect and model
  drift. This is both product Imagination **and** the test architecture for the
  autonomous runtime.
- [ ] **Drive generated runs from a named workload distribution** over programs,
  Records, Transfers, people, time, signals, actions, faults and operator
  choices. Record the root seed **and a split seed per generator/node** so
  failures replay when generation is parallelized.
- [ ] **Deterministic shrinking of a failing trace** that preserves the violated
  invariant, emitting a portable replay capsule plus a readable causal
  counterexample. **A seed without the engine/program/model hashes and captured
  fixtures is not a complete reproduction.**
- [ ] **Small independent reference models for foundational invariants** where
  practical: Ledger/quantity conservation and compensation, schedule occurrence,
  grant/budget consumption, exactly-once intent identity, workflow state,
  Transfer readiness. Differentially compare production kernel, reference fold
  and upgrade versions.
- [ ] **Simulate multiple Cells** with independent occurrence cursors, clocks,
  visibility, grants, outboxes, partitions and delivery schedules. Assertions
  distinguish per-Cell deterministic replay from convergence properties that
  should hold after all permitted messages arrive.
- [ ] **Shadow mode runs a candidate revision beside the active one** against
  live captured evidence, blocks all effects, and compares candidates, intents,
  resource cost, false alarms and policy outcomes. **Promotion criteria and
  rollback triggers are stored before the shadow begins.**

**Exit:** seeded failures replay and shrink; sampled production capsules
hash-match; multi-Cell convergence and revocation/dispatch crash boundaries are
covered; unsupported and timeout Proof results stay `unknown`, never green.
---

## 19. Runtime operations

Why: an always-on engine needs operability as part of its data model. Queue lag,
scheduler mode and cost, program/model/connector health, denials, uncertain
effects, replay audits and recovery controls must be readable and actionable
**without shell access**. Build health projections alongside each block rather
than adding metrics after autonomy ships.

- [ ] **Publish engine health through Protein:** mode, active build/schema,
  leader/sequencer lease, last cursor, deadline lane plans/arms/earliest
  deadline, active/estimated/actual semantic and wake rates and budget, queue
  depth and oldest age by class, runs per state, effect worker health, schedule
  lag, model backlog, storage pressure, last successful checkpoint/replay audit.
- [ ] **Type failures** as invalid definition/input, missing/stale/denied data,
  Proof rejection, policy/authority denial, conflict/stale revision, budget/fuel
  exhaustion, adapter unavailable, retryable/terminal/uncertain effect,
  invariant violation, or engine fault. **Retry policy follows type, not string
  matching.**
- [ ] **An unexpected invariant violation enters stage-effects or emergency-stop**
  per configured severity, preserves the replay capsule, stops related dispatch,
  and opens one high-priority operational decision. **It never catches an error
  and silently continues acting.**
- [ ] **Separate user pause, policy denial, program fault, connector outage and
  global stop**, so recovery cannot confuse "operator said no" with "try again".
  Resume shows the occurrences and intents that will become eligible.
- [ ] **Enforce CPU/fuel, memory, trace, storage, I/O, network, notification,
  Action, value and candidate/fan-out quotas** per run/program/principal/Cell.
  Maintenance and safety controls retain reserved capacity under overload.
- [ ] **Trace and evidence retention is purpose- and sensitivity-aware.**
  Redaction produces a new view, not a modified Fact; secret values and
  unnecessary raw personal data never enter general traces in the first place.
- [ ] **Periodically replay sampled completed runs from their capsules and
  compare hashes.** A mismatch is a determinism incident with an
  engine/revision diff, **not an ignorable test flake.**

### Cross-cutting proof gates

The architecture is not complete when the happy path works.

- [ ] A replay capsule produces byte-identical canonical runs, candidates,
  policy decisions, intents, unsigned Fact payloads/content hashes and captured
  signature/receipt bytes across repeated runs and different host thread
  schedules.
- [ ] Crashes at every persistence/lease/dispatch/receipt boundary lose no
  accepted occurrence, repeat no intended idempotent effect, resume workflows,
  and expose uncertain non-idempotent effects for reconciliation.
- [ ] Duplicate Facts, samples, sync packages, occurrences, decisions, Action
  requests and effect receipts are idempotent; recorded alternate arrival order
  is replayable and convergence assertions hold where specified.
- [ ] Simultaneous conflicting writers resolve by declared deterministic policy
  and preserve rejected alternatives and explanation; **no thread race selects
  one.**
- [ ] Revoking or narrowing a grant while runs are evaluating, staged, leased or
  about to dispatch prevents every still-preventable effect. Budget reservation
  is atomic under concurrent runs.
- [ ] Visibility and purpose taint apply before input, feature, aggregate, model
  update, explanation, recommendation, optimizer, notification and external
  effect. Small-cohort and differencing tests reveal nothing outside policy.
- [ ] Decimal/fixed-point quantities, probabilities, decay, conversions,
  schedules/timezones, solvers, seeded algorithms and pure extensions replay
  identically on supported platforms.
- [ ] DSL → canonical AST → visual graph → DSL round-trips without semantic
  drift. Layout and display-label changes preserve the revision hash; type,
  node, dependency, expression, policy or effect changes produce a new hash.
- [ ] Millisecond boundaries preserve exact `intended_at` and stable cursor
  ordering when Facts and timers share a millisecond. Late wake-up follows
  skip/coalesce/replay policy and never rewrites intended time.
- [ ] With simultaneous `3ms`, `5h`, daily and monthly Frequencies, tracing
  proves a fast wake reads, drains and re-arms only its due dense lane. Sparse
  registrations receive no SQL query, due-check, heap pop or timer re-arm from
  the `3ms` path, yet still produce their occurrence at the exact intended
  boundary. With only `5h`, the director performs no Frequency work between
  activation and its one-shot wake.
- [ ] Lane assignment contains no fixed cadence classes. Deterministic
  demand/capacity tests split, pack and merge the same schedules in stable uid
  order; changing host capacity may change only the recorded operational lane
  plan, never semantic occurrence ids or results.
- [ ] One reusable Frequency referenced by ten active consumers has one cursor
  and deadline and fans one occurrence out deterministically. Removing the last
  consumer disarms it; adding the first follows the exact `inactive_gap` policy
  and never surprises the owner with implicit dormant-history replay.
- [ ] A `1ms` Frequency is denied unless its computed demand fits aggregate Cell
  capacity, Program wake/evaluation/write/effect budgets, required
  dense/precision capabilities, and a declared overload policy.
- [ ] Dense `OccurrenceBatch` replay yields the same semantic tick ids, state
  transitions, candidates, intents and Facts as individual scheduling;
  compacting no-op traces never coalesces requested semantics.
- [ ] For each evidence cursor, all already-active reaction work precedes its
  learning update. A threshold-crossing model update or meta-rule creates a
  later occurrence and cannot change the revision, parameter or checkpoint used
  to process its own evidence.
- [ ] A meta-rule changes `freq:@recovery.reminder_tick` from `1d` to `3d` only
  through a range-scoped grant and parameter Action. All four rephase policies
  produce their specified next boundary, survive restart, and replay.
- [ ] A learned pattern can remain observed, create one explained suggestion,
  create an editable draft, or promote a template revision only per its
  route/grant/shadow policy. **No probability value manufactures authority.**
- [ ] Every Transfer lifecycle capability is tested both denied-by-default and
  permitted inside an exact delegation. Automation signs only its principal's
  side, respects current revision and domain gates, never treats prediction as
  consent or evidence, and cannot widen visibility through another capability.
- [ ] Automation Trust selectors prove exact Person, origin Organ, via Organ,
  proximity, `any/all/not`, deny-first, concept/direction, stage ceiling,
  threshold, limit, expiry and blocked-contact behavior, with the full
  structured allow/deny proof available through Protein.
- [ ] Commands, HTTP, models, UI controllers and microcontrollers prove schema,
  secret redaction, capability scoping, timeouts, retry/idempotency, receipts,
  uncertainty, interlocks, simulation substitution and manual override.
- [ ] Human UI, CLI and software agent perform the same authorized program,
  simulation, candidate, decision, grant, workflow and intent operations through
  Actions and Protein; **none has a hidden database or effect path.**
- [ ] Emergency-stop, observe-only and stage-effects survive reboot;
  queue/workflow/effect disposition is explained before resume, and normal
  inspection works without filesystem logs.

### Vertical workflows that prove the pieces compose

Each is a product surface built on the blocks above, with no new core.

- [ ] **Economy** (a preset, through the Karma sand): individual and recurring
  resource gains/losses, correction/void, due-occurrence resolution, exact
  monthly gain/loss/net, tag/source profile, actual/expected resource graph, and
  entry/Fact drill-down. Typed, voice and photo capture later produce the same
  inert entry draft without a privileged Fiote path.
- [ ] **Todo / knowledge base:** a habit re-arms daily and completing it posts a
  causal Fact; missing a day is negative evidence only if the opportunity policy
  says completion was observable.
- [ ] **Recurring tasks:** a monthly schedule fires exactly once under normal
  time and obeys its catch-up policy after downtime and DST transitions.
- [ ] **Adaptive Frequency:** a recovery rule tunes another reusable Frequency
  from daily to every three days after seven stable observations, then restores
  it when stability leaves — with no same-occurrence or mid-cascade definition
  change possible.
- [ ] **n8n-style command flow:** a signal → rule/workflow → leased effect graph
  built visually, dry-run, executed, inspected and safely retried.
- [ ] **CRM / people:** a birthday whisper arrives at the chosen moment and an
  interaction report is one aggregate Protein.
- [ ] **Calendar / time budgeting:** the projected week renders and moving a
  promise recomputes it without storing a duplicate calendar truth.
- [ ] **Health / IoT:** a scale posts weight Facts, a streak program reacts, and
  the source off-switch stops new sampling, use and effects; calibration, clock
  drift, malformed data, offline buffering and actuator interlock are visible.
- [ ] **Apple / pantry recurrence:** confirmed family consumption grows a decayed
  cadence model; projected shortage plus visible nearby OPEN offers yields one
  explained ranked recommendation around the learned window. A matching Trust
  scope plus grant may create a local draft; publishing, agreement and
  settlement each require their own ceiling and capability.
- [ ] **Delegated recurring Transfer:** a person grants one named apple program
  value/quantity/seller/window limits bound to a Trust scope for proposal, own
  agreement, evidence-qualified confirmation and local settlement. It runs end
  to end, while an unlisted/distant/blocked Organ, changed seller/price/revision,
  exhausted budget, missing evidence, or Trust/grant revocation returns to
  Attention **without partial authority.**
- [ ] **Neighborhood matching:** a scoped match rule and visibility grant produce
  a draft in Attention after polling, without widening proximity.
- [ ] **Shared family pattern:** two People publish permitted pantry evidence and
  a signed percentage with exact denominator and window; the consuming Cell uses
  it without exposing hidden members or importing anyone's authority.
- [ ] **Chat / calls:** "when Transfer Y reaches agreed, ask controller X to open
  the room" as a typed single-claim intent.
- [ ] **Interface policy:** a program may present or focus a relevant Record on
  one bound device inside attention and accessibility policy, but cannot click
  agreement, forge input, hide warnings, or take over an unbound sand.
- [ ] **Games / THE Game:** Records provide state and a Karma program provides
  the inspectable rulebook, with no special game automation core.
- [ ] **Garden/farm and inventory/production:** watering and threshold programs
  derive work and Needs, ingest moisture and controller receipts, honor physical
  interlocks; projections distinguish actual/available/planned, and settlement
  remains the only quantity truth.
- [ ] **Operations research:** a week scheduler combines tasks, promises, travel,
  energy preferences, protected time and hard commitments, returns multiple
  plans with constraint/slack and infeasibility explanation, and applies only
  the separately approved schedule Actions.
- [ ] **Monthly recap:** a program selects the month's Facts, drafts the recap,
  and links its evidence **without training on its own output.**
---

## 20. Shared and collective Karma

Why: a person may choose to expose records, evidence, percentages, model
summaries or programs. **Collective Karma is composition of explicitly published
evidence — not a central brain and not a loophole around Protein visibility.**

- [ ] **Publish one of four typed products:** visible raw evidence; a signed
  aggregate; a model/forecast card with stated inputs; or a program template.
  Each carries owner/origin, purpose/terms, audience, visibility, time window,
  concept/unit scope, method/revision, freshness, lineage/hash, signature, and
  revocation/expiry.
- [ ] **A percentage always includes numerator, denominator, eligibility/cohort
  definition, excluded/missing count, time window, unit/concept, method, and
  signature/verification coverage.** "80% of people do X" without those fields
  is invalid input, not Karma.
- [ ] **Apply visibility before aggregation and track input taint through derived
  outputs.** An output cannot be published more broadly than its inputs unless
  an explicit declassification policy proves an allowed aggregate. Small
  cohorts, repeated queries, joins and differencing attacks obey
  minimum-group/query budgets or optional deterministic privacy mechanisms.
- [ ] **A Cell may combine local evidence with permitted remote raw facts or
  signed aggregates** using declared weighting, provenance, freshness and trust
  policy. **Remote claims remain inputs with uncertainty; they do not become
  local Facts about an unseen person merely because they are signed.**
- [ ] **Sharing a live rule means sharing a content-hashed definition or
  template** — never its secrets, private inputs, model checkpoint, grant or
  authority. Installation creates an inactive local revision whose references,
  data scope, budgets and effects must be rebound and proven.
- [ ] **Family and team cooperative patterns work only over scopes each
  participant granted.** A household need can use Ana's and her mother's visible
  pantry evidence, while explanations and outgoing Transfers reveal no more than
  their grants permit.
- [ ] **Revocation stops future export and use and cancels eligible queued work;
  it cannot erase signed data already shared.** Retention and redistribution
  terms stay visible, and downstream models mark revoked or unavailable
  provenance rather than laundering it.
- [ ] **Prove:** recurrence saturation/decay, cadence, confidence separation,
  deduplication, negative/censored evidence, feedback, opt-out, drift, no
  self-training, no private leak, no reputation laundering, and no unauthorized
  automatic commitment.
---

## Appendix A — where the code goes

| Crate | Modules and ownership |
| --- | --- |
| `nucleus` | `karma/{ids,value,ast,dsl,schedule,trace,policy,proof,model,workflow,simulation}.rs` — pure types, parsing, math, graph evaluation contracts, no I/O |
| `store` | `karma/{programs,schedules,occurrences,runs,models,candidates,grants,trust_scopes,workflows,intents}.rs` — typed repositories and transaction helpers; each table owned by a Rust row/input type |
| `engine` | `karma/{supervisor,sequencer,scheduler,evaluator,learning,policy,proof,workflow,effect_worker,simulation}.rs` — orchestration, and the only bridge between pure kernel, Store and runtime ports |
| `protein` | `karma.rs` — union source projection, predicates/includes, visibility/taint-before-aggregate, capability/blocking projection |
| `transport` | Reuse the multiplexed protocol; add only typed Karma payloads. **Never a second socket or a private sand API** |
| `lince` | Start one Karma supervisor per writable Cell and own graceful shutdown. Contains no scheduling or rule semantics |
| `web` | `sand/karma/{mod,body,style,script}.rs` plus `app/{bridge,state,library,builder,why,learn,imagine,authority,queue,health}.js`; host state stores layout only |

**There is no equivalent table for any sand, and the one that used to sit here
was deleted rather than moved** — it assigned `economy/` modules to `nucleus`,
`store`, `engine` and `protein`, which is exactly what the standing rule
forbids. The Karma sand lives in `crates/web/src/sand/karma/` and nowhere else;
what it needs from the backend it gets as a general primitive with a general
name, or it does not get it.

- [ ] **Use one destination subsystem** rather than expanding `karma.rs`,
  `signals.rs`, `senses.rs`, `effects.rs` and `imagination.rs` into parallel
  engines. During migration they may call the new modules as adapters; after
  their behavior is covered, delete the duplicate paths.
- [ ] **The supervisor owns an injected `RuntimePorts` bundle** — wall/virtual
  clock, sleeper/wakeup, deterministic entropy, process, HTTP, filesystem,
  device/UI controllers, secret resolution. **Pure nodes never receive it.**
  Production and simulation differ by port implementation, not business logic.
- [ ] **A small fixed set of long-lived tasks, not tasks proportional to
  Program/Frequency count:** deadline director (durable registration mirror,
  tickless wheel, lane plan, one-shot timers; submits only due batches);
  occurrence sequencer (persist/deduplicate/order, freeze epoch, commit run
  order and reaction closure); pure evaluator pool (prefetch immutable context,
  evaluate in parallel where safe, return deterministic results); learning
  worker (consume completed eligible cursors behind reaction priority, commit
  checkpoints in cursor order); effect worker pool (lease by adapter/capability,
  recheck policy, dispatch, store attempts/receipts); connector supervisor (own
  Signal adapter lifecycles, push captured observations into the occurrence
  path); maintenance worker (repair/checkpoint/retention/replay audit, scheduled
  through the same director).
- [ ] **Channels are bounded and carry stable uids and small commands, not giant
  snapshots.** On receipt a worker reloads authoritative state or uses the
  frozen immutable epoch. **Backpressure parks durable work; it never drops an
  occurrence because an in-memory channel is full.**
- [ ] **Active compiled definitions live in an immutable `Arc<CompiledEpoch>`**
  holding revision/parameter/model/grant/Trust hashes and the dependency index.
  Activation commits Store state first, builds the next epoch, publishes it
  through `tokio::sync::watch`, and enqueues its effective occurrence. Runs
  clone one `Arc`, so no mutex is held across evaluation and **no mid-run edit
  is observable.**
- [ ] **SQLite constraints, not process memory, guarantee correctness.** Unique
  keys for source occurrence identity, `(program_revision, occurrence,
  correlation)` run identity, schedule batch identity, Action request replay,
  candidate dedupe, and intent idempotency. **Never hold a database transaction
  while awaiting a person, network, process, model or device** — commit the
  intent first and reconcile the receipt in a later transaction.

**Where the kernel types actually live:** `nucleus::karma::cadence` (`Cadence`,
`Cadence::between`, `Cadence::civil_at`, `Cadence::ordinal_of`),
`nucleus::karma::calendar` (`CivilDateTime`, `CivilTime`, `CalendarSchedule`,
`TimeZoneProvider`, `GapPolicy`, `FoldPolicy`), `nucleus::karma::schedule`
(`ElapsedSchedule`, `ScheduleCursor`, `OccurrenceRange`, `ScheduleSpec`,
`RationalRate`, `ScheduleDemand`, `ScheduleWorkloadUpperBounds`,
`SchedulerCalibration`, `ScheduleDemandCapacity`, `OverloadPolicy`),
`nucleus::karma::frequency` (`FrequencyAst::compile`, `CompiledFrequency`),
`nucleus::karma::dsl` (`format_program`, `parse_program`, `format_frequency`,
`parse_frequency`), `nucleus::karma::state` (`IntentStatus`),
`nucleus::karma::timezone_artifact`, plus `evaluate.rs`, `value.rs`
(`ValueType`, `LiteralValue`), `proof.rs` and `store::exact`. Durable tables:
`karma_program`/`_revision`/`_request`, `karma_frequency`/`_revision`/
`_activation`/`_request`, `karma_frequency_consumer`, `karma_schedule_cursor`,
`karma_deadline`, `karma_grant`/`_revision`, `karma_intent_event`/`_state`.
Evidence Fact types: `ProgramMutationEvidence`, `FrequencyMutationEvidence`.

**Known pre-existing failures, not caused by this work:**
`promise_lifecycle_through_actions` fails on the transfer WIP's "trusted local
transfer action requires an acting Person" check, and the debug build of the
Action dispatcher needs more than the default 2 MiB test stack (raised in
`.cargo/config.toml`).

## Appendix B — the old vocabulary, mapped

The condition → consequence pair the diary used is **too small as a destination
abstraction**: it conflates recognition, inference, authority and execution. It
remains useful shipped machinery and becomes a **legacy importer** into the
typed graph — not a second engine. These words are DSL sugar over typed nodes
and Actions; the wire never executes text.

| Diary concept | Canonical type | DSL | Durable effect |
| --- | --- | --- | --- |
| Karma | Program, `prog:@slug` | `program`, `when`, `act` | Program Record + immutable revision; activation selects one |
| Condition | Typed expression/recognizer | `let`, `when`, `sense` | Pure by default; a materialized value is an explicit Fact |
| Operator | Gate node | `== != < <= > >=`, `and/or/not`, `crosses`, `enters`, `leaves` | No domain change; records transition state when stateful |
| Consequence | Candidate or intent | `recommend`, `draft`, `ask`, `act` | Inert candidate/decision or an authorized intent |
| Delivery | Occurrence + Run | `on fact`, `on every`, `on signal`, `run` | Occurrence/run/trace plus resulting candidates, intents, receipts, Facts |
| Frequency | Schedule, `freq:@slug` | `every 1d`, `at 08:00`, `after 250ms` | Stores schedule/anchor/cursor; a due boundary creates an occurrence and does **not** edit a Record timestamp |
| Sum | Aggregate/feature node | `sum`, `count`, `avg`, `rate`, `window` | Pure unless explicitly `emit`ted; the exact input Fact set stays explainable |
| Command/query | Signal when reading, effect when acting | `input … = signal`, `do command`, `do http` | Capture creates observation Facts; execution creates intent → attempts → receipt → provenance Fact |
| Karma category | Program tags/scope | `tags`, `purpose`, `scope` | Metadata only; **tags do not confer permission** |
| Calendar/Graph/Orchestra | Flow Plane + Imagination + optimizer | `sim`, `project`, `solve` | Simulation/analysis runs and candidates, never real-world state wholesale |
| Ask/Agent/Tinkerer | Route policy | `observe`, `suggest`, `draft`, `ask`, `act` | Selects trace-only, recommendation, decision, or authorized intent |
| Senses | Recognizer, `sense:@slug` | `sense name = …` | Evidence-backed candidates; cannot write or contact by itself |
| Learning/growth | Model, `model:@slug` | `learn … using …`, `predict` | Model checkpoint/update evidence; cannot mutate a Program |
| Learned-rule promotion | Revision candidate | `revise from template`, then `ask` or delegated `act` | Proven/shadowed revision candidate; activation is a later occurrence |
| Rule changing rule | Meta-control candidate/intent | `tune`, `revise`, `pause`, `resume` | Parameter/revision/activation data effective only for later occurrences |
| Recommendation | Candidate, `cand:r_…` | `recommend "…"` | One lifecycle-managed recommendation with evidence and preview |
| Attention/whisper | Decision/delivery | `ask`, `whisper via …` | Decision is durable; channel attempts are receipts. **Delivery never answers it** |
| Imagination | Simulation run, `sim:r_…` | `project`, `branch`, `assert`, `sim` | Isolated run/trace/bookmarks only; applying uses separately reviewed Actions |
| Workflow | Workflow instance, `flow:@slug` | `step`, `parallel`, `wait`, `retry`, `compensate` | Node position/waits/intents; domain changes still use Actions |
| Optimization | Objective/solve run, `obj:@slug` | `solve`, `require`, `minimize`, `maximize` | Ranked plans and explanations; applying a plan is separate |
| Authority | Delegation grant, `grant:@slug` | `require grant`, `budget` | Signed grant/narrow/revoke; **no program enlarges its own grant** |
| Automation Trust | Local scope, `trust:@slug` | `trust`, `allow/deny`, `any/all`, `ceiling` | Concept/stage/person/Organ/proximity gate; never changes probability, visibility or another Person's authority |

## Appendix C — worked examples

These are **interface specifications**, not promises that the current parser
accepts them.

**A daily habit re-arms itself.** A calendar Frequency at a parameterized local
time; when the task's quantity is zero, an authorized `record.set_quantity`
intent sets it to `-1`. If it is already negative the run records `false` and
changes nothing. Completing the task later sets it to zero through the ordinary
user Action — **the program never owns a private "completed" boolean.**

    program habit.meditate {
      owner person:@ana
      param reminder_time: civil = 07:00 America/Sao_Paulo
      on every calendar 1d at reminder_time missed coalesce id daily_tick
      input task: ref<record> = record:@meditate
      when quantity(task) == 0 {
        act record.set_quantity { record: task, quantity: -1 }
        require grant:@habit.local_records
      }
    }

**A rule changes another rule's schedule.** `grant:@recovery.manage_reminder`
permits only `karma.manage.parameter` on one Frequency, one parameter, range
`[1d, 3d]` — no edits, activation, data scope or effects.

    frequency recovery.reminder_tick {
      param interval: dur = 1d range [1ms, 30d]
      every elapsed interval
        anchor 2026-07-21T09:00:00.000Z
        rephase preserve_anchor
        missed coalesce
      timer { resolution 1ms  max_lateness 30s  coalesce_window 5s }
    }

    program recovery.adapt_frequency {
      on fact(record_eq: record:@recovery.score) queue id score_changed
      input scores: list<fact> = facts(record:@recovery.score, window: 14d)
      sense stable: bool = all(scores.last(7d), value >= 8)
      when enters(stable) {
        tune freq:@recovery.reminder_tick param interval = 3d
          rephase preserve_anchor
        require grant:@recovery.manage_reminder
      }
      when leaves(stable) { tune … interval = 1d … }
    }

The ordering that matters: a score Fact at cursor 200 is evaluated with
parameter revision 7 (`1d`); the meta-rule produces a tune intent;
`set-karma-parameter` commits at cursor **201** as revision 8 (`3d`) and
recomputes the next boundary from the original anchor; learning for cursor 200
runs after its reaction work and changes neither run retroactively; later
occurrences use revision 8, and any occurrence already ordered before cursor 201
uses revision 7. Cursors establish deterministic order when a score Fact and a
timer boundary share a millisecond.

**Learning cannot judge its own evidence.** `model:@apple.need` sits at
checkpoint 12 with `0.69p`. A confirmed consumption Fact arrives at cursor 500:
the reaction lane runs the restock program against **checkpoint 12**, so the
`0.72p` suggestion gate stays false. After the reaction closure the learning
lane admits the Fact and creates checkpoint 13 at `0.74p`. Crossing the route
threshold creates a pattern-threshold occurrence at a **later** cursor, which
evaluates against checkpoint 13 and creates one recommendation. If the higher
draft threshold and grant later pass, the candidate is a local Transfer draft —
publishing, inviting, agreeing, confirming and settling still need their own
capabilities. "Replay cursor 500 under checkpoint 13" is offered as simulation;
applying any difference is a new Action.

**A sensor waters a garden.** Each packet appends a raw observation Fact on the
Signal Record; calibration creates a derived value in the run. The `holds` node
persists transition state. On entering true, policy checks device, bed,
duration/rate budget, freshness and interlocks before creating an intent. **The
controller acknowledgement is a receipt: it proves the command was
acknowledged, not that water physically flowed.** A later moisture observation
or flow sensor may independently prove the outcome and train a model.

    program garden.water_bed_1 {
      on signal sig:@garden.soil_moisture coalesce by bed_id every 500ms
      input moisture: datum<qty<percent>> = signal sig:@garden.soil_moisture
      sense dry: bool = moisture < 22% holds 10m
      when enters(dry) {
        do device:@garden.controller water { valve: "bed-1", duration: 20s }
        require grant:@garden.water_bed_1
        on stale ask "Soil sensor is stale; watering was not started"
      }
    }

**The full apple restock program, and the Trust scope that bounds it.** The
first branch explains and suggests; the second may create an editable local
draft **only if** the seller matches the named Trust scope and that exact grant
exists. Publishing, addressing the seller, agreeing, confirming and settling are
different ceilings and capabilities needing their own nodes and grants. **No
model or LLM is necessary** — the recurrence model, inventory projection, offer
query and optimizer are all deterministic. `offers` is the visibility-gated
Protein view over local and permitted discovery-cache OPEN Contributions from
other Cells; source freshness, signature status, proximity ceiling, unit, window
and missing route data stay **visible inputs**, never hidden ranking behavior.

    program household.apple.restock revision 4 {
      owner person:@ana
      mode active
      on fact(concept_in: @apple) coalesce by household every 5m
      on schedule @hourly

      input pantry: datum<qty<kg>> = view:@family.apple.inventory freshness 2h
      input outcomes: list<fact> =
        view:@family.apple.confirmed_consumption window 180d
      input offers: list<transfer> = protein {
        source: transfer, open: true, concept_in: @apple,
        near: { place: @home, max: 5km }
      }

      learn need: model<recurrence> = recurrence.beta_cadence {
        evidence: outcomes, prior: beta(1, 1), half_life: 90d,
        cadence: weekly(local_tz), min_effective_samples: 5
      }

      let shortage: bool =
        project.quantity(pantry, at: need.next_window.end) < 0kg
      solve seller: estimate<ref<transfer>> from offers lexicographic {
        require compatible_unit && window_overlap && route_eta <= 25m
        minimize expected_total_cost
        minimize route_eta
        tie_break transfer.uid
      }

      when shortage && need.probability >= 0.72p && need.confidence >= 0.65c {
        recommend "Apples are likely needed this week"
          dedupe need.pattern_window
        preview transfer.draft_local from seller
          quantity need.expected_quantity
      }
      when shortage && need.probability >= 0.90p && need.confidence >= 0.85c {
        draft transfer.draft_local from seller quantity need.expected_quantity
        require trust:@apple.known_sellers at draft
        require grant:@apple.local_drafts
      }
    }

    trust apple.known_sellers {
      owner person:@ana
      concept exact concept:@apple
      direction buy
      deny {
        origin_organ in [organ:@blocked.market]
        person in [person:@seller.with.dispute]
      }
      counterparty any {
        origin_organ in [organ:@family.coop, organ:@neighborhood.market]
        all { proximity <= 2; via_organ in [organ:@trusted.relay] }
      }
      stage suggest require probability >= 0.70p confidence >= 0.60c
      stage draft   require probability >= 0.85p confidence >= 0.75c
      stage propose require probability >= 0.92p confidence >= 0.85c
      ceiling propose
      quantity in [0.5kg, 5kg]
      value <= 50  per 7d
      window local [07:00, 20:00]
      expires 2026-12-31T23:59:59.999Z
    }

**How that resolves.** An OPEN apple offer from `organ:@random.shop` at
proximity 4 cannot be drafted or proposed even at `0.99p`. From the family
co-op, the first `any` arm matches. Through the trusted relay at proximity 2,
the second `all` arm matches. **The blocked market is denied even when another
arm would allow it.** The creation Action is typed, not a free-form policy
string: `create-automation-trust-scope` carries `request_id`, slug, purpose,
program uid, concept uid, `include_descendants`, direction, `stage_ceiling`, a
structured `selector` with separate `deny` and `allow` trees (`any`/`all`/
`origin_organ_in`/`via_organ_in`/`proximity_at_most`), per-stage
probability/confidence thresholds as fixed-point strings, `max_quantity` and
`max_value` with `unit_uid` and `window_ms`, and `expires_at`.

**Someone else's statistic is a prior, not a fact about you.** The imported
product records numerator, denominator, cohort/window/method, signature
coverage, visibility and freshness. It can influence a declared prior but cannot
install the publisher's rule, reveal hidden members, grant Transfer authority,
or become a Fact that Ana herself needs apples.

    input family_rate: datum<aggregate<prob>> = published @family.apple.weekly {
      require signed  require denominator >= 5  freshness 14d
    }
    learn need = recurrence.beta_cadence {
      local_evidence: view:@family.apple.confirmed_consumption,
      external_prior: family_rate.value weight 0.20,
      never_train_remote: true
    }

**A program may offer a screen, not drive it.** This creates a presentation
intent and a device receipt. It may show or focus an "Open room" control, but
cannot click agreement, answer a Decision, execute arbitrary JavaScript, hide
warnings, or claim Ana joined. Opening the room is a separate bound controller
Action; attendance is later evidence.

    when enters(transfer_state(transfer:@band.rehearsal) == agreed) {
      do ui:@ana.phone present {
        surface: "call-room", subject: transfer:@band.rehearsal, mode: "offer"
      }
      require grant:@ui.call_offer
    }

**A solver proposes, a person chooses.** The run stores variables, constraints,
objective values, alternatives, slack/infeasibility and the deterministic
tie-break, then creates a Decision with three exact Action previews. **Nothing
reserves time until Ana selects an option and the current world revalidates.**

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
    ask "Choose a proposed week" options plan preview action:@calendar.apply_plan

---

## Extracted 2026-07-29 — what used to be filed under Karma

This file was the one living document, so everything landed in it. The sections
below were whole, self-contained features that only shared a file with Karma;
they now have their own.

The Karma pillar above **was** a mixed block — Perception, Deliberation,
Attention, Imagination, Effects and the Ledger slices interleaved, filed by the
phase they were written in rather than by what they build. It was reorganized on
2026-07-29 into the ordered blocks above: every statement is a task, blocks are
in build order, and each is meant to be built once. The old `K*` and `E*` phase
labels survive as parenthetical tags so commit history stays findable.

| Moved to | What it holds |
| --- | --- |
| `docs/Central: Protein.md` | The read contract: six sources, one JSON shape, predicates, includes, aggregation after visibility, saved Proteins as records. |
| `docs/Central: Sync and Organs.md` | Organ introduction/adoption, contacts and proximity, the visibility-gated outbox, import hardening and quarantine, discovery, File Sync to markdown on disk. |
| `docs/Central: Trust.md` | Signing on write, origin signatures surviving import, verifiable archives, and the standing refusal of any global reputation score. |
| `docs/Interface: Board.md` | The shipped web surface: one WebSocket bridge, Rust-canonical sands, groups, host card state, the Data panel, the shared slash-block editor, permissions. |
| `docs/Sand: Index.md` | Status roll-up of every shipped and planned sand. Individual specs stay in their own `Sand: *.md`. |
| `docs/Future Instincts.md` | OSM place data, time-varying unit conversion, storage-engine independence, and the product surfaces beyond Transfer. |
| `docs/Parked.md` | The two items that need the user before anyone can act on them. |
| `docs/Acceptance Workflows.md` | The Window: end-to-end workflows that prove the pieces compose. |
| `docs/Maneirisms.md` | The cross-cutting standing laws of the whole system. |

Two concepts that were *scattered* through the Karma text rather than sitting in
a section of their own also moved, and this file now points at them where it
needs them:

| Moved to | What it holds |
| --- | --- |
| `docs/Central: Lingua.md` | The concept DAG and unit model: many parents, `record_concept` classifications, exact rational conversion factors, and why conversion has no time dimension. |
| `docs/Central: Senses.md` | The pure recognizer: match rules, the discovery join, `confidence()` and `demand()`. |
