# Park: rebuild responsiveness across the whole experience

This directory is the working optimization plan for Park and the active Lince interface/backend paths it uses. The goal is prompt typing, clicking, scrolling, opening, dragging and local editing in a normal populated workspace on both monitors. A faster Firefox fixture alone does not finish it. The queue selects the next authorized task; merely editing these documents changes no executable or session.

The owner's [Park Record](../../trails/lince/Park.lingua) and [Lince Record](../../trails/lince/Lince.lingua) retain authority. The old [plan entry point](../Park.md) links here so existing Record and packaging links remain useful. All unfinished tasks live here, with one checkbox definition per P number. Existing IDs survive the move. Completed mechanisms and rejected experiments belong in the evidence ledger, not the task list.

## What success means

On the user's existing machine, with both physical monitors, many ordinary canvas items and normal Firefox/Ghostty activity:

- Continuous activity presents at least 57 FPS on each 60 Hz output, with good frame intervals and no accumulating frame/input backlog. The stronger goal is sustained delivery at the actual refresh rate.
- Typing, clicks, scroll and drag produce the correct visible response within two refresh periods at p95; p99, worst pauses and queue age are measured separately. Input dispatch and local computation have their own budgets.
- Local title/description edits echo immediately, update other local views promptly and reach honest durable-save acknowledgement without waiting for network sync. A local save must not trigger document-sized work per character or rebuild the entire interface.
- Unchanged presentation sleeps. Cost scales with changed data, visible/demanded content and necessary semantics, rather than every registered feature, every stored Record or the whole canvas.
- The same workflows still function together: live Records, editing/IME, composition, scrolling, focus, accessibility, media, automation, capture and both outputs. Quality, correctness and supported functionality remain part of the result.

Exact workloads, targets and measurement limitations are in [08-benchmarks.md](08-benchmarks.md). They are targets to implement and demonstrate, not claims that the current build passes.

## Execution queue and subject files

| Stage | File | Decision/output |
| --- | --- | --- |
| Evidence to reuse | [00 — Evidence and research](00-evidence-and-research.md) | What is established, what is only a theory, receipts and source anchors; do not repeat completed work. |
| 1 | [01 — Measurement](01-measurement.md) | Reproducible current workload, causal input/save traces, ranked critical path, CPU/GPU/wait ownership. |
| 2 | [02 — Input and scheduling](02-input-and-scheduling.md) | Short, bounded interaction work; independent rendering and background work; useful parallelism. |
| 3 | [03 — Local edits and storage](03-local-edits-and-storage.md) | Cheap local feedback, incremental recovery and writes, short transactions, bounded fanout. |
| 4 | [04 — Styles, layout and retention](04-style-layout-and-retention.md) | Read-optimized resolved styles, precise invalidation, local layout and retained transforms. |
| 5 | [05 — Feature costs and scaling](05-feature-costs-and-scaling.md) | Every active feature mapped to shared paths; visible-list scaling, bounded workers and retained panels. |
| 6 | [06 — GPU and outputs](06-gpu-and-outputs.md) | Correct device work, fewer passes/transfers/waits, independent output pacing and damage. |
| 7 | [07 — Unbloat and release](07-unbloat-and-release.md) | Delete proven waste, simplify ownership and produce a better optimized shipping artifact. |
| Contract used by every stage | [08 — Benchmarks](08-benchmarks.md) | Stable numerical and work-count targets for Laboratory/tests and real daily use. |
| Coverage used by every stage | [09 — Workflows and combinations](09-workflows.md) | User actions, feature inventory, combinations, oracles and missing coverage. |
| Architecture used by every stage | [10 — Architecture](10-architecture.md) | Shared families, intended data ownership, invalidation and handoff boundaries. |
| 11 | [11 — Acceptance and experiments](11-acceptance-and-experiments.md) | Combined candidate evidence, honest physical acceptance, unfinished repairs and rejected directions. |

Follow [execution-order.md](execution-order.md), the single cross-file queue. The first pending ready row is the next task. Re-rank it after every result so the next task always has the greatest supported expected benefit; actual gains become known through testing. Subject-file numbers organize details, not priority. Their checkboxes follow the current global order within each subject. Do not finish a whole subject before moving to a more valuable task in another file.

The [workload definitions](workloads.json), input correlation, borrowed editor input/splicing, incremental recovery and dependent local-query refresh have evidence in 00. P118 removes unrelated queries/history transactions; P126 removes unrequested result preparation. P102 now maps the history and writer phases. Indexed retention is kept after linked-SQLite controls and 101 checks, with its slower small control explicit; it does not establish a whole-save or desktop gain. P149 removes unrequested body decoding and overflow reads from metadata storage queries; its scaling, correctness and mixed controls are recorded in 00. P150 removes unused SQLite allocator-statistics serialization after optimized multi-worker controls and native correctness checks; mixed native latency and actual sync waits promote safe write-boundary reduction through P116/P117. Canonical composition/canvas checks share one fresh Record read, and P137 now reuses exact validated JSON text sizes instead of rescanning old text for its byte budget. Full-history structure, limits and snapshot checks remain. The helper/debug-save experiments returned priority to P66/P67. Four installed optimized nested observations now map the frame path: the warm populated update takes about 18.6 ms followed by 5.6 ms export, while client pixels usually bypass main but still pay extraction/render/export. P56 submission batching is the next supported bounded experiment, followed by attribution of the large native main remainder for P60 and coherent overlap through P62. GPU rendering is already active; these observations do not justify a speculative CPU-to-GPU migration. P66 takes priority when the current optimized traced session is available; P102 retains causal publication and CPU/wait attribution. These findings do not establish the cause of external-client or canvas-drag lag. Promote a proven dominant render, wake, storage or input cause immediately; do not finish every profiler audit before making useful fixes.

