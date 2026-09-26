# Simulation, Karma, Frequency, and shared time

Reviewed 2026-09-20 against the current working tree, including uncommitted work. This is a source review and an architecture proposal, not a claim that the proposed features already run. No build or runtime tests were run for this document.

The owner requested **Simulation** as Resenha's new name and one feature joining Simulation with Karma and Frequency. The [Record](Lince.lingua) still says Resenha; this document uses the requested name without editing that Record. Other differences between the Record and the implementation are listed below.

## Recommendation

Build one **Timeline** primitive: a scope with explicit time, ordered events, and saved progress. Live execution follows the host clock. Simulation advances the same engine through an isolated copy of state. Frequency produces scheduled events; Karma and Transfers consume events through their normal rules and authority checks.

Keep the familiar concepts: a Frequency says when, a Rule says what to do, and a Simulation explores what happens. They share execution machinery. A Transfer keeps its agreement, claim, and settlement rules.

Start by extracting the existing Program deadline director and its clock contract. Replace the separate forecast calculations with runs of this shared engine as coverage grows.

Exact recurrence means exact intended times, stable occurrence identities, and declared handling of delays. It cannot promise that an ordinary operating system executes a command at an exact physical millisecond.

## What exists

### Karma rules

| Area | Current code | Limit or unfinished part |
| --- | --- | --- |
| Recurrence rule | Condition, gate, carried value, ordered consequences, cadence, pause state | Separate from Programs |
| Condition | Arithmetic, quantities, tag totals, Signals, Frequency counts, historical totals, `value()`, and other readings | `value()` still recursively reads another rule; the Record asks to remove it |
| Gate and carry | Nonzero, always, or comparison; carry the result, one, or a constant | The simple rule does not expose all Program controls |
| Consequences | Capture entries, set/add quantities, change assertions, propose Promises, ask, notify, commands, saved queries, Actions, visibility | Outward effects use a separate queue |
| Reactions | Record changes can wake dependent Recurrence rules | Runtime reaction budget is 256 checks; this is not a general proof of termination |
| Program | Typed graph, exact arithmetic, unit checks, parameters, definition hashes, validation, activation, pause | The Rust interface has no dedicated Karma authoring surface |
| Program controls | Delay, threshold with separate enter/leave events, hysteresis, debounce, cooldown, rate limit; saved state | Evaluation updates state but does not return timer registrations for future control deadlines |
| Program inputs | Parameters, Record quantities, saved Protein views reduced to one number | Signal and captured-Fact inputs are represented but unresolved by the stored-run reader |
| Program outcomes | Observe, recommend, draft, ask, act candidates; accept/dismiss/snooze | These routes do not themselves execute an effect |
| Authority | Grants constrain capability, target, Program revision, validity, and budget; acceptance can authorize an intent | Authorized Program intents still have no executor |
| Replay | Frozen evaluation inputs, saved state, limits, expected results, and content hashes | Replays one Program evaluation, not a whole Cell |

Sources: [conditions](../crates/nucleus/src/karma/condition.rs), [consequences](../crates/nucleus/src/karma/consequence.rs), [Program nodes](../crates/nucleus/src/karma/ast.rs), [validation](../crates/nucleus/src/karma/proof.rs), [evaluation](../crates/nucleus/src/karma/evaluate.rs), [stored runs](../crates/store/src/karma/runs.rs), [intents](../crates/store/src/karma/intents.rs), [replay](../crates/nucleus/src/karma/replay.rs).

### Frequency and time

There are two scheduling paths:

| | Recurrence | Program Frequency |
| --- | --- | --- |
| Definition | `recurrence` cadence and the separate `frequency` table | Versioned Frequency compiled into an elapsed or calendar schedule |
| Driver | Cell heartbeat every 60 seconds | One director waits for the earliest deadline |
| Frequency reading | `freq(@name)` counts beats in `(since, at]`; it can exceed one | A durable occurrence identifies a scheduled beat or batch |
| Progress | Occurrence application records; 60-day catch-up lookback, at most 64 applications per rule per tick | Persisted cursors, leases, fencing, occurrence expansion, Program runs |
| Sharing | Conditions can read a named Frequency | Several Programs share one Frequency cursor and beat |
| Result | Applies consequences and drains queued effects | Evaluates Programs and produces candidates |

