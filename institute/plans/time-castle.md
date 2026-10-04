# Time Castle implementation

## Purpose and agreed behavior

Time Castle combines its existing stopwatch and work logs with a view of upcoming work. The same schedule is drawn as a clock face, a helix in the workspace, and a straight timeline. The present comes from live state. The future comes from cached simulation and individually scheduled tasks.

One complete revolution represents the selected aperture, initially one hour. The future horizon is a separate setting, initially four hours. A two-hour aperture over a four-hour horizon creates two turns. The pointer follows actual clock time in the selected timezone. At 10:10 with a one-hour aperture, minute 8 represents 11:08, rather than an elapsed task at 10:08.

Only current and future geometry is visible. Ongoing intervals are clipped at Now. Date-only tasks appear in an all-day list. Unfinished tasks with expired deadlines appear in an overdue list, outside the geometry. The straight view keeps Now on the left while the ruler and future work advance toward it.

Tasks without duration are thin lines at every aperture. Explicit start and due endpoints define an interval. Otherwise `estimate_min` gives the cost of one occurrence: extend forward from a start or recurring occurrence, and backward from a lone deadline. Quantity does not multiply that cost and does not imply occupied time. Repeated occurrences share the strip with other scheduled tasks.

Collisions are expandable stacks. Selecting an entry emits a scoped `Record selected` event. Record Castles can opt into that event and provide their existing CRUD controls. Time Castle does not need its own task-editing form or drag-to-reschedule behavior.

## Work rules

- Execute the steps below sequentially on the existing branch.
- Preserve other work already in the checkout and inspect shared files immediately before editing them.
- Use `cargo check` rather than `cargo build`; warnings are errors.
- Add no code comments. Do not change AGENTS.md, README.md, `.lingua` files, or the unplugged web crate.
- Keep pure time and layout behavior in the Rust shared model; use the desktop host for workspace meshes, events, subscriptions and persistence.
- Include dependency licenses and credits in Time Castle.
- Update this document as steps finish and record real validation results and remaining blockers.

## Step 1 — Scheduling metadata and Record Castle controls

Status: implemented; scheduling and work-metadata tests passed.

### Domain representation

Add a shared schedule-time parser that accepts an exact `YYYY-MM-DD` calendar date or an RFC3339 timestamp with an explicit timezone offset. Reject malformed dates, implicit-offset timestamps, timestamps outside supported storage bounds and excessive input lengths. A date is an untimed value, not midnight disguised as a timed occurrence.

Extend existing work Start/Due metadata to use that parser. Keep estimates finite, nonnegative and bounded by the existing estimate limit. Compare precise endpoints by absolute instant, so different textual offsets do not affect ordering. Reject reversed complete intervals. Derive timed ranges in one shared function so manual work and simulation use identical precedence.

Preserve the distinct concepts of scheduled time and historical work logs. Existing timer log validation, running entries, totals and start/pause behavior continue to use their current timestamps.

### Editing and query integration

Expose scheduling values through Record queries and their field schema. Extend Record Castle creation and editing to offer date, optional time and timezone controls. A user can clear time to return an endpoint to date-only scheduling. Preserve exact instants when switching the displayed timezone.

Resolve local date/time input with IANA timezone rules. Nonexistent civil times fail with a useful field error. Repeated civil times require an explicit offset choice. Retain explicit-offset text entry as a complete way to specify either repeated instant.

Route edits through the existing Record change pipeline, including permissions, receipts and collaborative registers. Update any work metadata projection and synchronization paths that assume dates are the only permitted endpoint values. Calendar consumers must derive local dates from timed endpoints in the Calendar timezone rather than compare timestamp strings with date strings.

### Acceptance

- Date-only work still appears in Calendar and the all-day list.
- A start at 11:02 and a ten-minute estimate produces 11:02–11:12.
- A due time at 11:02 and the same estimate produces 10:52–11:02.
- Explicit start/due values override estimates; equal endpoints remain a point.
- Invalid scheduling cannot enter through creation, single-property edits, whole-metadata edits or synchronization.

