# Time Castle: readable clock and theme controls

## Requested result

Keep the round clock minimalist. Its only configuration entry is a centered stack of a skull, `memento`, and `mori`. Show the current time and aperture clearly. Let the user choose between a cursor that stays at twelve o'clock while time passes underneath it and a cursor that travels around the face.

Show each event's title and time without requiring hover. Preserve separate point markers and duration bands, including simultaneous points, overlapping durations, and points inside durations. Simultaneous events form an outward-growing list with visible connections to their time. Use the existing theme system for the clock and make global, Sand-type, and individual-Sand overrides easy to reach and reset.

Work sequentially in the shared branch. Preserve concurrent changes. Add no code comments and do not modify AGENTS.md, README.md, `.lingua` files, or the unplugged web crate. Use `cargo check`, with warnings denied.

## Step 1 — Central entry and cursor behavior

- Add a saved cursor behavior to the shared Time Castle settings. The moving behavior retains civil-clock phase; the stationary behavior uses twelve o'clock as Now and positions future work by elapsed duration. Use the same position and transverse calculations in the ring, helix, and untwisting animation.
- Provide a cursor choice inside Clock configuration. Changing it updates presentation immediately while retaining schedules, selected identities, sources, aperture, and horizon.
- Replace the round face's settings icon with a transparent, accessible button over its central skull and two-line phrase. Keep the existing icon entry in straight and spatial modes. An open configuration panel retains its close action and Escape behavior.
- Draw a restrained skull using the repository's vector renderer. Keep current time and aperture separate from the three stacked entry elements so their meanings remain clear.
- Verify both cursor behaviors and settings serialization. Check that the round view has one configuration entry, the fallback icon is hidden, and changing modes restores the appropriate entry without losing drafts or timer state.

## Step 2 — Theme and token overrides

- Audit global theme selection, Sand-type overrides, individual-Sand overrides, inheritance, reset behavior, and persistence before extending them.
- Add any clock-specific color and layout tokens needed for the strip, events, overlaps, present marker, and labels. Include valid values in every bundled theme and its export/import document.
- Remove hardcoded clock colors and forced local color overrides. Resolve the face, typography, markers, range widths, labels, and inspector surfaces through the existing inheritance chain. Include the effective palette in render invalidation so theme changes appear immediately.
- Provide a direct customization shortcut inside Clock configuration. Target this Castle in the existing customization panel. Keep configuration access in the clock center while the normal round view is closed.
- Add scoped theme presets and an inheritance reset to the existing customization panel. Applying a preset to one Sand or type must not alter the global theme or unrelated Sands. Preserve explicit dimensions when applying a color/style preset.
- Verify token inheritance, a local override, reset back to global values, scoped presets, and theme document round trips. Retain existing workspace and composition token storage.

## Step 3 — Individual ranges and outward event lists

- Build a deterministic layout from every timed event inside the aperture. Clip elapsed geometry at Now while retaining the original start/end times for labels. Exclude overdue and all-day categories from timed geometry; their existing Agenda sections remain.
- Assign overlapping duration bands separate radial lanes. Keep points thin and separately represented even when their time matches a range or another point. Preserve occurrence identities and selection.
- Place readable event titles and time ranges in outward stacks. Connect each stack to its time on the strip. Resolve collisions between neighboring lists instead of hiding events behind an overlap count or requiring hover to reveal them.
- Keep the clock's dimensions, position, center, and strip radius fixed. Attach separate outward annotation stacks sized from typography, without resizing the clock or adding scrolling. Keep the center clear for current time, aperture, and the sole configuration entry. Every event retains a readable title and time.
- Use the same layout for rendering and hit testing. Selecting a visible band, point, or label routes the existing scoped Record-selection event. Keep the underlying future-only Schedule source and simulation cache unchanged.
- Verify combinations of equal-time points, nested intervals, crossing intervals, points within intervals, zero-duration events, long titles, clipping, and source updates. Check that labels do not overlap, no event is dropped, and layout remains deterministic with realistic dense apertures.

## Step 4 — Native inspection and completion

