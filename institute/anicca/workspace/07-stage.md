# 07 — Native room authoring, multi-copy editing and mounted applications

Finish able to build and furnish a low-poly room or simple castle using native tools, rather than relying on the supplied building. The authored place remains useful from map overview through walking inside it, with readable wall content and ordinary batch shape editing.

## Required workflow milestones

- [ ] **07.M01.** Create walls/floor and a doorway with numeric/snapped vertex/edge/face tools, extrusion and openings. Preview, cancel, undo and save/reload the room; walk inside, mount useful Sands and move the whole space without moving real physical calibration.

- [ ] **07.M02.** Create a simple castle/custom structure at a chosen saved location, see it from the street and on the overview, enter it and use mounted content. Keep the underlying source/map and authored replacement distinct; richer comparison/reimport is stage 09.

- [ ] **07.M03.** Select several matching copies of a furnishing/mesh and edit once through the normal tool/scope control. Only the captured selection changes by default, unselected copies stay intact, compatible geometry is reused and the original source recipe remains inspectable.

- [ ] **07.M04.** Drag geometry/formula handles faster than a bounded worker completes. Show useful compatible preview progress, commit the exact final requested value once and retain the last usable source on invalid/cancelled/stale output.

- [ ] **07.M05.** Use the same native Sand in Sandbox and Park. In Park, additionally mount a live planar application panel with correct focus/hit mapping/popups and recover it when its face disappears; remote pixels/input require separate allowed grants.

## Entry and scope

