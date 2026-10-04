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

## Follow-up: minimal rim, flowing events, sharp rendering and sound

The approved sketch replaces the earlier inner circles and inward lanes. Keep the clock footprint and rim fixed. Digital time and aperture belong at the top; current work and the next task belong in the middle; skull, memento and mori belong low inside. The phrase remains the sole configuration entry in round mode. A thin moving hand is the default, with the stationary-pointer alternative retained.

### Task 1 — Shared rendering quality

- [x] Correct premultiplied transparent-surface compositing so glyph edges are not multiplied by alpha twice. Check text, opaque panels, images, rounded corners and translucent surfaces.
- [x] Make clock rasterization follow physical DPI and workspace zoom within the shared texture and memory budget, removing the independent 2×/1600-pixel limits.
- [x] Audit density allocation, fractional positioning and filtering across native Sands. Preserve readable text when budget pressure reduces resolution; use real text measurements for card wrapping and sizing.
- [x] Keep titles and time ranges readable without hover, shrinking secondary timing text or resizing the clock.

### Task 2 — Strings attached to their actual events

- [x] Connect point cards to their markers and range cards to the midpoint of their visible band, preserving original endpoints in the text.
- [x] End each string at the nearest card edge, with a restrained curve following card and band motion.
- [x] Share geometry with hit testing, including animation, simultaneous points and clipped ranges.

### Task 3 — One rim and useful divisions

- [x] Remove decorative inner circles and the redundant thick track. Draw one perimeter with twelve major ticks and four minor ticks between each pair: sixty divisions for every aperture.
- [x] Derive values from aperture: one hour gives 5-minute majors/1-minute minors; two hours gives 10-minute majors/2-minute minors; one minute gives 5-second majors/1-second minors.
- [x] Label actual next-occurrence clock times, using compact minute values for the normal hour and sufficient hour, date or fractional-second precision for other apertures.
- [x] Draw a thin moving hand without obscuring interior readouts. Keep current time and aperture clear; retain the fixed-pointer mode.

### Task 4 — Interior summary and configuration entry

- [x] Show digital current time at the top with aperture and window beneath it. Show the earliest-starting ongoing task then the next future task; if none is underway, show the next two future tasks.
- [x] Include titles, original times/ranges and compact countdown amounts without the word ETA. Count future work to its start/point and ongoing work to its end.
- [x] Move skull/memento/mori below those rows, keeping the transparent entry aligned with its hit area and accessible to keyboard users.
- [x] Keep diagnostics, theme controls and sound controls inside configuration, without empty decorative boxes.

### Task 5 — Outward bands resting on the rim

- [x] Replace constant inward lanes with outward profiles that vary across event boundaries. Earlier original starts take priority; equal starts put longer ranges underneath; identities break remaining ties.
- [x] Stack bands outside the units rim. When a lower interval ends, let the supported interval descend toward the rim through a short smooth curve while preserving separation.
- [x] Place unestimated points above bands occupying their instant and keep simultaneous points individually visible.
- [x] Reconcile new earlier-starting data without changing the rim, footprint, selection or original times. Preserve full separate cards, no internal scrolling, and shared helix/straight behavior.

### Task 6 — Gentle entry, exit and following

- [x] Retain motion by occurrence identity. New events approach radially and settle with a small damped bounce; departing events gently move outward and fade.
- [x] Use cheap springs for cards and band adjustment, preserving velocity on target changes and taking the short route across angular wraparound.
- [x] Keep cards separated during motion, update strings with animated positions, and disable retired-event interaction immediately. Past geometry remains clipped.
- [x] Stop unnecessary animation work once settled; update geometric motion without rasterizing all text every frame. Hidden clocks keep scheduling without visual animation.

### Task 7 — Accurate projection status

- [x] Retain structured projection status rather than conflating updating and every incomplete result into unavailable. Show specific diagnostic reasons inside configuration.
- [x] Repair the smoke fixture with a signed authorized execution roster. Its absent roster caused the earlier unavailable caption; preserve authorization checks.
- [x] Keep scheduled Records visible when projection is incomplete or unavailable and cancel stale alerts on disconnection.

### Task 8 — Shared sound utility and clock alerts

