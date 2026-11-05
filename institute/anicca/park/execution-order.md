# Execution order: highest expected useful gain first

[Organizer](Park.md). This is the single cross-file execution queue. Numbered subject files group related details; they are not a requirement to finish one entire subject before the next. Each P ID has exactly one checkbox definition at the linked location.

Take the first ready task in this table. After every completed investigation or experiment, update this table from the new evidence before taking another task. Actual gains cannot be known before testing: “biggest next gain” means the greatest supported expected improvement to the current experience, including the smallest prerequisite needed to establish or unlock it. Never claim this initial source-based ranking proves a future percentage.

Rank by contribution to command-to-effect delay/freezes, repetition rate, affected daily workflows/outputs and confidence, then weigh implementation/validation effort, resource cost and correctness complexity. Use absolute milliseconds, CPU/bytes/wakes avoided and workload frequency as well as percentages. A necessary measurement can have higher expected value than a speculative fix. A broad scan or major blocking phase generally outranks a getter; one measured dominant cause can promote any matching task straight to the front.

The initial order prioritizes input blocking and local edit amplification, then long exclusive/render work, useful parallelism, invalidation and demand, then smaller repeated costs and build quality. This reflects the reported UX and source evidence, not a measured current ranking. If P66 finds render/GPU cost dominates, immediately promote P67/P70 and the matching P60/P62/P72/P68/output task ahead of the save hypotheses. If an edit or source wake dominates, promote the corresponding storage/wake task. A reproduced correctness error such as P94 is promoted to its affected path immediately.

## Readiness and prerequisites

P103 completed identity/specifications ([workloads.json](workloads.json), receipt in 00); P101 completed the input correlation and controlled private pixel checks; P102 adds only missing local-edit/save causal signals and reuses the reporter. P66 consumes that evidence without waiting for every profiler audit to finish. Do not spend the entire first round building observability infrastructure if the current tools already reveal a dominant blocker.

P60 precedes its dependent P61 split/batch work; P62 needs a coherent handoff, not a blind pipeline flag. P67/P70 feed processor/wait experiments. P36 feeds source-wake changes. P109 precedes style representation changes, P78 precedes dirty-consumer application, and P113 supplies the proven boundary for P122. P102 feeds the local-save experiments; P138 analysis constrains P139. A missing dependency is promoted only as far as needed to make the best candidate executable. A conditional premise that is absent is documented and deferred; it does not block the queue.

P124 inventories the shared feature paths and feeds cost/coverage work. P49 precedes reuse in P84. P86 identifies the concrete obsolete/duplicate/dependency candidate for P87–P90/P92. P93/P104 are promoted when the existing runner/catalogue genuinely blocks the selected experiment; they are not a reason to rewrite tooling before making a useful fix. P134 needs the fixture/inventory and existing case integration. P135 checks combinations at meaningful integration points; final acceptance uses the installed optimized candidate. Compiler experiments retain their own fixed-profile dependencies and use the established release workflow when selected.

## Small wins are welcome

There is no minimum percentage required. A small repeatable runtime improvement, or a clear reduction in executed work/allocations/copies/wakes with unchanged behavior and little added cost, is worth keeping when its regression checks and maintenance tradeoff are sound. Simpler ownership/data flow can justify an inexpensive mechanism improvement even if wall-time resolution cannot establish its desktop impact; label that result honestly as proven work reduction or simplification, with runtime gain unresolved. A theoretically nicer design alone is not proof of speed.

Prefer cheap elimination of work over adding caches, knobs, queues or layers for a marginal result. Take a nearby free win while touching the responsible family when it does not derail the dominant fix; give it its own receipt so its effect is not hidden. Do not reject it automatically because a duration is small, and do not spend many build/benchmark rounds proving a tiny percentage while a large blocker remains. If regression repair costs more than the gain, show that tradeoff rather than calling it free.

Many small wins can accumulate. Verify the combined path periodically: overlapping optimizations may remove the same work, alter contention or cancel each other. Do not sum microbenchmark percentages. Keep a useful combined net result even if individual cases become slower, with affected workflows and any unfinished repair explicit, reflecting the user's stated preference rather than inventing a new regression policy.

