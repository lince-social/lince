Do these tasks in order. Do not run tests until the user authorizes them. Use two monitors for the first hardware workflow; defer projector tests until the user has access to a projector and authorizes them.

1. Two workspaces in Park
   - [x] Inspect existing output and workspace assignment. Park already assigns workspaces to outputs and creates workspaces for connected displays.
   - [ ] Code any missing support for independent views, usable assignment and default-monitor selection, and disconnect/reconnect recovery.
   - [ ] Code focused tests for output assignment, independent views and disconnect/reconnect.
   - [ ] Execute those tests and verify the workflow with two monitors.

2. Camera feed and hand-model mode
   - [x] Code camera index selection, separate camera and hand-analysis controls, feed preview and status; reuse the existing hand models.
   - [x] Code a test for independent camera selection and hand-analysis settings.
   - [ ] Code further lifecycle tests covering live capture, preview, errors, stopping and tracking loss.
   - [ ] Execute the tests and verify the feed and controls with a camera.

3. Head-relative hand control on the default monitor
   - [ ] Code head detection and detection of up to two open palms facing the camera. When both are visible, prioritize the palm on the right side of the unmirrored camera image shown in the preview.
   - [ ] Code head-relative up, down and sideways canvas movement with a neutral region, smoothing and bounded speed; stop on uncertain or stale input. Route it to the default monitor's workspace while preserving keyboard and mouse control and the other workspace.
   - [ ] Code focused tests for directional movement, two-palm priority, neutral position, tracking loss, mode changes and workspace isolation.
   - [ ] Execute the tests and test the complete setup with two monitors and a camera. Measure responsiveness and accidental actions; fix failures and record results.

4. Projector validation
   - [ ] After the user authorizes projector testing, connect the projector as the second extended display and repeat the workspace and hand-control checks. Record the equipment, display settings, lighting, responsiveness and failures; fix issues found.

5. Optional item manipulation
   - [ ] Refine and code a visible pointer and deliberate gestures to select, move, release and cancel items if the user wants these beyond canvas movement. Keep keyboard and mouse recovery and cancel unfinished actions on tracking loss.
   - [ ] Code focused interaction tests.
   - [ ] Execute the tests and verify with available displays before any further projector test.
