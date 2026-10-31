# Park: remaining desktop validation and reinstall handoff

Updated 2026-10-31. B1–B2 share one scheduled physical/login session. No new subjects, provider matrices or stress suites. Native Wayland only. Accessibility, XWayland and the separate workspace refactor are outside this plan. Login, bars, launchers, lockers and portals remain external programs.

## Resume here after reinstalling NixOS

Park can log in through SDDM/UWSM and run real Firefox and Ghostty windows, but the last installed release is not a completed daily-driver candidate. Do not repeat VT switching or suspend tests with that binary: a controlled VT switch terminated Park and its clients. Two corrections are implemented in the working tree and need a newly compiled release, installation and physical validation. A separate output-configuration failure still needs investigation before publishing the replacement.

The repository was `/home/user/git/lince-social/lince`, on `main` at `5daf8c25ebfb1bc5d5e5415c99c40ca426183f11`. At this handoff, these files have uncommitted changes. The new corrections have not been pushed, built into a replacement release or installed by the assistant:

| File | Work to preserve | Actual status |
| --- | --- | --- |
| `crates/park/src/server/buffers.rs` | Advertise the renderer's device with Smithay DMA-BUF feedback; retain accepted format/import restrictions. | Implemented; wire test and real external EGL-client test passed. |
| `crates/park/src/nested/renderer.rs` | Discover the EGL render device, obtain its character-device `rdev`, and supply it lazily when initializing client formats. | Implemented; full session library check passed. |
| `crates/park/src/server/tests/dmabuf.rs` | Default/surface-feedback regression and ignored real EGL-client comparison with and without device feedback. | Both selected cases executed and passed; no installed-release performance pass yet. |
| `crates/park/src/session.rs` | Pause DRM/rendering and input before requesting another VT; restore on a refused switch, avoid pausing for the current VT, skip inactive-seat events and clear consumed VT keys. | Implemented; session library check passed in 25.06 seconds without warnings. Physical recovery has not been retested with this source. |
| `institute/packaging/linux/package-flake.sh` | Stop copying deleted `institute/anicca/Lince.lingua`; package `institute/trails` and `LICENSE` for the flake instead. | Implemented; existing CI artifact successfully repackaged/evaluated locally. Corrected public release still needed. |
| `institute/anicca/Park.md` | This handoff and the remaining execution plan. | Documentation only; retaining this file alone does not retain the source fixes. |

Before erasing a filesystem, preserve or commit/push those working-tree changes and the personal NixOS configuration. `target/park-validation/` is ignored build output, not a Git backup; copy it separately if detailed evidence matters. Preserve the user's actual Lince Cell/workspace data, `~/.config/lince/park.json`, selected external tools' configurations and their wrapper. The session module's default `directory = null` uses the normal user directory; do not invent a replacement data path. No migrations, README or AGENTS.md were changed for this work.

Resume in this order:

1. Recover the source changes and compare them with the table. If only this Markdown survived, reconstruct missing changes from the implementation notes below; the old Git revision does not contain them.
2. Investigate the additional output-configuration failure below within the existing output/recovery work. Do not start a new feature or hardware matrix.
3. Check affected source and run only relevant existing cases. Keep the incremental `target/` if available. Do not repeat valid independent passes without an affected change.
4. Publish one coherent corrected release containing the graphics, VT and any required output-recovery fix, with repaired flake packaging. Verify that binary and flake identify the same source revision.
5. Restore the flake-based login configuration below and install the prebuilt candidate. Keep Niri as fallback. Do not retest the old installed binary to establish whether source fixes work.
6. Complete the six existing tasks below, removing each only after its required fixes and execution pass. Retain the receipts. Do not expand the suite.

## Known failures and implementation notes

### Hardware rendering: source correction exists, installed release still affected

