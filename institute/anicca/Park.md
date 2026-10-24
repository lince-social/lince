# Park: ergonomics, installed session and daily use

Implementation started 2026-10-22 under the user's approval. This record keeps the remaining Park work, excluding Spacematter-specific development. Completed features and checks become short prose below; unfinished verification stays explicit. System activation, physical login and human observations still require the scheduled handoff.

The 2026-10-23 human test identified the next usability pass below. The final review confirms horizontal tiled rows, Mod+T for workspace tiling, Mod+Shift+T for individual floating, the existing bottom-right controls with Edit always leftmost, and removal of demo navigation/reset buttons. Operation uses Mod+P to preserve Mod+Y/U/I/O monitor focus. Earlier implementation work is recorded below; unfinished controls, defaults, configuration and graphical editors remain until verified. Earlier test passes do not validate these new requirements. The user has exercised a physical session; detailed scanout/protection/recovery acceptance remains pending rather than treating that interaction as no test having occurred.

## Approved scope and implementation order

The 2026-10-24 review selects suggestions **1, 2, 4 and 5**, with **5 kept simple**, and separately confirms Window properties/Record inspection. Suggestion 1 includes one repeatedly pressed command for horizontal width presets, editable without editing a file by hand. That review refined scope; the continuing active goal authorizes implementation and isolated tests. Preserve completed implementation and its evidence, remove only work whose feature and relevant checks are complete, and retain details for remaining implementation and acceptance. Live system activation and physical-session changes still require the scheduled human handoff. The selected numbered suggestions are recorded explicitly below so the original numbering is not lost:

| Reviewed suggestion | Decision and implementation owner | Completion gate |
| --- | --- | --- |
| 1. Stable scrolling layout and horizontal size presets | U8: stable neighbor sizes; one rebindable command, initially Mod+R, cycles 1 → 1/2 → 1/3 → 2/3 → 1; edit the ordered presets in the existing width modal, extended to Window layout by U7 | Exact ratios, one step per distinct press, startup persistence and real-client sizing/constraints; explicit Unfocus must leave the item and camera unchanged; retain the implemented width feature and finish its outstanding U11 integration |
| 2. Recent-window switching | U8: Alt+Tab / Alt+Shift+Tab with stable MRU order, a compact chooser and commit/cancel | Quick switch, reverse, modifier-release commit, Escape restore and safe target disappearance through the real input bridge |
| 3. Search existing windows through Operation | Not selected; deferred | No implementation in this pass |
| 4. Predictable opening and closing | U8: insertion, activation, transient ownership and focus fallback | Ordinary windows and dialogs preserve identity, neighboring sizes and valid focus; background mapping cannot steal the camera |
| 5. Pointer move/resize, simple version | U8: modifier-drag, one placement preview, release to apply and Escape to cancel; preserve ordinary client clicks | Tiled/free gestures commit or restore once, respect client constraints and preserve unmodified application context menus |
| Window properties / Record inspection, separately approved | U9: open the focused item's existing Lince Record through the shared inspector; rebindable action, initially unbound | Assertions use the existing editor/save path; focus changes or window closure cannot redirect the inspected Record or an outstanding Save |

Finish the remaining U7 input/command prerequisites and U8 core work in the reviewed order below, with focused verification alongside each feature. Window properties follows the core interactions; its stable Record target must be ready before the additional opening-rule controls. U1–U7's existing implementation stays in place, and their outstanding acceptance remains explicit.

1. **Suggestion 1 — stable rows and width presets.** Reuse the implemented width model/editor and finish its real-client acceptance. One configurable command, initially Mod+R, cycles **1 → 1/2 → 1/3 → 2/3 → 1**: 100%, 50%, 33⅓%, 66⅔%. Fractions are exact; each distinct press advances once and holding the key does not repeat. The ordered list and separate opening-width preference are editable in Park and saved through the existing Lingua configuration transaction. Resizing the focused item preserves neighbor widths, row height and bar reservations. U8 specifies constrained-client behavior and U11 retains integrated startup/input acceptance.
2. **Suggestion 2 — recent-window switching.** Implement the compact MRU chooser with configurable forward/reverse bindings, initially Alt+Tab / Alt+Shift+Tab, initiating-modifier release to commit and Escape to cancel. Reuse the existing window/workspace identities and focus controller; keep target disappearance and same-update press/release in its acceptance.
3. **Suggestion 4 — opening and closing.** Complete the common placement/activation path, parent-owned dialogs and deterministic close-focus fallback. Preserve stable sizes and saved placements; background mapping cannot steal focus or move the camera. Apply opening preferences once through this path and reuse it for the later rules UI.
4. **Suggestion 5 — simple pointer move/resize.** Add configurable modifier-drag using one target, one interaction owner and one placement preview. Commit on release, cancel with Escape and preserve ordinary application clicks/right-click menus. Keep the first version inside the current workspace; keyboard transfer commands retain cross-workspace/monitor movement. U8 defines the bounded tiled/free behavior; no snapping system, velocity throws or additional gesture mode is part of this suggestion.
5. **Window properties / Record inspection.** Open the focused item's existing Record through the shared inspector, initially with no default chord. Keep that stable target while focus changes, loading finishes or the window closes, and save assertions through the normal Record editor. Application metadata and owner-editable properties remain distinct.

The useful desktop feature families are accepted follow-up work in U9/U10, including families previously described as later. Their coverage table below is the complete family inventory; they are not prerequisites for demonstrating the five core steps. Adding those families does not broaden suggestion 5: its first implementation remains one-target move/resize inside the current workspace, with one preview and commit/cancel. Overview dragging, gestures, groups and scratch-window behavior retain their separate follow-up tasks. Park implements its own policy, shared interface and missing general adapters while the user chooses external services. Track implemented controls, automated integration and provider/physical-device acceptance separately. An unavailable provider is not a reason to rebuild its service or to mark the family complete.

Suggestion **3**, existing-window search through Operation, remains deferred. Ideas proposed specifically from PaperWM/Scroll/newm/Wayfire, camera history and arrangement undo also remain deferred. Keep the existing tiled/free model and ordinary UI destinations. The remaining delivery, provider and physical-session checks still apply; system activation and physical login require the scheduled human handoff.

The first combined review must follow the same core sequence: open two ordinary windows, cycle only the focused window's width, edit/reorder the presets and verify them after restart with the saved binding, switch between the windows, open and close a parent-owned dialog, perform and cancel a modifier-drag, then inspect that window's Record. Preserve the other window's size, normal application clicks and the authored workspace throughout. Use the ordinary controls and existing validation runner, without adding a new showcase interface. U8/U9/U11 hold the detailed acceptance criteria; only remove work after its relevant feature and tests are complete.

## Existing foundation and evidence

The validation output directory was removed during the v6 optimized discovery compilation on 2026-10-23. That compiler exited with a missing object-output directory; its open log was recovered as `target/park-validation/ergonomics-discovery-park-release-v6-recovered.log`. Earlier captures, logs, hashes and test executables under `target` are now historical observations whose original paths are unavailable. Keep their observed results and failures below, but reproduce the current-source acceptance instead of treating a deleted artifact as an inspectable pass. The fresh output directory currently contains the recovered failure and the new keyboard implementation checks. This does not establish a new installed artifact or change any physical-session result.

Park already runs through the Linux `lince --park` mode; ordinary `lince` opens Sandbox, and `lince --park-nested` is the development mode inside another compositor. Park requires a binary compiled with the `park` feature and its graphics/input/runtime dependencies. A binary on PATH alone does not register a login session or provide host seat access.

The repository already contains `packages.<system>.lince-park`, `nixosModules.park`, the `programs.lince-park.enable` option, and `institute/packaging/linux/wayland-sessions/lince.desktop`. The module installs the package, registers it through `services.displayManager.sessionPackages`, and enables graphics support. It does not supply a login manager. Reuse these implementations.

The normal Linux release workflow selects `instinct,park`; Park supplies reusable checks instead of a separate executable. Binary/source installation share launchers, metadata and licenses. The final optimized binary archive, Nix installation and installed functional acceptance passed. Source-package compilation, checks, finalization and live runtime acceptance also pass. Quiet pacing targets and physical/login acceptance remain below.

Earlier runs reported 142 release tests passing, including hardware GPU checks, nested Firefox menus/input/fullscreen/remap, two-output Noctalia reservations/focus and Sandbox persistence. Those disposable artifacts were removed before this run. Treat those results as historical and reproduce the relevant checks for the new artifact. Earlier fresh-frame rates were approximately 45–51 FPS for Showoff; its 44.5 FPS stress miss remains a failure. Physical scanout and clean display-manager login remain unproven. Current installed-package evidence is recorded below.

## Completed delivery groundwork

The opt-in NixOS module now supports an optional absolute Cell directory and installs one effective session entry. Isolated Nix checks passed for enabled/disabled registration, preserving SDDM and Niri's default, directory overrides with quotes and shell metacharacters, and rejecting relative or immutable-store paths. Checks are in `institute/packaging/linux/park-checks.nix`; evidence is under `target/park-validation/nix-contracts*`.

The shared Nix graphics wrapper preserves absent, empty and explicit host overrides and normalizes previously saved originals before applying its own libraries, so an installed package does not mix a development shell's private driver paths. The packaged session launcher forwards literal argv, keeps an existing D-Bus session and establishes one only when missing. Isolated launcher checks passed, including the D-Bus configuration needed inside a Nix closure. Evidence is `target/park-validation/launch-contracts-final` (including nested-wrapper normalization) and `metadata-contracts.json`.

The preceding delivery build compiled the Linux GUI (`instinct,park`) and xtask with warnings denied. Eighteen validation regressions passed, covering capability inspection, ELF/shell wrapper payload identity, interrupted-child cleanup, fixture bounds, fresh-frame analysis, per-output scanout timing and actual physical-terminal identity. Fresh-frame analysis rejects repeated callbacks, duplicate output observations and the known sub-45 FPS stress miss. That build passed all five ownership/activation tests, all 94 protocol/launch regressions, and compilation of Park, ordinary Sandbox and headless `instinct`. It passed 145 Park release tests, including hardware synchronization, Showoff, popup/output rendering, protocol policy and failed-save retry (`lince_park-corrected-suite.log`). The save fixture selects its intended Record by UID rather than indexing randomly ordered results. Its 17 native layout tests passed, including maximize camera/placement restoration and imported-model placement (`lince_interface-corrected-suite.log`). These are historical delivery results; current usability checks are recorded under U1–U11, and physical-session acceptance remains pending.

The installed empty-workspace idle fixture completed 30 minutes and saved its logout; its resource summary is `runs/idle-1792722299/summary.json`. Stable thread counts and non-growing descriptor counts passed, while RSS residency fell under compiler memory pressure; this does not claim total allocation or GPU stability. Previous target artifacts were removed before this implementation run; earlier reported evidence is historical until reproduced for this build.

## Delivery contract

- Publish one Linux GUI executable supporting Sandbox and Park. Keep `--park` explicit; installing or updating Lince must not automatically turn it into the active compositor. Keep headless builds separate and restrict Park packages/apps/modules to supported Linux targets.
- Keep `lince-park` as an opt-in package/session entry point backed by that executable. A second full build of the same GUI binary is unnecessary when the published artifact already has Park enabled.
- Direct TTY startup is `lince --park [--directory PATH]`. The existing Linux flake app, `nix run .#park -- [arguments]`, invokes the packaged Park session launcher; invoking a package whose main program is plain `lince` does not imply `--park`. Reuse that app rather than adding a second entry point.
- Display-manager startup selects **Lince (Park)** through a packaged Wayland session entry. The existing `lince.desktop` session identifier and `DesktopNames=Lince` can remain; use an absolute installed executable or session launcher path. Session launch must work outside the checkout without Cargo, a development shell or files under `target`.
- Nix is the packaging and system-registration route for this machine, not a compositor requirement. Other Linux installations use the same executable mode, runtime dependencies, host session/seat access and distribution-installed Wayland session file. Do not build another installer or authentication stack for that route.
- A login manager, service supervisor, bar, launcher, locker, notification service, IME, clipboard history and portal/PipeWire implementation remain external choices. Implement only missing general bindings demonstrated by integration. Noctalia and the user's terminal/browser are initial test fixtures, not product dependencies.

## Usability pass before the remaining delivery acceptance

Implement U1–U11 sequentially, with appropriate checks as each feature is built and U11 providing integrated acceptance. Keep the eight delivery stages after them: installation, measured performance, external services and physical/login acceptance are still required. U8 is the selected core navigation extension; U9/U10 promote the accepted useful desktop families into concrete work. Hardware/provider acceptance cannot be inferred from synthetic checks. Keep deferred ideas explicitly separate. Do not expand this pass into Spacematter, a bar, a launcher, a locker or a service supervisor.

### U1. Finish the Park controls and settings destination

The native Park tab and /park entry reuse the same shortcut editor as the general Shortcuts tab. Edit is pinned outside the scrollable bottom-right controls, and 2D/3D has stable ordering. All nine toolbar/corner regressions and thirteen edit-panel regressions pass, including late-created control ordering, focus preservation, tab activation/reopen, saved tab scroll and clamping the panel beside the leftmost Edit control on narrow/bar-reserved viewports. Evidence is `target/park-validation/ergonomics-final-controls-tests.log` and `ergonomics-widths-edit-tests-v3.log`.

The persistent advice Sand and the complete Showoff Canvas/Map/WGSL 3D/Scatter/Tile/Columns/Help toolbar were removed. They are not relocated elsewhere. Showoff retains its note, timer, shader, model, map, physics examples and Area. Its release hardware GPU regression passes with the updated contents and persistence checks (`ergonomics-final-gpu-release.log`). Direct camera setup in that render fixture does not establish the ordinary navigation/overview acceptance still required by U4/U11.

The v11 optimized command-help fixture passes at wide/narrow sizes, including suggestion wrapping, typing, unavailable submission, live availability refresh and no-focus width rejection (`ergonomics-external-discovery-release-v11.log`, **14.58s**). All eight shared Operation tests also pass (`ergonomics-external-operation-v12.log`, **2.35s**). This closes the existing help implementation; remaining U7 command additions use that same registry/help path and retain their combined acceptance in U11.

The current command-discovery implementation keeps the existing Park/Shortcuts buttons and modal. `layout/desktop/commands.rs` now owns a read-only `CommandContext` for workspace mode, eligible/focused item, application role, selection, projection and host-bridge presence. Park's binding buttons use the same localized registry label, effective chord, scope and availability reason in their tooltip and accessibility name. They stay editable when the command is unavailable. Cache context per root for each refresh and update existing labels/help; do not rebuild the panel, replace its editor or change row geometry merely to show availability.

`presentation/tests/commands.rs` covers real typing, pointer completion, unavailable submission, host handoff and no-target width feedback at 1000 and 360 pixels. Its preparation stage is `command-discovery-rendering`. Preserve existing toolbar ordering and modal ownership when extending this passing implementation.

The **Overview** control occupies order-3 through U4's shared typed command. Pointer selection and keyboard opening/cancellation pass the completed overview release suite at 1000, 640 and 360 pixels (`ergonomics-overview-gpu-release-final.log`). The inspected combined toolbar is **Edit**, **Focus/Unfocus**, **Tiling**, **Overview**, **2D/3D**, then existing navigation/zoom. Effective shortcut help and the reveal/always-show preference use the existing controls. Areas remains in the edit panel. U11 retains the final combined output-scale and installed-workflow acceptance.

Tiling is one stateful control shared with General settings, with its effective binding in accessible help. Focus/Unfocus uses the same toolbar and registry, with disabled/no-target and selected-target handoff behavior. The obsolete Columns/Grid/layout picker and stack choices are absent from normal Operation/editor discovery; maximize is labelled per item. Internal arrangement APIs remain available to other Lince features. U11 still verifies the combined toolbar through the complete installed workflow.

The expanded rendered check exposed a shared toolbar reparenting bug: using `replace_children` to move late controls into an already-populated scroll container could leave them without `ChildOf`, appearing at the top-left. `order_controls` now first inserts the destination `ChildOf` relationship, then sorts/replaces children already belonging to that parent. The existing toolbar regression now adds another control after the scroll container is populated and verifies both parent links, order, focus and scroll. Keep this relationship update when adding future controls:

```rust
world.entity_mut(control).insert(ChildOf(scroll));
world.entity_mut(scroll).replace_children(&ordered);
```

The subsequent tiling pointer test exposed a separate creation race: the deferred general-control factory and synchronous shell factories could create two toolbars for one canvas before commands were flushed. Their controls overlapped, so a click at the reported Tiling rectangle activated Recenter. General controls now use the same synchronous `toolbar(world, view)` factory as the rest of the shell. The no-flush, two-canvas factory regression and all ten toolbar tests pass (`ergonomics-toolbar-factory-tests.log`); the unchanged pointer-driven tiling scenario passes again (`ergonomics-focus-tiling-debug-v5.log`). Future factories must request that shared toolbar rather than defer creation of another one.

Keep the ordinary Recenter, Bring here and zoom controls: Recenter moves the camera to the origin; Bring here relocates items near the camera. Neither is Focus/Unfocus or overview. The new shortcut modal preserves live editor entities, draft, focus and scrolling; further Park/monitor pages must preserve that behavior rather than rebuilding the edit panel on status changes.

### U2. Share a command registry and persist bindings as Lingua

The typed interface.lingua codec, atomic save path, Mod alias and exact ratio settings are wired into startup and the existing editor. Workspace JSON no longer stores or restores shortcuts. The current registry supplies stable IDs, declared scopes, localized labels and shared repeat policy. All 34 current shortcut/configuration/editor tests, 37 workspace tests and the preceding 13 Lingua tests pass, including startup ordering, recovery isolation, swaps, malformed input, private atomic writes, external-edit rejection and preserving explicit modifiers when Mod changes. The full Park session/test targets and xtask compile with warnings denied (`target/park-validation/ergonomics-widths-settings-tests-v5.log`, `ergonomics-final-workspace-tests.log`, `ergonomics-lingua-tests.log`, `ergonomics-widths-check-v6.log`, `ergonomics-widths-xtask-check.log`). New command behaviors and availability/Operation integration remain below; the implemented input handoff is described in U3.

- [ ] Register U7's remaining commands through the existing stable ID, localized label, scope, availability, repeat policy, default chord and typed-action model. Overview, desktop Edit and current Operation discovery already use this registry; duplicated desktop labels/action mappings have been removed. Do not create a second compositor shortcut table; all entry points must invoke the same semantics.

The duplicated desktop command table is now replaced by registry metadata in `shortcuts/commands.rs`: each discoverable command has one Operation path and uses its existing `Shortcut::desktop_action`, label and settings. Arrow aliases share their primary action without duplicate discovery rows. `/edit` has one entry with the same toggle semantics as its binding, and `/window/properties` is the explicit Record-inspection command. Keep the non-shortcut `/help`, `/store`, `/workspaces` and `/park` panel destinations separate from command bindings. Record lookup remains bounded and unchanged; this adds no existing-window search.

```rust
let command = Shortcut::CycleWidth;
let context = CommandContext::new(world, root);
if context.shortcut(command).is_none() {
    command.desktop_action().unwrap().apply(world, root);
}
```

Operation shows current command help even when unavailable. Enter reports the reason in the same popup and preserves the typed input/focus; a successful command closes it and invokes the shared typed action. Refresh existing suggestion button/text entities, retain the selected command by path and preserve scroll. Clear a stale availability error when its context changes; do not clear a pending Record mutation's result. Host-bridge presence describes where a command can run, not authorization or proof that an external provider is configured. Actual launch/provider/privilege failures remain with the existing backend handlers; U7 supplies the remaining provider settings/actions.

The narrow discovery capture exposed clipped suggestion text. Operation now uses the registry's short localized caption and effective binding for the visible row, with complete scope/availability in its tooltip and accessibility label. Rows wrap at words or characters inside the popup's width; changing availability alone leaves their text, geometry and identity intact. Keyboard selection requests one reveal through `scroll_sand::reveal` after UI layout, keeping keyboard focus in the command input. That helper checks ancestry, measured geometry and scale and bounds the scroll; Operation defers the reveal while a pointer button is held. Query changes and explicit selection may reveal a row; context refresh must not undo a deliberate scroll. The expanded rendered fixture passes shaped text bounds, unchanged row geometry after availability changes and reaching/wrapping the last command by keyboard at both widths, as recorded below.

Scope now also drives desktop key parsing, replacing the separate allowlist that omitted width cycling and incorrectly rejected its modified Space/Escape bindings. `DesktopAction::requires_host` supplies the shared Park classification. Keep the existing validation for native-editor reserved keys, Linux VT chords, overlapping scopes and duplicate normalized chords.

The fresh nested Park/test target passes checking with warnings denied (`ergonomics-discovery-check-v1.log`, 50.43 seconds). Focused discovery/editor/Operation/context unit tests, the rendered wide/narrow fixture, both GUI checks and a full optimized Park run are sequenced under `ergonomics-discovery-*.log`. The first attempted release Park build in `ergonomics-renderer-integration-release-park-v2.log` was stopped because source changed during compilation; it also recorded two accessibility setter type errors, now corrected to pass string slices. Keep it as a stopped/failed build rather than a test result. The pending discovery pipeline must establish current-source acceptance before deleting these remaining checkboxes.

The preceding v3 check passes in 10.46 seconds, its 51 selected interface tests pass in 3.14 seconds, and its expanded wide/narrow native GPU fixture passes in 21.72 seconds, including explicit Unfocus with an existing eligible item. Park/Sandbox GUI checks pass in 51.53/45.41 seconds. That pipeline was stopped before the queued xtask/optimized stages to correct the clipped row shown by visual review; these passes precede the wrapping/selection-scroll changes. The first wrapping check, v4, fails because the Bevy 0.19 API is `TextLayout::linebreak`, not `new_with_linebreak`; the call is corrected. The v5 nested/test check passes in 9.49 seconds with warnings denied, and all **53 focused unit tests pass in 3.15 seconds**. Its GPU run reaches the last command but fails an unjustified exact-zero scroll assertion after wrapping to the first: the border leaves a one-pixel scroll while the whole row is visible. The v6 assertion checks the actual first/last painted row bounds without weakening text-fit or input-ownership checks. That expanded wide/narrow native GPU fixture passes in **78.11 seconds** (`ergonomics-discovery-gpu-debug-v6.log`), and the paired width/terminal/keyboard-scroll captures were visually inspected in `ergonomics-discovery-captures-debug-v6`. Park/Sandbox GUI checks pass in **13.38/9.52 seconds**, xtask checking passes in **2.67 seconds**, and all **18 validation-tool tests pass in 0.10 seconds**. These historical logs use the v6 suffix and precede the keyboard implementation. Their artifacts were removed as described above. The optimized v6 run failed during compilation after that removal; none of its runtime tests ran. Current keyboard/source and optimized integration must be established again, retaining the recovered failure rather than claiming a release pass.

Configuration shape using the existing Lingua Extension grammar and registered width/overview commands:

```text
Interface configuration (@interface-config: 0) {
}
Extension @interface-config "lince.interface" {
    bindings [
        { command "layout.toggle-tiling" chord "Mod+T" scope "desktop" }
        { command "item.toggle-floating" chord "Mod+Shift+T" scope "desktop" }
        { command "item.cycle-width" chord "Mod+R" scope "desktop" }
        { command "focus.toggle" chord "Mod+Backspace" scope "desktop" }
        { command "view.overview" chord "Mod+Space" scope "desktop" }
        { command "operation.open" chord "Mod+P" scope "desktop" }
        { command "focus.left" chord "Mod+H" scope "desktop" }
        { command "focus.monitor-up" chord "Mod+I" scope "park" }
    ]
    width_presets [[1 1] [1 2] [1 3] [2 3]]
    default_width [1 2]
    move_step 64
    gaps 0
    application_opening "maximized"
    mod_key "super"
}
```

Omitted commands inherit defaults; an explicit null chord disables a command. Scope in an exported binding is descriptive and must match the registered command; editing a file must not grant a command permission to run in a more privileged scope or while locked. Normalize modifier ordering, left/right modifier equivalence and the `Mod` alias; on Linux `Mod` defaults to Super and is selectable in the editor. Store a defined key identity rather than an ambiguous rendered character. Default directional letter positions use physical keys; show the current XKB label and physical identity when they differ. Test Brazilian layout and shifted symbols. Keep modifier-only input, unsupported keys and truly reserved OS/session combinations distinguishable from valid desktop chords. The existing general chord parser rejects Space while the desktop parser allows it: `Mod+Space` must validate in the appropriate scope without taking plain Space away from normal controls.

The codec uses `anicca::parse` and `anicca::extension::project`, then typed extraction of the lince.interface fields; it does not project/import the configuration Record into Cell or mint persistent IDs during reads. Output uses the shared Extension renderer and canonical formatter. Configuration is bounded at 256 KiB, contains one local interface-config Record and one matching Extension, and rejects scripts, unknown commands, wrong scopes, repeated command entries and invalid/equivalent width ratios. The save transaction validates the complete candidate, checks the previously loaded source against disk, syncs a private temporary file and atomically replaces the destination before publishing new active bindings. A detected external edit rejects a stale save. Directory-sync failure after replacement is reported as a durability warning rather than falsely claiming the old file was preserved. U9 adds watcher/revision-aware draft resolution; it is not implemented by the initial source check alone.

### U3. Implemented capture and conflict-editor foundation

The native editor in `crates/interface/src/native/shortcuts/editor.rs` replaces typed per-row saves. Click a binding, press its combination, then Validate/Enter or Cancel/Escape. Modifier-only input and repeats are ignored; unsupported/native-reserved chords show an error, modified Escape can be recorded for desktop commands, and Linux VT chords are rejected because the physical backend owns them. Mod is selectable in the draft, with effective shared help/accessibility labels refreshed after saving.

One searchable, scrollable modal owns the batch. It shows every conflicting command, supports inline capture/unbind/reset and direct swaps, and prevents Save until ambiguity is resolved. All currently registered scopes can overlap; their different display names do not authorize duplicate chords. Affected rows stay visible after their conflicts are resolved. Reset-all is a reviewable draft change. Save uses the same validated atomic Lingua transaction; Cancel leaves the active configuration unchanged. File revision/write failures preserve the draft, entities, keyboard focus and scroll. U9 still supplies live external reload/rebase.

