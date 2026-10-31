# Park: remaining desktop validation

Updated 2026-10-30. B1–B2 share one scheduled physical/login session. No new subjects, provider matrices or stress suites. Native Wayland only. Accessibility, XWayland and the separate workspace refactor are outside this plan. Login, bars, launchers, lockers and portals remain external programs.

## B1 — Physical outputs and recovery (needs the user)

- [ ] **B1.1 — Validate monitor configuration and rollback.** In the scheduled physical session, verify output and pointer mapping on both monitors. Change position and a supported resolution/scale, let preview rollback restore usability, then keep a valid arrangement. Fix any incorrect mapping or inaccessible configuration.
- [ ] **B1.2 — Validate hotplug and VT recovery.** Disconnect/reconnect one output, then leave and return through a VT. Require reachable windows and resumed input without replayed held state. Keep Niri available as the return session.
- [ ] **B1.3 — Validate power and locked recovery.** Check ordinary display-power recovery and one suspend/wake with the chosen external locker. Require current-epoch power confirmation within the existing two-second bound and no private exposure during locked wake/hotplug. Reuse established locker-loss protocol evidence instead of destructive crash experiments.

## B2 — Installed session and ordinary use (needs the user)

- [ ] **B2.1 — Verify the newly published candidate.** Update the personal flake's Lince release input after the intended main publication, then check the exact binary revision/features, packaged session and module evaluation outside the development shell. Validate the newer candidate containing the UWSM integration. Use the prebuilt package, not a local Rust rebuild.
- [ ] **B2.2 — Validate UWSM login and session startup.** After the user authorizes host activation, select Park in the existing greeter. Require readiness after the first interactive frame, correct display-environment ownership and session-scoped external autostart. Verify the configured external bar, launcher and locker work in the installed session, with one bar instance and no dependence on Niri's startup commands.
- [ ] **B2.3 — Complete the ordinary-use sample.** In the same session as B1, spend 15 minutes on browser/terminal/Lince work, including the user's layout/dead keys or configured IME, a saved edit and outstanding launcher/portal consent UI. Require no crash, lost work, wrong-target input, private leak or stuck input. Fix demonstrated failures without adding endurance or provider matrices.
- [ ] **B2.4 — Validate logout and saved restart.** Exercise save-aware logout, return to Niri and log into Park again. Require saved settings to return and Park services/display environment to be cleaned up on exit. Keep the previous NixOS generation available.

B1 and B2 share one scheduled session with the user's participation. Do not log out, switch VT or activate NixOS before that session is arranged. SDR keyboard/mouse operation is the target; HDR, VRR, tablet/touch and connector/rotation matrices are not additional gates.

## Execution rules

Use the build defaults in [Build-iteration.md](Build-iteration.md). Build each artifact once, freeze it and run all pending independent observations in one orchestration pass. A failure must not suppress independent results. Reuse compositor/browser/provider lifetimes where reset is reliable; after a destructive failure, record it and start a fresh private session for the remaining checks. Give GPU/session fixtures one execution slot. CPU-only cases may overlap; performance measurements may not compete with builds or other test scenes.

Use the existing fixture/observation ledgers. Record selector/action, result, artifact identity, relevant inputs, duration and log path; flush each result immediately. An exact selector must execute one case: zero selected cases is not a pass. Preserve valid passes with their original provenance. After a change, rerun failures and affected observations, not the former whole suite. Never silently transfer a pass to another binary. Compositor/renderer/input changes can affect several workflows; unrelated documentation or provider configuration must not invalidate unrelated logic. Where impact is uncertain, rerun the affected workflow.

The runner and ledgers are implemented. Use:

    nix develop .#interface --option builders ''
    cargo xtask park validate final init /absolute/settings.json
    cargo xtask park validate final run /absolute/run --only B2

Select B2 explicitly for installed-package preparation; the default run selects A1–A3 and never starts a physical session. Initialize a fresh candidate after compiled-source changes; preserve old reports as historical evidence. Confirm the final installed release with the existing A2 smoke and checks affected by final production fixes, without a sixth workflow.

Delete each completed task only after its required fixes and execution pass, retaining the receipt in the run report. Do not restore completed subjects merely because another subject failed. Do not add duplicate size/orientation/provider sweeps, twenty-cycle stress, six pacing intervals or long resource soaks.

## Installed session configuration

The personal configuration already imports inputs.lince.nixosModules.park and enables programs.lince-park, using the published main-binary flake with its default prebuilt package. Keep that route. The lin shell command updates the Lince input and applies the NixOS configuration. No nixpkgs registration or source-package override is needed.

The October 23 published module predates UWSM; the current repository module defaults withUWSM to true. Verify the new release contains that integration in B2. Keep the existing greeter, Niri and its default-session setting until the user chooses otherwise. A configuration rebuild is separate from validating login; no reboot is inherently required for adding the session.