The installed release was `0.8.0`, revision `5daf8c25ebfb1bc5d5e5415c99c40ca426183f11`, target `x86_64-unknown-linux-gnu`, optimized, with `ui`, `instinct` and `park` enabled. Firefox 157 and Noctalia 5.2.1 selected software rendering; Noctalia logged `llvmpipe (LLVM 21.1.8, 256 bits)`. A visible 1280×720/60 video decoded about 60.15 frames/s but delivered only 17.77 video-frame callbacks/s; animation was around 18 FPS. This is not a smooth-video pass, even if an early raw observation labels decoder progress as a pass. Browser callbacks do not prove physical scanout FPS.

The old server advertised linux-dmabuf v3 without device feedback. The correction uses `DmabufFeedbackBuilder` and `create_global_with_default_feedback` when an EGL render device is available. Smithay exposes a feedback-capable global; the regression client binds v4. Device discovery uses `EGLDevice::device_for_display`, prefers `render_device_path`, falls back to `drm_device_path`, checks character-device metadata and supplies `rdev`. A lazy closure avoids repeating filesystem work every frame. With no device, retain the v3 fallback. Keep existing single-plane, linear, accepted RGBA formats and import validation. The unused `enable_dmabuf()` convenience method was removed; the actual format-enabling method now accepts the device closure and returns `io::Result<()>`.

The real external-client test used the same Noctalia executable against a private server and private D-Bus/XDG directories. Without feedback it selected llvmpipe; with feedback it selected `Mesa Intel(R) Iris(R) Xe Graphics (TGL GT2)`. This proves device selection in that fixture, not smooth video on the final desktop. No browser-specific workaround was added.

### VT switching: confirmed installed-release exit; source correction needs physical validation

On 2026-10-31 at 09:36:06 America/Sao_Paulo, Park PID 1344 was on tty2. The user pressed Ctrl+Alt+F6, then Ctrl+Alt+F2. The capture observed tty6, followed by Park exiting with status 1:

```text
The underlying drm surface encountered an error:
DRM access error: Page flip commit failed on device Some("/dev/dri/card1")
(Permission denied (os error 13))
```

Noctalia and Ghostty lost their Wayland connections; SDDM restarted on tty1 and tty2 became a text login. The immediate page-flip error is the relevant failure; subsequent failures to clear/restore DRM state occurred during teardown. UWSM later showed an inactive unit with `Result=success` after cleanup; that property must not override the journal's recorded status-1 process exit. Earlier confused attempts were followed by reboots and lost their journals; the controlled attempt is the conclusive evidence.

The old keyboard path called `session.change_vt` during input handling and could submit another frame before processing the queued seat-pause notification. In `session.rs`, input now records a VT request. The main loop reads `/sys/class/tty/tty0/active`, does nothing for the current VT, and invokes shared idempotent `Hardware::pause` before requesting a different VT. Pause suspends libinput, pauses Smithay DRM outputs, clears input through the existing focus-loss path, retires prepared work and invalidates power/color confirmation. Normal activation uses shared `Hardware::resume` to reacquire/reset outputs, resume input and restore focus. A refused switch restores the active seat and reports feedback. Inactive-seat presentation/device/input events are ignored. Do not replace this with blanket suppression of DRM errors.

Only source checking has passed for the VT correction. VT assignments change across boots and greeter logins. At the last inspection Park was PID 4961 on tty4, and a separate user TTY remained on tty2; these are historical values, never return-shortcut defaults. Discover the current session with logind every time. Do not start a second manual compositor to recover a greeter-managed session.

### Additional output-configuration failure: cause unresolved

A later Park login, PID 4409, exited at 09:46:36 on the same boot. Immediately preceding it:

```text
Mode-setting failed with buffer format DrmFormat {
  code: DrmFourcc(AR24), modifier: Unrecognized(72057594037927944)
}: DRM access error: Error testing state ... (Invalid argument (os error 22))
RecoveryFailure {
  configuration: "... Error testing state ... (Invalid argument (os error 22))",
  rollback: "... Error testing state ... (Invalid argument (os error 22))"
}
```

