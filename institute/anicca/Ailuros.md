# Ailuros: testing and fixes

The feature coding and the focused tests have been written. The implementation has not yet been verified by a successful compile or test run. The remaining work is to run the existing tests, perform the hardware checks, and make small fixes for any failures found.

Do the following in order when the user authorizes execution. The user is required for the hands-on test with two monitors and a camera, and is willing to participate. Coordinate that step with the user. Projector testing remains separate until the user has access to a projector and authorizes it.

1. [ ] Obtain the face and hand models with `cargo xtask hand-models`, then run the appropriate `cargo check` commands with warnings treated as errors.
2. [ ] Run the already-written focused tests for Park workspace assignment and reconnection, camera controls and lifecycle, hand detection and movement, display selection, workspace isolation, and pointer manipulation. Apply small fixes for failures and rerun the affected checks.
3. [ ] With the user present, connect two monitors as extended displays and use a camera. Verify independent workspaces, camera preview and analysis controls, and that hand movement controls the selected display's workspace. Check up, down, left, right, neutral and tracking loss; with two open palms, the palm on the right side of the unmirrored camera image takes priority. Check pointer selection, movement, release and cancellation, as well as keyboard and mouse recovery. Measure responsiveness and accidental actions, record results, and make small fixes as needed.
4. [ ] When the user has a projector and authorizes its use, connect it as the second extended display and repeat the workspace and hand-control checks. Record the equipment, display settings, lighting, responsiveness and failures; make small fixes as needed.

The existing ignored `downloaded_models_run` integration test requires the downloaded models. For real-image coverage, set `LINCE_FACE_TEST_IMAGE` to a frontal face image and `LINCE_TWO_PALMS_TEST_IMAGE` to an image with two open palms facing the camera; it also accepts `LINCE_HAND_TEST_IMAGE` and `LINCE_HAND_CLOSED_TEST_IMAGE` for open- and closed-hand images. These inputs are optional, so a run without them does not verify real-image detection.
