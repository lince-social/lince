# Park: installed session and daily use

Implementation started 2026-10-22 under the user's approval. This record keeps the remaining Park work, excluding Spacematter-specific development. Completed features and checks become short prose below; unfinished verification stays explicit. System activation, physical login and human observations still require the scheduled handoff.

## Existing foundation and evidence

Park already runs through the Linux `lince --park` mode; ordinary `lince` opens Sandbox, and `lince --park-nested` is the development mode inside another compositor. Park requires a binary compiled with the `park` feature and its graphics/input/runtime dependencies. A binary on PATH alone does not register a login session or provide host seat access.

The repository already contains `packages.<system>.lince-park`, `nixosModules.park`, the `programs.lince-park.enable` option, and `institute/packaging/linux/wayland-sessions/lince.desktop`. The module installs the package, registers it through `services.displayManager.sessionPackages`, and enables graphics support. It does not supply a login manager. Reuse these implementations.

The normal Linux release workflow selects `instinct,park`; Park supplies reusable checks instead of a separate executable. Binary/source installation share launchers, metadata and licenses. The final optimized binary archive, Nix installation and installed functional acceptance passed. Source-package compilation, checks, finalization and live runtime acceptance also pass. Quiet pacing targets and physical/login acceptance remain below.

Earlier runs reported 142 release tests passing, including hardware GPU checks, nested Firefox menus/input/fullscreen/remap, two-output Noctalia reservations/focus and Sandbox persistence. Those disposable artifacts were removed before this run. Treat those results as historical and reproduce the relevant checks for the new artifact. Earlier fresh-frame rates were approximately 45–51 FPS for Showoff; its 44.5 FPS stress miss remains a failure. Physical scanout and clean display-manager login remain unproven. Current installed-package evidence is recorded below.

## Completed delivery groundwork

The opt-in NixOS module now supports an optional absolute Cell directory and installs one effective session entry. Isolated Nix checks passed for enabled/disabled registration, preserving SDDM and Niri's default, directory overrides with quotes and shell metacharacters, and rejecting relative or immutable-store paths. Checks are in `institute/packaging/linux/park-checks.nix`; evidence is under `target/park-validation/nix-contracts*`.

The shared Nix graphics wrapper preserves absent, empty and explicit host overrides and normalizes previously saved originals before applying its own libraries, so an installed package does not mix a development shell's private driver paths. The packaged session launcher forwards literal argv, keeps an existing D-Bus session and establishes one only when missing. Isolated launcher checks passed, including the D-Bus configuration needed inside a Nix closure. Evidence is `target/park-validation/launch-contracts-final` (including nested-wrapper normalization) and `metadata-contracts.json`.

The Linux GUI (`instinct,park`) and xtask compile with warnings denied. Eighteen validation regressions passed, covering capability inspection, ELF/shell wrapper payload identity, interrupted-child cleanup, fixture bounds, fresh-frame analysis, per-output scanout timing and actual physical-terminal identity. Fresh-frame analysis rejects repeated callbacks, duplicate output observations and the known sub-45 FPS stress miss. All five ownership/activation tests and all 94 protocol/launch regressions pass. Park, ordinary Sandbox, and headless `instinct` modes compile with warnings denied. All 145 current Park release tests pass, including hardware synchronization, Showoff, popup/output rendering, protocol policy and failed-save retry (`lince_park-corrected-suite.log`). The save fixture now selects its intended Record by UID rather than indexing randomly ordered results. All 17 current native layout tests pass, including maximize camera/placement restoration and imported-model placement (`lince_interface-corrected-suite.log`). Physical-session acceptance remains pending.

The installed empty-workspace idle fixture completed 30 minutes and saved its logout; its resource summary is `runs/idle-1792722299/summary.json`. Stable thread counts and non-growing descriptor counts passed, while RSS residency fell under compiler memory pressure; this does not claim total allocation or GPU stability. Previous target artifacts were removed before this implementation run; earlier reported evidence is historical until reproduced for this build.

## Delivery contract

- Publish one Linux GUI executable supporting Sandbox and Park. Keep `--park` explicit; installing or updating Lince must not automatically turn it into the active compositor. Keep headless builds separate and restrict Park packages/apps/modules to supported Linux targets.
- Keep `lince-park` as an opt-in package/session entry point backed by that executable. A second full build of the same GUI binary is unnecessary when the published artifact already has Park enabled.
- Direct TTY startup is `lince --park [--directory PATH]`. Add an explicit flake app, `nix run .#park -- [arguments]`, which selects that mode; invoking a package whose main program is plain `lince` currently does not imply `--park`.
- Display-manager startup selects **Lince (Park)** through a packaged Wayland session entry. The existing `lince.desktop` session identifier and `DesktopNames=Lince` can remain; use an absolute installed executable or session launcher path. Session launch must work outside the checkout without Cargo, a development shell or files under `target`.
- Nix is the packaging and system-registration route for this machine, not a compositor requirement. Other Linux installations use the same executable mode, runtime dependencies, host session/seat access and distribution-installed Wayland session file. Do not build another installer or authentication stack for that route.
- A login manager, service supervisor, bar, launcher, locker, notification service, IME, clipboard history and portal/PipeWire implementation remain external choices. Implement only missing general bindings demonstrated by integration. Noctalia and the user's terminal/browser are initial test fixtures, not product dependencies.

## Remaining work, in implementation order

The eight stages below are the implementation list. Each stage records its decisions, affected code, examples and exit criteria. Examples marked **proposed** are implementation targets, not interfaces that already exist. Shell examples are instructions for the later implementation/acceptance run; none are executed by this planning update. Keep code examples free of embedded comments and keep future configuration fields distinct from today's schema.

| Stage | Result | Main implementation locations |
| --- | --- | --- |
| 1 | One installable Linux GUI binary with both modes and a session entry | `flake.nix`, `.github/workflows/build.yml`, `crates/lince/src/main.rs`, Linux packaging |
| 2 | A session that starts, owns its Cell and shuts down correctly | Park environment/launch/commands/server, Lince activation, external manager bindings |
| 3 | The tested package reaches the normal release/update path | Both Linux workflows, flake checks, `crates/xtask/src/park` |
| 4 | Measured frame pacing and longer resource stability | Park runtime, presentation, renderer and validation fixtures |
| 5 | Chosen external tools work through standard contracts | Existing protocol handlers, native input, shared launch boundary |
| 6 | Real graphics, input, protection and recovery are accepted | `crates/park/src/session.rs`, session display backend, physical runner |
| 7 | The reviewed host configuration exposes Park to SDDM | Existing NixOS module and the selected personal configuration |
| 8 | A clean login and a longer daily workflow succeed | Installed session entry, external session lifecycle, acceptance report |