Input ownership is shared with Park through `InputSnapshot.keyboard_modal`, `keyboard_capture` and the computed rectangles of `KeyboardModalTrigger` controls. The compositor defers input through its existing tracked-serial acknowledgement when a click changes keyboard ownership. The native path also buffers a key arriving in the capture click's own frame, using Bevy's computed hit geometry/clipping instead of hardcoded rectangles. Captured keys remain quarantined through release; an already-held compositor shortcut cannot repeat a newly assigned action. Capture bypasses/resets compose state and suspends native IME edits, while ordinary modal search retains normal text composition. Session locking retains the existing higher-priority protection path.

Modal cancellation resolves plain Escape against the owning overlay's explicit cancel binding before Bevy's text widget consumes it to collapse a selection. Active IME composition keeps priority over that cancellation; modified Escape remains available to chord capture. The owner resource also suppresses underlying native binding scopes, and closing either settings modal quarantines held keys through release. GPU input fixtures must supply actual logical modifier keys as well as physical key codes: Bevy text selection reads `ButtonInput<Key>`, while the desktop commands also use physical `KeyCode` state.

All 34 shared shortcut/configuration/editor regressions pass, including the width editor. The release GPU shortcut test drives pointer activation, a key in the activation frame, conflict resolution and Save through the real native controls at 1280×720, 640×640 and 360×720; it verifies input ownership, no leaked native key, and reachable footer/focused controls. Captures are under `target/park-validation/ergonomics-final-captures-release`; logs are `ergonomics-final-gpu-release.log` and `ergonomics-widths-settings-tests-v5.log`. The full UI test exposed and fixed a schedule cycle: refresh runs after ApplyActions and before UI Layout, not before Prepare, since editable text belongs to UI Content. These establish rendered correctness, not installed-session pacing or a new physical login.

A temporary conflict is permitted only inside a draft, for example:

```rust
let mut draft = active.clone();
draft.stage(Shortcut::DeleteSand, Some("W"))?;
draft.stage(Shortcut::Forward, Some("Delete"))?;
draft.validate()?;
configuration::commit(world, draft, layout)?;
```

Continue to use this implementation for U4/U7's new built-in/external commands. If genuinely exclusive activation contexts are introduced, model their actual overlap rules in the registry and add corresponding reuse tests; scope labels alone are insufficient. U11 retains actual Wayland client/IME/lock/inhibition and release-artifact acceptance. The native GPU test does not substitute for those compositor/provider checks. The tutorial Ctrl+Shift+Escape recovery remains reserved only during practice.

### U4. Existing focus, tiling, monitor navigation and overview foundation

Mod+T and the Tiling control switch the whole workspace between free canvas and `WorkspaceLayout::Tiled`. Horizontal rows use H/L within a row, K/J to the nearest presented horizontal center in an adjacent row, and Shift to reorder/transfer. Vertical edge moves create useful rows and remove empty ones; horizontal and singleton edges do not wrap or create empty rows. Mod+Shift+T floats/retiles one item, restoring a surviving row/slot or reconstructing its former row. In free mode it reports a harmless no-op. Live application surface identities remain intact.

`layout/desktop/rows.rs` serializes row IDs, member widths, saved row slots/neighbors, free geometry and the free camera. The native transition captures the latest free placement before projection and restores the saved view/placement when untiled. A one-frame observation guard prevents tiled geometry from overwriting the free baseline. Fullscreen/maximize camera caches include their layout mode. Imported scene models stay outside window tiling. Keep these APIs as the shared arrangement model; normal Park discovery exposes only tiled/free, without a competing column/grid/stack picker.

The current source implements shared free-canvas focus in `native/layout/desktop/focus.rs`. `ensure` enables management for explicitly used free workspaces, including a fresh Sandbox, without a preliminary tiling roundtrip. Mod+H/J/K/L chooses an eligible native/application item in the requested world direction and enables camera following. The score is distance multiplied by `1 + cross_distance / forward_distance`, with a positive forward threshold and placement-ID tie breaking. Active-workspace eligibility excludes chrome, imported scene models, Areas, pinned/attached items, pending clients, disabled and minimized targets, including descendants of minimized transient parents. Offscreen eligible items remain navigable.

Mod+Backspace, Operation's `/focus/toggle` and the order-1 **Focus/Unfocus** toolbar button invoke `DesktopAction::ToggleFocus`. Focus chooses a valid selection, then the remembered target, then the nearest eligible item to the camera; no available target disables the button. The toolbar remembers a selected placement before keyboard focus enters its controls. Unfocus clears desktop/native keyboard focus and camera following while preserving the view and tiled/free arrangement. `DesktopAction::Unfocus` remains the one-way internal release.

Ordinary keyboard focus, explicit focus mode and camera following are separate. Clicking a free application does not implicitly enter camera focus: the next Focus action locks onto it in one press. An explicit focus mode remembers its placement independently of Bevy's `InputFocus`, so editing a settings field does not discard the target. Deliberate pan/zoom or spatial flight releases the camera lock while retaining item keyboard focus; explicit Unfocus releases both. Deleted/unmapped targets and removed workspaces clear stale runtime entries, and hidden workspaces pause their camera updates. Changing tiled/free mode releases the explicit camera mode while preserving the normal saved-view transition. Overview and MRU must suspend/restore these same states rather than inventing another camera controller.

Free/floating Mod+Shift+H/J/K/L moves the actual rectangle using `LayoutSettings::move_step`, initially **64 canvas units**, bounded to 1–10000. The width modal edits and saves that step through the same Lingua transaction. A floating move never reorders an unrelated tiled member; tiled moves retain the row semantics above. Directional tiled focus also changes native keyboard ownership when the target changes; a boundary no-op preserves an existing child editor's focus.

Following fits the target into the bar-reserved viewport by changing camera scale/distance, preserving client dimensions. Canvas zoom supports 0.0001–3, including framing a 100000×80000 native rectangle; the zoom control displays sub-percent values rather than rounding them to zero. Following uses final placement, layout offsets and Area scale, with the target and camera resolved from the same frame. Desktop focus of a managed top-level item does not count as editing in `physics::held`; actual child editors, direct editable text and pointer drags keep their existing hold behavior.

The final presentation order is explicit:

```text
input and typed actions
workspace physics and Area effects
desktop and authored layout through ResolveLayout
ResolveFollow and refreshed published 2D geometry
final topology camera, surfaces, assets and world visuals
CameraUpdateSystems
selection / Area overlays and UI preparation
UI layout, world transform propagation and frame extraction
```

`topology::view::synchronize` retains manual input/collision processing in Update. Final camera projection and world visuals synchronize in PostUpdate after `ResolveFollow` and before `CameraUpdateSystems`. UI factories/readouts remain in Update. Selection and Area overlays run after camera updates, before UI preparation. Their projection helpers read current local/ancestor transforms with a bounded hierarchy walk instead of relying on a previous frame's `GlobalTransform`. This also fixes selection outlines trailing a moving target even when the target's pixels are steady. Preserve that ordering when extending rendering; UI Content/Prepare/Layout dependencies must remain acyclic.

`Presentation::frame_geometry` rebuilds final surface mappings for single and multiple outputs and resolves the current model focus before applying its temporary application-click override. A toolbar Unfocus must not be undone by a cached application focus. Park crops its exported canvas image to the usable region and records its output origin; pixel probes must use the exported buffer's actual dimensions, not assume the full monitor dimensions.

Use the same typed actions from native controls and the compositor bridge:

```rust
use crate::{host::{DesktopAction, Direction}, native::actions::Action};

DesktopAction::ToggleFocus.apply(world, root);
DesktopAction::Focus(Direction::Right).apply(world, root);
DesktopAction::Move(Direction::Down).apply(world, root);
```

The preceding 71 layout, 23 physics and 34 shortcut/settings regressions pass (`ergonomics-focus-layout-tests-v4.log`, `ergonomics-focus-physics-tests-v3.log`, `ergonomics-focus-settings-tests-v3.log`), including current-frame physics following, large framing, free/floating movement, keyboard ownership, toolbar target retention and movement-step persistence. The current combined interface suite and optimized Park suite also pass as recorded below. The rendered focus test covers changing-motion native and real Wayland-client targets in both projections, comparing projected corners and actual client pixels while preserving client dimensions, then using the real keyboard chord and pointer control. Its toolbar-placement, client unmap and correctly configured remap checks pass. U11 retains different-rate/spatial-output/hardware and installed-session acceptance rather than repeating those completed basic checks.

**Mod+Y/U/I/O monitor focus** now means left/down/up/right through the existing `FocusOutput` action and `Assignments` path. Directional selection uses monitor rectangle centers in the desktop's logical coordinate space, with `hypot(forward, cross) * (1 + cross / forward)` for positive forward distance and output-ID tie breaking. It is independent of connector enumeration; an edge is a no-op. Window properties/Record inspection remains discoverable and rebindable, with no default chord, freeing Mod+I. Item, row and whole-workspace transfers remain distinct; U7 owns the remaining item/row bindings.

Workspace switching retains its model focus and per-workspace view/follow state. The shared `focus::remember_input` / `restore_input` helpers also retain a live native child editor, using a bounded ancestry check against the current eligible focused placement. A deleted/reparented editor falls back to its valid parent item, and an explicitly unfocused workspace stays unfocused. Both ordinary Sandbox workspace switching and Park output switching reuse this path. It neither resizes clients nor changes focus mode simply to restore keyboard ownership.

Initial monitor attachment now assigns a previously unassigned workspace's home only when that workspace becomes the monitor's active assignment. It no longer writes the first connector's name into every unassigned workspace preference. Explicit saved preferences remain authoritative. The four-output GPU fixture exercises the real registry chords from native editor focus, directional edges, moving followed targets and geometry, bar reservation changes, disconnect/reconnect, retained editor drafts and unchanged workspace/item identities. It passes in `ergonomics-monitors-gpu-debug-v3.log`; all 150 combined layout, output, focus, shortcut, toolbar and workspace regressions pass in `ergonomics-monitors-interface-tests-v2.log`. This is virtual-output/native-content evidence; spatial and real-client output changes, actual scanout and full U9 affinity policy remain separate acceptance work.

The native UI queues `DesktopAction::FocusOutput` through the existing host bridge; Park executes it through the same presentation entry point used by its compositor commands:

```rust
canvas.desktop_action(DesktopAction::FocusOutput(Direction::Right))?;
```

Do not add another monitor selector or dispatch the action a second time after handling the queued request.

The shared overview provides workspace/item thumbnails with current monitor/workspace/focus labels, pointer and keyboard selection, Escape/second-chord cancellation and the ordinary toolbar entry. The completed optimized overview build passes **152 distinct Park tests**: fourteen hardware GPU fixtures and 138 ordinary regressions (`ergonomics-overview-gpu-release-final.log`, `ergonomics-overview-park-release.log`). The same source passes 193 interface regressions and both Park/Sandbox GUI checks (`ergonomics-overview-interface-final.log`, `ergonomics-overview-lince-{park,sandbox}-final-check.log`). Its test executable SHA-256 is `a1e116a33e1038f1909af59969c710ce2efeef2c546dbdcfc639221ff2b63474`, recorded in `ergonomics-overview-release-tests.sha256`.

The GPU fixtures cover the distinct keyboard-selection ring at 640/360 pixels, twenty additional workspaces with wheel scrolling and bounded image/camera cleanup, and an actual inactive Wayland client's pixels, unchanged requested size, callback from the painted snapshot and title/live rename. Showoff retains map, GLB, WGSL, physics, ordinary dragging and persistence in both projections. Release captures of the narrow keyboard selection, real client and complete Showoff overview were inspected under `ergonomics-overview-captures-release`. The shared focus revealer also waits until pointer release before scrolling a pressed control. U11 retains actual compositor keyboard/IME/inhibition/lock and popup handoff, external activation, output changes and installed-artifact acceptance; the optimized test executable is not an installed daily-use candidate.

`view.overview` and `edit.toggle` are registered with localized labels, scopes and non-repeat defaults. **Mod+Space** opens overview, **Mod+P** opens Operation, and **Mod+E** toggles Edit. Native Ctrl+K and Alt+E remain defaults with Super as Mod. Selecting Alt as Mod resolves the implicit Alt+E alias once; explicit overrides still undergo normal conflict validation. These use the same registry, editor, typed actions and compositor input snapshot. Thirty-nine shortcut/controller regressions passed in `ergonomics-overview-interface-tests-v5.log`; the basic native GPU fixture also exercises Mod+E and both pointer/keyboard overview entry. U11 still verifies live compositor input from real clients, inhibition, IME and lock behavior for the final artifact.

#### Overview architecture to preserve during the remaining work

The shared runtime/controller lives in `native/layout/desktop/overview.rs`, with a modal native view beside the existing desktop controls. Keep the canonical workspace and placement identities: opening overview does not create a workspace, replace the desktop model, toggle tiling or move applications. One session belongs to a canvas/current output and records its return workspace, canvas/spatial view, eligible focused placement, native input target and explicit follow state. Reuse the existing focus controller's suspend/restore helpers rather than serializing private runtime entities or inventing a second camera-follow implementation. On cancel, validate every saved identity and restore the surviving state; a moving followed target resumes against its current placement. Physics and application buffers continue updating while the overview owns input.

The native composition is a dismissible overlay with named workspace thumbnails, current monitor/workspace/focus indication and selectable item previews. Selecting an item commits its workspace/output and shared focus action; selecting a workspace restores its valid last focus. Keyboard and pointer use the same selection model. Escape and the second overview chord cancel. Keep the main Edit panel/editor entities intact underneath. The order-3 toolbar control, Operation entry and compositor chord invoke the registered typed action:

```rust
DesktopAction::ToggleOverview.apply(world, root);
```

Build previews from the existing native `Surface` images, application imports and shared world-rendering resources. Use bounded preview rendering requests for visible cards, including cached/refreshable views of inactive workspaces. A preview is a rendering view, never a new `OutputViewport` assignment or advertised Wayland monitor. Its thumbnail extent must not enter the window solver, send a client configure or change a saved camera. Preserve GLB/map/shader content through the existing rendering path; do not replace those workspaces with fabricated window rectangles. Reuse U6's pixel-demand/budget mechanism as it is introduced, cancel stale requests when cards disappear and release preview resources on close. Capture exclusion in U10 must apply to these previews too.

`topology/output_views.rs` already builds per-view backgrounds, render layers, clipped native/application surface instances and holes. Reuse or factor those rendering pieces for preview views, keeping preview identities separate from real `OutputId` assignments. Extend `topology/presentation.rs` surface-demand collection to account for visible preview consumers, taking the maximum required density across consumers so a thumbnail cannot lower a focused window's quality. Imported models/terrain need the corresponding preview render layers, not copies of authored items. Never resize the primary Bevy `Window` to a card's dimensions to obtain a thumbnail. Keep the existing workspace execution/idle policy: showing overview must not globally pause physics or activate every inactive world's simulation solely because its name appears in the chooser.

Extend the existing keyboard ownership/handoff description only as needed for an actual overview context. Binding capture must still quarantine all keys; a generic settings modal must retain its existing isolation. An open overview needs its own toggle/cancel/navigation available while ordinary client input and unrelated desktop commands remain blocked. In particular, clearing every compositor binding whenever `keyboard_modal` is true cannot be the entire overview policy: the second Mod+Space must still close it, including when opened while a real application owns focus. Declare that context's allowed typed actions in the shared registry/ownership path, with the existing tracked-serial handoff and held-key release rules. Do not maintain a separate hardcoded compositor shortcut list. Test close/cancel through the real input bridge, not only by calling the controller directly.

The current implementation uses `topology/previews.rs` for bounded rendering requests, independent `ViewKey::Preview` identities in `topology/output_views.rs`, and `focus::suspend` / `resume` for return state. Preview layers start at **128**, after the existing shader-preview range 64–127; use `RenderLayers::none().with(layer)` for dynamic layers. Limit visible preview targets to eight, each at most 768×432, release offscreen card images and remove preview cameras/instances on close. Surface producers render at order -3, preview backgrounds at -2 and preview worlds at -1 before the main canvas. Preserve the maximum density requested by real output and thumbnail consumers. Imported models, spatial WGSL and render-only terrain use these views without activating an inactive workspace's physics; ground field overrides are restored before publishing physical terrain state. U6 still owns shader source-image quality and the aggregate renderer budget.

The shared `FocusScrollGroup` marks a workspace card so keyboard focus reveals the complete current card when it fits in the list. Keep the existing `FocusScroll` behavior for ordinary fields and fall back to revealing the focused control when a group is taller than its viewport. Window hit rectangles derive from the same scaled, rotated and clipped world polygons used for the preview. Workspace names and live focus/monitor labels update without rebuilding surviving cards. Cancellation restores a valid native item editor or edit-panel field, never a deleted/minimized target. A currently rendered inactive Wayland window is listed in `Frame::previewed`; the compositor advances callbacks from the exact painted snapshot once on the presenting output, without announcing another monitor or configuring the client to thumbnail dimensions. Preserve these now-tested real-client and complete Showoff paths while extending monitor settings and renderer sizing.

Keep the overview's keyboard-selection outline separate from its remembered window-focus border. Its buttons own that outline/geometry through the existing `FixedGeometry` marker, so generic button styling cannot erase the indicator or resize a preview hit target. Application captions use existing name/window-title/head/app-ID metadata, then the native Sand kind, with bounded plain text; refresh caption, tooltip and accessible label on metadata changes without replacing the button. Names are presentation data, never target identity or authority.

The native desktop-key interceptor runs after shortcut capture and before `InputFocusSystems::Dispatch`, because Bevy buttons otherwise consume modified Space as button activation. Respect explicit local binding scopes and use `ActionSequence::run` so practice/suspension rules still apply. Retain a cursor over the original keyboard stream; do not append retained events back into that same stream, since other readers can receive them twice. For an intercepted batch, the implementation dispatches only the unconsumed events through the public cached focused-input dispatcher using a separate persistent message stream, then restores the cleared original resource. The interceptor applies to real canvas roots; panel-only scoped actions keep their own semantics. Generic settings/capture modals still block desktop bindings, while the exact active overview owner retains its registered toggle.

### U5. Implemented graphical monitor editor and output transactions

**Park → Monitors** now provides labelled draggable output rectangles, numeric positions, edge snapping/alignment, enabled state, supported resolution/refresh choices, scale, identification and live usable-area information. Negative drag positions translate the complete enabled layout into the backend's nonnegative range. Changes target the selected output. Nested mode controls only virtual Park heads; Sandbox explains that its host owns physical displays. The ordinary canvas settings remain available in both modes.

The implementation shares the existing output validator and backend staging. It rejects stale catalogues, unsupported modes, all-disabled configurations and invalid pixel/position/scale bounds. Apply performs a backend test before previewing, then exposes Keep/Revert with a compositor-owned 15-second deadline. Timeout, panel loss, lock, failed application and relevant hotplug use the same bounded recovery path. Settings reach machine-local `park.json` only on successful explicit confirmation. U10 still owns rotation/VRR/color extensions; external mirroring/nightlight providers remain separate. U11 and delivery stage 6 retain actual coordinate/provider and physical mode/seat acceptance.

#### Implemented ownership and extension points

The shared types now live in `crates/interface/src/host/display.rs`. `DisplayConfiguration` maps every connected `OutputId` to its supported mode/position/scale settings, or `None` for a disabled head. `DisplaySnapshot` carries the complete catalogue revision, usable viewports, optional nested bounds, current confirmation token/deadline, identification and correlated request result. Requests use the existing `HostRequest::Display` channel and scene snapshots return to the native interface through `runtime::Scene`; UI code does not access DRM or use a private command socket. The actual action contract is:

```rust
enum DisplayAction {
    Test { revision: u64, heads: DisplayConfiguration },
    Preview { revision: u64, heads: DisplayConfiguration },
    Keep { token: u64 },
    Revert { token: u64 },
    Cancel { request: u64 },
    Identify { output: OutputId },
}
```

`server/output_control.rs` now shares the catalogue/mode/settings types and complete-layout validation with trusted native requests. The native transaction controller is `server/output_control/shell.rs`; its private token registry is separate from live Wayland configuration resources. Both paths produce the same backend `OutputRequest`. Native Preview first submits a test, then applies only after backend success. The compositor starts the 15-second monotonic deadline after apply; Keep alone calls the persistence closure. External protocol applies retain their ordinary semantics, while a native transaction blocks conflicting applies. Rollback alone may restore outputs while locked, takes priority over a saturated external-test queue and retries a bounded safe-output fallback after topology changes. The physical backend retains rollback work across an inactive seat and refreshes its backend deadline on resume. Physical seat/mode acceptance remains a later hardware check.

`Commands::save_outputs` atomically merges confirmed connected-output preferences into the existing machine-local `park.json`, retaining command argv, keyboard settings and disconnected output preferences. Invalid files, stale reads or write failures keep the preview available for retry/revert until its deadline. Preview, pointer motion and cancellation never write that file. `Cancel` invalidates pending native tokens, including already-dequeued work; if apply has completed, it uses the same rollback path as Revert/timeout. `Desktop::scene_timeout` keeps pending server-to-interface status changes from sleeping indefinitely without a native redraw.

Atomic replacement is the persistence commit point. `save_outputs` returns an optional warning after a committed replacement if syncing the parent directory fails; Desktop forwards it through the existing native feedback/notification channel while the server accepts Keep and clears its rollback timer. Failures before replacement still return an error and preserve the retry/revert flow. Reporting a post-replacement sync failure as an uncommitted save would leave the new file active while reverting the visible layout. The fault-injected `directory_sync_failure_after_replacement_keeps_the_confirmed_display_and_reports_a_warning` regression passes through the actual Preview/Keep server transaction and verifies the committed file, retained keyboard settings and cleared preview.

Physical preference recovery also preserves the full pre-preview profile map, including disconnected connectors and disabled heads' stored mode/position/scale. `session/display.rs::PreferencePreview` snapshots that map before an accepted native preview and restores it after unconfirmed cancellation/recovery or relevant connector/mode changes. A durable `kept_display_token()` distinguishes successful Keep from later Identify/error responses; observing only the latest response could wrongly restore a confirmed profile. The topology comparison ignores the preview's own position/scale/enabled changes, so a KMS notification caused by applying the preview cannot cancel it immediately. Three policy regressions cover absent-profile restoration, confirmed replacement and topology-change discrimination. They do not claim physical unplug/replug acceptance.

The native draft/view is `native/layout/desktop/monitors.rs`, with normalization, scaled-edge snapping and alignment in `monitors/model.rs`. Fields, mode selection and draft identities survive ordinary status updates. An external catalogue revision marks the draft stale; explicit Refresh rebuilds it from the new catalogue. Input ownership reuses `KeyboardModalOwner`, modal triggers, held-key quarantine and the existing focused-editor return behavior. Closing or losing the overlay cancels pending work/reverts an unconfirmed preview through the typed host bridge. Controller guards serialize requests and reject queued duplicate/mutating actions during a pending transaction in addition to disabling their controls. This also prevents Identify from replacing a result the editor is still awaiting.

The combined interface suite passes **207 tests**, including fourteen monitor/model cases (`ergonomics-displays-interface-final-v3.log`). The completed optimized Park build passes **179 distinct tests**: all fifteen explicitly selected hardware GPU fixtures in **51.92 seconds**, plus 164 ordinary regressions in **1.01 seconds** (`ergonomics-displays-gpu-release.log`, `ergonomics-displays-park-release.log`). This includes the physical preference-policy and committed-save warning corrections. Its test executable SHA-256 is `56b91af8f347f5f53e4b5a97a8b10e8948586a0d62219b5af9019f419e848f33`, recorded in `ergonomics-displays-release-tests.sha256`.

The rendered monitor scenario covers primary/passive identification, changing scale and reverting while identification remains active, left/above pointer arrangement without camera motion, and 640/360-pixel layouts. Release confirmation, narrow-layout and scale-change identification captures were inspected in `ergonomics-displays-captures-release`; the earlier debug identification and normalized-drag captures were inspected too. The full Park GUI compiles with warnings denied after the final save correction (`ergonomics-displays-commit-park-check.log`). Ordinary Sandbox passed its GUI check after the last shared-interface change (`ergonomics-displays-render-target-sandbox-check.log`); the later preference/save changes are Park-only. All eighteen validation-tool regressions pass (`ergonomics-displays-xtask-tests.log`). These checks establish isolated feature/render/source integration. The failing release pacing measurements stay in delivery stage 4; installed/provider and physical acceptance remain in U11 and the delivery stages.

Keep identification rendering separate from the late form update. `EditableTextSystems` belongs to `UiSystems::Content`, and ApplyActions runs after it: scheduling the form after ApplyActions but before Prepare creates a real schedule cycle, caught by the first full rendered fixture. Identification cameras/labels belong before CameraUpdateSystems/Prepare; form validation and labels belong after ApplyActions and before Layout. Use this dependency split when extending the panel:

```rust
app.add_systems(PostUpdate,
    monitors::identify.after(ResolveFollow)
        .after(crate::native::topology::presentation::synchronize)
        .before(CameraUpdateSystems).before(UiSystems::Prepare));
app.add_systems(PostUpdate,
    monitors::update.after(ApplyActions).before(UiSystems::Layout));
```

Identification consumes the passive output `Targets` map, updated inside native `topology::presentation::synchronize`, so schedule it after that producer and before camera preparation. Mode/scale changes replace output images; rebinding an identification camera from the previous map could retain a removed render target. Exercise a scale change and rollback while the identifier is still active, as well as focused/nonfocused labels and expiry. The physical render loop also drains existing prepared/inflight frames while configuration work is queued and suppresses new rendering until that work finishes, so continuous video cannot keep the backend perpetually busy. The real-seat/video test still has to verify this behavior on the user's driver.

The rendered `presentation::tests::displays` fixture is registered in validation preparation as `monitor-settings-rendering`. It exercises the actual native controls and shared virtual backend, pointer arrangement, supported resolution/refresh selection, Apply/Revert/Keep persistence, the named outputs' identification pixels and narrow layouts. Once outputs own their render targets, `Presentation::resize` rejects raw resizing; callers must use output configuration so all cameras receive the replacement targets together. The fixture verifies that rejection and then resizes through the real virtual-output path. A virtual-head pass cannot establish physical DRM mode or refresh-rate behavior.

