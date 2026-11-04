# Park: performance work

The remaining planned work is performance and responsiveness throughout the desktop. Ordinary Firefox use, typing, clicking, scrolling, dragging canvas items and opening windows are all affected. Better FPS does not complete this work if input waits in queues or appears late.

## Required outcomes

- Both monitors stay close to 60 FPS during movement and animation with a representative daily workspace containing many native canvas items, Ghostty and Firefox. Measure each output independently. A static desktop should sleep instead of producing 60 identical frames per second.
- Firefox video loses nearly no frames under that same load. First meet or beat the matched direct-Niri reference, then investigate remaining losses. Separate decoded, dropped and presented frames: decoder progress and browser callbacks do not establish physical smoothness.
- Typing, clicking, scrolling and dragging respond promptly under that load. Distinguish input delivery, native update and presentation latency; a stream of stale frames can look smooth while interaction remains slow.

Use the real daily workspace for acceptance. Private fixtures diagnose causes and verify mechanisms; a dedicated quiet machine establishes maximum throughput, not daily-use acceptance. Match resolution, refresh, scale, zoom, visible content and Firefox version. Begin with 30-second measurements and extend only for a concrete uncertainty. Initial engineering gates: at least 57 FPS per 60 Hz output during continuous activity; video drops no worse than Niri's existing 0.5 frames/second; warmed input-to-visible p95 within two refresh intervals. Refine against matched measurements and record freezes and p99 latency alongside averages.

## Evidence and interpretation

Optimized 52a0afcf, at 100% zoom with tiling enabled and visible 1920×1080 Firefox, measured 12.1 animation FPS and 43.9 dropped video frames/second over 30 seconds. Direct Niri measured 59.9 FPS and 0.5 dropped frames/second. Receipt: target/park-validation/fixes-20261102/park-browser-1793756312562063054/results.json. The user also reports delayed ordinary typing and dragging.

The real session's park-canvas thread used about 80% of one CPU core with Firefox closed. Component lookup, allocation and style resolution appear in samples. The later loaded diagnostic ran on the laptop at 1366×768 and is not a matched Niri comparison. These identify suspects, not exact whole-system causal percentages; low GPU activity does not rule out synchronization or bandwidth stalls. Profiles: target/park-validation/fixes-20261102/live-performance-20261103/.

## Execution and shared families

Keep three tracks: correct CPU/GPU placement, useful parallelism, and less work during activity and idle. Execute numbered work in order. Each experiment needs a baseline, an isolated change, correctness checks and an after measurement. Keep only measured improvements; reject complexity without a benefit. Remove completed tasks. Push locally validated changes that need a new installed binary, and continue independent local work while the user sleeps; do not wait for Actions or restart the user's session. Leave physical acceptance open for the new binary.

Use shared native presentation and derived state in interface/native; input and scene delivery in park/runtime, park/desktop and park/native_input; and output rendering, imports and completion in park/session, park/graphics and park/nested/renderer. Preserve editing, inherited appearance, focus, input ordering, IME, lock privacy, capture exclusion, color precision and GPU resource lifetimes. Reuse the regression against missing text and flashing controls.

GPU rasterization is appropriate; moving small dependency walks to the GPU may cost more than it saves. More occupied cores, deeper queues and larger caches are not success metrics. Remove work before distributing it. Use precise dependencies, immutable inputs, bounded memory and explicit invalidation. Preserve the actual experience rather than lowering resolution or hiding existing canvas content to obtain better numbers.

## Measurement: responsiveness and the critical path

The stage profiler is opt-in with LINCE_TRACE_PERFORMANCE=1 and emits bounded JSON reports. Spans measure elapsed wall time, can overlap, and must not be summed as CPU time. Submission-to-completion measures observed queue/completion age, not hardware GPU execution. Per-output intervals identify DRM or dispatch fallback clocks; static content need not submit every refresh. Reports retain quantile upper bounds and lost-report counts. Local validation: target/park-validation/performance-20261104/profiler-receipt.json.

Input timing is enabled by the same profiler flag. It retains oldest native batch origins and queued/pointer serial watermarks through frame preparation, publication and per-output presentation. Device, dispatch and DRM clocks stay distinct; native frame age measures prepared-frame delivery. Application timings pair the latest forwarded input with a newer attached buffer of its surface tree, which does not establish visible text, event causation or damage-only updates. Match controlled interactions with the affected output for P65; neither buffer proxies nor FPS prove semantic response. A clock marked unobserved has no interval samples. No text or clipboard contents are recorded. Local validation: target/park-validation/performance-20261104/latency-receipt.json.

- [ ] **P36 — Measure native scene wake causes.** Count buffer-only commits, geometry/style changes, Record messages, timers and self-wakes in the daily workspace. **Why:** a quiet laptop workspace shares work with the other output and the rest of Lince.

## Less work on each core

- [ ] **P38 — Retain derived Record presentation by revision.** Refresh affected rows, labels and declarations only after relevant data changes. **Why:** unrelated client frames should not clone or rebuild unchanged native views.