## Step 2 — Simulation occurrences and cached schedule ranges

Status: implemented; recurrence, cache reuse and payload-budget tests passed.

### Cached entry type

Extend projection output with a separate schedule occurrence type. Each occurrence has a stable identity, an underlying Record reference, a precise start, an optional end, an origin and optional rule occurrence context. The absence of an end means a point. Do not encode a point as a fabricated one-millisecond or one-minute interval.

Keep existing quantity spans separate. They describe state over time, not the time a task takes. Schedule entries can carry quantity for labels and details without changing their duration.

### Calculation

Read scheduling metadata in the isolated simulation snapshot. Emit manual timed work and each admitted future occurrence. For recurrence, use the intended occurrence instant as the start; use the explicit endpoint duration if available, otherwise the estimate. Emit successful admitted occurrences even when no quantity fact is produced. Do not show skipped occurrences as work that will happen.

Preserve occurrence identity while simulation advances. Deduplicate actual materialized occurrences using the existing occurrence linkage. Capture metadata available in the simulation snapshot so later projection-only Records can be previewed without fabricating a writable live identity.

Publish schedule entries in the same atomic cache generation as quantity spans. Add the required storage migration with a new migration number checked against the live checkout. Preserve window coverage, source revision, runtime/build identity, cache expiry and incomplete status. Changes to work endpoints, estimates, recurrence definitions and materialized occurrences invalidate the schedule cache.

### Acceptance

- Repeated brushing-teeth occurrences each occupy ten minutes with `estimate_min = 10`.
- Occurrences without a cost remain points even if quantity stays pending.
- An occurrence with no quantity delta still appears when admitted.
- Actual and projected representations of the same occurrence do not duplicate.
- A changed estimate invalidates and regenerates cached durations.
- Incomplete simulation is visible to callers and cannot silently masquerade as a complete future.

## Step 3 — Schedule query, permissions and subscription lifecycle

Status: implemented; query, visibility and subscription lifecycle tests passed.

Add a `schedule` Protein source using the existing projection-window contract. Integrate it into source validation, source catalogs, transport projection requests and source read permissions. Use the existing Record filter machinery and reject or report filters whose future meaning cannot be evaluated correctly.

Return timed work, all-day work, overdue live work and a projection-status entry. Apply the requested window to timed entries, retaining an interval when it overlaps the window. Preserve its original endpoints as context even when the visible portion is clipped. Points belong to half-open windows: include the start boundary and exclude the end boundary.

Default Time Castle to local scheduled work. An optional Protein Area supplies the query and source, while the stopwatch's optional Record binding remains independent. Respect selected-source authentication and visibility; actor-scoped sources can still show authorized manual work when simulation is unavailable, with an incomplete status explaining the missing projection.

Request a future window with five minutes of coverage padding, bounded by existing projection limits. Reuse the subscription until coverage is exhausted or controls/source change. Clock ticks only clip and reposition already received data. Cache revision and expiry notifications refresh projected work through the existing subscription mechanism.

Unsubscribe when a Castle is closed or its source changes. Handle channel backpressure, disconnection, missing Protein Areas and deleted source areas without orphaned subscriptions or a stale schedule labeled ready.

### Acceptance

- Current Record state and future projected state remain distinguishable.
- One-off deadlines and recurrence appear together in the same bounded query.
- Unreadable Records and private metadata cannot leak through projection results.
- Source changes stop the previous subscription.
- Camera movement, aperture presentation changes and one-second ticks do not repeatedly rerun simulation.

## Step 4 — Shared layout, meshes and time advancement

Status: implemented; native clock, helix and straight-view smoke passed.

### Shared model

Add serializable Time Castle settings with aperture, horizon, timezone, source and coiled/straight state. Validate finite geometry and bounded durations. Enforce horizon at least aperture: increasing aperture beyond horizon raises horizon; decreasing horizon below aperture lowers aperture.