### U6. Fix WGSL image quality through the shared renderer

Native output images now use physical pixel extents through `presentation/outputs.rs`; the previous 1024×576 image for a 1280×720 mode at 1.25 scale is no longer the allocation contract. `host::raster::RasterMapping` carries the declared nominal scale and image origin, while logical layout, work areas, application configure sizes and `Frame::origin` remain logical. Primary/passive cameras, backgrounds, identification, spatial projected corners and native input share that mapping. `ImageRenderTarget::scale_factor` and the synthetic Window scale preserve Bevy's logical layout while rendering at the actual raster extent.

Physical composition now allocates a cached mode-sized texture per `Screen` instead of downsampling through a combined logical atlas. `Composition::render_output` uses the same output painter as capture and the nested path, applying the mapping to native content, client buffers, bars, popups, cursor/drag overlays and locking. The DRM texture element normalizes Smithay's rounded geometry to the exact mode without an intermediate resampled image. Existing GPU fences, retained imports and prepared/inflight frame lifetimes remain intact. This implements the rendering path; actual KMS/driver presentation remains stage 6 acceptance.

The extended hardware fixture passes all five scales, including 400%, after the clipping correction (`ergonomics-raster-gpu-debug-v12.log`, 24.85 seconds). It verifies real popup pixels and logical hit testing beyond the parent rectangle, plus black protection on both mode-sized outputs after overwriting a visible previous color. Its 125% and 400% popup captures were inspected under `ergonomics-raster-captures-debug`. This is an isolated EGL composition pass, not actual KMS or installed-session acceptance. The earlier full debug suite passes 16 hardware GPU fixtures and 165 ordinary regressions (`ergonomics-raster-gpu-debug-all-v9.log`, `ergonomics-raster-ordinary-debug-final.log`). Those cover 100/125/150/200%, odd modes, nonzero monitor origin, bar cropping, primary/passive switching, exact final native pixels, unchanged real-client dimensions and native editor/caret/drop input. Preserve the 400% edge-clipping failure in `ergonomics-raster-gpu-debug-v10.log` as the regression's original evidence.

All 219 selected shared-interface regressions pass on the updated nominal-scale contract (`ergonomics-raster-interface-tests-v2.log`). The initial raster unit fixture incorrectly expected a bit-exact result from an inferred floating-point ratio; that unused inference helper is removed, and tests now use the declared output scale. Both Park and ordinary Sandbox GUI pass `cargo check` with warnings denied (`ergonomics-raster-lince-{park,sandbox}-check.log`). All eighteen validation-tool regressions pass after adding `output-raster-rendering` and the shared raster/surface-planner tests to automated preparation (`ergonomics-raster-xtask-tests.log`). These results do not establish the optimized artifact's correctness or pacing.

The completed optimized raster baseline passes **181 distinct Park tests**: all sixteen hardware GPU fixtures in **53.99 seconds** and 165 ordinary regressions in **1.08 seconds** (`ergonomics-raster-gpu-release.log`, `ergonomics-raster-park-release.log`). This includes the explicit scaled-cursor pixel assertions added after debug v12, the five-scale real popup/lock fixture and all prior native controls, monitor, overview, camera-follow, import and synchronization fixtures. The executable SHA-256 is `e387d87cb72a1c72010bc8eff9ac576cb85c6d0b884b5fe0c1ce989dd9eb0ebf`, recorded in `ergonomics-raster-release-tests.sha256`. Its 125% and 400% popup captures were inspected in `ergonomics-raster-captures-release`. This baseline precedes the independent frozen-frame change below; keep its new checks separate rather than claiming they ran in that executable. Actual KMS, provider/installed-session acceptance and pacing remain in U11 and delivery stages 4–6.

Frozen repaint state is now independent of the repaint counter. `shader/snapshot.rs` retains the selected working program, complete shader inputs and whether its last frame passed the existing three-paint warmup. `EffectPreview::complete_frame` records that state during rendering, so freezing and resetting `painted` in the same update still captures the last ready frame. A repaint changes only raster dimensions and the shader-pixel pointer derived from its frozen normalized position. It retains time, quantities, numeric presence flags and program selection, leaves generated geometry buffers intact, then idles again after the repaint guard. Unfreeze resumes live values. The existing Frozen control and status label remain the UI; no second freeze mode or additional toolbar is introduced.

All **19 shader regressions**, including the real GPU surface, mesh, generated-vertex, lighting/prepass and volume paths, pass (`ergonomics-effect-snapshot-shader-suite.log`, 26.73 seconds). The GPU fixture now resets repaint progress in the same update as Freeze and changes live quantities, verifying retained source pixels and generated vertices before unfreezing. The controller regression supplies a different computed display extent, verifies retained uniforms/program and confirms that the camera returns to idle. Three focused snapshot tests cover missing initial frames, normalized pointer scaling and the simultaneous freeze/reset boundary. Both Park and ordinary Sandbox pass their updated GUI checks with warnings denied, and all eighteen validation-tool tests pass (`ergonomics-effect-snapshot-{park,sandbox}-check.log`, `ergonomics-effect-snapshot-xtask-tests.log`). Automated preparation includes the five CPU/controller checks through `effect-snapshot-policy`. This establishes the independent replay mechanism; the later allocation implementation and its distinct evidence follow.

Shader source targets now start at 1×1 and resize to the maximum demand of their visible consumers through `description/shader/extent.rs`. The same image handle retains `MAIN_WORLD | RENDER_WORLD` metadata, the source camera/render node and `ShaderInputs::size` use the granted physical extent, and the normalized shader pointer is converted once. Resizing resets repaint progress while retaining the frozen snapshot and working program. It does not recompile WGSL or change authored geometry/application configure sizes. Final descendant UI demand is measured after layout/transform propagation and applied in the following pre-camera preparation pass. Lit material overlays and record shader spheres declare their containing surface through `SurfaceConsumer`, using its current unconstrained requested density rather than feeding its reduced allocation back as new demand.

All **53 targeted renderer regressions** pass with this allocation/consumer implementation (`target/park-validation/ergonomics-effect-consumers-interface-v1.log`, 27.83 seconds). The hardware source probe verifies sharp alternating one-pixel rows, unchanged image/program identity, same-frame camera raster metadata and actual image sizes across 640×360 at 100%, 800×450 at 125%, 576×324 at 150% during a frozen repaint, and 768×432 at 200% after resume. Hidden sources shrink to 1×1 and stop their cameras; deleting a source releases its image/camera. CPU checks cover aspect retention, stale consumers, two material consumers sharing one source, and restoring intended density after visibility/scale changes. The existing full mesh/generated-geometry/volume/lighting GPU paths also pass in that suite. These are source-renderer checks; final Park tiling/output pixels, optimized measurements and layer isolation remain below.

The allocation/consumer source passes both GUI checks with warnings denied (`ergonomics-effect-consumers-{park,sandbox}-check.log`) and compilation of Park's session/test targets (`ergonomics-effect-consumers-tests-check.log`). All **127 shared layout/output/raster regressions** pass in `ergonomics-effect-consumers-layout-tests.log`, and all **eighteen validation-tool regressions** pass in `ergonomics-effect-consumers-xtask-all-tests.log`. These test sets overlap the allocator checks above; do not add their counts as distinct tests.

The optimized allocation baseline passes all **23 shader tests** in 10.22 seconds (`ergonomics-effect-consumers-shader-release.log`). Its full Park run records **180 passes and one failure** in 47.64 seconds (`ergonomics-effect-consumers-park-release.log`): the monitor fixture's first Tab assertion found a different entity from its caption lookup. The dialog has both diagram and selector buttons named Park-2; the fixture now targets `monitor.diagram.Park-2` explicitly, with verification pending below. Preserve the failed run. The executable checksums are `a64fc0c1c26f87e31a8499d54893fe733a86eaf9234584c2e5c2bb2b0c7526ed` for the interface tests and `84834785adcefeea55ba2c0d099d0057e191ef4e053a286d7a8a471c917ce44f` for Park, recorded in `ergonomics-effect-consumers-release-tests.sha256`. These precede the resource observer/scheduler changes; they do not verify that later code.

The original combined command's broad `session::` filter also selected unrelated Fiote voice/provider tests, including ignored integration fixtures, and failed before Park ran. That stopped run remains in `ergonomics-effect-consumers-release.log`; it is not a renderer pass or evidence that the separate voice work was validated. Use the fully scoped `native::description::shader::` filter for the interface suite and the Park package's own test executable/suite for compositor coverage. The corrected exact runs above avoid the ambiguous cross-package filter.

With no compiler running, the optimized Park run also collected 120 uncapped Showoff frames per projection: **50.91 FPS flat** (19.64 ms mean, 21.75 ms p95) and **44.62 FPS spatial** (22.41 ms mean, 26.55 ms p95). The spatial sample still fails the 45-FPS stress floor. Evidence is `ergonomics-effect-consumers-captures-release/showoff-{flat,spatial}-timing.json`, scoped to native update, GPU rendering and fence completion; it does not include physical scanout or real video. Retain those failures and the separate refresh-rate/quiet installed-workload gates.

The resource observer, idle-attachment cleanup, Park settings readout, native-capture reduction status and focus/fairness scheduler are implemented through `surface_budget/{residency,ui}.rs` and `shader/scheduling.rs`. Park's session/test target check passes with warnings denied (`ergonomics-effect-resources-check-v1.log`, 51.26 seconds). All **65 targeted resource/renderer regressions** pass in `ergonomics-effect-resources-interface-v2.log` (25.99 seconds), including actual GPU cleanup to the one-pixel hidden placeholder and then zero after producer/consumer release, stalled-retirement accounting, cache reuse/expiry, bounded tracking, focused service, contention fairness, failed-candidate slot release and settings identity/keyboard ownership. The first suite's 64 passes/one failure remain in `ergonomics-effect-resources-interface-v1.log`: its zero-resource check retained the fixture's own image handle. Releasing that handle fixed the fixture without weakening the assertion. The v1 monitor command did not run after that failure. The corrected rendered monitor fixture passes in `ergonomics-effect-resources-monitors-v2.log` (46.71 seconds), through arrange/test/apply/revert/identify and narrow layouts, confirming that its former first-Tab failure was the ambiguous duplicate-caption lookup. These targeted checks do not close broader integration or optimized pacing acceptance.

The tracker stores texture IDs/metadata/completion flags, and camera markers carry asset IDs, avoiding additional image handles that could prolong the resources being measured. Preserve Bevy's distinction between removing the main-world asset and dropping its last handle: `AssetEvent::Removed` alone does not evict its render asset; `AssetEvent::Unused` does. External image consumers may legitimately keep the rendered asset alive after its producer closes, so report it as a retained image rather than forcing it out of the renderer or falsely reporting zero.

The current resource implementation also passes both GUI checks with warnings denied (`ergonomics-effect-resources-{park,sandbox}-check.log`, 51.28/44.85 seconds) and the complete **181-test Park debug suite**, including all sixteen hardware GPU fixtures (`ergonomics-effect-resources-park-tests.log`, 312.40 seconds). That suite covers the corrected monitor dialog, shortcuts/widths, overview, Showoff, native/real-client focus, popup/output composition, five-scale raster/input/lock protection, synchronization and ordinary protocol/lifecycle behavior. Its captures are in `ergonomics-effect-resources-captures-debug`; the 360-pixel monitor view was inspected for readable controls, warning text and reachable footer. Debug Showoff timing files are diagnostic artifacts, not optimized pacing acceptance. The earlier optimized baseline's 180/181 result and 44.62-FPS spatial stress miss remain recorded above; neither is replaced by this debug pass. Rebuild/retest the optimized resource artifact after the remaining renderer integration work.

The integrated native pressure fixture now passes two complete resize cycles with sixteen real text Sands, a live shader and its separate inspected editor. The editor remains fully granted while the Sand capture is reduced; requested source size and image identity remain stable, repeated sizes recover identical grants, closing surplus Sands restores full quality and closing all producers/consumers reaches zero tracked textures. Park settings and the actual inspector report the reduction and its recovery. The corrected source passes **83 focused debug regressions** (`ergonomics-pressure-renderers-v7.log`, 51.86 seconds), covering shader surface/mesh/volume paths, native shader integration, allocation/residency, Fiote behavior, range isolation and overview terrain. Captures and per-stage measurements are in `ergonomics-pressure-captures-v7`. Its observed peak was **663429948 bytes** (632.7 MiB), below this fixture's explicit 1-GiB texture ceiling; current grants stayed below 256 MiB. This is a bounded stress-fixture observation, not a new global VRAM cap or frame-pacing pass.

The stronger fixture initially exposed real sizing feedback: a background used the full shader-preview widget, whose changing status text participated in flex layout and changed the image's aspect/requested extent. The source repeatedly reset its repaint guard, with retained/pending generations never settling. `shader::background_output` now hides that label from background layout; source status remains in the shader editor and Park settings. Background factories must use this helper rather than embedding a normal preview with its diagnostic label. Preserve the failed evidence in `ergonomics-pressure-{v4,v4-diagnose}.log` and `ergonomics-pressure-policy-v{5,6}.log`; nonblocking GPU polling alone did not fix the feedback. The fixture now fails immediately on a renderer error instead of silently waiting after rendering stops.

`native/render_layers.rs` now owns the named ranges below. Passive views, world/preview lights, shader slots, overview terrain and Fiote cameras/lights/scene descendants use those definitions and checked access. Distinct visible Fiote palettes are bounded to sixteen; excess palettes use the existing static portrait and the existing appearance settings explain the capacity limit. The actual two-output GPU fixture passes with seventeen palettes, an ordinary GLB and a WGSL mesh (`ergonomics-layer-rendering-v2.log`, 18.43 seconds): intended animated/static images contain rendered pixels, passive output pixels remain byte-identical, and destruction releases the character cameras and image render assets. Its rendered capacity panel was inspected in `ergonomics-layer-captures-v2`. The first fixture's blank-image failure was an invalid RGBA-preview/BGRA-export readback; the corrected fixture reads the original RGBA texture directly. Do not change character rendering to compensate for that fixture error.

The optimized interface renderer run now passes **83 selected tests** in **13.65 seconds** (`ergonomics-renderer-integration-release-interface.log`), including the final pin/unpin check for cleared `NativeRaster` diagnostics and the unchanged live shader image. Its pressure fixture records a **776672632-byte** observed texture peak below its explicit 1-GiB fixture ceiling; current grants remain below 256 MiB, all retained/pending/cache estimates settle between resizes, recovery restores full quality and closing all owners reaches zero tracked textures. Measurements/captures are in `ergonomics-renderer-integration-release-interface`. The separate optimized playback/transition regression also passes (`ergonomics-renderer-integration-release-playback.log`). These results precede the new stacking correction below and do not establish final Park output or frame pacing.

- [ ] Finish release pacing and final-artifact integration after the remaining feature changes. The full optimized v11 checkpoint, including command discovery and native keymap controls, is complete under U7; do not treat its earlier compilation failures as still-unrun tests. The newer transfer/UI changes need their own final optimized acceptance. Keep the preserved spatial stress-floor misses, quiet browser-video matrix, actual-client/output and installed-workflow gates open under U11 and delivery stages 4–6; passing correctness tests does not establish those pacing results.

The corrected optimized renderer baseline passes all **98 selected interface tests** in **13.63 seconds** (`ergonomics-renderer-integration-release-interface-v2.log`), covering shader surface/mesh/volume and frozen state, hidden character playback, layer ownership, target allocation/residency, native capture fingerprints, input/placement and terrain. This executable began compiling before the U1/U2 discovery changes, so it proves the corrected renderer baseline without claiming those later command tests ran. The pre-discovery renderer/runner source subset is recorded in `ergonomics-renderer-integration-v2-source.sha256`; later command/runner changes are distinct from that snapshot.

The preceding renderer-source Park and ordinary Sandbox GUI checks pass with warnings denied in **59.63/53.88 seconds**, and xtask checking passes in **2.28 seconds** (`ergonomics-renderer-integration-{park,sandbox,xtask}-check-v2.log`). All **10 selected validation-tool regressions** pass in **0.07 seconds** (`ergonomics-renderer-integration-xtask-tests-v2.log`); this uses the scoped `park::validation::tests::` filter and does not claim the broader eighteen-test historical tool suite was rerun. Those runner checks precede the new command-discovery stage. These source/runner results and the passing 51-case debug fixture do not establish optimized pacing or installation.

The hidden-preview lifecycle now passes its actual GPU fixture in `ergonomics-stacking-gpu-debug-v1.log`: sixteen hidden character targets become prepared 1×1 images, their render-world color/depth/prepass attachments disappear, playback remains unchanged, showing restores the same cameras/images, density changes resize correctly and destruction releases the final resources. The mixed-scene fixture also retains its passive-output/GLB/WGSL isolation and visible capacity-warning checks. The separate optimized playback/transition policy pass is recorded above. This completes that focused lifecycle work; the same combined GPU run still fails the shader-reference and stacking cases below, so it is not a passing integration suite. Preparation retains `preview-lifetime-policy` and `preview-layer-rendering` for future regressions.
The expanded **51-case native shader output fixture passes with zero sampled pixel mismatches** (`ergonomics-shader-output-debug-v5.log`, 132.68 seconds): surface/mesh/volume producers retain their source identity and camera extents across four output scales, free/tiled/maximized/overview-return transitions and three passive-output cases. `ergonomics-shader-output-debug-v5/shader-output-metrics.json` records all dimensions, mappings, sample counts and output identities. The preceding mixed-scene test proves ordinary imported-GLB presence and separation from preview cameras; arbitrary world models remain outside window tiling. This completes the focused shared-renderer pixel/extent work, while optimized integration/pacing and real-client/physical acceptance remain in the open item above and U11. Preserve the completed front/back, native/published pointer-order, passive-view and authored-elevation regressions when extending this path.

Use one named render-layer ownership module for the collision correction, preserving the existing world/projection/UI layers below 32. Allocate the following bounded ranges, with checked slot access rather than unchecked base-plus-index arithmetic:

| Range, end exclusive | Owner and exhaustion policy |
| --- | --- |
| 32..48 | Up to sixteen real output view slots, matching the existing output-catalog limit |
| 48..64 | Up to sixteen distinct visible Fiote character palettes; excess palettes keep the existing static portrait and report the preview limit in character settings |
| 64..128 | Existing sixty-four shader mesh/volume preview slots; retain their current admission/error policy |
| 128..136 | Existing eight visible overview preview slots; retain their current release-on-close/offscreen policy |

All camera, scene-descendant and light assignments must use the same ownership definitions, including the main world light's real-output/overview membership and the shader preview lights. Construct dynamic masks with `RenderLayers::none().with(layer)` or `from_layers`; Bevy's const `layer` constructor cannot represent every dynamic index. A disjoint-range unit test is necessary but insufficient: the GPU fixture must load the animated Fiote GLB and ordinary GLB/WGSL content while a passive output occupies the former colliding layer, and check both intended preview pixels and the absence of leaked geometry on the output. Hiding/destroying previews must release their cameras/images and leave output layers unchanged.

Use the implemented slot API when extending a view family, and use its `len` for admission rather than duplicating the limit:

```rust
if let Some(layer) = crate::native::render_layers::OUTPUTS.get(slot) {
    world.entity_mut(camera).insert(RenderLayers::none().with(layer));
}
```

An out-of-range slot is rejected/skipped by its owner; never wrap it into another family's range. Updating an existing Fiote palette slot updates every descendant as well as its camera and lights. Overflow portraits reuse the normal static-image path and appearance controls.

#### Hidden targets and playback ownership

Use the same release contract as shader sources: retain CPU scene/playback ownership, a stable image handle and a one-pixel published placeholder; retire the larger published backing, color/depth/prepass attachments and their in-flight generations through the shared renderer. Destruction also releases that final image asset. Keeping an inactive camera entity is not evidence that its GPU attachments remain allocated, and removing an entity is not evidence that GPU generations have retired. The GPU fixture inspects both render-world attachments and prepared image sizes.

The residency/target-refresh plugin is installed once by either the budget coordinator or the character plugin. Character-only image renderers can use it without introducing a canvas, workspace or the shader allocation coordinator. Target metadata refresh runs before camera preparation and after any native allocation pass; image metadata stays in both worlds so unchanged handles can resize correctly. Only mutate an image when its dimensions change: requesting mutable asset access every update would emit modification events and repeatedly prepare GPU textures. The observed offscreen-texture readout now includes character targets; the 256-MiB grant quota still governs the native/shader allocation families, while character admission remains separately bounded to sixteen targets of at most 256×256 pixels.

```rust
Target { image: image.id(), release: !visible }
```

Set `release` from visibility, not the animation frame throttle. A visible paused or throttled scene keeps its current image/attachments; a hidden scene releases the larger allocations and pauses both `AnimationPlayer` and its `AnimationTransitions`. Bevy advances cross-fade weights independently of paused clip playback, so retain/remove the transition component during the pause and restore it on return. Keep current scene entities and image identities when possible. If palette pressure evicts a scene, retain only its behavior and cloned playback/transition values, restore them against the same process-local `Activity::ALL` graph construction, and prune that metadata when no avatar owns the palette. That cache must not retain GPU/image handles or become a second persistent store. Focus loss alone pauses a still-visible scene without shrinking its image; logical hiding, window hiding or occlusion uses the release path.

#### Final output reference matrix

`presentation/tests/shaders.rs` adds a native Park reference fixture for surface WGSL, a mesh shader and a volume shader at 100%, 125%, 150% and 200%. Each scale exercises free placement, tiling, explicit maximize and return from overview through the ordinary typed actions: 48 active-output comparisons. Three additional cases compare those producer types on a passive output while another workspace owns focus, selecting pixels and geometry by the item's output identity. Verify the actual arrangement/maximize state, stable source image identity and matching camera/image dimensions. Record source size, native-capture size, projected pixel bounds, output size/identity, nominal scale and mismatch samples beside each capture in `shader-output-metrics.json`.

Compare Park's exported DMA-BUF pixels with the original RGBA shader target, using the published geometry that also owns input mapping. For example:

```rust
let logical = output.raster.logical_point([pixel_x + 0.5, pixel_y + 0.5]);
let surface = item.surface_point(logical)?;
```

Normalize that surface point through `item.mapping.surface` to sample the producer image. Require enough producer pixels for the projected output footprint, then compare locally uniform interiors to avoid treating intentional edge filtering as corruption. The surface reference also requires retained black/white producer contrast; a fixed 256-pixel producer must fail the extent check even if its enlarged pixels resemble the reference. Every mode captures the disabled background and compares the producer's actual straight-alpha composition in linear color. Surface backgrounds intentionally cap strength at 0.65 in the existing shader-effects path, so requiring opaque source pixels would discard every valid surface sample rather than test its output. Keep the same minimum sample count and pixel-mismatch threshold. These are final native Park output checks for the shared shader producer types; retain separate real-client/protocol, physical presentation and main-world spatial acceptance. The `effect-output-rendering` preparation stage runs this release fixture. Its implementation is not a pass until the matrix actually completes.

#### Coplanar placement order

The existing Bring to front/Send to back actions persist `ZIndex`, but the captured faces previously used only transparent depth sorting and exported geometry used identity as its final tie-breaker. `topology/stacking.rs` now carries floating state, the existing Z index and sibling order to captured faces and passive/overview instances. Flat cameras reuse Bevy's normal transparent sort, then reorder only captured items tied at the same finite depth, preserving other phase entries' positions. This changes neither depth values nor authored spatial elevation and leaves perspective rendering on its normal depth path. Native ray ties and published flat geometry use that same order; no second persistent stacking model is introduced.

```rust
super::stacking::synchronize(world, entity, face);
super::stacking::camera(world, camera, !spatial);
```

`presentation/tests/stacking.rs` now passes its full GPU interaction in `ergonomics-stacking-gpu-debug-v2.log`: final exported pixels, native pointer ownership and the published hit target agree before/after front/back actions, on a passive output and after returning to primary. Its perspective case verifies authored elevation takes precedence. Captures are in `ergonomics-stacking-captures-debug-v2`; preparation registers `item-stacking-rendering`. The initial failed run remains in v1: Bevy's GPU-driven material phase supplies `Entity::PLACEHOLDER` as the render entity and the real identity through `MainEntity`. Resolve extracted order metadata by that main identity, retaining the normal transparent phase and the untouched spatial geometry.

The shader-reference test failed in both v1 and v2, separately from that passing stacking fixture. Its v2 diagnostic records 60000 mapped points, 59004 interior points and 15912 uniform neighborhoods, but source alpha is consistently 166/255, matching the intentional surface-strength cap; its opaque-only filter accepted zero samples. The corrected alpha-aware v3 reference then exposes a real compositing defect: all 7959 sampled white neighborhoods are too dark, with expected values around 211 and observed values around 175. Preserve those failures rather than changing production opacity or loosening the reference threshold.

#### Premultiplied output composition

Captured UI and the transparent 3D scene already carry premultiplied color. The final main/passive/overview camera write previously used straight-alpha blending, multiplying that color by alpha a second time. Both `native/app.rs` and `topology/output_views.rs` now use the matching final-write contract:

```rust
CameraOutputMode::Write {
    blend_state: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
    clear_color: ClearColorConfig::None,
}
```

Keep the existing UI capture, `AlphaMode::Premultiplied` surface material, source opacity settings and texture lifetimes. Do not unpremultiply captures, force opacity or add an extra fullscreen conversion pass to compensate at another stage. The source reference is straight-alpha RGBA; its alpha is applied once when calculating the expected final blend over the disabled-background capture. The full **48-case active-output matrix passes with zero sampled pixel mismatches** after this correction (`ergonomics-shader-output-debug-v4.log`, 131.76 seconds for that fixture and the passing stacking interaction together). The subsequent v5 fixture also passes all three added passive cases, completing 51 comparisons as recorded above. Captures and per-case extents/counts are retained in each run's `shader-output-metrics.json`. These are correctness results in debug mode, not a frame-rate claim; optimized integration/pacing remain unfinished.

