# Park: pending validation

The agreed feature and test source has been authored. This file tracks execution and acceptance still outstanding, rather than implementation tasks already coded. No builds, compiler checks, tests, applications, Nix evaluations, workflows, sessions or system activation were run during the source-only round. Source review is not execution evidence. Later checks may expose corrections; fix those and repeat affected checks before accepting the feature.

Keep the user in Niri until unattended checks pass and a concrete physical command is ready. Retain Niri as a selectable fallback. Do not recommend daily use before physical protection/recovery, clean login and required-application gates pass. Native Wayland support is deliberate; required X11-dependent components remain limitations while XWayland is deferred.

References: [Using Park](../packaging/linux/park.md), [Validation handoff](../packaging/linux/park-validation.md), [fixture template](../packaging/linux/park-fixtures.json), and the unapplied [personal rollout proposal](../packaging/linux/park-personal-rollout.md). Source includes separate Sandbox/Park modes, tiled application policy/configurable commands, asynchronous presentation, native drop/IME/input bridges, Laboratory ownership isolation, external-tool adapters, capture export, transactional output configuration, packaging and the automated runner. Removing their implementation checklist does not mark these additions verified.

## 1. Automated checks while the user stays in Niri

After explicit authorization to execute, follow this order:

1. Inspect the shared working tree and available disk without undoing other agents' changes. Use `cargo check`, never `cargo build`, with warnings denied. Run through `cargo xtask` or the repository Nix environment. Cell-backed policy/Operation checks need `RUST_MIN_STACK=33554432`. Preserve migration guards and owner-authored files.
2. Run validation-tooling regressions for literal arguments, fixture schemas, capabilities, duration/watchdogs and resource reporting. Invalid fixtures must fail before launch; unrelated display/service environments must remain untouched.
3. Run `cargo xtask park ci` and `cargo xtask park input-check`. Check Park session/all-target features and ordinary Sandbox separately. Include server protocols, native editors, layout policy, physical output planning and isolated capture-helper lifecycle regressions. A compiler failure remains a failed gate.
4. Run policy, worker, clipboard, native-client and two-output graphics probes through their xtask commands. Confirm ordinary native Wayland clients and native Sands coexist, identity survives updates and closing the last application leaves Park alive.
5. Run the deliberately delayed GPU synchronization probe. Submission must return before the gate opens, resources must release after completion, and input/reactor dispatch must remain responsive. Exercise transformed damage, immutable snapshots, skipped commits, opaque-region subtraction, superseded/rejected scenes and bounded imports/readbacks. Distinguish shutdown waits from ordinary-frame work.
6. Run the hardware EGL selected-window capture regression explicitly; compare exact colors and row orientation. Run the separate two-output nested capture profile through preparation and require its atomic `nested-capture.json` report. Check authorization, SHM negotiation, completion, transform, monotonic timestamp, initial damage and nonempty pixels. These do not prove visual fidelity or portal compatibility.
7. Verify event-driven nested idling and independently presented output/input snapshots. Repeat open/resize/close and delay application, subsurface, popup and cursor buffers. Record queue, memory and descriptor behavior instead of treating startup as stability evidence.

Create isolated local fixtures using the user's available native Wayland terminal and separate browser profile. Test Ghostty/Firefox and Noctalia first, keeping their invocation in fixtures. Run `cargo xtask park validate prepare /absolute/path/to/fixtures.json`. Preparation must finish before the user receives its copied, identity-checked physical command. It has not run yet; no ready wrapper or checked binary exists merely because source names one.

## 2. Input, layout and Laboratory regressions

