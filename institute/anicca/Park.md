# Park: remaining work

## Candidate preparation

- [ ] **Niri P01 — Correct the output-state/rollback failure.** Establish the cause of the documented EINVAL/RecoveryFailure and fix the failing recovery path before disruptive output testing. **Why:** a display-setting change can currently terminate the desktop and its clients.
- [ ] **Niri P02 — Test automatic settings initialization.** When compilation is authorized, run the existing initialization regression covering private file creation and preservation of an existing configuration. **Why:** prevents first-login setup failures and accidental settings replacement.
- [ ] **Niri P03 — Test automatic terminal selection.** When compilation is authorized, run the existing terminal-selection regression for executable TERM values and Ghostty fallback. **Why:** terminal launching must work without a manually supplied binary path.
- [ ] **Niri P04 — Publish the corrected prebuilt candidate.** After P01/P02/P03, publish matching binary and flake artifacts containing the required source fixes. **Why:** local source corrections otherwise never reach the installed desktop.
- [ ] **Niri P05 — Install the corrected candidate.** At an agreed time, install the P04 prebuilt package through the personal NixOS flake. **Why:** physical acceptance must exercise the corrected release.
- [ ] **P06 — Verify the running candidate's identity.** After entering the corrected session, record its executable, revision, optimized mode and ui/instinct/park features against the published artifact. **Why:** avoids testing an older process after an upgrade.
- [ ] **Niri P07 — Verify installed content loading.** Exercise native Lince content outside the development shell, with content/lince as the LINCE_CONTENT path when configured. **Why:** catches packaged-content path failures hidden by the source checkout.

## External commands and input setup

- [ ] **Niri P08 — Restore the intended keyboard configuration.** Merge br layout, repeat rate 35 and repeat delay 300 ms into existing settings. **Why:** physical typing must use the user's actual layout.
- [ ] **Niri P09 — Configure the external launcher command.** Connect the selected provider to Park's launcher action. **Why:** Mod+D cannot launch applications without a provider.
- [ ] **Niri P10 — Configure the external locker command.** Connect the selected locker to Park's lock action. **Why:** authentication and locked recovery require a functioning locker.
- [ ] **Niri P11 — Wire an authorized display-power helper.** Use the inherited WAYLAND_SOCKET output-control grant without weakening public-client permissions. **Why:** the former power command failed before reaching Park.

## Interaction and rendering

- [ ] **P12 — Test keyboard-driven application launching.** After P09, use Mod+D to type a query and launch an application. **Why:** opening a launcher panel alone does not prove focus or dispatch works.
- [ ] **P13 — Test normal lock/unlock authentication.** After P10, use Mod+Escape and unlock with the user's normal authentication. **Why:** a visible lock screen alone does not establish a usable, authenticated unlock.
- [ ] **P14 — Verify client hardware rendering.** Inspect the renderer selected by an ordinary native Wayland client in the corrected session. **Why:** the earlier installed release selected software rendering despite working windows.
- [ ] **P15 — Test visible video playback.** Reuse the existing video check on the corrected candidate without competing compilation/GPU scenes. **Why:** hardware selection and decoded-frame counts alone do not prove the observed playback slowdown is fixed.
- [ ] **P16 — Test canvas animation responsiveness.** Reuse the existing animation check on the corrected candidate. **Why:** the earlier approximately 18-FPS animation slowdown is separate from video decoding.
- [ ] **P17 — Test physical keyboard composition.** After P08, type using the actual layout/dead keys or configured IME. **Why:** injected Unicode bypasses the input path used for real typing.
- [ ] **P18 — Save a native Lince edit.** Make and save an identifiable edit through the native interface. **Why:** establishes real user work to verify after logout.
- [ ] **P19 — Test the outstanding portal consent interaction.** Use the selected portal's actual consent UI for the pending request and confirm the requested result. **Why:** protocol automation alone does not establish that the user can complete the permission flow.
- [ ] **P20 — Complete 15 minutes of ordinary work.** Use browser, terminal and Lince in the same acceptance session without crash, lost work, wrong-target input, private exposure or stuck input. **Why:** catches interactions between components that isolated checks miss; no extra endurance run is required.

## Outputs and recovery

- [ ] **P21 — Test resolution-preview rollback.** After P01/P06, preview a supported resolution change and let it revert. **Why:** a bad mode choice must not leave the desktop unusable.
- [ ] **P22 — Test scale-preview rollback.** After P01/P06, preview a supported scale change and let it revert. **Why:** scaling changes logical geometry and can break input mapping independently of resolution.
- [ ] **P23 — Test second-output reconnection.** Disconnect/reconnect HDMI and verify existing windows remain reachable with correct pointer mapping. **Why:** unplugging a display must not strand windows or misdirect input.
- [ ] **P24 — Test one VT round trip.** After P01/P06, discover and agree on current departure/return VTs, record the transition and require the same compositor/windows to survive with resumed input and no replayed held keys. **Why:** the previous installed release exited during this operation.
- [ ] **P25 — Test ordinary display-power recovery.** After P11, turn displays off/on and verify current-epoch power confirmation within two seconds. **Why:** output disablement or a successful helper exit does not prove power recovery.
- [ ] **P26 — Test hotplug while locked.** After P13, reconnect the second output while locked and check for private-content exposure. **Why:** new outputs must not bypass lock privacy.
- [ ] **P27 — Test one locked suspend/wake.** After P11/P13, suspend while locked, wake and verify privacy, normal authentication and recovered output/input. **Why:** device reacquisition after sleep can fail even when ordinary unlock works.

## Logout and persistence — run last

- [ ] **P28 — Test save-aware logout.** At an agreed time, exercise Park's save-aware exit and return to Niri. **Why:** leaving the desktop must protect unsaved user work.
- [ ] **Niri P29 — Verify session cleanup.** After P28, inspect Park services and display-environment cleanup. **Why:** stale session state can break applications in the next desktop.
- [ ] **P30 — Verify saved settings after relogin.** Log into Park again and check the saved configuration. **Why:** settings that work only until logout are not persistent.
- [ ] **P31 — Verify the saved edit after relogin.** Reopen the edit saved in P18. **Why:** confirms native user data survives the complete session restart.
