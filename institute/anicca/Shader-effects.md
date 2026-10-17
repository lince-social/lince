# Shader effects and the physical workstation: remaining work

## Intended experience

Lince is the workstation for the computer and the room around it. Records supply task state and quantities to WGSL effects; Sands provide useful surfaces; a digital Kanban can also appear on a real wall. A desk can show a task, timer or answer beside an object. An explicitly requested camera frame can enter a Fiote conversation, and several independent requests can progress while immediate pointer feedback remains responsive.

Projection is onto surfaces. Lince can render a real digital 3D sphere, but projecting its image onto a desk does not create an object floating in the air. The ordinary computer interface remains available for editing, recovery and use with hardware disconnected.

[The Lightbulb Computer](https://lightbulbcomputer.com/) is inspiration for this connection between objects, useful projected information and interaction. The author describes real, bulky prototypes; the compact bulb is speculative. The written article and embedded demo descriptions were reviewed, not the separate video. Start with a fixed projector and camera; a small enclosure and automatic aiming are later decisions.

## Software boundary already provided

Completed implementation steps are removed from the remaining sequence below. Keep this contract as the starting point for the future hardware work:

- `crates/nucleus/src/physical_workspace.rs` defines devices, measured planar surfaces, calibration, timestamped observations, deliberate interactions and bounded request state. Its replay module accepts synthetic/prerecorded typed events without requiring hardware.
- `crates/interface/src/native/physical_workspace/` provides native controls, calibration patterns, dedicated projectively warped output windows, explicitly assigned Sand/Record/Kanban content, live supported shader images, pointer feedback, canvas receipts and Undo. Kanban drops request the existing authorized task transition rather than claiming an asynchronous write has already completed.
- `crates/cell/src/fiote/physical_workspace.rs` routes requests into existing Fiote conversations. Text-only and explicitly attached-frame requests are separate actions. Answers remain in those conversations and appear beside the assigned current Record. Stale answers cannot silently overwrite a changed Record. This does not implement automatic acceptance of arbitrary model-generated Record edits.
- One-shot native camera capture and a paired local hardware-adapter protocol supply observations. Camera detections are evidence, not commands. A surface assignment and selected Record provide explicit context; automatic object recognition and identity across views remain later work.

Camera/projector pixels, normalized surface coordinates, physical meters and the virtual canvas have separate meanings. A surface has a measured position/orientation and dimensions in meters; its camera and projector quadrilaterals map normalized coordinates, ordered top-left, top-right, bottom-right, bottom-left. Calibration revisions invalidate dependent input. A bound virtual Room uses local coordinates, so moving or rotating that Room on the canvas does not move its physical installation.

WGSL only determines appearance. It does not gain camera, networking, device, Record-write or Fiote authority. The existing shader editor, last-working effect, Reset/Disable, readable text, bounded glow and effect budgets continue to apply. Projection reuses those effect images. A projected effect can stay active when its computer placement is distant; explicit freezing and disabling still apply.

Limits are deliberate: 16 devices and 32 surfaces per configuration, 64 assignments per workspace, four active output windows, requested window resolutions sharing a 16-megapixel budget, and offscreen surface images capped at 1024 on each axis. Requests are bounded at 32 per workspace, four running globally; conflicting targets serialize. Observations expire after five seconds; pointers expire after 500 ms; requests time out after 120 seconds. Two ephemeral camera frames are retained per workspace session. These are software limits, not measured performance promises.

Saved workspace snapshots retain setup, calibration and assignments. They do not restore device sessions, camera consent, pairing credentials, pending requests or images. Capture never starts automatically on restore. Images explicitly sent to Fiote are saved in that conversation and may go to the configured model provider.

## Try the software before choosing hardware

1. In the local workspace, enable Edit mode and open **Edit store → Physical workspace**. **Load replay setup** supplies a starting configuration and synthetic device sessions; it does not open a camera.
2. Select the placements to share, choose the surface ID and use **Assign selected placements**, then **Open surface output**. Only assigned content appears. Move this dedicated window to the intended display when equipment is available; this is an ordinary OS display window, not a projector driver or automatic monitor discovery.
3. Use **Show calibration pattern** and the corner/dimension fields. **Apply surface calibration** increments the revision. **Reset surface calibration** returns to rectangular normalized coordinates; it does not recover a measured physical alignment. Reload setup fields after switching the selected surface. Advanced JSON configures additional devices and surfaces.
4. **Blank output** retains a black output window. **Close output windows** releases the windows and can reveal the desktop behind them. Capture blanks assigned output first; actual projector blank-to-exposure timing still needs measurement on the equipment.
5. For native one-frame input, set the local camera index and its actual resolution in the configuration, then use **Blank output and capture one frame**. **Stop all capture** revokes input and requests cancellation. A blocking device driver may take time to return; the inspector reports pending capture rather than claiming the hardware has closed.
Canvas Undo protects the complete later canvas state. Later edits or a still-running physics simulation can invalidate a movement receipt; it must not silently overwrite them. Pause workspace physics or pin the relevant layout for a stationary interaction experiment. The rendered replay explicitly pauses physics.

6. For a request, select an assigned local Record, include its attached Threads in the Protein fields, and enter an existing thread UID. Use **Request Fiote answer**, or explicitly **Request Fiote with latest frame** before that frame expires. The existing Fiote service must be configured. Individual requests can be canceled or dismissed. Questions requiring an answer are handled in the normal conversation.

The Rust example `crates/sandbox/examples/physical_workspace_smoke.rs` renders calibrated output, changes quantity-driven WGSL, moves a placement through an interaction receipt, uses Undo, checks a black frame, and closes the output. It runs with `nix develop .#interface --command cargo run -p lince-sandbox --example physical_workspace_smoke -- /tmp/lince-physical-workspace`.

## Future driver connection

Use **Enable and copy adapter pairing** in the inspector, then paste its pairing descriptor to a private local JSON file. The descriptor identifies an ephemeral loopback address/token, camera device/session, selected surface, calibration revision and current clock. It is not persisted or projected. Disable the adapter to revoke that camera session. Workspace/authority/session changes also invalidate it.

Protocol version 1 uses a four-byte big-endian byte length followed by UTF-8 JSON. Packets contain `version`, `token`, increasing `sequence` and a typed `command`. Commands are `clock`, `observe`, `interact` and `frame`; replies contain the same sequence, `now_ms`, an optional error and an optional interaction outcome. Read the exact serde types in `crates/interface/src/native/physical_workspace/adapter.rs` rather than inventing extra fields. Use Clock to synchronize capture timestamps; evidence sequences must also increase across reconnections within the same camera session.

Interactions carry begin/update/end/cancel phases and normalized camera coordinates. A tracking driver must emit an intentional Begin and End; every video frame must not become a task action. The server maps through current calibration, checks confidence/session/time/scope and uses existing operations. A task-transition reply means **requested**; a placement reply includes its receipt. The socket cannot request arbitrary Record writes or launch Fiotes.

There are at most two local endpoints, bounded queues and timeouts, a 1 MiB packet cap and a 512 KiB JPEG wire cap. Frames are decoded/prepared away from rendering and checked against the pixel budget. This is an authenticated local process boundary, not a LAN protocol. A future network transport needs its own trust and clock design.

`crates/sandbox/examples/physical_adapter_client.rs` implements the other side in Rust. After pairing, run `nix develop .#interface --command cargo run -p lince-sandbox --example physical_adapter_client -- /private/pairing.json clock`. Replace `clock` with `observe`, `drag`, or `frame desk /path/frame.jpg`; omit `desk` for non-frame modes to use the descriptor's selected surface. Drag sends synthetic deliberate input for testing; it does not detect hands. A future camera/vision process can replace that synthetic input while keeping the same connection contract.

## Remaining sequence: refine with the actual equipment

### 1. Bring up the first Ailuros camera/projector workstation

After initial learning and bench preparation, bring up one fixed camera and one projector on a stationary desk or wall. Choose mounting, working distance, coverage, ambient lighting, device interfaces, operating system, power and thermal arrangements before specifying drivers or an enclosure. Establish a way to stop capture and blank output from the ordinary workstation.

The **Interfaceless - Ailuros** Record (`@ailuros`) in [Lince.lingua](Lince.lingua) now contains the ordered camera/projector tasks under **First practical milestone: cameras and surface projection**. That Record is the owner-facing task plan; this Markdown supplies software instructions and verification details. Ailuros owns physical bring-up; Lince supplies the data, surface UI and software connection above.

Acceptance: the real devices enumerate and work through an appropriate local driver; the dedicated output is on the intended display, without unrelated desktop content. Record exact equipment, resolution, platform and setup.

### 2. Measure and commission one calibrated surface

Measure the surface dimensions/pose and calibrate camera-to-surface and surface-to-projector corners. Check interior points as well as corners. Moving equipment requires recalibration; moving the virtual Room must leave the physical mapping unchanged. Use the pattern and face-on output preview to inspect upright walls without rotating their real pose.

Choose numerical alignment and latency targets from measurements. Validate focus, ambient contrast, font size, color and glow. Projected darkness cannot make a bright real floor black; use a bright outline or other legible cleaning indicator. Calibrate blank-to-exposure timing or a projected-region mask so the camera does not mistake projected text for independent physical evidence. The current blank/capture actions do not prove that optical separation.

Acceptance: saved alignment survives a software restart with devices still requiring deliberate reconnection; measured corner/interior error is recorded; a real task and its shader update in Lince and on the surface.

### 3. Choose and validate physical pointing and object association

Choose a method suited to the camera, optics and surface: a known marker, pointing device, touch sensor or deliberate hand gesture. Implement its detector/driver as a producer of the existing observation/interaction contract. Existing hand-model facilities may be reusable, but an open-hand position alone is not reliable touch or permission to commit.

Begin with explicit selection and a clear release/commit action. Validate loss of tracking, beam occlusion, low confidence, reconnect, target deletion and recalibration. Pending interactions must cancel. Test a real Kanban transition and normal placement Undo. Keep keyboard/mouse recovery.

Associate a physical object with a Record deliberately and provide correction/removal before adding automatic recognition, OCR, voice or persistent cross-camera identity. A model thinking the room looks clean must not automatically complete its task.

Acceptance: measured targeting/commit accuracy and capture-to-feedback latency meet the agreed targets; accidental/uncertain input does not mutate Records; source observations and accepted results can be traced.

### 4. Commission concurrent Fiotes and multiple devices

Exercise two useful independent Fiote requests with real chosen model providers and explicit frame scope. Confirm answers reach the intended conversations/surfaces, individual cancellation works, waiting questions remain usable, and rendering/input retain capacity. No model work should run for every camera frame. Measure model delay separately from pointer delay.

Then add cameras/projectors one at a time. Maintain per-device calibration/health, clock alignment and explicit surface ownership. Initially use one projector per surface/region. Refine blending, brightness/color matching, overlapping coverage, automatic handoff or network transport only where the installation requires them. Test partial failure without corrupting unrelated surfaces or duplicating actions. The software supports multiple devices; their physical synchronization is not yet established.

Acceptance: a real combined workload has no crossed destinations, duplicate actions or unbounded backlog. Record frame-time percentiles, capture-to-feedback and Record-to-projection latency, inference latency, queue length and CPU/GPU/memory use in an appropriate optimized run.

### 5. Validate optional room-wide visuals separately

Keep usable surface work independent of a Gaussian room scan. Real scan quality/performance and arbitrary transparent-mesh/splat ordering still require visual evidence; use measured opaque surfaces/proxies where reliable occlusion matters. Room-wide cleaning/weather effects, reconstruction and sophisticated transparency should be separately refined around an actual scene.

The earlier combined virtual-room debug replay measured 224–270 ms median frame times on shared Intel Iris Xe/Vulkan hardware. Do not turn that result, or a functional software replay, into a claim of a smooth physical workstation. Profile the chosen combined setup and tune effects, resolution, capture cadence and inference concurrency together.

Free-floating holography, mobile aiming and a compact lightbulb enclosure remain deferred research/hardware work. They do not block the fixed surface workstation.

## Verification record

Verified on 2026-10-15 in the native Nix environment, with Rust warnings denied:

- `cargo check` passed for the native interface and both Rust examples; the final example check includes the rendering fixes.
- 11 core tests passed, including deterministic replay, geometry, scoped input, evidence identity/expiry, independent devices and request scheduling.
- 2 existing-Fiote integration tests passed, including an explicitly attached image, exact reply routing, waiting state and independent cancellation.
- 14 native tests passed, including a real loopback socket/JPEG exchange, pairing rejection, task scoping, room-local movement, normal Undo, native controls, valid output UI roots, blacking and asset release.
- Storage (5), recovery (8) and shader-placement (3) targeted tests passed. Some overlap the native filter above.
- The actual rendered replay passed after fixing the output hierarchy and using Lince's font. Its four PNG captures were inspected: glowing quantity −3, calm quantity 0, moved placement and a completely black output. Quantity changed 1,519 pixels and movement changed 168,601 pixels; these are functional checks, not performance measurements. Undo succeeded with the stationary fixture, then output closed and resources were released.

Replay captures and setup are in `/tmp/lince-shader-artifacts/physical-workspace-final/`; `physical-surface-0.png` through `physical-surface-3.png` show those four states. Current verification logs are `/tmp/lince-physical-core-final.log`, `/tmp/lince-physical-final-checks.log`, `/tmp/lince-physical-final-native.log`, `/tmp/lince-physical-final-examples.log` and `/tmp/lince-physical-live-5.log`. Temporary artifacts are local evidence, not checked-in assets. The earlier failed attempts remain in separate logs; the final replay and checks passed.

No physical projector/camera commissioning, gesture detector accuracy or optimized combined-workload performance has been claimed.