- [ ] **P39 — Retain query state in measured hot callers.** Replace newly constructed queries or broad World access where profiling justifies typed cached access. **Why:** repeated archetype discovery and component lookup scale poorly. Avoid a mechanical repository-wide rewrite.

- [ ] **P40 — Reduce measured allocation and serialization.** Reuse bounded scratch storage and immutable snapshots; convert JSON at actual inspection and persistence boundaries. **Why:** copying and allocation are visible costs, but changing every serializer would add unsupported scope.

- [ ] **P41 — Prevent false change notifications.** Avoid hot writes and insertions when values are unchanged. **Why:** a redundant component change can invalidate layout, text, capture and extraction.

- [ ] **P42 — Retain unchanged text layout.** Preserve shaping, glyph and intrinsic measurement state while text and constraints stay the same. **Why:** text is costly, and the former global reset caused disappearing labels and flashing controls.

## Interaction latency

- [ ] **P43 — Prevent input backlog behind scene work.** Shorten measured exclusive sections and batch compatible input, preserving keyboard transitions, clicks, IME and scroll target/order. **Why:** a frame-sized delay per queued event compounds into visible lag; increasing capacity alone worsens age.

- [ ] **P44 — Separate client pixels from semantic scene updates.** Retain native geometry, focus and hit-test state when only an application buffer changes. **Why:** Firefox animation should not rebuild the rest of Lince. Preserve resize, popup, capture and ownership revisions.

- [ ] **P45 — Remove unnecessary settling frames.** Request follow-up updates only for actual pending layout, animation, import or input work. **Why:** blanket settling multiplies the cost of every interaction; required settling needs an explicit owner.

- [ ] **P46 — Keep heavy background operations outside input handling.** Use bounded workers for measured blocking I/O, persistence, decoding or projection, publishing versioned results. **Why:** a slow data operation must not delay typing or pointer movement. Reject obsolete results.

## Idle and large canvases

- [ ] **P47 — Wake each source at its required cadence.** Use deadlines for clocks, animation and pending operations; sleep without visual or semantic work. **Why:** static content should not continuously poll storage or request frames.

- [ ] **P48 — Exclude invisible presentation from expensive preparation.** Account for output visibility, clipping, coverage and capture demand while retaining necessary subscriptions and state. **Why:** hidden items need not receive visible-item raster and layout work.

- [ ] **P49 — Virtualize measured large lists.** Present visible rows plus a small margin while preserving identities and edit/focus state. **Why:** long Record and settings lists should cost roughly their visible portion.

- [ ] **P50 — Measure spatial indexing for large canvas queries.** Compare incremental indices against existing scans for visibility, hit testing and relevant relations. **Why:** local interactions should stay local; maintenance overhead may outweigh indexing in small scenes.

- [ ] **P51 — Retain unchanged native surfaces.** Invalidate for actual content, transform, scale and capture-policy changes rather than unrelated activity. **Why:** cached presentation avoids repeated rasterization without freezing updates or leaking captured content.

## Correct CPU/GPU placement and output delivery

- [ ] **P52 — Track damage independently per output.** Recompose affected regions and retain buffer-age history, resetting after resize or resume. **Why:** activity on one monitor should not force full-screen work on both.

- [ ] **P53 — Pace outputs from their presentation clocks.** Keep dirty, queued and waiting states distinct and throttle callbacks for undamaged frames. **Why:** busy loops waste CPU and coupling monitors creates avoidable stalls. Follow Niri's redraw principles.

- [ ] **P54 — Reuse GPU import/export resources safely.** Pool compatible images and views within a bounded budget; wait for all consumers before reuse. **Why:** repeated allocation and import add driver and memory costs. Scanout, clients and capture retain ownership until completion.

- [ ] **P55 — Eliminate measured redundant passes and copies.** Account for client texture, native surface, canvas, color conversion and scanout stages. **Why:** acceleration can still waste bandwidth. Preserve alpha, transforms and output precision.

- [ ] **P56 — Batch compatible draws and uploads.** Compare batching, atlases and fewer small submissions while preserving z-order and clip boundaries. **Why:** retained scene and game renderers reduce driver overhead by batching compatible work.

- [ ] **P57 — Test eligible fullscreen direct scanout.** Require compatible geometry, format, overlays, color and capture policy, with a correct composition fallback. **Why:** it can avoid composition but does not solve sluggish native interaction by itself.

- [ ] **P58 — Bound rendering ahead of input.** Compare one and two frames in flight where ownership permits overlap; measure latency and memory too. **Why:** overlap can help CPU/GPU use while excess buffering makes interaction older.

- [ ] **P59 — Stabilize surface resolution during navigation.** Measure zoom/resize allocation churn and compare bounded reuse or hysteresis. **Why:** resource replacement can cause freezes; silently lowering text or client quality is not an acceptable improvement.

## Useful parallelism