Represent all three views with the same time parameter and entry identities. Compute the present angle from selected-zone civil clock time modulo aperture. Place future times clockwise according to elapsed duration, including across daylight-saving changes. Labels include dates and offsets when a repeated wall-clock time would otherwise be ambiguous.

Choose tick intervals from screen-space density, down to seconds. Keep point line widths and hit targets independent of the aperture. Clip ranges at Now without changing their original scheduling identity. Group visual collisions into expandable stacks while preserving each member's exact start/end and causal context.

### Desktop rendering

Integrate a real helix into existing workspace topology rather than placing an independent 3D viewer inside the Sand. The first turn forms the clock face. Future turns extend behind it along the depth axis so face-on presentation is the next aperture and side views reveal future depth.

Keep stopwatch controls, settings and lists on the existing Sand surface. Render the schedule as dedicated meshes with correct workspace ownership, placement, scaling, visibility and picking. Replace the normal rectangular solid only where needed for the schedule geometry; do not allow it to obscure the helix. Clean up owned meshes and materials when their owner disappears.

Untwist/Coil interpolates the same entries over 300 ms. The straight target places Now at the left and future time to the right. The configured horizon fits the straight display. Retain selection during transitions. Use bounded mesh tessellation and screen-space aggregation for dense schedules rather than spawning one unbounded entity hierarchy per tick or minute.

Arrange time advancement through the existing wake mechanism. Update Now once per second while visible; animation requests frames only while a transition is running. Rebuild expensive geometry when data, settings, effective layout or the time bucket changes, not simply because the camera changed position.

### Acceptance

- At 10:10 the one-hour clock's minute-8 position means 11:08.
- Four hours at one hour per turn produce four helix turns; two hours per turn produce two.
- No elapsed geometry reappears when wrapping around the ring.
- Overlapping entries remain individually selectable through a stack.
- Points stay thin at one-, two- and ten-hour apertures.
- Mesh ownership and picking remain correct after placement changes, deletion and workspace switches.

## Step 5 — Scoped Record selection and CRUD receiver

Status: implemented; receiver, pending-edit protection and scope isolation tests passed.

Define a public `Record selected` event name and a serializable payload containing the underlying Record reference, source and selected schedule occurrence context. Emit it for selected points, intervals, stack members, all-day tasks and overdue tasks.

Use existing event scope and boundary rules. Record Castles opt into the event through a persisted listener setting. Receiving an event changes the Castle's Record query and refreshes its existing editable fields. Retain drafts or report an unsaved/pending edit condition instead of silently discarding an edit during selection.

For projected occurrences of an existing Record, target that live Record and carry the occurrence as preview context. For a Record that exists only in simulation, show its preview and originating live Record/rule. Never issue a mutation against a synthetic projection identifier or imply that editing the live Record edits only one occurrence.

### Acceptance

- A selection reaches opted-in Record Castles in the same scope.
- Other workspaces, isolated compositions and event boundaries prevent cross-selection.
- Selecting a stack expands it; choosing its member emits one selection event.
- Record CRUD updates the schedule through ordinary live subscriptions and cache invalidation.

## Step 6 — Persistence, credits and complete verification

Status: complete; persistence, credits, 54 focused tests, backend/model checks, native check and visual smoke verified. Optional spatial readability follow-ups are recorded below.

Persist Time Castle settings in workspace snapshots and Custom Castle compositions. Remap internal Protein Area references when compositions are copied. Persist Record Castle listener choices using the existing area/workspace storage model. Restore settings before starting feeds so restoration does not briefly subscribe to the wrong source.

Keep standalone timer logs and bound Record work logs intact. Update Sand Store labels and previews to explain the schedule capability. Include the licenses and credits of Bevy, Chrono, Chrono-TZ, timezone detection and any other embedded dependency through Time Castle's credits.

