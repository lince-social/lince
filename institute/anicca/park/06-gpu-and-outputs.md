# 6. Use the GPU correctly and present each output independently

[Organization and execution order](Park.md). Targets are in [benchmarks](08-benchmarks.md); interaction coverage is in [workflows](09-workflows.md).

Follow [12](12-measurement-and-delivery.md): P153/P70/P67 finish frame/device/consumer measurements in A0, then independent measured GPU/output fixes join the broad A1 batch. One production design per measured owner, without old/new path switches; compare normal source-identified candidates. Local compilation and private GPU checks in Niri are allowed; their host-specific results do not close physical Park gates. Deliver a broad batch, measure in Park, record outcomes and stop.

This stage is coupled to input, retained presentation and output clocks. P67/P70 must distinguish actual software pixel work, CPU submission, hardware execution, synchronization and repeated copies. A low-utilization GPU does not establish wrong processor placement. Client-only native-frame reuse already exists. Reuse valid prior surfaces/resources and present the latest coherent frame without accumulating latency.

Start with the largest traced cost. Direct scanout is conditional late work for eligible fullscreen clients; it cannot solve native editing/saving. Do not make every window eligible by disabling overlays, transformations, capture or color precision. Bevy rendering is already GPU-backed; an entirely new renderer is justified only by evidence that the existing integration cannot meet the shared budgets. Preserve actual GPU/native thread affinity and every submitted consumer lifetime.


- [ ] **P53 — Pace one output from its presentation clock without a redraw loop.**

    **Why:** coupling outputs or answering undamaged clients too quickly can burn CPU and cause uneven delivery. **Scope:** distinct dirty, queued and waiting states in session/desktop, with callback throttling even when a redraw finds no damage. **Verify:** independent output activity, quiet/off-screen clients, missed deadlines, completion/failure and resume. Compare native frame age and output intervals. Follow Niri's estimated-vblank distinction; do not remove callback service or create a fixed global polling clock.

    **Remaining:** implement estimated-refresh/no-damage throttling and cooperate presentation-only preparation with actual output demand. Verify independent outputs, quiet/off-screen clients, missed deadlines, failure/resume, held multi-node GPU coherence and ready work while an earlier submission is deliberately stalled. Compare frame age and actual output intervals without delaying input, semantic work, capture or another ready output. A fixed global polling clock is not the intended mechanism.

    **Current delivery boundary:** the successful-output callback move and removal of native Started snapshots/channel/wakes are implemented and locally verified; 00 holds the red/green result. They are uncommitted and absent from the running 8c2348ed binary. Finish review/delivery and compare the updated optimized candidate before claiming FPS or latency gains. The original no-progress import and clock controls are already complete.

- [ ] **P55 — Remove one measured redundant render pass or copy.**

    **Why:** the completed direct-target experiments show that removing a copy can expose a larger GLES consumer wait or add allocation/retention cost. **Scope:** select a concrete change that resolves that observed complete-consumer cost or otherwise makes one copy/pass removal worthwhile. Fresh and bounded-pool comparisons are finished and archived in 00/Build.lingua; they are not queued reruns. The production grouped-copy path and its busy fallback remain the baseline.

    **Verify before production:** actual native/GLES/per-output DRM/capture retirement, frozen aliases, source/device/driver identity, teardown/error/hotplug and bounded memory. HAL-imported targets need initialization and reacquisition matching wgpu's tracked state. Filtered capture must complete before display storage becomes read-only to native code or use independent storage. Preserve SDR/sRGB/HDR precision, alpha, transforms, lock/privacy and real-client behavior. Simulated capture leases and an explicit private GLES fence do not prove every real consumer has retired. Retain a change for its complete-path net benefit, including small gains with justified ownership cost.

- [ ] **P68 — Compare processor placement for one evidenced misplaced stage.**

    **Why:** some image/geometry work may benefit from a different processor, while small transfers may erase the gain. **Scope:** choose exactly one stage from P67 and compare the current path with the justified CPU, batched CPU or GPU alternative using the same inputs and precision. Include preparation, transfer, synchronization and result-consumption cost. **Verify:** complete-path latency/CPU/GPU improvement and output correctness under normal load. Reject an isolated fast kernel that makes the interactive path slower or requires a synchronous GPU round trip for each input event.

- [ ] **P52 — Retain damage independently for each output.**

    **Why:** motion or a client stream on one monitor should not force unnecessary composition on the other. **Scope:** one end-to-end output-damage path through session, composition and retained native/client content; preserve buffer-age history and full-damage reset conditions. **Verify:** one active/one quiet output, partial updates, resize/resume, pointer/overlay changes and lock/capture transitions. Measure actual recomposed work and presentation. Surface content epochs, output damage and protocol callbacks are different signals.

- [ ] **P81 — Retain compatible incoming DMA-BUF import bindings.**

    **Why:** import.rs can allocate a Vulkan wrapper/memory binding before copying into a retained destination. **Scope:** measure and reuse that import setup for genuinely identical live buffers, including format/modifier/planes/offsets/strides and producer identity. **Verify:** release/reuse, FD-number recycling, partial/empty damage, resize, unsupported layouts and producer/output retirement. Bound the retained set and wait for all consumers. Existing damaged-region copying/retained destinations already work; this task does not assume that imports currently use CPU software blits.

- [ ] **P54 — Reuse compatible exported/output resources within a bounded budget.**

    **Why:** replacing output textures/views/export resources can add driver cost and freezes. **Scope:** one measured allocation site in graphics/session, separate from incoming-buffer imports. Key compatibility by actual device, extent, format, usage and ownership epoch. **Verify:** in-flight scanout/capture/native consumers, replacement, output loss and memory returning to a bound. No reuse before completion or unbounded pool. Measure allocation reduction and end-to-end cost; existing safe reuse is retained.

- [ ] **P59 — Reduce measured raster-resolution churn during navigation.**

    **Why:** repeated surface replacement near a threshold may cause allocation spikes and stalls. **Scope:** one observed zoom/resize boundary; compare existing bucketing with bounded hysteresis or compatible resource reuse. **Verify:** text/client sharpness, output density, overview demand, budget limits and memory after navigation settles. Keep exact-pixel consumers exact. Do not obtain better performance by silently lowering normal resolution or retaining every encountered size.

- [ ] **P57 — Test direct scanout for one eligible fullscreen SDR client.**

    **Why:** it can remove composition when that pass is actually a measured cost. **Scope:** explicit eligibility for geometry, format, synchronization, color, overlays and capture policy, with the current composition fallback. **Verify:** entry/exit, overlay/lock/capture appearance, replacement, buffer release and the other monitor. No feature disabling to manufacture eligibility. This is conditional and does not explain or solve slow native typing by itself; new HDR/VRR/provider matrices are outside this plan.