- [x] Add platform-neutral sound settings, cue queueing, cancellation, volume and occurrence deduplication to interface with an injectable native adapter; leave mobile integration for later.
- [x] Reuse CPAL output for a short generated blip independent of the recording library. Use the Rust tts crate 0.26.3 for spoken titles, including Linux Speech Dispatcher dependencies and Sand licenses/credits.
- [x] Save Off/Blip/Title modes, volume and supported voice selection inside clock configuration. Default to Off.
- [x] Alert at range start or point/deadline, once per canonical occurrence across clocks and refreshes. Initial loading remains silent. Simulation announcements say Projected; actual confirmation must not repeat the alert.
- [x] Make one blip for simultaneous occurrences or queue every title in stable order. Continue while Lince is open even if its workspace is hidden or its window minimized.
- [x] Cancel removed/rescheduled pending cues, stop queued cues when disabled, and show voice/device failures in configuration.

### Task 9 — Verification and completion

- [x] Test ordering, draping, clipping, point stacks, anchors, selection, motion, wraparound, retained entities and the unchanged clock footprint.
- [x] Verify twelve major/sixty total ticks across apertures, midnight and timezone transitions; validate summaries and countdown boundaries.
- [x] Inspect native-resolution screenshots at different DPI/zoom, including transparent edges, titles/times, themes, helix and straight views.
- [x] Use a fake sound adapter for boundaries, hidden clocks, queues, deduplication, promotion, cancellation and errors; then verify native playback.
- [x] Check texture memory, layout cost, animation settling and idle behavior. Run focused tests and cargo check with warnings denied, recording concurrent blockers without overwriting unrelated work.

## Follow-up implementation results

The earlier results are historical. This follow-up supersedes their inward lanes, decorative circles, unavailable caption and original thirteen-stage smoke. The compiler errors recorded above no longer block the current checkout.

- **Shared sharpness:** native Sand textures use premultiplied compositing, including opacity fades, so transparent glyph edges retain their color. Text-bearing surfaces request twice their physical display density and retain priority over media when the shared 32-million-pixel budget is tight. The clock raster follows that allocation, DPI and zoom up to the shared 4096-pixel dimension cap. Font data is cached; bundled Lato, DejaVu, symbols and CJK fallbacks cover clock labels and Unicode titles. Card titles and their original time ranges use the same fourteen-pixel default size and measured glyph advances.
- **Minimal face:** one hairline rim surrounds useful divisions, with no decorative inner circles or thick background track. Every aperture has twelve major and sixty total ticks. Labels represent upcoming civil-clock values, including midnight, date changes and daylight-saving offsets when needed. The thin moving hand is the default, stays clear of the interior readouts and hides behind configuration. Current time, aperture and the visible window sit high inside; current/next work and countdowns occupy the middle; skull/memento/mori sits low as the sole round-mode configuration entry.
- **Outward event geometry:** duration profiles are ordered by original start, then longer equal-start ranges, then identity. All bands rest outside the units rim. Shared descent timing keeps overlapping ranges separate as their supporting range ends. Points remain thin and individually visible above the ranges at their instant. Geometry clips elapsed time while text retains original endpoints. Leaders connect each actual point or visible range midpoint to its nearest card edge.
- **Motion and readability:** retained analytic springs follow new targets without resetting velocity. Arrivals fall inward with a restrained bounce; removals move outward and fade, immediately losing interaction. Cards retain identities and entities, keep full titles/times, remain separate during motion and fan sideways at ordinary window edges. The clock stays 420 × 420 in the native fixture and never gains an internal scrollbar. Motion updates meshes separately from glyph rasterization, sleeps after settling and skips hidden visual work. A final zoom check exposed a fractional-edge separation loop; separation is now bounded and includes explicit floating-point clearance and a centered-card fallback, with a regression test.
- **Projection status and identity:** the clock retains Ready, Updating and Incomplete status and presents specific reasons inside configuration. The earlier screenshot's unavailable result came from the native fixture lacking a signed authorized execution roster. The fixture now publishes a signed roster and asserts both device execution and ready simulation, preserving authorization. Scheduled Records remain visible even when projection is incomplete. The schedule backend carries validated materialized-occurrence identity so a projected alert and its actual confirmation deduplicate correctly.
- **Sound:** `interface::sound` owns saved Off/Blip/Title settings, volume, canonical-occurrence deduplication, stable simultaneous-title order, combined blips, quiet initial loading and cancellation behavior. The native sound worker owns future deadlines and playback, allowing cached alerts to fire while drawing is paused or the window is minimized. It reuses CPAL for the generated short blip and the native `tts` adapter for spoken titles, including supported voice selection and the spoken Projected prefix. Refreshes remove stale queued speech; disabling a clock cancels its pending and current cues. Voice/device failures appear inside the separate Sound configuration tab. New embedded dependencies and font fallbacks have Sand credits/licenses. Mobile integration remains a later step as requested.
- **Preserved controls:** straight/helix modes, untwisting, scoped Record selection, standalone stopwatch storage and global → Sand type → individual Sand token inheritance remain available. Theme/token customization stays inside configuration.

