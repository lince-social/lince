# Simulation: remaining work and implementation plan

Reviewed and updated after the first approved implementation round on 2026-10-12 against the shared working tree.

The checkout's base commit was `e3e57b6ce1aab00f5fc4e864ab56497466cbe561`. Uncommitted signal-endpoint, interface and other changes were being developed alongside this work. Test results apply to that shared working tree, not to the base commit alone.

This is an implementation assessment of the Simulation ideas in [Lince.lingua](Lince.lingua), including the related plugin and location follow-ups. The Record remains the source of truth. This file does not change its declarations or turn its post-v1 ideas into release requirements.

The existing system runs isolated Cells, copies a personal database, executes normal Karma logic, captures evidence, steps and pauses a run, compares local Record proposals and Transfer assumptions, runs campaigns, and produces cached Calendar and Schedule results. Remaining work extends these mechanisms.

The first approved round completed original tasks 1 and 2. Their implementation sections have been removed. **Six main tasks with 39 implementation steps remain.** Original task numbers 3–8 are retained so the next approval can identify them clearly. The next proposed round is tasks 3 and 4; starting it requires the owner's approval, as requested for these rounds.

Reproducibility and the original failure cases are now covered by passing checks. Generated authorities are deterministic in isolated execution, copied databases retain their existing authority, and live bootstrap remains random. The Transfer donation fixture configures both participants' policies before exchanging identities. “Next event” skips validation-only boundaries; date-bounded advancement preserves those boundaries. Failed runs retain their evidence, and replay diagnostics identify the first differing database table, row and column alongside execution state.

Local proposals now retain a bounded source fingerprint, stable Record identities, exact values, revisions and ordered changes. They execute normal authorized Record/text/Work operations. Baseline comparison preserves the same captured source, background events, clock, limits and checks; saved evidence contains protected Record outcomes and is verified by replay. The native Record and Calendar entries provide capture, structured edits, event timing/reordering/removal, horizon controls, draft persistence and live-source status. Executable document bodies require the composition editor through either text action path. Simulation leaves live Records unchanged; applying a proposal is outside this completed workflow.

## What survived the second review

“Required now” means a correctness problem in existing behavior. “Needed for the described feature” means the capability is missing from the idea in the Record, but is not a new v1 requirement. “Conditional” means implementation should start from a concrete unsupported use case rather than expanding everything in advance.

| Point from the first audit | Decision after checking the code again |
| --- | --- |
| Replay and equivalent-run failures | Completed in round 1. The original failure set passes; authority, stepping and replay diagnostics have regression coverage. |
| Simulation of the user's database | Implemented, including the local editing and comparison workflow completed in round 1. |
| Moving an appointment to another day | Completed in round 1 through structured Work edits, Calendar entry and comparison with a retained base. Both date endpoints are validated together. |
| General database diffs and approval chains | Local typed Record proposals and comparison are implemented. General sharing, recipient bindings and approval chains remain task 3. |
| Record Editor scenarios | Completed in round 1. Native drafts use a deliberate mutation allowlist, source/property preconditions and normal authorization. |
| Saved Record versions and history | Partly implemented internally through Loro, revisions and change evidence. Named checkpoints, browsing, restoration and a total retention budget remain needed for the described feature. |
| Constraints with different checking periods | Implemented. The equivalence regression passes; coverage and checking cost remain separate from domain behavior. |
| Preventing a violating commit | A separate semantic extension, not necessary for simulation checks. Current checks preserve the violating evidence and stop afterwards. |
| Controlled external responses | Command responses already exist. Plugin responses are the actual missing adapter. |
| Lingua declaration breadth | Conditional. Extension declarations are rejected explicitly; that does not make the importer defective. Add only a declaration needed by an actual scenario. |
| More seeds and concurrent execution | Seeds and campaigns already exist. Test more meaningful orderings; a literally concurrent simulator is not required. |
| Blob contents in saved runs | Conditional, with a real gap for external blob storage. Description assets stored inside SQLite are already included in database snapshots. |
| Focused Cell/event editing | Local Record event controls are implemented. Advanced scenarios retain JSON editing; broader topology controls need a concrete use case. |
| Calendar links to explanations | Already implemented for originating Rules and read-only occurrence previews. Additional links need a concrete missing explanation. |
| Projection filters and permitted viewers | Real limitations, conditional on supporting those queries/viewers. Safe incomplete results already exist. |
| More time range and workload capacity | Validation work first. Existing limits, timezone tests, large-workload fixtures and measurement tools should be exercised before changing capacity. |
| Replacing older projection approximations | Do not make this a general task. Calendar/Schedule already use runtime projections. Record `include.projection` deliberately says `scenario_required`; it is not an approximate forecast. |
| Transclusion | Already implemented, including limits and removal of inaccessible content. Reuse it. |
| Restore checkpoints during a run and finer cache invalidation | Remain deferred, as the Record says. Neither is a prerequisite for the workflow below. |
| NAT, relay, mailbox and desktop portal behavior | Real validation gap. It belongs in deployment/platform testing, not in a replacement simulation network. |
| Future buildings/resources as spatial overlays | A separate Space feature. It is not a prerequisite for Record, Karma or Calendar simulation. |

