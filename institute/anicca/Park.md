# Park: responsiveness, useful parallelism and efficient rendering

This is a plan for discussion. Editing it does not resume the paused implementation, compilation or physical tests. It covers the active Park path through interface, compositor, Cell and storage where those affect the experience. Park's declared behavior and validation boundaries remain in institute/trails/lince/Park.lingua; this file does not replace that Record.

The problem is the whole experience: typing, clicking, scrolling, dragging, opening windows and ordinary Firefox use. Better throughput is insufficient if input becomes older, freezes remain, or one monitor improves at the other's expense.

## Outcomes and evidence

The acceptance workload is the real daily workspace: many normal native canvas items, Ghostty and Firefox on both monitors. Initial engineering gates are at least 57 FPS on each 60 Hz output during continuous activity, Firefox video drops no worse than the matched Niri reference of 0.5 frames/second, and warmed input-to-visible p95 within two refresh intervals. Record freezes and p99 as well. A quiet desktop should sleep apart from work its actual sources require.

The last matched physical measurement was an older 52a0afcf binary: 12.1 animation FPS and 43.9 video drops/second, against Niri's 59.9 FPS and 0.5 drops/second. These are historical results, not measurements of the latest changes. The later 1d9581859a19 observation found park-canvas using 70.4% of one core, four compute workers around 9% each, and little aggregate run-queue wait. That workload was uncontrolled and startup performance tracing was disabled. It establishes neither idle acceptance nor the fraction of lag caused by incorrect CPU/GPU placement or serialization.

Already-pushed improvements remove full native updates from client-only changes, retain shared derivations, reduce redundant wakes, move profile writes off the foreground path and avoid some unnecessary layout/raster preparation. Daily acceptance remains open. Do not reimplement those mechanisms merely because the acceptance tasks are unchecked.

## Choose the worst problem first

1. Start with P66, then the CPU/GPU responsibility map P67, using the completed serial-dependency audit in serial-chain/report.txt. Use P36 and P70 where wake attribution or GPU execution is unobserved. Reuse the same captured evidence across these questions; a proven dominant cause can justify its experiment before every audit is complete. The release-build baseline and serial audit are recorded in the evidence index. These architectural assessments come before another round of small caches or getter optimizations.
2. Rank candidates by measured contribution to end-to-end delay: duration on the critical path, repetition rate, input age, freezes, work forced onto other outputs and confidence in the cause. A busy core or low GPU utilization alone is not a diagnosis. Overlapping elapsed spans cannot be added as CPU time.
3. Select the highest-impact supported experiment from the groups below. Group order is a guide, not a requirement to finish every small task before useful parallelism or a major rendering change. Split a dominant exclusive system, investigate main/render overlap or remove a major transfer/wait when that is what the evidence supports.
4. Remove useless work and expose independent work together. Do not postpone parallelism indefinitely behind micro-optimizations. Compare a parallel candidate with the best serial version, including extraction, scheduling, synchronization, total CPU, memory and input latency. Bounded extra CPU or memory may be justified by a substantial responsiveness gain; report the tradeoff, including idle and thermal cost. More occupied cores are not an outcome.
5. Classify work before moving it: CPU semantic/state work, GPU drawing/image work, host submission work, transfer/wait, or a batch whose placement needs comparison. GPU rasterization already exists. Moving small branch-heavy dependency walks to a GPU can introduce transfers and waits. Keeping every operation on the CPU is not a default either.
6. Unbloating is useful when it removes executed work, duplicated ownership/derivation or an obstacle to a clear scheduling boundary. Separately report improvements to build time, startup, binary size and maintenance. Fewer lines or dependencies do not by themselves prove faster interaction.
7. Keep only changes supported by representative results and correctness. Tiny or noisy fixture wins do not justify a desktop regression. If the architectural trace is missing, do bounded source investigation instead of filling the time with speculative small benchmarks.

## Rules for executing one task

Each checkbox is one investigation, experiment or acceptance decision. Its scope is deliberately bounded; if it exposes another independently useful fix, give that fix a new atomic task. Preserve existing IDs when moving tasks. Experiments are conditional on the stated evidence: this is a backlog of theories, not a promise that every theory needs implementation.

Before coding, name the hypothesis, affected shared family, expected critical-path benefit and a correctness check. Compare frozen baseline/candidate sources with the same features, toolchain and workload. Compile both before the timed sequence; keep earlier runs as functional smoke evidence. Warm up, run the complete matched sequence without compilation or competing private GPU scenes, and inspect baseline-to-baseline drift. Record source identity, geometry, timing scope, CPU, memory, allocations/transfers and relevant input age. Start with 30-second physical observations; extend only to resolve a named uncertainty. Reuse existing regressions and do not create another provider/hardware matrix, soak or duplicate suite.

An experiment can close with a measured rejection: discard its implementation and retain the reason below. A product correctness failure, an unfinished check or a required physical result remains pending. An accepted local mechanism can leave daily acceptance IDs open. Additional runs must answer a specific uncertainty; do not spend another full pass tuning a marginal path while a dominant serial stage remains unexplored.

Preserve authored layout/appearance, intrinsic and Fit sizing, live Record state, editing, focus, accessibility and input/IME ordering. Preserve lock and capture privacy, current ownership/UIDs, output mappings, color precision and GPU consumer lifetimes. No result may depend on reducing normal content, silently lowering quality, dropping input or disabling valid features. A visibility optimization is not permission to stop semantic work.

Use cargo check with warnings treated as errors; run Lince/tests through the existing xtask or Nix workflow. Do not start a local release build, restart the session or wait/poll Actions as part of a documentation or source task. Coordinate a necessary physical action individually and wait for the user's answer. Concurrent work must be excluded from a causal comparison; preserve its files. Do not duplicate whole worktrees and recompile the entire dependency graph for each tiny hypothesis when a matching frozen setup can be reused.