### 1. Accept the installed Linux artifact

The normal GUI release and source GUI select `instinct,park`; Linux `lince`, `lince-ui` and `lince-park` alias one derivation. Headless source remains explicit, and non-Linux outputs do not expose Park. Ordinary invocation opens Sandbox; `--park`, the Linux `park` flake app, and the installed session launcher select Park explicitly.

`--build-info` reports revision, target, optimization and feature booleans before Cell/runtime/display startup. The release archive derives its metadata from that JSON and includes the shared session launcher, desktop entry and dependency licenses. Both Nix installation paths use those files and validate installed capabilities. The session launcher preserves an existing bus and creates one only when missing. Native child launches restore absent, empty and explicit host graphics overrides through the common wrapper/launch boundary, and remove inherited compositor control sockets.

The optimized executable was archived and installed through the actual Nix binary installer. Its outside-checkout nested run loaded EGL/Vulkan, Showoff assets and a normal native Firefox without development-shell launch shims. The native context-menu capture is complete beyond the browser's bounds. Loader inspection resolved all 36 loaded shared libraries; physical libseat/DRM access remains the foreground-seat acceptance below. Evidence is under `target/park-validation/installed-package`, `installed-loader.json`, `installed-smoke` and `runs/installed-*`. Final artifacts must be regenerated after subsequent rendering changes; preserve executable hashes with results.

The current optimized executable was repackaged and installed at `target/park-validation/final-package`; `candidate.json` records its revision, features and archive identity. Exact-artifact Firefox popup/input/video capture, ownership rejection/activation, forced-save failure/retry, saved logout and Sandbox reopening passed in `browser/runs/installed-1792735553-944580`. The same installed package passed two-output Noctalia reservations and launcher controls in `browser/runs/installed-1792735601-947213`. Both runs preserved Niri's user-manager environment. These functional runs occurred during compilation and do not establish FPS acceptance.
The clean Nix source GUI compiled and passed all 16 package tests (9 + 2 + 5), installed with the correct GUI/Park capabilities, and passed live Showoff/Firefox popup/input/video, ownership, failed-save retry and Sandbox reopen checks. Evidence is `source-package-checks.log` and `source-current-result.json`. The packaging audit found the project license absent from that first source output; the current finalization hook now installs it and the shared build-info/revision files. That exact updated hook passed a new Nix installation contract and was applied to the already-built source payload, with wrapper paths relocated and the payload checksum unchanged. The finalized source artifact passed the same live checks (`source-finalized-result.json`, `source-finalized-functional.log`). This verifies the packaging-only correction separately from the completed compilation/tests; the Rust application source was unchanged. The ignored embedded Showoff GLB is included and compiled, and every literal embedded asset was checked in the source snapshot.

The stable installed session identity is `lince`, displayed as **Lince (Park)**. Its effective `.desktop` points to an absolute packaged launcher. Installation remains additive; it never authenticates users, starts Park automatically or changes the existing default session.

#### Installed inspection example

```sh
park_package="$(nix build --no-link --print-out-paths .#lince-park)"
"$park_package/bin/lince" --build-info
cat "$park_package/share/wayland-sessions/lince.desktop"
```

The JSON must report optimized Linux `ui`, `instinct` and `park` support. Help output alone cannot establish compiled capabilities. Runtime libraries belong to the package closure; seat access, the host GPU driver and chosen services belong to the host session.

### 2. Finish session startup, ownership and teardown bindings

The installed launch path and D-Bus fallback are implemented. The generic module `sessionWrapper` argv prefix supports an external manager without making it a compositor dependency. Its literal-argument and registration checks pass. The user selected UWSM for installed-session service lifecycle. The prepared host uses UWSM and a ready adapter; real manager startup, environment activation and teardown remain acceptance below.
- [ ] Wire the user's selected terminal, launcher, locker and optional helper startup through existing local `park.json` argv/capability descriptions and the chosen external supervisor. Do not add a competing autostart list, login implementation or service supervisor to Park. Each privileged helper needs its own authorized Wayland connection; verify supervisor restart/FD handoff rather than sharing one connected socket among independent clients. Add only a general missing handoff adapter if required.
- [ ] Scope helper processes and display environment to this session, and stop them on normal logout and compositor failure through the selected manager. Test D-Bus/systemd activation of chosen services against the actual Park display. Nested testing or a concurrent Niri session must not have its user-service environment overwritten with Park's `WAYLAND_DISPLAY`.
- [ ] Accept the prepared host Cell handoff at clean login: the Lince desktop autostart is conditioned off when `XDG_SESSION_DESKTOP=Lince`, while Niri retains it. Verify the external manager sets that environment before starting services, preserves saves and leaves one owner. The core session-claim rejection and ordinary activation behavior already pass their regressions.
- [ ] Verify normal startup gives access to Operation, configurable focus/move commands and selected application launch targets without requiring the validation browser or Showoff. Showoff remains an optional workspace, not the daily login session's mandatory UI.

Acceptance: startup reaches an interactive desktop, selected services receive the correct display, closing the last application leaves Park running, logout waits for saves, and the manager stops only Park's session services. A subsequent Sandbox/Niri login can use the same Cell without stale ownership or display environment.

#### Architecture and implementation details

Keep responsibilities explicit:

| Owner | Responsibility |
| --- | --- |
| Display manager / host login session | Authentication, user identity, runtime directory and seat access |
| Existing Park bootstrap in `environment.rs` | Prepare the compositor environment before threads and enter `session::run` |
| Park compositor | Socket, rendering/input, protection, Cell ownership, existing desktop commands and readiness notification |
| Chosen external session manager | Start/restart/stop selected services and manage their process scope |
| Each external provider | Its own UI, configuration, storage and protocol client connection |

No new login/PAM stack, panel framework, user-service daemon or general supervisor belongs in Park. Reuse `reexec_environment`, `Application::command`, `Server::spawn_helper`, `Desktop::ready` and the existing save/logout path. If the selected direct-TTY route lacks a session bus, the external entry wrapper may use `dbus-run-session`; it must not create a replacement bus inside a display-manager session that already has one.

The selected host integration uses UWSM. The following reconstructs the prepared personal binding; the Noctalia path is this user's chosen provider, and the autostart condition applies to this machine's existing `lince.service`, not a service created by the Park module:

```nix
{ pkgs, ... }:
let
  ready = pkgs.writeShellScriptBin "lince-park-ready" ''
    set -eu
    ${pkgs.uwsm}/bin/uwsm finalize XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP XDG_SESSION_TYPE
    unset NOTIFY_SOCKET
    exec /home/user/.config/niri/start-noctalia.sh
  '';
in {
  programs.uwsm.enable = true;
  programs.lince-park.sessionWrapper = [
    "${pkgs.uwsm}/bin/uwsm" "start" "-F" "-D" "Lince" "-e"
    "-N" "Lince (Park)" "--"
  ];
  environment.systemPackages = [ ready ];
  systemd.user.services.lince.unitConfig.ConditionEnvironment = "!XDG_SESSION_DESKTOP=Lince";
}
```

Its user-owned `park.json` ready target is `lince-park-ready` with `window-list` and `window-control` capabilities. That adapter finalizes first-frame readiness before replacing itself with one privileged provider; it does not redistribute its connected socket. UWSM owns the compositor/service scope, failure cleanup and activation environment. Verify those lifecycle behaviors at clean login. A different provider or multiple privileged clients require their own demonstrated handoff, rather than inheriting this connection implicitly.

The intended lifecycle is:

```text
host login -> claim selected Cell -> open seat and display -> first presented scene
          -> ready hook -> external manager starts selected services
          -> user requests logout -> save confirmation -> compositor exit
          -> external manager stops this session's services -> greeter
```

A save failure keeps the desktop alive and reports the problem. A crash must cause the external session scope to end and the greeter to recover. Do not mark a crash or watchdog termination as a successful save-aware logout. Do not add `Restart=always` around a compositor in a way that repeatedly reacquires the seat or returns to an unsaved session without the manager's policy.

Current `park.json` example, using replaceable external tools:

```json
{
  "terminal": { "program": "xdg-terminal-exec", "arguments": [] },
  "launcher": { "program": "fuzzel", "arguments": [] },
  "lock": { "program": "swaylock", "arguments": [] },
  "keyboard": { "repeat_rate": 25, "repeat_delay_ms": 500 }
}
```

These tool names are examples, not defaults to install. The file lives in the selected Cell directory; today the normal default resolves through `utils::config::lince_data_dir()` to the user's configuration directory plus `lince`. `--directory` selects another directory. Continue using literal argument arrays; do not parse shell command strings, overwrite user files with Nix-generated defaults, or invent unsupported `autostart`/`providers` JSON fields.

The optional `ready` description can invoke the user's actual manager readiness adapter. It runs after first submission and receives the Park display environment. The name/path of that adapter remains an external-manager choice, not a new hardcoded service. A manager receiving one privileged `WAYLAND_SOCKET` cannot distribute that single connection to multiple independent clients. Before introducing a binding, demonstrate which selected provider needs it, how a separate authorized connection reaches it, and how restart/revocation works. A working ordinary layer-shell bar must not depend on implementing every privileged provider adapter first.

Cell ownership is implemented and five activation regressions pass. Park requires a session claim before opening the Cell and fails with an actionable error if Sandbox, headless Lince or another Park owns that directory; it never activates the other owner. Ordinary invocations still activate the owner, malformed control messages are rejected, and the directory can be reclaimed after shutdown. Physical and nested Park attach activation events to their event loop. Nested window closure requests save-aware logout. Logout also waits for pending window Record acknowledgments and flushes Park customization files before accepting exit; a write failure cancels logout and permits retry. The final installed artifact passes live forced-failure/retry and same-Cell Sandbox reopening. The corrected save regression also passes in the current 145-test release suite.

The host-service handoff is configured through the selected external manager. For the shared daily Cell, its previous service must save and stop before Park claims it and must not immediately restart. A separate preview directory is allowed for testing, but it is not a silent substitute for the user's daily Cell. Keep a concurrent Niri session on its own display/environment; never run a global user-manager environment import from nested validation. Validate service activation that uses the shared user bus as well as processes launched directly by Park.

### 3. Verify release and update integration

The normal release depends on the reusable Park compiler/protocol/package workflow. Its Linux dependency list includes the required graphics/input libraries, release execution selects the GUI/headless/target feature contract, and actual archive installation is checked through Nix. Metadata comes from the executable, with capability checks and separate headless expectations. There is no second Park-only GUI release. Actionlint, shell validation and isolated Nix registration/launcher contracts pass locally; hosted GPU/physical acceptance is explicitly unrun.

- [ ] Run the connected hosted pipeline when these changes reach CI. Publishing or pushing is outside this implementation's authorization; local checks do not claim a hosted run.
The existing Park update-restart regression passes (`update-restart-guard.log`): even ReadyToRestart does not request canvas exit while desktop host requests are present. The final installed Firefox capture shows a real available-update notification with Park and its client still running (`final-popup.png`); the GUI blocks download-and-restart in Park and a Nix-store installation uses package management. Both new host generations were prepared without activating or replacing the running desktop. Actual next-login generation selection and rollback remain stage 8 acceptance.

Keep `.github/workflows/park.yml` as reusable validation and `.github/workflows/build.yml` as the normal signed release/update path. CPU-only runners report hardware checks as unrun. A GPU/nested runner must consume the same accepted artifact; compilation or help output never substitutes for those results.

### 4. Close remaining performance and stability acceptance gaps

Quiet release baselines at the real 1366×768/60 Hz parent viewport measured ordinary windowed 52.62 FPS (p95 35.30 ms), fullscreen 53.65 FPS (34.55 ms), and two-output Showoff 43.70 FPS (38.56 ms). Bounded background GPU fence waits improved those results to 55.47, 56.13 and 45.95 FPS respectively; frame-interval gates still fail. A two-frame pipeline experiment measured 56.03, 56.48 and 43.45 FPS and was reverted because it regressed Showoff. Evidence is `quiet-matrix.json`, `quiet-completion.json` and `quiet-pipeline.json`, without competing compiler work. Client-import timing remains available for further profiling. All 145 current Park release/GPU regressions pass; final stamped-artifact frame-pacing measurements remain pending. A 30-second cycle profile of the actual final host-packaged Showoff/video session captured 1620 samples without loss (`final-active-perf.data`, `final-active-perf-flat.txt`). ECS query construction accounted for 5.65% of self samples and token-style kind/base resolution for 5.72%; these are measured candidates for CPU investigation, not proof that they dominate frame time. That profile ran alongside compilation and does not establish quiet FPS or physical scanout.

The final host-packaged release completed the four-workload matrix three times, without Cargo/Rust compiler processes during measurement. All 12 functional runs passed, but every pacing gate failed. `quiet-final.json` retains each 10-second warmup, 60-second measurement, frame identity, stage timing, outlier and artifact reference. Physical presentation remains null in these nested runs.

