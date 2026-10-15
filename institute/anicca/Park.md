# Park: remaining work for Lince's Wayland desktop

Park targets a Linux desktop session selected as **Lince** at login. It currently runs as a nested compositor; the standalone backend and login packaging are still required. Sandbox remains the ordinary windowed application.

Implement these steps in order. Remove completed tasks after their acceptance checks pass. Keep deferred work and concrete blockers; delete this document when all remaining work is complete.

## 1. Finish Laboratory service isolation across outputs

- [ ] Replace remaining global Laboratory pauses with per-owner isolation so Cell subscriptions and media on other outputs keep updating without allowing benchmark content to mutate ordinary Records. Preserve its temporary workspace on the canonical root and monitor assignments. Changing workspace/output or disconnecting its output closes that temporary Laboratory view.

Acceptance: while Laboratory/practice is active on one output, ordinary Cell subscriptions, timers, and media on other outputs continue updating. Benchmark data cannot read or mutate ordinary user Records, and closing the temporary workspace leaves no subscription, save, or application placeholder behind.

## 2. Finish popup and dialog integration

- [ ] Finish runtime checks for layer-shell popup menus, exclusion by non-modal native controls, multiple views of one application, parent closure, and reactive reconstraining after output or parent geometry changes.
- [ ] Complete parented `xdg_toplevel` dialog placement, focus, and lifecycle policy using the known Wayland parent relationship. Parent metadata alone does not establish complete dialog behavior.

Acceptance: real applications can open and reposition menus/dialogs, activation respects authority, and Record inspection/customization survives close without assigning saved assertions to an unrelated future window.

## 3. Finish rendering efficiency and presentation scheduling

- [ ] Update client textures from damage and use opaque regions to avoid unnecessary composition.
- [ ] Replace polling and blocking GPU fence waits with event-driven presentation and nonblocking in-flight resource retention. Keep compositor dispatch independent of presentation and Cell work.
- [ ] Carry the existing paired image/geometry/input state through per-output presentation. Hit testing uses the last presented frame; retain its resources until GPU/output use completes and release superseded unpresented frames promptly.

Keep Bevy/wgpu Vulkan and Smithay EGL/GLES/DRM as the graphics boundary. No routine CPU frame readback. Continue advertising only the supported linear single-plane ARGB8888 DMA-BUF path until additional formats, modifiers, or synchronization modes pass their own gates.

Acceptance: static scenes do not trigger continuous redraw, busy/frozen clients do not stall input, in-flight resources remain bounded, and memory stabilizes through repeated open/resize/close cycles. Record input and frame-time percentiles under mixed workloads on the initial Intel i915 target before expanding hardware claims.

## 4. Complete input and external-shell capabilities

- [ ] Add keyboard layout configuration, the native/application clipboard bridge, and drag/drop.
- [ ] Implement text input/IME with candidate popups positioned in the correct output coordinates; add relative pointer, pointer constraints, idle inhibitors, and shortcut inhibitors.
- [ ] Validate exclusive/on-demand layer keyboard ownership, overlays, and focus restoration through resize, unmap, disconnect, and output changes. Preserve consumed-chord, release, and cancellation behavior across these transitions.
- [ ] Implement the foreign-toplevel, output-control, and clipboard/data-control capabilities required by the selected external-shell modules. Advertise only completed capabilities.
- [ ] Validate remaining Waybar modules and Noctalia features against an explicit capability matrix, including popup menus and their required protocols. Basic layer-shell bars alone do not establish complete shell support.

External shells remain optional. Reuse native window/workspace controls and Operation; external tools may own bars, OS notifications, tray hosting, locking, and system controls. Session-service ownership and startup belong to steps 6 and 7.

Acceptance: cross-output text entry, clipboard transfer, drag/drop, relative input, shell menus, and focus restoration work with native Lince content and ordinary Wayland applications.

## 5. Connect application launching to Operation and bindings

- [ ] Add desktop-entry discovery and application suggestions to the existing Operation UI.
- [ ] Extend shortcut settings with executable/argument targets and saved local Command Record targets, using the same launch path as Operation.
- [ ] Preserve Command history and execution semantics. Distinguish long-running launches from bounded command-query execution; closing Operation must not terminate a launched GUI application.
- [ ] Add activation tokens, session D-Bus routing, launch supervision and visible failure feedback beyond the existing child-spawn/reap helper. Connect launches to the production session environment established in step 6.
- [ ] Implement explicitly opted-in application startup. Restore native workspace content independently of application startup.