Run pure-model tests for schedule parsing, duration derivation, geometry, tick density, clipping and timezone transitions. Run engine/store/protein tests for recurrence, caching, visibility and invalidation. Run desktop behavior tests for scoped event routing, source lifecycle, persistence and existing timer operation.

Add or extend a desktop smoke example to exercise the clock, side-view helix, straight transition, stacks, an all-day item, an overdue task and Record selection. Use the available graphics environment for visual inspection. Record environment limitations rather than claiming a visual check that did not run.

Run targeted `cargo check` for affected crates and appropriate focused test suites. Report concurrent changes that prevent checks, finish all independent work, and leave the exact remaining issue in this document.

### Repeatable validation commands

Run these commands from the repository root, sequentially, to reuse the shared Cargo output directory. The pure model commands disable the default UI feature so they do not compile Bevy unnecessarily.

```sh
nix develop .#interface --command cargo test -p nucleus --test schedule
nix develop .#interface --command cargo test -p engine --test private_work
nix develop .#interface --command cargo test -p engine --lib projection::tests
nix develop .#interface --command cargo test -p protein --test schedule
nix develop .#interface --command cargo test -p lince-simulation --test projection
nix develop .#interface --command cargo test -p lince-interface --no-default-features --features models --test time_castle
nix develop .#interface --command cargo test -p lince-interface --no-default-features --features models --test models calendar_
nix develop .#interface --command cargo test -p lince-desktop --lib time_castle
nix develop .#interface --command cargo test -p lince-desktop --lib scoped_events
nix develop .#interface --command cargo test -p lince-desktop --lib work_timer::tests
nix develop .#interface --command cargo test -p lince-desktop --lib standalone_time_log_and_running_stopwatch_survive_restart
nix develop .#interface --command cargo check -p nucleus -p store -p engine -p protein -p lince-interface --no-default-features --features lince-interface/models
nix develop .#interface --command cargo check -p lince-desktop --example time_schedule_smoke
nix develop .#interface --command cargo run -p lince-desktop --example time_schedule_smoke
```

## Execution log

