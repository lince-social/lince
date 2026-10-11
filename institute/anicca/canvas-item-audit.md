# Canvas item deletion and interaction audit

Investigated on 7 October 2026 against the saved Home workspace and the current desktop code. This is a diagnosis and proposed implementation plan; production behavior was not changed during this audit.

## Result

The saved items are deletable through the shared deletion actions. The confirmed failure is reaching an AoI through canvas pointer selection: the relation area's ray hit is correct, but the Mouse hover target becomes the canvas root, leaving selection empty and exposing no Delete control. Selecting the same AoI through the Areas panel allows deletion.

Rendering-enabled Fiote tests give a different result: the visible generated card is selected, and its Delete control removes both the card and its grouped source area. The initial renderer-detached Fiote test must not be used to claim that this castle is generally undeletable. Its nested controls and duplicate-view behavior remain separate issues to investigate.

There is no evidence that these particular saved areas are corrupt. The shared family needs a common way to resolve a canvas owner from a hit on a rendered surface, in addition to the common deletion action.

The reported Relation Castle crash has not yet been reproduced as a crash caused by selecting its area. Keep that issue open; successful deletion tests do not establish that all interaction paths are safe.

## Copy and evidence

The source snapshot was `/home/user/.config/lince/interface.snapshots/00001791418626117594.json`, with SHA-256 `027b0861e6851d5160377c3e04c61c39a06a66c6776fb0fc382a6ab0d67836bc`.

A private test copy was made at `/tmp/lince-canvas-audit-20261007`. It contains the original snapshot, a SQLite backup made with SQLite's backup API, and copies of workspace settings and Fiote data. Each independent deletion test received a fresh snapshot copy. All deletions were performed against those copies.

The snapshot contains two workspaces, with Home active, and 22 saved canvas items:

| Family | Saved items |
| --- | ---: |
| Areas of Influence | 16 |
| Ordinary Sands: Organ, Time Castle, Sync | 3 |
| Kanban board | 1 |
| Karma Castle | 1 |
| Frequency Castle | 1 |

Loading the copied database in the full application produces 71 canvas items, including 49 generated items. Generated cards and links account for the difference; comparing that count with the saved item count alone would incorrectly suggest duplication or corruption.

The replay example is `crates/desktop/examples/workspace_canvas_audit.rs`. It restores a copied workspace, runs the actions attached to the real Areas panel ActionButtons and deletion confirmation, and can inject programmed pointer input into an offscreen application. It uses no Wayland click or keyboard automation. By default, the offscreen replay initializes rendering and then detaches the render sub-application while retaining the application systems, UI layout, cameras, picking, and actions. It does not validate the final rendered appearance; `--keep-renderer` retains rendering for the full replay.

## Deletion results

Each of the 22 saved items was tested separately on a fresh copy. All 22 deletion requests succeeded, and each requested entity remained absent after the next application update.

For AoIs, the test used the panel's Select, Delete Area, and modal Delete actions. For ordinary Sands and the other castles, the isolated reference test invoked the shared DeleteItem action and the real confirmation ActionButton. Those reference tests establish that deletion eligibility and execution work; they do not establish that a human can reach the corresponding canvas button.

| Item or family | Independent test | Observed scope |
| --- | --- | --- |
| Relation Castle with relation configuration | Removed | One saved AoI |
| Second area named Relation Castle | Removed | One saved AoI |
| Kanban board | Removed | Board and eight grouped AoIs |
| Backlog, Todo, Next, WIP, Review, Done, Documented | Each removed on its own fresh copy | Board and eight grouped AoIs |
| Kanban task-spawning AoI | Removed | Board and eight grouped AoIs |
| Organ Sand | Removed | One saved Sand |
| Time Castle | Removed | One saved Sand |
| Sync Sand | Removed | One saved Sand |
| Both Fiote Castle AoIs | Each removed | One saved AoI in each isolated test |
| All three Command Castle AoIs | Each removed | One saved AoI in each isolated test |
| Record Castle AoI | Removed | One saved AoI |
| Karma and Frequency castles | Each removed | One saved castle |

