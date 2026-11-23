# Stage 004 · Recover an accepted scene edit

Open **Laboratory → showoff workspace → Edit → Workspaces → Scene document**. Choose **Try the bracket example**, save it to a new directory, then open **Accepted edits and measurements**.

| Try it | Visible result |
| --- | --- |
| Choose **Publish a copy and interrupt its reply**. Choose **Retry the same request**. | The tree gains one placement. Its receipt identifies the new slot, the local operator, the original request and the accepted revision. Retrying keeps the same slot. **Undo the accepted edit** removes just that copy. |
| Select the linked Bracket and choose **Hold a distance measurement**. Enter 6, 0 and 8 in the metre fields and choose **Publish placement coordinates**. Choose **Measure distance from origin**, then **Finish the held measurement**. | The Bracket moves in its Room. The inspector shows 10 m with its generation and source revision. The earlier completion is discarded and the 10 m result stays visible. Undo the accepted move to restore the Bracket. |
| Give a local Fiote access through its ordinary native tools and ask it to inspect the saved scene, place another copy or move it. | The same inspector displays the resulting slot and attributes the receipt to that Fiote and conversation. Native undo can restore the accepted placement. The replay exercises the registered tool directly. |
| Restart Lince, open the same scene and choose **Recover last accepted receipt**. For an earlier operation enter its identifier under **Request UID** and choose **Find accepted receipt**. | The original Actor, slot and accepted revision reappear. Looking up an old receipt leaves later edits in place. |

## Operation and publication

The existing local Canvas Action takes a workspace ID and a scene query. Inspect returns the saved document, source revision, occurrence handles, coordinates and available operations. Submit captures the document, expected revision, stable request UID and one typed operation. Place-copy creates a new occurrence of a pinned part. Move updates only that occurrence’s local metres and preserves its Record. Undo inverts the specified edit at its accepted head. The Canvas host supplies the Fiote and thread identities from the calling tool session. A pending native draft survives programmatic attempts to publish over it. [Command types](../../../../crates/nucleus/src/canvas/scene/operation.rs) · [Local engine acceptance](../../../../crates/engine/src/canvas/scene_edits.rs) · [Native inspector](../../../../crates/interface/src/native/edit_mode/scene/accepted.rs)

The scene directory publishes its source revision and receipt index by replacing one synced HEAD. Requests use the directory’s existing writer lock. Immutable index pages branch on the request UID, with at most 32 entries per leaf and 26 branch levels. An identical retry reads the stored receipt and current scene. A reused ID with different parameters or Actor fails. Ordinary native saves retain published receipts. Existing scene directories acquire the index on their first accepted operation. [Atomic publication and recovery](../../../../crates/store/src/workspace_scene/acceptance.rs)

A measurement captures the document, source revision, occurrence and a fresh generation ID. The desktop keeps one held job. A bounded MEASUREMENT file records the current invocation and its completed distance. Completion checks both the accepted source and live generation under the scene writer lock. Reopening the same placement recovers its measured distance and generation. Changing selection or editing the draft retires a pending result in that inspector. Remote document grants and delegated Agent lifetimes have their own numbered stages. This local workflow runs without a language-model download.

B01 supplies the repeated Bracket and its linked Room placement. B20 interrupts publication and acknowledgement, races writers and finishes an older measurement late. The local B22 case calls the registered Fiote tool with an actual Agent and conversation, attempts an edit in another workspace and one on an obsolete base, then hands the result back to the native editor. Provider inference and hosted delegation have separate acceptance stages.

## Reproduce

Run the following from the repository root in the interface development shell. Give the showcase replay a fresh output directory. Run the last command after compilation in a quiet measurement window.

```sh
cargo xtask workspace showcase target/workspace-stage004-replay
cargo test --locked -p store --lib workspace_scene
cargo test --locked -p lince-interface --features native-presentation --lib native::edit_mode::scene
cargo test --locked -p store --lib workspace_scene::tests::measure_accepted_edits_and_retries -- --ignored --exact --nocapture --test-threads=1
```

The first command checks the Cargo dependency graph for the model, store and engine, then runs the cumulative desktop scenario. It builds and edits the Bracket, restarts, exercises receipt loss and out-of-order completion, continues through the local Fiote tool registry, and restarts again to inspect both Actors’ receipts. The storage fault check also terminates a child writer on each side of HEAD publication. [Showcase replay](../../../../crates/sandbox/examples/workspace_scene/receipts.rs) · [Crash, conflict and latency checks](../../../../crates/store/src/workspace_scene/tests.rs)

## Laboratory regression checks

The normal Linux desktop binary opened a new profile directly into **showoff**. Its saved workspace contained the workbench instructions, the Bracket linked to its Room and Record, and the sculpture asset. A fresh desktop later showed the original Fiote receipt alongside the currently selected Bracket’s distance. [Recovered inspector](recovered-receipt.png)

The cumulative replay passed on 2026-11-22, including reloading the shipped sculpture and the saved Bracket in two fresh desktop processes. A separate 360 × 700 replay found the focused receipt control inside the edit panel. The original two-process Stage 002 build and reopen also passed. All 82 previously recorded native workspace cases and both new scene cases passed, alongside nine functional scene storage checks and the ten baseline fixture cases.

The wider Laboratory fast suite still reports three failures. Its Protein Area fixture cannot activate **Add a condition**, its password-field fixture tries to add a plugin after Bevy finishes setup, and its stale-pointer case leaves a button pressed. All three reproduced individually on a native test binary built before Stage 004. The full-suite run during compilation also reached its 30-second deadline. The focused workspace checks above include the desktop edit-panel geometry test. [Laboratory interface cases](../../../../crates/interface/src/native/laboratory/interface.rs) · [Pointer cancellation case](../../../../crates/interface/src/native/laboratory/driver.rs)

## Measured on 2026-11-22

The local two-placement fixture ran 40 warm commit/retry pairs on NVMe after compilation finished. The largest single command staged 7,558 bytes across its immutable pages and HEAD. Every retry returned the original receipt and staged zero bytes. The 200 ms small-commit limit held for every observation. [Raw samples, resource counters, background load and source hashes](verification.json)

| Measurement | p50 | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Durable edit with receipt | 29.301 ms | 34.085 ms | 113.751 ms |
| Duplicate retry | 1.077 ms | 3.389 ms | 3.939 ms |
| Showoff control activation through its action frame | 89.602 ms | 116.062 ms | 116.062 ms |
| Local Fiote tool round trip, including canvas queue | 136.476 ms | 180.971 ms | 180.971 ms |

The desktop timing used the unoptimized Linux build, a 1680 × 960 window on a private Weston display, and the normal user desktop running alongside it. The nine local tool calls used at most 455 bytes each in the shared 32-call canvas queue. They ran sequentially. The regular Stage 002 save/open regression ran another 40 pairs, with a 23.774 ms save p95 and 1.132 ms verified-open p95. [Measured boundaries and observed results](verification.json)
