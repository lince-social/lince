# Stage 002 · Save and reopen a tiny scene

Open **Edit → Workspaces → Scene document**. Choose **New scene**, enter a part name and choose **Create part and place it**. **Place another copy of this part** adds an independently selectable placement of that same definition. Use the inspector to enter local X, Y and Z in metres and an optional Record UID. **Apply to selected placement**, **Undo scene edit**, **Redo scene edit**, **Save scene** and **Open directory** act on that document. The default directory belongs to the active desktop workspace. **Close scene inspector** folds the section and retains the in-memory draft.

The supplied [two-screw scene](two-screws.workspace/) can be opened directly. The tree labels and inspector were captured from a fresh desktop process. [Tree](tree.png) · [Inspector](inspector.png) · [Narrow viewport](narrow.png)

## Result

| Placement | Coordinates after reopen (m) | Binding |
| --- | --- | --- |
| `slot_01M821ZMAH9S7R5KZQ5Z7X09NW` | (0, 0, 0) | Empty |
| `slot_01M821ZMGE8CZBMW1S2H7JTSM7` | (0.125, −0.002, 0) | Synthetic Record `r_01M821ZMTV88Z9ANQCMSQF5JSK` |

Both handles retain root `root_01M821ZM5C8SBV6SMCJYQ558BA` and resolve definition `part_01M821ZMAH8B4T5013AK9VYMMH` at revision `cdbe09559c16…`. Undo restored the original coordinates and redo restored the move. Independent decoding of the retained pages verified both root generations, domain hashes, shared definition, original handles and exact decimal coordinates. [29 native Edit tests, 5 storage checks, source fingerprints and observations](verification.json)

## Directory publication and recovery

The single-root composition profile stores stable document, root, definition and slot IDs using Lince's UID conventions. A placement pins its definition revision. Local positions use normalized exact decimals in metres with up to 18 decimal places. A plain part exposes local origin element zero. This slice admits 64 definitions and 128 placements per document and 65,536 encoded bytes per structured file. The tree represents model placements. Stage 003 owns their connection to existing canvas items and Rooms.

**Save** writes immutable canonical definition and placement pages, verifies any reused content, syncs the pages and manifest, then atomically replaces and syncs the directory's `HEAD`. A process-level lock and expected revision serialize publication. The kept scene has 2,056 source bytes across its head and both generations. A reader resolves the head and checks the supported profile, units, page hashes, record and slot identities, and pinned definition closure. It ignores temporary files and unreachable pages. [Model](../../../../crates/nucleus/src/canvas/scene.rs) · [Storage](../../../../crates/store/src/workspace_scene.rs)

An automated child process exited after the replacement head had been written and flushed to a temporary file, immediately before publication. Another process opened the previous revision, verified both placements and successfully saved a subsequent edit. A concurrent-writer test accepted one successor. The native conflict test kept the local draft and saved it to a new directory while preserving the other writer's published work. Missing pages, damaged hashes, broken definition references, oversized input and unknown required profiles were rejected. [Recovery tests](../../../../crates/store/src/workspace_scene/tests.rs) · [Native conflict replay](../../../../crates/interface/src/native/edit_mode/scene/tests.rs)

## Replaying it without an operator

From the repository's interface development shell, run the following with a fresh output path. Rust 1.99.1 used the repository's existing workspace compiler wrapper. [Linux build configuration](../../../../.github/actions/linux-build/action.yml) The command launches two native Lince processes, drives the ordinary Edit actions and text controls, and exits after verifying the restored inspector. This run used a separate Weston headless display on the local Intel GPU. The focused Apply control was reachable at both 1000 × 900 and 360 × 700. The Edit panel began at the upper-left and ended above the desktop controls in both layouts. The Stage 001 native drag, gravity, attraction, workspace isolation and sleep replay also passed on this checkout. [Replay](../../../../crates/sandbox/examples/workspace_scene.rs) · [Exact layout and comparisons](verification.json)

```sh
cargo run --locked -p lince-sandbox --example workspace_scene -- target/scene-replay-01
cargo test --locked -p store --lib workspace_scene::tests
cargo test --locked -p lince-interface --features native-presentation --lib native::edit_mode::
```

The wider Edit suite exposed two stale test fixtures that also failed on the Stage 001 binary. Its fractional-width check now asserts the declared flush-left panel geometry. Its keyboard test runs Bevy visibility propagation before navigating the newly created controls. Both passed with the full Edit suite.

## Measured cost

The local development build ran 40 serialized warm save/open pairs on a two-placement scene. Each pair changed a coordinate, flushed the new root, reopened the directory and validated the complete referenced scene. Save p50/p95/p99 were **18.075 / 20.501 / 21.526 ms**. Open p50/p95/p99 were **0.584 / 0.700 / 0.865 ms**. Test startup and UI rendering are outside those boundaries. Raw times, process memory high-water marks, whole native replay wall time and hardware are in the measurements. The two-process UI run used an existing OS page cache. Process RSS includes the runtime and does not isolate parser heap allocation. [Raw measurements](verification.json) · [Reference budget](../stage01/profile.json)

The undo stack retains up to 16 local edits. A snapshot shares its definition and placement vectors until mutation. The selected-copy test verified that moving one occurrence retained the same definition allocation and copied the bounded placement page. The fixture contains authored names, exact coordinates and synthetic IDs. It carries no Record contents or permissions. [Implementation and measured copy counts](verification.json)