- Detailed plan saved before implementation. Existing checkout contains substantial concurrent changes; no worktree or agent delegation will be used.
- Scheduling values, duration derivation, timezone resolution and date movement live in `crates/nucleus/src/schedule.rs`. Date-only values remain untimed. The new date/time popup uses existing Record edits and creation fields. Moving a date through Calendar preserves its time and subsecond precision, while ambiguous or nonexistent local times require the explicit time editor.
- Migration `0337_projection_schedule.sql` adds a schedule cache beside quantity spans. Applied recurrence admission is read independently from quantity facts, so a zero quantity change still produces a point or cost interval. Schedule and quantity cache data publish in one transaction, subject to revision and byte budgets.
- `protein::Source::Schedule` combines live manual scheduling with cached simulation. Its Record reads request only the fields needed for scheduling and enforce the occurrence count limit. Date-only and unfinished overdue work have separate categories. The transport requests the existing projection calculation for this source.
- `crates/interface/src/time_castle.rs` owns saved settings, phase, helix and straight positions, tick spacing, clipping, stack membership and event payloads. The first geometry suite passed four tests, including 50,000 points with bounded stack count. The scheduling parser initially passed four tests, and the extended work-metadata suite passed eighteen tests before the subsequent occurrence-link addition.
- Desktop Time Castle adds aperture, horizon, timezone, source and untwist controls before the existing stopwatch. Flat mode rasterizes the first aperture with resvg; spatial mode creates Bevy meshes behind a real opening in the Castle's UI face. The enclosing solid box is hidden. The shared workspace camera controls these meshes. Geometry is bounded to 4,096 background samples, 256 stacks and 256 ticks; per-stack curve sampling is bounded too.
- Schedule feeds reuse a padded window rather than shifting subscriptions every frame. Source changes clear old rows, unsubscribe from the former source and subscribe through its existing local or remote connection. Remote authentication stays with the selected Protein Area. Clock movement wakes the idle interface once per second; animation requests short wakes for its 300 ms transition.
- `Record selected` now carries the occurrence and source to an explicit persisted Record Castle listener. Pending or unsaved edits prevent target replacement. Preview-only Records show a read-only panel and hide the prior editable row while that panel is open. An originating recurrence can be opened when its identity is available. Workspace membership is now part of ungrouped event scope, closing an existing cross-workspace routing gap.
- Settings are saved in workspace Sands, Custom Castle parts and native Canvas component state. Custom Castle copying captures linked Protein Areas and remaps the saved reference. Standalone stopwatch logs retain their existing storage. Time Castle includes the licenses and attribution for timezone detection, Chrono-TZ, Chrono, Bevy, Lato, resvg, usvg, tiny-skia and fontdb.
- Added `time_schedule_smoke` to exercise overlapping ranges, a point, a later task, all-day work, overdue work, side-camera mode and untwisting. It writes four native screenshots under `/tmp/lince-time-schedule-*.png` when run successfully.
- Intermediate desktop checks encountered concurrent changes in Store role handling and then an unfinished Engine workspace-sync module. Those files belong to other ongoing work and were preserved. Final checks will record their actual outcome below.
- Cache publication excludes Store's `commit_sequence` bookkeeping table from source-invalidation triggers. Otherwise publishing a cache transaction invalidated its own revision and discarded the result. The cache-reuse and concurrent-window tests verify successful publication after this correction.
- Materialized occurrence linkage now passes through collaborative work registers and whole-metadata changes. The Schedule test materializes a cached occurrence and confirms that the live Record replaces its projected representation without duplication.
- Simulation fixtures authorize their runtime through a signed device roster. A runtime without execution authorization preserves manual work and reports an incomplete projection instead of claiming that an empty future is complete.
- Serialized cache payloads have a combined 16 MiB budget. Calculation retains entries that fit and marks the result incomplete when the budget is exceeded. A regression test covers JSON escaping that expands a value beyond its original byte count.
- Completed backend validation: 5 shared scheduling tests, 19 work-metadata tests, 5 simulation projection tests, 3 Schedule query tests and 1 serialized-payload budget test. The simulation suite covers zero quantity changes, warm cache reads, disk reuse, shared windows and private actor visibility.
- Completed frontend validation: 5 pure Time Castle model tests, 2 Calendar model tests and 13 focused native tests. Coverage includes daylight-saving labels, fractional seconds, timezone-specific Calendar dates, 50,000 points, bounded aggregation, persistent render handles, subscription cleanup, scoped selection, pending edits, workspace isolation, linked-source copying, workspace restoration and the existing stopwatch/log behavior. Together with backend suites, 53 focused tests passed.
- The graphical smoke uses a temporary workspace and an in-memory Engine, so it does not alter the user's saved workspace. Its results and final native `cargo check` are recorded below.
- The first native smoke completed and confirmed the flat clock, all-day/overdue categories and selection reaching the Record Castle. Visual inspection found that spatial meshes disappeared. Schedule geometry now retains its render entities, material and mesh handles between ticks, replacing vertex data and pick identities in place. A regression test verifies handle reuse and removal of unused stack meshes. The smoke now also requires visible spatial schedule geometry before exiting successfully.
- The final native check initially encountered concurrent Workspace Sync changes: a newly referenced backend command had not yet reached the compiled dependency, and a moved `source` value failed its borrow check. Those changes remain with their existing author; the repeat checks use the current shared files.
- The repeated native smoke completed successfully with visible spatial geometry and the expected selected Record. Screenshots were inspected: the flat view shows the first aperture, a stacked overlap and a thin appointment marker; the side view shows the future helix; untwisting produces a horizontal strip with the selected interval retained. The example's isolated runtime correctly reports unavailable future simulation while showing its six manually scheduled Records.
- Spatial tick spacing now accounts for the full helix length and interpolates toward the straight display width during untwisting. The timezone field has additional width, and selected schedule details use readable manual/recurrence descriptions rather than internal identifiers or serialized metadata.
- A subsequent native check and screenshot refresh were blocked by concurrent Fiote changes in `crates/fiote/src/communication/discovery.rs:141`: `acp::Config.session_meta` expected `serde_json::Map<String, Value>`, but discovery supplied `BTreeMap::new()`. The earlier successful native smoke and the seven-test native Time Castle suite included the render-handle fix. Independent backend/model checks and the Calendar timezone regression continued separately; no Fiote files were edited for this feature.
- Concurrent work subsequently corrected Fiote's session metadata initialization with `Default::default()`. Final native verification can be retried after the queued independent checks; the earlier diagnostic remains in this log to explain the interrupted check.
- Final independent `cargo check` passed for Nucleus, Store, Engine, Protein and the Interface model with UI features disabled. Workspace Cargo settings deny compiler warnings. The two Calendar tests also passed, including precise instants crossing a local date boundary without moving date-only values.
- Final native `cargo check -p lince-desktop --example time_schedule_smoke` passed on the current shared checkout with compiler warnings denied. The earlier concurrent Workspace Sync and Fiote failures were resolved by their ongoing work and no longer block this feature.
- All six implementation steps are complete. The successful native smoke screenshots remain at `/tmp/lince-time-schedule-0.png` through `/tmp/lince-time-schedule-3.png`. An optional final screenshot refresh waited behind another shared build and was canceled while still queued; the completed visual inspection already covered the required views and selection, and the final compiler check includes the subsequent label and tick-spacing changes.
- Follow-up audit refreshed the native screenshots successfully after the label-width and tick-spacing changes. The example exited successfully, including assertions for the straight state, selected Record and visible spatial geometry. The refreshed clock, side-view helix and straight strip were inspected.
- The audit found that selected recurrence descriptions read the wrong origin level and projection previews displayed serialized origin metadata. Both presentations now share a readable description using the Schedule source's nested recurrence cause. A regression exercises a typed rule cause through Record selection, requiring a read-only preview, an unchanged live Record query, a readable intended time, an originating-rule control and no exposed internal identifiers.
- Optional spatial presentation follow-up: put time labels directly on the 3D strip. The spatial renderer currently draws tick marks; clock-face labels and selected-task details already show times. The quiet presentation follow-up below addresses the surrounding UI surface that partly occluded side-view turns. Past geometry remains outside this version's agreed scope.
- The follow-up native Time Castle suite passed all eight tests, including the new projection-preview regression, with compiler warnings denied. That adds one distinct test to the previous 53, for 54 focused passing tests overall. The native graphical smoke also passed and its refreshed screenshots were inspected. No planned validation remains pending.

