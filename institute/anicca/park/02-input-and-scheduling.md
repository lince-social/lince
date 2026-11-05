# 2. Make input responsive and shorten the critical path

[Organization and execution order](Park.md). Targets are in [benchmarks](08-benchmarks.md); interaction coverage is in [workflows](09-workflows.md).

The shared input/state/worker/render path is the target; do not special-case Firefox, a tooltip or one Castle. Shorten proven blocking spans and remove unrelated dependencies from local feedback. Use immutable snapshots and precise component access for useful concurrency. Rendering a stale backlog faster is insufficient. Source anchors: crates/park/src/runtime.rs, runtime/input_queue.rs, native_input/, presentation.rs; crates/interface/src/host/input/ and native/record_binding.rs. P105–P108 are specific gaps beyond the existing demand-driven worker and queue guards.

- [ ] **P43 — Bound input waiting behind the longest native work section.**

    **Why:** one frame-sized wait per event turns typing and dragging into a backlog. **Scope:** shorten or make interruptible the single measured blocking section in runtime/native_input/presentation; batch only compatible motion or scroll work with the same target, mapping epoch, source, units, modifiers, gesture phase and order. Preserve the oldest input origin when coalescing. **Verify:** input age and freezes under load, queue pressure and replacement, keyboard transitions, clicks, IME, modal/lock cancellation and output handoff. Increasing queue capacity alone or prioritizing stale frames is not a fix.

- [ ] **P105 — Remove the longest unrelated dependency from native text feedback.**

    **Why:** a letter should become visible without waiting for unrelated semantic or rendering work. **Scope:** use P101 to select the dependency blocking one native editable-text path, then retain immediate local editor state and schedule only its required text/layout/presentation changes before slower publication. Reuse EditableText, focus, TextBinding and the host input family; do not add a second editor or fake an overlay caret/text. This may require separating the native input/state application phase from a full app update, with coherent generations. **Verify:** ordered Unicode/IME composition, paste, selection/undo, same-Record multiple views, read-only/revoked access and saving while the backend is held. Compare end-to-end input p95/p99 and whole-update CPU, including dense scenes. The external client path has separate causality. **Needs:** P101/P66; preserve P43 queue guarantees.

- [ ] **P106 — Bound one busy queue drain so control input remains serviceable.**

    **Why:** processing every ready message or subscription result in one indivisible batch can postpone new input even with spare cores. **Scope:** select the measured drain in runtime, Cell delivery or view publication; introduce a bounded work/time quantum and a real continuation signal, preserving per-source order and transactional groups. Give releases, cancellations, focus/lock changes and ready persistence their required service without starving bulk work. **Verify:** burst plus sustained load, no lost/duplicated events, nonempty continuation, empty-queue sleep, eventual completion and bounded memory; compare queue age and input tails. Do not busy-loop, recursively pump the event loop, or add an arbitrary global polling timer. **Needs:** P101/P102 evidence; queue capacity alone is not the fix.

- [ ] **P60 — Split the highest-cost exclusive system into derivation and application.**

    **Why:** broad mutable world access can block unrelated work for the entire calculation. **Scope:** the serial audit's first candidate is topology::presentation::synchronize, the largest individually instrumented exclusive body in the saved diagnostic traces. First attribute its existing span among input collection, request derivation, global budget allocation and asset/camera application. Split the expensive pure derivation from narrow application using the existing NativePlan/budget family; preserve one shared deterministic byte/priority budget, current source/owner/output revisions and GPU/native resource affinity. If P66 identifies a stronger optimized-workload candidate, select that with its evidence. **Verify:** critical-path/input-age reduction, total CPU and snapshot-copy cost, same-tick updates, stale-result rejection and memory bounds. Include snapshot construction and validation in the comparison; retain the split only when its responsiveness gain justifies the total resource cost.

- [ ] **P61 — Parallelize one sufficiently expensive independent batch.**

    **Why:** substantial independent derivations may shorten the serial native update. **Scope:** use the completed serial audit/P60 to choose one batch. Per-owner surface-request derivation is the first candidate; its shared budget decision remains ordered. Use precise immutable inputs, existing workers and chunks large enough to amortize scheduling; keep dependent parent/child phases ordered. **Verify:** best serial versus parallel total cost, input p95/p99, small scenes, determinism and worker/memory bounds. Retain a cheap serial path where batching overhead dominates. No thread per Castle, item or monitor.

- [ ] **P62 — Test coherent main/render overlap.**

    **Why:** Park explicitly disables Bevy's pipelined rendering, and the serial audit confirms substantial main/render wall spans. **Scope:** NativeCanvas::frame immediately consumes updated geometry/input/images after app.update; establish a coherent frame-epoch handoff before enabling overlap. Respect Rc/NonSend/native-device thread affinity; do not make types superficially shareable with Arc. Render-update wall time is not hardware GPU execution time; use P67/P70 where placement or waits remain unknown. **Verify:** critical-path and input-age improvement, extraction coherence, first-frame readiness, output replacement, lock/capture policy changes and all GPU consumer lifetimes. Keep the existing path if snapshot transfer or synchronization outweighs overlap.