- Extend the isolated graphical smoke with simultaneous points and several overlapping ranges. Capture the central entry, both cursor modes, readable event lists, configuration, an individually themed clock, helix, and straight mode.
- Inspect the native screenshots and fix typography, contrast, spacing, and clipping before marking the work complete.
- Run focused model, theme, desktop, persistence, and stopwatch regressions appropriate to the changes. Run the native example's `cargo check`; record actual results and any concurrent compilation blockers.
- Reuse the existing schedule backend, scoped events, workspace persistence, and embedded renderer credits. Add no new dependency solely to draw the skull.

## Execution results

- Steps 1–3 are implemented. The round clock has a transparent central button over the drawn skull, `memento`, and `mori`. Its corner settings entry stays hidden while the round view is closed. Straight and spatial views retain their existing entry. Configuration preserves timer drafts, source settings, and selected occurrence identities.
- The saved cursor setting selects moving civil-clock phase or a stationary twelve-o'clock pointer. Both use the shared ring, helix, transverse, and untwisting calculations. The face shows current time, the aperture, and its start/end window; apertures below a minute show seconds.
- The clock resolves the existing global → Sand type → individual Sand token chain. Seven clock tokens cover track, events, overlaps, present marker, secondary text, range thickness, and label spacing. Bundled themes include all tokens and the fixed default clock dimensions. The customization shortcut selects this clock, and scoped presets preserve dimensions and leave the global theme and other Sands unchanged. Inheritance and individual-token resets remain available.
- Every timed occurrence retains an individual point or interval and its identity. Overlapping intervals receive distinct radial lanes inside the existing strip. The same geometry supplies drawing and click selection, including equal-time points. Labels keep complete titles, original times, and projected-occurrence status without hover or count aggregation.
- Annotation stacks sit outside the clock as separate workspace surfaces. Their layout avoids intersections and leaves the face clear. They follow movement and rotation, reuse their entities, and close with their owner. They do not resize the clock or scroll. Very dense stacks occupy more surrounding workspace; the clock's own dimensions and strip radius stay fixed.
- Nine shared-model tests passed, covering cursor behavior, serialization, daylight-saving labels, aperture bounds, clipping, zero-duration events, overlap lanes, complete text, deterministic layouts, and a 5,000-occurrence layout performance check.
- Forty-three distinct focused desktop tests passed. The Time Castle run reported 14 passes; the related theme, customization, composition, stopwatch, typography, and workspace run reported 30 passes, with one shared composition test. These cover the central entry, retained drafts, label reuse and cleanup, exact point selection, theme inheritance, scoped presets, reset, export, persistence, selection scope, and stopwatch behavior.
- Six shared theme tests passed, including every compiled token's defaults, inheritance/reset, invalid values, bundled-theme readability, incomplete documents, and complete export/import round trips. Total for this follow-up: 58 distinct focused tests passed.
- The isolated native walkthrough completed all 13 captures successfully. It uses temporary storage and an in-memory Engine. It verified the unchanged 420 × 420 clock footprint, both cursor choices, the customization shortcut's selected scope, selected-Record delivery, straight state, and visible spatial geometry. Captures wait for at least sixteen rendered frames and one second so animation and typography have settled.
- Screenshots inspected: `/tmp/lince-time-schedule-0.png` (moving cursor), `-4.png` (stationary cursor), `-5.png` (individual clock theme), `-6.png` (helix), `-9.png` (straight strip), `-10.png` (settled return to clock), `-11.png` (Stopwatch), and the remaining configuration, Agenda, and selection states.
- No new dependency or scheduling backend was required. Existing scheduling, simulation-cache, scoped-selection, workspace/composition storage, and embedded-renderer attribution remain in use. A regression corrected the style classification of a clock before initialization so its dimension overrides survive.

- Native `cargo check -p lince-desktop --example time_schedule_smoke` passed during implementation with warnings denied. The final repetition exited with five errors from concurrent Engine record-policy edits: `record_policy/edits.rs` calls the private `actions.rs::resolve_concept_opt` method four times, and line 213 iterates `&resolved` where `resolved` is already a borrowed vector. These files were left intact. Formatting checks passed for the Time Castle modules, shared layout/model tests, and smoke; whitespace checks passed for the related changes.

Status: requested clock implementation, focused tests, and graphical verification complete. Final compiler verification of the current shared checkout is blocked by the concurrent Engine errors described above.