A permission error restoring state also occurred during teardown. The triggering user action is not established. Do not attribute the failure to a particular monitor setting, the VT race, Noctalia or the modifier without evidence. Review `session.rs`, `session/display.rs` planning/staging/rollback and the relevant Smithay output/compositor calls; distinguish format fallback/recovery from lost device ownership. Ask what the user changed around that login if the saved evidence does not establish it. Resolve this before claiming output configuration/recovery is complete on the corrected candidate.

Historical hardware: Intel Iris Xe/TGL, `/dev/dri/card1`, `/dev/dri/renderD128`; eDP-1 at 1366×768, approximately 60.059 Hz, position `[0,166]`; HDMI-A-1 at 1920×1080/60 Hz, position `[1366,0]`; both scale 1. Rediscover device numbers and topology after reinstall. The user confirmed both displays/pointer crossing, the arrangement UI and a position-preview rollback worked. Resolution/scale changes were not separately reported as passed. Do not inflate that confirmation into exhaustive output validation.

### External display power: integration remains unfinished

Live Noctalia `msg dpms-off` and `msg dpms-on` returned failure before Park received a power request. That version's generic backend tried `wlr-randr`, which was absent from its PATH. No actual display-power cycle passed. Installing that executable alone does not establish a working provider: a child reconnecting through the public Wayland socket does not inherit its parent's per-connection `OutputControl` grant, and disabling an output is not the same check as confirming its power transition.

Park already implements output-power management and authorized helper spawning through a `WAYLAND_SOCKET` connection. Use a compatible external tool/provider through that mechanism, adding only a minimal general binding if necessary. Existing media/screenshot command settings deliberately restrict privileged capabilities; do not smuggle output-control through a screenshot command or weaken public-client permissions. Do not implement a bar-specific workaround in Park. Ordinary/locked power, suspend/wake and normal authentication still need execution with the selected tools.

## B1 — Physical outputs and recovery (needs the user)

- [ ] **B1.2 — Finish output, hotplug and VT recovery on the corrected candidate.** Investigate and correct the additional output-state/rollback failure described above. Verify a supported resolution/scale preview and rollback, and that existing windows remain reachable after reconnecting the second output, then leave and return through a VT. Install the VT correction before repeating the physical check. Determine Park's current VT with logind and capture the service, session and foreground-VT changes; never reuse a previous login's F-key assignment. Require the same compositor process and existing windows to survive, correct output/input mapping, and resumed input without replayed held state. Keep Niri available as the return session.
- [ ] **B1.3 — Validate power and locked recovery.** Wire a compatible external display-power command/provider through the existing authorized helper mechanism; the selected Noctalia version's generic `wlr-randr` handoff failed before issuing a Park power request. Do not replace the bar or weaken connection permissions. Check ordinary display-power recovery and one suspend/wake with the chosen external locker. Require current-epoch power confirmation within the existing two-second bound and no private exposure during locked wake/hotplug. Reuse established locker-loss protocol evidence instead of destructive crash experiments.

## B2 — Installed session and ordinary use (needs the user)

- [ ] **B2.1 — Install and verify the corrected candidate.** Publish the repaired Nix flake and a binary containing the DMA-BUF render-device feedback, VT-handoff and any required output-recovery corrections, then restore the personal flake's Lince input from the temporary local CI artifact to the `main-binary` release. Verify the exact binary revision/features, packaged UWSM session and module evaluation outside the development shell. Keep the corrected `LINCE_CONTENT` path pointing at `content/lince`. Use the prebuilt package, not a local Rust rebuild.
- [ ] **B2.2 — Finish external-provider interaction checks.** Verify Mod+D accepts a launcher query and launches an application using the keyboard, and Mod+Escape locks and unlocks using the selected external locker and the user's normal authentication. The unattended panel open/close check does not establish keyboard focus or successful authentication.
- [ ] **B2.3 — Complete ordinary use on the corrected candidate.** Reuse the existing video check to confirm ordinary Wayland clients select hardware rendering and no longer reproduce the observed software-rendered video/canvas slowdown. In the same session as B1, finish 15 minutes of ordinary browser/terminal/Lince work, including actual layout/dead keys or the configured IME, a saved native Lince edit and outstanding launcher/portal consent UI. WebDriver Unicode entry does not establish physical keyboard or IME behavior. Require no crash, lost work, wrong-target input, private leak or stuck input. Reuse valid independent passes; do not add endurance or provider matrices.
- [ ] **B2.4 — Validate logout and saved restart.** Exercise save-aware logout, return to Niri and log into Park again. Require saved settings to return and Park services/display environment to be cleaned up on exit. Keep the previous NixOS generation available.

