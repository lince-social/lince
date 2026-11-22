# Stage 003 · Edit an existing canvas item through the model

## Use it on the desktop

1. Start with a local Room and an existing, ungrouped Sand that is a direct member of that Room. Enter Edit mode.
2. Open **Workspaces → Scene document** and create or open a scene. Select the model placement you want to link. Select the existing Sand on the canvas and choose **Link selected Room member to this placement**.
3. The inspector shows the Room and canvas placement identities. Linking copies the Sand’s current Room-local translation and optional Record UID into the chosen model occurrence. A conflicting Record UID leaves both unchanged.
4. Select and drag the Sand. On release, the placement inspector shows the resulting translation in metres. **Undo scene edit** restores the whole drag in one step. **Redo scene edit** reapplies it. Editing X, Y or Z in the inspector moves the same Sand.
5. Choose **Save scene**. After restarting Lince in the same desktop workspace, choose **Open directory**. Selecting the linked slot selects the existing Sand. A scene opened without its matching Room and Sand reports the missing binding and keeps the authored scene available.

## Scope

The optional `canvas-room/1` profile associates one scene root with a saved Room placement ID and one slot with a saved Sand placement ID. Each lookup is scoped to the active desktop workspace. The bridge checks identity uniqueness, direct membership and Record agreement before projecting onto the canvas. The Room supplies the origin and orientation. The native rendering convention is 100 canvas units per metre, independent of zoom. The scene stores exact decimal coordinates. Pointer input and canvas projection use floating point.

The desktop workspace saves the Room, Sand and Record attachment through its existing persistence path. **Save scene** publishes the model translation and binding together using the scene directory’s atomic head. Opening that directory reapplies its published translation to a matching desktop placement. Undo history lasts for the local editing session. Nested anchors, hosted edits, grouped movement and binding several canvas items are outside this slice.

## Replay

From the interface development shell, use a fresh output directory.

```sh
cargo run --locked -p lince-sandbox --example workspace_scene -- target/room-bridge-replay --bridge
cargo test --locked -p lince-interface --features native-presentation --lib native::edit_mode::
cargo test --locked -p store --lib workspace_scene::tests
cargo test --locked -p nucleus --lib room_hierarchy_rejects_cycles
```

The native replay creates two placements of one bracket and two text Sands. One Sand supplies the Room and the other carries a synthetic Record attachment. It sends pointer presses, moves and releases through Lince’s input path. It checks scene selection, the displayed coordinates, restoration in one undo, equality of the entire saved scene, and reacquisition of the same canvas placement in a fresh desktop process. Its output includes the observed drag coordinates and a screenshot from the reopened workspace.

## Observed on 2026-11-22

The bracket moved from canvas X/Z (240, −100) to (320, −68). In the Room frame this changed (5, 0, 1) m to (5.8, 0, 1.32) m. One undo restored (5, 0, 1) m. A new process recovered the original slot, the same canvas placement and Room IDs, and the same Record attachment. The sibling bracket remained at the origin. [Captured identities and coordinates](replay.json) · [Reopened desktop](reopened.png)

The focused suites passed 30 native Edit tests, 34 selection, Room and pointer tests, 5 storage tests and 1 hierarchy validation test. The interrupted-writer test terminated a child process before publishing the new head and recovered the full bound scene. The ordinary Stage 002 create/reopen replay and the 360-pixel-wide inspector replay also passed. Lingua validated all 309 workspace Records.

The replay ran on an offscreen Weston desktop and supplied native pointer events. Wayland declined an OS cursor warp. The checks used the actual picked entity and its resulting canvas position. Concurrent development made this a correctness run. The isolated storage benchmark was skipped.