## Follow-up — Quiet clock presentation

Requested behavior: the normal Castle is a round clock-like figure showing its time strip and events, with one small button for its controls. Configuration and work lists must be easy to show and hide. Refine the design through the actual native renderer and inspect the result.

### Step 1 — Reduce the permanent interface

Status: implemented and inspected in the native preview.

Use a compact square footprint with a transparent enclosing surface. Keep the schedule viewport full size. Remove the permanent title, source text, form fields, task list and stopwatch from the normal presentation. One small circular control button opens a panel and becomes its close button. The panel groups Clock, Agenda and Stopwatch into separate tabs instead of placing every control on one surface. Keep editable fields alive when the panel closes so hiding controls preserves drafts. Keep the existing stopwatch Record input attached to its Castle for workspace and Custom Castle snapshots.

### Step 2 — Reveal event context when needed

Status: implemented; bounded selection details and retained selection passed their regression.

Keep points, intervals and overlaps clickable in the clean view. A selected event or overlap opens a compact, dismissible detail panel. Choosing a member still sends the existing scoped Record-selection event. The complete future, all-day and overdue lists remain in the Agenda tab. Closing the control panel returns immediately to the clean clock. Escape dismisses transient panels without changing scheduling or stopwatch state.

### Step 3 — Refine the face and spatial strip