B1 and B2 share one scheduled session with the user's participation. Do not log out, switch VT or activate NixOS before that session is arranged. SDR keyboard/mouse operation is the target; HDR, VRR, tablet/touch and connector/rotation matrices are not additional gates.

## Execution rules

Use the existing Cargo profiles and incremental target directory. The former `Build-iteration.md` is absent at this handoff; do not depend on it or recreate a separate planning document. Use `cargo check`, not `cargo build`, for source validation; warnings are errors. Do not run `cargo clean`, create worktrees or spawn coding agents. Run applications through `cargo xtask` or Nix development/shell commands. Build each required artifact once, freeze it and run all pending independent observations in one orchestration pass. A failure must not suppress independent results. Reuse compositor/browser/provider lifetimes where reset is reliable; after a destructive failure, record it and start a fresh private session for the remaining checks. Give GPU/session fixtures one execution slot. CPU-only cases may overlap; performance measurements may not compete with builds or other test scenes.

Use the existing fixture/observation ledgers. Record selector/action, result, artifact identity, relevant inputs, duration and log path; flush each result immediately. An exact selector must execute one case: zero selected cases is not a pass. Preserve valid passes with their original provenance. After a change, rerun failures and affected observations, not the former whole suite. Never silently transfer a pass to another binary. Compositor/renderer/input changes can affect several workflows; unrelated documentation or provider configuration must not invalidate unrelated logic. Where impact is uncertain, rerun the affected workflow.

The runner and ledgers are implemented. Use:

    nix develop .#interface --option builders ''
    cargo xtask park validate final init /absolute/settings.json
    cargo xtask park validate final run /absolute/run --only B2

Select B2 explicitly for installed-package preparation; the default run selects A1–A3 and never starts a physical session. Initialize a fresh candidate after compiled-source changes; preserve old reports as historical evidence. Confirm the final installed release with the existing A2 smoke and checks affected by final production fixes, without a sixth workflow.

Set the final-runner settings field `executable` to the verified absolute prebuilt candidate path, such as `/run/current-system/sw/bin/lince`, before initializing the suite. The template leaves it null; `ensure_candidate` then falls back to compiling/running a release locally. Do not accidentally take that fallback when the intent is to reuse the installed prebuilt candidate. Configure the existing fixture/provider fields for the selected environment instead of copying old `/nix/store` paths.

Delete each completed task only after its required fixes and execution pass, retaining the receipt in the run report. Do not restore completed subjects merely because another subject failed. Do not add duplicate size/orientation/provider sweeps, twenty-cycle stress, six pacing intervals or long resource soaks.

## Installed session configuration

Restore the personal Lince input to `https://github.com/lince-social/lince/releases/download/main-binary/lince-flake.tar.gz` after publication of the corrected candidate. Keep the existing Park module import, `programs.lince-park` configuration and default prebuilt package. The `lin` shell command updates the Lince input and applies the NixOS configuration. No nixpkgs registration or source-package override is needed.

Verify the replacement release retains the UWSM integration. Keep the existing greeter, Niri and its default-session setting until the user chooses otherwise. A configuration rebuild is separate from validating login; no reboot is inherently required for adding the session.

### Restore the published input, not the temporary local artifact