| Final nested workload | Fresh FPS range, 3 runs | p95 interval range | Gate |
| --- | --- | --- | --- |
| Ordinary windowed | 54.32–55.25 | 34.03–35.09 ms | Failed |
| Ordinary fullscreen | 55.70–56.02 | 33.45–33.62 ms | Failed |
| Ordinary, two outputs + bar | 53.42–54.08 | 34.22–34.64 ms | Failed |
| Showoff, two outputs + bar | 44.87–45.20 | 38.18–38.64 ms | Failed |

The 44.87 FPS Showoff sample remains a stress-floor failure, as do the preserved older misses. Ordinary client-import medians of approximately 3–4 ms and native update medians around 9–10 ms are measured profiling leads; two-output export also costs about 7 ms. The next physical trace must establish actual scanout and correlate these costs before choosing further rendering changes. The existing physical runner records kernel page flips and fresh client content separately; these nested failures are not converted into a physical pass.

- [ ] Profile and address the remaining frame-time cost before claiming consistently smooth playback at the output refresh rate. Preserve the failing 44.5 FPS stress sample and distinguish browser-produced video frames, fresh compositor buffers and actual physical presentation. Cover ordinary workspaces and Showoff, windowed/fullscreen playback, popup transitions, and multiple outputs with and without a bar. Do not count repaint callbacks as presentation or lower the acceptance target to hide a miss.
The installed host package passed 20 real browser-window lifecycle rounds on two virtual outputs: create, maximize, fullscreen, restore exact requested dimensions, close and return to the original window with its typed text preserved. Fullscreen geometry and all 20 recorded restores pass their explicit invariants. Descriptors remained within 95–98; saved logout and Niri's unchanged manager environment passed. Evidence is `browser/runs/installed-1792738574-1007501/results.json` and `lifecycle-geometry-check.log`. The reusable runner exposes `--lifecycle-rounds 0..20`; invalid bounds are rejected before creating a session. Round durations of 2.56–2.98 seconds include navigation, geometry transitions and a deliberate one-second wait under compilation, so they do not establish physical input latency or quiet frame pacing. Those remain the quiet matrix and physical acceptance below.

Both final host-package 30-minute idle and active Showoff/video fixtures completed with normal saved logout. After 120 seconds of warmup, idle descriptors changed from 85 to 84 and active descriptors from 102 to 103, with bounded transient ranges; thread counts remained bounded. The main Vulkan DRM client total was unchanged from warmup to finish in each run: 313956 KiB idle and 459620 KiB active, with bounded fluctuations. The active runner verified Niri's user-manager environment before/after. Evidence is `final-soaks.json` and the per-client breakdowns in `final-resource-summaries.json`. RSS residency fell under concurrent compilation pressure; these observations establish bounded resource counts in these workloads, not complete CPU allocation accounting, quiet FPS or physical presentation. The representative 90-minute interactive session remains stage 8.

Acceptance: record reproducible workloads, measured limits and any remaining performance problem. A short nested functional pass is not a daily-use soak or a physical refresh-rate result.

#### Measurement design and correction boundaries

Start with the current `runtime.rs`, `desktop.rs`, `presentation.rs`, `presentation/clients{.rs,/tree.rs}`, `nested/renderer.rs` and physical presentation path. `LINCE_PARK_TRACE_FRAMES=1` already records native update, export, composition and nested submission timings. Use those stages to find work exceeding the frame budget before introducing another rendering backend or replacing Smithay. Optimize demonstrated redundant layout, texture preparation, copies or waits; keep the existing application-window composition architecture unless profiling proves it cannot meet the target.

Keep these invariants during optimization:

- A frame callback is a repaint opportunity, not proof of display presentation. Early callbacks must not release buffers whose GPU use is unfinished.
- Buffer identity alone is not content identity. Preserve commit/version tracking, synchronized-subsurface atomicity and geometry/input-region changes when reusing imported pixels.
- Skip native work only when the relevant scene and client state are unchanged. Animated shaders, video, physics, caret activity and changed output geometry still require updates.
- Cached images/materials must survive Bevy asset preparation and window resize. Keep the passed repeated-color/resize and popup pixel regressions when changing texture handling.
- A native update time, completed GPU fence, nested submission and DRM page-flip event are separate measurements. Correlate their clocks and frame identities; do not subtract incompatible wall/monotonic timestamps or label a fence as physical scanout.

Use a defined workload matrix rather than a single averaged FPS figure:

| Workload | Variants | Measurements |
| --- | --- | --- |
| Ordinary browser video | Windowed/fullscreen, one/two outputs, with/without selected bar | Video production/drops, fresh compositor buffers, frame-time percentiles, actual physical presentation when available |
| Showoff | 2D/3D, WGSL/GLB/map, active physics, popup open/close | The same frame metrics plus native stage timings and input response |
| Actual idle workspace | No video, animated shader, active physics or focused blinking editor | CPU wakeups, render activity, descriptors, retained CPU/GPU resources |
| Repeated applications | Map/unmap/remap, resize/fullscreen/restore, close/reopen, focus/move | Correct geometry/content, latency and resources returning to a stable baseline |

Warm up for 10 seconds, measure video workloads for 60 seconds, and record three runs after closing competing test/compiler processes. For the first 60 Hz daily-use target, the proposed ordinary-workspace gate is at least 57 fresh frames/second, p95 frame intervals no higher than 25 ms, and no repeated interaction stalls over 100 ms. Record observed maximum stalls as well as percentiles. The existing Showoff stress floor remains 45 FPS; its 44.5 FPS miss remains a failure. A higher-refresh configuration needs its own measured budget rather than inheriting a claim of refresh-rate playback from the 60 Hz result.

These are explicit acceptance targets for the performance work, not claims that the current build passes them. The physical display result is authoritative for physical pacing; a nested run also includes the parent compositor's scheduling.

Example of a proposed normalized measurement entry, using the known failing stress sample:

```json
{
  "workload": "showoff-browser-two-outputs-with-bar",
  "refresh_hz": 60,
  "browser_video_fps": 44.65,
  "compositor_fresh_buffer_fps": 44.53,
  "physical_present_fps": null,
  "gate": { "minimum_fps": 45, "passed": false }
}
```

Extend the existing validation report rather than creating another benchmark database. Retain the executable hash, source revision, enabled features, output layout, scale, client/provider versions, trace mode and warmup/measurement duration with each result. A missing physical measurement stays null/unrun, not zero or passed.

The frame-analysis tool is implemented and its synthetic regression tests pass:

```sh
cargo xtask park validate analyze target/park-validation/session.log \
  --warmup 10 --seconds 60 --minimum-fps 57
```