## Existing code to reuse

- [Scenario and event definitions](../../crates/simulation/src/scenario.rs), [World execution](../../crates/simulation/src/world.rs), and [saved artifacts/replay](../../crates/simulation/src/artifacts.rs).
- [Personal database preparation](../../crates/desktop/src/simulation_castle/runner.rs) and the [Simulation Castle](../../crates/desktop/src/simulation_castle.rs).
- [Normal Record mutations](../../crates/engine/src/record_change.rs), [Record policy checks](../../crates/engine/src/record_policy.rs), [collaborative text operations](../../crates/engine/src/collab.rs), and [document qualification](../../crates/store/src/record_docs.rs).
- [Karma proposal request/report](../../crates/engine/src/karma_preview.rs) and its [simulation runner](../../crates/simulation/src/karma_preview.rs). These already demonstrate snapshotting, source fingerprints, permission checks and filtering result evidence.
- [Local Record proposals](../../crates/simulation/src/proposal.rs), [general comparison](../../crates/simulation/src/comparison.rs), and [native Record scenario controls](../../crates/desktop/src/simulation_castle/records_ui.rs).
- [Transfer sharing](../../crates/nucleus/src/simulation/sharing.rs) and [sharing controls](../../crates/desktop/src/simulation_castle/sharing_ui.rs).
- [Check definitions and evaluation periods](../../crates/nucleus/src/simulation/checks.rs), [check execution](../../crates/simulation/src/checks.rs), and [saved check sets](../../crates/store/src/simulation_checks.rs).
- [Projection calculation](../../crates/engine/src/projection.rs), [Calendar query execution](../../crates/protein/src/calendar.rs), [Schedule query execution](../../crates/protein/src/schedule.rs), and [Calendar explanations](../../crates/desktop/src/time_castle/events.rs).

## Sequential implementation plan

The correctness gate and local scenarios are complete. Build general sharing on that local proposal meaning, then let history reuse it. Optional dependency adapters follow the same run and evidence contracts. The six tasks below remain sequential and require approval in two-task rounds.

### 3. Share general proposals with results and approval chains

**Why this is needed.** The Record describes sending a change to someone, letting them run it against their own database, and returning results before approval. Current `Shared` messages identify one Transfer revision and carry selected assumptions/check coverage. They cannot express a general appointment/Record change or an approval chain.

**Implementation steps.**