The original CI build was GitHub Actions run `41148219014` / workflow #119, source `5daf8c25`. Compilation succeeded, and the downloaded release binary actually included Park. `publish-main` failed because `package-flake.sh` copied the deleted `institute/anicca/Lince.lingua`. Therefore the public `main-binary` flake/assets remained stale at the time of testing. A successful binary job did not mean the published release had updated. The source packaging correction in the table above fixes this copy step; it still needs publication.

To avoid recompiling the old candidate, the personal flake was temporarily changed to:

```nix
lince.url = "path:/home/user/.local/share/lince/park-prebuilt/5daf8c25ebfb/flake";
```

That local directory is already missing at this handoff, although the old package remains installed in the current system. The temporary flake had pinned an unpacked CI artifact in `/nix/store`; neither that path nor a GC root is a reinstall backup. Do not carry this `path:` input into the fresh system or run `lin` against it. After the corrected public release exists, restore the URL below and refresh the Lince input. Reusing the old CI artifact cannot include the graphics or VT source corrections; those require one new compilation. Do not claim a packaging-only rebuild updates Rust behavior.

The personal configuration repository was `/home/user/git/xaviduds/nixos/`, host `computer`. It contained unrelated edits; preserve those. The flake imports the Park module and supplies `inputs` through `specialArgs`. This is the shape to restore, using the user's actual machine modules and host name:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    lince = {
      url = "https://github.com/lince-social/lince/releases/download/main-binary/lince-flake.tar.gz";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = inputs: {
    nixosConfigurations.computer = inputs.nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      specialArgs = { inherit inputs; };
      modules = [ inputs.lince.nixosModules.park ./configuration.nix ];
    };
  };
}
```

The relevant `configuration.nix` settings are:

```nix
{ inputs, pkgs, ... }: {
  services.displayManager.sddm.enable = true;
  services.displayManager.sddm.wayland.enable = true;
  services.displayManager.defaultSession = "niri";
  programs.niri.enable = true;
  programs.lince-park = {
    enable = true;
    sessionWrapper = [
      "${pkgs.coreutils}/bin/env"
      "RUST_BACKTRACE=1"
      "LINCE_CONTENT=${inputs.lince.packages.${pkgs.stdenv.hostPlatform.system}.lince-content}/content/lince"
    ];
  };
  environment.systemPackages = with pkgs; [ ghostty firefox grim slurp ];
}
```

This supplements the user's normal machine configuration; it is not a complete NixOS hardware/disk configuration. Use the selected external bar/launcher/locker packages separately. The Park module in `institute/packaging/linux/park-module.nix` defaults `withUWSM = true`, installs the prebuilt `lince-park` package, enables graphics and registers the UWSM greeter session. No nixpkgs registration or source-package override is necessary. Select **Lince (Park) (UWSM)** at login. Plain `lince` remains Sandbox/window mode; `--park` is standalone and `--park-nested` is the development/nested compositor.

The old installed session wrapper pointed `LINCE_CONTENT` at `.../content`, causing missing `.../content/instinct/Instinct.lingua`. Actual content is under `.../content/lince/instinct/Instinct.lingua`. The personal source configuration was corrected to `/content/lince`, and its Nix evaluation passed; that change was not activated in the observed installed session. Preserve the corrected wrapper above. Do not edit an owner Record to hide the path failure.

The existing `lin` Bash function updated the Lince input and applied this personal flake. Preserve it if desired, but inspect it after reinstall instead of assuming it survived. Its intended underlying operations, once the input points at the verified public release, are:

```bash
cd /home/user/git/xaviduds/nixos
nix flake update --refresh lince
sudo nixos-rebuild switch --flake .#computer --accept-flake-config
/run/current-system/sw/bin/lince --build-info
ls /run/current-system/sw/share/wayland-sessions
```

Require the expected new revision, optimized release mode, and `ui`, `instinct`, `park` all true. Verify the UWSM session and resolved executable outside the development shell; do not infer the running version from checkout HEAD, a cached release URL or PATH alone. Activation changes the available session, not the executable inside an already-running compositor; log out/in at an agreed time to run the replacement. A reboot is not inherently required.

### Restore per-user external commands

`~/.config/lince/park.json` held keyboard layout `br`, repeat rate 35, repeat delay 300 ms and terminal `ghostty`. Its main descriptions were:

```json
{
  "keyboard": { "layout": "br", "repeat_rate": 35, "repeat_delay_ms": 300 },
  "terminal": { "program": "ghostty" },
  "ready": {
    "program": "/home/user/.config/niri/start-noctalia.sh",
    "capabilities": ["window-list", "window-control", "output-control", "clipboard"]
  },
  "launcher": {
    "program": "/home/user/.config/niri/start-noctalia.sh",
    "arguments": ["msg", "panel-open", "launcher"]
  },
  "lock": {
    "program": "/home/user/.config/niri/start-noctalia.sh",
    "arguments": ["msg", "session", "lock"]
  }
}
```

This excerpt omits saved outputs and screenshots; merge settings rather than overwriting preserved configuration. The path is a user-owned external-tool wrapper, not a Park dependency on Niri. It selected the installed Noctalia executable and invoked its hidden `.noctalia-wrapped` with the system nix-ld loader/library directory so it could load the current NixOS graphics driver. Preserve or reassess that wrapper after reinstall; it does not replace the DMA-BUF fix and should not become a hardcoded Park integration. Any compatible selected providers may replace it. Noctalia uses the PAM `login` service on the tested setup; `/etc/pam.d/login` existed, but a real normal unlock is still untested.

Screenshot commands were configured through `external.screenshot-screen` and `external.screenshot-region`, each with capability `["capture"]`. Print invoked Grim; Ctrl+Print ran Slurp, then Grim, saving private files under `~/Pictures/Screenshots`. The region script unset `WAYLAND_SOCKET` only for Slurp so that Grim retained the inherited authorized connection. Noctalia's own screenshot implementation expected the older wlr-screencopy protocol; Park exposes standard ext-image-copy-capture. The user chose external Grim/Slurp instead of adding an old-protocol adapter. Do not replace that decision with new compatibility scope.

Do not restore hardcoded `WAYLAND_DISPLAY`, `DISPLAY` or `XDG_SESSION_TYPE` exports in `.bashrc`; those were removed so terminals inherit the active compositor's environment. Never copy credentials from shell configuration into this plan or logs.

## Evidence already obtained; do not expand or blindly repeat it

Historical installed-release passes were real visible Wayland clients: Firefox text/button/dialog/context-menu dispatch, fullscreen and restore; a private Ghostty shell and normal exit; native two-window configure/present, background fullscreen/restore, unmap/remap and destruction. Firefox automation used an isolated profile and WebDriver, not headless rendering. Its content screenshot is not proof of compositor popup pixels or capture authorization, and injected Unicode is not proof of physical XKB/dead-key/IME behavior.

The selected bar reserved and released 28 pixels: native app height 724 with the bar, 752 without, then 724 after restoration. An early assertion expected 32 from a private fixture; the actual bar advertised 28, which matched the observed change. Noctalia launcher panel IPC open/close passed, but keyboard query/launch through Mod+D has not been validated. UWSM readiness and the desktop environment were observed. A 900-second read-only single-display observation completed without compositor restart and RSS stayed about 1092–1102 MiB. User applications/canvas were running, so this is not an empty-scene idle CPU benchmark; later samples overlapped compilation and must not be used for performance conclusions. The first observer attempt hit an fdinfo disappearance race; the corrected per-fd collector completed the full observation.

The laptop was connected alone with brightness zero overnight, without logout, VT switching, suspend or system activation. Original user applications were preserved and temporary test clients closed. The user later raised brightness and reconnected HDMI. Brightness zero is historical, not a future-test requirement. Basic desktop login and relogin happened, but a successful saved-edit/save-aware logout/Niri-return cycle was not established. Physical launcher/typing, normal locker authentication, suspend/privacy, power transitions, corrected-renderer video and saved native Lince edits remain in the six tasks.

Useful evidence paths, if backed up:

| Path under `target/park-validation/` | Contents |
| --- | --- |
| `single-display/result.json` | Overnight identity and observations; read its video/resource caveats rather than treating every raw status as a final pass. |
| `single-display/run-1793412015118365758/` | Browser/video, bar/launcher/power and resource receipts. |
| `single-display/feedback-test.log`, `feedback-egl-test.log`, `feedback-check.log`, `native-check.log` | DMA-BUF source regression/check results. |
| `human-session/monitor-workflow.json`, `monitor-journal.log` | User-reported position/rollback workflow and saved arrangement. |
| `human-session/vt-20261031T123416Z/result.json`, `failure-journal.log`, `state.jsonl` | Confirmed VT-induced exit, exact service invocation, process and foreground-VT transitions. |
| `human-session/vt-source-fix.json`, `vt-fix-check.log` | Source correction and successful 25.06-second session check. |
| `human-session/additional-output-failure.log` | Separate mode-state test/rollback failure from PID 4409. |
| `human-session/capture-vt.py` | Read-only VT/service/journal collector; run it before a coordinated retest and retain its on-disk files. |

The former `target/park-validation/login-preflight/` helper scripts and local prebuilt flake directory are absent at this handoff. An earlier user-provided verifier receipt said the candidate was active in `/run/current-system`; that established only the old release. Do not depend on missing scripts/receipts, old `/nix/store` paths, PIDs or socket names after reinstall. The critical failure details are embedded above so losing `target/` does not erase the handoff.

## Focused commands for resuming source validation

These commands document completed checks and how to rerun them only if changed source or the new environment makes that necessary. They do not add tasks or a new test suite:

```bash
nix develop .#interface --option builders '' --command \
  cargo check --locked -p lince-park --no-default-features --features session --lib --jobs 4

