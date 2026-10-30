# Park: remaining desktop validation

Updated 2026-10-30. A2 contains automatic checks without leaving Niri; B1–B2 share one scheduled physical/login session. No new subjects, provider matrices or stress suites. Native Wayland only. Accessibility, XWayland and the separate workspace refactor are outside this plan. Login, bars, launchers, lockers and portals remain external programs.

## A2 — Applications and external tools (automatic)

- [ ] **A2.1 — Resolve file-drop completion.** Diagnose why the real drop fixture receives no completion after replacement hover. Check fixture dispatch against the production path before changing product code. Require exactly one completed intended drop and no delayed write to a cancelled or replacement owner.
- [ ] **A2.2 — Restore ordinary toolkit input.** Diagnose Qt's empty text field despite application focus; correct the general input path or demonstrated fixture error. Then finish the existing typing, popup/submenu, owned-dialog and move/resize workflow, with correct pixel bounds and recipients.
- [ ] **A2.3 — Resolve appearance pixel preservation.** Diagnose the flat/free client-pixel comparison failure. Preserve sharp application content and input geometry when effects are enabled; fix the renderer or an invalid measurement based on evidence, without simply relaxing the assertion.
- [ ] **A2.4 — Complete capture rejection.** Finish one representative denied/revoked capture path using existing automation. The previous broad sweep hit a six-minute diagnostic limit and is incomplete, not evidence of a leak. Require rejected requests to leave no unauthorized pixels or retained pending work; do not expand the permission matrix.
- [ ] **A2.5 — Verify the real browser/terminal workflow.** Reuse one private nested Park with the selected native Wayland browser and terminal for typing, extended popup pixels/input, fullscreen/restore, cross-application copy/paste and file drop. Record independent outcomes and recover the private session after a failure so remaining observations still run.
- [ ] **A2.6 — Verify external bar/provider handoffs.** On a private bus, run the user's selected bar/provider and verify reserved-space allocation/restoration, launching and authorized capture with usable pixels. Use standard integration points; do not build replacement services. Carry only unavoidable human-consent interaction to B2.

## B1 — Physical outputs and recovery (needs the user)

- [ ] **B1.1 — Validate monitor configuration and rollback.** In the scheduled physical session, verify output and pointer mapping on both monitors. Change position and a supported resolution/scale, let preview rollback restore usability, then keep a valid arrangement. Fix any incorrect mapping or inaccessible configuration.
- [ ] **B1.2 — Validate hotplug and VT recovery.** Disconnect/reconnect one output, then leave and return through a VT. Require reachable windows and resumed input without replayed held state. Keep Niri available as the return session.
- [ ] **B1.3 — Validate power and locked recovery.** Check ordinary display-power recovery and one suspend/wake with the chosen external locker. Require current-epoch power confirmation within the existing two-second bound and no private exposure during locked wake/hotplug. Reuse established locker-loss protocol evidence instead of destructive crash experiments.

## B2 — Installed session and ordinary use (needs the user)

- [ ] **B2.1 — Verify the newly published candidate.** Update the personal flake's Lince release input after the intended main publication, then check the exact binary revision/features, packaged session and module evaluation outside the development shell. Validate the newer candidate containing the UWSM integration. Use the prebuilt package, not a local Rust rebuild.
- [ ] **B2.2 — Validate UWSM login and session startup.** After the user authorizes host activation, select Park in the existing greeter. Require readiness after the first interactive frame, correct display-environment ownership and session-scoped external autostart. Arrange any needed bar/launcher startup through the user's chosen external mechanism; Niri's startup commands do not run in Park.
- [ ] **B2.3 — Complete the ordinary-use sample.** In the same session as B1, spend 15 minutes on browser/terminal/Lince work, including the user's layout/dead keys or configured IME, a saved edit and outstanding launcher/portal consent UI. Require no crash, lost work, wrong-target input, private leak or stuck input. Fix demonstrated failures without adding endurance or provider matrices.
- [ ] **B2.4 — Validate logout and saved restart.** Exercise save-aware logout, return to Niri and log into Park again. Require saved settings to return and Park services/display environment to be cleaned up on exit. Keep the previous NixOS generation available.

Run the remaining A2 tasks. Group A uses private directories, sockets, profiles and provider buses; never log out, switch VT, activate NixOS or change the host graphical-session environment there. Missing tools block only their dependent observations. B1 and B2 share one scheduled session. SDR keyboard/mouse operation is the target; HDR, VRR, tablet/touch and connector/rotation matrices are not additional gates.

## Execution rules

Use the build defaults in [Build-iteration.md](Build-iteration.md). Build each artifact once, freeze it and run all pending independent observations in one orchestration pass. A failure must not suppress independent results. Reuse compositor/browser/provider lifetimes where reset is reliable; after a destructive failure, record it and start a fresh private session for the remaining checks. Give GPU/session fixtures one execution slot. CPU-only cases may overlap; performance measurements may not compete with builds or other test scenes.

Use the existing fixture/observation ledgers. Record selector/action, result, artifact identity, relevant inputs, duration and log path; flush each result immediately. An exact selector must execute one case: zero selected cases is not a pass. Preserve valid passes with their original provenance. After a change, rerun failures and affected observations, not the former whole suite. Never silently transfer a pass to another binary. Compositor/renderer/input changes can affect several workflows; unrelated documentation or provider configuration must not invalidate unrelated logic. Where impact is uncertain, rerun the affected workflow.

The runner and ledgers are implemented. Use:

    nix develop .#interface --option builders ''
    cargo xtask park validate final init /absolute/settings.json
    cargo xtask park validate final run /absolute/run --only A2

Select A2 for its pending observations. The default run selects A1–A3 and never starts a physical session, so use an explicit pending selector. Initialize a fresh candidate after compiled-source changes; preserve old reports as historical evidence. B2 is explicitly selected for installed-package preparation. Confirm the final installed release with the existing A2 smoke and checks affected by final production fixes, without a sixth workflow.

Delete each completed task only after its required fixes and execution pass, retaining the receipt in the run report. Do not restore completed subjects merely because another subject failed. Do not add duplicate size/orientation/provider sweeps, twenty-cycle stress, six pacing intervals or long resource soaks. The unresolved rendering/input/persistence checks above address actual failures.

## Installed session configuration

The personal configuration already imports inputs.lince.nixosModules.park and enables programs.lince-park, using the published main-binary flake with its default prebuilt package. Keep that route. The lin shell command updates the Lince input and applies the NixOS configuration. No nixpkgs registration or source-package override is needed.

The October 23 published module predates UWSM; the current repository module defaults withUWSM to true. Verify the new release contains that integration in B2. Keep the existing greeter, Niri and its default-session setting until the user chooses otherwise. A configuration rebuild is separate from validating login; no reboot is inherently required for adding the session.
