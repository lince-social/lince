# Using Park

Park runs Lince's canvas as a native Wayland compositor. Sandbox remains the ordinary windowed Lince application. Park targets one graphics device, with preferred modes, horizontal placement, and scale 1 as defaults. Local mode/position/fractional-scale preferences now have source implementations awaiting checks. X11 programs need a native Wayland mode; XWayland is not included.

## Start a session

From a separate TTY, using an existing terminal program as the first application:

```sh
cargo xtask park session --directory /path/to/lince-data -- foot
```

Replace `foot` with an installed native Wayland application and its literal arguments. Everything after `--` belongs to that program. An installed executable starts with `lince --park`; `--directory` is optional and otherwise uses Lince's usual data directory.

The session opens the real Cell and claims its directory. Starting ordinary Lince against the same directory activates Park's Operation instead of opening another Cell. Closing the last application window leaves the desktop running. Logout waits for native edits and workspace storage; if they cannot be saved, Park cancels logout and shows feedback. Successful logout releases the Cell and directory lock. The selected session manager handles other session processes.

Park prints its Wayland socket path. Programs launched through Park receive that display and native Wayland toolkit settings. Park prepares its own display environment before starting threads, so native Lince text editors use the same clipboard as client windows.

Nested development runs inside the existing desktop:

```sh
cargo xtask park nested --directory /path/to/separate-test-data --outputs 2 -- foot
```

Nested development does not open the real Cell. Use a separate directory for experiments. It leaves the parent desktop's clipboard environment in place. The parent compositor may capture Super shortcuts; use its shortcut-passthrough facility or rebind Park's controls for the test. Programs that reuse an existing process may open in the parent desktop; use their supported separate-instance/profile option when testing them.

## Default controls

`MOD` defaults to Super, usually the Windows key. Directional keys follow Vim: H left, J down, K up, L right.

| Keys | Action |
| --- | --- |
| MOD + H/J/K/L or arrows | Focus a neighboring tiled item |
| MOD + Shift + H/J/K/L | Move the focused item between columns or rows |
| MOD + Ctrl + H/L | Stack the focused item into the neighboring column |
| MOD + Enter | Open the selected terminal |
| MOD + D | Open the selected launcher |
| MOD + Space | Open Lince Operation |
| MOD + Q | Ask the focused window to close |
| MOD + F | Toggle fullscreen |
| MOD + Shift + F | Toggle maximization |
| MOD + V | Toggle floating |
| MOD + 1–9 | Select a workspace |
| MOD + Ctrl + arrows | Focus another output |
| MOD + Ctrl + Shift + arrows | Move an item to another output |
| MOD + Alt + Shift + arrows | Move the workspace to another output |
| MOD + I | Inspect the focused application's Record |
| MOD + Escape | Request the selected screen locker |
| MOD + Shift + Escape | Log out |
| Ctrl + Alt + F1–F12 | Switch virtual terminal in a physical session |

Change individual chords in the existing canvas edit panel: Alt + E, then **Shortcuts**. Bindings are saved with Lince's local interface settings and used by Park even when an application has keyboard focus. Desktop chords require Ctrl, Alt, or Super. Row, column, grid, and scrolling-column layouts use their own visible geometry for directional movement; scrolling columns are the initial Park layout.

The numbered workspace shortcuts select existing workspaces. Create workspaces through the existing canvas controls before selecting them by number.

Operation also provides `/launch/terminal`, `/launch/launcher`, `/session/lock`, `/session/logout`, `/stack/left`, and `/stack/right`, alongside the existing layout and window commands. Existing workspaces keep their saved layout. Use `/layout/scrolling`, `/layout/row`, `/layout/column`, or `/layout/grid` to tile a previously free canvas.

## Choose external programs

Create `park.json` inside the selected Lince data directory. It is local executable configuration, separate from synchronized Records. For example, if these programs are installed:

```json
{
  "terminal": { "program": "foot" },
  "launcher": { "program": "fuzzel" },
  "lock": { "program": "swaylock" },
  "keyboard": { "repeat_rate": 25, "repeat_delay_ms": 500 }
}
```

Each program may also have `arguments`, an array of literal strings, and `directory`, a working directory. No shell parses these values. Terminal, launcher, and lock targets reload when invoked. Keyboard repeat settings apply at session startup; rate 0 disables repeat, the maximum rate is 100, and the maximum delay is 5000 ms. XKB uses its standard `XKB_DEFAULT_LAYOUT`, `XKB_DEFAULT_VARIANT`, `XKB_DEFAULT_OPTIONS`, `XKB_DEFAULT_MODEL`, and `XKB_DEFAULT_RULES` environment variables.

With no terminal target, Park tries the generic `xdg-terminal-exec` utility. Launcher and lock actions need explicit targets. Missing tools produce visible feedback. Park does not install or start a bar, launcher, notification service, portal, input method, or clipboard history manager automatically.

Ordinary clients share text selections through Wayland's data-device protocol. Lince's native clipboard provider has internal data-control access; an explicitly configured external history helper can receive its own connection-scoped `clipboard` capability.

An optional `ready` target runs once after the first scene is submitted, with the Park display environment. Use it for the readiness/environment activation command required by your selected session manager. It is a handoff hook, not an autostart list or a process supervisor. Park does not change the parent process's environment or another active desktop's user-service environment automatically.