1. Define a typed shared proposal envelope on top of the implemented local Record proposal. Include its content digest, author identity, selected operations, time assumptions, touched public references and the recipient bindings required to run it. Exclude raw SQL, arbitrary filesystem paths and executable configuration.
2. Distinguish shared Records from local resources. A shared Record can keep its agreed identity. A participant's private stock, calendar or other local Record needs an explicit local binding. Never assume the sender's private UID names the recipient's resource.
3. Treat source preconditions per participant. Each recipient simulates from their own snapshot. Their result records the proposal digest, their source fingerprint, limits, selected check set, stop status and disclosed evidence. Different source databases do not make the proposal invalid; changed agreed terms or stale local preconditions require review.
4. Store an explicit proposal lifecycle: draft, sent, received, bound, simulated, approved/rejected, superseded and withdrawn as needed by the implementation. An approval authenticates the exact proposal digest and relevant agreed preconditions. It is distinct from a simulation result, which may be incomplete, and from an eventual live operation.
5. Build forwarding as a chain or graph of references to authenticated approvals. Each participant chooses which content/results may be forwarded. A hidden approval must not be displayed as a verifiable public endorsement. Changed proposal content starts a new revision and invalidates approval of the old content for the new proposal.
6. Reuse existing conversations, authenticated transport, deduplication and disclosure mechanisms. Keep the general proposal type outside Transfer-only disclosure logic. Delivery and receipt of a proposal must not execute its changes.
7. Build native controls for inspecting the exact outgoing fields, mapping local Records, running the proposal, viewing returned results and approving/rejecting it. Explain missing bindings and stale results before approval. Sending is always an explicit interaction.

**Acceptance.** Two Cells with different private databases can run the same proposal with explicit local mappings and return results bound to the same digest. A third participant can forward a chosen approval without exposing unrelated Records. Duplicate delivery is harmless. Altering one operation invalidates the old approval for the altered content. Revoked access removes protected proposal details. Receiving, simulating and approving do not mutate the proposed live Records.

**Performance and storage.** Bound envelope bytes, operation count, participants and forwarded evidence. Keep run artifacts local unless explicitly selected for disclosure. Add append-only storage migrations only for lifecycle data that cannot be represented cleanly by existing Record/conversation storage.

### 4. Expose bounded Record checkpoints and history

**Why this is needed.** Loro already holds title/body operations, and the engine has Record revisions, document qualification, scalar/property changes, facts and limited displaced-change evidence. What is missing is the owner's usable “save this version, inspect it, try it in a scenario, restore it” feature. Reimplementing text history would duplicate working machinery.

**Implementation steps.**

1. Add a checkpoint descriptor containing the Record identity, label, capture time, document generation/revision and enough immutable content or version material to reconstruct the selected state. Verify whether Loro versions remain reconstructible after compaction; keep a bounded snapshot when a version reference alone is insufficient.
2. Capture selected scalar fields and active assertions alongside text. A text version does not reconstruct a quantity or relation. Identify the exact fields captured so that “restore description” cannot unexpectedly restore unrelated metadata.
3. Start with explicit checkpoints. Add idle-time grouping only after explicit save, restore and retention behavior work. Idle grouping should consolidate an editing session, not create a permanent version per keystroke.
4. Enforce a total local history budget in addition to existing per-document limits. Account for snapshot bytes, scalar/assertion data and any separately retained assets. Deduplicate shared content and define deterministic eviction of unpinned automatic checkpoints. Reject or explain a save that cannot fit rather than silently deleting explicitly pinned work.
5. Read historical content into an isolated view. Send “Try this version” through the implemented local proposal workflow. Apply restoration as a new authorized edit after a conflict check; do not roll the live Loro document back behind remote operations or change an old fact.
6. Build one native history view with checkpoint labels/times, selected-field differences, a read-only preview and “Try in Simulation”/“Restore selected fields” actions. Reuse description rendering and existing live transclusion. A historical view must not imply that a transcluded Record is historically frozen unless its version was also captured.

**Acceptance.** A checkpoint can reconstruct its text and selected properties after reopening and compaction. Restoring creates a new revision and respects newer remote edits and current permissions. Retention remains bounded across many Records. Quantity and relation restoration uses ordinary exact-value and assertion paths. A scenario can start from a chosen checkpoint without changing the live Record.

### 5. Make runs self-contained for external blob dependencies

**Why this is needed only for affected runs.** SQLite snapshots already carry database-resident description assets. [Description asset handling](../../crates/engine/src/description_assets.rs) reads/writes the `description_assets` table, so the earlier blanket statement about missing attachments was too broad. The remaining dependency gap concerns bytes retained separately by [Blob Sync](../../crates/engine/src/blob_sync.rs). An operation that depends on those bytes cannot be reliably replayed from a database reference alone.

**Implementation steps.**

