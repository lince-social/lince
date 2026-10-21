# ESP
- [ ] Run the added Video Castle persistence tests and repeat visible playback with a fresh Actions desktop build.
- [ ] Finish the Lince restart test after resolving the local database/disk-full error; confirm the existing Signal resumes without provisioning.
- [ ] Verify reconnection after changing the PC’s Wi-Fi, without reconfiguring the ESP.
- [ ] Select the fresh package for normal startup when requested by the user; leave the service unchanged during these tests.
- [ ] Last, test charger-only cold startup and sustained wall-powered streaming: FPS, latency and stability over direct and relay connections.

Notebook validation on 2026-10-20: the prepared desktop build displayed the authenticated `esp-camera` feed in the real Cell's Video Castle over an iroh relay. A USB reset preserved provisioning, rejoined Wi-Fi and recovered visible playback. Relay interruptions were observed, so sustained stability is not passed. The local camera Castle now has explicit size overrides so it remains readable after restart. The test app was closed; the service was not changed. The database passed a read-only `quick_check` after closing the app.

Implemented: shared canvas registration, capture and restoration of Video Castle settings, including Signal source, playback state and file position. Three new regression tests cover canvas reload, invalid state and restored controls. Run `cargo test -p lince-interface --features native-presentation --lib video_castle` with warnings denied in Actions before marking the fresh build verified.

Local checks: `git diff --check` passed for these changes. The warning-denied native `cargo check --tests` was interrupted by SIGTERM; its retry encountered another build's lock and was stopped. No passing native-check result is claimed. The existing host provisioning test executable passed 24 tests; compiling a fresh host test executable hit a no-space error. No cleanup or service changes were made.