Local command descriptions can request `capabilities`, an array containing `clipboard`, `window-list`, `window-control`, `output-control`, `capture`, and/or `input-method`. A configured helper receives those permissions on one inherited connected Wayland socket through `WAYLAND_SOCKET`; unrelated connections remain unprivileged. For example, a chosen clipboard helper can use `"capabilities": ["clipboard"]`. The helper must support the standard connected-socket mechanism; reconnecting by the display path does not retain that connection's permissions. Keep separate helper processes under the chosen external session supervisor; several independent Wayland clients cannot share one inherited connection. `window-list` permits read-only ext/wlr foreign-toplevel and ext-workspace listing. `window-control` permits wlr listing plus activation, close, minimization, maximization and fullscreen requests through the existing desktop policy, and ext-workspace activation committed through the standard transaction. Minimization preserves layout membership; activation restores the window and any minimized transient ancestors. Controls are blocked during session lock. Workspace handles preserve canvas/workspace identity across ordering changes and exclude temporary Laboratory workspaces. The protocol advertises activation only; external workspace creation, removal and reassignment are unavailable. This authorization/management path has not been compiled or executed in the current source-only round.

The `input-method` capability permits the standard input-method-v2 and virtual-keyboard interfaces used by an IME for composition and passthrough keys. Ordinary applications use text-input-v3. The offscreen native canvas uses a private internal Wayland client to connect its existing Bevy editors to those protocols. Password fields send empty surrounding text and request sensitive/password input. Candidate popups, Unicode composition, byte-based surrounding deletion, cancellation, and editor/focus identity checks have source implementations and regression tests; compatibility with the user's chosen IME still awaits execution.

The `capture` capability permits the standard ext output/toplevel capture-source and image-copy capture managers. A provider needing window discovery should also request `window-list`. Park reuses the isolated upstream helper backport under `crates/vendor/smithay-capture`, with local lifecycle corrections and connection authorization. The source now includes asynchronous GPU export to SHM, selected-window capture, paired presented output snapshots, cursor images/metadata and optional cursor painting. It protects capture on lock and source removal, and pauses during seat inactivity. Compilation, GPU pixel probes, output/cursor integration and provider acceptance remain outstanding; screenshot/screen-sharing compatibility is unverified. The external portal backend owns consent, selection and PipeWire. Helper license/provenance are included in the unexecuted Nix package and release-artifact definitions.

`output-control` permits the wlr output-management protocol. Its source implementation connects complete test/apply transactions to the nested virtual outputs and the physical connector/mode backend, retaining disabled heads for later re-enabling. It supports advertised modes, nonnegative positions and fractional scales, rejects unsupported transforms/custom modes, and keeps at least one output enabled. Physical requests use a validated plan, wait for outstanding presentation, and attempt DRM restoration if applying the plan fails. Test requests validate topology/layout without modesetting; a successful test cannot guarantee that the kernel will accept a later apply. Runtime changes remain in this session's memory; profile storage and the display settings UI remain external. This adapter, its rollback behavior, and its regression sources have not been compiled or executed.

Local `outputs` preferences are keyed by connector name, for example:

```json
{
  "outputs": {
    "eDP-1": { "scale": 1.25, "position": [0, 0] },
    "HDMI-A-1": { "mode": [1920, 1080], "refresh_mhz": 60000 }
  }
}
```

Scale is between 0.5 and 4 in increments of 1/120. Positions are currently nonnegative logical coordinates; an omitted position continues horizontal placement. `refresh_mhz` means millihertz, so 60000 requests approximately 60 Hz. Explicit modes must be offered by the connector. These preferences are loaded at physical-session startup; they are not a live output-control protocol or a display-profile service. The mixed-scale rendering/input path still needs acceptance evidence and any resulting corrections.

Layer-shell bars specify their own exclusive zones. Park uses Smithay's arrangement to shrink the canvas's usable rectangle; bar heights and positions are not hardcoded. Session-lock clients own their password UI. Park owns input/render protection and remains locked if the locker disconnects. Do not rely on the physical lock path until it has passed the isolated hardware checks listed in the [remaining plan](../../anicca/Park.md).

Restart-dependent updates and encrypted owner backups require leaving Park. They cannot silently restart the active desktop; the backup panel explains how to use Sandbox after logout.

## Optional NixOS session entry

The flake provides `packages.<system>.lince-park` and an opt-in module:

```nix
{
  imports = [ inputs.lince.nixosModules.park ];
  programs.lince-park.enable = true;
}
```

This adds the executable and a **Lince** Wayland session entry to an existing display manager. It does not select the default session, enable a display manager, or select shell services. Device access goes through libseat and the system's existing seat/session setup. `LINCE_PARK_DRM_DEVICE` can select a particular DRM device path.

An unapplied [personal rollout proposal](park-personal-rollout.md) records the inspected configurations and the minimal module import. It does not change either personal repository or activate the system.

## Checks and current limits

```sh
cargo xtask park protocol-check
cargo xtask park input-check
cargo xtask park policy-check
cargo xtask park clipboard-check
cargo xtask park nested --outputs 2 --smoke
```

The development checks cover two-output canvas rendering, native Wayland window lifecycles, bar reservations, fullscreen changes, input handoff, and logout cancellation after a failed save. Firefox has also rendered as a real client through the nested display using a separate temporary profile.

Physical output changes, VT switching, suspend/resume, and lock presentation need validation on an isolated seat. Fractional output configuration, pointer constraints, shortcut/idle inhibition, helper authorization, window/workspace management, IME/native-editor composition, native file-drop transport and capture export have source implementations awaiting compilation and testing. Remaining source audits, integration probes and provider checks are tracked in the [remaining plan](../../anicca/Park.md). See the [validation handoff](park-validation.md) for later automated preparation. No new checks or sessions were run in the source-only round.