1. Verify MOD+H/J/K/L focus, MOD+Shift+H/J/K/L movement, MOD+Ctrl+H/L row stacking and rebinding in Shortcuts. Cover empty/tiny layouts, mixed Sands/application items, fullscreen/maximization, minimization/restoration and inactive workspaces. Activating a dialog must restore minimized ancestors. Parenthood alone must not introduce modality.
2. Run native drop transport/frontend regressions: Unicode, MIME validation, bounds/deadlines, filters/limits, URI consent, stale editor/workspace, removed target, output change, cancellation, and frontend acknowledgment before offer completion. Check Copy-only negotiation, rejected action changes and disconnect races with real clients. Unsupported types must not trigger implicit downloads/imports.
3. Run text-input-v3/input-method-v2 and virtual-keyboard checks: preedit/commit, UTF-8 deletion, selection replacement, private fields, filters/limits, stale replies, restart, physical/virtual passthrough and candidate placement. Repeat with the selected actual IME across focus, output, mode, scale, usable-region and lock changes.
4. Run pointer lock/confinement, relative-pointer and shortcut-inhibition protocol regressions, including escape, focus changes, destruction and lock. Later verify physical transformed deltas/regions. Stale keys, grabs and inhibitors must not survive session/output/workspace transitions.
5. Run idle notification/inhibition tests for hidden/visible surfaces, reference counts, input-idle semantics, lock and output removal/restoration. Verify real activity/deadlines with the chosen provider.
6. Run popup/transient tests for parent output/workspace changes, nested parents, fractional scale and bar/usable-region changes. The reactive output-move regression must keep the popup open and constrained. Check actual layer-shell menus.
7. Run Laboratory ownership, persistence/subscription and two-output GPU checks. Ordinary Cell messages, timers, media and edits must progress while temporary owners cannot read/mutate ordinary Records. Verify injected-binding denial, cleanup of temporary subscriptions/resources, ordinary saves and application survival/restoration. Preserve Sandbox's global Laboratory suspension. Logout/update/encrypted-backup flows must not restart or maintain storage underneath active Park.

## 3. External-tool acceptance

Services remain user-selected. Test independent protocol clients as well as the concrete setup, recording executable/provider versions and modules/protocols exercised. Do not emulate Niri/Sway/Hyprland IPC for a consumer.

1. Verify explicit connection-scoped helper authorization, ordinary-client denial, unrelated reconnect denial and restart/disconnect. The single-helper fixture transfers its readiness connection through exec. Confirm the chosen supervisor/wrapper preserves that descriptor and only affects Park's lifecycle; do not mutate Niri's service environment.
2. Test ext/wlr foreign-toplevel, wlr management and ext-workspace activation with applicable Noctalia modules, then an independent consumer. Check title/app-ID/state/output/parent updates, stale/released/closed handles, minimized windows, ordinary workspace identity and transaction invalidation during lock. Exclude temporary Laboratory workspaces.
3. Test the selected clipboard-history helper with Unicode/supported MIME transfers, selection/focus changes, disconnect/restart and unprivileged denial. Keep history/storage/UI external and preserve Lince's native clipboard connection.
4. Test capture output/window selection, clipping/transforms, optional cursor painting, cursor images/position/hotspot, resize, cancellation, buffer destruction, unmap/source destruction, hotplug, inactivity/resume and helper death. Lock must stop/protect capture, including frames held by the backend. Repeated capture must stabilize resources.
5. Test the installed portal backend, consent/picker and PipeWire stream through the private bus where supported, then the clean session. Upstream xdg-desktop-portal-wlr's ext capture implementation is a candidate, not evidence about the installed version. Reuse providers; do not implement a portal, PipeWire service or picker. Record compositor-private API requirements as provider limitations.
6. Test wlr output-management mode/refresh, position, fractional scale and enable/disable through a selected external tool. Check complete transaction validation, stale serials, test/apply, destroyed requests, last-output protection, controller assignment, resource budgets and stable connector/workspace identity. Require real modesets and a rejected/failed configuration preserving or restoring the previous working layout; planning tests cannot prove DRM rollback.

Use separate fixtures for privileged providers. The default physical fixture runs the independent capture probe; a selected external helper replaces that physical probe, while independent capture remains covered by nested preparation. Startup alone is not compatibility evidence. Login, supervision, bars, launchers, notifications/tray, password UI, IME engines, clipboard history, portal/PipeWire services, catalogs and system settings remain external responsibilities.

## 4. User-assisted physical session

Begin only after unattended gates pass and execution is authorized. The agent prepares/automates the run; a real foreground seat and physical observations require the user.