## A. Establish the current critical path

- [ ] **P66 — Produce one ranked bottleneck report for the current optimized candidate.**

    **Why:** the previous live binary predates substantial fixes, and the CPU/GPU/serialization breakdown is unknown. **Scope:** verify the running executable and included changes; collect matched quiet, continuous-motion/Firefox and controlled-interaction observations with startup tracing enabled. Separate on-CPU work, waiting, GPU execution, queue age and output presentation. Reuse the profiler and existing physical workload, not an empty desktop. **Close with:** the three largest evidenced contributors, their uncertainty and the next selected task. Record unobserved signals explicitly; no causal percentage from an untraced or mismatched run.

- [ ] **P36 — Attribute the remaining native wake stream.**

    **Why:** unnecessary requests can repeatedly activate the whole shared scene. **Scope:** distinguish input, client pixels, semantic/geometry changes, Record messages, timers, ready results and self-wakes in the traced daily workspace. Correlate requested, coalesced, executed and published work; counters overlap and are not OS wakeups. **Close with:** source-attributed excess wakes, or evidence that current wakes are justified. Existing Command notifications, source-owned deadlines and demand-driven frame updates are the starting implementation, not missing features.

- [ ] **P67 — Map CPU/GPU responsibilities across the active frame path.**

    **Why:** we have not established whether any expensive stage runs on the wrong processor. **Scope:** trace input and scene preparation through Bevy rendering, client import, export/composition and DRM presentation. For each stage record owner thread/device, data size, cadence, transfer/wait dependencies and whether it is CPU-required, GPU-required or a candidate for comparison. Check actual client renderer/device selection; do not infer software rendering from low utilization. **Close with:** an evidence-backed placement table and one justified migration candidate, or a finding that wrong placement is not the current bottleneck.

- [ ] **P70 — Observe GPU execution separately from submission/completion age.**

    **Why:** existing completion elapsed time combines queueing, execution and observation; it cannot assign GPU cost. **Scope:** use supported hardware timestamps or an appropriate existing driver profiler around the dominant native/copy/composition passes. Reuse bounded opt-in reporting, label unavailable measurements and calibrate overhead. **Close with:** a CPU submission/GPU execution/wait timeline suitable for P67. Do not add blocking readback to the normal frame path or treat per-client DRM counters as additive whole-GPU utilization.

## B. Shorten the critical path and enable useful parallelism

These are early architectural candidates after group A. Choose by the critical path, not by their position after smaller interface work.

- [ ] **P43 — Bound input waiting behind the longest native work section.**

    **Why:** one frame-sized wait per event turns typing and dragging into a backlog. **Scope:** shorten or make interruptible the single measured blocking section in runtime/native_input/presentation; batch only compatible motion or scroll work with the same target, mapping epoch, source, units, modifiers, gesture phase and order. Preserve the oldest input origin when coalescing. **Verify:** input age and freezes under load, queue pressure and replacement, keyboard transitions, clicks, IME, modal/lock cancellation and output handoff. Increasing queue capacity alone or prioritizing stale frames is not a fix.

- [ ] **P91 — Remove one proven false scheduling dependency.**

    **Why:** an unnecessary chain can serialize otherwise compatible systems. **Scope:** the serial audit identifies token_style::presentation::apply → icons as a candidate: their output components differ, Tokens caches are Local, and clear independently writes ClearColor. Validate actual schedule access and apply's deferred BorderColor/Outline insertion before removing one edge; preserve sizes-before-consumers and UI/layout barriers. The measured style work is small in the saved traces, so a more valuable edge demonstrated by P66 can take priority. **Verify:** dependency/access analysis plus repeated affected interaction and rendering checks, including same-tick data/style changes. Retain deterministic externally visible behavior. Removing a chain between still-exclusive World systems does not create concurrency.

- [ ] **P60 — Split the highest-cost exclusive system into derivation and application.**

    **Why:** broad mutable world access can block unrelated work for the entire calculation. **Scope:** the serial audit's first candidate is topology::presentation::synchronize, the largest individually instrumented exclusive body in the saved diagnostic traces. First attribute its existing span among input collection, request derivation, global budget allocation and asset/camera application. Split the expensive pure derivation from narrow application using the existing NativePlan/budget family; preserve one shared deterministic byte/priority budget, current source/owner/output revisions and GPU/native resource affinity. If P66 identifies a stronger optimized-workload candidate, select that with its evidence. **Verify:** critical-path/input-age reduction, total CPU and snapshot-copy cost, same-tick updates, stale-result rejection and memory bounds. Include snapshot construction and validation in the comparison; retain the split only when its responsiveness gain justifies the total resource cost.

- [ ] **P68 — Compare processor placement for one evidenced misplaced stage.**

    **Why:** some image/geometry work may benefit from a different processor, while small transfers may erase the gain. **Scope:** choose exactly one stage from P67 and compare the current path with the justified CPU, batched CPU or GPU alternative using the same inputs and precision. Include preparation, transfer, synchronization and result-consumption cost. **Verify:** complete-path latency/CPU/GPU improvement and output correctness under normal load. Reject an isolated fast kernel that makes the interactive path slower or requires a synchronous GPU round trip for each input event.

- [ ] **P61 — Parallelize one sufficiently expensive independent batch.**

    **Why:** substantial independent derivations may shorten the serial native update. **Scope:** use the completed serial audit/P60 to choose one batch. Per-owner surface-request derivation is the first candidate; its shared budget decision remains ordered. Use precise immutable inputs, existing workers and chunks large enough to amortize scheduling; keep dependent parent/child phases ordered. **Verify:** best serial versus parallel total cost, input p95/p99, small scenes, determinism and worker/memory bounds. Retain a cheap serial path where batching overhead dominates. No thread per Castle, item or monitor.

