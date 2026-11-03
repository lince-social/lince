# Park: remaining work

## Quick checks first

- [ ] **P23 — Test monitor reconnection.** With a window maximized by double-clicking its title bar, unplug and reconnect HDMI twice. Pass if each workspace returns to its original screen, the window stays maximized, and windows and pointer work on both screens. **Why:** reconnecting must preserve the desktop arrangement and window state.

    The fix has passed automated tests; it needs a new optimized binary and this physical retest.

## Coordinated session checks

- [ ] **P24 — Fix and retest a VT round trip.** Switch away from Park’s VT and back. Pass if the same session and windows remain, input works, and no held key is replayed. **Why:** Park previously exited during this operation.

    VT 7 → VT 3 → VT 7 returned to login and the original Park process exited. Its log reported DRM mode-test EINVAL and a RecoveryFailure. Resume now clears conflicting DRM connector and plane state before rebuilding outputs; install a new optimized binary and repeat the physical test.

## Placement and capture fixes

- [ ] **Niri P15A — Test new-window placement.** On the corrected binary, open Firefox on each monitor. Pass if each new window appears in the intended visible viewport. **Why:** windows that open offscreen cannot be used or measured.

    A delayed-buffer placement fix passed its focused regression; it needs an optimized binary and live verification.

- [ ] **Niri X10 — Fix capture of model and shader content.** Run `imported_glb_and_generated_wgsl_meshes_follow_their_real_sources_in_capture_and_a_replacement_keeps_its_public_pixels`. Pass if both sources become ready and replacement pixels remain visible. **Why:** public model and shader content must appear in capture.

## Performance — after P15A passes

- [ ] **Niri P15 — Test video playback.** Run the existing 30-second fixture with Firefox visibly fullscreen at the same geometry as the direct-Niri run. Pass if dropped frames are close to the Niri reference of 0.5 per second. **Why:** video callback counts alone do not show dropped frames.

    The last private Park run dropped 22.1 frames per second. Maximum-throughput acceptance needs an uncontended machine.

- [ ] **Niri P16 — Test canvas animation.** Run the existing visible animation fixture without competing compilation or GPU tests. Pass if animation is close to the direct-Niri reference of 59.9 FPS. **Why:** video playback alone does not show canvas responsiveness.

    The last private Park run measured 38.0 FPS. Maximum-throughput acceptance needs an uncontended machine.

