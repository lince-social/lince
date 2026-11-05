# 1. Measure complete interactions

[Organization and execution order](Park.md). Targets are in [benchmarks](08-benchmarks.md); interaction coverage is in [workflows](09-workflows.md).

Measure command-to-effect latency first, including native typing and local Record saving. Existing counters/timing histograms and the private renderer fixtures are reusable; add only the missing causal links. An observation of high CPU or a stage's elapsed duration is not attribution of user-visible delay. The fixture manifest comes first, then fill missing trace signals, then rank causes. The same trace serves scheduling, storage, style and GPU questions.

- [ ] **P103 — Freeze the representative workload and executable manifest.**

    **Why:** the old slow measurement, the current checkout and the running executable may differ. **Scope:** define the small, normal and dense scenes in 08 and the workflow/factor registry in 09 using deterministic synthetic data, Record/source identities, output geometry/scale/refresh and feature counts. Identify the actual optimized binary, complete source/features/flags and hardware/device/thermal conditions; preserve an unchanged control and the matched Niri comparison inputs. **Close with:** a reusable manifest and explicit missing setup, not a screenshot or claim that a synthetic scene equals daily use. Native hidden/off-screen consumers, multiple views and active backgrounds must be represented; private user text is unnecessary. Reuse the existing validation fixtures rather than making another scene framework. **First in stage 1;** later tasks reuse this exact fixture identity.

- [ ] **P101 — Trace one input all the way to its correct visible effect.**

    **Why:** input queue statistics and FPS can both improve while typing remains late. **Scope:** extend the existing latency/performance reporting only where necessary with a bounded correlation ID across device/dispatch, queue admission, native editor or client delivery, semantic application, frame preparation, GPU completion and affected-output presentation. Include oldest age, queue residence, long indivisible work and a controlled visible response oracle. Keep raw text, passwords and clipboard values out of traces; document clock domains and unsupported physical visibility signals. **Close with:** one causal native typing/drag trace and one external-client trace identifying observed versus inferred boundaries, plus tracing-off overhead control. Reuse host/performance and runtime/latency; do not build another profiler or infer response from an arbitrary later frame. **Feeds:** P66, P43 and P65.

- [ ] **P102 — Trace one local Record edit through durability and view publication.**

    **Why:** a title/description save currently has several possible amplification and wait boundaries. **Scope:** on a private synthetic local Record, correlate editor revision, draft preparation/encoding, draft-write queue/acquire/commit, ChangeRecord admission, access scope, canonical transaction, acknowledgement, query/collaboration refresh, affected-view update and visible presentation. Count SQL statements, transaction/fsync boundaries where observable, snapshots/bytes, clones, query executions and views touched. Measure head and body separately at 1 KiB/64 KiB/1 MiB body sizes and one versus eight views. **Close with:** a critical-path/work-amplification report distinguishing immediate echo, durable recovery, canonical save, publication and remote sync, with no network dependency on the local baseline. A blocked writer scenario is a correctness probe, not the normal latency result. **Feeds:** stage 3; use actual file-backed storage for durability costs.

- [ ] **P66 — Produce one ranked bottleneck report for the current optimized candidate.**

    **Why:** the previous live binary predates substantial fixes, and the CPU/GPU/serialization breakdown is unknown. **Scope:** verify the running executable and included changes; collect matched quiet, continuous-motion/Firefox and controlled-interaction observations with startup tracing enabled. Separate on-CPU work, waiting, GPU execution, queue age and output presentation. Reuse the profiler and existing physical workload, not an empty desktop. **Close with:** the three largest evidenced contributors, their uncertainty and the next selected task. Record unobserved signals explicitly; no causal percentage from an untraced or mismatched run.

- [ ] **P36 — Attribute the remaining native wake stream.**

    **Why:** unnecessary requests can repeatedly activate the whole shared scene. **Scope:** distinguish input, client pixels, semantic/geometry changes, Record messages, timers, ready results and self-wakes in the traced daily workspace. Correlate requested, coalesced, executed and published work; counters overlap and are not OS wakeups. **Close with:** source-attributed excess wakes, or evidence that current wakes are justified. Existing Command notifications, source-owned deadlines and demand-driven frame updates are the starting implementation, not missing features.

- [ ] **P67 — Map CPU/GPU responsibilities across the active frame path.**

    **Why:** we have not established whether any expensive stage runs on the wrong processor. **Scope:** trace input and scene preparation through Bevy rendering, client import, export/composition and DRM presentation. For each stage record owner thread/device, data size, cadence, transfer/wait dependencies and whether it is CPU-required, GPU-required or a candidate for comparison. Check actual client renderer/device selection; do not infer software rendering from low utilization. **Close with:** an evidence-backed placement table and one justified migration candidate, or a finding that wrong placement is not the current bottleneck.

- [ ] **P70 — Observe GPU execution separately from submission/completion age.**

    **Why:** existing completion elapsed time combines queueing, execution and observation; it cannot assign GPU cost. **Scope:** use supported hardware timestamps or an appropriate existing driver profiler around the dominant native/copy/composition passes. Reuse bounded opt-in reporting, label unavailable measurements and calibrate overhead. **Close with:** a CPU submission/GPU execution/wait timeline suitable for P67. Do not add blocking readback to the normal frame path or treat per-client DRM counters as additive whole-GPU utilization.

- [ ] **P93 — Consolidate the performance-comparison runner around existing fixtures.**

    **Why:** duplicated ad-hoc scripts and ambiguous builds have increased validation time and weakened attribution. **Scope:** reuse xtask/park/validation and the existing performance fixtures for one source-stamped baseline/candidate runner. Support matching features, compile-before-timing, warmup, drift/overhead checks, serialization of competing workloads and persistent bounded reports. Preserve independent fixture state and exact selectors with nonzero case counts. **Verify:** a known unchanged control and one existing workload; do not rewrite archived receipts, weaken assertions or add another broad suite. Report saved validation/build effort separately from product runtime.

- [ ] **P104 — Expose the existing performance cases as useful Laboratory results.**

    **Why:** ordinary tests must catch expensive work and the user needs a repeatable way to check the new behavior. **Scope:** extend the current Laboratory catalogue/driver and xtask report wiring for the selected workflow cases, work-count assertions and the benchmark metrics in 08. Show case/source/workload identity, observed values, target, timing scope, missing hardware evidence and cancellation. Reuse headless CPU cases and hardware cases through the same result model, without running physical-session acceptance inside an isolated fixture. **Verify:** known passing/failing controls, exact nonzero selectors, Stop/Close cleanup, temporary data isolation and no writes to the user's Records. A debug duration is not an optimized desktop pass. This is integration of the agreed measurements, not a new general benchmark product. **Needs:** P93 and the first usable P101/P102 cases.
