# Park validation handoff

The validation runner is development tooling. Its source is written, but it has not been compiled or executed in the current implementation round. No prepared physical-session command exists until preparation succeeds. Keep daily Lince data separate from this disposable profile.

## Preparation in the existing desktop

After the user authorizes execution, make a local copy of `park-fixtures.json` with absolute executable paths or names available on PATH. Arguments are literal arrays. Preparation replaces `{run}` with the private run directory and `{profile}` with a fresh private directory for that particular client. Choose each application's supported separate-instance options and point its profile option at `{profile}`. The initial user setup uses Ghostty, Firefox, and Noctalia, while the fixture schema remains general. A native Wayland terminal is required. Other clients are optional fixtures.

Run through the repository tooling:

```sh
cargo xtask park validate prepare /absolute/path/to/local-fixtures.json
```

Preparation runs compiler, protocol, policy, clipboard, GPU synchronization, capture-buffer/cursor regressions, the hardware EGL window-export pixel probe, and two-output nested checks. The capture pixel probe requires hardware EGL and is explicitly selected despite being ignored in ordinary test runs. A failure remains a failed gate. Preparation creates a unique private run directory under `XDG_STATE_HOME/lince-park-validation`, falling back to `$HOME/.local/state/lince-park-validation`. It copies the actual Lince executable, native session probe, and runner there, records executable digests and the source revision, chooses test ports, and writes the local `park.json`. It emits the exact `run-seat` command only after preparation succeeds. Compilation happens before switching TTY.

Preparation also copies the output capture probe and runs a separate two-output nested profile with an explicitly enabled readiness hook and a private authorized connection. That probe requests a standard output source, negotiates an ARGB8888 SHM buffer, waits for the compositor export and checks ready/transform/timestamp/initial damage and nonempty pixels. Its atomic JSON report contains counters and dimensions, not the image. Nested hooks require `--ready-hook`; ordinary nested launches leave session-manager hooks alone. The selected-window EGL probe separately compares exact colors and row orientation.

The native probe exercises two windows through map, fullscreen, restore, unmap, remap, destruction, and presentation callbacks. Those callbacks are evidence of the compositor's presentation path, not proof of the physical pixels on the monitor. It deliberately exposes no virtual-input protocol or remote-control service.

Fixtures may set `lifecycle_rounds` to 1–20, `observation_seconds` to 15–300, and `soak_seconds` to 0–3600. Defaults are one round, 90 seconds and no additional soak. Each round reconnects a fresh native probe and saves a separate log. The runner records resident/peak memory, thread count, file-descriptor count and cumulative CPU ticks from the compositor process before/after rounds and once per second during observation/soak. Samples are in `progress.json` and `results.json`; they contain process counters, not window titles, clipboard contents or browser data. Compare growth and static-scene activity after the run rather than treating a successful timeout as stability evidence. All watchdog/client deadlines include the configured rounds and hold time.

An optional `helper` fixture contains `target` (the same literal program/argument shape as a client) and a nonempty `capabilities` array. Allowed capabilities are `clipboard`, `window-list`, `window-control`, `output-control`, `capture` and `input-method`. For example:

```json
{
  "target": { "program": "/absolute/path/to/chosen-provider", "arguments": [] },
  "capabilities": ["capture", "window-list"]
}
```

Park authorizes the readiness connection with exactly those capabilities. The runner writes the readiness manifest and then replaces itself with the helper, retaining its inherited `WAYLAND_SOCKET` descriptor. The helper remains a direct Park child for the disposable run; its diagnostics are in the session log. Separate fixtures test separate providers. This exercises the same connected-socket handoff that a compatible external supervisor or exec wrapper can preserve, without creating an autostart framework. A helper that reconnects using only `WAYLAND_DISPLAY` loses the privileged connection. Helpers and ordinary clients use the private test bus. Consent, secret entry, clipboard history storage, portal/PipeWire services and provider-specific setup remain external. Launching the process is not proof of compatibility; record its actual protocol behavior and installed version.

Without an external helper fixture, the readiness connection execs the copied capture probe and physical acceptance requires its `physical-capture.json` report. With an external helper fixture, the independent capture probe has already run in the nested preparation; physical provider compatibility remains a separate observation. The runner captures kernel/sysfs graphics and connector inventories before and after the physical run. Park records its chosen DRM device and successfully installed output modes, scales and positions in the session log. An unavailable driver-version field remains unavailable.

## Physical smoke

Keep Niri logged in. Use an unused TTY, log in as the same user, and run the exact command preparation printed. The runner rejects an agent pseudo-terminal or inactive/non-seat login. Each prepared run is single-use; prepare a fresh run for a retry.

The run starts a private D-Bus session and points its fixtures at that bus and Park's exact display. It launches the native probe automatically, then the selected external clients. Observe actual windows and use the existing shortcut checklist. The runner requests save-aware logout after the initial observation period if the user has not logged out already. Startup, probe, and session execution have deadlines, with external timeout processes as an independent fallback. Forced termination never counts as successful logout or locking.

Return to the Niri VT recorded during preparation. Read the bundle with:

```sh
cargo xtask park validate report /absolute/path/to/run
```

The run preserves preparation logs, `physical-session.log`, `session.log`, per-round probe/client logs, resource samples, `progress.json`, and, when the runner can finish, `results.json` and `summary.txt`. The outer wrapper records `watchdog-exit.txt` when control returns to it; status 124 indicates its timeout. A watchdog or machine failure may leave only progress and logs; missing final results mean the run is incomplete. Private browser data and clipboard content are not copied into the result manifest. Review logs before sharing them, since an external program controls its own diagnostics.

## Remaining human checks

Real key and pointer delivery, visible scanout, VT return, monitor reconnects, suspend/wake, and protected lock presentation need direct observations. Passwords and consent belong to the user's selected tools. Later lock/suspend runs require an explicit recovery arrangement; the short disposable smoke does not establish lock safety.

After the source changes have passed checks and physical acceptance, enable the existing opt-in NixOS module through a reviewed personal configuration change. Test a clean Lince login with the existing display manager and chosen external services. Keep Niri selectable. The private-bus TTY smoke is not a substitute for the real session's service lifecycle.

## CI and distribution

`cargo xtask park ci` groups the compiler checks and deterministic protocol tests. The Park GitHub workflow produces a separate Linux action artifact containing the executable, license, session entry, usage document, revision, and runtime-library inventory. It does not publish a release or change the ordinary Sandbox update channel. The archive requires compatible system libraries; use the existing Nix package for a dependency-managed installation. Packaging validation and real hardware acceptance remain open until executed.
