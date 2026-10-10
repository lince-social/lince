# Clock requests

Implement these requests in order, using the existing clock and Protein scheduling data.

Keep the earlier requested behavior: one aperture setting and one timezone; centered, scrollable upcoming tasks without “Next X things” or a time-window caption; the skull and curved motto; cards attracted to their time point or range midpoint with two-dimensional repulsion, optional physics, and hover-only display. Cards must remain draggable, may leave the screen when the camera moves, and must not lift or push the clock. Past tasks accompany the cursor by default and can be disabled. Keep moving text and geometry clear at different resolutions.

1. [x] Curve “memento mori” more strongly beneath the skull. The ends now turn by 90 degrees from the middle, following the latest request. The motto’s arc and skull share a center and stay aligned as the clock changes size.
2. [x] Show outstanding needs at the clock cursor when their scheduled time passes. Inspect a copy of the current data, including “Passar Fio Dental” at quantity −1 with a daily Karma rule of −1, and repair the existing mechanism where possible. Completing a need must remove the corresponding outstanding quantity.
3. [x] Simplify the floating cards’ inertia. After dragging, moving the clock, or changing its tasks, motion should gradually cool to a stop even without collisions. With a stationary aperture and moving cursor, future cards should stay in place. Preserve two-dimensional repulsion, card dragging, the physics toggle, and freedom to move off screen.
4. [x] Make “Schedule source” open a temporary Protein Castle to configure the clock’s source and filters.
5. [x] Run the relevant correctness checks and native visual tests with automated interactions, including the stronger curve, outstanding Karma needs, cooling, and temporary source editor. Use offscreen rendering without external keyboard or mouse emulation.

Update each checkbox when its implementation and verification are complete. Do not modify the live user data.

The data copy confirms that “Passar Fio Dental” has no manual work dates and its daily rule sets its quantity to −1. The clock now reads current needs from the schedule feed and keeps their outstanding quantity at the cursor, including after their original time has left the aperture. The rule itself and the live data were not changed.

Follow-up fixes requested after the cards became visible:

6. [x] Stop recurring forecast disappearance. Sync delivery bookkeeping must not invalidate the forecast unless a Karma program can read it. Keep the previous future schedule visible during recalculation, applying current source filters, visibility, completions, and deletions. Renewing the same source’s time window must not clear the display.
7. [x] Use simple elapsed-time cooling. Clock ticks and unchanged schedule snapshots must not restart motion. Settled past cards accompany the cursor as a group; future cards remain still with a stationary aperture until interaction or contact requires adjustment. Preserve dragging and the physics toggle.
8. [x] Remove repeated work: reuse card measurements, skip sleeping spring forces, avoid unchanged opacity updates and connector meshes, and refresh the stationary clock face once a minute when its rim labels change on minute boundaries. Shorter and fractional windows keep their faster refreshes.
9. [x] Verify the fixes with focused regression tests and native offscreen rendering using a private copy of the current data. Force a forecast refresh and check every frame for missing cards or an empty upcoming list.

Follow-up verification passed: 34 desktop clock tests, 6 schedule tests, 3 shared motion tests, and the projection invalidation regression test. `cargo check` passed with warnings treated as errors. The copied-data native test retained all 8 outstanding needs and 5 future tasks across 215 frames, including the updating state; sync retries caused no extra forecast builds, future cards stayed still, and card entities were retained. Changing aperture through 20 → 21 → 24 → 20 hours also retained every card throughout each refresh. The full native interaction test passed dragging, cooling, camera movement, zoom, hover-only display, the physics toggle, Karma admission and completion, and the temporary source editor.

See the [refresh report](../target/clock-refresh-verification/refresh-report.json), [stable copied clock](../target/clock-refresh-verification/refresh-stable.png), [aperture restored](../target/clock-aperture-verification/aperture-1200.png), and [source filter applied in the native test](../target/clock-interaction-verification/20-source-applied.png). The private database copy was removed after testing. The live data was not modified.

Responsive layout requests:

10. [x] Bring the motto to a semicircle with 90-degree ends, closer to the skull. Center the digital time, upcoming list, and skull with motto together, using fixed gaps.
11. [x] Hide the skull and motto first as the clock shrinks, then the upcoming list. The smallest clock shows only digital time. Remove the central task rows’ lighter backgrounds.
12. [x] Fit floating card widths to their text, wrapping longer titles. Prevent overlaps while cards settle and while past cards follow the cursor toward future cards, preserving dragging and optional physics. Keep contact separation active after cooling without restarting the springs.
13. [x] Visually test multiple sizes, including wide and tall clocks, small clocks, and restoration to the original size. Check card bounds with physics enabled and disabled.

Responsive verification passed: `cargo check`, 37 desktop clock tests, and 21 shared clock tests. The centered face uses 16-pixel gaps and measures upcoming rows at their actual font size. Resizing does not mark unchanged layout nodes as dirty. A three-hour cursor regression keeps all 13 cards separated without restarting cooling.

Native offscreen rendering with a private copy of the current data passed 13 resize cases from 80 to 800 pixels, including wide and tall clocks and restoration to 420 pixels. Cards fit their text and stay separated with physics disabled and while physics cools. See the [resize results](../target/clock-responsive-verification/sizes.json), [large clock](../target/clock-responsive-verification/size-800x800.png), [tasks without the motto](../target/clock-responsive-verification/size-260x260.png), and [digital-only clock](../target/clock-responsive-verification/size-80x80.png). The private data copy was removed after testing; the live data was not modified.