A full application replay also deleted the relation castles first, then the Kanban, then the Organ, and then the remaining saved items. It ended with zero canvas items. Deleting an owner removed its generated cards on subsequent reconciliation. The full sequential deletion replay used the copied database without a Fiote host; the separate pointer replays enabled the copied Fiote host.

Deleting the active Relation Castle reduced the count from 71 to 42. Deleting the second relation area reduced it to 41. Deleting the Kanban reduced it to 16. Deleting the Organ reduced it to 15. The remaining owner and Sand deletions reduced it to zero.

Raw evidence is in `individual-results.json`, `individual/*/replay.log`, and `full-corrected.log` under the test-copy directory. An initial offscreen run selected the canvas menu's Delete button instead of the modal's identically labelled button. That harness mistake was corrected by requiring the confirmation ActionButton to target the canvas root. The initial `full.log` is not valid evidence of a deletion failure.

## Why canvas selection still fails

Renderer-detached programmed center clicks on these saved areas produced an empty SandSelection, no selected AreaEditor entity, a Mouse hover hit on the canvas root, no target-specific Delete menu button, and no deletion confirmation. The Fiote result changes when its visible generated card participates in rendering and mesh picking:

| Area | Saved area identity |
| --- | --- |
| Active Relation Castle | `44319614519a90e4410c0bcf733e0841` |
| First Fiote Castle | `4b7dc49e7ec1663f2d6e93dba4d7c77f` |

The relevant implementation paths are:

1. `topology/presentation.rs::synchronize` installs SpatialRoot on workspace roots even when `topology/view.rs::View.spatial` is false. Flat view still uses the surface presentation system.
2. `area_input.rs::hit_areas` skips its flat shape-hit fallback whenever the root has SpatialRoot. Removing that marker in a unit fixture does not reproduce the normal application's flat-view setup.
3. `topology/input.rs::pointer` can route an AoI surface hit into CONTENT_POINTER even in edit mode. When it creates that content location, the Mouse PointerHits target becomes the canvas root rather than the surface's canvas owner.
4. `canvas_selection.rs::hit` resolves its target from the Mouse HoverMap. A root-only hit contains no CanvasItem owner, so selection is empty. Area input and inspection also depend on the Mouse path for selecting the area.
5. `canvas_item.rs::menu` needs a selected or hovered eligible item. The shared DeleteItem action exists, but the menu cannot expose it for an owner that was never selected.

This explains why adding DeleteItem and changing an AoI's Pickable value was insufficient. A Pickable change cannot recover an owner that pointer routing has replaced with the root. The final routing instrumentation confirms that the relation ray hit is the requested area, its Pickable is hoverable, View.spatial is false, and SpatialRoot is present. Nevertheless, the Mouse hover target is the canvas root and the selection is empty. This reproduced both in the complete 71-item workspace and in a focused 29-item copy retaining the same relation owner and its generated contents. `picking-all-v2-routing.log` and `focused-relation-routing.log` contain those results. The focused test also reproduced the same failure with rendering retained throughout the replay; `rendered-relation-pointer.log` records the correct area ray hit, root-only Mouse hover, empty selection, and unchanged count of 29 items. The relation result therefore does not depend on detaching the renderer.

The existing `canvas_item::tests::spatial_area_hits_reach_selection_after_hover_generation` injects an AoI entity directly into PointerHits. It verifies hover filtering and selection for that input, but does not exercise the surface backend's owner-to-root conversion. The existing right-drag tests set PointerState.hit explicitly and verify movement; they do not verify that the same input selects an owner or exposes Delete.

Layout code also assigns normal Pickable values to layout-backed AoIs, while CanvasItem's picking synchronization derives the value from SpatialRoot. These responsibilities should be consolidated. In the normal application SpatialRoot is already present in flat view, so this is not evidence that a per-frame IGNORE overwrite caused the reproduced workspace failure.