The main changes are in `crates/interface/src/time_castle.rs`, `crates/interface/src/time_castle/layout.rs`, the new shared `motion.rs` and `sound.rs`, the native `time_castle` modules, the new `crates/desktop/src/sound_cues.rs`, shared topology presentation/budgeting, and `crates/protein/src/schedule.rs`.

### Verification results

- Shared model/library run: **31 passed** (15 library tests and 16 clock integration tests), covering springs, sound boundaries/queues/errors, lifetime deduplication, midnight/DST, tick divisions, summary/countdown boundaries, original-time ordering, smooth outward profiles, points, clipping, full text and dense layouts. A 50,000-cue quiet-deadline check and 5,000-event annotation layout check passed.
- Focused native clock run: **25 passed**, including central access, hidden-clock scheduling, cancellation/rescheduling, no native device for default Off, font fallbacks, high-density transparent edges, retained entities, animated anchors/hits, retirement, nearest-edge strings, viewport packing and the fractional-edge/center separation regression.
- Shared topology runs: **12 passed** (5 surface tests and 7 presentation tests), including the texture budget, text priority, supersampling and premultiplied fade behavior.
- Schedule backend run: **3 passed**, including cache invalidation, scoped privacy and materialized-occurrence identity.
- These four groups contain **71 distinct focused tests**. Related stopwatch, customization, composition and workspace tests also passed. The broader desktop selection reported 84 passes, two failures and one ignored test; two failures also reproduced in isolated runs.
- `nix develop .#interface --command cargo check -p lince-desktop --tests --example time_schedule_smoke` passed with workspace warnings denied. Rust formatting checks passed for the changed clock, sound, rendering and schedule modules; `git diff --check` passed. No `cargo build` was used.
- The final isolated native walkthrough exited successfully after **23 captures**, with its fixed-footprint, projection-ready, selection, mode and sound assertions passing. Playback verified a blip, two queued test titles and a future scheduled projected title, with at least four starts and three speech completions and no native sound error. Future cue scheduling runs in the worker independently of render wakeups.
- Native screenshots were inspected for the dark clock (`/tmp/lince-time-schedule-0.png`), themed return (`-10.png` and `-22.png`), configuration and sound (`-1.png` and `-14.png`), helix (`-6.png`), straight (`-9.png`), two-hour aperture (`-16.png`), one-minute aperture (`-17.png`), double zoom (`-18.png`) and doubled display DPI with reduced workspace zoom (`-20.png`). Normal clock views retain all nine separate fixture cards with complete titles and original timing. Text remains sharp at the tested physical scales. The double-zoom walkthrough now progresses through the formerly stalled separation boundary.

Two broader desktop test failures remain outside the clock implementation:

1. `edit_mode::tests::workspace_and_store_controls_work_with_filtering_and_default_content` (`crates/desktop/src/edit_mode.rs:2003`) filters the store with timer and expects WorkTimer, while its current title is Time Castle.
2. `laboratory::tests::laboratory_stress_is_temporary_bounded_and_never_saved_as_a_user_workspace` (`crates/desktop/src/laboratory/tests.rs:149`) indexes the first report after 200 updates, but no report has been produced by that cutoff.

The related store/laboratory code and concurrent workspace edits were left intact.

Follow-up status: all nine implementation tasks, focused checks and the final native sound/DPI walkthrough are complete. The two broader desktop test failures above remain documented; no mobile sound integration was added.

## Linux integration, minimized operation and load verification

Run these steps sequentially on the existing branch. Keep directly scheduled work equal to recurring work in querying, rendering, selection and alerts. Fix regressions uncovered by this verification and record measurements, captures and environment limitations.

1. [x] Run the Linux scheduling, metadata, projection/cache, privacy and shared clock/sound suites.
2. [x] Add full recurrence integration through the real Engine, schedule subscription, native clock model and alert queue. Exercise admission, current work, materialization, source updates and multiple clocks without duplicate alerts. Keep standalone scheduled points, explicit intervals and estimated deadlines visible and actionable with or without simulation.
3. [x] Verify native Linux alerts while the test window is actually minimized, using an isolated window session. Check scheduled and projected titles, rescheduling/removal, duplicate clocks, restoration and clean shutdown.
4. [x] Run graphical load checks for representative dense and adversarial overlap layouts at ordinary, wide and short apertures. Measure settled frame times, update cost, texture allocation and asset/entity stability; record bounded allocations and remaining performance work.
5. [x] Capture and inspect different physical resolutions, DPI and fractional zoom. Verify titles, times, tiny units, overlapping bands and connectors without changing the clock footprint.
6. [x] Repeat the affected checks, document actual results and remaining platform limitations, and preserve unrelated shared-branch edits.