1. Enumerate external content dependencies of the selected operations and evidence. Classify each as embedded in SQLite, externally stored and available, deliberately omitted, or required but unavailable. Do not copy the entire user's blob repository into every simulation.
2. Extend the artifact manifest with bounded content-addressed dependency entries: content hash, byte size, logical use and bundle location. Export only selected required content from the normal blob API into the run directory and verify the content hash after copying.
3. Give isolated Cells a bundle-local read-only content adapter. A replay can read packaged bytes; it must not fetch missing bytes from a real peer or export them to a live destination as an accidental side effect.
4. Charge external bytes and manifests against the evidence/dependency budget before copying. Deduplicate content, reject unsafe paths and partial files, and save an explicit incomplete reason when a required dependency cannot be packaged.
5. Add a native dependency summary to the run setup and saved-run inspector. Show what will be retained locally and what is missing. Sharing a proposal uses its disclosure selection, not every dependency of its private local run.

**Acceptance.** A blob-dependent run replays after its original blob store is removed. Corrupted packaged bytes are detected. A missing required dependency yields incomplete evidence, not a verified result. Database-only simulations do not export unrelated blobs. Description assets already in SQLite remain available without redundant packaging.

### 6. Add controlled plugin responses through the existing plugin contract

**Why this is needed.** Command-response inputs already support controlled stdout/stderr/results. Native plugin Actions and Sync still refuse simulated execution, and isolated Cells have no executable plugin host. The actual extension is to let the normal integration code consume recorded/mock responses without starting a process, publishing to a service or writing to an export destination.

**Implementation steps.**

1. Define a narrow plugin request/response adapter at the existing `plugin_call` boundary. Live execution delegates to the current Host; simulation delegates to a bounded recorded-response provider. Reuse [plugin protocol types](../../crates/nucleus/src/plugins.rs) and their validation.
2. Package the contribution/schema metadata required to validate a recorded response. Resolve it from a reviewed local package or include selected validated metadata; do not discover or launch executables during replay. Copying a plugin's approval row does not authorize a real process inside Simulation.
3. Match a response to the plugin/contribution identity, package digest, operation, canonical input and occurrence/request index. Multiple identical requests may consume distinct recorded responses in order. Reject a response supplied for a different plugin, operation or input.
4. Represent success, provider error, timeout and absent response distinctly. Save consumed responses as scenario evidence. Missing/mismatched responses keep the run incomplete. Mocked publication can report a simulated receipt but cannot mark a real external publication as delivered.
5. Route actual import/reconciliation logic through the adapter so tests exercise normal validation, conflicts, receipts and retries. Keep output files bundle-local, or represent exports as proposed artifacts instead of writing the configured live path.
6. Add native response-selection controls to the advanced scenario setup. Show the plugin, operation, selected input and whether the response is recorded or authored. Users should not have to infer those facts from raw JSON.

**Dependency.** Plugin Actions through Karma are a separate Blood task. Do not claim to test automated plugin Actions until that invocation path exists. The response adapter can first support the operations that are actually callable, such as explicitly selected Sync passes, and later serve the automation path.

**Acceptance.** A plugin import/publication scenario runs with the normal validation logic while a sentinel executable is never launched and configured external destinations remain untouched. Replay consumes the same selected responses. Wrong digest, missing response, malformed data, revoked access, retry and uncertain-delivery cases have explicit results. Existing command mocks continue to work.

### 7. Extend forecasting only for concrete unsupported queries

**Why this is conditional.** Calendar/Schedule runtime projection and caching already work. Existing code deliberately reports incomplete results for unsupported predicates, identified viewers and certain ranges. Those are honest limitations, not evidence that all forecasting needs replacement. Complete the user's concrete saved-query/viewer use cases without removing the boundaries blindly.

**Implementation steps.**