Use `--minimum-fps 45` only for the documented Showoff stress workload. It deduplicates content versions across callbacks and output observations, reports frame intervals and timestamped render-stage costs, and rejects insufficient measurement duration. The current physical runner enables real page-flip tracing: per-output scanout metrics use kernel monotonic intervals and distinguish dispatch-clock fallbacks. Nested reports retain null physical presentation. All callback groups in one composition share a trace timestamp, and startup probes cannot displace the later active application's measurement window. Synthetic timing, duplication and startup-window checks pass; these do not establish physical presentation or compositor performance acceptance.

The existing `crates/xtask/src/park/validation.rs` fixture schema supports lifecycle rounds, observation time and soak time, and already samples RSS, peak RSS, CPU ticks, threads and descriptors. A current-schema starting fixture is:

```json
{
  "terminal": { "program": "xdg-terminal-exec", "arguments": [] },
  "showoff": true,
  "clients": [],
  "lifecycle_rounds": 20,
  "observation_seconds": 90,
  "soak_seconds": 1800
}
```

Replace the terminal only with an installed selected tool and add clients with private profiles when required. The optional `showoff` flag seeds the physical test workspace and the installed nested preparation capture; it defaults to false and does not affect ordinary login. The current limits are 1–20 lifecycle rounds, 15–300 observation seconds and at most 3600 soak seconds. Do not put a longer value into this schema without deliberately changing its bounds. Run a real idle soak separately from an animated Showoff soak. After cache warmup, resource counts must stabilize across repetitions; investigate monotonic descriptor/buffer growth rather than treating every retained shader/font cache as a leak. Stage 8 supplies the longer interactive work session.

Physical preparation also accepts optional `launcher` and `lock` descriptions using the same `program`/`arguments` envelope as `terminal`. It resolves their executables, validates literal argv and expands private `{run}`/`{profile}` placeholders before forwarding them into the generated `park.json`. Both default to unset. The expanded schema passes its 15 validation-tool regressions (`test-fixture-commands.log`); the current suite including installed-session observation passes 18 (`test-session-observer-final.log`). The current prepared basic test has no locker selected; choose that external program and re-freeze before locker acceptance.

### 5. Validate selected external providers

The reusable installed-browser runner is `cargo xtask park validate browser`. Optional `--ownership`, `--save-failure` and `--reopen-sandbox` exercise live ownership rejection/activation, cancelled logout after a forced customization write failure and reopening that saved Cell in ordinary Sandbox. `--lifecycle-rounds 20` adds repeated application window creation, maximize/fullscreen/resize/restore and closure, with per-round geometry/resources and preservation of the original window; it defaults to zero. `--host-prepare` is an external program receiving only the owned nested PID, for a host-specific viewport/focus setup; product logic remains host-independent. It uses an isolated Firefox profile, Cell, runtime directory and private D-Bus activation environment; the parent user-manager display environment is checked before and after. It records the optimized executable identity, native popup capture, WebDriver typing, visible offline video metrics, frame traces, RSS/swap/virtual memory, CPU ticks, threads/descriptors, deduplicated per-process DRM client memory counters when available, and cleanup results. `--host-close` accepts an external program receiving the nested compositor PID; without one, save-aware logout stays unrun. Default probes compile in release mode. A provider fixture supplies ordinary literal argv for its target/show/hide/launcher controls and the specific capabilities it needs; product code has no browser/bar-name exceptions.

```sh
cargo xtask park validate browser /installed/bin/lince \
  --geckodriver /installed/bin/geckodriver \
  --host-close /path/to/close-own-nested-window \
  --plain --fullscreen --seconds 75
```

Add `--provider /path/to/provider-fixture.json` to test real bar reservations by hiding/showing it and comparing maximized browser geometry. Omit `--plain` for Showoff, and add `--keep-size` to keep the browser small enough to see the map, WGSL/GLB and physics around it during the stress workload. Run providers and pixel capture as separate phases because a connected privileged socket is not shareable among independent clients. Runner output and private files stay under `target/park-validation/browser`; frame results remain separate from functional passes and physical presentation.

Free/floating maximize uses the current usable work area, preserves prior placement and restores the canvas camera. Twenty layout-model regressions and the warnings-denied GUI check pass. The final stamped installed package passes two-output Noctalia bar reservation: the maximized client grows from 718 to 752 pixels when hidden and returns to 718 when restored. Evidence is `browser/runs/installed-1792735601-947213`.

Ghostty 1.3.1 renders inside Showoff on the final stamped installed package: the readiness-based capture verifies 114651 pixels within tolerance of its configured background, followed by last-client survival and saved Park logout. Evidence is `runs/client-1792735728-951502`; earlier metadata-only captures do not prove terminal painting. The physical runner restores original host graphics variables and removes inherited compositor control sockets before launching its external clients. Its current validation-tool regressions pass.

- [ ] Extend the passed browser/Noctalia nested checks to the installed package's ordinary launch and session-service paths. Use the user's native terminal/browser and relevant Noctalia modules first; keep bindings compatible with other tools implementing the same protocols.
- [ ] Exercise the user's selected IME, clipboard-history helper, locker, output-control provider and portal/PipeWire backend where required for daily use. Verify connection authorization, helper restart/disconnect, native-editor composition, capture consent/selection/cursor/resize and session teardown. Check a polkit agent or notification provider only through the chosen external implementation when needed.
- [ ] Check native file drops, relative motion/pointer constraints and shortcut/idle inhibition with representative real programs where they are part of the intended workflow. Reuse passed protocol regressions; investigate application integration failures instead of reimplementing those protocols or retesting everything without a reason.

The initial provider/application version inventory is verified in `provider-versions.json`. The table below records the tested scope; granted connection capabilities do not establish that every provider UI module uses them correctly. No missing general binding was demonstrated on these tested paths. Additional selected services retain the integration checks above; unselected optional providers are not requirements for the core compositor.

| Tested program | Version | Accepted installed nested behavior |
| --- | --- | --- |
| Noctalia | 5.2.1 | Two-output bar reservation show/hide/restore and launcher controls; private configuration and service environment. The fixture grants `window-list` and `window-control`, without claiming all taskbar modules accepted. |
| Ghostty | 1.3.1 | Native window pixels inside Showoff, closing the last client without exiting Park, saved logout. |
| Firefox | 157.0 | Native popup/input/video, 20 create/maximize/fullscreen/resize/close cycles, ownership/save retry and same-Cell Sandbox reopen. |

