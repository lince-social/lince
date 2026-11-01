# Park: remaining acceptance checks

Updated 2026-11-01. Testing and compilation are paused until the user resumes them. This is the remaining finite checklist; subsequent day-to-day use and UX bug reports do not add an indefinite acceptance gate.

## Starting point and blockers

- On 2026-10-31, the installed optimized release identified revision `363669ca48d600b5484239c7ae1f21ec4759ed43`, with `ui`, `instinct` and `park` enabled. That revision includes the DMA-BUF render-device feedback, VT-handoff and flake-packaging corrections. The public release input, Park module and UWSM login entry were restored. Recheck the running executable when testing resumes; checkout changes do not update a running session.
- Ghostty opened after configuring `terminal.program = "ghostty"`. Launcher and locker commands were unconfigured at the last inspection. Restore selected external providers and the intended keyboard settings (`br`, repeat rate 35, delay 300 ms), merging existing settings and preserving Lince data. Use the chosen Grim/Slurp capture integration if needed; do not add legacy capture protocols or a bar-specific workaround.
- The output configuration/rollback crash remains unresolved: on 2026-10-31, mode-setting with AR24/modifier `72057594037927944` failed state testing with `EINVAL`, followed by `RecoveryFailure` for both configuration and rollback. The triggering action is unknown. Investigate `session.rs` and `session/display.rs`; distinguish output-format recovery from lost device ownership before repeating disruptive output checks.
- Display-power integration is unfinished. Noctalia's earlier generic `wlr-randr` handoff failed before Park received a power request. Use a compatible helper through Park's authorized `WAYLAND_SOCKET` connection; a child reconnecting publicly does not inherit output-control permission. Do not weaken permissions or repurpose screenshot grants.
- Automatic `park.json` creation and executable `$TERM` selection with Ghostty fallback were implemented in the working tree. The session library check passed; command tests were interrupted during compilation. Their tests and installation remain pending, separately from the physical checks below. Do not resume compilation without the user's instruction.

## Remaining checks

Execute in this order where dependencies permit. Independent checks may proceed while another is blocked. The assistant prepares tools, captures evidence and investigates failures; the user performs physical actions and confirms visible results.

### 1. B2.1 — Verify the installed candidate

- [ ] Verify the running executable's exact revision, optimized mode and `ui`/`instinct`/`park` features outside the development shell; verify the packaged UWSM session and NixOS module evaluation.
- [ ] Verify content loads from `content/lince` when configuring `LINCE_CONTENT`. Preserve the public `main-binary` flake input, prebuilt package, existing greeter and Niri fallback.
- [ ] If the output investigation requires code changes, publish/install one coherent corrected candidate and verify binary/flake source identity before final acceptance. Reuse the existing A2 smoke and only affected checks; installation alone does not update an already-running Park session.

### 2. B2.2 — External launcher and locker

Requires working selected external providers.

- [ ] Press Mod+D, enter a query with the keyboard and launch an application. Panel IPC open/close alone is insufficient.
- [ ] Press Mod+Escape, lock and unlock using normal authentication. An existing PAM service alone is insufficient.

### 3. B2.3 — Rendering and ordinary interaction

Keyboard, saved-edit and rendering checks can start in an existing Park session; launcher/portal checks depend on provider setup.

- [ ] Confirm ordinary native Wayland clients use hardware rendering and the existing video/canvas check no longer reproduces software-rendered slowdown. The old release delivered about 18 video callbacks/animation frames per second despite approximately 60 decoded frames per second; decoder progress alone is not a pass, and browser callbacks do not prove physical scanout FPS.
- [ ] Test physical keyboard layout/dead keys or the configured IME. Injected Unicode is insufficient.
- [ ] Save an actual native Lince edit and complete outstanding launcher/portal consent interactions.
- [ ] Complete 15 minutes of browser/terminal/Lince work without crash, lost work, wrong-target input, private exposure or stuck input. Combine this with the other checks in the same session; no additional endurance run is required.

### 4. B1.2 — Outputs, hotplug and VT recovery

Resolve the known output/rollback failure first. Use a candidate containing the VT correction; the older `5daf8c25` release exited on a VT switch with a page-flip permission error.

- [ ] Preview a supported resolution/scale change and roll it back successfully.
- [ ] Disconnect/reconnect the second output; existing windows remain reachable and output/input mapping remains correct. Earlier position-preview success does not establish resolution/scale recovery.
- [ ] Discover the current Park VT through logind, agree on departure/return keys, capture service/session/foreground-VT changes, then leave and return once. Require the same compositor process and windows to survive, input to resume and no replayed held keys. Never reuse a historical VT number.

### 5. B1.3 — Display power and locked recovery

Requires the authorized power helper and a working external locker.

- [ ] Turn displays off/on normally; confirm the current power epoch within the existing two-second bound. Disabling an output is not proof of a power transition.
- [ ] Suspend/wake once with the locker active; verify normal authentication and recovered output/input. Check locked wake/hotplug exposes no private content. Reuse existing locker-loss protocol evidence rather than crashing the locker deliberately.

### 6. B2.4 — Save-aware logout and restart

Run last, at an agreed time, keeping Niri and the previous NixOS generation available.

- [ ] Exercise save-aware logout, return to Niri and confirm Park services/display environment are cleaned up.
- [ ] Log into Park again and verify saved settings and the saved Lince edit return.

## Execution and completion

- On resuming, ask the user to perform one concrete physical action at a time, wait for confirmation, inspect evidence, then continue. Coordinate VT switching, suspend, logout and system activation explicitly.
- Record each result immediately in the existing fixture/observation ledger or run report: action/selector, result, executable revision, relevant inputs, duration and persistent log path. Save the service invocation, process/executable identity and logs before disruptive actions or reboot. Old `target/park-validation` evidence was absent after reinstall; do not depend on missing helpers or old store paths.
- Remove each successful checklist item only after its required fix and execution pass, retaining its receipt. Remove a whole group only when all its requirements pass. Keep failures and deferred items; delete this file when everything is complete.
- Reuse valid independent passes with their original provenance. After changes, repeat failures and affected checks only; never transfer a pass silently to another binary. An exact test selector must execute its intended case; zero cases is not a pass.
- Use the installed prebuilt candidate. If using the existing final runner, set `executable` to its verified absolute path before initialization and select `--only B2`; leaving it null can trigger a local release compilation. The default A1–A3 selection does not run a physical session.
- Do not run performance observations alongside compilation or competing GPU scenes. Reuse existing profiles and incremental output; use `cargo check`, treat warnings as errors and never `cargo clean`. Run applications through `cargo xtask` or Nix shell commands.
- No new provider/hardware matrices, stress cycles, resource soaks or duplicate automated suites. Native Wayland SDR keyboard/mouse is the scope; XWayland, accessibility, HDR, VRR, tablet/touch, rotation matrices and the separate workspace refactor are outside this checklist. UX bugs can be recorded at any time.