Small inexpensive wins are welcome with care. There is no minimum percentage: count a repeatable gain or clearly eliminated work, allocations, copies or wakes, especially when ownership and code get simpler. If elapsed results are below measurement resolution, keep a sound low-cost simplification/work reduction with that limit stated; do not claim measured FPS or latency improvement. Check regressions and total resources, avoid new layers for a marginal benefit, and do not spend many iterations chasing noise while a dominant blocker remains. Combined improvements are measured rather than added arithmetically.

## Direction of the overhaul

Treat input, semantic mutation, presentation derivation, GPU execution, output presentation and durable storage as distinct responsibilities with explicit handoffs. One keystroke must not require every feature to poll, every style to resolve, every subtree to lay out, every document to serialize and every output to repaint before its own effect becomes visible. Reuse the existing ECS, host bridge, source revisions, workers, wake deadlines, retained surfaces and test catalogue; replace a boundary when the evidence shows it is fundamentally wrong. This is permission to plan substantial refactors of active paths, not a requirement to keep an inefficient architecture intact.

**Styling favors reads.** Themes and token assignments are usually edited far less often than their resolved values are consumed. Prefer paying bounded derivation/invalidation cost on actual changes and reading compact, already-resolved values on ordinary frames. Continuous sliders, relation color rules, dynamic inherited contexts and newly opened items still need correct live updates. Persisted JSON/token source representation is a separate concern from the runtime hot representation. No claim of “100% optimized” is credible without workload evidence; the open style tasks explicitly test this gap.

**Local editing is local.** Separate immediate in-memory feedback, recoverable draft persistence, canonical local commit, view publication and remote replication. Keep their status honest. Existing durability and collaboration protections are intentional; replace excessive work with a cheaper correct mechanism rather than removing recovery, authorization, history or synchronization.

**Parallelism must shorten the user-visible path.** Use existing pools for sufficiently large independent immutable batches, retaining dependent parent/child and global-budget decisions in order. Removing a chain between exclusive World systems does not make them concurrent. Include extraction/copy, queues, validation and synchronization in the comparison. Correct CPU/GPU placement is assessed from actual work and transfer/wait cost, not a utilization screenshot.

**Unbloating must reach executed work.** Remove duplicate scans, conversions, subscriptions, writes, ownership adapters, caches and eager initialization where their use/cost is established. Source size alone is not a responsiveness diagnosis. Preserve functionality, attribution, credits and declared behavior; do not touch the unplugged web frontend or protected files as collateral cleanup.

## Benchmarks and combinations govern experiments

Each experiment names its critical-path hypothesis, shared family, workloads, expected gain, correctness oracle and dependencies before coding. An isolated throughput result does not establish typing latency or whole-desktop benefit. Keep raw observations and compute the same metrics for baseline and candidate, including unchanged controls and baseline drift. Do not add percentages from overlapping spans or multiply independent microbenchmark gains into a desktop prediction.

The user accepts a net performance gain even when some cases become slower. Show the actual gains, costs and affected workflows instead of imposing a new regression policy or automatically rejecting any slowdown. A temporary failure during a staged refactor stays visible with a specific repair task and affected benchmark; it is not a passed result. Try the follow-up optimization that can restore the affected case while retaining the win. Keep benchmark definitions and correctness oracles stable so a worse result cannot be hidden by changing the test. Final acceptance reflects the measured combined candidate and the user's tradeoff decisions.

Use the finite workflow factors and pairwise coverage in 09, plus named high-risk triples and transitions. Enumerating the infinite Cartesian product of every feature, document, layout and event is impossible. Coverage is complete only relative to a source-stamped inventory and an explicit selected set; unknown/uncovered cases are named. Newly reachable UI features extend that inventory through P124, without requiring a new benchmark framework per Castle.

## Completing one task

Keep only one definition of each task. A task may finish with a useful implementation, a measured rejection, or an investigation that has the stated deliverable. Remove it only when that outcome and its required checks are recorded. Missing required evidence stays pending. Record a rejected experiment in the evidence ledger; the user's existing authorization for an exact negative result in Build.lingua does not authorize unrelated Record edits. A successful mechanism need not close the physical acceptance tasks.

Use a frozen source/toolchain/features/profile identity and actual executable hashes. Compile comparison variants before timing; exclude compilation, competing test renderers and uncontrolled benchmark processes. Reuse existing fixtures and a bounded runner. Start with short observations and extend only to answer a named uncertainty. Do not create whole worktrees or rebuild the dependency graph for each small hypothesis when the existing matching setup suffices.

Use cargo check, warnings as errors, and the existing Nix/xtask workflow for executions. No cargo build/clean, code comments, past migration edits or automatic session disruption. Schema changes, if necessary, use new append-only migrations and the repository's migration checks. Preserve concurrent agents' edits. Documentation work does not initiate release builds, Actions polling, installation, logout or physical monitor/lock/VT actions. A task requiring those actions remains explicit and coordinated with the user.

The source review for this reorganization was at b4429a44cf3d20ba18b3f90a131339f64ba14201 with a dirty shared worktree. Historical optimized runs and the uncommitted P91 scheduling change have different identities; none is silently a current desktop benchmark.