Existing calendar math supports component steps from years to milliseconds, anchor-based calculation, weekday landing, count/end limits, and invalid-day handling. Civil schedules also define daily, weekly, and monthly times.

The Program scheduler already declares:

- Missed beats: skip, coalesce, bounded replay, or pause on lag.
- Inactive periods: skip to the next anchor or follow the missed policy.
- Step changes: preserve the anchor, use the last intended instant, use the change instant, or fire immediately when overdue.
- Overload: reject activation, pause and ask, or degrade within the grant.
- Timer resolution, lateness, coalescing windows, and resource admission.
- Clock discontinuities and recovery of expired leases.

Timezone artifacts are pinned by revision and content hash. Missing local times can skip, shift forward, or pause. Repeated local times can select the first, second, both, or pause. The default Cell installs only the UTC artifact. A file loader exists, but a complete timezone database is not shipped through that default setup.

The basic cadence enumerator returns at most 512 dates and marks truncation. The Program path has separate bounded work and expansion limits. Neither makes a long, dense simulation free. Host defaults also leave several demand budgets effectively unlimited; admission infrastructure is not a measured performance guarantee.

Sources: [heartbeat](../crates/engine/src/lib.rs), [Cell startup](../crates/cell/src/lib.rs), [Frequency reading](../crates/engine/src/actions.rs), [cadence](../crates/nucleus/src/karma/cadence.rs), [schedule policies](../crates/nucleus/src/karma/schedule.rs), [calendar](../crates/nucleus/src/karma/calendar.rs), [director](../crates/engine/src/karma_runtime.rs), [durable schedules](../crates/store/src/karma/schedules.rs), [timezone setup](../crates/engine/src/karma_timezone.rs).

### Simulation and Transfers

The Resenha Record asks for seeded runs, several Cells, time/network/disk control, invariants, user-data scenarios, and sharing proposed changes for others to simulate. This full runtime does not yet exist.

There are useful pieces:

- `imagination` builds a snapshot and projects quantities from Recurrence rules and Promises. It uses floating-point quantities, selects one quantity movement per rule, supports only some inputs, and skips failed evaluations. It is not the live engine running in isolation.
- Protein's Timeline separately combines Facts with expected Recurrence and Promise amounts. It reads the host clock and is another projection path.
- Program replay capsules reproduce a frozen evaluation.
- Director tests contain a manual clock and test discontinuities, admission, leases, and recovery.
- Transfers already have revisions, agreements, occurrences, claims, settlement, idempotency, delivery queues, and receipts. These operations should run unchanged inside a Simulation scope. Current delivery workers still poll every five seconds and read host time directly.
- Sync already has import paths with grant, scope, timestamp, and signature checks. Previewing an incoming operation should reuse the applicable receiving path.

Sources: [Simulation intentions](Lince.lingua), [projection engine](../crates/nucleus/src/imagination.rs), [snapshot builder](../crates/engine/src/imagination.rs), [Protein timelines](../crates/protein/src/lib.rs), [clock tests](../crates/engine/tests/karma_runtime.rs), [Transfer storage](../crates/store/src/transfers.rs), [delivery worker](../crates/cell/src/transfer.rs), [sync import](../crates/engine/src/sync.rs).

### Differences from the Record

- The binary Frequency description is the intended rule interface. The Recurrence reader currently counts beats.
- Several unchecked tasks already exist in Programs: threshold transitions, debounce, cooldown, rate limits, saved Protein inputs, missed policies, rephasing, and admission. They are not uniformly connected to simple rules or the interface.
- “Time changes no quantity” applies to the unfinished Program intent path. Recurrence already changes quantities.
- Ask and emit-Promise already exist in Recurrence. Notification effects are queued and recorded; that does not establish a complete user notification experience.
- The checked shell-command description promises executable hashes, typed arguments, and timeouts. The current effect runner uses `sh -c` and captures output without that contract.
- `.lingua` parses Frequency and Karma declarations, including the [five-second toggle](../trails/karma-toggle.lingua). I found no current runtime importer consuming those projected declarations.
- The old Karma Sand belongs to the unplugged web interface. Its existence does not provide a Rust Karma editor.

Sources: [Record](Lince.lingua), [effect runner](../crates/engine/src/effects.rs), [Lingua projections](../crates/anicca/src/lib.rs), [file sync](../crates/engine/src/file_sync.rs).

