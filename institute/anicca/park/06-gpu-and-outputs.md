# 6. Use the GPU correctly and present each output independently

[Organization and execution order](Park.md). Targets are in [benchmarks](08-benchmarks.md); interaction coverage is in [workflows](09-workflows.md).

This stage is coupled to input, retained presentation and output clocks. P67/P70 must distinguish actual software pixel work, CPU submission, hardware execution, synchronization and repeated copies. A low-utilization GPU does not establish wrong processor placement. Client-only native-frame reuse already exists. Reuse valid prior surfaces/resources and present the latest coherent frame without accumulating latency.

Start with the largest traced cost. Direct scanout is conditional late work for eligible fullscreen clients; it cannot solve native editing/saving. Do not make every window eligible by disabling overlays, transformations, capture or color precision. Bevy rendering is already GPU-backed; an entirely new renderer is justified only by evidence that the existing integration cannot meet the shared budgets. Preserve actual GPU/native thread affinity and every submitted consumer lifetime.


- [ ] **P55 — Remove one measured redundant render pass or copy.**

    **Why:** hardware acceleration can still waste bandwidth and impose synchronization. **Scope:** select one transition from the P67 pass/transfer map: client texture, native surface, canvas, conversion or final composition. Keep the required precision, alpha, transformations and capture path. **Verify:** equivalent final pixels and privacy, fewer transferred bytes/submissions and lower complete-path cost. Do not remove a copy that is the actual ownership, lifetime or color boundary without replacing that responsibility.

    **Fresh-target comparison completed:** 00 records the private full/pixel-frame experiment at one/two monitor sizes, with actual GLES import/draw/fence, prepared-target DMA-BUF identity, unique per-frame HDR stamps and retained old-image checks. Fresh direct targets have small full-update gains or near ties, but make the important pixel-only path slower in both process orders. Allocation/zero initialization/registration/view replacement costs 3.2–5.2 ms here; a native-ready result also understates later GLES consumption in some active-GPU cases. The production batched-copy path stays enabled. Reuse this completed control rather than rebuilding it unchanged.

    **Bounded reuse comparison completed:** 00 records the corrected lease/GPU-retirement pool, exact clearing/precision, retained/exhausted slots, replacement, abandoned-work cleanup and repeated native/GLES comparison. Setup falls to about 0.6 ms; full updates improve about 4–9% with less CPU, but pixel elapsed results remain mixed and consuming old pixel frames during new GPU work is materially slower. Six retained groups also cost about 100/150 MB for one/two outputs. Keep the test-only candidate and production copies. Do not repeat either completed comparison unchanged; P60's larger main work now comes first.

    **Remaining decision:** identify a specific change that addresses the demonstrated complete-consumer wait or justifies integration with its resource cost. Before production reuse, prove actual native/GLES/per-output DRM/capture retirement, including frozen aliases, teardown/error/hotplug and source/device/driver identity; simulated capture leases and the private explicit GLES fence are insufficient. HAL-imported targets remain initialized, and reacquisition must match wgpu's tracked state. Filtered capture must finish before exported display storage becomes read-only to native code or use independent storage. Complete supported SDR/sRGB/HDR/alpha and real-client checks. Preserve bounded admission, busy copy fallback and immutable old frames. Retain a production change only when its complete-path net value justifies ownership/maintenance cost; small gains remain eligible. P55 stays open for this remaining justified change, not more unconstrained pool tuning.

- [ ] **P68 — Compare processor placement for one evidenced misplaced stage.**

    **Why:** some image/geometry work may benefit from a different processor, while small transfers may erase the gain. **Scope:** choose exactly one stage from P67 and compare the current path with the justified CPU, batched CPU or GPU alternative using the same inputs and precision. Include preparation, transfer, synchronization and result-consumption cost. **Verify:** complete-path latency/CPU/GPU improvement and output correctness under normal load. Reject an isolated fast kernel that makes the interactive path slower or requires a synchronous GPU round trip for each input event.

- [ ] **P52 — Retain damage independently for each output.**

    **Why:** motion or a client stream on one monitor should not force unnecessary composition on the other. **Scope:** one end-to-end output-damage path through session, composition and retained native/client content; preserve buffer-age history and full-damage reset conditions. **Verify:** one active/one quiet output, partial updates, resize/resume, pointer/overlay changes and lock/capture transitions. Measure actual recomposed work and presentation. Surface content epochs, output damage and protocol callbacks are different signals.

- [ ] **P53 — Pace one output from its presentation clock without a redraw loop.**

    **Why:** coupling outputs or answering undamaged clients too quickly can burn CPU and cause uneven delivery. **Scope:** distinct dirty, queued and waiting states in session/desktop, with callback throttling even when a redraw finds no damage. **Verify:** independent output activity, quiet/off-screen clients, missed deadlines, completion/failure and resume. Compare native frame age and output intervals. Follow Niri's estimated-vblank distinction; do not remove callback service or create a fixed global polling clock.

- [ ] **P81 — Retain compatible incoming DMA-BUF import bindings.**

    **Why:** import.rs can allocate a Vulkan wrapper/memory binding before copying into a retained destination. **Scope:** measure and reuse that import setup for genuinely identical live buffers, including format/modifier/planes/offsets/strides and producer identity. **Verify:** release/reuse, FD-number recycling, partial/empty damage, resize, unsupported layouts and producer/output retirement. Bound the retained set and wait for all consumers. Existing damaged-region copying/retained destinations already work; this task does not assume that imports currently use CPU software blits.

- [ ] **P54 — Reuse compatible exported/output resources within a bounded budget.**

    **Why:** replacing output textures/views/export resources can add driver cost and freezes. **Scope:** one measured allocation site in graphics/session, separate from incoming-buffer imports. Key compatibility by actual device, extent, format, usage and ownership epoch. **Verify:** in-flight scanout/capture/native consumers, replacement, output loss and memory returning to a bound. No reuse before completion or unbounded pool. Measure allocation reduction and end-to-end cost; existing safe reuse is retained.

- [ ] **P59 — Reduce measured raster-resolution churn during navigation.**

    **Why:** repeated surface replacement near a threshold may cause allocation spikes and stalls. **Scope:** one observed zoom/resize boundary; compare existing bucketing with bounded hysteresis or compatible resource reuse. **Verify:** text/client sharpness, output density, overview demand, budget limits and memory after navigation settles. Keep exact-pixel consumers exact. Do not obtain better performance by silently lowering normal resolution or retaining every encountered size.

- [ ] **P57 — Test direct scanout for one eligible fullscreen SDR client.**

    **Why:** it can remove composition when that pass is actually a measured cost. **Scope:** explicit eligibility for geometry, format, synchronization, color, overlays and capture policy, with the current composition fallback. **Verify:** entry/exit, overlay/lock/capture appearance, replacement, buffer release and the other monitor. No feature disabling to manufacture eligibility. This is conditional and does not explain or solve slow native typing by itself; new HDR/VRR/provider matrices are outside this plan.