The accepted Noctalia run selects its `ext-workspace` backend. Its startup log also reports `gamma-control=no`, and Park currently has no gamma-control implementation; night-light/color-control behavior is unaccepted. This is a known optional-provider limit, not another core bar/launcher implementation task. The fixture's `output-management=no` reflects its limited granted capabilities, so this run does not accept Noctalia's display-configuration UI; independent output-control protocol checks and real hardware acceptance remain separate. Evidence is `browser/runs/installed-1792735601-947213/session.log`.

Acceptance: configured services work in a Park session and cannot leave another desktop's service environment or permissions altered.

#### Provider contracts and examples

Use the existing protocol implementations and local capability enum as the integration boundary. Application identifiers, window titles and executable basenames are metadata, not authorization. The initial Noctalia setup is a compatibility fixture; passing its bar does not imply every Noctalia module or another portal/IME works.

| External concept | General contract | Existing Park connection capability |
| --- | --- | --- |
| Terminal/browser/ordinary application | Wayland/xdg-shell, clipboard, activation and input protocols as used by the application | None for ordinary window/input operation |
| Bar placement | Layer-shell anchors, margins, exclusive zone and output selection | None just to reserve space |
| Taskbar/window controls | Existing ext/wlr foreign-toplevel and workspace interfaces | `window-list` and, for actions, `window-control` |
| Output configuration | wlr output-management test/apply transactions | `output-control` |
| IME | text-input-v3 for applications, input-method-v2 and candidate surfaces for the provider | `input-method` for the provider |
| Clipboard history | Existing data-control integration | `clipboard` |
| Screenshot/screen sharing | Existing ext capture sources and image-copy capture; selection/consent/PipeWire handled outside Park | `capture`, plus `window-list` when discovery is needed |
| Screen locker | Existing session-lock path and host authentication | Use the existing lock policy; do not invent a new capability name |
| Launcher | Ordinary Wayland UI, selected command, activation when supported | Additional management capabilities only if its actual features require them |
| Notifications/polkit/audio | Chosen D-Bus/host services and their external UI | No invented Park capability |

For a single privileged provider started as the ready target, this current-schema description illustrates the handoff:

```json
{
  "ready": {
    "program": "selected-capture-provider",
    "arguments": [],
    "capabilities": [ "capture", "window-list" ]
  }
}
```

`selected-capture-provider` is a placeholder for the user's actual installed provider. It must accept the inherited connected-socket mechanism; merely changing its executable name does not establish support. This example starts one client, not a complete desktop service set. The external manager adapter from stage 2 must obtain a separate permitted connection for each additional privileged client and each restart. Keep least-required capabilities on each description; reconnecting through the public display path must not acquire them.

For the current validation tool, the optional helper fixture has a different envelope from `park.json`:

```json
{
  "terminal": { "program": "xdg-terminal-exec", "arguments": [] },
  "clients": [],
  "helper": {
    "target": { "program": "selected-bar", "arguments": [] },
    "capabilities": [ "window-list", "window-control" ]
  },
  "lifecycle_rounds": 3,
  "observation_seconds": 90,
  "soak_seconds": 0
}
```

Again substitute a real selected program. Preserve this distinction between fixture `helper.target` and application `ready` descriptions; do not teach users unsupported configuration keys.

For every selected provider, capture version/arguments, supported protocol names, granted capability set, readiness, normal operation, restart/disconnect and teardown results. Test an ordinary unprivileged client against the protected operations as well. For a bar, hide/show it and verify the usable rectangle changes on the right output; do not infer reservations merely from the bar being visible. For capture, require real consent/selection and a successful consumer stream where screen sharing is selected; the passed output-buffer pixel export alone is insufficient. For IME/native editors, cover Unicode composition, surrounding-text deletion, focus change and sensitive fields using existing editor/IME tests plus the selected provider.

Only a demonstrated failure adds implementation work: a missing shared protocol event, connection handoff, launch environment correction or lifecycle binding. Fix the responsible common path and add a regression reproducing that failure. Keep unselected providers recorded as optional/unrun instead of adding mandatory dependencies or writing substitutes for them.

### 6. Complete physical presentation, protection and recovery checks

- [ ] Run the prepared release TTY test while Niri remains available as the return session. Verify actual scanout/input, default and rebound commands, multiple outputs/scales/modes, transactional output rejection/rollback, connector removal/reconnect including the last output, VT return and suspend/wake. Investigate and fix failures; a watchdog exit is not success.
- [ ] With the chosen external locker, verify protection on every output, locker disconnect/hotplug, and blocked ordinary input/capture. Verify Cell activation against the intended directory, last-client survival, save-aware logout and reopen.
Preparation, protocol probes, captures, installed-binary identification and report collection are automated. The runner additionally requires its stdin to be the actual active Linux TTY reported by logind, rejecting terminal emulators and redirected input. The user still supplies foreground TTY login, physical input/visual observations, passwords/consent, cable changes and wake. A VM may automate virtual DRM/login/session lifecycle checks where its graphics support suffices; it does not establish the user's real GPU, outputs, locking or suspend behavior.

The final installed artifact is frozen at `run-1792736145212202998-971866`, exposed through `target/park-validation/physical-test`. It pins the installed Nix closure and hashes the executable, payload and probes. Its private Showoff workspace launches the selected native terminal and an isolated Firefox profile with the local video fixture. The command performs no compilation. Re-freeze after executable changes. Physical execution and observations remain unrun. Detailed fixture limits remain in [Validation handoff](../packaging/linux/park-validation.md).

#### Physical and VM acceptance design

Use `session.rs` and `session/display.rs` for physical backend corrections and the existing shared desktop/renderer for composition/input corrections. First classify a failure by layer: binary/driver loading, seat acquisition, output plan/modesetting, GPU composition, client import, input routing, protection or lifecycle. A whole-compositor rewrite is not a justified response to one failed modeset or popup.

After final preparation, invoke the reported frozen test from a separate foreground TTY:

```sh
/home/user/git/lince-social/lince/target/park-validation/physical-test
```

Niri can remain available on its VT for that isolated test. Super+Shift+Escape requests save-aware logout; Ctrl+Alt+F2 returns to the known Niri VT on this machine. Do not generalize VT 2 as a portable Niri setting: the prepared runner must record the actual return session/VT. The current runner rejects execution from an ordinary terminal window, preventing an accidental physical-seat test inside the active desktop.

The existing generic preparation/report commands are:

```sh
nix develop .#interface -c cargo xtask park validate prepare /absolute/path/to/fixtures.json
nix develop .#interface -c cargo xtask park validate report /absolute/path/to/run
```

Developer preparation still checks/compiles its own release executable. `prepare --executable ABSOLUTE_PATH FIXTURES` now consumes the installed closure, inspects its build info and freezes/hashes that exact executable and payload together with the probes and fixtures. Exact-artifact preparation passed for the final installed package; it does not silently substitute `target/release/lince`. The emitted physical command retains logs on errors, interrupts and normal logout. Its actual foreground-seat run remains pending.