- [ ] **P62 — Test coherent main/render overlap.**

    **Why:** Park explicitly disables Bevy's pipelined rendering, and the serial audit confirms substantial main/render wall spans. **Scope:** NativeCanvas::frame immediately consumes updated geometry/input/images after app.update; establish a coherent frame-epoch handoff before enabling overlap. Respect Rc/NonSend/native-device thread affinity; do not make types superficially shareable with Arc. Render-update wall time is not hardware GPU execution time; use P67/P70 where placement or waits remain unknown. **Verify:** critical-path and input-age improvement, extraction coherence, first-frame readiness, output replacement, lock/capture policy changes and all GPU consumer lifetimes. Keep the existing path if snapshot transfer or synchronization outweighs overlap.

- [ ] **P58 — Compare one versus two frames in flight at the actual bottleneck.**

    **Why:** overlap can hide waits, but rendering ahead of input can make every response older. **Scope:** change one ownership-safe queue boundary identified by P67, keeping input admission and output epochs explicit. **Verify:** presented FPS, input-visible p95/p99, stale/superseded frames, memory and texture/scanout/capture retirement. Use bounded backpressure and cancel superseded unsubmitted work safely. Do not increase buffering across every stage or count higher throughput with older frames as success.

- [ ] **P63 — Limit one background job family that delays visible work.**

    **Why:** useful background throughput can still steal the capacity needed for dispatch and presentation. **Scope:** choose an observed source of CPU contention or lock pressure, then cap/coalesce its concurrency through the existing worker infrastructure. **Verify:** interaction tails, total CPU, completion fairness, cancellation and bounded queues under the same foreground load. Do not starve semantic operations. Change affinity or priority only if run-queue evidence justifies it; the historical sample alone did not show substantial aggregate scheduler waiting.

- [ ] **P72 — Remove one avoidable foreground GPU-completion wait.**

    **Why:** a wait can serialize CPU/GPU work even when both processors look lightly used. **Scope:** identify a real interactive caller from P67/P70 and use an existing completion signal/retained valid frame instead of waiting in input or dispatch. Distinguish diagnostic wait_for_probe calls from production callers; the completion driver already exists. **Verify:** not-ready, failure, cancellation, device/output retirement and privacy epochs, plus measured wait/latency reduction. Preserve synchronization and release buffers only after every consumer finishes.

- [ ] **P73 — Test asynchronous preparation of a stalling shader pipeline.**

    **Why:** synchronous pipeline compilation is explicitly enabled and may cause first-use freezes. **Scope:** only after a cold/warm trace attributes a stall to it, compare asynchronous compilation for that pipeline with the current path. Publish readiness and retain a policy-valid previous presentation while preparing. **Verify:** first-open responsiveness, completed shader/text pixels, errors, removal and capture/lock transitions. Do not merely flip a flag and publish an incomplete frame or start a permanent readiness poll.

- [ ] **P83 — Move one remaining blocking foreground operation off the interaction path.**

    **Why:** serialization, storage, decoding or RPC work can dominate a freeze even when average FPS improves. **Scope:** select the largest remaining operation from P66, including any measured profile-snapshot copying that still happens on the caller. Reuse existing bounded workers and completion signals. **Verify:** foreground tail latency, total work, backpressure, source identity, cancellation, explicit save/shutdown durability and error retry. Profile writes already moved to a worker; do not rebuild that mechanism or hide work in an unbounded detached task.

## C. Stop useless work throughout interface

These target shared paths, not one named Castle. Run them when their cost or wake amplification is visible in group A; preserve semantic work and live editing.

- [ ] **P47 — Remove the worst remaining redundant source wake.**

    **Why:** one frequently duplicated source can keep the entire interface active. **Scope:** choose one P36-attributed cadence or notification-to-ready chain and reuse WakeDeadline/producer notifications. Keep each logical deadline and recompute remaining due times after a shared wake. **Verify:** quiet updates, owner removal, callback replacement, independent later deadlines, held fetches and actual ready-result publication. Command refreshes and measured speech/shader deadlines are already addressed. Reject a global earliest-only throttle or an extra full polling interval after an earlier wake.

- [ ] **P77 — Gate presentation work for one feature family with no live views.**

    **Why:** registering many UI plugins can leave work running when that presentation has no consumers. **Scope:** measure one absent-view family in native/app.rs and its systems, then gate only its visual derivation or UI subscriptions through explicit demand. Registration alone is not proof of waste. **Verify:** zero unnecessary idle work, first open, close/reopen, multiple views and backend events while hidden. Preserve notifications, commands, recording and other semantic activity; avoid a second scheduler/cache per Castle.

- [ ] **P78 — Narrow one overly broad style or layout invalidation.**

    **Why:** an edit to one source should not repeatedly resolve unrelated trees. **Scope:** use resolver/metric counters to select one actual global invalidation and propagate its real dependency scope through existing retained caches. **Verify:** source edits, inheritance, moves, added/removed bindings, authored zero values and dynamic themes; unrelated layout/text ticks should remain stable. Measure invalidation bookkeeping and small scenes. Existing style/metric caches already work; this task is about an evidenced remaining invalidation, not adding another cache layer.

- [ ] **P51 — Reduce validation work in the existing native-surface cache.**

    **Why:** reusing a raster can still require a full node/glyph fingerprint traversal every update. **Scope:** profile topology/surface_render.rs, then make one measured owner/subtree validation incremental using actual content, geometry, asset and policy revisions. **Verify:** changed text/fonts/wrap, clips, new/replaced sources, captures and animated images, including the missing-text/flashing-controls regression. Account for memory and invalidation cost. Do not cache authority decisions, weaken private capture or assume that a retained image can never change.