- [ ] **P72 — Remove one avoidable foreground GPU-completion wait.**

    **Why:** a wait can serialize CPU/GPU work even when both processors look lightly used. **Scope:** identify a real interactive caller from P67/P70 and use an existing completion signal/retained valid frame instead of waiting in input or dispatch. Distinguish diagnostic wait_for_probe calls from production callers; the completion driver already exists. **Verify:** not-ready, failure, cancellation, device/output retirement and privacy epochs, plus measured wait/latency reduction. Preserve synchronization and release buffers only after every consumer finishes.

- [ ] **P107 — Stop preparing one superseded unsubmitted native frame.**

    **Why:** finishing obsolete expensive presentation work can make every current input wait behind the past. **Scope:** at one P66-proven boundary, detect a superseded scene/output revision before its costly pure work starts or between safe chunks. Preserve semantic mutations, durable operations, input order and already-submitted GPU consumer lifetimes; keep an eligible coherent previous frame until the current one is ready. **Verify:** bursts during rendering, source/output replacement, current hit maps, privacy changes, cancellation and failure, with frame-age and wasted-work counts. A latest-value frame channel is already present; establish residual wasted preparation before adding cancellation. Never discard keyboard transitions or pretend a completed old frame used a newer input. **Coordinate with:** P58/P62; this is bounded stale-work avoidance, not extra buffering.

- [ ] **P58 — Compare one versus two frames in flight at the actual bottleneck.**

    **Why:** overlap can hide waits, but rendering ahead of input can make every response older. **Scope:** change one ownership-safe queue boundary identified by P67, keeping input admission and output epochs explicit. **Verify:** presented FPS, input-visible p95/p99, stale/superseded frames, memory and texture/scanout/capture retirement. Use bounded backpressure and cancel superseded unsubmitted work safely. Do not increase buffering across every stage or count higher throughput with older frames as success.

- [ ] **P47 — Remove the worst remaining redundant source wake.**

    **Why:** one frequently duplicated source can keep the entire interface active. **Scope:** choose one P36-attributed cadence or notification-to-ready chain and reuse WakeDeadline/producer notifications. Keep each logical deadline and recompute remaining due times after a shared wake. **Verify:** quiet updates, owner removal, callback replacement, independent later deadlines, held fetches and actual ready-result publication. Command refreshes and measured speech/shader deadlines are already addressed. Reject a global earliest-only throttle or an extra full polling interval after an earlier wake.

- [ ] **P108 — Replace the editor binding heartbeat with actual readiness and deadlines.**

    **Why:** record_binding currently arms a 150 ms wake while a text binding is focused or work is pending, even though ready storage/Cell events also notify. **Scope:** attribute necessary join retries, inflight retries, presence refresh and view retention, then express their distinct due times through existing source-owned WakeDeadline/producer notifications. Retain earliest and later independent deadlines; emit presence only at its required cadence and after real selection changes/lease refresh. **Verify:** focused-but-unchanged editing sleeps between required deadlines, fast completions appear without the heartbeat, full queues recover, retries/cancellation continue and removed editors retire requests. Measure wakes and unchanged work, including multiple editors. Do not simply remove the heartbeat until all necessary liveness paths have a proven signal. **Relates to:** P36/P47, but targets this concrete binding family.

- [ ] **P63 — Limit one background job family that delays visible work.**

    **Why:** useful background throughput can still steal the capacity needed for dispatch and presentation. **Scope:** choose an observed source of CPU contention or lock pressure, then cap/coalesce its concurrency through the existing worker infrastructure. **Verify:** interaction tails, total CPU, completion fairness, cancellation and bounded queues under the same foreground load. Do not starve semantic operations. Change affinity or priority only if run-queue evidence justifies it; the historical sample alone did not show substantial aggregate scheduler waiting.

- [ ] **P83 — Move one remaining blocking foreground operation off the interaction path.**

    **Why:** serialization, storage, decoding or RPC work can dominate a freeze even when average FPS improves. **Scope:** select the largest remaining operation from P66, including any measured profile-snapshot copying that still happens on the caller. Reuse existing bounded workers and completion signals. **Verify:** foreground tail latency, total work, backpressure, source identity, cancellation, explicit save/shutdown durability and error retry. Profile writes already moved to a worker; do not rebuild that mechanism or hide work in an unbounded detached task.

- [ ] **P73 — Test asynchronous preparation of a stalling shader pipeline.**

    **Why:** synchronous pipeline compilation is explicitly enabled and may cause first-use freezes. **Scope:** only after a cold/warm trace attributes a stall to it, compare asynchronous compilation for that pipeline with the current path. Publish readiness and retain a policy-valid previous presentation while preparing. **Verify:** first-open responsiveness, completed shader/text pixels, errors, removal and capture/lock transitions. Do not merely flip a flag and publish an incomplete frame or start a permanent readiness poll.
