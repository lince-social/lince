# Park: remaining desktop validation

Updated 2026-10-29. Keep **five workflows**: A1–A3 are automatic without leaving Niri; B1–B2 share one scheduled physical/login session. This file contains remaining work, not the completed feature catalog. No new subjects, provider matrices or stress suites. Native Wayland only. Accessibility, XWayland and the separate workspace refactor are outside this plan. Login, bars, launchers, lockers and portals remain external programs.

Completed observations have been removed from the pending paragraphs and retained with their original artifact identities and logs. They do not establish that the whole desktop passed. Evidence is under target/park-validation/final-suite/run-1793307617804755758-832833 and target/park-validation/automatic. [Build-iteration.md](Build-iteration.md) records the working defaults and measured costs.

The immediate batch is finished and testing has stopped at the user's request. The release application and the fixture with optimized shared dependencies compiled; the width workflow and four runner checks passed. Cold-start follow and Record picking failed again with specific diagnostics. The remaining paragraphs below await the next authorized pass; no physical session was started.

## Group A — automatic, keeping Niri running

- [ ] **A1 — Finish the failed controls/settings paths.** Fix saved native camera-follow restoration: the correct workspace, item and follow intent reload, but Bevy's default primary-window focus blocks restoration in the shared focus-memory logic. Treat that initial window focus appropriately while preserving protection against stealing a real editor's input. Fix the visible Record inspector receiving the underlying native surface's pointer before a click; align native surface picking with rendered stacking without a toolkit-specific exception. Rerun the corrected helpers for workspace Operation navigation/history, compact and scratch controls, and bounded/window-properties selection: their earlier failures involved scrolling, fractional-scale coordinates or incorrect Record ancestry. Complete opening-rule startup/save/reload, the eight-conflict narrow modal, and the real IME/pending-paste settings-draft path; obsolete setup interrupted their earlier runs. Complete the moving-item camera-follow regression after its coordinate correction, requiring camera and item to align within the same rendered frame. Reuse existing cases with representative sizes. These are the remaining navigation, editing and persistence checks needed to keep ordinary work reachable and saved.

- [ ] **A2 — Finish real application/input/provider validation.** Diagnose the file-drop fixture failing to receive completion after replacement hover; distinguish a fixture dispatch mistake from a product transfer failure. Resolve the Qt parent text field receiving no text despite application focus, then finish its existing popup/submenu, owned-dialog and move/resize path. Diagnose the client-appearance pixel-preservation failure in the flat/free scene. Complete one capture denial/revocation observation: the broad sweep hit the six-minute diagnostic limit and is incomplete, not evidence of a privacy leak; retain its log and use the representative path instead of expanding the matrix. Run one private nested Park with the selected native Wayland browser and terminal for typing, popup pixels/input, fullscreen/restore, cross-application copy/paste and a real file drop. On a private bus, run the selected external bar/provider to verify reservation/restoration, launching and authorized capture with usable pixels. Human consent that cannot be automated belongs in B2 and must not block independent protocol results. Require correct pixels and recipients, rejection of delayed input to cancelled/dead owners, restoration of space when the bar closes and rejection of unauthorized capture. This is the remaining evidence that normal apps and the user's external tools work together.

- [ ] **A3 — Measure the release candidate without competing work.** After ten seconds of warmup, measure one 60-second ordinary browser-video interval and one 60-second Showoff interval at controlled 60 Hz, with builds and other GPU fixtures stopped. Require ordinary video >=57 fresh compositor FPS, p95 frame interval <=25 ms and no repeated compositor-caused stalls >100 ms; require Showoff >=45 fresh FPS. Record decoded video progress separately from compositor presentations. Outside those quiet intervals, perform three open/resize/fullscreen/close/capture cycles, comparing settled RSS, FDs and pending requests. Flag strictly increasing samples exceeding four FDs or 32 MiB beyond the first measured round; capture fixtures must retire pending requests and GPU resources. After three seconds to settle, measure ten static seconds and require <=5 nested submissions/s. These are bounded jank/repaint/resource checks, not a long-term leak guarantee. If Niri constrains nested pacing, mark that measurement inconclusive and resolve the same observation in B1 instead of weakening thresholds or rebuilding repeatedly. Release FPS/idle/resource acceptance has not run.