- [ ] **P80 — Distinguish texture capability from actual texture updates.**

    **Why:** a texture usable as a render attachment is not necessarily rewritten every frame. **Scope:** test producer content/completion epochs for one measured over-invalidating image path in surface_render rather than marking every such image dirty solely by usage flags. Keep a conservative fallback for untracked producers. **Verify:** static retained images, active video/shaders, aliases, producer removal and private/public capture updates. Accept only if repeated raster work falls without stale pixels, feedback loops or missed wakeups.

- [ ] **P74 — Test a translation-only canvas-pan path.**

    **Why:** moving the camera can otherwise repeat content preparation that did not change. **Scope:** retain unaffected native layout/raster content while updating camera, visibility, output mapping and input snapshots for a pure pan. **Verify:** both outputs, pinned/interface content, hit positions, overview/capture demand and large coordinates, with an ordinary full-update fallback for semantic changes. Measure pan latency and preparation cost. Zoom and spatial changes need their own density/geometry decisions; do not silently treat every navigation event as a translation.

- [ ] **P75 — Test transform retention during an ordinary item drag.**

    **Why:** dragging a presentation should not unnecessarily reshape its unchanged text every tick. **Scope:** one shared drag path: update placement/projection while retaining content only where dependencies permit it. **Verify:** crossing Areas, nesting/Fit effects, physics, authored appearance, clipping, pointer capture and output handoff. Entering a different semantic/style context must trigger the normal update. Measure a representative drag, not only a stationary item. Do not special-case one application or infer unchanged ownership from unchanged pixels.

- [ ] **P76 — Test scrolling through one retained presentation transform.**

    **Why:** translating many unchanged children can cost more than updating their shared presentation transform. **Scope:** compare a common scroll-root transform with the current ordinary-content path, inspired by retained scene-graph scrolling. **Verify:** authored units, new/replaced children, clipping, nested scrollers, text editing and hit mapping. Preserve semantic ancestry, owner provenance and depth limits; a wrapper that changes those is not a free optimization. Include pixels, CPU and active-scroll latency. The rejected scroll-state-retirement experiment is not this implementation.

- [ ] **P48 — Define one safe hidden-subtree presentation-demand boundary.**

    **Why:** off-screen/inactive trees still add CPU preparation even when raster cameras are disabled. **Scope:** one contained native subtree family, with demand from every output, overview and capture consumer. Separate skippable presentation from state/subscriptions and sizing dependencies. **Verify:** re-entry after hidden data edits, focus/editor retention, inherited styles, intrinsic/Fit sizes, previews and private capture. Do not use Display::None/Disabled to suspend arbitrary semantics or infer full occlusion from translucent overlap. If containment cannot be proved, retain the necessary work.

- [ ] **P79 — Eliminate one repeated Record/value reconstruction still on the hot path.**

    **Why:** cloning or re-projecting unchanged structured data can consume CPU across many interface items. **Scope:** select one residual path in record_binding/protein_area/custom presentation from P66; use current typed views or source revisions and the existing retained-row infrastructure. **Verify:** same-tick edits, source/UID replacement, deletion/reload, dirty drafts, permissions and row removal. Compare both steady and constantly changing data and bound retained memory. The existing Record-presentation cache is already implemented; a whole-interface serde rewrite is not this task.

## D. Make large interfaces scale with relevant content

- [ ] **P49 — Virtualize one measured large Record-result list.**

    **Why:** layout/entity cost should follow visible rows rather than all loaded Records. **Scope:** visible rows plus a bounded margin, stable UID identity, variable-height bookkeeping and retained edit/focus state. **Verify:** scroll/reveal, sorting/filtering, live inserts/deletes, focused or dirty rows, viewport resizing and source replacement. Row virtualization must not delete Records or lose semantic subscriptions. Reuse appropriate existing scroll infrastructure; prove smaller presentation work under the real large-list workload before generalizing.

- [ ] **P84 — Apply the shared list approach to one other costly native list.**

    **Why:** the same excessive materialization may affect the store, overview or settings lists. **Scope:** choose exactly one list demonstrated costly after P49 or an equivalent existing abstraction is assessed. Preserve keyboard navigation, search, selection and the ability to reveal off-screen controls. **Verify:** node/layout cost versus visible rows, variable content and narrow/wide viewports. Do not virtualize a small list for uniformity or create another competing list framework; name the selected list before implementation.

- [ ] **P50 — Compare an incremental spatial index with one expensive canvas scan.**

    **Why:** a local hit/visibility/relation query may currently scale with the entire canvas. **Scope:** one P66-measured query; include insertion, movement, resizing, removal and origin-shift maintenance. **Verify:** indexed versus scan answers at large coordinates, overlap/clipping, multiple workspaces and outputs, plus total CPU and memory for small and moving scenes. Retain the scan when maintenance costs more. Index candidates are not cached authorization results or a proof of full occlusion.

- [ ] **P85 — Reuse one unchanged pointer-geometry derivation safely.**

    **Why:** rebuilding large input snapshots may delay every native frame. **Scope:** retain only measured pure geometry, keyed by real node/clip/transform/output changes; read action versions, activity, modal and current owner eligibility at the required boundary. **Verify:** stacked/nonblocking recipients, rounded clips, moves, replaced actions, reparenting, focus/IME/lock and queued presses. Include small controls and dense scenes. Previous camera/query and full projection-map experiments were rejected; a distinct measured invalidation strategy is required before revisiting this area.

## E. Correct GPU work, transfers and per-output presentation

- [ ] **P52 — Retain damage independently for each output.**

    **Why:** motion or a client stream on one monitor should not force unnecessary composition on the other. **Scope:** one end-to-end output-damage path through session, composition and retained native/client content; preserve buffer-age history and full-damage reset conditions. **Verify:** one active/one quiet output, partial updates, resize/resume, pointer/overlay changes and lock/capture transitions. Measure actual recomposed work and presentation. Surface content epochs, output damage and protocol callbacks are different signals.