nix develop .#interface --option builders '' --command \
  cargo check --locked -p lince-park --no-default-features --features server-tests --tests

nix develop .#interface --option builders '' --command \
  cargo test --locked -p lince-park --no-default-features --features server-tests --lib \
  server::tests::dmabuf_tests::ordinary_clients_receive_render_device_and_filtered_formats_on_default_and_surface_feedback \
  -- --exact --test-threads=1

LINCE_PARK_TEST_PROVIDER=/home/user/.config/niri/start-noctalia.sh \
LINCE_PARK_TEST_RENDER_NODE=/dev/dri/renderD128 \
nix develop .#interface --option builders '' --command \
  cargo test --locked -p lince-park --no-default-features --features server-tests --lib \
  server::tests::dmabuf_tests::external_egl_client_uses_hardware_when_the_render_device_is_advertised \
  -- --exact --include-ignored --test-threads=1 --nocapture
```

Rediscover/select the provider and GPU node before the hardware case. The two exact selectors previously executed one case each and passed in 0.01 and 0.69 seconds of test runtime. Initial source/dependency checking was longer; the final VT session check reused the target and finished in 25.06 seconds. Do not confuse compilation time with runtime or rebuild the full release merely to inspect one source change. The final installed candidate must still be a production release, not a debug desktop performance sample.

For session identification and durable logs, using the currently discovered session ID rather than a remembered VT:

```bash
loginctl list-sessions --no-legend
loginctl show-session SESSION_ID -p VTNr -p Active -p State
cat /sys/class/tty/tty0/active
systemctl --user show 'wayland-wm@lince\x2dpark\x2dsession.service' \
  -p ActiveState -p MainPID -p InvocationID -p Result -p ExecMainStatus
journalctl --user -b --unit 'wayland-wm@lince\x2dpark\x2dsession.service' --no-pager
```

Save the service invocation, `/proc/PID/exe` and build metadata alongside each report. The tested system did not retain previous-boot journals, so save logs into a persistent user directory before a reboot. Agree on the outgoing text VT and actual return VT before a physical test. If Park exits, inspect the saved log and return through the existing greeter to Niri; do not repeatedly reproduce a known destructive failure on the same old binary.