1. Inventory the Calendar/Schedule queries actually offered by the native interface and saved Proteins. Add regression cases for filters that should work on projected records but currently return `UnsupportedFilter`. Start with a supported query used by the feature, not the entire Protein language.
2. Replace the current separation of live metadata filters and projected quantity filters when a query needs combinations such as `Any`/`Not` across them. Evaluate one typed predicate tree against the projected state of a candidate at the relevant instant/span. For relation, text or schema predicates, retain the projected properties those predicates require. A live property value is not necessarily the future value.
3. Continue returning an explicit incomplete reason for predicates whose projected dependencies are not available. A partial result must not look exhaustive. Preserve manual work when future calculation is incomplete.
4. If permitted remote viewers need forecasts, define their projected read boundary first. The current `actor.is_some()` rejection must not simply be removed. Reuse preview visibility checks, partition caches by authority/viewer context, and filter causal evidence as well as rows. Never run a reduced visible world and claim an exact full-world forecast when hidden rules could affect it.
5. When a partially historical window is supported, specify which entries come from stored history and which come from future simulation. Retain timezone and boundary tests. A past-only query needs historical evidence, not a rerun pretending it happened in the past.
6. Keep the existing step, duration, payload and span limits until measured evidence identifies a specific bottleneck. Improve the bottleneck or choose narrower windows before increasing global limits.
7. Leave Record `include.projection` at `scenario_required` unless a precise request can be answered by the same runtime and source fingerprint. If extended, return the actual completeness and assumptions with the quantity. Do not revive an arithmetic approximation that skips Karma or Transfers.

**Acceptance.** Added queries agree with direct evaluation of the isolated state at sampled times. A viewer receives no hidden Record, Rule identity or causal explanation. Revocation invalidates their cached result. Cache sharing still works for equivalent authorized requests. Unsupported filters and exhausted budgets remain explicit. Calendar, Schedule and exported calendar entries agree for the same complete query/window.

### 8. Validate failure recovery, scale and real delivery

**Why this is real work.** Deterministic scenarios cover duplicated/delayed/dropped messages, partitions, offline Cells, clock changes and orderly restarts. They do not establish operating-system crash durability, power-loss behavior, NAT traversal, deployed mailbox behavior or Linux location-portal behavior. Test those boundaries with the normal runtime. Keep platform validation separate from fast replay tests.

**Implementation steps.**

1. Run existing workload/campaign tools before inventing new infrastructure. [Transfer volume fixtures](../../crates/simulation/src/fixtures/transfer/volume.rs) support up to 32 Cells and 1,000 Transfers; [transfer_acceptance.rs](../../crates/simulation/examples/transfer_acceptance.rs) records timing, memory, convergence and optional replay. Fixture-size validation alone is not an executed workload measurement.
2. Establish repeatable measurements for a small workload and the chosen larger workload: completed events per second, rule evaluations, check/evidence cost, peak memory, bundle bytes and replay time. Compare EveryChange with sampled checks while respecting their different coverage guarantees. Record the machine, configuration and workload digest.
3. Add meaningful input-order permutations to campaigns: conflicting quantity/appointment edits, approval versus revision, delivery versus restart, and competing reservations. Persist generated event order and reduce a failure with the existing minimizer. The World can remain sequential; varied causal orderings reveal more than an uncontrolled thread race.
4. Use a child-process harness for abrupt termination at named observable boundaries, especially before commit, after commit and before acknowledgement. Reopen through the normal Store/Cell paths and check exact-once application, retained receipts and integrity. Reuse the existing process-fault testing approach in engine tests where possible.
5. Add narrow storage-failure injection only for operations that need it: snapshot creation, artifact write/finalization and transaction commit. A failed artifact must never be reported as a complete replayable bundle. Distinguish error return/cleanup tests from physical power-loss durability; process killing does not prove the latter.
6. Exercise independent instances on separate real networks for direct connections, relay fallback and offline mailbox delivery. Record the actual route, interruption/reconnection behavior and deduplication result. The current simulation worker is local-only; add an explicit test configuration or use the normal runtime rather than treating its local success as internet evidence.
7. Validate driver, passenger and outside-observer location sessions on Linux with the actual portal/provider. Cover denial, start/stop, disconnection, handover and reconnect. Measure delivery delay, traffic and memory, and verify that observers lose access when the relevant permission is removed.
8. Save redacted measurements and outcomes for review. Real-world tests should use disposable instances and test data. Do not require personal databases or production credentials for the ordinary test suite. The existing copied-workspace frequency test is intentionally ignored and requires a private snapshot and owner authorization; it is not a standard unattended check.