P103/P101, borrowed editor input/splicing and incremental recovery are complete; P115's periodic encoding is already handled by the same bounded worker. The running executable was rechecked at /proc/528996/exe and is still the 9d8c20bc Nix package, excluding these changes. P102 now separates backend phases and populated query/sync work; per-edit causal linkage, SQL-statement counts and graphical publication/presentation remain open. P137 halves history checks/bytes, and completed P79 removes helper buffers with lower large-body first-update times. Their save acknowledgement controls remain mixed/slower, so reuse these frozen baselines to attribute the outstanding canonical/scheduling/I/O tails next; do not claim a total-save improvement or rebuild the profiler. P66 supplies current optimized daily attribution once its binary is running. P43/P105 retain the broader input-blocking work and remaining full-text/projection dependencies; diagnostic frame preparation is also substantial. Independent locally testable work can continue while live validation is pending, with shared unresolved mechanisms investigated sequentially.

P118 completed the measured local-owner dependency rejection: three edits now execute six relevant queries/history inserts rather than 24, with actual sync calls falling from 38 to 21; delegated and derived cases keep full refresh. See 00 and query-invalidation/receipt.json. P102 is next for the missing SQL-statement and production causal/publication links, rather than repeating this removed fanout. P66 remains the highest-value live attribution when the current optimized executable is available; do not infer the cause of external-client/canvas lag from this backend fixture. The additional private CPU captures in frame-cpu/receipt.json confirm worker execution but do not rank a native bottleneck: startup/readback work and insufficient native Rust call chains remain explicit. P126 has a concrete unused-projection source lead; P102 must distinguish its cost from durable query-history work before promoting it above the shared frame/input candidates.

## Pending tasks