All **11 input/placement/capture policy regressions** also pass (`ergonomics-stacking-input-policy-v4.log`, 0.34 seconds): front/back respects workspace/screen layers, pinning retains its scale, ordinary Sand/Area/clock dragging and wheel ownership/cancellation remain valid, and capture fingerprints track text, clipping and image changes. The inspected v4 125% tiled checker, 200% volume and v2 perspective-stacking captures are readable and preserve their intended geometry; these visual observations supplement the measured sampled-pixel/extent assertions.

#### U6 implementation sequence and sizing contract

Preserve the implemented output raster contract while extending effect producers. Carry each output's declared scale from the server through the host snapshot rather than reconstructing it from rounded logical dimensions. The usable logical rectangle, cropped physical raster extent and nominal scale are separate values; round shared crop boundaries consistently, clamp them to the mode, and retain the full mode size for an uncropped output. `OutputViewport::physical_region` resolves actual mode edges even when nominal scale multiplied by rounded logical size differs by a pixel. Keep the original destination rectangle when clipping a sampled image: shrinking the destination while retaining the whole source resamples the image and shifts every pixel.

For example, an 800×600 mode at 1.25 scale with a 33-logical-unit top reservation exports 800×559 pixels beginning at physical y=41. Its image origin is logical y=32.8 even though `Frame::origin` records the logical reservation of 33. The local view's raster origin is therefore −0.2 on that axis. Do not round away that offset, reconstruct scale as 559/447, or resize an application to compensate. Active/passive camera and grid insets account for this difference with the ordinary window-size projection rather than stretching an orthographic projection across rounded extents.

Use the actual mapping API when deriving a cropped target:

```rust
use crate::host::raster::RasterMapping;

let (pixel_origin, pixels) = viewport.physical_region(region)?;
let raster = RasterMapping {
    origin: viewport.raster.logical_point(pixel_origin.map(f64::from)),
    scale: viewport.raster.scale,
};
let local_origin = std::array::from_fn::<_, 2, _>(|axis| {
    raster.origin[axis] - region.origin()[axis]
});
```

`pixels` sizes the image; `region` remains its logical work area; `local_origin` belongs to its local host view. Pointer positions, control rectangles, spatial corners, IME context and editor drop coordinates use the same transform in the appropriate direction. The editor mapping now includes the computed content origin and actual text scroll. A queued text drop validates that scroll/content origin before applying its saved coordinates. A horizontally scrolled Parley IME context can report inverted bounds; include the actual caret and ensure positive bounds before mapping instead of sorting it into a huge negative exclusion rectangle. These corrections are generic native-editor behavior.

Real application captures now request their exact physical density through the shared surface planner, avoiding the previous fractional-scale bucket followed by a second resampling. The weighted device-aware allocator below governs current source/depth/native target grants. Retained/in-flight accounting, scheduling and reduction diagnostics have their implementation and focused evidence above; integrated pressure/recovery, final presentation and optimized measurements remain unfinished.

Finish the remaining renderer work in this order:

1. Complete optimized integration on the corrected stacking/output-blend source. Preserve the passed pin/unpin cleanup and hidden-preview lifecycle while rerunning the appropriate suite; neither needs another implementation. Retain the shared preparation path, stable intended demand, renderer error reporting and the separate 64-source/eight-active-camera limits.
2. Run the completed 51-case Park pixel matrix against the optimized artifact, preserving its dimension/pixel thresholds and the completed overlap/stacking interaction. The standalone source test and native pressure fixture do not substitute for those compositor transitions. No additional pixel-matrix implementation remains after the passing v5 fixture.
3. Measure optimized frame times and extra GPU/memory use with no compiler running. Keep the existing refresh-rate acceptance and quiet/video matrix; sharper output or a stress workload above 45 FPS alone does not establish smoothness.

The remaining producer integration must respect Bevy's ordering: final descendant UI geometry is available after `UiSystems::PostLayout`, while camera target/projection updates precede UI layout. Collect measured demand after layout and apply a pending extent before the next camera update/UI preparation; do not resize a camera after its update or rerun every camera system to hide that dependency. Refresh offscreen shader uniforms from the granted source extent in that same preparation pass. Keep normal application configure sizes and logical layout independent of it. Shader `size` describes the granted source raster; derive shader-pixel pointer coordinates once from the normalized pointer, preserving the input system's logical coordinates.

Make the shared budget coordinator usable without a native canvas, workspace or primary Window. The existing standalone shader GPU fixtures install the shader renderer with image-backed cameras and no primary Window; those are valid consumers too. Native surface request collection may return nothing when topology/window resources are absent, while effect requests still receive grants and prepare their targets. One ordered allocation pass combines the available request families; an early return from `topology::presentation::synchronize` must not silently disable source sizing for standalone previews. Preserve the existing render-camera order and schedule source preparation before camera/UI preparation, with final measured descendant demand supplied for the following update.

For a shader embedded in a captured Sand, its measured node size already includes the capture camera's granted scale. Retain the containing Sand's unconstrained requested density separately and normalize through the actual capture density when collecting producer demand. Otherwise a budget reduction becomes the next frame's apparent demand, which can cause quality/budget oscillation. The existing surface request already takes the maximum across focused/passive outputs and visible overview consumers; preserve that maximum, and do not count the same shared source image once per output. Independently consumed material/projection images must publish their own stable consumer requests and stop contributing when hidden or destroyed.

The implemented allocator in `surface_budget/allocation.rs` budgets current native captures at eight bytes per pixel (published image plus `sand_ui_staging`), surface sources at twelve (published plus ping-pong color), mesh sources at sixteen (including depth), and volume sources at twenty (including prepass). It uses a shared 256-MiB current-target budget and the actual `RenderDevice::limits().max_texture_dimension_2d`, with a 4096 fallback for renderer-free tests. Priority groups reduce background/preview demand before focused content; arithmetic is saturating and even insufficient minimum-size budgets terminate with bounded grants. The preceding surface-planner tests now exercise this allocator instead of retaining a second test-only legacy algorithm. The implemented residency observer separately accounts for retained/in-flight generations within the tracked target families; their integrated lifetime/peak measurements remain necessary. Resources outside those families remain outside this estimate. Existing source labels display requested/granted extents and the reduction reason; the shared `Diagnostics` resource reports granted bytes, reduced target count and device cap.

The implemented observer uses the existing renderer's texture cache and WGPU completion callbacks. Producer cameras carry their image asset ID and whether their auxiliary targets can be released. A temporarily unscheduled visible source may retain attachments; hidden or fully frozen sources release unused color/depth/prepass components, and 2D producers release stale 3D depth/prepass attachments. Bevy 0.19.1 removes extracted camera/view state on deactivation but leaves these attachment components referenced, so switching `Camera::is_active` off alone is insufficient cleanup. Preserve the published frozen image and recreate auxiliary targets when repainting resumes.

Observe actual texture IDs and format/extent/mip/sample metadata after the render submission, counting shared IDs once. Distinguish current producer textures, old images still retained for display/render assets, GPU retirements awaiting `RenderQueue::on_submitted_work_done`, and the existing cache's conservative three-frame retention estimate. Track metadata/completion flags only; a diagnostics observer must not extend a texture's lifetime by storing its handles. Bound tracker storage and disclose a partial measurement if that bound or an unknown format prevents full accounting. Include 1×1 placeholders, native staging and source color/depth/prepass attachments; output buffers, imported model resources and unrelated GPU allocations remain outside this producer-texture estimate. Report the current-target grant budget separately from observed current/retained/peak estimates rather than presenting it as a total VRAM cap.

The rendering readout is in the Park edit tab, with requested/granted pixels and reduction reasons for the focused native capture. The shader editor also reports the inspected Sand's enclosing capture reduction, independently of its own preview grant. Readouts refresh without rebuilding the panel or replacing its focused fields. The scheduler uses the allocator's focused priority, keeps the eight-active-camera limit, prioritizes changed inputs over ordinary idle animations and ages waiting sources into service. Failed candidates without a working program release their slots. Pure contention, actual controller and GPU cleanup checks pass as recorded above; rendered pressure/recovery and optimized integration remain outstanding.

Use the implemented producer/consumer API below inside the interface renderer. Explicit physical consumers refresh their demand each frame while visible and release it when hidden; the collector also expires missing/destroyed identities. A material attached to an existing native surface declares that surface so the allocator can resolve its current output/overview density. The collector belongs to the shader/rendering boundary; layout code does not resize shader assets itself.

```rust
shader::extent::request_extent(world, preview, consumer, physical_pixels);
shader::extent::release_extent(world, preview, consumer);
world.entity_mut(material_entity).insert(shader::extent::SurfaceConsumer {
    image: source_image.clone(),
    surface: sand_entity,
});
```

Bevy's camera update does not observe a scale-only change to the same `ImageRenderTarget`. `BudgetPlugin` refreshes projection state for inserted/changed targets and image-size/scale mismatches before `CameraUpdateSystems`; reuse that general binding instead of emitting fake image changes or adding a Park-only camera fix. The GPU probe asserts that an active source camera's cached physical extent equals its image metadata after every update, including resize frames, rather than waiting for a later settled frame. Preserve the ordering through future target changes and measure descendant UI demand only after layout.

### U7. Apply the accepted Niri-derived ergonomics through general settings

The user accepted the preceding proposals, subject to the explicit corrections above. Implement these as generic capabilities with this machine's chosen profile; do not install a particular bar, terminal, editor or hardware service as a Park dependency.

- [ ] Complete U11's integrated keyboard/IME, modal handoff/quarantine and installed-profile acceptance: Brazilian layout, dead-key composition, effective labels, held-key/modifier ownership and restart with the saved settings. Seed/verify Brazilian layout and 35 Hz / 300 ms in the reviewed daily Cell handoff in delivery stage 7, preserving generic defaults and Niri. Layout/variant/options/repeat controls, Park seat application, atomic persistence and the shared event/host-mask adapter are implemented with the software evidence below; do not implement them again. Retain actual operating-system input and uncommon host modifier-role acceptance. Shared command bindings remain in Lingua; device settings stay in local Park configuration.
- [ ] Validate the implemented pointer/touchpad preferences on actual libinput devices: supported tap/natural-scroll/handedness/acceleration, saved preferences on add/reconnect, unchanged unsupported controls and cancellation across lock/VT departure/resume. Confirm complete client button pairs and deferred application during active scrolling/gestures. The native editor, capability adapter and transactional persistence are implemented; the software fixtures below do not establish actual-device behavior. Apply/verify this user's tap preference through the reviewed local-profile handoff, preserving other devices' defaults and Niri.
- [ ] Complete U11's actual-client/provider acceptance for the implemented external commands: XF86 capture versus execution, held repeats across a settings change, shortcut inhibition, authorized locked hardware controls and lock entry/exit. Verify Print/Ctrl+Print with the chosen screenshot/region provider and its capture/consent boundary, plus missing-program feedback and the selected editor. The registry, native configuration UI, local atomic persistence, literal-argv launch and policy checks are implemented below; do not rebuild them. This user's initial brightnessctl/wpctl/playerctl amounts and optional Mod+Z editor binding belong to the reviewed profile in delivery stage 7, with all programs and arguments replaceable.
- [ ] Finish verification of the new resize/row-height actions and existing Window layout editor extension described below: shared model/native/configuration tests, real-pointer wide/narrow controls and real-client configure/ack/buffer sizing. Do not expose legacy column resizing under a misleading row label. Numbered workspace/item transfers, item/row monitor transfers, previous/next workspace and reordering are implemented with their focused software evidence below. Keep remaining optimized, installed and operating-system input acceptance in U11.
- [ ] Apply the remaining accepted terminal/maximize/fullscreen/logout defaults through the existing registry and add the saved-logout confirmation through the existing exit/confirmation family. Cancel and failed saves preserve the session; trusted shutdown and validation cleanup must still save and terminate without an interactive prompt. Preserve explicit owner overrides and verify the fresh/default and overridden configurations separately.

External brightness, audio, microphone, playback, screenshot/region and editor commands now share the implementation described below. Generic program descriptions remain unset; screenshot actions have Print/Ctrl+Print defaults and the generic editor action starts unbound. Keep locker choice and actual service startup with the external providers/UWSM. Do not copy the user's provider-kill shortcuts, private document scripts or activation debug bypass into defaults.

Workspace/output transfers must carry the existing placement identity, width intent, floating/free geometry and applicable row/home state through one source/destination transaction. Merely changing `WorkspaceMember` currently lets the next resolution remove the source item and insert a fresh default-sized destination item; extend that bridge instead of losing the size/arrangement state. Preserve live surfaces and native content, adapt insertion to the destination's tiled/free mode and retain a valid source arrangement if the destination disappears or rejects the move. Whole-row transfer preserves member order and row height; it is distinct from moving the workspace.

Implement the first input-settings item through the existing host boundary. Put portable keyboard settings and request/response snapshots beside `host/display.rs`; the native Park input page edits a draft and sends one bounded typed request through `HostRequests`. Desktop owns the local `park.json` write and Smithay owns keymap compilation, repeat information and client keymap/modifier delivery. The canvas worker receives the effective snapshot rather than reading or writing the host file itself. Sandbox shows host availability accurately and must not change Niri's keyboard configuration.

The keyboard editor is implemented in `native/layout/desktop/input_settings.rs`: a collapsed summary and **Edit keyboard settings** button, followed by the retained inline draft when expanded. Existing monitor/width controls retain their position before this section. Layout, variant, options, repeat rate/delay, Save and Cancel use native fields and the existing panel scrolling. A bounded scrollable feedback area keeps errors above stable action buttons. The editor retains field entities and focus after failure or an external active-setting change; Cancel explicitly reloads the active settings. A late successful Save reply preserves any newer field edits and explicitly labels that newer draft as unsaved, while advancing the saved origin/revision for the next Save. Its regression also checks the retained field focus and the subsequent request's new revision/value; it is part of the currently compiling interface suite. No new modal, shortcut table or permanent desktop toolbar is introduced.

`host/input.rs` owns the typed settings/request/response snapshot, including effective layout names and shared key-label/modifier metadata. `server/keyboard.rs` compiles the candidate through xkbcommon and applies it through Smithay, waiting until all seat keys are released. Lock cancels pending configuration. The server restores the previous map after an ordinary save failure, reports exceptional restore failures honestly, and sends actual effective settings rather than pretending a failed write succeeded. `Commands::update` is the existing bounded/private atomic replacement path factored for both keyboard and monitor changes; keyboard writes preserve unrelated provider/output entries and reject a stale saved keyboard source. Desktop publishes snapshots as `Input::Keyboard`, resets compose/repeat on a map revision, and leaves the canvas worker out of host-file ownership.

The shared shortcut captions/editor/Operation now show effective XKB symbols alongside unchanged physical binding identities when needed. Modifier roles come from xkbcommon state for each physical key, not assumptions about its name or symbol; native dispatch/capture and handoff reuse that metadata, including remapped Control/Alt/Super and AltGr classification. Native logical special keys come from the actual keysym. `actions::native_key_matches` lets native named controls and modal Escape/Enter use those logical keys, while configured command triggers retain their physical identity. Shortcut capture uses the same named-control matching and mapped modifier roles. U11 must still exercise mapped Escape/modal recovery and modifiers across the complete compositor/IME/client bridge; the new pure bridge and protocol tests alone cannot prove those combined interactions.

The shared input work addresses a source-confirmed gap outside Park's own keymap: the previous `Modifiers::pressed_for` and capture fallback inferred modifier roles from physical key names when `KeyboardSnapshot` was absent, as in Sandbox. The implementation below binds the available Bevy/Winit host modifier information and retains Park's XKB metadata as authoritative. Preserve physical command-trigger identity and do not build a replacement keyboard engine. Simply switching to Bevy 0.19.1's aggregate `ButtonInput<Key>` is insufficient: its keyboard input system releases a logical modifier on either physical release and clears only physical key state on keyboard-focus loss. Cover remapped Control/Alt/Super, multiple physical keys contributing one modifier, focus loss/return, AltGr versus Ctrl+Alt, capture and command dispatch without a Park snapshot. Actual host keymap/input acceptance stays in U11; Park metadata-present tests cannot establish Sandbox behavior alone.

A second source-confirmed gap concerned event batches in both modes. The previous `shortcuts::desktop_input` computed modifiers once from the frame's final held keys and retired intercepted keys only after the whole batch. That lost modifier context for complete fast chords and could treat two distinct presses as one held press. Native action/widget dispatch also needs the modifier context belonging to each event. The implementation below reuses the existing input handoff/capture/dispatch path, preserves chronological ownership and retires an intercepted trigger on its matching release; there is no width-specific key handler or second command table. Shared capture, desktop commands and ordinary native text widgets must agree, while leaving the final aggregate state correct for continuous camera controls. Verify these cases with and without Park metadata, through a rebound chord and actual focus/modal handoffs:

| Events delivered in one update | Required result |
| --- | --- |
| Modifier down, width trigger down/up, modifier up | Exactly one width action despite the final modifier state being released |
| Modifier down, trigger down/up twice, modifier up | Exactly two width actions; matching releases end interception for each press |
| Held trigger followed by OS repeat events | Only actions whose registry policy permits repeat may repeat |
| Control down, A down/up, Control up in a native field | The normal widget selects all using that event's modifier context |
| One of two modifier-contributing physical keys released, or window focus lost | A still-held contributor remains effective; focus loss clears stale ownership without a phantom modifier on return |

Keep U8's requested one-step-per-distinct-press contract and U11's IME/client isolation checks explicit. The preceding graphical fixtures sent settled individual events; the updated rendered width-editor fixture now sends a complete Ctrl+A press/release batch before replacement text, and must pass alongside the shared input tests.

The implemented shared event path is `native/keyboard.rs`, installed by the existing `ActionsPlugin`. Its `Prepare` system runs after Bevy `InputSystems` and before shortcut capture. It tracks held physical contributors and their logical keys per window, with Park's `KeyboardSnapshot` roles authoritative when supplied. The source event's context is retained as:

```rust
pub(crate) struct PreparedKey {
    pub input: KeyboardInput,
    pub modifiers: Modifiers,
    pub modifier: bool,
}
```

Shortcut capture consumes or returns those prepared events rather than recomputing modifiers from the final frame. Its existing activation-frame buffer retains the same event context across the delayed capture; swallowed-key ownership and quarantine remain separate from actual held-key state. Desktop interception is keyed by window and physical trigger and retires that ownership on the matching release inside the batch.

Permitted native events still use Bevy's `dispatch_focused_input::<KeyboardInput>` through one privately registered system and a retained message queue. Around each call, `keyboard::Dispatch` provides the event's modifiers to native actions and the ordinary `ButtonInput<Key>` core modifier flags are set for Bevy's real text widgets. Restore the final aggregate afterward, and clear the original raw queue so Bevy's scheduled dispatcher cannot replay the batch. A dedicated registered dispatcher keeps its reader cursor separate from other callers of Bevy's cached function. Focus loss clears held ownership and stale logical key state; manual/synthetic physical-state seeding is limited to keys not represented in the current event batch. No additional command table, text editor or configuration surface is introduced.

`native/keyboard/host.rs` now supplies the thin host adapter. It reads `RawWinitWindowEvent`, resolves the actual owning entity through Bevy's existing `WINIT_WINDOWS` map and records each non-synthetic keyboard event using Bevy's public `convert_keyboard_input`. A per-window modifier mask follows raw `ModifiersChanged` events in chronological order. Each converted keyboard event retains the mask known at its position; a later mask must not be applied retroactively to an earlier key. Match that context to the exact typed `KeyboardInput` in order, preserving source window, physical/logical key, text, state and repeat. Skip an unmatched raw prefix when the next exact typed event matches, so an already discarded window's event cannot hide a surviving key's earlier modifier mask. A matched event with no known mask uses its tracked logical contributors, not the later final mask. Events without a raw counterpart, such as Bevy's focus-loss releases and manual fixtures, can use the current mask for their window.

The current host mask also supplies continuous input state when a modifier was held before Sandbox gained focus and no modifier key-down exists in its local history. Focus loss, unfocused/destroyed windows and global keyboard-focus loss clear stale ownership; repeated focus-loss messages are drained in one update. A mask-only change does not require a keyboard event to refresh the native widget's logical modifiers. A nonempty Park keymap snapshot continues to take precedence over host masks. The direct optional `winit` dependency is pinned to the already used **0.30.13**, activated by `native-presentation`; it binds the existing windowing runtime rather than adding another event loop. Keep its resolution aligned with Bevy's dependency.

This code's initial interface/test check passes with warnings denied in **3m29s** (`ergonomics-events-check-v1.log`), before switching the dispatcher to its dedicated registration. The v2 selected suite finished **57 passed / 1 failed**: the new real text-widget fixture omitted the pointer-release message required by Bevy's widget plugin. The isolated v3 run confirmed that missing-message validation failure; the fixture now registers the message rather than removing the widget behavior. The new host adapter's v4 check found a mutable cursor/self borrow conflict, corrected by temporarily retaining the cursor outside the event loop. Keep those logs; the corrected results below close this implementation task without claiming installed-session acceptance.

The corrected v5 interface/test-target check passes with warnings denied in **1m39s**, and all **64 selected keyboard/action/shortcut/editor tests pass in 1.59 seconds** (`ergonomics-events-check-v5.log`, `ergonomics-events-interface-v5.log`). All **four rendered shortcut/width/keyboard/layout scenarios pass in 180.48 seconds** (`ergonomics-events-gpu-debug-v5.log`), including the updated batched Ctrl+A through actual modal fields. The narrow shortcut capture was visually inspected. The validation runner's existing `keyboard-shortcuts` stage now selects the shared keyboard and native action tests alongside the shortcut tests, so preparation reproduces this coverage. `ergonomics-events-v5-source.sha256` records that source before the following discarded-window correction.

Final review added the discarded-window-prefix correction and a mask-only test covering Control/Alt/Shift/Super, combined masks and clearing without any keyboard event. The current v6 source passes **185 shared tests in 10.34 seconds**, **177 ordinary Park tests in 3.37 seconds** and **18 validation-tool tests in 0.11 seconds**. The Park run leaves its **23 ignored hardware/GPU fixtures explicit**; it is not a new full 200-test run. The updated rendered width scenario separately passes again in **15.87 seconds**, and its 360-pixel capture was visually inspected. Both GUI checks pass with warnings denied: **1m10s** for `instinct,park`, **1m03s** for ordinary Sandbox `instinct`. These logs use `ergonomics-events-*-v6.log`. `ergonomics-events-v6-source.sha256` records the Rust/manifests/WGSL and root Cargo/Nix inputs and passes its post-run comparison. No optimized pacing, installed artifact, real operating-system input or physical login acceptance is claimed from these debug/software checks.

The selected tests now include complete/multiple chords per update, remapped logical modifiers, two physical contributors, focus loss/return, AltGr versus literal Ctrl+Alt, Park metadata precedence over a contradictory host mask, actual Bevy text-widget Ctrl+A/replacement, default/rebound width cycling, capture of a fully released batched chord, host modifiers held before focus, raw-mask updates without key events, switching owned windows and stale-window cleanup. Converted-stream tests cover chronological masks without fabricating Winit's private platform key fields; actual raw modifier/focus messages exercise the adapter through the normal App schedule. Real operating-system key events, uncommon host XKB modifier-role reporting, IME/client handoff and saved installed-profile restart remain U11 acceptance until observed.

The actual host transaction entry points are:

```rust
let request = KeyboardRequest { id, revision: snapshot.revision, settings: draft };
server.keyboard_request(request);
server.advance_keyboard(|candidate, expected| {
    Commands::save_keyboard(path, candidate, expected)
});
```

The native editor sends `HostRequest::Keyboard` rather than calling the server directly. Validate bounded strings and numbers before queuing, compile the proposed map before accepting it, and retain the editor on failure. Empty layout means the backend/system default; empty options explicitly requests no XKB options. The active names appear in the summary. A configuration error at startup leaves a usable seat and reports feedback. No process-wide XKB environment is changed.

Keep generic defaults independent of the user's profile. The local profile shape below preserves their Brazilian layout and repeat choices; the codec and software transaction are verified below, while the actual daily profile and physical acceptance remain outstanding:

```json
{
  "keyboard": {
    "layout": "br",
    "variant": "",
    "options": "",
    "repeat_rate": 35,
    "repeat_delay_ms": 300
  }
}
```

Directional bindings retain physical key identity. Shortcut capture/help may display the current XKB symbol together with that identity when they differ; saving a displayed symbol must not silently move the binding to another physical key. Verify real Wayland keymap/repeat delivery, Brazilian shifted symbols and dead-key composition, applying after held-key release, stale/write-failed saves, preservation of unrelated output/provider settings and continued modal/client input ownership. Pointer/touchpad controls use the same snapshot/request destination with libinput capability discovery and device-added handling; an unsupported tap/acceleration option must be reported rather than applied optimistically. Keep actual-device acceptance separate from synthetic event tests and leave that acceptance pending until observed.

The new tests cover typed bounds, editor draft/focus/entity retention, stale sources, atomic merging, actual Wayland keymap/repeat delivery, Brazilian shifted symbols, delayed application, invalid maps, write rollback and lock cancellation. `presentation/tests/keyboard.rs` adds real native field replacement, validation, Save through the host/file transaction and Cancel at 1000/360 pixels; validation preparation registers `keyboard-settings-rendering`, `input-settings-policy` and `input-settings-persistence`. The existing native input protocol stage also selects the new real-client keyboard tests. The fresh v1 check rebuilt dependencies after the output removal and found one missing qualified `Modifiers` reference, now corrected. The v2 check then found the old unused `KeyCode` import after logical conversion moved to keysyms; it is removed. The v3 Park nested/test-target check passes with warnings denied in **13.85 seconds** (`ergonomics-keyboard-check-v3.log`). All **59 selected interface tests pass in 3.32 seconds**, including keyboard bounds, editor/draft/focus preservation, late Save replies, mapped modifiers/named keys, shared command help, shortcut editing, actions and scrolling (`ergonomics-keyboard-interface-v3.log`). That executable includes the final feedback-column layout and late-reply changes. The corrected real-client fixture seeks the received keymap descriptor to offset zero before reading its bounded payload, matching the offset used by protocol clients. All **11 backend/protocol tests pass in 0.07 seconds** (`ergonomics-keyboard-backend-v4.log`), including real US-to-Brazilian keymap delivery, shifted symbols, repeat information, held-key deferral, invalid-map rollback and atomic persistence. The rendered editor then exposed two general input defects. Wheel movement was applied twice by the edit panel and shared scroll engine; the panel now leaves the bounded delta to the shared engine, and **16 scroll/edit-panel regressions pass** (`ergonomics-keyboard-scroll-interface-v7.log`). The graphical-monitor GPU transaction also passes on that source. The v7 keyboard GPU fixture failed replacing the three-character repeat-rate field: Bevy IME commit checks existing text plus inserted text without subtracting the selection. The shared controls editing adapter now clears composition and uses the existing selection-aware filtered insertion operation, preserving edit order and character limits; Park native IME delivery uses the same operation instead of temporarily increasing limits. The v10 unit attempt failed because its fontless fixture could not select the initial text; registering the same bundled font as the native editor corrected that fixture, with the v11 results below. Earlier v8/v9 attempts failed on the local linker selection and plugin tuple arity respectively; retain those logs. The previous source manifest predates these corrections. Keep remaining rendered, GUI, optimized and installed acceptance unchecked until their actual results are inspected.