- [ ] **P53 — Pace one output from its presentation clock without a redraw loop.**

    **Why:** coupling outputs or answering undamaged clients too quickly can burn CPU and cause uneven delivery. **Scope:** distinct dirty, queued and waiting states in session/desktop, with callback throttling even when a redraw finds no damage. **Verify:** independent output activity, quiet/off-screen clients, missed deadlines, completion/failure and resume. Compare native frame age and output intervals. Follow Niri's estimated-vblank distinction; do not remove callback service or create a fixed global polling clock.

- [ ] **P55 — Remove one measured redundant render pass or copy.**

    **Why:** hardware acceleration can still waste bandwidth and impose synchronization. **Scope:** select one transition from the P67 pass/transfer map: client texture, native surface, canvas, conversion or final composition. Keep the required precision, alpha, transformations and capture path. **Verify:** equivalent final pixels and privacy, fewer transferred bytes/submissions and lower complete-path cost. Do not remove a copy that is the actual ownership, lifetime or color boundary without replacing that responsibility.

- [ ] **P81 — Retain compatible incoming DMA-BUF import bindings.**

    **Why:** import.rs can allocate a Vulkan wrapper/memory binding before copying into a retained destination. **Scope:** measure and reuse that import setup for genuinely identical live buffers, including format/modifier/planes/offsets/strides and producer identity. **Verify:** release/reuse, FD-number recycling, partial/empty damage, resize, unsupported layouts and producer/output retirement. Bound the retained set and wait for all consumers. Existing damaged-region copying/retained destinations already work; this task does not assume that imports currently use CPU software blits.

- [ ] **P54 — Reuse compatible exported/output resources within a bounded budget.**

    **Why:** replacing output textures/views/export resources can add driver cost and freezes. **Scope:** one measured allocation site in graphics/session, separate from incoming-buffer imports. Key compatibility by actual device, extent, format, usage and ownership epoch. **Verify:** in-flight scanout/capture/native consumers, replacement, output loss and memory returning to a bound. No reuse before completion or unbounded pool. Measure allocation reduction and end-to-end cost; existing safe reuse is retained.

- [ ] **P56 — Improve one demonstrated draw/upload batching break.**

    **Why:** many tiny compatible draws or uploads can dominate host submission work. **Scope:** identify the expensive break first; reuse Bevy's existing batching/atlases where possible. **Verify:** draw/submission cost and exact z-order, alpha, clip, text and capture boundaries. Do not add a new atlas or reorder transparent content by default. Preserve private/public contributor ownership and account for any extra batching analysis or upload work.

- [ ] **P59 — Reduce measured raster-resolution churn during navigation.**

    **Why:** repeated surface replacement near a threshold may cause allocation spikes and stalls. **Scope:** one observed zoom/resize boundary; compare existing bucketing with bounded hysteresis or compatible resource reuse. **Verify:** text/client sharpness, output density, overview demand, budget limits and memory after navigation settles. Keep exact-pixel consumers exact. Do not obtain better performance by silently lowering normal resolution or retaining every encountered size.

- [ ] **P57 — Test direct scanout for one eligible fullscreen SDR client.**

    **Why:** it can remove composition when that pass is actually a measured cost. **Scope:** explicit eligibility for geometry, format, synchronization, color, overlays and capture policy, with the current composition fallback. **Verify:** entry/exit, overlay/lock/capture appearance, replacement, buffer release and the other monitor. No feature disabling to manufacture eligibility. This is conditional and does not explain or solve slow native typing by itself; new HDR/VRR/provider matrices are outside this plan.

## F. Optimize the final release binary

The user accepts longer production compilation when it produces a better final binary. These are potentially broad improvements with small source changes. Keep ordinary development/check profiles fast. This planning update does not authorize or start compilation, and compiler tuning does not replace the CPU/GPU and scheduling work above. Use the established CI/Nix release workflow when an experiment is selected, preserving identical source, features and runtime behavior across variants.

The completed release-profile audit verified the running c9f56419e4ac38dd1d83a9a18d6d66786837c674 binary against its completed CI build log and Nix packaging recipe. Its baseline is Rust 1.99.1 (58cf8f9c3), x86_64-unknown-linux-gnu, ui/instinct/park, opt-level = 3, debug = false and panic = "unwind". The lto, codegen-units and incremental lines are commented: Cargo supplies lto = false (local ThinLTO, not cross-crate LTO), codegen-units = 16 and incremental = false. CI additionally sets CARGO_INCREMENTAL = 0, so uncommenting incremental = false alone does not improve this binary. Codegen units govern compilation partitioning, not runtime thread count. Preserve panic recovery in comparisons. The receipt distinguishes observed flags from inferred Cargo defaults; optimized = true alone does not attest all compiler settings.

The observed shipping RUSTFLAGS are -D warnings -C link-arg=-fuse-ld=mold. They override .cargo/config.toml's target-cpu=native, leaving rustc's target default. The installed Nix derivation repackages this CI artifact without recompiling; a Nix source build instead selects clang/wild, and the development shell can select native CPU instructions. Treat those as separate variants. Twelve recipe inputs matched the shipping commit and all seven existing cache-identity tests passed. The receipt records the source, artifact hashes and baseline identity; current uncommitted work is excluded. For subsequent experiments freeze identical source for both variants and record their effective flags and artifact identities. P100 tests a deliberate target rather than blindly enabling runner-native instructions.

- [ ] **P96 — Compare cross-crate ThinLTO with the current release profile.**

    **Why:** cross-crate optimization may remove overhead across the many interface/backend crate boundaries. **Scope:** compare the recorded baseline profile with release lto = "thin" on identical frozen source, keeping codegen units, target, features and panic behavior fixed. **Verify:** the representative CPU preparation, input latency, FPS, startup and binary-size results, with build/link duration and peak memory recorded. Run the affected correctness/capture checks on the resulting artifact. Accept slower compilation if runtime gains justify it; enabling the flag is not itself evidence of a faster desktop.