## State checks and differences that matter

The copied database passed `PRAGMA integrity_check` and `PRAGMA foreign_key_check`. All 16 restored InfluenceArea values passed their Rust validation. Saved placement identities were unique. The saved layout graph contained 20 layout nodes, no missing parents, and no cycles. All Kanban area references resolved.

Two areas share the display name Relation Castle but have different identities and different behavior. The first has the relation protein configuration. The second, `8daac88b8997a2182ce046be0785f701`, has no protein configuration and has a layout box. It is a valid area bearing that name, rather than a functioning relation feed. A user may have intentionally removed its spawn behavior; the name alone cannot establish corruption. The UI should show its actual capabilities and an explicit way to restore relation behavior.

Both Fiote Castle areas query the same existing, non-deleted Person record, while belonging to different groups. That is legitimate duplication of a view. It requires control state, pending requests, focus, and responses to remain scoped to each view. A failure to interact with one view does not establish that its record is missing.

The Record Castle's saved query selects the slug `pending`, which currently matches no record in the copied database. That is an empty source query, not a failed foreign-key check. It should show an empty-source explanation and keep the owner's editing and deletion controls available.

The Kanban's board, source AoI, and seven column AoIs share a group. `canvas_selection::group_members` and `companions` expand deletion to that group. “Delete Area” therefore removes the complete Kanban, rather than just the named area. This did not prevent deletion, but the current generic wording does not explain the scope well.

## Fiote interaction and relation crash

The renderer-detached Fiote replay records a ray hit on the source area followed by a root-only Mouse hover hit. With rendering kept active, the same center click instead hits its generated card. The canvas selection contains both the card and source area; Inspection selects the card, and AreaEditor has no selected area. The common menu belongs to the card, rather than the source AoI.

The rendering-enabled sequential test used that card's actual Delete ActionButton and the confirmation control. Both canvas entities were removed, reducing the count from two to zero. See `rendered-fiote.log` and `rendered-fiote-sequential.log`. Looking only for a menu whose target is the source area would incorrectly call this a deletion failure.

Mesh picking depends on rendering visibility, so a nested-card test must retain rendering while moving the view. Content controls also use the virtual CONTENT_POINTER path and must be tested separately, including press and release forwarding. These tests do not establish that every nested Fiote control works. The group selection's failure to populate AreaEditor explains why selecting a card and selecting its source area can expose different editing controls; that context should be deliberate and visible.

Fiote session code also deserves a focused stability test: repeated status responses must preserve unchanged controls, draft text, and focus. Another change already present in the working tree adjusts status rendering and limits rebuilding unchanged Fiote panels. This audit did not author that change or establish that it fixes the reported castles.

Neither opening the Areas tab nor selecting either relation area through its panel control reproduced the reported relation crash in the completed replays. A focused rendering-enabled test also selected the configured relation area through the Areas panel and deleted it with its generated contents, reducing the count from 29 to zero without a panic; see `rendered-relation-panel.log`. A separate harness run crashed during startup with a Tokio worker stack overflow when it used the default small worker stack. The harness was changed to match the production runtime's explicit 32 MiB worker stacks in `crates/lince/src/main.rs`. That startup failure is not evidence of the reported AoI-tab crash.

To diagnose the remaining crash, capture the failing path with the relation owner, generated cards, source subscription, and UI fields loaded. Test selecting from the Areas list, selecting the surface, and switching between both identically named areas. Include normal production thread settings and a rendering-enabled run. Record the panic location or failure signal rather than treating any crash as proof of corrupt workspace data.

## Proposed shared-family changes

Keep CanvasItem as the common family for AoIs, saved Sands, castles, and generated canvas cards. Share owner resolution, selection, deletion eligibility, deletion scope, and diagnostics. Keep protein subscriptions, Fiote controls, relation links, and Kanban behavior specialized.

### 1. Resolve both the control and the canvas owner from a hit