- [ ] **P60 — Split measured exclusive systems by data ownership.** Separate read-only derivation from narrow mutation so Bevy can schedule independent systems together. **Why:** precise access enables useful parallelism without a thread per Castle or monitor.

- [ ] **P61 — Batch independent expensive derivations.** Evaluate parallel layout preparation, visibility, geometry or data projection only where those costs dominate. **Why:** substantial jobs may scale; tiny tasks and shared locks can erase the benefit.

- [ ] **P62 — Measure main/render overlap.** Evaluate immutable retained render snapshots and the currently disabled pipelined rendering path. **Why:** presentation may continue during native updates, but coherent extraction, input and GPU ownership must survive.

- [ ] **P63 — Bound background CPU work.** Reserve capacity for dispatch and visible interaction; defer eligible background work without starving it. **Why:** filling every worker can worsen tail latency. Affinity and priority changes require measurements.

## Validation and acceptance

- [ ] **P64 — Validate idle use in the daily workspace.** Measure wakeups, per-thread CPU and GPU activity with quiet applications and normal native content. **Why:** intentional clocks must remain live while unrelated static work sleeps.

- [ ] **P65 — Validate interaction latency under load.** Measure typing, clicks, scrolling and dragging with both monitors and Firefox active, including freezes and p95/p99 age. **Why:** responsiveness is an acceptance requirement separate from throughput.

    The shared style and metric paths now retain resolved values, invalidate conservatively on relevant source changes and retire removed entities. Typed bindings and retained queries remove repeated ancestor walks and entity collections. The latest paired diagnostic reduced large unchanged updates from about 204 ms to 33 ms, changing-theme updates from 196 ms to 122 ms, and total CPU time by about 63% relative to the first metric batch. Ten behavior cases and seven rendering/capture checks pass. These are diagnostic-build results; install the new optimized binary and measure actual typing, scrolling and dragging. If relevant source changes repeatedly invalidate the whole cache in the daily scene, narrow invalidation from that evidence. Comparison: target/park-validation/performance-20261104/style-cache-comparison.json.

- [ ] **P16 — Validate near-60-FPS presentation on both monitors.** Use the daily many-item workspace and Firefox; measure each output during continuous motion against its refresh rate. **Why:** one fast monitor or browser-only counter cannot establish desktop performance.

    Current matched live result: 12.1 FPS. Retest after installing a new optimized binary containing the validated changes.

- [ ] **P15 — Validate nearly lossless Firefox video.** Run the same visible 30-second 720p60 source under the daily workload at matched geometry and zoom. Compare drops with the 0.5 frames/second Niri reference and correlate presentation. **Why:** decoding 60 frames/second while discarding most is not successful playback.

    Current matched live result: 43.9 dropped frames/second. Retest after the new optimized binary is installed.

Run existing regressions appropriate to each changed family: retained text through UI replacement; theme and authored appearance; input/focus/IME order; two-output placement; asynchronous GPU completion; and public/private capture. Add targeted tests for new mechanisms without repeating unrelated functional acceptance. Check retained memory and resource counts when adding caches or pools. Keep revision, workload, geometry, warmup, interval and artifact paths with results. Profiled or nested timings remain diagnostic rather than physical acceptance.

## Research behind the experiments

- [libinput timestamps](https://wayland.freedesktop.org/libinput/doc/latest/timestamps.html): physical event timestamps use CLOCK_MONOTONIC and may arrive out of timestamp order; nested Wayland timestamps have a different origin.
- [Niri redraw loop](https://github.com/niri-wm/niri/wiki/Development:-Redraw-Loop): per-output state, damage and paced callbacks.
- [Qt Quick scene graph](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html) and [renderer](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html): retained state, changed-item synchronization, geometry reuse and batching.
- [Chromium RenderingNG](https://developer.chrome.com/docs/chromium/renderingng-architecture): distinct pipeline stages and isolation of input/compositing from expensive main-thread work.
- [Bevy 0.19.1 systems](https://docs.rs/bevy_ecs/0.19.1/bevy_ecs/system/index.html), [query state](https://docs.rs/bevy_ecs/0.19.1/bevy_ecs/query/struct.QueryState.html) and [parallel query example](https://github.com/bevyengine/bevy/blob/main/examples/ecs/parallel_query.rs): cached typed access, precise read/write sets and batched iteration. Verify exact APIs against local 0.19.1 sources.
- [Godot CPU rendering optimization](https://godotengine.org/article/rendering-cpu-optimizations/) and [GPU optimization](https://docs.godotengine.org/en/latest/tutorials/performance/gpu_optimization.html): isolate causes, measure after each change and distinguish preparation, driver and GPU limits.
- [Khronos profiling](https://docs.vulkan.org/guide/latest/profiling.html) and [frames in flight](https://docs.vulkan.org/tutorial/latest/03_Drawing_a_triangle/03_Drawing/03_Frames_in_flight.html): submission costs, safe resource reuse and the overlap/latency tradeoff.