For each physical run, record a before/after inventory of connectors, modes, scales and logical geometry, plus the executable identity, seat, return VT and session exit reason. Exercise at least:

| Check | Required observable result |
| --- | --- |
| Both initial monitors | Correct content and pointer coordinates on each enabled output |
| Scale/mode change | Client geometry/input/capture agree with the applied configuration |
| Rejected configuration | Prior valid desktop remains usable; no partially-applied topology |
| Remove/reconnect an output | Windows, focus and selected workspace remain reachable |
| Remove the last output | No crash/spin; recover correctly when a display returns |
| VT away/back and suspend/wake | Seat/input and rendering pause/resume without stale or unprotected output |
| Lock, locker death and hotplug while locked | Every output remains protected until the valid unlock path completes |
| Save-aware logout | Data is confirmed saved and control returns cleanly to the host session manager |

Automate protocol requests, captures and diagnostics; require explicit human observations for actual scanout, hardware input, cables, wake, passwords and consent. Do not infer a lock pass from an opaque rectangle in a nested screenshot. Do not collect passwords in validation logs.

A NixOS VM is useful for install/registration, user-bus/session lifecycle and virtual output tests if its virtio/Mesa configuration supports this rendering stack. It is optional infrastructure, not a new Park backend. A driver fragment after provisioning an appropriate guest illustrates only package/manager startup:

```python
import json

machine.start()
machine.wait_for_unit("display-manager.service")
info = json.loads(machine.succeed("/run/current-system/sw/bin/lince --build-info"))
assert info["features"]["park"]
```

`--build-info` is implemented in stage 1. This fragment does not log in or prove GPU presentation; the actual test node must install the package, choose the display manager and provision its virtual graphics support. Add a real guest login/session assertion only when the test can observe it. Mark unavailable virtual acceleration or hardware-dependent checks as unrun. The user's physical GPU/outputs, locking and suspend/wake still require the real-seat checks.

### 7. Prepare the user's actual NixOS installation

The active deployment path is `/home/user/git/xaviduds/nixos`, confirmed against its deployed Lince unit and automatic-upgrade configuration. An unactivated copy and diff are prepared under `target/park-validation/host-preview` and `host-preview.patch`; the real personal checkout is untouched. The proposed generation evaluated and built successfully using the actual binary archive. Its SDDM session directory contains both Park and Niri and retains Niri as default.

The preview explicitly uses `/home/user/.config/lince-park-preview`. A separate `host-daily` copy and patch select the normal per-user Cell and condition the existing Sandbox autostart off for Lince's session identity. Both use the user-selected UWSM manager with Park's first-frame ready hook. The core module still exposes a generic argv wrapper rather than depending on UWSM. The ready descriptions are prepared in `host-preview-cell/park.json` and `host-daily-cell/park.json`. Copy/merge the accepted description into the selected user-owned Cell before activating this UWSM configuration; without that hook, UWSM cannot receive the first-frame finalize signal. No actual home configuration has been overwritten. The original service must finish saving at Niri logout before daily Park claims the same directory; no forced takeover or automatic restart loop is introduced.

Both proposed host variants were rebuilt with the current final binary archive. Effective SDDM entries retain Niri as default, invoke UWSM and point to the host-packaged candidate; the previous Sandbox autostart is conditioned off for Lince. Installed capabilities and hashes of the compiled ELF text and read-only data match the independently accepted package; host-specific runtime wrapping gives it a different store path. Evidence is `host-preview-result.json` and `host-daily-result.json`. Neither generation was activated, and real UWSM/login acceptance remains pending.
- [ ] Review the concrete daily Cell/service handoff with the user before the scheduled activation. Preserve Niri and current user files; apply only the accepted configuration changes.

The intended minimal NixOS interface already exists:

```nix
{
  imports = [ inputs.lince.nixosModules.park ];
  programs.lince-park.enable = true;
}
```

Keep the [personal rollout proposal](../packaging/linux/park-personal-rollout.md) and [session proposal](../packaging/linux/park-session-proposal.nix) aligned with the implementation when that documentation work is authorized. The module supplies a session choice to an existing login manager; installing a user-profile binary alone is not equivalent system registration.

#### Host integration decisions and examples

Keep the module opt-in and additive. Its existing `enable` and `package` options remain the primary interface; retain the existing SDDM and Niri configuration. The planned package override lets an accepted local source build be reviewed before its release reaches the published flake:

```nix
{ inputs, pkgs, ... }:
{
  imports = [ inputs.lince.nixosModules.park ];
  programs.lince-park = {
    enable = true;
    package = inputs.lince.packages.${pkgs.stdenv.hostPlatform.system}.lince-ui-source;
  };
}
```

That source GUI output is Park-capable; its clean compilation/package tests and finalized installed runtime acceptance pass as recorded in stage 1. The normal final install should use the accepted binary-backed package default; it should not compile the application on every installation merely to register a session.

**Implemented module field:** `programs.lince-park.directory`, a nullable absolute string, defaulting to null. Null preserves the executable's per-user default directory. A non-null value adds `--directory` through an argv-safe installed session launcher. This supports an explicitly separate preview Cell or a reviewed daily directory without teaching the compositor a Nix-specific configuration path. Do not use a Nix path value that copies mutable Cell data into the store, expand shell variables in it, or create the Cell as root. Reject relative paths, NULs and immutable store locations. Keep default per-user behavior for multi-user hosts; a global directory override is an explicit single-user deployment choice, not a shared Cell for every login account.

The module creates a small session-entry derivation whose `providedSessions` is `[ "lince" ]` and whose launcher invokes `cfg.package`; this wrapper contains metadata/arguments, not another GUI build. Register only that effective entry when a directory/manager wrapper is needed, avoiding duplicate `lince.desktop` entries. Quote directory arguments with `lib.escapeShellArg`. Do not duplicate ordinary Sandbox mode settings or expose a second copy of all provider preferences in Nix; those remain in the existing user-owned configuration or selected external manager.

The two personal repositories are not both activation targets. Identify the active system's flake/revision and the deployed Lince user unit before editing either. `/home/user/git/xaviduds/nixos` currently describes a desktop `services.lince` autostart; the other configuration contains a separately declared older user unit. Review the actual running unit's Cell directory and restart policy, not just a filename in either checkout. Keep ownership handoff explicit; do not stop an unrelated Cell or promise that its unsaved UI can be terminated safely.

Example evaluation/build commands for the staged host and exact accepted binary, avoiding personal lockfile writes and copying `target` into a flake source:

```sh
park_repo=/home/user/git/lince-social/lince
park_host="$park_repo/target/park-validation/host-daily"
park_binary="$park_repo/target/park-validation/accepted-artifact.tar.gz"
nix eval --no-write-lock-file \
  --override-input lince "git+file:$park_repo" \
  --override-input lince/lince-binary "tarball+file://$park_binary" \
  --raw "$park_host#nixosConfigurations.computer.config.system.build.toplevel.drvPath"
nix build --no-write-lock-file \
  --override-input lince "git+file:$park_repo" \
  --override-input lince/lince-binary "tarball+file://$park_binary" \
  --out-link "$park_repo/target/park-validation/host-system" \
  "$park_host#nixosConfigurations.computer.config.system.build.toplevel"
```

Use the actually selected host path, not that example by assumption. The reviewed configuration must already contain the intended module import/enable change. Evaluation/build does not activate it. Keep these actions distinct from system switching, and show the resulting configuration diff, selected package/build info and service ownership change before requesting the later activation.

System activation can affect the display manager and current session. Preserving Niri as an installed/default choice is not a promise that a display-manager restart preserves running terminal tabs. The final activation/clean-login step must be scheduled with the user; do not perform it during ongoing agents or other work. Initial direct-TTY validation and package preparation remain possible without that final login transition.

### 8. Accept the installed login session for daily use

- [ ] After the user approves activation of the reviewed configuration, activate it and verify SDDM offers **Lince (Park)** alongside Niri. Automate preparation and log collection; the user performs the real session selection/login and required interactive observations.
- [ ] Perform one clean login using the installed entry, with no repository/development-shell dependency, and verify selected service startup, normal application launching, tiled/free workflows, native Lince features, locking, suspend/recovery, persistence and logout back to the greeter. Test a subsequent Niri/Sandbox login and package rollback.
- [ ] Complete a representative longer work session and review remaining performance/provider limits before recommending Park as the daily session. Record native-Wayland application coverage; XWayland support remains outside this plan. Update startup/installation instructions only with the tested package and login path, and remove completed plan items once their acceptance evidence passes.

#### Acceptance sequence and evidence

The installed session is the product being accepted. Do not finish this stage using a development-shell command, a nested window, a temporary Showoff-only directory or a binary different from the reviewed package. Showoff remains an available demonstration workspace; the ordinary login must also work without it and without a validation browser automatically opening.

Run the following sequence, with the user performing the real login and required observations:

1. Activate the reviewed host generation at the agreed time. Confirm the greeter offers the stable `lince` session identity as **Lince (Park)** and retains Niri. Leave the default unchanged until the user chooses otherwise.
2. Select Park and log in. Record the executable revision/hash/features and selected Cell directory. Confirm Operation and configured launch/focus/move commands are reachable immediately and that selected services use the Park display.
3. Open the selected native terminal and browser through ordinary user commands, not private test shims. Exercise columns/rows, moving/focusing, unfocusing to the canvas, free placement, resize/fullscreen, popups, clipboard, editing, areas and persistence. Open Showoff to exercise map/WGSL/GLB/physics as an additional workload.
4. Exercise the selected locker, portal/IME/history/output tools that are actually required for this user's daily work, then the accepted physical recovery checks. Record unsupported or unselected provider modules explicitly.
5. Spend at least one representative 90-minute work session in Park, including an ordinary idle period and active application work. Review frame-time/resource evidence against stage 4 rather than relying only on a subjective impression or a startup screenshot.
6. Save and log out through Park. Verify the manager terminates this session's services and returns to the greeter, the Cell lock is released, and a Niri/Sandbox login can use the intended directory and service environment again.
7. Test a subsequent Park login and a package-generation rollback using existing package-manager facilities. Confirm there is one compositor/Cell owner, the expected generation is selected, and no update restarts the active desktop under applications.

The installed-session observer is implemented:

```sh
nix develop .#interface -c cargo xtask park validate observe \
  /installed/bin/lince ACTUAL_PID --seconds 5400 --unit ACTUAL_USER_UNIT.service
```

Use the actual Park PID and installed package; `--unit` is optional and names the chosen manager's user service/scope. Observation validates the executable payload and current user, tracks process start time and executable identity, collects incremental CPU/GPU/descriptor samples, records selected environment/logind facts and optional user-unit status, and writes private `session.json` plus `resources.jsonl` under `target/park-validation/session-*`. Duration is 5–21600 seconds, default 5400; collection ends without stopping Park. Interruptions preserve the report and leave the observed process running. It stops following a PID whose process or executable identity changes. Full argv and environment are omitted. These are test-tool observations, not a login manager or a supervisor.

Eighteen validation regressions pass, including real same-PID executable replacement and literal-path checksum checks. Live nested integration verifies timed completion, interruption, wrong-executable rejection, omission of a private environment sentinel, target survival and unchanged Niri service environment (`observer-live-result.json`). That integration does not establish an installed login or 90-minute work session. An observed exit alone cannot prove saved logout; checks below remain pending until their actual evidence and human observations are supplied. The actual report includes an acceptance section with this shape:

```json
{
  "build_info": { "revision": null },
  "payload_sha256": null,
  "cell_directory": null,
  "checks": {
    "clean_login": "pending",
    "ordinary_application_launch": "pending",
    "selected_services": "pending",
    "lock_and_recovery": "pending",
    "work_session": "pending",
    "saved_logout": "pending",
    "niri_sandbox_return": "pending",
    "next_login_and_rollback": "pending"
  },
  "daily_use_recommendation": "pending"
}
```

Every pass needs its command/probe output or the required user observation, duration, artifact identity and remaining limits. Keep a failed performance threshold separate from successful functional/provider checks. A missing hardware test remains pending; unrelated optional providers do not invalidate the working core session, but a required locker, Cell ownership, save/logout or protection failure blocks recommending daily use.

Use failures to update the responsible earlier stage: missing libraries or a non-Park artifact return to stage 1; an activation/ownership greeter loop or stale session environment returns to stage 2; a release/package mismatch returns to stage 3; frame-time/resource failures return to stage 4; provider interoperability returns to stage 5; real-seat/scanout/protection failures return to stage 6. Do not restart the entire effort or mark an existing subsystem unimplemented because one integration check failed.

When the applicable checks pass, update the authorized installation/startup documentation with the exact tested commands, paths, provider choices and remaining limits. Remove completed work from this plan; retain genuinely unresolved work and keep Spacematter-specific development outside it.

Remaining work is delivery, general session bindings, demonstrated corrections and acceptance. External desktop services stay external, and future Spacematter development does not gate making Park installable.