## The shared primitive

### Time has different meanings

Use one contract with distinct types. Combining these values into one `now` would create incorrect expiry, ordering, and recurrence behavior.

| Value | Meaning | Used for |
| --- | --- | --- |
| UTC instant | A date on the wall clock; may jump | Appointments, validity windows, recorded receipt times |
| Elapsed duration / monotonic deadline | Time passing within a running clock epoch | Sleep, timeout, backoff, active worker leases |
| Civil date and timezone | A person's local calendar choice | Every Friday at 09:00, monthly on the 31st |
| Intended event time | When a beat was scheduled | Recurrence identity and lateness |
| Processing sequence and receipt time | When this Cell learned of and processed something | Deterministic replay, late input, current authorization |
| Causal stamp | Ordering evidence carried by sync | Merge decisions; never a timer deadline |

Keep integer milliseconds for the initial semantic contract, reusing the existing checked timestamp/duration types. Declare supported ranges and reject overflow. Declare the leap-second policy explicitly. Increasing precision later should require a concrete need.

Wall time and monotonic time already differ in Rust: [SystemTime is not monotonic](https://doc.rust-lang.org/std/time/struct.SystemTime.html). A duration timeout must not suddenly expire or gain an hour because a clock was corrected. Durable recovery needs an explicit downtime policy because a process-local monotonic instant cannot survive a restart.

### Timeline responsibilities

The Timeline owns ordering, deadlines, cancellation, bounded progress, and the record of events. Domain handlers own what an event means.

Conceptual interface:

```text
register(deadline, owner, revision, payload) -> token
cancel(token)
deliver(input, origin, receipt_time)
next_deadline() -> optional deadline
step(budget) -> progress, waiting, blocked, or exhausted
advance_until(target, budget) -> run report
```

These are proposed operations, not existing APIs. `advance_until` belongs to a Simulation controller. Live callers do not gain the ability to change the Cell's clock.

Each execution scope has its own clock state, pending work, store, IDs, causal clock, input stream, and effect adapters. The driver supplies an immutable context to each operation. Functions beneath it receive the context or explicit values rather than reading ambient time.

| Consumer | Registers or receives | Domain handler does |
| --- | --- | --- |
| Frequency | Next intended beat | Emits one identified occurrence and computes the next |
| Karma | Facts, beats, control deadlines, decisions | Evaluates the same rule and requests authorized actions |
| Transfer | Window boundaries, delivery, receipts, retry deadlines | Checks agreement, claims, expiry, and settlement |
| Promise / decision / grant | Expiry or renewal deadline | Performs its normal transition or refuses it |
| Signal | Sampling deadline and sample result | Stores evidence before rules read it |
| Sync | Message delivery, retry, lease expiry | Runs the existing receiving and merge rules |
| Simulation | Scenario inputs, faults, target horizon | Drives these same handlers in isolated state |

UI animation and rendering can keep frame time. Time that changes persisted behavior, permissions, or observable protocol decisions must use the execution scope. This boundary keeps the primitive focused.

## Execution rules

### One driver, replaceable environment

Refactor the director into a bounded step that processes ready work and returns the next wait. The production adapter waits on Tokio and real input. The Simulation adapter chooses the next event and advances virtual time to it.

An advance through one month visits each meaningful event in order. It drains work created at the current instant before moving to the next. A cancelled or revised timer is ignored using its generation/revision. Deterministic budgets bound events, same-time reactions, evaluations, writes, and trace size. A host-time watchdog can interrupt a run, but that interruption is not a simulated outcome or a completed result.

Advancing normally and simulating a Cell being offline are different operations. Offline simulation stops that Cell, lets the world continue, then exercises its actual missed-beat and recovery policies.

An earlier event must never be inserted before its cause. Order available work using an explicit queue key such as `(ready_at, sequence)`, with stable ordering when registering equal-time deadlines. Persist the order of external inputs and completions. A late message keeps its original timestamp as evidence but runs at its receipt position. DST tests can vary eligible event order using a seeded scheduler and record the chosen order for replay.

Generalize the existing durable occurrence sequence and revision checks instead of creating an unrelated queue. Unify timer indexes and semantic execution; domain records and protocol state can retain their own tables. Keep one semantic commit order per Cell; asynchronous work reports a completion event before its result can change that state.

Tokio's paused clock remains useful for adapter tests. It is insufficient as the product primitive: it does not pause standard-library time, and a large `advance()` can make several timers ready together without defining their processing order. See [pause](https://docs.rs/tokio/latest/tokio/time/fn.pause.html) and [advance](https://docs.rs/tokio/latest/tokio/time/fn.advance.html).

### Exact Frequency behavior

- Calculate the nth beat from the anchor and pinned definition. Do not add a period to the last actual wake.
- Give a beat an identity containing the Frequency, activation/revision, and occurrence position. Calendar identities must distinguish the two instants of a repeated local time.
- Deduplicate each consumer's application with that identity. Save the state change and progress atomically.
- Make `freq(@x)` return one for the matching occurrence and zero otherwise. Put a coalesced count/range in explicit input fields; do not silently turn one rule evaluation into several.
- A late beat carries both intended time and receipt/processing time. Catch-up must declare whether it reads current state or reconstructed historical state. Current-state catch-up cannot claim equivalence to punctual historical execution.
- Freeze or version the inputs used by one occurrence's Program evaluations. Shared consumers must not read an accidental mixture of database states because work was paged.
- A debounce boundary needs its own cancellable deadline. A changed input replaces that deadline. Cooldown expiry only causes an evaluation when the rule's declared behavior requires one.
- A clock jump changes the clock mapping and rebuilds affected wall-time waits. It does not rewind completed work. The live adapter needs a clock-change signal or bounded reconciliation; detecting drift only after a long sleep is too late.

Keep “every 24 hours” separate from “daily at 09:00 locally.” Resolve civil rules through a pinned timezone artifact, with explicit gap, fold, and invalid-date policies. A timezone update is a definition decision; it must not silently rewrite an existing run.

### Authority and effects

Use the same checks in live and simulated runs. Recheck authority at actual execution time: an old intended timestamp must not revive an expired or revoked grant. State-changing operations still need the normal actor and scope.

Commit local state and an outgoing effect request together. Deliver effects through adapters, with stable request IDs, attempts, and receipts. Local application can be deduplicated; an external command cannot promise exactly-once behavior without cooperation from its receiver.

Simulation records an effect request and supplies a recorded or declared response. Missing responses become unknown or blocked outcomes. It cannot call real shell commands, contacts, payment endpoints, or production key stores. Loading a branch must not automatically start the live Cell supervisors.

## Simulation runs and incoming operations

A run needs a consistent base snapshot, scenario, clock settings, horizon, limits, code/schema revision, timezone artifact, policy definitions, and input/effect fixtures. A seed controls generation and ordering; it is not enough to reproduce a run if those other inputs change.

Use an isolated SQLite snapshot first so the ordinary store and operations remain reusable. Capture the database and its base identity consistently; include referenced documents and blobs by hash. Use SQLite's [backup facilities](https://www.sqlite.org/backup.html), not a raw copy of an open database file. Protect the local copy as user data and exclude production credentials from the runtime adapters.

Rewind means restore a checkpoint and replay. A user-facing speed control changes how quickly virtual events are presented, without changing their semantic order.

To preview accepting somebody else's operation:

1. Capture the receiving Cell's state and the proposed operation, sender, scope, preconditions, and intended delivery position.
2. Fork two scopes from the same base: one without the operation and one receiving it through the appropriate authenticated import or Action path.
3. Run both to the same horizon with the same declared external inputs. Record refusals, changed Records, triggered rules, Transfer obligations, requested effects, and broken constraints.
4. Show the difference, including unknown outcomes. A positive result is evidence for that scenario, not approval or a prediction of another person's choices.
5. If the person chooses to apply it, revalidate the real state and permissions and submit the original operation through the live path. Do not merge simulated Facts or database rows into production.

Use domain-specific operations in a common scenario envelope: a CRDT change remains a CRDT change; a Transfer acceptance remains a Transfer acceptance. Arbitrary database diffs would bypass those meanings and checks.

Shared scenarios contain proposed operations, preconditions, declared assumptions, and deliberately disclosed evidence. Each recipient runs against their own state. Bind a review to the proposal hash, relevant base state, assumptions, horizon, and expiry. Changed state makes the result stale and requires revalidation. Private data and hidden peer state remain unknown unless disclosed.

Scenario comparison should share external inputs by stable identity, so one extra internal event does not shift every later random choice. Give randomness separate streams per Cell and purpose.

## Deterministic simulation testing

Time control is the first dependency. Full DST also needs control of every nondeterministic input that can affect the result.

| Boundary | Current issue | Proposed treatment |
| --- | --- | --- |
| Time | Direct `Utc::now`, OS instants, sleeps, and storage timestamp generation occur outside Karma | Inject the execution scope and audit remaining escapes |
| IDs | `new_uid()` reads host time and random UUIDs | Inject an ID source; deterministic IDs in isolated runs |
| Causal ordering | `nucleus::hlc` uses a process-wide atomic and host time | One persisted causal clock per Cell/scope, restored with checkpoints |
| Scheduling | Tokio task races can change observable order | Control semantic input/completion order; replay the recorded schedule |
| Storage | SQLx/SQLite performs real I/O | Use isolated databases for functional runs; add explicit storage fault coverage separately |
| Network | Iroh and workers wait on real sockets/time | Adapter for logical deliveries, delays, drops, duplication, partitions, and reconnects |
| External work | Commands, models, APIs, and files can change between runs | Recorded fixtures or deterministic models with declared uncertainty |
| Collections and queries | Unordered results can affect decisions | Stable ordering and canonical output hashes |

Sources for present escape points: [IDs](../crates/nucleus/src/id.rs), [causal clock](../crates/nucleus/src/hlc.rs), [store](../crates/store/src/lib.rs), [sync writes](../crates/store/src/sync_apply.rs), [sync worker](../crates/cell/src/sync_runner.rs), [effects](../crates/engine/src/effects.rs).

For several Cells, the harness owns a world timeline and each Cell has its own wall-clock offset, elapsed clock, and causal state. The world decides when messages and I/O completions arrive. Cells only observe their local clocks and delivered inputs. Never use the harness's global order as a new production consensus rule.

Initial DST can test domain behavior, message delivery, retries, process restarts, and failures around transaction boundaries using real isolated SQLite. This does not test torn database pages or fsync correctness. That needs a SQLite VFS/storage harness or an external fault-testing environment. Wrapping Lince's file helpers alone cannot intercept SQLite's disk access. Likewise, simulated messages do not establish real QUIC or NAT-traversal correctness.

Use independent invariants: no duplicate settlement, conservation where a Transfer requires it, no effect outside authority, no cross-scope writes, and completed progress surviving a restart. Always check critical invariants; make expensive diagnostics selectable. Save failing inputs and scheduler choices, then shrink the scenario for a small replay case.

The Record mentions [Octopii](https://github.com/octopii-rs/octopii). Its documented time/storage/network/randomness abstractions are relevant, but it also provides Raft, its own WAL, and QUIC machinery. My recommendation is to learn from those boundaries and evaluate reusable parts separately. Adopting its complete runtime would add a substantial storage/replication redesign to this work.

## Performance and honest results

Jump between events, not milliseconds. Share one timer per Frequency. Index interested rules instead of scanning every Program for every occurrence. Retain bounded pages and resumable cursors.

Skip repeated evaluations only when an optimization proves the same observable result: no intervening input, threshold transition, deadline, authority change, or per-beat effect. Repeated fixed additions may allow a shortcut; an arbitrary rule graph does not. Check each shortcut against ordinary stepping.

Return one of: complete to horizon, blocked on input, paused by policy, failed constraint, or budget exhausted. Partial results carry their stopping point and remaining uncertainty. A one-second rule over years may require millions of meaningful steps.

Replay guarantees the same result for the same pinned inputs and event order. It does not establish that future external inputs are known. A finished finite run does not prove a rule will never loop or fail. Graph checks can reject immediate cycles; runtime budgets and invariant traces handle wider feedback chains.

Keep exact quantities and explicit units through Simulation. Replace the floating-point projection shortcut. Existing Transfer boundaries that still use floats need an explicit audit before claiming exact results for those paths.

## Where the code belongs

| Location | Responsibility |
| --- | --- |
| `nucleus::time` | Shared checked time values, civil schedules, event/deadline identities, pure advance calculations; extracted from Karma |
| `nucleus::karma` | Rule graph, arithmetic, control state, proof, and evaluation |
| `engine::timeline` | Bounded semantic event driver and execution context, extracted from the director |
| `store` | Durable work, progress, receipts, checkpoints, isolated snapshot support |
| `cell` | Real clock, network, process, and storage adapters; live supervision |
| A `simulation` crate | Run/scenario control, virtual environment, faults, trace, comparison, replay; composes existing engine/store |
| `protein` | Query run results and expose timelines with source, assumptions, and coverage |
| `interface` | Simulation controls, comparisons, and rule/Frequency authoring |

These are proposed placements. Start with modules in the existing core and engine. A lower-level shared adapter crate is justified only if dependency direction requires it. Avoid putting OS access back into the pure time model.

## Sequential build order

1. **Shared time and scope.** Extract the checked time types and clock boundary. Add separate wall/elapsed readings, scoped IDs and causal state, and an operation context. Convert the first vertical's ambient reads. Keep the existing director working with its live adapter.
2. **Bounded Timeline driver.** Extract one step, deadline registration/cancellation, stable ordering, and a virtual controller. Add timer deadlines for temporal controls. Prove ordered advancement and restart recovery.
3. **One executable rule path.** Compile the simple Condition/gate/consequence authoring model into Programs, finish local authorized intent execution, and retire the overlapping Recurrence path after moving its useful behavior. Automatic execution can use an explicitly authorized grant; simulation alone grants nothing.
4. **First complete Simulation.** Run a five-second toggle and a chained quantity rule in isolated state using the same execution path. Add checkpoints, reports, effect fixtures, and a queryable result. Wire `.lingua` declarations through normal Actions if they are used as seeds.
5. **Civil recurrence and limits.** Ship pinned timezone data; cover month ends, leap years, gaps/folds, long advances, lateness, admission, and visible partial results. Reuse existing calendar math and tests.
6. **Transfers and incoming-op preview.** Route windows, retries, expiry, and receiving operations through the scope. Compare accept/reject branches. Revalidate before live application.
7. **Several Cells and DST.** Add controlled delivery, per-Cell clock skew, crashes/restarts, storage-boundary failures, invariants, and minimized replay cases. Add lower-level disk/network testing with an explicit coverage claim.
8. **Unify projections.** Make imagination and Protein future views read Simulation results. Remove duplicate future-state calculations once their consumers use the common path.

The first coding task today should be steps 1–2 for a tightly scoped vertical: the existing Frequency director, one deterministic scope, and ordered `advance_until`. Do not begin with a replacement calendar or a second simulator evaluator.

Recommended interface work, for the owner's approval when implementation reaches it: a Simulation Sand with pause, next event, advance to date, restart from checkpoint, scenario comparison, and a readable explanation of each change. Rule and Transfer screens should open that same run with their context prefilled. Keep live/simulated state visibly distinct. Any embedded Sand dependencies need their licenses and credits.

## Acceptance checks

- A five-second Frequency anchored at zero and first due at five seconds produces twelve distinct beats through sixty seconds, including sixty, whether advanced one event at a time or with one `advance_until` request.
- Normal advancement and declared offline/restart scenarios exercise different policies; each is reproducible.
- Several rules sharing a Frequency produce one scheduled beat with deduplicated consumer applications.
- An input starting a debounce receives its boundary evaluation even if no further external input arrives.
- Same-time events, late input, cancellations, and revocation/expiry at an execution boundary have defined, repeatable results.
- A crash after commit but before acknowledgement does not duplicate a local quantity change or settlement; external effects expose their delivery uncertainty.
- UTC, elapsed time, month-end rules, leap years, timezone gaps/folds, and clock jumps preserve their declared semantics.
- Identical snapshot, definitions, fixtures, scheduler choices, and seed yield the same canonical trace and result. Independent runs do not share causal-clock or ID state.
- An incoming operation is refused identically by preview and the corresponding live receiving checks on the same state. Stale previews never authorize application.
- Simulation leaves live state, real outboxes, contacts, files, and credentials untouched. Missing external responses remain visible.
- Dense schedules and feedback loops stop with a bounded, resumable or diagnostic result; they never return an incomplete forecast as complete.
- Transfer invariants survive delayed, duplicated, reordered, and partitioned deliveries and simulated restarts.

Use `cargo check` with warnings denied and the focused recurrence, calendar, director, replay, authority, and Transfer tests as each step lands. Add parity and invariant tests at the shared boundary; documentation alone does not validate this architecture.