Keep executable authorization, autostart, and desktop bindings local. A synchronized Record must never authorize local execution. Reuse literal executable/argument spawning; do not match launched windows to saved identities by title or app ID.

Acceptance: desktop entries and local Commands launch identically from Operation and bindings, long-running applications survive launcher dismissal, failures are visible, history remains correct, and only locally authorized startup entries execute.

## 6. Implement and package the standalone session

- [ ] Add DRM/KMS, GBM/EGL, libinput, udev, and libseat/logind integration around the existing compositor/presentation boundary.
- [ ] Connect physical output management, mixed/fractional scaling, hotplug, VT switching, and suspend/resume to the per-output views from step 1.
- [ ] Implement secure session-lock enforcement, including newly attached outputs. Locker disappearance must never reveal content or restore ordinary input.
- [ ] Add a Linux Park feature and `lince --park`, preserving ordinary `lince` and headless modes. Connect the real Cell to Park and reuse the shared per-directory lock/activation service: opening ordinary Lince against an active Park session activates it rather than opening another Cell on the same storage.
- [ ] Add standalone `cargo xtask park session` execution.
- [ ] Install a **Lince** Wayland session entry and repository NixOS enablement with required graphics/input/runtime dependencies. Preserve the current Niri session and default session selection.
- [ ] Export the native Park display and session environment to D-Bus/service activation before starting session services or configured applications. Define one owner per singleton session service and start optional external tools through that ownership model.

Acceptance: select Lince at login, open native Wayland applications, activate the existing Cell from an ordinary Lince invocation, switch VT, suspend/resume, and reconnect differently scaled displays without losing workspace ownership or exposing a locked session.

## 7. Finish session services and lifecycle safety

- [ ] Configure file-picker and output-sharing portals, with privileged capture restricted to trusted session helpers. Integrate portal services with the packaged session environment.
- [ ] Integrate an external session-lock client with Park's secure-lock enforcement and service ownership. Client failure must leave the session locked.
- [ ] Connect desktop lock/logout actions and visible session failures to the actual session lifecycle.
- [ ] Keep backup, updates, and exit distinct from Sandbox's close-to-tray and automatic restart behavior. Operations needing stopped storage use an explicit maintenance/logout path rather than implicitly restarting the desktop.

Acceptance: file dialogs and authorized sharing work, unauthorized capture is denied, locker failure stays secure, and logout/maintenance release session and storage resources without unexpectedly restarting the desktop.

## 8. Verify the remaining integration as a complete desktop

- [ ] Add focused regressions for background Laboratory isolation, remaining popup/dialog cases, IME/clipboard/relative input, launch targets, and session lifecycle as their steps land.
- [ ] Exercise real applications and the selected external-shell modules through output changes, disconnects, lock, suspend/resume, and mixed native/external workloads. Include frozen clients and output changes while locked.
- [ ] Verify the installed login entry, Operation/binding launch parity, local startup authorization, logout, VT transitions, and portal authorization in the standalone session.
- [ ] After the remaining changes, check Sandbox, headless Lince, affected consumers, and Park with warnings denied; run focused tests and native/session smoke checks through xtask or the repository Nix environment.

Keep rendering queues bounded and preserve the existing GPU-only normal path. Cell-backed native Operation tests require `RUST_MIN_STACK=33554432`, matching Lince's runtime thread stack.

## Deferred

Spatial/3D application windows; per-Area tiling groups; new modifier-drag controls; cross-GPU presentation optimizations; custom Lince replacements for external desktop services; browser-specific integration; XWayland. Park remains Rust with selected Smithay helpers and native Wayland clients only. Report unsupported capabilities rather than advertising them.

## References

- [Smithay API](https://docs.rs/smithay/0.7.0/smithay/)
- [Waybar client requirements](https://github.com/Alexays/Waybar/blob/0.15.0/src/client.cpp)
- [Layer-shell protocol](https://wayland.app/protocols/wlr-layer-shell-unstable-v1)
- [Niri behavior reference](https://github.com/niri-wm/niri)
