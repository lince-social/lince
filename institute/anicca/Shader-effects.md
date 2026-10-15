# Remaining shader work

## Deferred: physical projection and gesture control

This milestone is explicitly deferred by the user. Do not start it until the user reopens it and approves the refined implementation plan. Display calibration, projection output, camera tracking and gesture manipulation remain unimplemented. The proposed calibrated secondary-window output and camera-independent gesture input path are also deferred; do not build them as substitutes.

When reopened, refine and implement in this order:

1. Choose the actual projector/display and camera, supported platform, room scale and tracking coordinate system. Agree on calibration accuracy, latency and privacy behavior before implementation.
2. Build the approved projection/output path using persisted room-local transforms. Calibrate scale, orientation and origin against physical space, with visible recovery and Reset controls. Discuss whether a secondary window is useful at that point.
3. Add approved camera/tracking input with deliberate selection, confidence thresholds and cancellation on tracking loss. Decide whether a camera-independent input path is needed.
4. Route accepted manipulation through existing authorized canvas receipts and shared undo so virtual and projected views share placement changes.
5. Verify calibration recovery, selection, cancellation, undo, tracking loss and device disconnects on the chosen hardware. Measure alignment and latency against the agreed criteria before marking this milestone complete.

## Known limits for future refinement

These are limitations or unverified cases, not approved implementation tasks:

- Real-room Gaussian scan quality and performance remain unverified; the current fixture is small and synthetic.
- Gaussian scans do not supply reliable furniture occlusion geometry; explicit opaque proxies are needed.
- Exact interleaving of arbitrary splats and transparent meshes remains unproven.
- Upright wall content appears edge-on in the room's top-down view. An unfolded wall-board view is not implemented.
- Smooth frame-rate performance is not established. The combined room debug replay measured frame medians of 224–270 ms on shared Intel Iris Xe/Vulkan hardware; a controlled performance study would be needed before setting targets.