- [ ] **P97 — Compare release codegen-units = 1 with the current partition count.**

    **Why:** fewer partitions may give LLVM more optimization opportunities inside each crate. **Scope:** compare one versus the recorded baseline count of 16 while holding LTO and all other options fixed. If the selected LTO mode later changes, repeat only the comparison needed to establish that interaction. **Verify:** complete-path CPU/input performance and small/large scene controls, plus compilation cost and memory. This does not make Park single-threaded at runtime. Do not assume the combined gain is the sum of independent LTO and codegen results.

- [ ] **P98 — Compare full LTO with the best measured ThinLTO release.**

    **Why:** the user prefers final binary quality over short production builds, and full cross-crate optimization may provide an additional gain. **Scope:** test lto = true/"fat" against the P96 result at a fixed codegen count and otherwise identical configuration. **Verify:** representative runtime and correctness, including a control scene, and record linker/toolchain support, peak build memory and CI duration. Full LTO is not guaranteed faster than ThinLTO. If it wins, make the necessary production build budget explicit; retain ThinLTO when full LTO adds cost without a dependable benefit.

- [ ] **P99 — Test representative profile-guided optimization for the selected release profile.**

    **Why:** actual branch/call frequency can guide optimization beyond generic compiler settings. **Scope:** after the major runtime changes stabilize, train an instrumented candidate on normal native content, interaction and client activity; merge profiles with a matching LLVM toolchain and evaluate the final non-instrumented artifact. Keep flags/source consistent between training and use. **Verify:** held-out daily workloads, startup and correctness, stale/missing profile diagnostics and repeatability. Do not train solely on the Firefox FPS fixture or benchmark the instrumented binary as the final result. Keep this conditional on a maintainable release workflow and measured benefit.

- [ ] **P100 — Compare a deliberate CPU target with the shipping target.**

    **Why:** available SIMD/instructions may help some CPU work, while an accidental CI-host target may produce the wrong artifact. **Scope:** establish the actual machine's capabilities, then compare the current target with one supported explicit CPU/ISA target at fixed LTO/codegen settings. A machine-specific Nix build and a distributable CI artifact must have clearly identified target assumptions; do not use target-cpu=native on a random runner as a proxy for this laptop. **Verify:** actual execution, CPU-heavy and interaction workloads, code size and thermal effects. Keep the existing target if there is no measured gain; changing a linker or CPU name alone does not establish better runtime code.

## G. Unbloat the active codebase and interface

These tasks are conditional on a concrete candidate. They are not permission to strip current features, delete the unplugged web crate, edit owner Records/README/AGENTS, or rewrite past migrations. No compatibility layer is retained solely for an unused older version. Keep runtime, startup/build and maintenance benefits separate.

- [ ] **P86 — Rank actual bloat in active paths.**

    **Why:** large files and dependency counts alone do not identify wasted work. **Scope:** inspect active feature/dependency reachability, initialization, registered systems, duplicate derivations/adapters and ownership plumbing across interface and the backend paths P66 implicates. **Close with:** a short ranked inventory naming one removable or consolidatable unit per entry, evidence of its current use, and its runtime, startup/build or maintenance cost. Exclude archived/compiled-out code from FPS claims and do not turn this into a rewrite of every crate.

- [ ] **P87 — Remove one proven obsolete active-code path.**

    **Why:** dead or superseded branches increase maintenance and can retain unnecessary initialization/dispatch work. **Scope:** choose one exact P86 entry, trace callers and feature configurations, then remove its obsolete implementation and wiring without preserving unused-version compatibility. **Verify:** declared current capabilities, applicable checks and any measured resource effect. Protected files and migrations remain untouched. If no executed work changes, describe the benefit as simplification/build cleanup rather than a responsiveness fix.

- [ ] **P88 — Consolidate one duplicated pure coordinate-mapping operation.**

    **Why:** projection/mapping formulas appear in multiple presentation and input paths, increasing derivation cost and disagreement risk. **Scope:** assess host/raster, presentation/mapping and native_input projection/transfer/pointer; share one genuinely identical pure operation. Keep each caller's authority, lifetime and eligibility checks. **Verify:** scaling, rotated/projected geometry, clipping, output epochs, queued presses and replacement. Different native/client/capture policies must not be merged merely because the arithmetic resembles each other.

- [ ] **P89 — Consolidate one redundant derived-state cache or resolver layer.**

    **Why:** overlapping caches can repeat work and obscure invalidation/retirement. **Scope:** select one P86-proven overlap in shared style/metric/geometry/data derivation, reuse the existing canonical family and remove redundant storage/callers. **Verify:** keys, source revisions, zero/restored values, reparenting, removals, memory bounds and small/changing workloads. Do not merge unrelated semantics into an all-purpose global cache or cache permission decisions. Maintain a single explicit invalidation/lifetime contract for the selected value.

- [ ] **P90 — Remove one unused dependency or redundant feature activation.**

    **Why:** unnecessary dependency activation can enlarge builds, startup work and resident resources. **Scope:** choose one P86-proven unused active dependency/flag and verify its transitive uses in the supported shipping configuration. **Verify:** package/check coverage, startup/build or memory change and availability of existing features. Do not disable a supported Castle, decoder or backend to make benchmarks faster. A build-size improvement is worthwhile but must not be reported as a measured frame-time gain without runtime evidence.

- [ ] **P92 — Initialize one unused-at-startup resource family on demand.**

    **Why:** optional backend/watch/decoder/model resources may incur cost before any view or operation needs them. **Scope:** choose an actual startup allocation/worker from P86, retain the feature and coalesce first-use initialization through existing ownership infrastructure. **Verify:** startup and idle resources, first-open latency, simultaneous consumers, close/reopen, cancellation and cleanup. Preserve required background semantic operations. Registering a plugin does not prove its resources initialize eagerly; establish that first.