| Next | Task and full definition | Expected value / activation |
| --- | --- | --- |
| 1 | [P102 — Trace one local Record edit through durability and view publication.](01-measurement.md) | Separate echo, encoding, durable I/O, canonical commit and publication amplification. |
| 2 | [P66 — Produce one ranked bottleneck report for the current optimized candidate.](01-measurement.md) | Rank the actual largest causes and immediately update this queue. |
| 3 | [P43 — Bound input waiting behind the longest native work section.](02-input-and-scheduling.md) | Remove a measured input wait shared by everyday interactions. |
| 4 | [P105 — Remove the longest unrelated dependency from native text feedback.](02-input-and-scheduling.md) | Make native text feedback independent of unrelated expensive work. |
| 5 | [P106 — Bound one busy queue drain so control input remains serviceable.](02-input-and-scheduling.md) | Prevent one bulk drain from starving current input/control. |
| 6 | [P94 — Identify the cause of the database-locked toast.](03-local-edits-and-storage.md) | Promote on actual database-lock errors; otherwise defer missing reproduction. |
| 7 | [P137 — Remove one document-sized cost from canonical text mutation preparation.](03-local-edits-and-storage.md) | Potential document-sized canonical work per small edit. |
| 8 | [P138 — Shorten the engine-wide access gate without weakening operation validity.](03-local-edits-and-storage.md) | Potential engine-wide serialization of otherwise unrelated work. |
| 9 | [P139 — Remove unnecessary post-commit reaction waiting from local save feedback.](03-local-edits-and-storage.md) | Potential rules/effects on the local-save acknowledgement path. |
| 10 | [P121 — Prevent a slow Cell request from delaying unrelated local edit service.](03-local-edits-and-storage.md) | Potential long Cell request blocking every local request. |
| 11 | [P117 — Shorten the measured SQL transaction critical section for an edit.](03-local-edits-and-storage.md) | Shorten measured SQL acquire/hold/commit delay. |
| 12 | [P119 — Update a description preview incrementally while typing.](03-local-edits-and-storage.md) | Avoid complete description-tree reconstruction while typing. |
| 13 | [P67 — Map CPU/GPU responsibilities across the active frame path.](01-measurement.md) | Promote immediately if processor placement, transfers or waits dominate. |
| 14 | [P70 — Observe GPU execution separately from submission/completion age.](01-measurement.md) | Promote only where missing actual GPU timing prevents a causal choice. |
| 15 | [P60 — Split the highest-cost exclusive system into derivation and application.](02-input-and-scheduling.md) | Largest currently evidenced exclusive surface-preparation candidate. |
| 16 | [P61 — Parallelize one sufficiently expensive independent batch.](02-input-and-scheduling.md) | Exploit expensive independent work after the split proves its boundary. |
| 17 | [P62 — Test coherent main/render overlap.](02-input-and-scheduling.md) | Potential large main/render overlap; include input age and handoff costs. |
| 18 | [P72 — Remove one avoidable foreground GPU-completion wait.](02-input-and-scheduling.md) | Promote a proven foreground GPU wait; safe readiness already exists. |
| 19 | [P68 — Compare processor placement for one evidenced misplaced stage.](06-gpu-and-outputs.md) | Move the evidenced misplaced work, including all transfer/wait costs. |
| 20 | [P107 — Stop preparing one superseded unsubmitted native frame.](02-input-and-scheduling.md) | Stop expensive work on superseded unsubmitted frames. |
| 21 | [P58 — Compare one versus two frames in flight at the actual bottleneck.](02-input-and-scheduling.md) | Compare bounded overlap after the actual bottleneck is known. |
| 22 | [P113 — Separate one broad frame invalidation into its real dependency domains.](04-style-layout-and-retention.md) | Separate real dependency domains to avoid repeated whole-frame work. |
| 23 | [P122 — Resolve only the layout root affected by one ordinary edit.](04-style-layout-and-retention.md) | Solve only the affected constraint closure instead of unrelated layout. |
| 24 | [P36 — Attribute the remaining native wake stream.](01-measurement.md) | Promote if excess wakes keep the whole native path active. |
| 25 | [P47 — Remove the worst remaining redundant source wake.](02-input-and-scheduling.md) | Remove the largest attributed redundant source wake. |
| 26 | [P108 — Replace the editor binding heartbeat with actual readiness and deadlines.](02-input-and-scheduling.md) | Concrete focused-editor heartbeat/readiness candidate. |
| 27 | [P48 — Define one safe hidden-subtree presentation-demand boundary.](04-style-layout-and-retention.md) | Stop proven unneeded hidden presentation while retaining semantics. |
| 28 | [P124 — Complete the source-to-workflow inventory for active interface features.](05-feature-costs-and-scaling.md) | Map actual reachable features/owners and the next costly family. |
| 29 | [P77 — Gate presentation work for one feature family with no live views.](05-feature-costs-and-scaling.md) | Prevent absent views from triggering presentation work. |
| 30 | [P49 — Virtualize one measured large Record-result list.](05-feature-costs-and-scaling.md) | Make a large result list scale with visible rows. |
| 31 | [P126 — Bound one expensive query refresh without delaying local feedback.](05-feature-costs-and-scaling.md) | Bound the necessary expensive query after narrowing fanout. |
| 32 | [P125 — Retain one costly settings or property panel across ordinary edits.](05-feature-costs-and-scaling.md) | Keep expensive panels/controls instead of repeated reconstruction. |
| 33 | [P74 — Test a translation-only canvas-pan path.](04-style-layout-and-retention.md) | Retain unchanged content during canvas pan. |
| 34 | [P75 — Test transform retention during an ordinary item drag.](04-style-layout-and-retention.md) | Retain unchanged content during ordinary item drag. |
| 35 | [P76 — Test scrolling through one retained presentation transform.](04-style-layout-and-retention.md) | Retain children under one scroll transform. |
| 36 | [P109 — Measure the read-heavy styling cost before choosing its representation.](04-style-layout-and-retention.md) | Establish actual style read/write cost and reassess rare-write gains. |
| 37 | [P78 — Narrow one overly broad style or layout invalidation.](04-style-layout-and-retention.md) | Narrow invalidation rather than clearing unrelated style/layout work. |
| 38 | [P111 — Apply resolved styles only to affected consumers.](04-style-layout-and-retention.md) | Avoid scanning/applying unchanged consumers. |
| 39 | [P110 — Compare a compact resolved-style read representation.](04-style-layout-and-retention.md) | Make the remaining frequent resolved reads compact and cheap. |
| 40 | [P112 — Publish expensive style edits as one coherent revision.](04-style-layout-and-retention.md) | Only if read optimization makes style writes too costly/incoherent. |
| 41 | [P51 — Reduce validation work in the existing native-surface cache.](04-style-layout-and-retention.md) | Avoid expensive validation walks when retaining a raster. |
| 42 | [P80 — Distinguish texture capability from actual texture updates.](04-style-layout-and-retention.md) | Do not mistake texture capabilities for new pixels. |
| 43 | [P52 — Retain damage independently for each output.](06-gpu-and-outputs.md) | Avoid forcing the quiet monitor to recomposite. |
| 44 | [P53 — Pace one output from its presentation clock without a redraw loop.](06-gpu-and-outputs.md) | Fix output pacing/callback churn where measured. |
| 45 | [P55 — Remove one measured redundant render pass or copy.](06-gpu-and-outputs.md) | Remove a demonstrated redundant render pass or transfer. |
| 46 | [P81 — Retain compatible incoming DMA-BUF import bindings.](06-gpu-and-outputs.md) | Retain actual compatible DMA-BUF imports safely. |
| 47 | [P54 — Reuse compatible exported/output resources within a bounded budget.](06-gpu-and-outputs.md) | Reuse compatible output resources instead of allocating repeatedly. |
| 48 | [P56 — Improve one demonstrated draw/upload batching break.](06-gpu-and-outputs.md) | Repair the measured draw/upload batching break. |
| 49 | [P59 — Reduce measured raster-resolution churn during navigation.](06-gpu-and-outputs.md) | Avoid navigation-time raster replacement churn. |
| 50 | [P63 — Limit one background job family that delays visible work.](02-input-and-scheduling.md) | Promote on demonstrated background contention with foreground work. |
| 51 | [P83 — Move one remaining blocking foreground operation off the interaction path.](02-input-and-scheduling.md) | Promote a remaining dominant blocking operation beyond named paths. |
| 52 | [P73 — Test asynchronous preparation of a stalling shader pipeline.](02-input-and-scheduling.md) | Promote on a traced cold pipeline-compilation freeze. |
| 53 | [P116 — Amortize one stream of local edit commits without delaying feedback.](03-local-edits-and-storage.md) | Amortize residual commits within the local-durability budget. |
| 54 | [P120 — Eliminate one proven no-op edit write and its downstream publication.](03-local-edits-and-storage.md) | A cheap no-op write/publication elimination can be a useful nearby win. |
| 55 | [P128 — Keep terminal or Command output from monopolizing interface work.](05-feature-costs-and-scaling.md) | Promote when terminal/Command streams cause current input lag. |
| 56 | [P127 — Remove one blocking preparation step from opening a native view.](05-feature-costs-and-scaling.md) | Promote a measured cold/warm opening stall. |
| 57 | [P129 — Tie one media producer to real consumer presentation demand.](05-feature-costs-and-scaling.md) | Share/limit unnecessary media preparation for actual consumer demand. |
| 58 | [P130 — Make one Area or physics recomputation depend on changed participants.](05-feature-costs-and-scaling.md) | Avoid all-participant Area/physics work when one input changes. |
| 59 | [P50 — Compare an incremental spatial index with one expensive canvas scan.](05-feature-costs-and-scaling.md) | Index only a dominant scan where maintenance earns its cost. |
| 60 | [P85 — Reuse one unchanged pointer-geometry derivation safely.](05-feature-costs-and-scaling.md) | Retain measured unchanged geometry without caching eligibility. |
| 61 | [P84 — Apply the shared list approach to one other costly native list.](05-feature-costs-and-scaling.md) | Reuse successful list scaling for one other proven costly list. |
| 62 | [P131 — Publish accessibility changes incrementally for a stable interface.](05-feature-costs-and-scaling.md) | Accessibility export only if it is costly or a simple work removal exists. |
| 63 | [P132 — Retain one unchanged capture-contributor derivation.](05-feature-costs-and-scaling.md) | Retain unchanged capture contributor structure with current privacy. |
| 64 | [P133 — Share one repeatedly created immutable asset family.](05-feature-costs-and-scaling.md) | Share a proven repeatedly created immutable asset family. |
| 65 | [P86 — Rank actual bloat in active paths.](07-unbloat-and-release.md) | Rank actual executed/initialization bloat from the existing evidence. |
| 66 | [P87 — Remove one proven obsolete active-code path.](07-unbloat-and-release.md) | Delete one proven obsolete active path and wiring. |
| 67 | [P89 — Consolidate one redundant derived-state cache or resolver layer.](07-unbloat-and-release.md) | Consolidate redundant cache ownership, with invalidation preserved. |
| 68 | [P88 — Consolidate one duplicated pure coordinate-mapping operation.](07-unbloat-and-release.md) | Share identical coordinate math only when useful and simpler. |
| 69 | [P92 — Initialize one unused-at-startup resource family on demand.](07-unbloat-and-release.md) | Avoid an unused eager resource while keeping first use responsive. |
| 70 | [P90 — Remove one unused dependency or redundant feature activation.](07-unbloat-and-release.md) | Remove a proven unnecessary dependency/feature activation. |
| 71 | [P148 — Repair worker-only test feature gates.](07-unbloat-and-release.md) | Actual optional check failure in unchanged session-only test imports; validation/build benefit, no claimed desktop speedup. |
| 72 | [P93 — Consolidate the performance-comparison runner around existing fixtures.](01-measurement.md) | Promote if the existing comparison runner blocks the highest-value test. |
| 73 | [P104 — Expose the existing performance cases as useful Laboratory results.](01-measurement.md) | Promote when selected cases need reusable Laboratory integration. |
| 74 | [P134 — Generate the finite cross-feature workflow coverage set.](11-acceptance-and-experiments.md) | Close source-stamped finite cross-feature coverage gaps. |
| 75 | [P135 — Measure the combined candidate and repair one harmful interaction.](11-acceptance-and-experiments.md) | Run at integration boundaries; diagnose/repair harmful interactions. |
| 76 | [P57 — Test direct scanout for one eligible fullscreen SDR client.](06-gpu-and-outputs.md) | Conditional fullscreen-only win after broader rendering costs. |
| 77 | [P96 — Compare cross-crate ThinLTO with the current release profile.](07-unbloat-and-release.md) | Broad release-code opportunity after structural source stabilization. |
| 78 | [P97 — Compare release codegen-units = 1 with the current partition count.](07-unbloat-and-release.md) | Compare codegen partitioning at fixed LTO/profile. |
| 79 | [P98 — Compare full LTO with the best measured ThinLTO release.](07-unbloat-and-release.md) | Compare full LTO with selected ThinLTO, including build cost. |
| 80 | [P100 — Compare a deliberate CPU target with the shipping target.](07-unbloat-and-release.md) | Test an intentional supported machine target, not runner-native. |
| 81 | [P99 — Test representative profile-guided optimization for the selected release profile.](07-unbloat-and-release.md) | Representative PGO last, after hot paths stabilize. |
| 82 | [P64 — Establish genuinely quiet daily-workspace resource use.](11-acceptance-and-experiments.md) | Reuse current valid quiet-workspace evidence; close installed gate. |
| 83 | [P65 — Establish prompt visible interaction under normal load.](11-acceptance-and-experiments.md) | Reuse current valid causal input observations; close installed gate. |
| 84 | [P136 — Demonstrate prompt durable local title and description edits in daily use.](11-acceptance-and-experiments.md) | Close real local title/description durability and feedback acceptance. |
| 85 | [P16 — Establish near-refresh presentation independently on both monitors.](11-acceptance-and-experiments.md) | Close each physical output’s near-refresh presentation acceptance. |
| 86 | [P15 — Establish nearly lossless Firefox video under that same load.](11-acceptance-and-experiments.md) | Close matched nearly lossless Firefox video acceptance. |
| 87 | [P140 — Make rule discovery and hidden-comment coverage explicit.](11-acceptance-and-experiments.md) | Awaiting owner refinement; map body/header requirements to explicit checks. |
| 88 | [P141 — Reuse a bounded native UI fixture and define the ordered fast suite.](11-acceptance-and-experiments.md) | Needs P140 and refined fixture design; preserve ordinary input and isolation. |
| 89 | [P142 — Add the first failing regression for the reported Protein Area dropdown.](11-acceptance-and-experiments.md) | Needs P141; mandatory first case verifies visible choices and selection. |
| 90 | [P145 — Expose the same cases through Laboratory and the default interface check workflow.](11-acceptance-and-experiments.md) | Needs P142–P144; reuse the existing catalogue and P104 integration. |
| 91 | [P146 — Verify actual text paint with one small offscreen render check.](11-acceptance-and-experiments.md) | Needs P142/P145 and a supported adapter; accessible names are insufficient. |
| 92 | [P147 — Broaden coverage and optimize the routine suite against measurements.](11-acceptance-and-experiments.md) | Needs P140–P146; retain mandatory visual coverage within a measured runtime budget. |