- [ ] Confirm the preceding [stage 06](06-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 04 summary](04-stage.md#completion-summary); [stage 05 summary](05-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/interface/src/native/topology/`; `crates/interface/src/native/canvas_host/`; `crates/park/src/presentation/`; `crates/park/src/nested/`; `crates/nucleus/src/canvas/`.

## Ordered implementation tasks


- [ ] **07.T01.** Extend [stage 05](05-stage.md)'s bounded source/placement tools to the broader T1–T2 geometry operations: analytic planar patches and polygon mesh source, vertex/edge/face selection, numeric/snapped move, extrusion, opening and edit preview/cancel/commit.

- [ ] **07.T02.** Add purposeful flat/sharp/smooth appearance, readable content placement and native Room/building-element presets. Keep measurement source and render triangulation distinct.

- [ ] **07.T03.** Mount Sands, Record views and supported assigned content/windows through existing lifecycle/input/output paths; preserve explicit gravity/body policy for unmounted content. Verify save/reload and moving the containing space.

- [ ] **07.T04.** Apply the shared I4 preview protocol to topology/room edits: current requested handles, compatible intermediate geometry, bounded/coalesced work, exact final target, one accepted edit/undo and invalidated-source recovery. Complete the native B31 room/formula subset before CAD extends it.

- [ ] **07.T05.** Implement the minimal B30 surface-display subset for planar polygons/openings: one accepted boundary/source mapping, consistent shared edge nodes and separate crease normals, bounded triangulation and explicit fallback/error. Reuse existing CPU meshing where sufficient; optional structured/trimmed CAD planning and GPU evaluation remain gated extensions.

- [ ] **07.T06.** Complete the ordered Park spatial application adapter tasks above through the shared workspace surface/input/mount implementation. Apply H9 to permissioned pixel publication and any supported remote input separately from shared placement, with actual participant authority, focused input ownership, revoke/lock handling and admitted capture/egress. Extend B34/B36 for local/remote room editing, optional shared previews and retained exact final commits; ordinary hosted native Sands already work from [stage 05](05-stage.md).

- [ ] **07.T07.** Complete B40 through the ordinary native editing family: select multiple matching supplied/baked meshes, edit shape/visual appearance once, retain a concise selected/shared scope, prepare compatible editable variants when required, and commit/undo as one supported batch. Preserve original source branches, occurrence transforms/overrides/state and unselected copies; cancel/stale/denied cases leave accepted state honest. Report source geometry reuse and work versus unique groups and occurrences, without a mandatory conversion wizard or domain-specific editor flow.

## Worked contract example

- [ ] **07.X01.** Implement the complete preview protocol with captured source/selection/authority/input validity. This example shows ordering and exact-final distinctions only; compatible intermediate geometry also needs semantic eligibility and cannot mutate accepted state. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  #[derive(Clone, Copy)]
  struct Preview {
      base: u64,
      scope: u64,
      sequence: u64,
      value: i64,
  }

  fn may_display(candidate: Preview, current: Preview, shown_sequence: u64) -> bool {
      candidate.base == current.base
          && candidate.scope == current.scope
          && candidate.sequence <= current.sequence
          && candidate.sequence > shown_sequence
  }

  fn may_commit(candidate: Preview, current: Preview) -> bool {
      candidate.base == current.base
          && candidate.scope == current.scope
          && candidate.sequence == current.sequence
          && candidate.value == current.value
  }

  fn main() {
      let requested = Preview {
          base: 7,
          scope: 2,
          sequence: 10,
          value: 40,
      };
      let useful = Preview {
          sequence: 9,
          value: 39,
          ..requested
      };
      assert!(may_display(useful, requested, 8));
      assert!(!may_commit(useful, requested));
      assert!(may_commit(requested, requested));
  }
  ```

## Architecture and task requirements

### Proposed defaults for refinement

- [ ] **R0053.** Native modeling — **Proposed default:** Shared planar/ellipsoid fallback, supplied indexed meshes and surface-source adapters first; no default inferred relief/buildings; exact CAD and implicit volumes follow their workflow gates — **When it may change:** Supplied data or an explicit authored operation adds shape through the same contracts

- [ ] **R0066.** Exceptional incompatible edit — **Proposed default:** Keep the last usable accepted version/result visible while preparing the candidate; pause affected evolution at a safe boundary when continuing is unsupported, then offer refresh/restart — **When it may change:** Supported ordinary edits remain automatic; no hidden restart, false fresh result, obligatory new scenario or mesh dialog

### Source baseline and shared families

- [ ] **R0072.** Geometry and physics: native `topology/assets.rs`, `splats.rs`, `physics.rs` — **What to reuse:** Imports, appearance, picking and existing Avian/Parry collision paths — **Required extension or boundary:** Current Bevy is pinned to 0.19.1 and Avian to 0.7.0 with f64. Rendering, collision, source geometry and canonical poses need separate owners. Verify compatibility rather than changing these pins speculatively.

### Capability coverage and reusable workflow recipes

- [ ] **R0112.** **W04 — Native geometric/CAD authoring** — **Shared primitives and thin specialization:** Supports, stable selections, feature/constraint operators, correspondence, representations and drawings — **Minimum proof / later expansion:** B03/B04/B10: room/cave/part; all retained exactness, fabrication and CAD research decisions still apply

### Interface contracts I1–I6

- [ ] **R0303.** **I2 — Presentation and surface composition.** A content presentation declares local bounds, layout/spatial constraints, sidedness, readable scale, focus/input mapping, lifecycle and render/demand capabilities. The existing Sand/Castle, window and output systems provide adapters. Planar content on a modeled patch, a desktop panel and an assigned projector output use explicit coordinate maps; no core primitive assumes all content is a room wall or physical object. Optional colliders/body parameters are separate authored interaction data. Optional avatars and cursors are bounded presentations with their own visibility/lifetime. Shared application placement, pixel publication and remote input follow separate H9 grants.

### Ordinary editing of selected copies and derived shapes

- [ ] **R0309.** Owner-confirmed A05 refinement: editing generated/optimized geometry must stay close to the ordinary workflow. Select one or several objects, use the normal shape/material/parameter tools and apply one undoable edit to the captured selection. Expose a concise scope control for Selected objects and an intentional Edit shared definition/All linked copies action with its consumer scope. Preserve Q2's local-to-selected default; changing one selected set must not unexpectedly edit every instance elsewhere. Finding/selecting matching copies reuses authorized queries and stable definition/occurrence identity rather than loading the entire world or comparing appearance.

- [ ] **R0310.** When a chosen freeform operation requires an editable mesh source, the ordinary Edit shape action can prepare the corresponding variant with a small source/status indication. Do not require a separate conversion wizard, file-format choice or provenance graph before editing. Retain the original CAD/procedural recipe and its branch as inspectable source lineage; arbitrary sculpting cannot be represented as still satisfying its original exact constraints. Source-preserving parameter edits keep using the recipe directly. Preparation is a draft; cancel leaves accepted objects unchanged, and commit records the source/variant bindings through I4/E2 with one undo outcome.

- [ ] **R0311.** Use one derived editable variant per compatible selected source/input group and bind the selected occurrences to it, reusing the resulting geometry. Preserve each occurrence's placement, material overrides and independent evolving state. Group movement changes occurrence transforms; shape edits change the chosen source variant. Different source revisions/parameters are not merged merely because objects look the same. Editing an explicitly shared definition remains available through the same tool/scope control. This applies to ordinary mesh models, baked low-poly output and supported CAD-derived output; no distinct per-domain editing frontend is introduced.

- [ ] **R0312.** Capture selection, source/base revisions, source-read and target-write permissions before preview/acceptance; stale or denied targets cannot be silently widened or reported as complete. Within one authority, accept the supported batch coherently; across authorities retain explicit pending/partial outcomes instead of claiming a global transaction. Undo restores the captured bindings/edits without erasing others' accepted work. B40 verifies the native flow, source preservation, copy reuse, conflict/cancel/undo and measured work versus unique inputs and selected occurrences.

### Park spatial application integration

- [ ] **R0313.** Workspace models, cameras, authoring, Areas and native Sands must work in Sandbox and Park through the shared interface. Park adds live external Wayland application surfaces and control of the Linux session. Keep model semantics independent of Wayland. Reuse Smithay, Bevy/wgpu and existing compositor adapters; no dependency replacement is currently justified.

- [ ] **R0314.** Shared modeling, precision, navigation, streaming and native Sand mounting belong to the shared workspace contracts. These tasks extend its presentation and attachment contracts to live Park applications; do not duplicate that shared machinery. Refine the adapter contract before implementation. Start with readable planar content attached to 3D geometry, retaining ordinary flat presentation.

- [ ] **R0315.** These adapter tasks extend [stage 07](07-stage.md) after [stage 05](05-stage.md) proves the shared surface/input path. Implement them in the listed order within that stage; they are not a second delivery schedule:

- [ ] **R0316.** **Spatial application presentation.** Adapt Park's existing imported application textures/surface trees to the shared spatial content presenter. Keep application pixels/logical dimensions separate from model units, camera scale and output resolution. Reuse import synchronization and lifecycle. Current `presentation/clients.rs` and `clients/tree.rs` present clients through Bevy UI nodes/materials; they do not implement this spatial adapter. **Proof:** the same native Sand works in Sandbox and Park; Park additionally hosts a live application on a moved/rotated panel, with correct clipping, appearance and bounded resources.

- [ ] **R0317.** **Spatial input tied to presented frames.** Extend the current rectangular `host::ContentMapping` and `nested/input.rs` routing with the scene's verified hit/local-coordinate mapping, visibility/occlusion and camera/view revision. Retain paired per-output presentation snapshots. Adapt existing popup/dialog, IME, drop, relative-pointer and constraint routing to that mapping; do not reimplement their Wayland protocols. **Proof:** input reaches the visible application at the correct coordinates across camera/panel movement and independently paced outputs; obscured/stale hits fail safely and auxiliary surfaces remain usable.

- [ ] **R0318.** **Mount and view transitions for live applications.** Connect shared attachments to Park's existing application identity, focus and desktop actions. Navigating away, unloading geometry or deleting a mounting face must preserve the process/window and expose an accessible recovery placement. Define how existing focus/fullscreen/minimize/layout commands behave in that presentation. **Proof:** mount removal, scene changes and flat/spatial switching preserve application state; existing lock/session controls remain independent of scene geometry; output capture matches the presented scene and selected-window capture retains its defined application scope.

- [ ] **R0319.** The missing work is the spatial adapter and its tests. Capture export, locking, application lifecycle and desktop commands already have implementations and need integration/validation rather than replacement. This plan does not authorize curved/deforming application content or expand physical projection support. Remove completed tasks only after their relevant evidence passes.

- [ ] **R0320.** Hosted presentation uses H2/H9: shared placement, published application pixels and remote input have separate grants and admitted streams. Existing local capture/input support is not proof of a permissioned remote application session. Do not stream a visitor's or host's desktop merely because its window is mounted in the shared model.

### Authoring representations and exactness

- [ ] **R0321.** The first editable spatial source uses analytic planar patches plus polygon/half-edge boundary meshes with stable local element IDs. Preserve polygons for editing; triangulation is a render derivation. Support open surfaces intentionally. Watertight, oriented, non-self-intersecting shells are required before claiming a solid volume. A displayed mesh can still be useful when those tests fail, with unsupported solid operations explicit.

- [ ] **R0322.** Provide create plane/wall/floor, move vertex/edge/face, split/connect, extrude, inset/opening and numeric/snap controls in bounded increments. Room walls, slabs, ceilings and desks favor analytic planes/extrusions. A cave fixture uses an explicit connected mesh with floor/ceiling/overhang queries first; volumetric sculpting follows when the shape needs topology-changing unions/cuts.

- [ ] **R0323.** Exact mechanical CAD has a different source representation and adapter, described below. An exact planar primitive can be shared; a low-poly boundary mesh does not become an exact NURBS solid by renaming it. “Exact CAD” still has kernel tolerances and operation-specific numerical failure.

- [ ] **R0324.** Sparse volumes use bounded local blocks and versioned scalar fields; they do not allocate a voxel array over the planet. Extract mesh/collider products only in changed regions with neighbor halos and shared boundary conventions. [OpenVDB's sparse-field model](https://www.openvdb.org/documentation/doxygen/overview.html) is a reference/adapter candidate, not a mandatory C++ dependency for the first room.

### Domain workflows as implementations of the primitives

- [ ] **R0364.** Room/cave — **Authored/source support:** Planar/polygon boundary or sparse implicit support — **Specialized operation/constraint:** Extrusion/opening, sculpt/displacement, mount and simplification — **Shared outputs and reuse:** Same surface queries/selections, material regions, content and local dynamics

### Verification obligations

- [ ] **R0816.** B39/B40 add import-versus-state and ordinary batch-edit intention checks through the actual native/backend path. Match known identities/revisions and supported typed content, refuse same-name false matches, preserve source rights/provenance and treat live environment bindings separately from definitions/state. Verify selected copies share compatible geometry while retaining independent placements/overrides/state; cancelled preparation cannot change their accepted bindings. Show source/placement mismatch after re-import and preserve fixed-versus-surface-follow intent. Measure actual source/instance/variant bytes and permission/revision checks for the captured set; an optimization cannot become a hidden global shared edit.

- [ ] **R0824.** For B30/B31, measure each phase separately: input/preview scheduling, useful displayed update age, exact final acceptance, topology planning, numeric evaluation, bytes copied/uploaded, render preparation and frames. Warm/cold/disabled caches and real CPU/GPU paths are separately identified; a software fallback or incomplete approximation cannot masquerade as hardware/exact output. Repeated-instance fixtures record distinct source assets versus occurrences, edits/insertion/removal, reflected/nonuniform transforms, view-origin movement, overlapping leases and single-lane/nested completion. A completed worker continuously discarded during a drag fails intention even if its isolated kernel timing is fast. A reusable artifact that exceeds admission or deadlocks one lane fails performance/recovery even if its hash and geometry are correct.

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0829.** **Q2 — Local variant by default.** Micro/material changes target the selected occurrence/region through a local variant. A broader shared update is intentional and identifies affected consumers; do not silently mutate every instance or add routine permission prompts.

- [ ] **R0845.** **Q18 — Simple editing across selected copies.** Retain the original recipe when freeform editing needs a derived editable variant, while keeping the normal select/edit/apply/undo flow. Multiple matching selected objects can share one compatible edited variant without losing independent placement, overrides or state. Default scope is the captured selected set; intentional shared-definition/all-linked edits use the same concise scope control. Variant preparation is not a mandatory conversion wizard or a reason to fork the editing frontend.

- [ ] **R0846.** **Q19 — Safe default for an incompatible live edit.** Ordinary supported edits update automatically. For an exceptional unsupported continuation, preserve the last usable accepted version/results, prepare the candidate separately, mark it pending and pause only affected evolution at a safe boundary when continuing is unsafe/unsupported. Offer an explicit refresh/restart with captured inputs/state and its real consequences; never silently reset history or label old results current. No compulsory scenario or mesh decision is added to ordinary editing. Core support recovery uses this now as needed; scientific transitions arrive with deferred profiles.

### Accepted architecture/detail choices and stage-specific evidence

- [ ] **R0857.** **A02 — Agreed: safe exceptional-update recovery.** Ordinary supported floor/Sand/shape edits continue through automatic bounded regeneration and safe support adoption. When an affected calculation cannot continue, preserve its last usable accepted version/results while preparing the candidate; pause affected evolution safely if continuing is unsupported, show pending/stale status and offer refresh/restart with real input/state consequences. Do not silently erase history, invent a fresh answer or create an obligatory new scenario/mesh dialog. See Q19; core geometry/support recovery at stages [07](07-stage.md)–[10](10-stage.md) as needed, new numerical transitions only in deferred stages [17](17-stage.md)–[19](19-stage.md).

- [ ] **R0860.** **A05 — Agreed with simpler ordinary multi-object editing.** Retain original CAD/procedural source when freeform editing requires a mesh variant, but expose it through normal selection and Edit shape tools with a small source/status indication. Select several matching copies, edit once and apply/undo for the captured selected set; use the same scope control for an intentional shared-definition/all-linked edit. Reuse compatible variants while retaining per-occurrence placement/overrides/state. No compulsory conversion wizard or a separate workflow for derived meshes. See Q18/B40; stages [stage 04](04-stage.md)/[stage 07](07-stage.md)/[stage 08](08-stage.md)/[stage 10](10-stage.md).

### Detailed topology requirements

- [ ] **R0863.** These remaining requirements expand stages [stage 04](04-stage.md), [stage 07](07-stage.md) and [stage 08](08-stage.md). The existing height-based floor, mathematical effects, generic bindings, gravity and native Floor editor are the starting point.

### Topology experience

- [ ] **R0864.** Customize the canvas as a useful, attractive low-poly 3D space: a model of a real room, a cave, or an invented environment. Floors, walls, ceilings and overhangs can hold Sands and other canvas content. A room should remain recognizable and pleasant with few polygons. Low-poly appearance is a deliberate visual style, not simply a coarse sampling grid.

- [ ] **R0865.** Shape the space while editing, then explicitly simplify and bake a chosen version for efficient long-term use. Preserve the editable source so optimization can be undone or repeated with different settings. Moving through a finished space should not require rebuilding its geometry every frame.

- [ ] **R0866.** This work extends the existing Room and physical-workstation foundations. Read the Ailuros Record's first practical milestone in [Lince.lingua](../Lince.lingua) alongside this plan. Reuse the existing Room hierarchy, local poses, floor/wall/desk/computer roles, proxy surfaces, mounted content and window/output management where applicable. A virtual room model and a calibrated physical installation are related representations, with an explicit association between their surfaces.

### Topology constraints

- [ ] **R0867.** Keep independently editable mathematical effects and their existing anchors. An optimized static surface is another representation of a chosen shape, not a replacement for all live effects.

- [ ] **R0868.** Separate editable geometry, rendered geometry and collision geometry. They share a revision and surface identities but may use different levels of detail.

- [ ] **R0869.** Preserve silhouettes, intentional sharp edges, doorways, cave passages and mounting surfaces during simplification. Triangle count alone does not establish quality or efficiency.

- [ ] **R0870.** Prefer simple planar geometry for room walls and desks, and locally detailed meshes for caves. Avoid remeshing an entire room just to move a Sand or edit its appearance.

- [ ] **R0871.** Shader effects determine appearance; geometry and collision determine support and physical boundaries. A convincing shader does not establish a navigable cave or a calibrated real wall.

- [ ] **R0872.** Physical meters, Room-local coordinates, canvas coordinates and projector/camera coordinates retain their existing distinct meanings. Moving a virtual Room must not silently change a physical installation's calibration.

### T1. Surface queries, identity and bounded authoring

- [ ] **R0873.** Replace height-only assumptions with shared surface queries: ray intersections, closest surface, normals and physical support. Allow several surfaces at the same canvas position, including cave floors, ceilings and overhangs.

- [ ] **R0874.** Define stable surface identities within the existing Room model. Keep mounts attached to their intended surface when a Room moves; track source revisions when geometry changes.

- [ ] **R0875.** Implement analytic planar patches and polygon boundary meshes through the shared geometry contracts. First tools create a planar wall/floor, move vertices/edges/faces, split/connect, extrude and carve an opening, with numeric controls and snapping. Volumetric sculpting has a separate bounded workflow and cost proof; it is not required to move a Sand or create a wall.

- [ ] **R0876.** Specify interaction with live mathematical effects: which surfaces an effect deforms, how its bounds work in 3D, and what happens when an effect changes a previously baked surface. Recommended behavior: retain the bake until explicit rebuild, and clearly indicate that its source has changed.

- [ ] **R0877.** Acceptance: a room with walls and an opening, and a cave with a ceiling, can be queried and collided with correctly. Existing hills and valleys continue to work. The first editing tools and effect-to-surface rules are agreed before coding them.

### T2. Model and furnish a low-poly room and cave

- [ ] **R0878.** Add native edit-mode controls for the agreed tools, with selection, numeric dimensions, snapping, preview, cancel, undo/redo and persistence.

- [ ] **R0879.** Provide flat shading and deliberate sharp/smooth edge control, with restrained materials and lighting. Review actual room and cave fixtures for silhouettes, readable content and attractive facets.

- [ ] **R0880.** Mount Sands, Records and supported window/content surfaces on walls, ceilings and other modeled faces using the existing Room-local placement path. Mounts are explicit attachments; ordinary unmounted Sands retain gravity.

- [ ] **R0881.** Define a stable face-relative mounting frame and an explicit surface offset. Topology edits that remove a mounting surface must report an unresolved attachment and preserve the content's last pose rather than silently reassigning it.

- [ ] **R0882.** Reuse physical-workstation surface assignments and dedicated output windows. Associate measured planar walls/desks with modeled surfaces without changing their calibration merely because the virtual model moves. Curved modeled surfaces do not imply curved physical projection support.

- [ ] **R0883.** Acceptance: create a recognizable room and a cave, place useful content on their walls, save/reload, and move the Room without breaking local mounts. Verify the physical projection mapping separately through the existing calibrated surface path.

### Interoperability: choose by purpose

- [ ] **R1025.** Runtime mesh delivery — **Candidate and documented role:** [Khronos glTF/GLB](https://www.khronos.org/gltf/) targets runtime 3D asset delivery — **Proposed boundary in Lince:** Render geometry/materials; preserve a separate semantic/identity manifest. A GLB does not supply process logic or operational truth.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0728.** B03 — **Fixture:** A 5 m × 4 m × 2.7 m room with a 0.9 m × 2.1 m opening and wall-mounted content — **Proof required:** Dimensions, support/collision, local mounts, flat/perspective interaction and saved calibration separation

### Owned fixture specifications

- [ ] **R0756.** B31 — **Fixture:** A continuously moving native/programmatic edit whose bounded preview worker is slower than pointer updates, with out-of-order completion, source/selection/authority changes and cancellation — **Proof required:** Compatible displayed progress without request-order regression, current requested and displayed solved values distinguishable, no stale errors or accepted-state mutation, exact final-target validation, one receipt/undo, bounded queue and released obsolete demand; room/formula first, CAD later

### Owned fixture specifications

- [ ] **R0765.** B40 — **Fixture:** Several selected matching ordinary/derived mesh occurrences plus unselected and differently configured copies, then a supported CAD-derived source — **Proof required:** Same normal native tools and selected/shared scope, one compatible variant per input group with retained recipe lineage, preserved occurrence transforms/material overrides/state, no unintended other-copy change, coherent supported batch/undo, cancel/stale/denied recovery and bounded reuse/work; no mandatory conversion wizard or false exact-constraint preservation

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Author and furnish the complete room/castle, use its doorway and content, edit a selected group of repeated furnishings, then collaborate with another editor during a drag. Review actual appearance/input screenshots and measure edit/frame/commit/capture costs, not just emitted triangle counts.

- [ ] **R0563.** **[stage 07](07-stage.md) — L2/L3/L4, room L5 proof** — **Intention and defined deliverable:** Native wall/floor/opening edits and mounted content with normal transactions — **Correctness/failure evidence and performance obligation:** B03/B22: one accepted operation under spatial/numeric controls, readable input and stable mounts. Measure preview/commit/frame latency and actual screenshots on named hardware; editing a face cannot rebuild unrelated world assets. B30/B31 add bounded shared-boundary planar triangulation and moving-preview/final-target evidence. B34/B36 cover shared room/formula edits and the ordered Park adapter tasks with separate placement/pixel/input permissions, capture limits and revoke/lock recovery. B40 proves ordinary multi-copy derived-mesh editing, retained source lineage and one supported batch/undo with no conversion wizard.

- [ ] Run the actual native/backend/persistence/transport path for every 07.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0637.** author the room fixture with a passable opening and useful wall content; orthographic/perspective/table selection identifies the same object. Virtual movement leaves calibrated physical outputs unchanged.

- [ ] Advance only when **all 07.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Replace the pending fields below with a compact verified handoff, preferably at most 200 words excluding concrete code/test links. Record the actual APIs/module paths, accepted contract decisions, workflow proofs, measured limits and remaining scope needed next. Update only this stage's registry status in `00-General.md` after its advance gate passes; status stays pending until then.

  ```yaml
  status: pending
  delivered_user_workflows: []
  contract_and_module_changes: []
  fixtures_and_replay_commands: []
  observed_error_latency_and_resource_results: []
  permission_failure_and_recovery_evidence: []
  remaining_blocked_or_deferred_scope: []
  next_stage_context: []
  ```

- [ ] After verified implementation, remove only completed task detail as required by repository planning rules; preserve this factual completion summary, unfinished requirements and usable links to the implemented contracts/evidence. Do not replace a pending task with an invented success summary.