- [ ] **P93 — Consolidate the performance-comparison runner around existing fixtures.**

    **Why:** duplicated ad-hoc scripts and ambiguous builds have increased validation time and weakened attribution. **Scope:** reuse xtask/park/validation and the existing performance fixtures for one source-stamped baseline/candidate runner. Support matching features, compile-before-timing, warmup, drift/overhead checks, serialization of competing workloads and persistent bounded reports. Preserve independent fixture state and exact selectors with nonzero case counts. **Verify:** a known unchanged control and one existing workload; do not rewrite archived receipts, weaken assertions or add another broad suite. Report saved validation/build effort separately from product runtime.

## H. Daily acceptance gates

These IDs cover both establishing the current baseline and final acceptance. Reuse a valid observation when its revision/workload still applies; do not add duplicate “test it again” tasks. A locally accepted optimization does not close these automatically.

- [ ] **P64 — Establish genuinely quiet daily-workspace resource use.**

    **Why:** static content should not keep unrelated interface work alive. **Scope:** trace the current optimized binary with both screens and ordinary quiet applications; record native updates, source/OS wakes, per-thread CPU, GPU activity and retained resources. Exclude compilation and private rendering tests. **Pass:** intentional clocks and background semantics remain live, unrelated static work sleeps, and resource use is stable and explained. The older uncontrolled 1d9581859a19 sample is not this pass. Reuse P36 attribution instead of collecting another anonymous CPU number.

- [ ] **P65 — Establish prompt visible interaction under normal load.**

    **Why:** throughput alone does not establish usable typing, scrolling, clicking or dragging. **Scope:** both outputs, many native items and Firefox; correlate controlled visual changes with input delivery, native work and the affected output's presentation. **Pass:** warmed input-to-visible p95 within two refresh periods, with freezes and p99 recorded and investigated. Application buffer commits alone do not prove text changed because of the input. Keep clock domains separate and label unobserved causal response; record no private text/clipboard contents. Coordinate any necessary user action individually.

- [ ] **P16 — Establish near-refresh presentation independently on both monitors.**

    **Why:** one fast output or a browser animation counter can hide slow desktop delivery elsewhere. **Scope:** current optimized binary, normal many-item scene and continuous activity on both physical outputs, with resolution, refresh, scale, zoom and geometry recorded. **Pass:** at least 57 FPS per 60 Hz output, without excessive stale frames or increased interaction age. Static content need not submit identical frames at refresh rate. The historical 12.1 FPS result remains evidence of the earlier failure until this matched observation passes.

- [ ] **P15 — Establish nearly lossless Firefox video under that same load.**

    **Why:** decoding frames that are discarded does not deliver smooth video. **Scope:** the same visible 30-second 720p60 source at matched geometry/100% zoom, comparing the current optimized Park candidate with the existing matched Niri reference. Separate decoded, dropped and compositor-presented frames; correlate P16 rather than treating browser callbacks as physical scanout. **Pass:** drops no worse than 0.5 frames/second and no new input/other-output regression. The historical Park result was 43.9 drops/second. No physical run starts without the needed user readiness.

## I. Separate reported correctness issue

- [ ] **P94 — Identify the cause of the database-locked toast.**

    **Why:** contention may stall operations or expose a correctness failure independently of rendering speed. **Scope:** attribute one occurrence to the responsible database/operation and transaction/connection lifetime, using bounded logs without private values. **Close with:** the cause and one concrete fix target, or an explicit missing reproduction. Do not assume it is the FPS cause, increase timeouts to hide it, relax durability, or alter existing migrations. A subsequent implementation gets its own atomic task once its mechanism is known.

## Evidence already available; do not repeat completed mechanisms

These are local implementation/regression receipts, not current physical acceptance. Historical target paths may be absent after reinstall; retain source/workload provenance and do not invent a pass when an artifact is missing. Do not add the diagnostic percentages together.

| Implemented family | Local evidence under target/park-validation/performance-20261104/ |
| --- | --- |
| Shipping release-profile audit: running c9f56419, completed CI log, recipe/artifact identity and seven cache checks; no runtime-performance claim | release-profile/receipt.json; rerun with nix shell --offline nixpkgs#jq --command python3 target/park-validation/performance-20261104/release-profile/audit.py |
| Serial-dependency audit: 14 committed source files and pinned Bevy executor checked against 34 saved reporting windows/495 diagnostic main updates. Exclusive World barriers and the sequential main/render boundary are established; surface preparation is the first conditional split. Untimed main-system costs and optimized daily CPU/GPU attribution remain open | serial-chain/receipt.json, report.txt and stage-summary.json; rerun python3 target/park-validation/performance-20261104/serial-chain/audit.py |
| Stage/wake/input-age reporting | profiler-receipt.json, wake-receipt.json, latency-receipt.json |
| Demand-driven native updates and current queued-click guards | settling/receipt.json, persistence/receipt.json |
| Client-only pixel frames reuse native geometry/input state | pixels/; a private 24-buffer stream required zero full native updates instead of 24 |
| Retained style/metric and Record derivations | style-cache-comparison.json, records/comparison.json |
| Bounded background profile saving and explicit capture provenance | persistence/receipt.json |
| Producer-driven Command history/output and source-owned speech/shader deadlines | notifications/receipt.json, wake-cadence/receipt.json |
| Primary-window query plans and shared capture-source extraction | primary-window/, queries/, allocations/ |
| Required-property metric preparation and admission before text walks | offscreen/, admission/; runtime source includes 85b464ef and 74dc80f1 |
| Layout-solver grid scratch allocated only for nonempty grids; 1,280 bit-exact layout comparisons, six full-frame runs and three text/capture checks passed. A 1,024-node Free/Row/Column fixture went from 3,079 allocator requests to 4; physical FPS improvement remains unproven | layout-scratch/receipt.json |

