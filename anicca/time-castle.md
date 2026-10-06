# Clock requests

Implement these requests in order, using the existing clock and Protein scheduling data.

Keep the earlier requested behavior: one aperture setting and one timezone; centered, scrollable upcoming tasks without “Next X things” or a time-window caption; the skull and curved motto; cards attracted to their time point or range midpoint with two-dimensional repulsion, optional physics, and hover-only display. Cards must remain draggable, may leave the screen when the camera moves, and must not lift or push the clock. Past tasks accompany the cursor by default and can be disabled. Keep moving text and geometry clear at different resolutions.

1. [x] Curve “memento mori” more strongly beneath the skull. The ends should turn by roughly 60 degrees from the middle.
2. [x] Show outstanding needs at the clock cursor when their scheduled time passes. Inspect a copy of the current data, including “Passar Fio Dental” at quantity −1 with a daily Karma rule of −1, and repair the existing mechanism where possible. Completing a need must remove the corresponding outstanding quantity.
3. [x] Simplify the floating cards’ inertia. After dragging, moving the clock, or changing its tasks, motion should gradually cool to a stop even without collisions. With a stationary aperture and moving cursor, future cards should stay in place. Preserve two-dimensional repulsion, card dragging, the physics toggle, and freedom to move off screen.
4. [x] Make “Schedule source” open a temporary Protein Castle to configure the clock’s source and filters.
5. [x] Run the relevant correctness checks and native visual tests with automated interactions, including the stronger curve, outstanding Karma needs, cooling, and temporary source editor. Use offscreen rendering without external keyboard or mouse emulation.

Update each checkbox when its implementation and verification are complete. Do not modify the live user data.

The data copy confirms that “Passar Fio Dental” has no manual work dates and its daily rule sets its quantity to −1. The clock now reads current needs from the schedule feed and keeps their outstanding quantity at the cursor, including after their original time has left the aperture. The rule itself and the live data were not changed.

Correctness checks passed: `cargo check` without warnings, 32 desktop clock tests, 7 pointer tests, 20 clock model tests, and 5 schedule tests, including source privacy and bounded dense layouts.

The native automated visual test passed with offscreen software Vulkan and 27 captures. It verified scrolling, dragging without snapping or clock drift, picking cards over transparent clock corners, cooling to rest, camera movement, zoom and resolution changes, hover-only cards, the physics toggle, a daily Karma need becoming outstanding and then satisfied, and the temporary source editor applying its filters. Settled layouts with 13, 14, and 15 cards have no overlapping text.

Review the [visual report](../target/clock-visual-verification/report.json), [outstanding floss need](../target/clock-visual-verification/17-floss-outstanding.png), and [temporary source editor](../target/clock-visual-verification/19-source-editor.png). All requested items are complete.