1. Record the Niri VT and recovery arrangement. Keep Niri logged in, log into an unused TTY as the same user and run the exact prepared `run-seat` command. This initial test does not need display-manager installation or a changed default session. An agent pseudo-terminal is insufficient; credentials remain user actions.
2. Require readiness, repeated native lifecycle probes, the default capture report where applicable, fixture launch and confirmed save-aware logout. Closing applications must leave Park alive. A watchdog/forced exit is failed or incomplete acceptance.
3. Observe scanout and physical key/pointer delivery. Exercise default/rebound commands, editors, terminal/browser windows, focus/tiling, minimized dialogs, fullscreen/maximization and mixed-scale outputs. Opening ordinary Lince against the same directory must activate Operation without another Cell/Sandbox. Reopen after logout.
4. Switch VT and return; suspend/wake through the selected session manager. Check libseat release/reacquisition, clean input and resumed presentation/capture without wedged queues. Passwords and wake remain user actions.
5. Disconnect/reconnect monitors, remove the last connector and change modes/scales/positions. Check independently paced page flips, surviving-output focus fallback, connector/workspace identity, paired visible/input snapshots and input while another output waits.
6. Exercise the selected compatible layer-shell bar and launcher. Change exclusive zones/keyboard ownership, dismiss menus and remove outputs. Bar requests must shrink/restore the usable canvas through Smithay arrangement without hardcoded heights.
7. Test the selected locker with explicit recovery. Acknowledge lock only after every active output presents protection. Include disconnect, hotplug, VT return and suspend while locked. New outputs must protect before normal content, and ordinary input/shortcuts/capture must stay blocked. Protocol tests cannot prove physical protection.
8. Return to Niri for agent inspection of `cargo xtask park validate report /absolute/path/to/run`, logs, atomic probe reports, progress/results and hardware inventories. Missing final results remain incomplete. Record actual device/driver, modes/scales, tool versions and physical observations against the executable digest. Relevant source changes invalidate affected evidence.

Existing independent remote access can help supervision, but installing it is not required. Cable changes, physical input/visual acceptance, passwords/consent and machine wake require the user. The private-bus TTY smoke does not establish clean-login service integration.

## 5. Longer acceptance, packaging and clean login

1. Use fixture lifecycle rounds, observation and soak durations for repeated client lifecycles and a longer hold. Inspect RSS/peak RSS, descriptor, thread and CPU-tick series, including static scenes and capture/helper restart. Record measured duration/growth; an elapsed timeout is not a stability pass.
2. Exercise Sandbox regressions independently, including clipboard, Laboratory, Operation and update/backup/logout behavior.
3. Evaluate/check the existing Nix Park derivation/module. Validate installed executable/runtime libraries, session entry, license/credits and startup/logout. Check the separate Park workflow/artifact while preserving Sandbox and other-platform/headless releases. Hosted CI records unrun graphics/physical gates; a green compiler job does not replace them.
4. Review the unapplied personal proposal against the actual deployed NixOS configuration and intended source tree. Inspected configurations retain SDDM/Niri; Git inputs may omit untracked files. Review the evaluated system change and existing Lince service/directory ownership. Neither personal repository was edited. System activation needs later authorization; this plan does not authorize a rebuild.
5. After authorized installation, test one clean Lince login through the existing display-manager entry. Verify supervisor/readiness environment, chosen services, locking and teardown. Use a clean same-user login or genuinely isolated services: concurrent sessions may share user D-Bus/service state. Keep Niri selectable and its default unless the user chooses otherwise.
6. Inventory required applications/workflows, verify native Wayland behavior and record unsupported X11-dependent components. Finish a representative work session with recovery, locking and clean logout before recommending daily use.

Remove acceptance items only after evidence passes. Keep failures, unavailable gates and user-assisted work explicit. Delete this plan only when the agreed acceptance work is complete.

## Still deferred

- Spatial/3D application surfaces/input picking, per-Area tiling groups and modifier-drag window controls.
- Additional DMA-BUF formats/modifiers, cross-GPU rendering/scanout and broader optimization. Keep the chosen linear single-plane ARGB8888 path; cross-GPU acceptance also needs additional hardware.
- Desktop-entry discovery, a built-in catalog/launcher and additional saved launch-target UI in Operation. Reuse external launchers and local literal targets.
- XWayland and browser/tab-specific integration.

Do not silently pull these into compatibility testing. A provider requiring a deferred format or private API needs an explicit scope decision instead of advertising unsupported behavior.