The shared text-input correction passes its four selected interface/editor tests and all seven native IME regressions (`ergonomics-text-input-interface-v11.log`, `ergonomics-text-input-backend-v11.log`). The actual keyboard-settings GPU scenario passes at 1000 and 360 pixels in **47.73 seconds** (`ergonomics-keyboard-gpu-debug-v11.log`): real pointer/wheel access, Select All replacement, invalid-number rejection, typed Save through the real keyboard/file transaction and Cancel. The narrow saved capture was visually inspected. These results precede the shared pointer-widget/request refactor; its combined checks below must establish current-source acceptance rather than treating the earlier executable as unchanged.

#### Pointer and touchpad preferences: implementation and remaining acceptance

The first implementation is now registered and passes the session/server-test-target compile check with warnings denied (`ergonomics-pointer-check-v1.log`, **56.31 seconds**). Its focused software results follow below; actual-device and optimized/installed acceptance remain unfinished. It reuses the host snapshot/request, retained native draft and `Commands::update` ownership pattern. Physical `session.rs` owns the libinput device handles; the generic Desktop/canvas bridge must not attempt to configure Niri's devices in nested mode. The adapter lives beside the physical backend, with no new input daemon or replacement pointer event engine. The initial preference set is tap-to-click, natural scrolling, left-handed buttons and acceleration speed. Touch/tablet mapping and desktop gestures retain their separate U9/U10 owners.

Use global preferences applied to every capable device, with each device's supported controls and effective values visible. No persistent per-device override database is required for this first adapter. Libinput's session device ID/sysname is suitable for current add/remove bookkeeping and display; do not pretend `eventN` identifies the same hardware after reboot. Unplugging a device must not delete the owner's preferences. Nested Park reports physical-device configuration as unavailable; a forged/stale request must return that error without touching the parent compositor.

The local configuration codec now accepts this shape:

```json
{
  "pointer": {
    "tap": true,
    "natural_scroll": null,
    "left_handed": null,
    "acceleration": null
  }
}
```

Null means the device's libinput default; booleans select an explicit preference, and acceleration is a finite number from -1 to 1. Generic defaults are null. The user's accepted tap preference belongs in their local profile. Query capabilities and effective/default values from the actual device. In the installed `input` 0.9.1 API, tap availability comes from `config_tap_finger_count`, acceleration from `config_accel_is_available`, natural scrolling from `config_scroll_has_natural_scroll` and handedness from `config_left_handed_is_available`. Apply through `config_tap_set_enabled`, `config_accel_set_speed`, `config_scroll_set_natural_scroll_enabled` and `config_left_handed_set`. These are existing facilities, not features to implement internally.

`host/input.rs` now shares `InputRequest<S>`, `InputOutcome` and `InputResponse` between keyboard and pointer settings. `host/input/pointer.rs` carries device capabilities, defaults, effective values and configuration errors. The native pointer page reuses keyboard settings' field/button/feedback builders and request-ID allocator; it has three Default/On/Off choices, a bounded acceleration field, scrollable device/status details and Save/Cancel. Unavailable controls retain preferences and explain their capability state. The physical device adapter is `session/devices.rs`, with `input_settings.rs` providing the tested transaction/controller boundary. Desktop only queues bounded typed requests and publishes `Input::PointerSettings`; physical `Hardware` owns application and persistence. Nested mode sends an explicit unavailable response instead of touching its parent's input devices.

The implementation uses these entry points inside the physical loop:

```rust
self.devices.request(request, protected);
self.devices.advance(protected, pointer_idle, |settings, expected| {
    desktop.save_pointer(settings, expected)
});
desktop.pointer_settings(self.devices.snapshot())?;
```

The device snapshot is published after application and rollback so the UI receives actual effective values. Track finger scrolling per device **and axis**; stopping one axis must not apply new settings while another axis or gesture is active. VT departure clears tracked devices/interactions and cancels pending changes; resume repopulates from libinput add events. Atomic pointer writes preserve keyboard/output/provider fields and compare the previously saved pointer source before replacement. A simulated device/write fixture does not prove actual libinput setters or resume behavior on this laptop.

All **nine shared input/editor tests pass in 0.13 seconds** (`ergonomics-pointer-interface-v2.log`), covering both editors, late acknowledgements/newer drafts, hotplug/stale drafts, unsupported controls, field bounds and the single shared wheel path. All **20 backend/persistence tests pass in 0.04 seconds** (`ergonomics-pointer-backend-v2.log`), including partial-apply and write rollback, failed restoration reporting, private atomic merges, invalid/stale saves, per-axis/gesture deferral, seat departure and the preserved existing host-pointer test. The separately selected **five real-keymap/native-conversion tests pass in 0.06 seconds** (`ergonomics-pointer-keymap-v3.log`); the v2 selector used the wrong keyboard-module prefix, so do not claim those five from its 20-test result. Expected invalid-map xkbcommon diagnostics belong to that rejection fixture.

Both native rendered settings scenarios pass at 1000/360 pixels in **94.93 seconds** (`ergonomics-pointer-gpu-debug-v2.log`): keyboard validation/Save/Cancel, pointer validation, apply/write rollback, retained fields, retry, provider/keyboard preservation and Cancel. Visual review of the pointer saved/failed captures found that its small device-details viewport clipped the second fixture device without a clear scroll affordance. The device readout now has a labelled, fixed 160-pixel scroll region, separate from the stable status area. The v4 rendered assertion proves both fixture devices fit without clipped lines at both widths; both rendered settings tests pass in **103.74 seconds** (`ergonomics-pointer-gpu-debug-v4.log`). The revised wide/narrow saved captures were visually inspected. All **nine current interface tests pass in 0.32 seconds** (`ergonomics-pointer-interface-v4.log`). Nested production checking passes in **13.11 seconds**; both initial GUI checks pass, and the final current-source Park/Sandbox GUI checks pass in **10.70/0.84 seconds** (`ergonomics-pointer-nested-check-v3.log`, `ergonomics-pointer-{park,sandbox}-gui-v4.log`). Validation-tool checking passes in **74 seconds**, and all **18 tool regressions pass in 0.10 seconds**, including metrics and observer tests (`ergonomics-pointer-xtask-check-v3.log`, `ergonomics-pointer-xtask-tests-v4.log`). The earlier root-only selector passed ten tests; the final selector covers the complete validation module. The scoped input source manifest is `ergonomics-input-v4-source.sha256`, checked unchanged after the final results. This correction does not change device transactions. The preparation runner includes `input-settings-policy`, `input-settings-persistence`, `physical-input-settings` and `pointer-settings-rendering`; the physical-input policy stage is a hardware-free backend fixture, not physical-seat acceptance. Keep optimized, installed and actual-device acceptance in U11/delivery stages 4–8.

Process `InputEvent::DeviceAdded`/`DeviceRemoved` in the existing physical input loop. Apply retained preferences before normal events from a newly added device, publish a revised capability/effective-state snapshot and remove departed handles. Unsupported controls remain unavailable for that device and are not represented as successfully applied. A newly added device whose setter fails remains usable with its actual effective values and a visible error. Keep settings requests bounded and revisioned; a device-set change invalidates a stale draft instead of silently configuring a different set.

Wait for the current pointer press/grab and touchpad scroll/gesture to finish before changing settings that could alter its interpretation. Preserve complete application press/release pairs and existing grabs. Lock/VT departure cancels pending requests. Capture previous effective values, apply to capable devices, then save through the same bounded private-file transaction while preserving keyboard/output/provider entries. On apply/write failure restore already changed devices and retain the editable draft; publish any failed restoration honestly with effective values. Never mark requested values as active merely because the file was saved.

Expose a compact pointer/touchpad section alongside Keyboard, with default/on/off choices for supported booleans, default-or-speed for acceleration, Save/Cancel, capability explanations and effective per-device information. Preserve draft entities, focus and scroll as device state changes. Cover capability mixtures, defaults/range validation, partial apply/save rollback, stale requests, device add/remove, held-button/gesture cancellation, lock and unavailable nested mode with isolated tests and the native wide/narrow controls. Actual tap/scroll/handedness/acceleration and unplug/replug acceptance still requires devices in the physical handoff; simulated devices cannot close that gate.

The input-v4 optimized Park run finished with **198 passing tests and one failure in 109.71 seconds** (`ergonomics-input-park-release-v1.log`, captures in `ergonomics-input-captures-release-v1`). It used `session,server-tests`, serial execution and included the ignored hardware-render fixtures. The shortcut-editor fixture tested its modal-trigger rectangle before scrolling the now-longer Park panel; that control was outside the visible clip. The fixture now reaches it through the existing real pointer/wheel helper, retaining the trigger, click and activation-frame key assertions. The v2 focused release recompilation was deliberately interrupted before tests to integrate the next coherent layout candidate; it supplies no runtime pass. The corrected fixture still needs its rerun. The input-v4 Rust sources/manifests/WGSL baseline remains archived as `ergonomics-input-v4-rust-source.tar` with a companion SHA-256 file. It predates the Window layout work below and is not current-source acceptance.

That completed run measured **52.23 FPS flat** (19.15 ms average, 23.22 ms p95) and **41.72 FPS spatial** (23.97 ms average, 28.87 ms p95), over 120 frames per projection with native update, GPU rendering and fence completion. Spatial pacing fails the existing 45-FPS stress floor. The timing JSON files are in the capture directory; retain this failure alongside the earlier samples. No physical scanout, real browser video, installed-session or hardware-input acceptance follows from these measurements. Niri and the physical seat remain untouched.

#### Implemented shared layout and opening preferences

Finish this verification before adding the remaining external-action/default-command family. Portable layout/opening preferences now extend `shortcuts/configuration.rs`'s existing `LayoutSettings`, `Document`, decode/encode and the same `lince.interface` Extension in interface.lingua. Keyboard/pointer hardware preferences remain in local park.json. The existing width modal is extended as **Window layout**, with one retained draft, input owner and Save/Cancel for widths, gaps and opening state. Its visible entry/actions are **Edit window layout**, **Save window layout** and **Cancel layout draft**. It reuses the existing fields and bounded input-settings feedback; there is no second Layout page or permanent desktop control. Compilation and tests must establish this implementation before pruning its remaining verification.

The new fields are bounded integer `gaps` from 0 to 256 in logical layout units, initially 8 generically and 0 in the reviewed user profile, and an explicit normal/maximized `application_opening` enum, generically normal. `native/layout/desktop/preferences.rs` synchronizes gaps through `Workspace::set_gap`, comparing values before taking mutable workspace access and invalidating affected geometry/influence caches. It covers inactive/restored models so serialized per-workspace gaps cannot defeat the current global preference. Preserve item identities, rational width intent, authored free geometry and bar reservations. Park's `Presentation::open_window` marks a genuinely new application with transient `ApplicationOpeningPending`; only a successful first insertion may consume that marker and apply defaults. A saved window profile or matching existing placement consumes/discards it without reapplying opening defaults. Lazy layout initialization caused by Focus is not a new window-opening event. Free application windows retain the configured normal-width fraction and original canvas scale through the model's shared `set_free_width` operation; native Sands and dialogs keep their own geometry. Initial maximization retains that normal width and respects usable output bounds.

The existing commit API remains the implementation entry point. This example uses the wired fields, keeps current bindings/width settings and still requires a retained-origin conflict check before committing:

```rust
let mut layout = world.resource::<LayoutSettings>().clone();
layout.gaps = draft.gaps;
layout.application_opening = draft.application_opening;
layout.validate()?;
let bindings = world.resource::<Settings>().clone();
configuration::commit(world, bindings, layout)?;
```

The unified editor constructs its candidate from the retained complete origin. **Reset widths** only resets width presets/default width/move step, preserving gaps and application-opening policy. Active-origin conflicts, validation/write failures and Cancel retain the established draft/atomic-save ownership. Composing/pending text participates in validation and observed state so Save cannot commit an older field value. Its local wheel observer now only stops propagation; the shared engine owns the delta once. The bounded shared feedback leaves the footer reachable. Check opening/width precedence with U8's 900-unit fixture, then restore a saved workspace and verify the profile does not reapply initial opening state.

The earlier prepared files are integrated into the actual configuration/editor sources and registered `presentation/tests/layout_preferences.rs`; the xtask preparation stage is `window-layout-rendering`. Codec tests cover bounds, binding-preserving round trips and file persistence/restart. Native editor tests cover width Reset preserving gaps/opening, invalid gaps, stale active settings, retained focus and pending-text rejection. The rendered case covers real typing/scrolling, invalid gaps, Save/Cancel and width Save/Reset preserving opening/gaps at 1000/360 pixels. Model integration tests cover live/restored gaps, normal-width/camera restoration, bar reservations, authored native geometry and dialog exclusion. First-visit cleanup now removes only application placements without a matching live placement identity, preserving a restored/transferred live application's state. U7's later transfer checkpoint verifies the complete transaction; it is no longer outstanding feature work. The feature implementation is complete with the focused evidence below. Preserve U6/U11's current optimized, actual-client/startup and installed acceptance, and seed this user's zero-gap/maximized choices through delivery stage 7 rather than changing their live profile here.

The current session/test check passes with warnings denied in **12.66 seconds**, and all **150 selected shared layout/shortcut/editor/text tests pass in 1.92 seconds** (`ergonomics-layout-check-v4.log`, `ergonomics-layout-interface-v4.log`). Earlier interface runs recorded 142/8 and 148/2. Corrections distinguish true opening events from lazy focus initialization, install the real text-edit pipeline in the lightweight editor fixture, verify pending-save feedback through the visible status and align the fresh-canvas width test with the approved focus-required contract. Width rejection before focus and successful cycling after Focus are both asserted; explicit Unfocus remains a no-op for width changes.

The corrected source passes the session/test check in **12.60 seconds**, all **165 shared layout/shortcut/editor/text/edit-panel tests in 10.02 seconds**, and all **three rendered layout/width/shortcut fixtures in 132.49 seconds** (`ergonomics-layout-{check,interface,gpu-debug}-v8.log`). The rendered layout case passes at 1000 and 360 pixels, including opening an application through `Presentation::open_window`, initial maximization and normal half-width restoration. Wide/narrow Reset captures were inspected.

The expanded final check passes all **165 shared tests in 9.93 seconds**, including inactive-workspace gaps without changing its authored name/view. Both GUI checks pass with warnings denied: **59.77 seconds** for `instinct,park` and **50.44 seconds** for ordinary Sandbox `instinct`. XtTask checking passes in **1.51 seconds**. These logs use `ergonomics-layout-*-v9.log`. The initial validation-test command incorrectly selected a nonexistent xtask library target and ran no tests; the corrected binary-target command passes all **18 validation-tool tests in 0.11 seconds** (`ergonomics-layout-xtask-tests-v10.log`).

The full current Park `session,server-tests` debug suite passes **all 200 tests, including ignored GPU fixtures, in 764.22 seconds** (`ergonomics-layout-park-debug-v10.log`). It covers the complete rendering/input/protocol suite, Showoff, shader source/output comparisons, stacking, graphical monitors, shared modal capture, pointer/keyboard settings, window lifecycle and normal-width restoration. The free-window lifecycle fixture now asserts the configured half-width normal size, **388×480** inside an 800×600 output with default gaps, rather than the former unconditional 640×480. Captures in `ergonomics-layout-captures-debug-v10` include actual gap/opening controls and invalid-gap feedback at 1000/360 pixels; the narrow options/error images were inspected and have readable controls/status with reachable Save/Cancel. `ergonomics-layout-v10-source.sha256` records the unchanged Rust/manifests/WGSL and root Cargo/Nix inputs for this run. Debug timing samples do not replace the optimized pacing matrix or erase the recorded spatial stress-floor failures. No new release, installed-artifact or physical-session acceptance is claimed.

Earlier v3–v7 rendered attempts retain the narrow-scroll failure and diagnostic evidence. Wheel input reached the correct 424-pixel viewport with 641-pixel content, advanced to 217, then returned to 18. The shared `edit_mode::reveal_focus` cache included every panel's dimensions, allowing unrelated panel changes to reveal the modal's first field again. It now caches only the focused entity's bounded ancestor panels, sorting their identities for stable comparison. Its regression preserves manual scrolling across unrelated panel creation/resizing while still revealing focus when its own viewport resizes. This fixes the ordinary scrolling path, with no direct-scroll mutation in the fixture. The unnecessary content-column prototype and temporary observer logging are removed; `Modal.list` remains the existing viewport/content owner. Actual client configure/ack acceptance remains U11.

Audit navigation/overview motion before adding an instant-navigation toggle. The current overview and tiled transition controllers change state directly; free focus-follow already writes the final camera transform without interpolation. Preserve that immediate default and zero relative follow lag. Reuse an existing motion policy for any animated navigation path found; do not implement an animation engine merely to give a new preference something to toggle, or mark an unused boolean as a working feature. U10's optional transitions must obey the same policy. Add meaningful behavior/codec/native UI tests for the settings actually exposed, including zero-gap rows, constrained widths, stale drafts, restart and unchanged authored content.

#### Following U7 implementation: command defaults and general external actions

Use the existing registry in `shortcuts/model.rs`, its metadata/Operation integration in `shortcuts/commands.rs` and the current typed `DesktopAction` path. The source registry now contains 95 commands, including the twelve external actions, thirteen item-workspace/row-monitor additions, four adjacent-workspace/reorder commands and five size commands. The navigation checkpoint below has 90 commands; its results do not verify the later size additions. Add remaining commands to the complete metadata/action model, not just its key handler; label, scope, availability, repeat policy, conflict editor, Operation discovery and saved startup binding must agree. `configuration::encode` already persists only `Settings::overrides()`. Change generic defaults directly while retaining explicitly chosen overrides; do not add version migrations or rewrite the owner configuration merely to update a default. Seed the reviewed personal profile separately in delivery stage 7.

The numeric-workspace actions remain `Workspace(u32)` and `MoveToWorkspace(u32)`, with **one-based** addressing. Their implementation now prepares a complete source/destination transaction instead of changing membership alone. Missing destinations 1–9 are staged through the normal allocator; a failed move cannot publish the allocation. The registry supplies Mod+Shift+1–9 and four independent `MoveRowToOutput` commands. The completed native/UI/runtime verification is recorded below, followed by the completed workspace-reordering checkpoint. U9 retains only optional back-and-forth policy for this workspace-navigation family. A successful item move follows its destination and retains the moved item as the focus target.

The shared transfer moves a complete placement through one source/destination transaction: stable item/live-surface identity, width intent, normal/free/floating geometry and scale, row/home information, constraints and applicable focus state. It validates the destination and prepares both models before publishing changed membership or root/output ownership. A rejected/disappearing destination leaves the source intact. Numeric workspace and item-to-monitor movement use this operation; U9's later overview drag must reuse it. The whole-row variant preserves order and row height and has its own typed action for Mod+Shift+Y/U/I/O. `MoveWorkspaceToOutput` still moves a whole workspace, initially unbound under the new monitor defaults; it is not an alias for row movement.

The pure model transaction is implemented in `layout/desktop/transfers.rs`, with row/home extraction and rebasing in `layout/desktop/rows/transfers.rs`. The native wrapper is `native/layout/desktop/transfers.rs`: it captures the live members, parent ownership, models and input target; revalidates them on commit; publishes both models before membership; and returns a selection to activate after Park has published its output assignment. Its entry points include:

```rust
let moved = source.transfer_to(&mut destination, item, TransferKind::Item)?;
```

```rust
let prepared = transfers::prepare_numbered_move(world, root, number, TransferKind::Item)?;
let destination = prepared.destination();
let selection = prepared.commit(world)?;
```

The pure-model and native snippets show different layers, not two operations to dispatch for one request. Park prepares and validates a cloned output assignment before committing the native transaction, then publishes that assignment and activates the returned selection. Rejected commands report a useful notification and leave source geometry, focus and membership intact. Ordinary destination errors are not fatal compositor failures.

Workspace-name buttons request `HostRequest::Workspace { owner, id }`, and New requests `HostRequest::CreateWorkspace { owner }`. The compositor sends tracked runtime inputs and uses the same selection/creation transaction as commands. Keep the stable ID across that bridge: converting a clicked workspace to an index before closing Laboratory can select the wrong entry after the temporary entry disappears. `workspace::numbers` excludes the scoped temporary Laboratory workspace, displays authored ordinals separately from editable names, and provides the allocator/command availability mapping. Temporary IDs neither consume authored numeric slots nor exhaust the permanent allocator. An invalid owner or a closed target cannot switch another canvas. The real-pointer fixture passes these routes, including creation while Laboratory occupies an otherwise available permanent ID, at 1000/360 pixels.

Name editing also binds its field to `WorkspaceName(id)` rather than whichever workspace happens to be active when autosave runs. `workspace::rename_id` validates that captured identity, while `workspace_changed` saves the old field before rebuilding the selected workspace row. Other Park settings remain intact across a workspace switch. Long workspace buttons use a full-width wrapping caption inside their assigned row space; the v7 rendered capture reproduced a 472-pixel caption overflowing the button and delete control. The corrected v8 capture is readable at 360 pixels, with the complete name, ordinal, active editor and delete controls inside the panel.

`TransferKind::Row` moves the focused tiled row as one ordered unit; a floating focus has no tiled row and must receive a useful unavailable result. Preserve the incoming member's width and floating-home caches. An individual item joins the destination's normal insertion position without changing that row's existing height; a whole row creates a destination row with the original height and member order. Row IDs are local to a workspace: allocate destination IDs and rebase moved floating homes/neighbor references instead of accidentally attaching them to an unrelated row with the same source ID. Preserve free/authored rectangles and width scale intent, and resolve requested fractions against the destination's usable bounds and constraints.

Carry fullscreen/maximize and minimized bookkeeping explicitly. Moving a normal focused item to a workspace whose previous item is fullscreen must reveal the incoming normal item without promoting it to fullscreen through `Workspace::focus`'s existing fullscreen-follow behavior. A fullscreen incoming item retains that state on the destination. Preserve other items' normal geometry for restoration. UI transfers require an eligible live focus; any lower-level minimized transfer must not leave `focused` referring to a minimized item. Re-evaluate derived Area influence in the destination through its existing owner rather than treating source influence caches as authored transfer state.

Selecting or moving to a missing keyboard destination creates the required entries through the staged allocation/reservation path, bounded to the nine keyboard destinations. Allocation preserves authored entries, trash reservations and the Cell's workspace ID allocator. A rejected transfer cannot leave a partially created destination or switch through intermediate workspaces. Zero/unbounded missing destinations are rejected; moving to the current workspace is a no-op. Existing higher-numbered workspaces remain available through ordinary workspace navigation and identity-based management.

The model/native regressions cover byte-for-byte rollback on duplicate IDs, invalid models and capacity limits; first full resolve into an unvisited destination; floating→retile restoration; fullscreen/normal state and parent-owned dialogs; an item moving out and back; whole-row order/height; retained editor text/focus; and a disappearing live target. Output selection/focus stays in Park's assignment controller, with both source and destination validated before publishing the transfer. New destinations are registered in a cloned assignment before selection, so a failed allocation or monitor validation cannot partially publish live assignments.

The current transfer checkpoint passes **232 shared model/native/shortcut/workspace/edit-panel/Operation tests in 14.64s**, **182 ordinary Park tests in 4.55s**, and **both hardware GPU transfer cases in 30.15s** (`ergonomics-transfers-{interface,park-debug,gpu-debug}-v8.log`). The ordinary run explicitly leaves 26 GPU cases ignored. A further **five existing GPU scenarios pass in 239.30s**, covering wide/narrow Operation discovery, monitor/editor/follow preservation through bar changes and reconnect, overview, shortcut capture/conflicts and actual tiling controls (`ergonomics-transfers-integration-gpu-debug-v8.log`). Thus seven of the current 26 GPU cases ran in this checkpoint; the other nineteen retain final optimized integration acceptance rather than inheriting a current-source pass from v11.

Both current GUI source checks pass with warnings denied: Park in **58.60s**, ordinary Sandbox in **59.82s** (`ergonomics-transfers-{park,sandbox}-gui-v8.log`). The v6 validation-tool run passes all **18 tests in 0.07s** and registers `workspace-transfer-rendering` for the existing release preparation runner. `ergonomics-transfers-v8-source.sha256` records the unchanged scoped Rust/manifests/WGSL and root Cargo/Nix inputs after these runs; its scoped source archive and checksum-recorded test executables are retained under the same prefix. The archive omits assets/toolchains, and these debug test executables are not installable GUI packages. Direct test invocation uses `RUST_MIN_STACK=8388608`, matching Cargo's configured stack floor. No live session, owner configuration or system activation changed.

Minimal Park, server-only Park and portable interface checks also pass with warnings denied in **0.38s**, **2.14s** and **1.11s** (`ergonomics-transfers-{base,server,portable}-v8.log`). The scoped source checksum check remains clean afterward. These checks retain the shared host/model feature boundaries; they do not substitute for final optimized, installed or physical acceptance.

The new live-client case uses two private native Wayland clients, actual configure/ack/buffer commits and pixel readback. It preserves surface/entity/placement identity through a move to an unvisited ninth workspace, return, whole-row monitor move, rejected floating-row move and an individual floating move/retile. It then cycles exact full/half/third/two-thirds intent with a 450-unit client minimum: half and third both configure 450 without trapping the next step. This is offscreen functional acceptance of the shared typed actions, not proof of physical input dispatch, browser-video pacing, a new optimized GUI package or installed login.