### Integration changes

- Read confirmed recurrence applications from the execution ledger, independently of the simulation cache. Retain an admitted interval until its actual duration ends, preserve the forecast occurrence identity, exclude unauthorized records before applying the bound, and suppress materialized duplicates. Use a set when suppressing confirmed entries from the cache.
- Carry the active execution clock into background projection jobs. Controlled simulations and their live subscriptions now calculate and expire their caches against the same clock.
- Convert schedule entries into sound cues in the shared interface model. Native playback and integration tests use the same conversion, including confirmed/projected classification and canonical occurrence identity.
- Preserve selection through forecast admission and materialization. When the selected occurrence acquires an actual Record, notify an opted-in Record receiver with that Record while keeping round-clock configuration closed.
- Exercise real Records with zero quantity, explicit start/end ranges, estimates, signed execution authorization, two schedule subscriptions, live recurrence admission and materialization. Verify complete labels, actionable selection, quiet initial loading, one alert per occurrence, cache reuse and private actor filtering.
- Add `time_castle_linux_smoke` with isolated temporary storage. Its minimized modes use an actual X11 iconic window and pause drawing across three deadlines; its load mode captures six aperture/DPI/zoom stages and measures frame time, visible labels, sampling density, capture memory and restored asset counts. Native playback retains at most 64 diagnostic start records so minimized timing can be checked after restoration.
- Initialize native speech when future title cues are registered, including saved settings restored without opening configuration. Initialization now happens before the first deadline. Spoken titles retain their sequential queue; the minimized check verifies completion as well as starts.

### Linux correctness and minimized playback results

- The repeated Linux domain run passed **66 tests**: scheduling 5, work metadata 19, projection budget 1, real projection/subscription integration 6, schedule backend 4, shared library 15 and clock models 16. The new native recurrence/subscription/selection test and the existing focused native tests passed **26 tests**. Shared surface tests passed **11 tests**. These groups contain **103 distinct focused checks**.
- The real native integration verifies that an opted-in Record receiver follows a forecast when it becomes a materialized Record. It also inserts, reschedules and removes zero-quantity scheduled work through the live subscription, checking pending deadlines and cancellation rather than just changing a fixture array.
- Actual minimized playback used an isolated rootful Xwayland/Openbox session with the **Intel Iris Xe / Vulkan / Mesa 26.1.2** adapter. Both clocks held the same three future occurrences. The window reported `WM_STATE=Iconic`, and drawing was paused across all deadlines while the live recurrence director and sound worker continued.
- The blip run produced three starts, two confirmed scheduled and one projected recurring, with start delays **95 ms, 1 ms and 0 ms**. Restoration produced no replay. The spoken-title run produced three starts and three completed speeches, with delays **8 ms, 1 ms and 5 ms**, including the projected prefix, and no native sound error or replay.
- The first title attempt initialized Speech Dispatcher at its first deadline and failed the strict timing check. Preparing speech when a future title schedule arrives removed that delay. The test allows later titles to wait for an earlier speech to finish, while retaining a 500 ms bound for idle playback; all three measured starts in the final run were within 8 ms.
- Xvfb could not present the hardware Vulkan adapter because it lacks DRI3. This was a display-harness limitation; actual minimized verification succeeded under Xwayland. The title/blip checks use temporary data and close their own windows and pools.

### Density, resolution and performance results

The final native runs for **48 and 128 real scheduled Records** both completed six captures: one-hour aperture, ten-hour aperture, 1.5 display scale with 0.75 workspace zoom, 2 display scale with 1.25 zoom, one-minute aperture and restored normal scale. Titles included Latin accents and Japanese, and data combined equal-time points with overlapping intervals. Both runs asserted nonintersecting cards, the unchanged **420 × 420 logical clock**, at least one sample per physical pixel for visible text, the 4096-pixel dimension cap and bounded retained assets after restoration.