Introduce one target resolver that returns the canvas root, canvas owner, optional nested control, and pointer/surface context. Use it from canvas selection, inspection, AoI selection, menus, and gestures. The surface backend already knows the owner in PointerState.hit; preserve that information while forwarding content input.

Do not disable all AoI content picking to make selection work: layout controls, summaries, and nested cards still need their input. In edit mode, select the owner while continuing to route intentional control activation to the content target. Inspector and modal overlays must continue to take priority over the canvas.

### 2. Make deletion scope explicit

Represent the deletion target set once and share it among DeleteItem, keyboard deletion, and Delete Area. Distinguish selecting a whole group from selecting a member for editing. For the Kanban, display the affected board, areas, and generated cards before confirmation. If deleting a column independently is supported, update the board's structure in the same operation; otherwise explain that the column belongs to the board.

Generated cards need an explicit lifecycle policy. Removing an owner must stop its subscriptions and remove its generated contents. Removing just a generated card must either hide that view persistently or explain that the feed can recreate it. Keep deleting a local view separate from deleting a database Record.

### 3. Return actionable eligibility reasons

Replace opaque boolean/no-op results with a small result type covering a missing entity, wrong workspace, excluded overlay, practice restriction, unresolved owner, edit mode requirement, and changed confirmation context. Surface the relevant reason when a human explicitly asks to delete an item.

Avoid weakening workspace or practice checks to compensate for a target-resolution failure. Resolve the intended owner first, then apply the existing checks.

### 4. Add a workspace health check and deliberate repair actions

Validate persisted geometry, area configuration, unique identities, layout parent references and cycles, group workspace ownership, Kanban references, and generated-owner references. Report an empty query separately from a missing record or invalid configuration. Report missing castle capabilities separately from a valid area retaining an old display name.

Preserve the original snapshot before repair. Offer targeted repairs such as detaching an orphan layout, removing a dangling generated view, or restoring an explicitly chosen castle configuration. Do not silently delete or reset valid areas because their display name and current behavior differ.

### 5. Test the actual presentation path

Add regression cases with SpatialRoot present in both flat and spatial views, a real AoI surface, and the application's Mouse-to-CONTENT_POINTER forwarding. Assert the hit owner, selected owner, visible Delete control, modal target list, cancellation, confirmed deletion, and absence after reconciliation and reload.

Include a plain area, a layout-backed area, a relation area with generated cards, both views of the same Fiote record, and a grouped Kanban column. For Fiote controls, verify that identical status updates retain field entities and focus. For the relation crash, keep a loaded-workspace replay rather than relying only on a newly spawned empty castle.

## Suggested order

1. Fix shared pointer-to-owner resolution and reproduce owner selection through the surface backend.
2. Make group/member deletion scope explicit and verify generated-content cleanup and persistence.
3. Verify nested Fiote controls and focus stability with duplicate views of the same record.
4. Reproduce and capture the relation crash through its exact failing path.
5. Add the workspace health report and targeted repair controls for independently verified invalid state.

This order addresses the reproduced interaction failure before adding more deletion exceptions to individual castle implementations.

## Validation and current limitations

The replay example passed `cargo check --offline -p lince-desktop --example workspace_canvas_audit` before concurrent location changes entered the workspace. The completed runtime checks include 22 independent saved-item deletions, the full 71-to-zero sequential replay, rendering-enabled Fiote group deletion, relation panel deletion, and the rendering-enabled relation pointer failure. `git diff --check` passed.

A subsequent rebuild was blocked by five E0061 errors in the concurrently added `crates/nucleus/src/location.rs`: calls to `valid_uid` omit its required prefix argument. Those unrelated files and migrations were left untouched. The check used the cached Nix shell environment because this environment denied access to the Nix daemon. No migration integrity guard was bypassed.

This audit adds the report and reusable replay example. It does not implement the proposed production fixes, establish that every nested Fiote control works, or reproduce the reported relation crash.