## Rejected directions and constraints to retain

- Camera/ancestor query retention and one-entry camera-fact reuse did not improve pointer/full-frame cost enough to justify their complexity. See pointer/receipt.json. Do not restore them without distinct new evidence.
- Unconditional full projection maps improved dense-tree fixtures by 3.6–8.0% CPU but made small controls 4.5–5.8% worse; no implementation was kept. See pointer-admission/receipt.json.
- Retiring ordinary-child scroll state at zero offset had an unstable large-scene baseline and 3.4–8.5% worse shallow controls. No implementation was kept. See layout-scroll/receipt.json. This does not settle the separate retained scroll-transform hypothesis P76.
- Guarded companion/camera component writes showed roughly 0.6–1.4% changes alongside a 1.3% control shift; the rewrite was discarded. See changes/receipt.json.
- Typed-child filtering had mixed private diagnostic results: visible-only CPU was 2.1% worse, off-screen 3.1% better and inactive-workspace 7.4% better, with equal pixels. No representative overall win justified adoption; the candidate and its experiment-only test were removed. See typed-children/receipt.json and institute/trails/lince/Build.lingua. Revisit only with distinct evidence, not another pass over the same marginal path.
- Existing text shaping already preserves unrelated glyph/layout storage through targeted edits, inherited font changes and wrap changes. See text/receipt.json. Do not add a blanket second text cache without a new measured defect.
- Capture ownership cannot be inferred solely from scene ancestry: WindowRecord cards and image aliases can be siblings of metadata. Preserve explicit owner/UID/placement provenance and conservative stale/missing-owner behavior.
- A shared deadline must retain independent later requests; dropping a task handle is not cancellation. A shallow pipeline and low utilization do not prove there is no GPU wait. An inactive raster camera does not mean CPU layout work has disappeared.

## Primary research and its application

- [Cargo release profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [rustc code-generation options](https://doc.rust-lang.org/rustc/codegen-options/index.html), [Cargo flag precedence](https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags) and [Rust PGO](https://doc.rust-lang.org/rustc/profile-guided-optimization.html): release defaults/overrides, LTO/codegen tradeoffs, effective target flags and representative training inform the completed baseline audit and P96–P100. These are candidate build-quality improvements; use measured shipping-artifact results rather than assuming every stronger-looking setting wins.
- [Niri redraw loop](https://github.com/niri-wm/niri/wiki/Development:-Redraw-Loop): output-local redraw/wait states and estimated-vblank throttling inform P52/P53. Preserve the undamaged-but-callback-active case.
- [Chromium pipeline architecture](https://developer.chrome.com/docs/chromium/renderingng-architecture) and [RenderingNG structures](https://developer.chrome.com/docs/chromium/renderingng-data-structures): immutable layout outputs, property trees and distinct pipeline responsibilities motivate explicit snapshots and scoped invalidation. Their application to Park remains an experiment, not proof that the same boundaries fit.
- [Qt retained scene-graph renderer](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html): retained GPU geometry, compatible batching and transform roots motivate P74–P76/P56. Preserve transparency and clipping; do not copy Qt's visibility shortcuts into Lince's semantic tree.
- [Godot CPU optimization](https://godotengine.org/article/rendering-cpu-optimizations/), [GPU optimization](https://docs.godotengine.org/en/latest/tutorials/performance/gpu_optimization.html) and [thread-safe APIs](https://docs.godotengine.org/en/stable/tutorials/performance/thread_safe_apis.html): measure the expensive path and separate background data work from mutation of live scene/resources. Thread safety alone does not imply useful scaling.
- [Bevy 0.19.1 systems](https://docs.rs/bevy_ecs/0.19.1/bevy_ecs/system/index.html), [query state](https://docs.rs/bevy_ecs/0.19.1/bevy_ecs/query/struct.QueryState.html) and [UI picking](https://docs.rs/bevy_ui/latest/src/bevy_ui/picking_backend.rs.html): precise access and necessary ordering determine concurrency; camera-group admission can avoid node work. Check APIs against the pinned local sources. The full-map adaptation was rejected rather than assumed beneficial.
- [Khronos profiling](https://docs.vulkan.org/guide/latest/profiling.html), [threading](https://docs.vulkan.org/guide/latest/threading.html) and [frames in flight](https://docs.vulkan.org/tutorial/latest/03_Drawing_a_triangle/03_Drawing/03_Frames_in_flight.html): distinguish host work, actual device execution and synchronization; host parallelism still needs explicit resource synchronization. These inform P67/P70 and P58/P62/P72.
- [CSS containment](https://www.w3.org/TR/css-contain-2/#content-visibility), [Godot occlusion](https://docs.godotengine.org/en/stable/tutorials/3d/occlusion_culling.html) and [Skia recorded drawing](https://github.com/google/skia/blob/main/src/core/SkRecordDraw.cpp): presentation skipping needs conservative bounds and preserved transform/clip/effect, focus and sizing dependencies. Index/containment maintenance must itself earn its cost.
- [Rust stable key sorting](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by_key): equal-key order is part of the existing behavior. The rejected typed-child filtering experiment demonstrates why fewer lookups still need whole-scene controls.
- [Tokio watched changes](https://docs.rs/tokio/1.53.1/tokio/sync/watch/struct.Sender.html), [resettable sleep](https://docs.rs/tokio/latest/tokio/time/struct.Sleep.html), [task lifetime](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html) and [bounded Rust channels](https://doc.rust-lang.org/std/sync/mpsc/fn.sync_channel.html): retain source identity, offsets, explicit cancellation and nonblocking bounded submission. Do not reintroduce polling or unbounded work under a different name.
- [libinput timestamps](https://wayland.freedesktop.org/libinput/doc/latest/timestamps.html): preserve clock-domain distinctions and event ordering when attributing input-visible latency.
