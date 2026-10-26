# 08 — Caves, selective low-poly optimization and safe support

Finish with an attractive editable cave containing a floor, ceiling, overhang and passage, then explicitly optimize settled room/cave regions for long-term use. Useful openings and mounted content survive and editable source is retained.

## Required workflow milestones

- [ ] **08.M01.** Author a cave with multiple surfaces at the same horizontal location, walk through its protected passage and use mounted content on a selected face. Existing Areas and gravity continue to respect the supported geometry.

- [ ] **08.M02.** Preview a selected low-poly simplification with polygon/work/error budgets and protected openings/edges/mounts. Inspect its actual silhouette/facets, accept or cancel, undo and save/reload without losing source.

- [ ] **08.M03.** Rebuild only changed render/support regions; cancel or supersede a bake while another revision is accepted. Keep the old valid support until replacement is ready and expose unresolved correspondence instead of attaching content to a guessed face.

- [ ] **08.M04.** Leave the finished space idle and navigate away/back. Geometry is reused, obsolete RAM/GPU/collider products release under demand and the stable place does not remesh every frame.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

- [ ] Confirm the preceding [stage 07](07-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 07 summary](07-stage.md#completion-summary); [stage 04 summary](04-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/interface/src/native/topology/`; `crates/workspace_model/ (proposed)`; `crates/interface/src/native/topology/physics.rs`.

## Ordered implementation tasks


- [ ] **08.T01.** Build the cave with floor/ceiling/overhangs using bounded connected mesh editing; add selected implicit-volume operations only when the fixture needs topology-changing sculpting.

- [ ] **08.T02.** Implement T3's preview/simplify/bake workflow with counts, dimensional error, protected boundaries/passages/mounts, source retention, correspondence and cancel/undo. Distinguish estimated versus bounded error.

- [ ] **08.T03.** Produce purpose-specific render/collision proxies, cache only affected regions and implement safe existing step-boundary support replacement/wake semantics. Preserve additive live terrain effects and explicit stale/rebake state. This derives geometry for the implemented controller/physics, not new material-driven inertia, joint/flexible physics or thermal behavior; those remain deferred.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Represent and traverse real overhangs.** Implement the selected multi-surface/volumetric cave subset behind the existing support/source contract. Exercise a ceiling and narrow passage with actual Walk and mounted content; keep the declared open/closed support capabilities honest.

- [ ] **Cycle B — Optimize a selected source.** Capture protected openings/edges/mount selections, polygon/work/error limits and the mapping recipe. Generate a preview, measure supported error and review facets/silhouette before accepting a retained-source low-poly product; failed or unknown guarantees remain visible.

- [ ] **Cycle C — Replace and release safely.** Build only affected render/support regions, validate correspondence/generations and replace support at the supported boundary while retaining the previous usable product. Test cancel/supersede, undo/reopen, idle reuse and last-demand release, including old/new staging peaks.

- [ ] Preserve these contracts; the cave/room is an example recipe for built-in multi-surface support and selective source-preserving optimization.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **08.W01 — Inhabit an overhanging space** | Built-in cave support plus authored cave recipe; 08.M01. Walk a protected passage, look up and use wall content. | Ceiling/passage readability, useful low-poly form and controls work from inhabited viewpoints. | Multiple surfaces at one horizontal location resolve correctly; protected passage/mounts remain valid; measure queries, active triangles/support data and existing Area/gravity response within the selected profile. |
| **08.W02 — Choose a useful low-poly result** | Built-in selected simplification/bake tools; 08.M02. Set limits, compare preview, accept/cancel, undo and reopen. | Silhouette/facets and protected features pass the recorded views; low polygon count alone is insufficient. | Source and exact authored dimensions remain recoverable; report achieved/unknown error and protected-feature checks; measure counts, bake time, scratch and dual-buffer peaks against admitted bounds. |
| **08.W03 — Edit support without losing the place** | Built-in changed-region replacement/lifetime; 08.M03/08.M04. Supersede a bake, replace ready support, idle and revisit. | Pending/repair states preserve a usable place and intelligible next action. | Stale publication/guessed attachment acceptance = 0; no idle remesh loop; measure affected-only rebuilds, cancellation, frame cost and last-demand release. |

- [ ] **Checks to select.** Geometry/property tests cover protected boundaries, declared distance/error methods and valid mapping coverage. Integration/fault tests race a bake with source edits, undo and closure; assert old support remains usable until valid replacement and ambiguous mounts request repair. Native replay simplifies the actual navigated cave and visually compares selected viewpoints before walking it again.

- [ ] **Earlier-work repair.** Retain 07.W01 authoring/doorway/mounts, 04.W01 influence and 05.W02/05.W04 navigation/demand. Later skinned simplification uses stage 11's weight/morph/region contract; a successful static cave bake cannot certify articulated deformation.

## Concrete acceptance replay

- [ ] **08.P01 — The cave needs real multi-surface geometry; 08.T01 / 08.W01.** Author floor, ceiling, overhang and a protected passage, query multiple supported surfaces at the same horizontal coordinate, walk through and use a mounted Sand. Check query normals/intersections and passage clearance against the fixture geometry, including existing Area/gravity behavior. Count actual active triangles/support regions and query work; review low-poly shape and navigation from floor/ceiling/passage viewpoints. Reuse 07.W01 controls/save and 05.W02 support rather than a cave-specific controller.

- [ ] **08.P02 — Simplification preserves intended use; 08.T02 / 08.W02.** Select a settled region, set polygon/work/error bounds and protected openings/edges/mounts, compare preview at named views, accept/cancel, undo and reopen. Compare measured or bounded geometric error with an independent source query and label sampled-only coverage explicitly. Verify source recovery and passage/mount validity; record input/output counts, achieved error, elapsed bake time, scratch and dual-buffer peak. A lower count fails when the recorded silhouette/facet rubric or usable opening fails.

- [ ] **08.P03 — Replace only a valid changed product; 08.T03 / 08.W03.** Supersede a bake with a source edit, finish the older job late, replace ready render/support products at the supported boundary and leave/revisit/idle the space. Assert no stale publication, guessed mount mapping or per-frame rebake; retain last usable support and an actionable repair. Measure affected regions, cancellation, wake/settle behavior, replacement peaks and final release. Preserve 04.W01 influence, 04.W03 attachments and 05.W04 demand through the replacement, not only before it.

## Worked contract example

- [ ] **08.X01.** Produce the actual error/correspondence/passage evidence before accepting a bake; never supply these booleans from a success exit alone. Label sampled versus bounded geometric error and retain explicit repair when a protected mapping cannot be proved. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn accept_bake(
      source: u64,
      current: u64,
      error_mm: f64,
      requested_mm: f64,
      passage_open: bool,
      mappings_valid: bool,
  ) -> bool {
      source == current
          && error_mm.is_finite()
          && requested_mm.is_finite()
          && error_mm >= 0.0
          && requested_mm >= 0.0
          && error_mm <= requested_mm
          && passage_open
          && mappings_valid
  }

  fn main() {
      assert!(accept_bake(3, 3, 4.0, 5.0, true, true));
      assert!(!accept_bake(3, 4, 4.0, 5.0, true, true));
      assert!(!accept_bake(3, 3, 4.0, 5.0, false, true));
  }
  ```

## Architecture and task requirements

### Low-poly appearance, simplification and physical use

- [ ] **R0329.** Low-poly quality is an authored style: deliberate planar regions, silhouettes, readable scale, restrained materials/lighting, consistent face orientation and chosen sharp/smooth boundaries. Fewer polygons alone do not make a scene attractive or faster. Preserve UV seams, material interfaces, narrow passages and meaningful architectural edges.

- [ ] **R0330.** Simplification takes selected supports, triangle/vertex budget, dimensioned shape-error limit and protected features. Return a preview, source-to-result mapping, achieved counts/error, violations and timing. Distinguish a heuristic simplifier metric from a measured maximum deviation or a conservative bound. Sampling a Hausdorff-like distance is not proof of a global maximum.

- [ ] **R0331.** Use constrained collapse/QEM or another proven library as the leading surface candidate; protect borders, mounted patches, openings, sharp edges and alignment references. Reject collapses that invert faces or destroy required passages. The [original QEM work](https://www.cs.cmu.edu/~garland/quadrics/) establishes the simplification family; additional quality checks are Lince requirements, not guaranteed by that algorithm.

- [ ] **R0333.** A collision update is transactional at a physics step boundary: retain the old collider until the replacement is ready, map contacts where valid, resolve new penetration deliberately, then wake affected bodies. Visual LOD changes do not swap physical support underneath a sleeping Sand. Protect navigable clearances and mount orientation through save/reload and rebake.

- [ ] **R0334.** The [stage 10 CAD requirements](10-stage.md#architecture-and-task-requirements) surface-plan refinement applies the same source/representation distinction to shared-edge tessellation and CPU/GPU evaluation. [stage 07](07-stage.md) implements the supported planar subset; [stage 10](10-stage.md) widens analytic/trimmed CAD surfaces. B30 verifies boundary agreement, visible error, sharp normals and reuse before an optimization can replace the existing renderer path.

- [ ] **R0335.** Detailed topology requirements T1–T6 below remain normative and use these contracts.

### T3. Simplify and bake a settled shape

- [ ] **R0884.** Add an explicit low-poly optimization tool with a preview, triangle budget and maximum shape-error control. Show before/after counts and protected features before applying.

- [ ] **R0885.** Protect openings, boundaries, sharp edges and mounting/calibration patches. Offer selective optimization of part of a space instead of forcing a whole-scene operation.

- [ ] **R0886.** Produce cached render meshes and suitable collision meshes, preserving stable surface associations or an explicit remapping. Reject or require adjustment when simplification exceeds the allowed error or closes an opening.

- [ ] **R0887.** Keep the editable source and bake settings. Support undo, save/reload and deliberate rebaking after source edits. A stale asynchronous bake must never overwrite a newer geometry revision.

- [ ] **R0888.** Perform optimization on bounded background jobs with cancellation. Reuse geometry while idle; release obsolete meshes and collider caches. Establish budgets for triangles, jobs and memory before implementation.

- [ ] **R0889.** Acceptance: simplified room and cave fixtures remain attractive, retain passable openings and stable wall content, and have measured render/collision costs. Record hardware, triangle counts, memory, frame-time percentiles, bake duration and shape error. Choose numerical targets during refinement; do not claim efficiency from polygon counts alone.

- [ ] Extend this same source-preserving simplification contract in stage 11 when articulated geometry is authored: propagate or explicitly repair skin weights, morph deltas, semantic regions and attachments, and measure deformation in declared test poses. Static room simplification does not prove a safe character bake, and a changed vertex order cannot be accepted against stale deformation indices. This stage retains its current room/cave gate without requiring the later rig editor.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0729.** B04 — **Fixture:** A cave with two surfaces at the same horizontal location, a ceiling, overhang and narrow passage — **Proof required:** Surface queries, no height-only assumption, protected passage under simplification

## Workflow, correctness and performance evidence

- [ ] Instantiate the local **08.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Optimize the room/cave people actually navigate and furnish, with a doorway, ceiling, narrow passage and repeated wall content. Record before/after visual review, achieved dimensional error, protected-feature integrity, bake/cancel cost and dual-buffer peaks; a low polygon count alone is insufficient.

- [ ] **R0564.** **[stage 08](08-stage.md) — L0/L2/L3/L4** — **Intention and defined deliverable:** Cave support and selected low-poly bake/collision outputs from retained editable source — **Correctness/failure evidence and performance obligation:** B04/B05: passable protected openings, source maps, stale/cancelled bake and safe support replacement. Record shape-error method, counts, peak dual-buffer residency, bake/cancellation cost and frame percentiles.

- [ ] Run the actual native/backend/persistence/transport path for every 08.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] Verify every local **08.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0641.** room and cave remain attractive, navigable and mounted correctly after simplification and reload. New geometry cannot remove physical support before its collider is valid. Record measured errors/costs on the named hardware.

- [ ] Advance only when **all 08.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **08.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

  ```yaml
  status: pending
  gate_verified_against_revision: pending
  implementation_and_rationale: pending
  relevant_alternatives_not_chosen: []
  retained_workflow_contracts: []
  delivered_user_workflows: []
  contract_and_module_changes: []
  fixtures_and_replay_commands: []
  observed_error_latency_and_resource_results: []
  qualitative_review_results: []
  permission_failure_and_recovery_evidence: []
  remaining_blocked_or_deferred_scope: []
  current_regressions_and_repair_state: []
  next_stage_context: []
  ```

- [ ] After verified implementation, remove only completed task detail as required by repository planning rules; preserve this factual completion summary, unfinished requirements and usable links to the implemented contracts/evidence. Do not replace a pending task with an invented success summary. Keep the retained workflow paragraphs independently readable; replace completed instructions/examples with real contract/test links and redirect incoming anchors before pruning detail. Unfinished scope keeps its full implementation context.
