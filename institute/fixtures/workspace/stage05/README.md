# Stage 005 · Use another document’s definitions

Open **Laboratory → showoff workspace → Edit → Workspaces → Scene document**. In a fresh workspace choose **Try the bracket example** and **Save scene**. If the Bracket is already saved, choose **Open directory** to continue where you left it. Use **Save to a new directory** to keep a separate copy. Open **Definition library → Browse the example parts library**. The binary supplies an editable source document with two revisions of one Bracket, two grades of Steel and a shared millimetre unit. **Source scene directory → Browse source definitions** also accepts an ordinary saved scene directory. [Shipped source](../../../../crates/interface/fixtures/showoff-parts.json)

| Try it in the workbench | What to look for |
| --- | --- |
| Review **Bracket · 040bbc352c8a**. Use **Show source and revision details** to inspect provenance, then choose **Import reviewed definitions**, **Place this definition** and **Save scene**. | The review brings in the Bracket, Steel grade A36 and Millimetre. The existing workbench Bracket keeps its original definition and Record. The new occurrence starts at the local origin. |
| Review **Bracket · 8303a75c2bca**. Import, place and save it. Review and import it once more. | The first import brings in the second Bracket and Steel grade 304. It reuses Millimetre and flags the same-name definitions. Repeating the import reports zero additions and three reused definitions. The two Brackets share a source identity and keep their different revisions. |
| Inspect either Bracket under **Definitions in this document**. Inspect its Steel, create and select a local part, and choose **Require for the selected placement**. Undo and redo. | The inspector shows the exact source, revision and required grade. Adding the requirement revises the definition pinned to the selected local placement. Other copies retain their own pins and coordinates. |
| Review **Coated bracket** in the sample source. To try both sides yourself, inspect an imported Steel, choose **Keep this definition local** and save. Browse the destination as a source and review its dependent Bracket. Restore the permission and save again. | The unavailable requirement blocks the whole import. Its private name and properties stay hidden. Restoring permission makes the dependency available for review again. An already imported copy stays usable. |
| Select a new Bracket placement. Open **Accepted edits and measurements**, change X and choose **Publish placement coordinates**. Save and reopen the destination after moving the source directory out of the way. | Both grades remain inspectable from the saved destination. The selected Bracket has its new local position and a receipt. The original Room member, its Record and the other placements keep their previous state. |

The scene tree shows the new placements. The original Bracket is the visible Sand linked to the Room. An import copies declarative part, material and unit definitions. A source placement’s transform, Record binding and run state stay in its own document. The local library follows up to 32 required definitions across eight levels, with at most eight requirements per definition, 64 KiB per file, a 256 KiB read budget per resolution and 64 definition revisions in the destination. Placement coordinates are measured in metres. Steel grades and the shared millimetre unit are descriptive data here. Material simulation has a later stage. [Model identity and dependencies](../../../../crates/nucleus/src/canvas/scene/definitions.rs) · [Import and verification](../../../../crates/store/src/workspace_scene/library.rs)

## Reproduce

Run these commands from the repository root in `nix develop .#interface`. Give the showcase a fresh output directory. The Linux verification used Rust 1.99.1 with the workspace compiler wrapper from the [Linux build configuration](../../../../.github/actions/linux-build/action.yml).

```sh
cargo build --locked -p lince --bin lince
target/debug/lince --showoff
cargo xtask workspace showcase target/showcase-stage005-01
cargo test --locked -p store --test workspace_definition_import
cargo test --locked -p lince-interface --features native-presentation --lib library_rechecks_a_review_before_import
cargo test --locked -p store --test workspace_definition_import -- --exact measure_verified_b39_import_save_and_reopen --ignored --nocapture --test-threads=1
```

After the earlier exhibits are built, `--showcase-import` runs the review, placement, local dependency and sharing workflows in the table. It publishes the move and takes the source library offline. In a fresh desktop, `--showcase-library-reopen` inspects both grades, compares the three original placements, Room binding and receipt, then clicks **Place this definition** and undoes the new occurrence. Run the storage timing test alone after compilation. It measures Rust allocations above the entry value of each phase, encoded definition and undo data, immutable page growth and durable save latency. [Native replay](../../../../crates/sandbox/examples/workspace_scene/library.rs) · [Storage cases and measurement](../../../../crates/store/tests/workspace_definition_import.rs)

## Verified on 2026-11-22

The normal Linux binary opened the workbench. The five-process native replay and a copy of the actual Stage 004 workspace both retained three original placements and their Room binding. With the source offline, the inspector showed both steel grades. The imported Bracket reopened at X = 0.375 m with its original move receipt. At both 1680 × 960 and 360 × 700 a pointer click placed one fresh occurrence and Undo restored the six saved placements. [Second Bracket and shared unit](definition-review.png) · [Reopened narrow inspector](definitions-reopened-narrow.png)

The narrow replay found two interaction defects. Keyboard navigation and accessibility each requested the same 1,041-pixel scroll before layout. After correcting that offset, the selected item’s canvas toolbar still covered the button. The replay now checks a pointer click on the rendered control. The small viewport also passed the earlier placement and receipt workflows. A separate canvas Area test reproduced a missing Bevy pointer resource on the pre-Stage-004 binary. Its fixture now initializes both picking resources and tests selection in 2D and 3D.

100 focused native tests, 8 additional canvas item cases, nine scene-storage checks, four definition-import integration cases and all ten baseline fixture cases passed. Nine document-renderer tests passed after an interim EPUB type correction. Concurrent document changes replaced that line before the final Linux build. Three previously documented Laboratory tests still fail on this checkout. They cover a stale pointer press, the Protein Area filter and the workspace password field. [Earlier findings](../stage04/README.md)

The local NVMe fixture began with two placements of one definition. The first durable save containing both imports took 44.853 ms and added 2,871 immutable page bytes. Each dependency review read 2,831 bytes, including the catalogue and verification reads. Import retained the previous 59-byte encoded definition list for undo and shared the same 555-byte encoded placement list. The destination’s six definitions occupied 1,432 encoded bytes.

All 40 warm durable saves met the 200 ms limit. The unoptimized Linux benchmark ran after compilation, with one warm-up and a verified reopen for every saved edit. The preliminary correctness replay ran alongside compilation and one accepted edit took 2895.745 ms there. A separate timing attempt also overlapped an external Rust build. Its longest accepted edit took 2664.897 ms and failed the limit. The qualified measurements began after a 15-second quiet window and detected no concurrent builds. Both attempts are retained. The final native replay’s longest accepted edit took 32.886 ms. The slowest observed **Place this definition** control took 523.766 ms in the screenshot-instrumented replay. This measures the time from the native control activation to the action frame, including rendering work. Background load, per-phase heap peaks, raw samples and source hashes are retained. [Machine-readable results](verification.json)

| Warm operation | p50 | p95 | Maximum | Peak additional Rust heap |
| --- | ---: | ---: | ---: | ---: |
| Browse the published catalogue | 0.663 ms | 1.056 ms | 1.195 ms | 16,173 B |
| Review required definitions | 0.815 ms | 1.304 ms | 1.411 ms | 7,284 B |
| Verify and reuse the three definitions | 0.773 ms | 1.303 ms | 1.388 ms | 7,284 B |
| Durably save an edited placement | 22.944 ms | 30.469 ms | 32.186 ms | 16,854 B |
| Open and verify the saved scene | 2.026 ms | 2.760 ms | 3.911 ms | 13,044 B |