Status: implemented and inspected in the flat, spatial and straight native views.

Use a subdued round face, a slim track, clear event colors and a warm present pointer. Separate fine ticks from sparse time labels so extra zoom detail does not turn into extra text clutter. Add a restrained current-time readout. Use matching widths and colors on the helix and straight strip. With the enclosing UI background transparent and controls hidden, surrounding form chrome must no longer cover future helix turns. Keep the existing future-only schedule, aperture behavior and selection identity.

### Step 4 — Verify the interaction and design

Run focused native regressions for the default hidden state, control-panel toggling, retained drafts, selection and stopwatch behavior. Update the smoke to capture the clean face, open controls, event selection, unobstructed helix and straight strip. Inspect the images and fix layout problems before reporting completion. Run the native example's `cargo check` with warnings denied and record results here. This is a frontend presentation change using the existing schedule backend.

Status: complete; fourteen focused native tests, ten inspected screenshots and the final native compiler check passed.

- The default footprint is 420 × 420. The enclosing background and outline are removed, and the circular control button rests inside the face. The closed state has one visible action button. Opening controls replaces that button with a close action and reveals Clock, Agenda and Stopwatch tabs.
- The form and timer entities stay alive while hidden. The stopwatch Record input remains a direct child of its Castle so workspace and composition snapshots keep its binding text. Closing releases input focus; Escape closes open panels and clears transient selection without changing settings or time logs.
- Event details show readable titles and times and render at most twenty entries per page. Dismissing the event panel retains the highlighted occurrence and its Record Castle selection. Selecting another event reveals its details again. All-day and overdue work live in the Agenda tab.
- The clock uses a slim slate track, teal events, violet overlaps and a warm present marker. Fine tick density and sparse label density are independent. The small aperture caption and control-button tone indicate when only scheduled work is available; the full projection status stays in the Clock tab.
- Spatial rendering uses matching colors with conversion to linear vertex colors. Only visible controls and detail panels retain UI face geometry, so a hidden rectangular face cannot obscure the helix or intercept its event picks. Replacing that geometry also refreshes its bounding box before picking.
- The native smoke now captures ten stages: the clean clock, Clock controls, Agenda, event details, the unobstructed helix, spatial controls, the untwisting result, the settled straight strip, Stopwatch and the closed flat strip. It uses a temporary workspace and an in-memory Engine and passed its straight-state, selected-Record and visible-geometry assertions. The screenshots were inspected after fixing a clipped enclosing outline and readout showing through panels.
- The latest native Time Castle suite passed all ten tests, including two new regressions for hidden controls with retained drafts/logs and bounded, dismissible event details. The three stopwatch tests and one workspace-restoration test also passed, for fourteen focused native tests in this follow-up. The two new regressions increase the feature's cumulative distinct test count from fifty-four to fifty-six.
- Intermediate native runs encountered concurrent Engine and Workspace Sync compiler changes. Their ongoing work resolved the Engine issues. One unreachable fallback arm in the desktop workspace-sync operation match was removed after the compiler confirmed the four explicit cases covered every operation; this allowed validation with warnings denied.
- Final `nix develop .#interface --command cargo check -p lince-desktop --example time_schedule_smoke` passed on the current shared checkout with compiler warnings denied. Formatting checks passed for the Time Castle modules and smoke example, and `git diff --check` reported no whitespace errors. The quiet clock presentation and its planned verification are complete; this follow-up reuses the existing schedule backend.

## Central entry, readable events, cursor modes, and theme overrides

The latest presentation is documented in [Time Castle: readable clock and theme controls](time-castle-readability.md). The fixed-size round clock opens configuration through its central skull, `memento`, and `mori` stack. Every timed event has an individual marker or range and an always-readable outward annotation. It supports both cursor behaviors and global, Sand-type, and individual-Sand theme tokens, with a scoped customization shortcut.
