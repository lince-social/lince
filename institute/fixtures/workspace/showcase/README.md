# Explore the Workspace showcase

Open **Laboratory → showoff workspace** in the native desktop, or launch the binary with `lince --showoff`. The showcase is a saved, editable desktop workspace. A fresh profile is convenient for trying the original layout again.

| Stage | Try it yourself | What to look for |
| --- | --- | --- |
| [001 · Desktop baseline](../../../trails/workspace/stages/001-re-establish-the-desktop-baseline.lingua) | Move a “Physics” Sand into the circular Area. Raise the Gravity Sand using its Y control and release it. Use the 3D view to inspect its height. Switch to Home and back. | The Area attracts its matching Sands. The Gravity Sand falls and settles. The other workspace keeps its own contents. |
| [002 · Tiny scene](../../../trails/workspace/stages/002-save-and-reopen-a-tiny-scene.lingua) | Open **Edit → Workspaces → Scene document → Try the bracket example**. Inspect both slots. To build your own, start a new scene, create a part, and place another copy. Edit one occurrence, undo, redo, save and reopen. | Both occurrences use one definition. Their positions and selection are independent. Use **Save to a new directory** to keep a separate draft. |
| [003 · Room binding](../../../trails/workspace/stages/003-edit-an-existing-canvas-item-through-the-model.lingua) | Drag the example’s **Bracket** Sand in Edit mode. In a scene built from scratch, select a slot, select the Bracket, then choose **Link selected Room member to this placement**. Undo the drag and reopen the saved scene after restarting. | The tree selects the linked occurrence, the Room supplies its coordinates, and undo restores the visible Sand. Its Record attachment and sibling occurrence survive the restart. |
| [004 · Accepted scene edits](../stage04/README.md) | Save the Bracket scene and open **Accepted edits and measurements**. Publish a copy with an interrupted reply, retry it, then undo it. Hold a measurement, move the Bracket, measure again and finish the held job. | The retry returns the original slot and receipt. The newer distance survives the late completion. Reopen the saved scene to inspect a receipt by request UID and see who made the edit. |
| [005 · Reuse a definition](../stage05/README.md) | Open **Definition library → Browse the example parts library**. Review, import and place both Brackets. Publish a move on one, inspect their Steel grades and review the Coated bracket. | The two Brackets share Millimetre, keep their own grade and can move independently. A private coating blocks the dependent import. The destination reopens with both grades even with the source offline. |

To try the scene revision conflict, open the same saved directory in Home and in showoff. Make and save an edit in showoff. Edit the older draft in Home and choose **Save scene**. The inspector reports the conflict and offers to reopen the published scene or save the draft to a new directory.

The workbench, physics Area, shader, map and imported sculpture share one canvas. Try moving the Bracket while the Area is active. Only the matching Physics Sands respond to the Area. Reopening an older showoff workspace adds the workbench once and preserves existing edits. The example uses its own synthetic Record UID.

## Run the checks

Build the normal desktop in the interface development shell with `cargo build --locked -p lince --bin lince`. To run the cumulative end-to-end check with a display, use `cargo xtask workspace showcase target/showcase-replay-01` from the repository root in that shell. Supply a fresh output directory. The replay opens Laboratory, activates its launcher and exercises the visible controls across five desktop processes sharing one profile. It captures the Room drag, native and Fiote receipts, the retained measurement, both imported definitions and the reopened scene with its source library offline. [Replay entry point](../../../../crates/sandbox/examples/workspace_scene.rs) · [Room and canvas assertions](../../../../crates/sandbox/examples/workspace_scene/bridge.rs) · [Definition library assertions](../../../../crates/sandbox/examples/workspace_scene/library.rs)

Earlier isolated evidence remains available for [baseline timing and settling](../stage01/resumption-2026-11-22/README.md), [interrupted scene publication and competing writers](../stage02/README.md) and [Room-local edits and stale gestures](../stage03/README.md).

| Native replay phase | Observable result in the shared scenario |
| --- | --- |
| `--showcase-create` | Open the example, build a second scene through the editor, link its slot to the Room, drag with physics active, undo, redo and switch workspaces. |
| `--showcase-reopen` | Resolve that Room member after restart. Recover a lost reply, reject a late measurement, then use the local Fiote and native undo on the saved scene. |
| `--showcase-recover` | Recover the earlier native and Fiote receipts by identity and inspect the current measurement. |
| `--showcase-import` | Compare two source revisions, import and place them, change a local dependency and publishing permission, then check the new placement’s accepted move. |
| `--showcase-library-reopen` | Inspect both materials with the source directory offline. Check the earlier slots, Record, binding and imported placement against their saved identities. Click **Place this definition** and undo the new occurrence. |

## Verified through Stage 005 on 2026-11-22

The Linux binary opened the editable Showoff. Five native processes built the earlier exhibits, imported both Brackets and reopened their grades with the source offline. A pointer click in the narrow editor placed one copy and Undo restored the previous six placements. The upgrade check used the workspace saved by the Stage 004 binary. The 40 warm saves had a 30.469 ms p95 against the 200 ms limit. [Workbench and evidence](../stage05/README.md) · [Second Bracket import review](../stage05/definition-review.png)

### Earlier verification through Stage 004

The normal desktop binary opened the same workbench described above. The cumulative replay built and linked the Bracket, recovered an edit after an interrupted reply, ignored an obsolete measurement, then used the local Fiote’s Canvas tool and native undo on the same saved scene. A third desktop recovered earlier receipts from both Actors. All 84 focused native cases and nine functional storage cases passed. Forty measured durable edits each met the 200 ms budget. The fixture also identifies three failures reproduced in the broader Laboratory suite on a pre-Stage-004 binary. [Stage 004 results](../stage04/README.md) · [Recovered desktop](../stage04/recovered-receipt.png)

### Earlier verification through Stage 003

The Linux desktop binary opened a clean profile directly into showoff and saved its workbench Room, member and synthetic Record attachment. The cumulative replay moved the linked bracket from (5, 0, 2) to approximately (6.231, 0, 2.492) metres while the gravity and attraction examples ran. It restored the original bracket with undo, saved the redo, switched workspaces, and reopened the restored binding and both occurrences in a fresh process. The older-workspace check retained a user’s custom Sand and respected a deliberate deletion on the next visit. [Reopened desktop](reopened.png) · [Captured values and test results](verification.json)

All 82 focused native checks and 5 scene storage checks passed. The Stage 001 dataset passed its 10 fixture cases, and the original scene replay and narrow inspector checks passed. These runs verify behavior. Stage 001 owns the separate isolated performance measurements.
