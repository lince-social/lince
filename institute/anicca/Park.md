# Park: remaining work

## Interaction and rendering

- [ ] **Niri P15 — Correct and retest dropped video frames.** Compare the optimized candidate with the existing 30-second fixture: old Park dropped 22.4 frames/s versus 0.5 directly in Niri. **Why:** both desktops report approximately 25 video callbacks/s, so callback count alone cannot establish playback quality.

    Implemented damage-only shared-memory uploads; GPU readback passed for partial damage, retained frames and resizing. Measured the old Park and direct-Niri baselines; corrected physical Park measured 41.5 dropped video frames/s over 30 seconds at 1920×1080, versus 0.5 in the direct-Niri reference. The performance check failed. User reports severe Firefox freezes and slideshow-like playback on the currently installed build. Maximum-throughput acceptance needs a dedicated, uncontended machine; local checks remain useful for regression diagnosis. A private optimized one-output run reproduced 21.4 dropped frames/s; client import took 5.7 ms median. Removed redundant DMA-BUF copies for empty/full damage and added opt-in allocation/copy/release timings; GPU readback passed for empty, partial, full and resized content while preserving older frames; the optimized comparison remains pending. The private run used 1920×995 browser content, so it is diagnosis, not a replacement acceptance result.

- [ ] **Niri P16 — Correct and retest canvas animation responsiveness.** Reuse the existing animation fixture on the optimized candidate without competing compilation/GPU scenes; old Park measured 37.6 FPS versus 59.9 directly in Niri. **Why:** confirms the animation slowdown is actually fixed.

    Removed redundant GPU allocations for imported client images and added finer frame timings; hardware composition regression passed. Corrected physical Park measured 9.4 animation FPS over 30 seconds at 1920×1080, versus 59.9 in the direct-Niri reference. The performance check failed. Private optimized tracing reproduced 38.8 FPS with one output: native scene update took 13.5 ms median, frame export 4.0 ms and final composition 0.75 ms. User-only CPU profiling found costs spread across ECS query setup, lookup, allocation and clearing, without establishing one dominant application function. Further diagnosis targets scene processing and buffer transfer; physical acceptance remains pending.

- [ ] **P20 — Complete 15 minutes of ordinary work.** Use browser, terminal and Lince in the same acceptance session without crash, lost work, wrong-target input, private exposure or stuck input. **Why:** catches interactions between components that isolated checks miss; no extra endurance run is required.

## Outputs and recovery

- [ ] **P21 — Test resolution-preview rollback.** Preview a supported resolution change and let it revert without EINVAL/RecoveryFailure or compositor exit. **Why:** a bad mode choice must not leave the desktop unusable.

    Preserved working display modifiers and added bounded fallback for rejected allocations; display regressions and 20 real-kernel blob lifetime cycles passed under Niri. The physical attempt exited to SDDM while scrolling the mode list, before selecting a resolution. Mode-change rollback remains untested until X05 is fixed.

- [ ] **P22 — Test scale-preview rollback.** Preview a supported scale change and let it revert. **Why:** scaling changes logical geometry and can break input mapping independently of resolution.

    Passed the hardware scaled-output regression for pixels, client geometry and logical input mapping; physical preview/revert remains untested.

- [ ] **P23 — Test second-output reconnection.** Disconnect/reconnect HDMI and verify existing windows remain reachable with correct pointer mapping. **Why:** unplugging a display must not strand windows or misdirect input.

- [ ] **P24 — Test one VT round trip.** Discover and agree on current departure/return VTs, record the transition and require the same compositor/windows to survive with resumed input and no replayed held keys. **Why:** the previous installed release exited during this operation.

- [ ] **P25 — Test ordinary display-power recovery.** After the display-power helper is installed, turn displays off/on and verify current-epoch power confirmation within two seconds. **Why:** output disablement or a successful helper exit does not prove power recovery.

    Implemented the power helper and display actions; automated delayed off/on confirmations, unauthorized-client rejection and the two-second timeout passed. Actual display power recovery remains untested.

- [ ] **P26 — Test hotplug while locked.** With ordinary authentication verified, reconnect the second output while locked and check for private-content exposure. **Why:** new outputs must not bypass lock privacy.

- [ ] **P27 — Test one locked suspend/wake.** After the display-power helper is installed and with ordinary authentication verified, suspend while locked, wake and verify privacy, normal authentication and recovered output/input. **Why:** device reacquisition after sleep can fail even when ordinary unlock works.

## Logout and persistence — run last

- [ ] **P28 — Test save-aware logout.** At an agreed time, exercise Park's save-aware exit and return to Niri. **Why:** leaving the desktop must protect unsaved user work.

    Passed save-aware exit in the private nested browser run. User confirmed physical logout reached SDDM and relogin worked; logout used Noctalia with Ghostty tabs open and no prompt. Protection of an identifiable unsaved native edit remains untested.

- [ ] **Niri P29 — Verify session cleanup.** After P28, inspect Park services and display-environment cleanup. **Why:** stale session state can break applications in the next desktop.

    Verified the nested run leaves Niri’s user-manager display environment unchanged; cleanup after an actual Park login/logout remains untested.

- [ ] **P30 — Verify Park settings after relogin.** Log into Park again and check that its saved keyboard settings and external-command bindings are restored from `park.json`. **Why:** desktop input and launch commands must survive a session restart.

## Extra work encountered

- [ ] **Niri X02 — Fix renderer allocation errors on the corrected build.** Diagnose and eliminate Bevy slab allocator errors during physical Park startup and application opening. **Why:** invalid rendering-resource references can leave native content missing or unstable.

    Reopened: optimized revision 046d54b4391e02ca194b9968cff2264eee7b98e4 passed the isolated nested startup/terminal check, but physical session PID 19886 logged five “Use-after-free: attempted to copy element data for an unallocated key” errors after relogin. Saved corrected-session-startup.log; the isolated pass did not cover this physical behavior. Added a shared upload guard for empty clipped meshes: retire their prepared GPU assets and buffers while preserving their main-world identity for later visible updates. GPU regressions passed for repeated empty/visible transitions and workspace clipping pixels/input. Physical startup and application-opening logs still need checking on the new binary; empty geometry has not yet been proven to explain every reported error.

- [ ] **X03 — Test corrected Noctalia lock rendering.** After installing the corrected build, confirm both lock screens and the authentication bar remain stable. **Why:** authentication controls must remain stable and usable.

    Observed by the user during the physical lock test on the installed baseline; password entry did not unlock. Corrected the launcher/PAM startup defect and the compositor discarding ready lock images when rendering the other output. The GPU regression reproduced the disappearance before the fix and passes afterward; physical retest on corrected revision confirms authentication works and controls stay visible, but the screen flickers about every 0.5 seconds. Investigate replacement of pending lock-buffer imports; a new regression reproduced black frames while a replacement lock buffer was pending and passes with current-session image retention. New binary and physical flicker retest remain pending.

- [ ] **X05 — Verify scrolling remains usable under input bursts.** Retest the resolution list, Edit panel and ordinary native scroll controls after the shared input-queue fix. **Why:** scrolling currently exits Park to SDDM before display tests can run.

    Implemented bounded shared scroll batches and nonfatal overflow, with ordered focus/handoff replay. Automated burst, queue-full, recipient-retirement, IME and two-output checks passed. Fixed cross-workspace mesh picking, modal focus loss during handoff and stale requests to removed outputs. A new binary and physical scrolling retest remain pending.