Do A1, then A2, then A3. Use private runtime/data directories, sockets, browser profiles and provider buses. Never log out, switch VT, activate NixOS or change the host graphical-session environment during Group A. A missing tool blocks only its dependent observations. Clean up only test-owned processes.

## Group B — one scheduled session with the user

- [ ] **B1 — Validate physical outputs and recovery.** With the user's two monitors and keyboard/mouse, confirm usable output and correct pointer mapping on both; change position and a supported resolution/scale, verify preview rollback, then keep a valid arrangement. Disconnect/reconnect one output, leave/return through a VT, and suspend/wake once with the selected external locker. Check one ordinary display-power recovery and one locked recovery, including the existing two-second bound for current-epoch power confirmation. Require reachable windows, resumed input without replayed held state, usable rollback and no private-content exposure on locked wake/hotplug. Reuse established locker-loss protocol evidence rather than adding destructive crash experiments. Resolve any A3 measurement explicitly left inconclusive by nested presentation. SDR keyboard/mouse operation is the target; HDR, VRR, touch/tablet and connector/rotation matrices are not additional gates. These facts require the hardware session.

- [ ] **B2 — Validate installed login, ordinary use and saved restart.** Evaluate the exported NixOS module and package/session entry, and prepare the exact source candidate outside the checkout/development shell. After the user approves host activation, log in through the existing greeter with external UWSM; verify readiness after the first interactive frame, environment ownership and scoped autostart. In the same session as B1, spend 15 minutes doing normal browser/terminal/Lince work, including the user's layout/dead keys or configured IME, a saved edit and any launcher/portal consent UI A2 could not automate. Exercise save-aware logout, return to Niri and log into Park again. Require no crash, lost work, wrong-target input, privacy leak or stuck input; require cleanup of Park services/display environment and restoration of saved settings. Keep Niri and the previous NixOS generation available. This is the remaining delivery/login evidence, not a 90-minute endurance claim.

## Execution rules

Build each artifact once, freeze it and run all pending independent observations in one orchestration pass. A failure must not suppress independent results. Reuse compositor/browser/provider lifetimes where reset is reliable; after a destructive failure, record it and start a fresh private session for the remaining checks. Give GPU/session fixtures one execution slot. CPU-only cases may overlap; performance measurements may not compete with builds or other test scenes.

Use the existing fixture/observation ledgers. Record selector/action, result, artifact identity, relevant inputs, duration and log path; flush each result immediately. An exact selector must execute one case: zero selected cases is not a pass. Preserve valid passes with their original provenance. After a change, rerun failures and affected observations, not the former whole suite. Never silently transfer a pass to another binary. Compositor/renderer/input changes can affect several workflows; unrelated documentation or provider configuration must not invalidate unrelated logic. Where impact is uncertain, rerun the affected workflow.

The runner and ledgers are implemented. Use:

    nix develop .#interface --option builders ''
    cargo xtask park validate final init /absolute/settings.json
    cargo xtask park validate final run /absolute/run --only A1

Select A2 or A3 for their pending observations. The default run selects A1–A3 and never starts a physical session. Initialize a fresh candidate after compiled-source changes; preserve old reports as historical evidence. B2 is explicitly selected for installed-package preparation. Confirm the final installed release with the existing A2 smoke and checks affected by final production fixes, without a sixth workflow.

Delete each pending paragraph only after its required fixes and execution pass, retaining the receipt in the run report. Do not restore completed subjects merely because another subject failed. Do not add duplicate size/orientation/provider sweeps, twenty-cycle stress, six pacing intervals or long resource soaks. The unresolved rendering/input/persistence checks above address actual failures.

## Installed session recipe to validate in B2

No nixpkgs registration is needed. With inputs.lince available to the NixOS configuration:

    { inputs, pkgs, ... }:
    {
      imports = [ inputs.lince.nixosModules.park ];
      programs.lince-park = {
        enable = true;
        package = inputs.lince.packages.${pkgs.stdenv.hostPlatform.system}.lince-ui-source;
      };
    }

withUWSM defaults to true. Retain the existing greeter and Niri. Select a flake input containing the intended source: a remote revision cannot include uncommitted local changes. Omitting package selects the published-binary package, which requires a matching successful publication. The installed Nix derivation has its own build/cache identity; Cargo artifacts do not automatically populate that cache. Validate the installed closure once for B2. Host activation, logout and physical session changes remain scheduled with the user.