Preserve the native reconciliation boundary accepted by the first-visit and real-client tests: the first `Runtime::seen` visit removes only saved application entries without a matching live `LayoutBox` placement identity. It does not blindly remove an arriving live application's saved width/home state. Keep retired/disconnected identities removable and never match a new window merely by its title. The previous v11 release checkpoint below predates these transfer changes and does not validate their optimized or installed behavior.

Apply the accepted defaults together: terminal Mod+N, maximize Mod+F, fullscreen Mod+Shift+F, saved logout Mod+Shift+E, item-to-monitor Mod+Ctrl+Y/U/I/O, row-to-monitor Mod+Shift+Y/U/I/O and item-to-workspace Mod+Shift+1–9. Retain monitor focus Mod+Y/U/I/O and Operation Mod+P. Logout through the user command must show a native confirmation, then use the existing save-aware exit; Cancel and failed saves preserve the session. Reuse the current confirmation/exit family. Trusted process shutdown and validation cleanup must still save and terminate without being stranded behind an interactive confirmation.

The adjacent-workspace commands are implemented against the ordered authored IDs returned by `workspace::numbers`. Their localized, rebindable registry entries start unbound and do not auto-repeat. They run in both Sandbox and Park through the existing typed host handoff:

| Stable command ID | Operation path | Typed action |
| --- | --- | --- |
| `workspace.previous` | `/workspace/previous` | `PreviousWorkspace` |
| `workspace.next` | `/workspace/next` | `NextWorkspace` |
| `workspace.move-earlier` | `/workspace/move-earlier` | `MoveWorkspaceEarlier` |
| `workspace.move-later` | `/workspace/move-later` | `MoveWorkspaceLater` |

Previous/next stop at either edge with useful live availability feedback; they neither create an empty workspace nor wrap. While scoped Laboratory is active, its underlying authored workspace anchors navigation. The shared implementation captures the destination ID before Laboratory closes and uses the existing identity transaction:

```rust
let destination = workspace::navigation::destination(world, root, -1)?;
let prepared = transfers::prepare_identity(world, root, destination)?;
```

Only registered one-step directions are exposed. `workspace::navigation::reorder(world, root, 1)` exchanges the active authored header with its next authored header, preserving active identity, live members, models, camera and output assignment. Temporary entries are excluded from the mapping and remain in their original list positions; reordering while the active entry itself is temporary gives an unavailable reason. Reordering persists through the existing workspace snapshot; it requires no new storage schema or provider service.

`edit_mode/workspaces.rs` retains each workspace row, name editor, caret and other panel children, then reorders those existing children and updates their ordinal labels. It does not rebuild the draft. `FocusScrollRevision` joins the existing focus-reveal cache, scoped to the focused field's ancestor panel. It changes only on row reordering, requests one reveal after layout, and preserves the shared held-pointer deferral. Ordinary wheel scrolling does not change it. The long-list fixture reproduced the earlier field at y=899 beyond the 700-pixel viewport; the correction passes visibility and later manual-scroll retention at both 1000/360 pixels. The original `EditMode.revision` remains reserved for edit-opening/practice semantics, so a reorder cannot complete an unrelated practice step.

Park's existing workspace protocol already emits changed coordinates while keeping handle identities, names and active state. The new real-protocol case confirms this and activates an old handle after reordering to prove that the request still targets its original workspace ID. No Noctalia-specific branch or new bar adapter is needed. U9's optional back-and-forth preference must use the shared selection path; identity-based protocol activation remains idempotent so a repeated provider snapshot cannot become a workspace jump.

The current navigation checkpoint passes **208 selected shared native/shortcut/workspace/storage/edit-panel/Operation tests in 15.37s**, **183 ordinary Park tests in 4.84s**, and **three mixed GPU/protocol cases in 195.32s** (`ergonomics-workspace-nav-{interface,park-debug,gpu-debug}-v8.log`). The mixed run comprises two hardware GPU scenarios and the real workspace-protocol case; the latter also belongs to the ordinary suite, rather than being another distinct test. The ordinary run leaves **27 GPU cases ignored**. Both GUI source checks pass with warnings denied: **60.00s** for Park and **49.31s** for ordinary Sandbox (`ergonomics-workspace-nav-{park,sandbox}-gui-v8.log`). The unchanged preparation runner passes **18 tests in 0.06s** on v2 and registers `workspace-navigation-rendering` alongside the existing transfer fixture.

Coverage includes nonsequential IDs, edge/no-op and invalid steps, unchanged snapshot content/focus/camera, saved note/Area/order restoration, captured-ID selection across reordering and closed destinations, live command availability, visible-on-another-output navigation, Laboratory anchoring and live Wayland clients. The rendered scenario also applies a user-chosen binding through the actual shared capture editor, moves the focused name field through a sixteen-workspace list, retains its caret/draft and verifies that later manual wheel scrolling is not undone. Its no-Cell fixture applies the binding for that session; the separate configuration/startup test commits the new command bindings to a private Lingua file and verifies them after restart. Do not claim that the no-Cell screenshot fixture writes the owner's profile. The final fixture-only rerun passes in **188.00s** (`ergonomics-workspace-nav-gpu-debug-v9.log`): it dismisses the ordinary success toast through its real close control, then verifies the full workflow at both widths. The wide/narrow scrolled captures in `ergonomics-workspace-nav-captures-debug-v9` were visually inspected; the same focused name field and caret remain visible above the ordinary bottom controls.

Minimal Park, server-only Park and portable interface checks pass with warnings denied in **0.84s**, **1.60s** and **0.86s** (`ergonomics-workspace-nav-{base,server,portable}-v9.log`). The unchanged scoped source is recorded in `ergonomics-workspace-nav-v9-source.sha256`; its Rust/manifests/WGSL archive and the copied Park/interface test executables have separate verified checksums under that prefix. The v9 Rust change after the v8 product checks is limited to dismissing the toast in the rendered fixture. The archive omits assets/toolchains, and those debug test executables are not installable GUI packages. No owner profile, live desktop or system activation changed.

These are debug/offscreen functional checks and source GUI checks, not acceptance of a new optimized GUI package, physical input dispatch or daily-session pacing. Mixed ordinary/ignored test selections use `--include-ignored` alone: supplying it with `--ignored` is rejected before any test executes. Preserve the red long-list failure in `ergonomics-workspace-nav-gpu-debug-v5.log`; the marker corrects that real visibility defect without adding another scrolling controller.

#### Shared item resizing and row-height presets

The size implementation is in `layout/desktop/rows/sizing.rs` and extends the existing `native/layout/desktop/widths.rs` action controller. It uses the current width intent/constraint solver and `Rows` data, without aliasing `resize_column`'s legacy column geometry. The initial compile passes; the expanded functional tests remain required before closing this work. All five commands start unbound, use the shared Desktop scope in Sandbox and Park, and have localized labels, conflict-editor entries, live availability and Operation paths:

| Stable ID | Operation path | Typed action | Repeat policy |
| --- | --- | --- | --- |
| `item.shrink-width` | `/item/width/shrink` | `Resize(Left)` | Held-key increments allowed |
| `item.grow-width` | `/item/width/grow` | `Resize(Right)` | Held-key increments allowed |
| `item.shrink-height` | `/item/height/shrink` | `Resize(Up)` | Held-key increments allowed |
| `item.grow-height` | `/item/height/grow` | `Resize(Down)` | Held-key increments allowed |
| `row.cycle-height` | `/row/height` | `CycleRowHeight` | One step per distinct press |

Horizontal resizing changes only the focused item's width. Tiled vertical resizing changes the shared row height; free/floating resizing changes the normal rectangle around its center, converting the logical-pixel step through the current canvas scale. Neither path changes neighbor width or bar reservations. A manual resize clears only the changed axis's proportional intent. Row-height cycling uses the same exact-ratio comparison and bounded-list validation as width cycling, so equal constrained pixel sizes cannot trap it. Fullscreen rejects size changes, and an explicitly unfocused workspace cannot acquire a target implicitly. Size actions leave maximize through the existing state request, preserving the other normal axis and the client's identity.

`Workspace::row_height`/`set_row_height` access the focused member's real row height. Updating that height also updates the matching floating-home fallback, so rebuilding an emptied row later does not restore an obsolete height. Proportional row heights now include inter-row gap allowance, matching the width solver: two half-height rows fit the usable height including their intervening gap. Every member still applies its own minimum/maximum constraints and Area scale. Preserve requested height independently from the resulting pixels; a manual height changes it to `Width::Pixels` rather than overwriting horizontal intent.

The full native floating → retile → free regression exposed a real restoration defect: the rectangle returned to 364×264 instead of its authored 300×200 because the floating resize had replaced the native fixed-size rules. Preserve that red result in `ergonomics-sizing-interface-v5.log` (**190 passing tests, one failure**). The correction extends the existing `Home`/`Member` cache with per-axis `SizeIntent`, retaining only sizing mode and requested size for normal/floating placements. Restoring that intent clamps its size against current rules and never restores obsolete client minima/maxima. The existing ToggleFloating action synchronizes the restored rules to the same root's native `LayoutBox`; no duplicate runtime cache or application-specific branch is added. Free-mode configuration/geometry changes update that same authored home, and transfers carry the complete existing home/member state. The correction and independent Fit/Fixed/client-limit/restart case still need the current rerun.

The implemented entry points are:

```rust
DesktopAction::Resize(Direction::Right).apply(world, root);
DesktopAction::CycleRowHeight.apply(world, root);
```

Their solver operations, used by the controller rather than dispatched a second time, are:

```rust
model.resize_focused(0, 64.0, usable, canvas.zoom)?;
model.cycle_row_height(&layout.height_presets)?;
```

`LayoutSettings`/`Document` add `height_presets`, initially full/half/third/two-thirds, and an independent `resize_step`, initially 64 logical pixels and bounded to 1–10000. They round-trip through the same `lince.interface` Extension and atomic commit as widths and bindings. The existing Window layout modal contains both ordered lists, each with Add/Remove/Up/Down and its own Reset, plus the resize-step field. Width Reset retains heights and resize step; row-height Reset retains widths, opening preferences and resize step. Reuse the existing field validation, one scroll owner, keyboard modal ownership, stale-origin check and Save/Cancel. Saving preferences alone does not resize existing items or execute an action. No additional layout mode, toolbar or private Park binding table is introduced.

The verification fixtures cover model rollback, exact fractions and gap fitting, constrained clients, independent axes, floating-home restoration, retained draft/editor identity, duplicate heights, invalid steps, independent Reset, saved bindings and startup persistence. The expanded wide/narrow Window layout fixture uses actual typing, wheel scrolling and pointer controls; the real Wayland-client fixture checks configure/ack/buffer sizes, unchanged neighboring output content and live surface identity. Keep a failed or unrun fixture explicit and retain U11's optimized/installed/input acceptance after these focused checks pass.

External actions reuse `launch::Description`, `Commands`' bounded local park.json parsing/private atomic writes and `Server::spawn_helper`. Their command bindings remain in interface.lingua; executable/argv/provider choices remain local to this host. First expose the accepted general concepts: brightness, audio output/microphone, media playback, full-screen/region screenshots and editor launch. Every action invokes a replaceable literal argv description; the initial tools in this user's profile are fixtures rather than executable-name checks. Expose descriptor editing and unset/error status in Park, using retained native fields and Save/Cancel. Editing an action or capturing its shortcut must not run it. Preserve unrelated keyboard, pointer, output and provider settings when saving.

The implementation places portable `Description` and `Capability` data in `interface::host::launch`, re-exported by Park's server-enabled launch module. Process creation stays in Park's `Application::from_description`/`Server::spawn_helper`; the interface gets no process-launch implementation. `host::external` owns the twelve initial typed actions, their stable command IDs, repeat/lock eligibility, bounded configuration and request/snapshot types. Settings acknowledgements reuse `InputRequest`/`InputResponse` and the existing local-file transaction. The invocation is `DesktopAction::External(action)`; it contains a registered action identity, with the compositor resolving and validating its locally configured description. There is no execution request carrying caller-supplied argv.

The local configuration shape is:

```json
{
  "external": {
    "volume-up": {
      "command": {
        "program": "wpctl",
        "arguments": ["set-volume", "@DEFAULT_AUDIO_SINK@", "0.05+"],
        "directory": null,
        "capabilities": []
      },
      "allow_when_locked": false
    }
  }
}
```

The example is a replaceable owner choice, not a generic default. Initially support brightness up/down, volume up/down/mute, microphone mute, play/pause/next/previous, full-screen/region screenshot and editor launch. Hardware/editor commands get no privileged Wayland capabilities; screenshot descriptors may request the existing Capture capability. Locked execution additionally requires the action's hardware eligibility and the saved explicit flag. Validate both again in the compositor before launching. Settings changes stop an already-intercepted external key from repeating a newly saved descriptor until release.

Use the existing native dropdown to select an action, retained program/directory fields, separately editable literal argument rows, shared shortcut capture and Save/Cancel. Keep a draft for each selected action when switching the dropdown; selection and edits never execute the program. Preserve empty literal arguments and shell metacharacters without inventing shell quoting. Show unconfigured/unavailable/error state and keep fields/focus after failed or stale saves and late acknowledgements. Reuse the existing scroll, feedback and settings controls, with a wide/narrow rendered transaction test.

The narrow form must also remain visually usable. Shared settings buttons have an explicit content-width column with wrapping captions; rendered checks require positive caption dimensions and laid-out glyphs as well as an accessible action name and a reachable button. Field composition, pending edits and an unresolved paste prevent Save or action selection; Cancel discards the unresolved paste before restoring the active description. Keep these checks in the actual transaction fixture instead of relying on a direct action invocation. Panel anchoring uses `ComputedNode::unrounded_size()` in logical coordinates: feeding its rounded width back into the anchor caused a 338/339-pixel oscillation, repeatedly triggering focus reveal and undoing manual wheel scrolling. The multi-scale anchor regression and real wheel-driven Cancel path must retain coverage for that failure.

Bevy/Winit already supplies named physical audio/media/PrintScreen keys, but pinned Winit 0.30.13 represents Linux brightness down/up and microphone mute as `Unidentified(NativeKeyCode::Xkb(224/225/248))`: these values are evdev scancodes, without the XKB +8 offset. Preserve those exact physical identities in the shared parser/capture/codec and verify against Winit's converter. Unmodified dedicated hardware/Print keys are valid desktop triggers; ordinary letter/navigation desktop triggers still require their normal modifiers. A separate snapshot view of lock-eligible bindings may be derived from the same registry so a previously open settings modal cannot erase lock controls; it is not another binding table, and unlocked capture must keep precedence.

Carry XF86/media physical key identities through capture, parsing, conflict detection, saved bindings, native conversion and compositor dispatch. Repeat only repeatable controls such as volume/brightness; play/pause, screenshots, logout and editor launch use one distinct press. Allow-when-locked is explicit, default off and limited to the owner-selected hardware-control concepts; it cannot attach to built-in terminal/launcher/edit/capture/automation actions or grant protocol capabilities. Reuse helper authorization, process bounds and cleanup. The compositor dispatch must validate the registered action/descriptor again rather than trusting a caller-supplied executable, argv or locked-state permission. An unavailable provider produces an actionable error through the same command/UI path.

Verify fresh and explicitly overridden defaults, all nine numeric destinations, source/destination rollback, live client content and width preservation, row order/height and monitor changes. Extend the existing real-client input tests for XF86 capture versus execution, held-key repeat, normal/inhibited/locked dispatch, provider spawn failure and literal arguments. Cover the confirmation's keyboard/pointer cancellation and failed-save recovery. Reuse the implemented wide/narrow external-action editor fixture, and include its controls and commands in the existing U11 real-client workflow rather than adding another demo interface.

The external-action feature now passes **201 selected shared-interface tests in 11.05 seconds**, **182 ordinary Park tests in 3.44 seconds**, and the actual wide/narrow external-command transaction/capture GPU fixture in **93.13 seconds** (`ergonomics-external-{interface,park,gpu-debug}-v11.log`). The ordinary Park run explicitly leaves **24 GPU/hardware cases ignored**; the separately executed form fixture closes only its own case. Coverage includes literal empty/metacharacter arguments, private atomic merges, write/revision/late-reply failures, unresolved-paste cancellation, real XKB hardware symbols, physical-key capture/codec identity, Sandbox isolation, typed lock-policy restrictions and literal-argv spawning against a real Unix socket. Fields retain their entities/focus on errors, and binding capture does not execute the configured program.

Both GUI source checks pass with warnings denied: Park in **2m48s**, ordinary Sandbox in **3m40s**. Minimal Park, server-only Park and portable interface checks pass in **11.84s**, **11.94s** and **5.01s**; all **18 validation-tool regressions pass in 0.11s**. The logs are `ergonomics-external-{park-gui,sandbox-gui,base,server,portable,xtask}-v11.log`. Preparation registers `external-command-rendering` alongside the existing settings fixtures. The source checksum list and scoped Rust/manifests/WGSL archive are `ergonomics-external-v11-source.sha256` and `ergonomics-external-v11-rust-source.tar`; the archive is not an installable package.

The frozen v11 release executable passes **all 206 Park tests**: 182 ordinary cases in **1.12s**, seven settings/Showoff GPU cases in **48.66s**, the command-discovery GPU case in **14.58s**, and the remaining sixteen GPU cases in **94.03s**. Logs are `ergonomics-external-{park,gpu,discovery,gpu-remainder}-release-v11.log`. The last partition covers actual private Wayland-client focus, popup/subsurface pixels, buffer remapping, capture, output raster scaling, graphical monitors, overview, source shaders, stacking, tiling and stalled-GPU lifetime. The copied test executables and checksums are retained under `ergonomics-external-v11-{park,interface}-tests` and `ergonomics-external-v11-park-tests.sha256`. They identify this checkpoint before the subsequent transfer-model work; they do not verify later source changes or establish a new installed/physical session.

Cargo attempted a second dependency compilation for the ordinary release run despite the unchanged scoped source checksums. That redundant compilation was interrupted; ordinary and remaining GPU cases ran through the checksum-recorded release executable in the same Nix environment. Direct test invocation must set **RUST_MIN_STACK=8388608**, matching `.cargo/config.toml`; the first direct shared Operation run omitted that setting and aborted in an existing large backend future. Its corrected eight-case run passes as recorded under U1. Do not rerun passed GPU cases merely to reproduce Cargo's compilation step, and do not treat this copied executable as an installed GUI candidate.

Keep the rejected evidence explicit: v6/v7 reproduce the narrow panel's 338/339-pixel anchor oscillation; v8 passes button interaction but its inspected captures expose collapsed captions, so it is not visual acceptance. The corrected v11 fixture additionally requires positive caption bounds and glyphs, and the inspected 360-pixel invalid-draft capture has readable wrapped bindings, validation feedback and reachable Save/Cancel. The v9 run was interrupted for that visual correction; v10's new paste test initially omitted the ordinary queued-edit settlement after Cancel, corrected before the passing v11 suite. These are closed software defects, while real-client IME/inhibition/lock dispatch, selected providers, personal defaults, optimized acceptance and installed/physical use remain in U11 and the delivery stages.

### U8. Complete stable scrolling rows and simple everyday window control

Rows now keep per-item widths stable across insertion/removal/reordering and reveal overflow through scrolling on their own output. The shared `item.cycle-width` command, default Mod+R, uses `DesktopAction::CycleItemWidth` and `Workspace::cycle_item_width`; Operation's `/item/width` invokes that same action. The cycle is **1 → 1/2 → 1/3 → 2/3 → 1**, with no key auto-repeat. A half-width item advances to one third, two thirds, full, then half. Arbitrary/manual sizes and removed presets start at the first entry. Ratios are compared by exact intent, so client minimums producing equal pixel sizes cannot trap the cycle. Maximize restores normal state and advances from full; fullscreen reports that it must be left first. Height, row membership, neighbors and client identity remain intact.

**Park → Layout · Window layout** opens the implemented native modal with percentage/fraction fields, Add/Remove, Up/Down, a separate default opening width, gaps, application-opening policy, Save and Cancel. Retain this shared U7/U8 editor rather than adding a competing size editor. It initially displays 100%, 50%, 1/3 and 2/3, preserving exact thirds instead of round-tripping approximate percentages. Validation rejects empty/invalid/equivalent presets and bounds the list to 1–32. Save uses U2's atomic Lingua transaction and preserves current bindings; validation/write/revision failures retain the same draft entities, focus and scroll. Changing presets does not resize existing items. Newly managed items use the configured normal-width default, initially half, with the separate opening-state policy applied as U7/U8 specify. Their neighbors retain their widths. The settings summary shows the focused rectangle's current logical width separately from its requested preset, including constrained sizes. The layout and shortcut modals share `KeyboardModalOwner`, compositor handoff markers and held-key quarantine on close; native configured bindings, underlying outside-modal binding scopes and compositor shortcuts remain suspended while either owns input.

The interaction contract is one width-cycle command, one press per step, shared by normal application windows and eligible native items. Starting with a full-width item, the accepted default sequence is:

| Separate Mod+R presses | Requested horizontal width |
| --- | --- |
| Before pressing | 1 (100%) |
| 1 | 1/2 (50%) |
| 2 | 1/3 (33⅓%) |
| 3 | 2/3 (66⅔%) |
| 4 | 1 (100%); the cycle repeats |

Holding the chord does not race through the list. Ratios refer to the current output's usable width after bar reservations and gaps, rather than the width of an arbitrary neighboring item. Full width changes horizontal size only; it does not request fullscreen, hide bars or change row height. In free/floating placement, use the existing width solver and retained scale intent instead of tying client resize requests to camera motion. Represent the requested 0.33/0.66 examples as exact 1/3 and 2/3 by default. Users can replace and reorder the fractions in the existing modal; the chord is editable in the shared shortcut editor. The default opening width is a separate preference, initially half; editing the cycle must not resize existing windows. No additional preset buttons or dedicated chord per size are required.

Keep custom input distinct from the default examples: entering `0.33` or `33%` deliberately stores 33/100, while entering `1/3` stores one third. Do not silently convert an owner's decimal choice to the nearest default fraction. Display the exact retained value when reopening the editor, and reject equivalent duplicates such as `0.5` and `1/2` with a useful validation message while preserving the draft. Editing presets configures the cycle; invoking the width command is what resizes the focused item.

Count input presses rather than rendered frames: a complete modifier/chord press and release arriving in one update must still advance once, while two distinct presses in that update must advance twice. OS key-repeat events must not add steps. Exercise this through the shared input bridge and a saved replacement chord, preserving the modifier state associated with each event rather than inferring every event from the frame's final held keys. This is part of the existing command/input implementation, not a separate width-key handler.

When a tiled item grows, retain the other items' widths and reveal the focused item through that row's existing viewport. Do not shrink its neighbors or zoom the entire workspace to fit the row. With no eligible focused item, report the command as unavailable through the same registry/help path and leave the cycle and view unchanged. Rebinding Mod+R must preserve these semantics: the cycle belongs to the typed command, not a hardcoded key handler.

Treat an empty workspace and an explicitly unfocused workspace containing eligible items as separate acceptance cases. In the second case the width action must not select the nearest item, restore a previous focus, enter focus-follow or resize anything implicitly. Check availability again when the typed action executes, so changing focus after opening Operation cannot apply a stale resize target. Preserve the requested width intent, camera, item rectangles and explicit Unfocus state on rejection; return the same useful reason to the button, shortcut and Operation entry points.

The cycle follows the item's requested preset, not its closest measured pixel width. The following cases define the command's state transitions and its integrated acceptance:

| Starting state | Result of one width-cycle press |
| --- | --- |
| New item with the default half-width intent | Request one third, the next entry in the configured order |
| Manual horizontal resize, with no retained preset | Request the first configured entry, initially full width |
| Manual vertical resize or position-only movement | Advance from the retained horizontal preset; neither action resets it |
| Current preset removed from the saved list | Request the first entry in the new list on the next press; saving the list itself leaves geometry unchanged |
| Current preset retained but reordered | Find that exact fraction in the new order and advance from it; do not retain an obsolete numeric index |
| Several presets constrained to the same effective client width | Advance the requested fraction once even when the effective size does not change |
| Last entry in the configured list | Wrap to the first entry; a one-entry list remains on that entry |
| Normal state restored from maximize | Advance from full width as specified above, preserving the normal height and row |
| Fullscreen or no eligible focused item | Explain the unavailable action and leave placement, camera and focus unchanged |

Keep the focused-width summary explicit about requested proportion and effective logical size, and test this distinction with a constrained Wayland client rather than changing the preset order to hide it. Existing pure/model checks remain evidence for their covered cases; extend only missing integrated client/startup coverage instead of rebuilding the width feature.

Use a concrete integration fixture to make this distinction observable: with zero gaps, a usable width of 900 logical units and a client minimum width of 450, the requested full/half/third/two-thirds/full sequence resolves to 900/450/450/600/900. The repeated 450 is valid; the next press must still advance to two-thirds. Moving that item to another output retains its requested fraction and resolves against the destination's usable width, preserving its live surface and neighboring items. Restart restores the configured cycle and saved width intent through their existing configuration/workspace owners.

`layout/desktop/widths.rs` resolves rational widths against usable bounds with inter-item gap allowance, so two halves or three thirds fit without cumulative gap error. `Item::free_width` stores optional ratio intent plus the canvas scale at invocation for free/floating rectangles. Re-resolving output/bar bounds preserves that scale and the item's center/height; ordinary camera zoom does not resize clients every frame. Manual width changes clear the preset intent, while movement and representational float rounding preserve it. Floating homes cache the authored and floating width intents separately, alongside their respective geometry. Native layout updates the free rectangle's fixed-width rule so the subsequent authored-layout pass cannot undo the resize. During geometry reconciliation, compare each observed size axis with the last presented geometry: a position-only update preserves a pending model resize and its center, while an actual observed size change takes precedence as a manual resize. This prevents physics or dragging from copying a previous frame's width over a new command. Arbitrary imported scene models remain outside this window-layout path.