**Acceptance.** The measured workloads finish with expected verdicts and replay/convergence evidence. Crash tests demonstrate intact commits and no duplicated application across the tested boundaries. Storage errors leave explicit failed/incomplete artifacts. Real-network tests document actual delivery routes and recovery. Desktop location tests demonstrate permission enforcement and bounded measured resource use. Any remaining platform limitations are recorded as such.

## Items deliberately left out of the implementation sequence

### Broad Lingua support without a blocked scenario

[Simulation Lingua import](../../crates/simulation/src/lingua.rs) uses the normal directory importer. That importer supports the existing Record/Frequency/Rule projection and explicitly refuses Extension declarations. First identify a valid, supported schema/extension declaration that a chosen scenario cannot seed through existing mechanisms. If that case appears, extend the shared importer and typed projection together, use the same authorization/schema validation as normal edits, and add a package/replay test. Do not introduce a simulation-only grammar or silently accept declarations that are ignored.

### Preventive constraints

The current model includes End, At, EveryChange, EveryEvents and EveryDuration checks, windows, selected/disabled checks, Stop/Continue failure modes, exact quantities, available quantities, budgets and coverage explanations. [The quantity restriction test](../../crates/simulation/tests/karma_rules.rs) explicitly expects stopping after the violating commit. This preserves the action that caused the failure and is appropriate for testing a proposed future.

Rejecting a violating commit would change the simulated domain behavior and might hide the failure being investigated. It needs a separate domain rule or transaction-level invariant shared by live execution and simulation. It should not be added as a shortcut to finish the existing checking-period idea. Sampling remains a performance tradeoff and must not claim continuous safety.

### In-run restoration, fine cache invalidation and Space overlays

The Record defers restoration and finer invalidation until needed. Starting another run from a retained base plus a changed proposal is enough for the planned workflow. Existing projection caching already shares windows and has revision/expiry invalidation tests. Introduce finer dependency tracking only after measurements show avoidable recalculation.

Future spatial overlays need their own scene representation, bindings to Records and interaction design. The current map renders geographic data and location workflows; that does not make a forecast-to-3D adapter a missing piece of the Record scenario feature. Keep it under Space planning.

## Verification and completion rules

Use `cargo check`, never `cargo build`. Workspace warnings remain errors. Run commands through `cargo xtask` or the repository Nix environment. No Node-based tooling is required. Do not modify the web crate, AGENTS.md, README.md, existing migrations, existing migration checksum entries, or owner-authored `.lingua` files.

The correctness gate uses:

```sh
cargo xtask test -p lince-simulation --lib --test bootstrap --test assumptions --test checks --test effects --test lingua --test projection --test session --test proposals --no-fail-fast
```

After a runtime change, add the relevant target to that gate rather than rerunning the entire workspace without a reason. Use focused checks for affected crates, for example `cargo xtask check -p lince-simulation` and the relevant desktop/engine packages. New migrations additionally require `cargo check -p store` and migration-guard tests before continuing.

Each step is complete when its concrete acceptance cases pass, the native interface exists where an interaction is part of the feature, and evidence distinguishes completed, failed, inconclusive and unsupported runs. Backend support without the required user workflow is incomplete. An unsupported effect must never be silently skipped to obtain a passing verdict.

The completed round passed 33 distinct focused simulation tests: library 3, bootstrap 2, assumptions 3, checks 3, effects 3, Lingua 3, projection 8, session 1 and proposals 7. The final two proposal regressions were verified in the subsequent focused proposal run. Engine validation passed `record_changes` (16) and `private_work` (19). Native validation passed `simulation_castle` (11) and `time_castle::ui::tests` (4), including capture/edit/compare/reload, stable event IDs, draft switching, live staleness and Calendar entry guards. `cargo xtask check -p engine -p lince-simulation -p lince-desktop` passed; the final proposal guard also passed the focused simulation check.

This simulation round did not alter an existing migration, checksum, README, AGENTS file, web crate or owner-authored Record. Concurrent agents' changes remain in the shared tree. The remaining sections describe future work and its acceptance conditions.
