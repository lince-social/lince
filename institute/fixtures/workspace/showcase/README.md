# Explore the Workspace showcase

Open **Laboratory → showoff workspace** in the native desktop, or launch the binary with `lince --showoff`. The showcase is a saved, editable desktop workspace. A fresh profile is convenient for trying the original layout again.

| Stage | Try it yourself | What to look for |
| --- | --- | --- |
| [001 · Desktop baseline](../../../trails/lince/workspace/stages/001-re-establish-the-desktop-baseline.lingua) | Move a “Physics” Sand into the circular Area. Raise the Gravity Sand using its Y control and release it. Use the 3D view to inspect its height. Switch to Home and back. | The Area attracts its matching Sands. The Gravity Sand falls and settles. The other workspace keeps its own contents. |
| [002 · Tiny scene](../../../trails/lince/workspace/stages/002-save-and-reopen-a-tiny-scene.lingua) | Open **Edit → Workspaces → Scene document → Try the bracket example**. Inspect both slots. To build your own, start a new scene, create a part, and place another copy. Edit one occurrence, undo, redo, save and reopen. | Both occurrences use one definition. Their positions and selection are independent. Use **Save to a new directory** to keep a separate draft. |
| [003 · Room binding](../../../trails/lince/workspace/stages/003-edit-an-existing-canvas-item-through-the-model.lingua) | Drag the example’s **Bracket** Sand in Edit mode. In a scene built from scratch, select a slot, select the Bracket, then choose **Link selected Room member to this placement**. Undo the drag and reopen the saved scene after restarting. | The tree selects the linked occurrence, the Room supplies its coordinates, and undo restores the visible Sand. Its Record attachment and sibling occurrence survive the restart. |

To try the scene revision conflict, open the same saved directory in Home and in showoff. Make and save an edit in showoff. Edit the older draft in Home and choose **Save scene**. The inspector reports the conflict and offers to reopen the published scene or save the draft to a new directory.

The workbench, physics Area, shader, map and imported sculpture share one canvas. Try moving the Bracket while the Area is active. Only the matching Physics Sands respond to the Area. Reopening an older showoff workspace adds the workbench once and preserves existing edits. The example uses its own synthetic Record UID.

## Run the checks

Build the normal desktop in the interface development shell with `cargo build --locked -p lince --bin lince`. To run the cumulative end-to-end check with a display, use `cargo xtask workspace showcase target/showcase-replay-01` from the repository root in that shell. Supply a fresh output directory. The replay opens Laboratory, activates its showcase launcher, uses the published exhibit and actual scene controls, and restarts the native application with the same temporary profile. The output includes the saved scene, observed drag coordinates and a screenshot after reopening. [Replay entry point](../../../../crates/sandbox/examples/workspace_scene.rs) · [Room and canvas assertions](../../../../crates/sandbox/examples/workspace_scene/bridge.rs)

Earlier isolated evidence remains available for [baseline timing and settling](../stage01/resumption-2026-11-22/README.md), [interrupted scene publication and competing writers](../stage02/README.md) and [Room-local edits and stale gestures](../stage03/README.md).

## Verified on 2026-11-22

The Linux desktop binary opened a clean profile directly into showoff and saved its workbench Room, member and synthetic Record attachment. The cumulative replay moved the linked bracket from (5, 0, 2) to approximately (6.231, 0, 2.492) metres while the gravity and attraction examples ran. It restored the original bracket with undo, saved the redo, switched workspaces, and reopened the restored binding and both occurrences in a fresh process. The older-workspace check retained a user’s custom Sand and respected a deliberate deletion on the next visit. [Reopened desktop](reopened.png) · [Captured values and test results](verification.json)

All 82 focused native checks and 5 scene storage checks passed. The Stage 001 dataset passed its 10 fixture cases, and the original scene replay and narrow inspector checks passed. These runs verify behavior. Stage 001 owns the separate isolated performance measurements.