Use the implemented shared entry points when extending this work:

```rust
DesktopAction::CycleItemWidth.apply(world, root);
model.cycle_item_width(&layout.width_presets, usable, canvas.zoom)?;
```

The first call is the native command entry; the second is its solver-level operation, not an additional action to dispatch after the first. The existing fixed-column cycle is internal support only. All 62 layout tests and 34 shortcut/configuration/editor tests pass (`ergonomics-widths-layout-tests-v4.log`, `ergonomics-widths-settings-tests-v5.log`), covering constraints, gaps, wraparound, free/floating state, restart persistence, native controls, stale saves, plain Escape from an edited field and suppression through held-key release. The release width-modal GPU test passes actual text replacement, duplicate validation, reordering, saving at 360 pixels, reopening the underlying Park panel and Escape cancellation (`ergonomics-final-gpu-release.log`). Wide and 360-pixel captures were inspected. The edit panel now clamps its width/anchor to the usable viewport even though Edit is the leftmost toolbar control; all 13 edit-panel tests pass (`ergonomics-widths-edit-tests-v3.log`). End-to-end real-client/provider/hardware acceptance remains in U11.

- [ ] Implement **Alt+Tab** and **Alt+Shift+Tab** MRU switching for eligible top-level native items and application windows, across accessible workspaces/outputs. Snapshot order at the beginning of a switching gesture so preview focus cannot reorder the list underneath it. Release Alt to commit and Escape to cancel back to the original still-valid target/view. Skip destroyed/hidden/protected targets, keep focus/follow coherent and avoid changing client sizes for previews. Reuse the overview/window identity data; this is not a new launcher or search feature.
- [ ] Finish opening/closing integration: preserve parent-relative transient placement and explicitly restore parent focus when a child closes. Background launches without a valid activation request must not steal focus or move the camera. Apply U7's opening/maximize preference and existing owner-authored properties through that same path, which U9's rules UI will also invoke. The model already inserts after the focused row member, keeps neighbor widths, and prefers the next visible member on ordinary close, then the previous member or a surviving row; extend that tested policy instead of replacing it.
- [ ] Add simple **Mod+left-drag to move** and **Mod+right-drag to resize** for eligible windows/items, with a single insertion preview for tiled drops. A tiled move previews the destination row/slot and applies once on release; Escape cancels. A tiled horizontal resize changes that item's width, and resizing a shared row edge changes row height deliberately. Free/floating moves/resizes use their normal rectangle. Respect client constraints and current projections, preserve the existing unmodified native Sand edit gestures, and leave ordinary application clicks/right-click menus untouched. Do not add velocity throws, magnetic zones or a second gesture mode.

Keep the reviewed implementation order explicit: suggestion 1's implemented width model/editor and outstanding real-client acceptance, then suggestion 2's MRU switching, suggestion 4's opening/closing and suggestion 5's simple pointer control. Add focused verification alongside each change and collect their combined acceptance under U11. Do not rebuild the completed width feature or wait for every U9/U10 family before validating this core workflow. The following UI and lifetime decisions define the first version:

| Interaction | Required behavior |
| --- | --- |
| Quick recent-window switch | Alt+Tab selects the previous eligible target; releasing Alt commits. Repeating Tab while holding Alt advances, and Shift reverses. With no alternative target, leave the current view intact. |
| Switcher presentation | One compact, scrollable chooser with title/identity and workspace/output context, reusing overview identity, naming, selection and accessibility helpers. Text is sufficient for the first version; reuse an available cached thumbnail only if it requires no additional preview engine. Inactive workspaces remain reachable; minimized items, descendants of minimized parents, hidden group members and protected/unmapped targets are excluded. Preview does not deliver ordinary keystrokes to clients. |
| Switch cancellation and disappearance | Escape restores the original workspace, view and explicit focus/follow state when still valid. If a target disappears, remove its identity from the gesture snapshot and use a deterministic surviving target; never reuse an unrelated entity or surface. Update MRU order on committed selection, not each preview. |
| Opening and activation | Use one placement/activation path for client requests, chosen launch commands and opening rules. Preserve parent ownership for dialogs and valid user-activation context; mapping a background window alone cannot authorize a focus or camera jump. |
| Closing | Request normal application close through the existing protocol; keep native item confirmation/save behavior. Closing a dialog restores its surviving parent, while an ordinary close uses the tested neighboring-item fallback. Closing the last application leaves the authored workspace, Sands and Areas intact. |
| Move preview | Show a single destination outline/slot. Retain source membership until release; a tiled drop reorders/transfers through the same typed actions as keyboard movement. Moving outside the preview does not implicitly float or change workspace layout. |
| Resize | Choose resize edges from the initial pointer position and keep them for the gesture. Free/floating resize edits the rectangle; tiled resize edits the selected item's width or selected row boundary. Respect client limits and preserve other item widths. Persist the accepted result after release. |

Opening policy must resolve U7's open-maximized preference and the separate default width without either continually overriding the other. Reuse the existing Record/assertion resolution for explicit opening rules; do not introduce a second rule-priority system. U8 owns the placement/activation entry point and reads existing properties; U9 adds the missing Window properties/rules controls and routes their results to it. This avoids making core opening/closing depend on a rules editor that in turn depends on core placement. Apply the resulting policy once when a new top-level application is first managed:

1. Respect the surface role and parent relationship. Popups remain protocol-owned popups; a transient dialog uses its parent's placement and focus rules rather than occupying a new ordinary row slot by default.
2. Apply explicit owner-authored opening properties through U9's existing Record/rule path. An explicitly specified normal/maximized/floating state overrides the profile's opening-state default; a rule specifying only a width does not implicitly change that state.
3. Fill unspecified normal geometry from the default opening width, initially half, and unspecified state from the user's application-opening preference. Initial maximization preserves that normal width for restoration. Native Sands retain their authored geometry unless explicitly managed by the corresponding shared layout action.
4. Resolve the final geometry against usable output bounds, row gaps and client minimum/maximum constraints. Preserve the requested width fraction separately from its constrained size. Insert through the existing row transaction without resizing neighbors.
5. Resolve activation separately from placement. Neither a matching rule nor the profile may turn an unauthorized background map into a focus/camera jump. Saved existing placements retain their identity and geometry during workspace restoration; opening defaults are not reapplied every frame, focus change or monitor reconnect.

With 900 logical units of usable width and zero gaps, a new application with a half-width default has a 450-unit normal width. If this user's open-maximized preference applies, it initially occupies the usable maximized area and restores to 450 units on unmaximize. An explicit normal-state rule with a two-thirds width instead opens at 600 units. A client minimum can constrain either normal result without changing the retained fraction. U11 must verify these cases with a real client, including a dialog closing back to its parent and a background launch leaving focus/camera unchanged.

MRU switching must use the registry's configured forward/reverse chords. Alt is the default initiating modifier, not a constant inside the controller: rebinding the gesture changes which held modifier keeps it open and which release commits. Validate that a switching chord includes a usable hold modifier; do not accept plain Tab as a desktop switcher gesture. Capture, settings modals, client shortcut inhibition and session lock retain their existing ownership rules. Keep the first version to the chooser, forward/reverse, commit and cancel; application filtering, search and extra switcher modes are outside suggestion 2.

Validate the forward/reverse pair together: they need a common hold modifier, and Shift used only to reverse direction must not commit the chooser when released. Trigger-key auto-repeat does not race through targets. A quick press/release between rendered frames still selects and commits once. Cancellation caused by lock, modal takeover or lost input ownership closes the chooser safely without replaying intercepted keys into an application. Test these through the same event ownership used by normal shortcuts; do not add a second MRU keyboard table.

Retain per-event ownership for the hold modifier as well as Tab. Winit may notify a modifier release before its following `ModifiersChanged` mask update; a keyboard event's captured mask therefore does not always describe the state after that release. Use the existing logical/Park modifier roles and contributor tracking, extending the shared event context only where the controller needs missing transition information. Do not infer every commit from one final-frame mask or hardcode the physical Alt key names. Two complete switching gestures delivered in one update must remain two gestures, while releasing reverse-only Shift or one of two held contributors cannot commit the active gesture prematurely. Preserve this distinction when rebinding the common hold modifier.

For pointer interactions, reuse the existing Smithay pointer-grab and native item-action paths with one interaction owner. Route motion/release/cancel to that owner until completion; lock, target unmap or output removal must cancel safely. An intercepted modifier-drag must not also start a client click, text selection or context menu, and cancellation must restore the original geometry/membership without stuck buttons. Keep the preview transient; do not write intermediate arrangements to persistence on every motion.

Acquire the gesture from the modifier/button state at the initial press over an eligible managed top-level item. Popups, external bars and other shell surfaces retain their existing interaction ownership. Releasing the modifier before the mouse button does not turn the intercepted press into a client click: retain the grab until button release or cancellation. After cancellation, quarantine any still-held intercepted button until release. A later ordinary right-click must arrive as one complete press/release pair, including its normal popup authorization. Cover this sequence without application-name exceptions.

Keep suggestion 5 small: one target, one move or resize gesture and one destination preview. Reuse existing hit testing, projection and constraints; no new gesture framework, multi-selection, inertial throwing, snapping zones or automatic float-on-detach behavior. The first version moves an item within its current workspace; cross-workspace/monitor movement remains available through the typed commands, with overview dragging added separately in U9. Mouse modifier/button choices belong to the shared configuration/editor as typed pointer triggers, with the same conflict feedback and atomic save; do not encode a mouse button as a keyboard key or create private Park-only bindings. Existing unmodified Sand editing and application context menus remain usable.

