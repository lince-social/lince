# 11. Combine improvements and finish physical acceptance

[Organization and execution order](Park.md). Targets are in [benchmarks](08-benchmarks.md); interaction coverage is in [workflows](09-workflows.md).

Use the same workload identities and numerical targets across the stages. Headless tests, private GPU fixtures and installed physical observations have different scopes. Reuse a valid result when its binary/workload still applies; do not repeat every test after every change. Source acceptance is not installed-session acceptance.

A temporary regression in an intermediate slice stays attached to the specific failing case and its repair task; keep the original benchmark target. The user permits net-positive tradeoffs and has not requested a new regression policy. Report the combined measured cost/benefit and preserve an explicit outstanding failure rather than turning a missing result into a pass.

Do not initiate monitor/lock/VT/logout/resume tests or a new release build from this document alone. Existing correctness passes retain their provenance. Coordinate necessary physical actions, verify the installed executable and avoid private content. The established final runner is reused; stage-1/5 additions extend it, not another provider/hardware matrix.

- [ ] **P134 — Generate the finite cross-feature workflow coverage set.**

    **Why:** isolated cases miss the combinations that make normal daily use sluggish or break a retained/parallel path. **Scope:** implement the factors, representative workflows, mandatory transitions and high-risk triples from 09 in the existing fixture/runner, using deterministic pairwise coverage for compatible factor values. Save which pairs/rows ran, which are invalid/inapplicable, and which lack an oracle. Use private local/practice data and deterministic held workers for fault cases. **Verify:** a known missing pair and injected wrong outcome fail coverage, exact selectors run nonzero cases, cancelling restores Laboratory state, and no normal user data changes. This extends existing cases, not a new broad provider/hardware matrix or exhaustive product of every UI field. **Needs:** P124/P103 and reusable P104 integration; physical checks remain separate.

- [ ] **P135 — Measure the combined candidate and repair one harmful interaction.**

    **Why:** the sum of isolated wins is not the behavior of shared queues, caches, layouts, GPU resources and output pacing together. **Scope:** combine accepted input/save/style/scheduling/GPU changes at a frozen revision and run the fixed benchmark/workflow set. Compare baseline, each relevant single change and the combined path only where needed to attribute a regression; include style read/write distribution and representative active backgrounds. If a conflict appears, trace and implement one targeted repair or record the actual tradeoff/next repair rather than hiding the case. **Verify:** end-to-end p95/p99, both outputs, local durability, quiet CPU/memory and the workflow oracles, with drift controls. No arithmetic addition of microbenchmark gains. A still-failing required outcome stays open. **Runs at integration boundaries,** not as another full suite after every tiny edit.

- [ ] **P64 — Establish genuinely quiet daily-workspace resource use.**

    **Why:** static content should not keep unrelated interface work alive. **Scope:** trace the current optimized binary with both screens and ordinary quiet applications; record native updates, source/OS wakes, per-thread CPU, GPU activity and retained resources. Exclude compilation and private rendering tests. **Pass:** intentional clocks and background semantics remain live, unrelated static work sleeps, and resource use is stable and explained. The older uncontrolled 1d9581859a19 sample is not this pass. Reuse P36 attribution instead of collecting another anonymous CPU number.

- [ ] **P65 — Establish prompt visible interaction under normal load.**

    **Why:** throughput alone does not establish usable typing, scrolling, clicking or dragging. **Scope:** both outputs, many native items and Firefox; correlate controlled visual changes with input delivery, native work and the affected output's presentation. **Pass:** warmed input-to-visible p95 within two refresh periods, with freezes and p99 recorded and investigated. Application buffer commits alone do not prove text changed because of the input. Keep clock domains separate and label unobserved causal response; record no private text/clipboard contents. Coordinate any necessary user action individually.

- [ ] **P136 — Demonstrate prompt durable local title and description edits in daily use.**

    **Why:** P65 checks visible feedback but does not establish the reported slow local-save workflow is fixed. **Scope:** the installed source-identified optimized candidate, normal populated two-monitor workspace and one private local sample Record visible in more than one native view. Type a short title edit and body paragraph, change focus and reopen; correlate echo, view publication and durable acknowledgement with P102 and perform a private process-reopen recovery check through the existing harness. Include ordinary background activity and verify the local path does not await network; do not disable the user's real network or restart their session for this. **Pass:** B02/B05 budgets, exact text/undo semantics, no lost draft and accurate status. Any physically observed part requiring user input is coordinated one concrete action at a time. Missing timings remain explicit. **Needs:** the relevant stage-3 fixes in the installed binary.

- [ ] **P16 — Establish near-refresh presentation independently on both monitors.**

    **Why:** one fast output or a browser animation counter can hide slow desktop delivery elsewhere. **Scope:** current optimized binary, normal many-item scene and continuous activity on both physical outputs, with resolution, refresh, scale, zoom and geometry recorded. **Pass:** at least 57 FPS per 60 Hz output, without excessive stale frames or increased interaction age. Static content need not submit identical frames at refresh rate. The historical 12.1 FPS result remains evidence of the earlier failure until this matched observation passes.

- [ ] **P15 — Establish nearly lossless Firefox video under that same load.**

    **Why:** decoding frames that are discarded does not deliver smooth video. **Scope:** the same visible 30-second 720p60 source at matched geometry/100% zoom, comparing the current optimized Park candidate with the existing matched Niri reference. Separate decoded, dropped and compositor-presented frames; correlate P16 rather than treating browser callbacks as physical scanout. **Pass:** drops no worse than 0.5 frames/second and no new input/other-output regression. The historical Park result was 43.9 drops/second. No physical run starts without the needed user readiness.

## Receipt for every completed experiment

Record the task/hypothesis and selected family; baseline/candidate full source and executable identities; effective toolchain/features/profile/flags; machine/device/output/workload manifest; warmup, sample counts and workload order; tracing overhead and baseline drift; total and critical-path CPU/wall/GPU/transfer/memory/allocation/wake values where observed; input and save p50/p95/p99/max and both-output presentation; correctness selectors with nonzero counts; missing evidence; and the keep/reject/continue decision. No private text/clipboard values are needed.

A useful implementation remains in source. Remove a completed checkbox and link its receipt in 00, preserving downstream acceptance dependencies. A rejected experiment loses its candidate-only code and tests while retaining patch/evidence/reason; do not silently rewrite old receipts. The owner's exact Build.lingua rejection entry is edited only under its specific authorization. If a task cannot yet pass, leave it and name the actual blocker or repair.

## Comparing combinations without an explosion

Use an unchanged control, the best baseline, and the combined candidate. Add a single-change ablation only when needed to explain an interaction: read-optimized styles plus dirty layout; input fast path plus main/render overlap; batched durable writes plus subscription narrowing; output damage plus shared GPU resources. Track whether each effect becomes redundant, additive, mutually dependent or harmful using actual measurements. Do not run all 2^N feature-switch permutations. Runtime toggles are diagnostic tools only when they preserve a valid equivalent path and can be removed afterward.

When a combined change fails, first locate the critical-path or dependency disagreement. One queued generation, wrongly broad invalidation, unnecessary memory copy or shared lock can erase several isolated wins. Repair that boundary and rerun the affected cases plus a control, then the final combined acceptance when meaningful.