All 48 and all 128 titles were on screen at normal scale, the ten-hour aperture and the fractional-zoom stage in the 3600 × 2000 test window. One point elapsed during each run, leaving 47 or 127 future/current entries on restoration. At 2 display scale with 1.25 zoom, the final 48-event processing run contained **33 fully visible cards out of 47**; the 128-event run contained **27 out of 127**. Cards remain full text and separate, but large zoom and high density can exceed the viewport. This is a remaining packing/space limit, not a claim that arbitrarily many full titles fit on any screen.

The largest observed capture allocation was **24,527,143 pixels**, below the shared **33,554,432-pixel** cap. Font atlases grew from 13 MiB to 37 MiB as new physical font sizes were exercised. These are cached glyph images; subtracting their known atlas count from total images showed stable capture assets after restoration. The mesh/material counts also remained bounded. The harness originally mistook font-cache growth for a capture leak; its corrected assertion compares assets separately from those atlases.

The 48-event processing run measures from the beginning of `First` to the beginning of its `Last` verification system, separately from wall-clock frame intervals. This includes main-app update work, and excludes later extraction/render work and background waits. It uses the workspace's **unoptimized debug profile**, with 4–9 settled samples per stage; it is diagnostic data rather than a release-frame-rate benchmark.

| Aperture / display scale / zoom | Update median | Update p95 | Capture pixels |
| --- | ---: | ---: | ---: |
| 1 hour / 1 / 1 | 33.42 ms | 335.74 ms | 3,547,200 |
| 10 hours / 1 / 1 | 220.23 ms | 222.72 ms | 3,547,200 |
| 1 hour / 1.5 / 0.75 | 286.80 ms | 298.57 ms | 7,089,456 |
| 1 hour / 2 / 1.25 | 642.54 ms | 896.72 ms | 24,055,100 |
| 1 minute / 2 / 1.25 | 558.35 ms | 650.23 ms | 6,589,464 |
| Restored 1 hour / 1 / 1 | 224.49 ms | 290.38 ms | 3,488,000 |

Wall-clock intervals often approached one second while the nested display was off-screen. Do not report those intervals as renderer execution time. The separate update measurements still show significant cost at large physical scales, so performance is **not considered resolved** by passing the memory and correctness checks.

Native captures were inspected at `/tmp/time-castle-linux-12-0.png`, `-12-3.png`, `-48-0.png`, `-48-1.png`, `-48-2.png`, `-48-3.png` and `-128-3.png`, using the common `/tmp/time-castle-linux` prefix. Normal and large-scale captures show readable titles, original ranges, divisions, aperture labels and midpoint/point connections. The ten-hour capture confirms that the public aperture setter also expands the horizon; assigning an invalid aperture larger than the horizon had caused the first harness attempt to retain its old face. Japanese renders correctly, but the current upstream Parley/ICU combination reports its missing complex-script segmentation model during layout. That diagnostic remains.

### Final checks and remaining work

- Repeated the native clock suite after speech preparation: **26 passed**, including default Off not initializing a native sound device. The earlier domain and surface runs remain green, totaling **103 distinct focused tests**.
- `cargo check -p lince-desktop --tests --example time_castle_linux_smoke --example time_schedule_smoke` passed with warnings denied. A final example-only check passed after adding separate update timings. Rust formatting and the affected-file `git diff --check` passed. No `cargo build` was used.
- Both final load runs printed `LOAD PASS all six stages captured`; minimized blip/title runs printed `MINIMIZED PASS`. The harness propagates an unsuccessful app exit and avoids advancing past its final capture. All isolated display processes and windows were closed.
- Concurrent Instinct/practice edits caused repeated transient compilation failures and build-lock waits. Small compile fixes were necessary to run these checks; unrelated feature changes were preserved.

Follow-up tasks, in order:

1. [ ] Profile an optimized foreground run, separating static face/readout rasterization, label layout, Bevy UI capture and GPU rendering. Compare no-event, 48-event and 128-event cases at identical physical scales, with sufficient samples and Latin/CJK cases separated.
2. [ ] Reduce the measured raster/layout cost at large physical scales while preserving supersampling, current countdowns, pointer motion and the fixed clock. Re-run the same captures and asset checks after each measured improvement.
3. [ ] Improve dense card distribution where unused viewport space exists. Keep full text, event anchors and the fixed clock; measure visibility explicitly instead of silently treating off-screen cards as readable. Document the finite space limit when full titles exceed the available area.
4. [ ] Address the upstream complex-script segmentation diagnostic without losing Japanese glyph coverage or changing the displayed Record text.

Linux verification and recurrence/scheduled-work integration are complete. Rendering cost, extreme zoom/density visibility and the upstream segmentation diagnostic remain explicit follow-up work.