The requested cyclic-size interaction is also documented by [Niri's preset widths](https://niri-wm.github.io/niri/Configuration%3A-Layout.html#preset-column-widths): one action cycles configured widths, with a separate default opening width. Park uses the user's requested order and per-item horizontal-row semantics. Keep the first version to the accepted forward cycle; a reverse-cycle chord, fixed-pixel presets and separate keys for every size are not required by this review. The omitted numbered suggestion, searching existing windows through Operation, remains deferred; keep Operation's existing commands and ordinary external launcher integration without building that search.

### U9. Complete the accepted workspace, interaction and configuration families

- [ ] Add configurable desktop touchpad gestures: three-finger navigation of the row/canvas viewport and workspaces, and four-finger overview. Preserve one/two-finger application input, honor instant transitions and provide keyboard equivalents. Give a recognized desktop gesture one owner for its full lifetime; cancellation/VT changes/lock must not leave an application or drag in a pressed state. Coordinate with U8's pointer actions instead of adding another move engine.
- [ ] Extend overview with dragging items between rows/workspaces and hover-to-activate during a real file drag. Use bounded, configurable dwell/edge-scroll timing with a visible target; do not switch workspaces just because the pointer crosses a thumbnail. Keep file-drop delivery distinct from moving an application's window, preserve valid client drag serials, and cancel safely on unmap or output change. Basic overview remains a single entry point without new demo buttons or a second layout chooser.
- [ ] Add optional back-and-forth workspace switching, default off, through U7's shared selection path. Named identities, explicit creation, visible numeric mapping, staged creation of missing destinations 1–9, adjacent navigation and workspace reordering are implemented; do not rebuild them. The saved-restart unit path preserves the reordered names, notes and Areas; keep the installed restart workflow in U11. Reordering changes numeric positions while retaining every workspace/item identity and authored content. Unlike Niri's empty-workspace cleanup, absence of application windows never makes a Lince workspace disposable; never delete or overwrite a workspace to obtain a number. Saved Sands, Areas, models and names survive all normal navigation.
- [ ] Preserve per-output workspace/view/focus affinity. On disconnect, relocate to a surviving output while retaining the original affinity; on reconnect, offer/perform safe restoration without stealing current keyboard focus or duplicating a workspace. Treat a deliberate user move as the new affinity. Serialize this through the existing workspace/output state, not a parallel desktop database. Extend the existing physical hotplug acceptance rather than claiming it from the virtual-output fixture.
- [ ] Expose click-to-focus, optional pointer-follow focus and optional cursor warping as distinct settings, defaulting to click focus with no warp. A pointer crossing a moving component must not repeatedly retarget a locked camera. Respect explicit canvas Unfocus, activation authorization, popup ownership and urgent-window notifications. An urgent app may indicate attention; it must not bypass focus/lock policy to jump the camera.
- [ ] Finish **Window properties / Record inspection** as its own user-facing feature, reusing the current inspector and normal Record mutation path. Resolve the focused window's stable Record identity when the action starts and retain that target while loading or editing; later focus changes must not redirect an asynchronous result or Save to another window. Present application-reported metadata separately from owner-editable assertions/properties. Preserve the editor and draft when the window closes if its Record remains valid, and show a useful missing/unavailable state otherwise. Keep the action in Operation and the shared rebindable registry, initially unbound, without a new permanent toolbar button.
- [ ] Add per-application opening rules for initial workspace/output, size, floating/maximize and focus behavior through the existing window Record/assertion/Area mechanisms and Window properties above. Distinguish one-shot initial placement from continuously applied Area behavior so focus changes do not repeatedly reinsert a window. Application titles/app IDs are matching metadata, not authority to launch programs or acquire privileged protocol capabilities. Avoid a second competing rules engine.
- [ ] Add bounded live reload for external edits of interface.lingua. Watch the directory as well as the selected file so atomic replacement/symlink updates work. Parse and validate the entire proposed configuration, then publish the same transaction used by the editor; invalid input retains the last good active configuration and shows a useful error. Do not overwrite an open draft silently: detect an external revision change and offer reload/rebase/cancel. A write by Park must not produce an infinite save/reload loop or run configuration scripts.
- [ ] Expose command invocation and relevant window/workspace/output change events to external automation through the existing authorized management/host boundaries, with typed command IDs and explicit errors. Add only missing general bindings. Avoid an unrestricted command socket or importing another compositor's API. Use the same actions as the UI, rate/bound event queues, handle disappearing targets by identity and reject privileged actions without the correct scope. Reuse existing launch descriptions for external commands.

### U10. Complete the accepted capture, window utilities, accessibility and hardware families

These families were previously shown as later possibilities; the user has now asked to put them in the plan. Implement them after the core workflow. Split local capability implementation from provider/hardware acceptance, and retain an unmet item instead of marking it complete merely because its control is hidden. New default shortcuts are unnecessary unless listed in the accepted defaults; expose occasional features in Park/Operation and the relevant settings.

- [ ] Complete external screenshot and monitor/window sharing integration with a visible active-sharing indicator and per-window capture-exclusion setting. Reuse the selected portal/PipeWire/capture provider and existing authorization. Enforce exclusion in every supported capture path, not just in thumbnail UI; output composition must mask excluded content where required. Define capture of popups/transients and interaction with lock/output changes, and clear indicators when the provider disconnects. Lince does not become a replacement portal or recording service.
- [ ] Add **windowed fullscreen** as a distinct application action: the client receives fullscreen state while its compositor-controlled rectangle remains bounded. Expose its active state and restore the previous state/geometry on exit. Keep normal fullscreen, maximize and tiling width independent, and handle a client declining/changing its configure without looping. This supports presentation workflows without a browser-specific implementation.
- [ ] Add explicit compact/tabbed groups inside a normal row slot: several native/application items share the slot, one is visible, and selection/close/reorder remain deterministic. It is grouping within the single tiled arrangement, not a workspace Columns/Tabbed layout switch. Respect per-item constraints, modal children, focus, capture and lifetime; a hidden member must not continue receiving ordinary input. Keep original identities/content and restore individual placement on ungroup. No new permanent toolbar button is required.
- [ ] Add user-selected scratch windows as a general hide/show utility for an existing terminal/calculator/other item. Preserve a home workspace/rectangle and explicit ownership while shown on another workspace; do not duplicate the live surface, relaunch it unnecessarily or auto-assign all floating windows to scratch. On target close remove the stale entry; lock blocks reveal. A chosen external launcher/supervisor still owns launching/services. This utility was explicitly present in the accepted useful-family list; no PaperWM-specific minimap, scratch-layer key scheme or shell dependency is adopted.
- [ ] Finish keyboard/screen-reader accessibility for Park's shell controls, overview, switcher, configuration errors and confirmation flows using existing Bevy/AccessKit and the host accessibility bridge. Test the chosen external reader in a real Park session without registering a competing global bridge in nested Niri. Add output-level desktop magnification distinct from canvas/world zoom; transform input/overlay/cursor mapping coherently and preserve camera-follow invariants. Keep text labels and keyboard recovery usable at high scale.
- [ ] Extend output settings with rotation, VRR and supported color/HDR controls using Smithay/backend/driver capabilities and the existing test/apply/rollback path. Do not assume advertised DRM properties mean the full color pipeline is correct. Preserve per-output color/format state through composition, surface import, capture and hotplug; validate with actual supported hardware. Capability-gate controls and report unavailable features, leaving unsupported/unverified requested work explicit. Nightlight/color-provider integration remains external rather than a hardcoded service.
- [ ] Bind laptop lid, idle, output power-off, suspend and wake policy to the user's selected external power/idle services. Park supplies only missing device/output/session adapters and safe recovery. Do not implement a battery daemon, locker, login manager or supervisor. Retain a usable output when the lid closes with an external monitor and verify input/scanout restoration on wake through the physical acceptance path.
- [ ] Extend supported touch/tablet/peripheral mapping through the existing input backend, with output selection/calibration and usable pointer/button/gesture routing. Expose only supported device settings and preserve application ownership/grabs. Keep software fixtures for event invariants separate from actual-device acceptance; do not claim hardware support based on the absence of crashes.
- [ ] Reuse existing Lince theme/shader controls for optional compositor borders, shadows, blur and transitions; add only missing general rendering bindings and budgets. Defaults preserve instant transitions and accessible focus feedback. Effect toggles cannot alter layout, revive removed demo chrome or introduce relative camera-follow lag. Measure extra damage/GPU/memory cost in release mode, respect reduced-motion choices and keep lock/capture rendering correct. This is the accepted visual-effects family, not approval for Wayfire-style effect/plugin architecture.

### U11. Add focused acceptance coverage and simplify the handoff

Input acceptance must include a key arriving in the release frame of a click that opens Overview, Operation, a width modal or binding capture, from both a real Wayland application and a native Sandbox text editor. Park's tracked handoff uses `KeyboardModalTrigger` rectangles; the native capture buffer currently recognizes the narrower `CaptureTrigger`. Verify the general ownership transition rather than assuming the capture-only fixture proves every modal. Preserve IME composition and held-key quarantine, and deliver permitted buffered text only to the new owner without replaying already-read raw keyboard messages. Operation and Window properties now join the existing typed-action barrier in `nested/input.rs`; test rapid typing and asynchronous inspector readiness through the actual runtime bridge. The shortcut GPU regression also keeps a capture button stationary while the pointer is held; focus-reveal scrolling must wait until release rather than move a pressed control away from its pointer.

The native editor's end-of-quarantine behavior is implemented and verified below: after the last swallowed held key is released, a later permitted fresh key reaches the restored owner once. Retiring quarantine cannot replay the captured chord, turn its release into a client click or leave an unrelated new chord blocked solely because the batch began with a swallowed key. Keep the remaining real-client/IME and lock/runtime handoffs in the integrated acceptance below.

The native editor part now processes interception per event instead of retaining one blocked flag for an entire batch. Save/Cancel quarantine ends on the matching final swallowed release, and a later fresh key retains its normal physical pressed state and enters the existing prepared-event dispatcher once. Enter validates the candidate present at that event; later keys cannot overwrite it before validation. Plain Escape applies the owning modal's existing cancel action at its event, preserving IME precedence. These transitions temporarily publish the current Editor resource so ordinary actions and `release_keyboard` see the correct held-key ownership:

```rust
world.insert_resource(editor);
actions.run(world, target);
editor = world.remove_resource::<Editor>().unwrap_or_default();
```

Discard any earlier, not-yet-forwarded prefix when its modal is cancelled or its capture owner changes; it must not be replayed into the restored canvas. Keep only the permitted tail for `keyboard::replace`, and reset only still-swallowed physical keys rather than every held key. Activation-frame buffered input may continue after validation/cancellation when capture was actually confirmed; buffered input for an unconfirmed activation remains blocked. Retain the shared capture and action paths, not a second modal dispatcher or event replay engine.

The red v1 tests reproduce both defects: the fresh key after quarantine was dropped, and a later B replaced the F2 candidate before Enter validation (`ergonomics-handoff-interface-red-v1.log`). Three corrected unit cases cover Save/Cancel, retained physical state, immediate Enter validation, modal Escape, prefix isolation and no later replay. All **188 selected shared tests pass in 10.66 seconds** on v2 and **10.80 seconds** on the current v3 (`ergonomics-handoff-interface-{v2,v3}.log`). The actual pointer-driven GPU shortcut fixture also queues F2 down/up, Enter down/up and fresh B down/up in the capture click's release frame, asserting the F2 draft and exactly one ordinary B delivery. Both current shortcut/width rendered scenarios pass in **49.18 seconds** (`ergonomics-handoff-gpu-debug-v3.log`), including their wide/narrow controls; the current 360-pixel shortcut capture was visually inspected. All **177 ordinary Park tests pass in 3.34 seconds**, with the other **23 hardware/GPU tests explicitly ignored in that ordinary run**. Both current GUI checks pass with warnings denied: **11.91 seconds** for Park and **8.61 seconds** for Sandbox (`ergonomics-handoff-{park,sandbox}-gui-v3.log`). Real-client, IME, lock/inhibition and asynchronous inspector/runtime handoffs remain separate unfinished acceptance.

The unchanged current source passes `ergonomics-handoff-v3-source.sha256` after the checks. A scoped Rust/manifests/WGSL snapshot is retained as `ergonomics-handoff-v3-rust-source.tar` with its checksum, and `ergonomics-handoff-v3-test-executables.sha256` identifies the tested executables. That source snapshot omits assets/toolchains and is not an installable source package; the test executables are not installed GUI candidates. The earlier optimized pacing failures and installed/physical acceptance remain open.

- [ ] Extend the existing capture/configuration tests for U4/U7's newly registered commands, external actions, XF86 keys and actual XKB labels, including Brazilian layout and shifted symbols. Verify real Wayland-client/IME isolation, held-key repeats across a save, stale-frame click handoff, shortcut inhibition and lock entry/exit in nested Park. Test rebound directional aliases inside overview: dropping a desktop chord's modifiers must not let its trigger key replace Enter/Space/Escape or create ambiguous arrow/navigation bindings. Keep those modal controls usable and fall back to arrows when a custom alias collides. Restart actual Sandbox and Park with the same saved interface.lingua; startup/recovery/atomic-save unit regressions already pass. Add exclusive-context reuse tests only when such contexts actually exist.
- [ ] Extend the passing row navigation/reordering, edge, free↔tiled restoration and individual floating/retile checks to end-to-end real clients and the remaining collision-free defaults/monitor transfers. Add focus/follow coverage for different render/simulation rates, changing Area effects, spatial monitor handoff and real-client output reconfiguration. Basic native/Wayland-client 2D/3D pixel stability, unchanged client sizes, keyboard/pointer Focus/Unfocus, target unmap/remap and selected-button handoff already pass their focused checks; preserve them when extending coverage. Exercise overview entry/cancel/selection without client resize storms or keyboard leakage.
- [ ] Extend the passed output test/apply/confirm/revert, expiry, cancellation and stale/hotplug fixtures to real application/popup/native-editor/bar interactions during configuration and rollback. Verify physical coordinate mapping, shader pixel demand/extent/aspect/cleanup and release performance under tiling and monitor changes. The rendered monitor editor and virtual backend are already covered under U5; keep actual KMS, protection and physical visual acceptance in the existing foreground-session handoff.
- [ ] Extend the passing Edit-first, Showoff-removal, toolbar factory/reparenting, Overview and narrow-modal checks to output scale changes and the installed ordinary workflow. Verify accepted profile bindings, XF86 actions and unavailable-provider errors without changing Niri's session environment. Obsolete alternative-layout/stack choices are already absent from Operation, the shortcut registry and edit-panel discovery.
- [ ] Finish integrated width acceptance through actual client input and the installed workflow: maximum constraints, bar/output updates, fullscreen/maximize restoration and unchanged neighbors/content. The private-client transfer case now passes configure/ack/buffer changes, minimum-constrained half/third widths, exact requested cycling and retained pixels/identity across workspace/monitor transfers. Verify four distinct physical presses follow the configured cycle, holding the chord does not advance repeatedly, preset editing/reordering survives restart and full width preserves bars/height. Include full chord press/release and two distinct presses batched into one update; repeat through a saved replacement chord. Check that growth scrolls the row without resizing neighbors, and verify the unavailable/no-focus case leaves the view and width intent unchanged. The shared ratio/cycle/gap/persistence/draft tests and preceding release wide/narrow modal fixture already pass. Cover the remaining opening/close activation work, MRU quick-switch/reverse/commit/cancel/unmap, hold-modifier versus reverse-modifier release, minimized-parent eligibility and simple pointer move/resize preview/cancel, including normal right-click behavior, cancellation on lock/unmap/output removal and no stuck buttons.
- [ ] Cover U9's gesture cancellation, overview file/window drag distinction, missing numeric destination, named-workspace persistence, monitor affinity, focus/urgency policy, opening-rule precedence, configuration reload races and bounded authorized automation. Cover U10's capture exclusion/indicator teardown, compact-group and scratch lifetime/input isolation, windowed-fullscreen restoration and accessibility. Add actual selected-provider/hardware acceptance for magnification, rotated/VRR/color outputs, lid/wake and peripherals. Missing hardware/provider evidence remains unfinished work, not a fabricated pass.
- [ ] Verify Window properties uses the focused window's stable Record identity, saves assertions through the normal editor and distinguishes live application metadata from owner-edited properties. Change focus while the inspector is loading and while a draft is open; neither transition may change the edited target or the destination of Save. Keep an open valid Record usable after its window closes, report unavailable Records clearly and confirm that property edits neither launch programs nor grant protocol permissions.

Validation preparation includes shortcut/layout/output/toolbar unit checks and release GPU fixtures for shortcut editing, tiling controls, width settings, camera following, monitor navigation, overview, graphical monitors and output raster composition. It now also registers `effect-source-rendering` for the complete optimized shader suite, alongside `effect-snapshot-policy` and the shared surface allocator tests. U4 records the preceding 152-test overview suite, U5 the 179-test graphical-monitor baseline and U6 the completed **181-test optimized raster baseline**, 219 shared-interface regressions and both GUI source checks. Eighteen validation-tool regressions passed after registering `output-raster-rendering` (`ergonomics-raster-xtask-tests.log`). U6 separately records the later 53-test source/consumer check; the raster baseline does not verify code added afterward. Captures from automated preparation belong to each run's native-captures directory. A test executable is not an installed GUI candidate, and isolated popup fixtures do not substitute for U8's complete real-client/provider workflow. Park and ordinary Sandbox source checks establish integration; installed-session and release pacing acceptance remain separate.

- [ ] Regenerate the installable release candidate and physical Showoff fixture after the remaining core implementation/checks. The new physical handoff exercises Edit, Focus/Unfocus, Tiling, Overview and 2D/3D through the ordinary bottom-right controls and their shortcuts; then edit a binding, resolve a conflict, change/revert a monitor arrangement, follow a moving component and inspect sharp tiled WGSL. Use no demo jump/reset button as a substitute for usable navigation. Collect logs/captures automatically and distinguish observed passes from unperformed steps. The optimized test executable above does not replace the installed artifact or validate unimplemented controls.

### Accepted command defaults

Confirmed requests take priority over Niri's old behavior. `Mod` means Super on this Linux setup. The following defaults include the user's acceptance and latest corrections; they remain implementation targets, not live configuration changes. All settings and commands use the same configuration/editor path. Additional utility commands in U9/U10 begin unbound to avoid inventing conflicting default key families.

| Command | Proposed chord | Decision / reason |
| --- | --- | --- |
| Workspace free ↔ tiled | Mod+T | Confirmed; whole workspace, one horizontal-row arrangement |
| Directional focus / follow | Mod+H/J/K/L | Confirmed; same row/between rows in tiling, spatial camera lock in free mode |
| Reposition item in tiling | Mod+Shift+H/J/K/L | Confirmed; reorder in row or transfer between rows |
| Focus monitor left/down/up/right | Mod+Y/U/I/O | Confirmed; replaces today's arrow combinations and displaces Mod+I inspection |
| Overview | Mod+Space | Confirmed; replaces Park's desktop Operation chord |
| Edit panel | Mod+E | Accepted compositor-wide entry; keep Alt+E in shared native canvas scope; leftmost toolbar button |
| Terminal | Mod+N | Accepted from the user's Niri; replace Park's Mod+Enter default |
| Close focused application/item | Mod+Q | Retain; native deletion confirmation still applies where appropriate |
| Maximize in usable area | Mod+F | Accepted from Niri; Park currently assigns this to fullscreen |
| Fullscreen | Mod+Shift+F | Accepted explicit distinction; Park currently assigns this to maximize |
| Cycle focused window width | Mod+R | 100% → 50% → 33⅓% → 66⅔% → repeat; editable ordered presets |
| Recent-window switcher | Alt+Tab / Alt+Shift+Tab | MRU forward/backward; release Alt commits, Escape cancels |
| Operation | Mod+P | Confirmed after the Mod+O conflict review; shared Ctrl+K remains native-scoped |
| External application launcher | Mod+D | Retain; use the selected provider, show an actionable unset status |
| Focus / Unfocus | Mod+Backspace | Stateful toggle and bottom-right button; never interpreted as untile |
| Float / retile just the focused item | Mod+Shift+T | Confirmed; an exception within a tiled workspace |
| Select workspace 1–9 | Mod+1–9 | Retain existing behavior |
| Move focused item to workspace 1–9 | Mod+Shift+1–9 | Accepted; moves the item, not the whole row |
| Move focused item to monitor left/down/up/right | Mod+Ctrl+Y/U/I/O | Accepted Niri family, distinct from focusing a monitor |
| Move focused row to monitor left/down/up/right | Mod+Shift+Y/U/I/O | Accepted Niri-style family adapted to rows; do not alias whole-workspace movement |
| Window properties | Unbound initially | Existing application Record inspection with a clearer label; no dedicated button |
| Lock | Mod+Escape | Retain current action; selected external locker is still required |
| Saved logout | Mod+Shift+E | Accepted from Niri; replace Park's less discoverable Mod+Shift+Escape and show confirmation |

Keep whole-workspace-to-monitor, additional row-height presets and numeric resize commands discoverable in Park/Operation even when initially unbound. The accepted horizontal-width cycle remains bound to Mod+R; it is not one of those unbound additions. Row membership changes through the agreed directional move commands, without a separate exposed column/stack mode. Fullscreen is useful for games/video; maximize preserves bar-reserved space. Individual floating permits a temporary exception inside tiling. Workspace/output moves avoid reconstructing a work arrangement by hand.

**Window properties**, currently called application Record inspection, opens the Lince Record associated with the focused native application window. It allows inspection/editing of that window's Lince identity, assertions and stored properties using the existing Record inspector; Areas can use supported Record predicates. It does not inspect a web page's DOM, read an application's internal documents or automate its controls. Keep the existing advanced action available in Operation/the ordinary inspector, with no standalone toolbar button or default shortcut. A helpful example is tagging a browser window as a work item so Lince can organize it using its own Record/Area mechanisms. Unsupported or disconnected Records need an actionable state rather than inventing application data.

This inspector is approved scope. Reuse the current Record editor and mutation/validation path, keeping application-reported identity metadata distinguishable from owner-edited assertions and properties. If the live window closes while inspected, retain the Record editor when the Record remains valid and clearly indicate the missing live window. Editing properties must not implicitly launch an application or acquire additional compositor permissions. U9 owns the remaining properties/rules integration; this is not a separate application-inspection engine.

Read-only inspection of `/home/user/.config/niri/config.kdl` informed the accepted U7 settings. These are general capabilities with user-local provider choices, not new mandatory dependencies:

| Niri setting / workflow | Recommendation for Park | Ownership / scope |
| --- | --- | --- |
| Brazilian XKB layout, repeat 35 Hz / 300 ms | Add keyboard layout selection and editable repeat controls in Park; preselect these values in this user's profile only | Park already has repeat configuration; general XKB settings/binding-label integration need verification and possible implementation |
| Animations off | Provide an instant navigation/overview preference; target-follow always has zero relative lag regardless of animation preference | Shared interface preference, not a physics-disable switch |
| Open maximized; zero gaps; focus ring off | Offer explicit opening/layout preferences, but retain a clear selected/focused indicator in overview/editing | Do not globally maximize every native Sand, override Areas or confuse no gaps with no selection feedback |
| Brightness, audio, microphone, media hardware keys | Allow registered external argv actions using chosen brightness/audio/media tools; preserve Niri's current amounts in the user's profile | External `brightnessctl`, `wpctl`, `playerctl`; no hardware-service replacement; support XF86 key identity end to end |
| Media/brightness allowed when locked | Require an explicit per-action allow-when-locked flag, off for arbitrary commands; only chosen harmless hardware actions may opt in | Preserve lock isolation; launcher, terminal, editing and capture remain blocked |
| Print and Ctrl+Print screenshots | Bind external screenshot/selection actions if the chosen provider supports them | Existing capture authorization/portal path, no second screenshot UI stack |
| Workspace navigation/reordering and height presets | Offer next/previous workspace, row/item size presets and resize actions in the command list | Accepted discoverability; leave extra chords unbound; no accidental workspace jumps at a row boundary |
| Touchpad tap | Expose supported pointer/touchpad preferences after checking device support | General input setting, not hardcoded to this laptop |
| Editor launch on Mod+Z | Optional user external-action example | Do not install or hardcode the user's editor |

Do not import Niri's startup list into Park: UWSM and selected providers still own service startup. Its Noctalia start/kill shortcuts, personal PDF shell pipelines, clipboard download command, synthesized `@` workaround and debug activation bypass are user-specific choices, not default Park features. Keep external commands as literal argv descriptions and represent any intentional shell script explicitly as the user's chosen program. Do not weaken activation checks to copy Niri's debug option. A shortcut editor manages registered external actions as well as built-in commands, but does not execute actions while capturing keys.

### Accepted family coverage and deferred scope

The previously reviewed useful desktop families are now requirements, not an unselected comparison catalogue. This mapping makes their implementation and evidence traceable. The sources are interaction references; they do not change Park's Smithay/Bevy foundation or introduce dependencies on another compositor. They follow the selected core workflow rather than expanding suggestion 5 into advanced pointer behavior. Retain three separate completion states for each family: implemented behavior and controls, automated integration acceptance, and any still-required provider or physical-device acceptance. An unavailable hardware capability must not erase its outstanding acceptance or cause a replacement service to be written.

Implement the selected core suggestions in U8 before the additional families. U9's overview dragging builds on U4's controller and U8's interaction ownership; its opening rules build on U8's placement/activation path. U10's display capabilities extend U5's output transaction, and its capture/effect work reuses U6's rendering budgets. Complete each family's backend binding, existing UI destination and relevant checks together. Where another program owns a service, the task is capability negotiation, the general adapter if missing and integration testing with the user's selected provider. Adding a family does not authorize building its external service from scratch.

Within U9, finish Window properties before its opening-rule controls, preserving the stable inspected Record identity across focus changes. Within U10, use the existing output/capture/input capability boundaries before adding controls for dependent effects or hardware. Follow the listed family order where there is no dependency; record a capability-dependent acceptance gap against that family while continuing independent feature work. Keep the general adapters reusable and the user's selected programs as the initial fixtures.

| Accepted family | Implementation / acceptance owner |
| --- | --- |
| Stable-size scrolling horizontal rows | U8; preserve neighbors and reveal overflow through the viewport |
| Cyclic horizontal sizes and manual resize | U8; Mod+R, ordered full/half/third/two-thirds list, editable in Lingua and Park |
| Recent-window switching | U8; Alt+Tab, reverse, commit/cancel and stable MRU order |
| Predictable opening/closing and simple pointer move/resize | U8; nearest insertion/fallback, transient ownership and one drop preview |
| Touchpad navigation and overview gestures | U9; cancellation and client gesture isolation |
| Drag between workspaces in overview | U9; distinguish window movement from real file drag and activation dwell |
| Workspace names, navigation, reordering and on-demand creation | U7 implements the stable mapping, adjacent commands and saved order/content; U9 retains optional back-and-forth; U11 retains installed restart/input acceptance |
| Monitor affinity/reconnect restoration | U9 plus existing physical recovery stage 6 |
| Configurable focus policy and cursor warping | U9; safe click/no-warp defaults, no camera oscillation |
| Window properties / Record inspection | U9; stable target, existing editor and owner assertions; U11 covers loading/focus/close races |
| Application opening rules | U9; existing Lince Records/assertions/Areas, no competing rule engine |
| External-file configuration reload | U9; atomic/revisioned transaction with last-good state and draft conflict handling |
| Automation/event access | U9; reuse authorized protocols and typed commands |
| Capture privacy and sharing indicators | U10 plus chosen external provider acceptance in stage 5 |
| Tabs/compact groups, scratch windows and windowed fullscreen | U10; occasional commands within the existing tiled/free model, no extra global layout picker |
| Accessibility and desktop magnification | U10; native semantics plus actual external-reader/output tests |
| Rotation, VRR and supported color/HDR | U10; extend existing output transactions, capability checks and physical evidence |
| Lid/idle/power/wake integration | U10 plus physical stage 6; selected services remain external |
| Touch/tablet/peripheral input | U10; backend/device capabilities and actual input tests |
| Presentation effects | U10; reuse Lince themes/shaders, instant default and measured budgets |

Expose each accepted family through an existing destination rather than adding another permanent desktop toolbar. Workspace names/number mapping, switching and grouping/scratch actions belong in the normal workspace/item controls and Operation; focus policy, gestures and device preferences belong in Park input settings; monitor affinity, rotation/VRR/color and magnification use the relevant display/accessibility settings; rules and capture exclusion use Window properties; sharing status uses the existing status/notification surfaces. Configuration reload belongs to the current configuration transaction and error UI, while authorized automation is a backend binding to the same commands. Provider-dependent controls must explain an unavailable capability and the required external choice. These are UI destinations for U9/U10, not extra desktop implementations or additional default key families.

Preserve the two execution modes when assigning each family's owner:

| Boundary | Required behavior |
| --- | --- |
| Shared interface / Sandbox and Park | Rows, width presets, eligible native-item focus/MRU, workspace navigation, overview, Record editing and interface configuration use the same model/actions and native UI. Sandbox gestures apply inside its own canvas and cannot intercept unrelated host applications. |
| Park compositor | Global application-window management, monitor/device settings, host-level focus/input routing, scratch/group visibility and output composition use Smithay plus the existing host bridge. Sandbox exposes only capabilities supplied by its current host; it does not claim control of that host's windows or monitors. |
| External provider / hardware | Screenshot/sharing, accessibility reader, hardware controls and session/power services use the chosen general provider/protocol. Test available capabilities and keep missing integration or physical evidence explicit; no service is reimplemented to fill an unavailable provider. |

For shared features, verify both a native Sandbox item and a real Park Wayland client where applicable. For compositor-only features, verify that Sandbox keeps working and presents capability availability accurately. These checks extend U11 rather than creating another implementation stage or a separate copy of each interaction.

For each family, first identify the existing Lince action/model, Smithay facility or external protocol/provider that owns it. Implement only the missing shared behavior and its binding/UI, then verify the complete interaction. A registered command or visible settings control alone is not completion; neither is an unavailable provider a reason to implement that provider inside Park. Keep outstanding provider/hardware acceptance in the corresponding checklist while removing only the feature work and tests that have actually passed.

Niri documents [cyclic preset widths](https://niri-wm.github.io/niri/Configuration%3A-Layout.html#preset-column-widths), [MRU switching](https://niri-wm.github.io/niri/Configuration%3A-Recent-Windows.html), [overview](https://github.com/niri-wm/niri/blob/main/docs/wiki/Overview.md), [pointer/touchpad gestures](https://github.com/niri-wm/niri/wiki/Gestures), [workspace lifecycle](https://niri-wm.github.io/niri/Workspaces.html), [input/focus policies](https://niri-wm.github.io/niri/Configuration%3A-Input.html), [opening rules](https://niri-wm.github.io/niri/Configuration%3A-Window-Rules.html), [IPC](https://niri-wm.github.io/niri/IPC.html), [capture/presentation](https://github.com/niri-wm/niri/wiki/Screencasting), [tabs](https://niri-wm.github.io/niri/Tabs.html), [accessibility](https://niri-wm.github.io/niri/Accessibility.html), and [output settings](https://niri-wm.github.io/niri/Configuration%3A-Outputs.html). The width and MRU references were rechecked for this review: Niri provides a configurable width-cycle action and a separate opening-width setting, plus forward/reverse recent-window switching. Park retains the requested full/half/third/two-thirds order, per-item horizontal rows and its existing configuration/editor path. These references do not approve Niri's extra fixed-width presets, reverse width cycling or application-filtered switcher modes. Park's exact defaults, shared interface model and scope follow the decisions in U1–U11.

Explicitly deferred: existing-window search through Operation (the omitted numbered suggestion 3), the additional PaperWM/Scroll/newm/Wayfire-inspired minimap/label-jump/bookmark/filter ideas, camera back/forward history and arrangement undo. The scratch-window utility remains planned because it was already included in the accepted useful-family list; no other desktop's scratch-layer UI/key scheme is adopted. Do not revive demo jumps or reset buttons to implement these utilities.

XWayland remains excluded by the native-Wayland-only contract. Login, bar, launcher, locker, notification, power/idle, IME, clipboard and portal services remain user-selected external software. Do not auto-delete authored Lince workspaces, add a second layout chooser or move Spacematter development into this plan.

## Remaining delivery work, in implementation order

After the usability pass, the eight stages below retain the delivery work and acceptance. Each stage records its decisions, affected code, examples and exit criteria. Examples marked **proposed** are implementation targets, not interfaces that already exist. Shell examples are instructions for the later implementation/acceptance run; none are executed by this planning update. Keep code examples free of embedded comments and keep future configuration fields distinct from today's schema.

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

- [ ] Regenerate and accept the optimized GUI, binary package and Nix source package from the final reviewed implementation. Verify that the package input contains every intended new workspace crate/module and literal embedded asset: the current worktree includes untracked implementation files that an ordinary Git-backed flake input omits. Use a reviewed filtered source snapshot when necessary, preserving unrelated changes and excluding owner data, build output and Git internals; do not stage unrelated work to make a package evaluation pass. Record the exact source/payload hashes and build info, then repeat the outside-checkout installed workflow. The earlier package results below establish the delivery foundation, not acceptance of the newer input/ergonomics code.

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

The frozen external-command v11 release checkpoint measures **51.85 FPS flat** (19.29ms average, 22.78ms p95) and **42.11 FPS spatial** (23.75ms average, 29.53ms p95), over 120 uncapped frames each. Reports are `ergonomics-external-captures-release-v11/showoff-{flat,spatial}-timing.json`. Scope is native update, GPU rendering and fence completion, with no physical scanout or real browser video. The spatial result fails the existing **45 FPS stress floor**; all 206 correctness tests passing does not close that failure, erase earlier misses or establish ordinary refresh-rate pacing. This measurement precedes the new transfer-model implementation and does not replace the installed quiet matrix below.

The earlier usability release Showoff fixture measured 120 frames per projection: flat throughput was **54.17 FPS** (18.46 ms average, 21.51 ms p95) and spatial throughput was **49.00 FPS** (20.41 ms average, 24.79 ms p95). Reports are `ergonomics-final-captures-release/showoff-flat-timing.json` and `showoff-spatial-timing.json`. They include native update, rendering and GPU-fence completion without physical scanout or a real client/video workload. These historical short measurements neither establish the current artifact's performance nor replace the quiet matrix below. U5/U11 record the current passing correctness suite; the package evidence below belongs to the preceding delivery artifact.

Quiet release baselines at the real 1366×768/60 Hz parent viewport measured ordinary windowed 52.62 FPS (p95 35.30 ms), fullscreen 53.65 FPS (34.55 ms), and two-output Showoff 43.70 FPS (38.56 ms). Bounded background GPU fence waits improved those results to 55.47, 56.13 and 45.95 FPS respectively; frame-interval gates still fail. A two-frame pipeline experiment measured 56.03, 56.48 and 43.45 FPS and was reverted because it regressed Showoff. Evidence is `quiet-matrix.json`, `quiet-completion.json` and `quiet-pipeline.json`, without competing compiler work. Client-import timing remains available for further profiling. All 145 current Park release/GPU regressions pass; final stamped-artifact frame-pacing measurements remain pending. A 30-second cycle profile of the actual final host-packaged Showoff/video session captured 1620 samples without loss (`final-active-perf.data`, `final-active-perf-flat.txt`). ECS query construction accounted for 5.65% of self samples and token-style kind/base resolution for 5.72%; these are measured candidates for CPU investigation, not proof that they dominate frame time. That profile ran alongside compilation and does not establish quiet FPS or physical scanout.

The final host-packaged release completed the four-workload matrix three times, without Cargo/Rust compiler processes during measurement. All 12 functional runs passed, but every pacing gate failed. `quiet-final.json` retains each 10-second warmup, 60-second measurement, frame identity, stage timing, outlier and artifact reference. Physical presentation remains null in these nested runs.

| Final nested workload | Fresh FPS range, 3 runs | p95 interval range | Gate |
| --- | --- | --- | --- |
| Ordinary windowed | 54.32–55.25 | 34.03–35.09 ms | Failed |
| Ordinary fullscreen | 55.70–56.02 | 33.45–33.62 ms | Failed |
| Ordinary, two outputs + bar | 53.42–54.08 | 34.22–34.64 ms | Failed |
| Showoff, two outputs + bar | 44.87–45.20 | 38.18–38.64 ms | Failed |

The 44.87 FPS Showoff sample remains a stress-floor failure, as do the preserved older misses. Ordinary client-import medians of approximately 3–4 ms and native update medians around 9–10 ms are measured profiling leads; two-output export also costs about 7 ms. The next physical trace must establish actual scanout and correlate these costs before choosing further rendering changes. The existing physical runner records kernel page flips and fresh client content separately; these nested failures are not converted into a physical pass.

The preceding monitor/focus release test executable also recorded 120 uncapped Showoff frames per projection: **54.25 FPS flat** (18.43 ms average, 20.94 ms p95) and **45.07 FPS spatial** (22.19 ms average, 28.57 ms p95). Evidence is `ergonomics-monitors-captures-release/showoff-{flat,spatial}-timing.json`. Those measurements include native update, GPU rendering and fence completion, without physical scanout or a real browser-video workload. They do not replace the installed quiet workload matrix, erase prior misses or close the refresh-rate acceptance gap.

The later completed overview build records **52.77 FPS flat** (18.95 ms average, 22.51 ms p95) and **44.74 FPS spatial** (22.35 ms average, 29.09 ms p95) over the same 120-frame scope (`ergonomics-overview-captures-release/showoff-{flat,spatial}-timing.json`). The spatial result misses the existing 45-FPS stress floor; retain that failure and the separate refresh-rate/quiet installed-workload gates. Passing the rendering correctness suite does not close performance acceptance.

The current completed graphical-monitor release suite records **50.00 FPS flat** (20.00 ms average, 26.30 ms p95) and **37.28 FPS spatial** (26.82 ms average, 32.93 ms p95), again over 120 uncapped frames per projection (`ergonomics-displays-captures-release/showoff-{flat,spatial}-timing.json`). Spatial pacing fails the stress floor. Keep this result even if later isolated repetitions differ; the 179 passing correctness tests do not make it a pacing pass. Compare isolated optimized samples before attributing the change to a particular feature, then profile the shared update/render/export stages. Physical scanout, real-video and the installed quiet matrix remain separate acceptance.

Three isolated repetitions of that identical optimized executable all pass rendering correctness but still miss spatial pacing: flat throughput is **51.32–52.49 FPS** (median run 51.74) and spatial throughput is **43.19–43.28 FPS** (median run 43.24). Evidence is `ergonomics-displays-performance-{1,2,3}/showoff-{flat,spatial}-timing.json`, with each 120-frame projection scoped to native update, GPU rendering and fence completion. The executable checksum still matches U5. These repetitions distinguish the full-suite sample from repeatable isolated behavior without proving why they differ; retain all failures and profile before attributing a source regression or changing budgets.

A separate diagnostic run with `LINCE_PARK_TRACE_FRAMES=1` passes the same Showoff scenario (`ergonomics-displays-performance-trace.log`). Its summarized benchmark-stage samples are in `ergonomics-displays-performance-trace-stages.json`: flat median native-update is **11.84 ms**, with **5.77 ms** export; spatial is **13.86 ms** native-update, with **8.28 ms** export. Native-main medians are 6.62/8.44 ms and are included inside native-update, so do not add them again. The summary selects the last 120 events per stage preceding each benchmark report's filesystem timestamp; it is diagnostic attribution, not a new acceptance measurement. Trace output adds overhead, and export timing includes the existing GPU copy/synchronization path rather than proving CPU-only cost. Profile repeated native work and export/copy/fence waits before expanding raster demand or removing any lifetime/synchronization guard.

The completed optimized raster baseline records **50.79 FPS flat** (19.69 ms average, 23.30 ms p95) and **45.36 FPS spatial** (22.05 ms average, 28.56 ms p95) over 120 uncapped frames per projection (`ergonomics-raster-captures-release/showoff-{flat,spatial}-timing.json`). The spatial sample clears the stress floor in this run; it does not erase the preserved misses, establish repeated pacing or meet the ordinary-workspace refresh-rate gate. Scope remains native update, GPU rendering and fence completion, without physical scanout or real browser video. These measurements belong to the raster executable above, before the later frozen-frame/source changes.

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

- [ ] Handle the last physical output disappearing as a recoverable headless state. The server can disconnect all outputs, but Desktop publishes an empty output catalogue to the canvas worker and `Presentation::outputs` currently rejects an empty list. Preserve the Cell, windows, workspace/view identities and confirmation recovery while presentation/input to missing outputs is suspended; reconnect must restore a valid assignment without recreating applications. Do not advertise a fabricated physical monitor to work around the worker error. Add an isolated disconnect-all/reconnect lifecycle regression before the physical cable/lid test. This is remaining implementation work as well as hardware acceptance.
- [ ] Run the prepared release TTY test while Niri remains available as the return session. Verify actual scanout/input, default and rebound commands, multiple outputs/scales/modes, transactional output rejection/rollback, connector removal/reconnect including the last output, VT return and suspend/wake. Investigate and fix failures; a watchdog exit is not success.
- [ ] With the chosen external locker, verify protection on every output, locker disconnect/hotplug, and blocked ordinary input/capture. Verify Cell activation against the intended directory, last-client survival, save-aware logout and reopen.
Preparation, protocol probes, captures, installed-binary identification and report collection are automated. The runner additionally requires its stdin to be the actual active Linux TTY reported by logind, rejecting terminal emulators and redirected input. The user still supplies foreground TTY login, physical input/visual observations, passwords/consent, cable changes and wake. A VM may automate virtual DRM/login/session lifecycle checks where its graphics support suffices; it does not establish the user's real GPU, outputs, locking or suspend behavior.

The earlier installed artifact is frozen at `run-1792736145212202998-971866`, exposed through `target/park-validation/physical-test`. It pins the installed Nix closure and hashes the executable, payload and probes. Its private Showoff workspace launches the selected native terminal and an isolated Firefox profile with the local video fixture. The command performs no compilation. The user now reports exercising Park and has supplied the usability/quality findings above; detailed physical logs and the remaining protection/recovery cases have not been audited in this planning update. Re-freeze after implementing the usability pass; the old artifact and its shortcuts cannot validate those new requirements. Detailed fixture limits remain in [Validation handoff](../packaging/linux/park-validation.md).

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
3. Open the selected native terminal and browser through ordinary user commands, not private test shims. Exercise horizontal tiled rows, moving/focusing, unfocusing to the canvas, free placement, resize/fullscreen, popups, clipboard, editing, areas and persistence. Open Showoff to exercise map/WGSL/GLB/physics as an additional workload.
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

Remaining work is the requested usability pass, delivery, general session bindings, demonstrated corrections and acceptance. External desktop services stay external, and future Spacematter development does not gate making Park installable.
