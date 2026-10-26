# 09 — Imported bases, authored overlays and physical comparison

Finish able to place an authored castle/room over a real or imported site, inspect it beside the unchanged source at declared physical scale and update the source without losing customization or silently moving the authored structure.

## Required workflow milestones

- [ ] **09.M01.** Keep imported terrain/building/scan as a pinned base and author an opening/replacement/addition in a separate layer. Inspect the active edit target, hide/replace/additive behavior and the same selected object in overlay/side-by-side views.

- [ ] **09.M02.** Register a modeled room/site using declared units/frames and measured control points, inspect residuals and a withheld check point, and compare at 1:1. Moving the virtual model does not recalibrate a projector or move the physical installation.

- [ ] **09.M03.** Reimport a renamed/reordered/split source and reconcile the local layer. Fixed engineering poses stay fixed by default; explicitly surface-relative items follow only verified successors and ambiguous/deleted supports retain last pose with repair.

- [ ] **09.M04.** Keep the castle/location identity, mounted content and source provenance intact while a better terrain source arrives. Show mismatch and pending reconciliation rather than allowing the new source timestamp to erase authored work.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

- [ ] Confirm the preceding [stage 08](08-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 07 summary](07-stage.md#completion-summary); [stage 08 summary](08-stage.md#completion-summary); [stage 03 summary](03-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed)`; `crates/interface/src/native/physical_workspace/`; `crates/interface/src/native/topology/`; `crates/nucleus/src/physical_workspace.rs`.

- [ ] Use captured/synthetic geometry and declared measurement/control-point data for Workspace alignment evidence. Preserve the existing distinction between virtual placement, model-to-source registration and physical calibration. This stage adds no camera/projector setup, detector/driver task or real-equipment acceptance requirement; the installation work remains in [Ailuros](../Ailuros.md).

## Ordered implementation tasks


- [ ] **09.T01.** Implement pinned source/base layers, sparse typed overrides, hide/replace versus additive composition, independent instance runtime and edit-target inspection. Support overlay and side-by-side views with shared or independent cameras.

- [ ] **09.T02.** Add manual measured registration from declared frames/control points with known scale, residuals and withheld checks; allow explicit estimated alignment. Store model-to-physical association separately from projector/camera calibration.

- [ ] **09.T03.** Implement source re-import three-way conflict/mapping review, split/deletion repair and preserved last poses. Test that a source rename/reorder does not erase authored work or Record bindings. Verify Q17's default preservation of authored/measured placement under changed terrain/scan sources and explicit verified surface-follow exceptions, with native mismatch/reconcile controls; deliberate parent movement and existing mounted attachments still behave normally.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Separate base and authored intent.** Pin imported source/version/units/rights and keep additions, masks and replacements in explicit local layers. Connect overlay/side-by-side views to the same identities and active edit target without duplicating the source document.

- [ ] **Cycle B — Measure alignment in software.** Solve the declared supported registration using unit/frame maps and control points; report residuals and an independent withheld check. Preserve physical output calibration and distinguish authored adjustment from a new calibration operation; no equipment setup is required.

- [ ] **Cycle C — Reimport through verified correspondence.** Match renamed/reordered/split features using supported evidence, preserve fixed engineering placement and update surface-follow items only through valid successors. Retain last pose/source on ambiguity and provide a native reconciliation/repair path with undo.

- [ ] Preserve these contracts; a measured room and a customized geographic site are example recipes over built-in import/layer/registration/reconciliation primitives.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **09.W01 — Compare a customized imported place** | Built-in layered editing/comparison plus room/site recipe; 09.M01/09.M04. Change an opening/structure and inspect both source and authored views. | Active layer, source age, mismatch and visible authored intent remain clear. | Imported base stays pinned; timestamp arrival cannot erase custom work; measure narrow patch/affected query work and aggregate dual-view residency. |
| **09.W02 — Register and compare at declared scale** | Built-in supported registration tools; 09.M02. Set control points, inspect held-out error and compare at 1:1. | Misalignment and uncertainty are visible; moving the virtual model has predictable scope. | Preserve unit/frame conversions and physical calibration; report control/withheld residuals against the chosen source-specific bound; record solve/input/view costs without claiming survey accuracy from rendering. |
| **09.W03 — Reimport and repair deliberately** | Built-in source update/correspondence tools; 09.M03. Reimport changed features, reconcile a layer and repair an ambiguous mount. | Fixed versus surface-follow placement, conflicts and repair are understandable. | Fixed poses stay fixed; verified successors alone may follow; identities/source lineage/undo survive; measure changed bytes, affected mappings and replacement peaks instead of whole-source copies. |

- [ ] **Checks to select.** Unit/property tests cover frame/unit registration, held-out points, layer composition and fixed/follow policies. Integration/fault tests reorder/rename/split/delete sources, race local edits and deny required dependencies; compare incremental results with a bounded from-source reference. Native replay switches comparison views, registers, reimports and resolves the actual affected attachment.

- [ ] **Earlier-work repair.** Retain 07.W01 authored placement, 08.W02 source recovery, 04.W03 mounts and 06.W01 offline places. A better imported source must preserve the earlier castle/landmark and ordinary support/navigation behavior until an explicit accepted change replaces them.

## Concrete acceptance replay

- [ ] **09.P01 — Authored intent remains separate; 09.T01 / 09.W01.** Import a versioned site/room, author an opening/addition/replacement and inspect base/custom layers in overlay and side-by-side views. Hide/show each layer, edit only its declared scope and reopen. Compare accepted source hashes, custom patches, occurrence IDs and mounted content; count changed bytes and aggregate dual-view residency. Review active edit target, source age and whether both designs are legible. Retain the recognizable authored landmark and usable interior from 07.W01/06.W01.

- [ ] **09.P02 — Registration has an independent check; 09.T02 / 09.W02.** Use declared units and a synthetic/captured set of known control points; solve the supported registration and inspect withheld points that were not used in fitting. Introduce a wrong unit/point or degenerate configuration and reject or label the result according to the contract. Record fit and held-out residuals separately, source uncertainty, solve/input/view costs and the selected tolerance. Verify virtual adjustment leaves physical calibration untouched; review 1:1 comparison and mismatch feedback without requiring equipment.

- [ ] **09.P03 — Reimport changes source, not history; 09.T03 / 09.W03.** Reimport reordered/renamed/split/deleted features while local overrides exist. Preserve fixed engineering placements; move explicitly surface-following content only with verified correspondence, and repair ambiguity natively with undo. Compare incremental reconciliation with a bounded independently constructed expected result. Record affected mappings, dirty bounds, source/patch bytes and old-new replacement peaks; review conflicts and last-pose recovery. Preserve 04.W03 mounts and 08.W02 retained editable source through the update.

## Worked contract example

- [ ] **09.X01.** Preserve fixed versus explicitly surface-following placement intent in reimport commands. The fixed-pose example does not remove deliberate parent motion, calibrated associations or existing valid mounts; expose mismatch/repair for incompatible source changes. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  struct Pose {
      x_mm: i64,
      y_mm: i64,
      z_mm: i64,
  }

  fn fixed_authored_pose_after_source_update(authored: Pose, _new_source_height_mm: i64) -> Pose {
      authored
  }

  fn main() {
      let castle = Pose {
          x_mm: 1200,
          y_mm: 3400,
          z_mm: 200,
      };
      assert_eq!(fixed_authored_pose_after_source_update(castle, 700), castle);
  }
  ```

## Architecture and task requirements

### Source baseline and shared families

- [ ] **R0073.** Physical workstation: `crates/nucleus/src/physical_workspace.rs`, native `physical_workspace/`, [Ailuros Record](../Lince.lingua) — **What to reuse:** Measured planar surfaces, calibration revisions, assigned content, dedicated outputs and replay — **Required extension or boundary:** Model-to-installation registration is explicit. A virtual edit does not move or recalibrate a real installation. Current hardware/output bounds remain meaningful.

### Commit, recovery, sharing and garbage collection

- [ ] **R0234.** Import/re-import performs a three-way reconciliation: previous source, new source, local layer. Stable source IDs take priority; geometric fingerprints are suggestions with confidence. Deleted/split elements leave unresolved bindings and repair actions. Authored layers are not overwritten by a newer source timestamp.

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0844.** **Q17 — Preserve authored placement on source replacement.** Updated terrain/scans retain authored/measured engineering poses by default and show mismatch/reconciliation. Explicit surface-relative placements can follow verified successors; ambiguous mappings require repair. Parent transforms and existing supported attachments retain their ordinary semantics.

### Accepted architecture/detail choices and stage-specific evidence

- [ ] **R0859.** **A04 — Agreed: preserve authored pose under source replacement.** Better terrain/scan data does not silently move a measured/authored house or machine. Show mismatch and reconciliation; explicit surface-relative attachments may follow verified successors, with repair for ambiguous mappings. Deliberate parent movement and existing mounted Sand following remain ordinary operations. See Q17; stages [stage 04](04-stage.md)/[stage 05](05-stage.md)/[stage 09](09-stage.md).

### Interoperability: choose by purpose

- [ ] **R1030.** Composed authoring scenes — **Candidate and documented role:** [OpenUSD](https://openusd.org/release/intro.html) supports layered scene composition and interchange — **Proposed boundary in Lince:** Use an optional conversion/composition worker when a source requires it; retain layer references and semantic mappings. Do not assume prim paths are durable Record IDs.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0731.** B06 — **Fixture:** A measured/imported room with authored opening and at least one withheld registration check point — **Proof required:** Alignment residual/error, source age, re-import split/deletion conflicts and 1:1 comparison

## Workflow, correctness and performance evidence

- [ ] Instantiate the local **09.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Use a measured/imported room with an authored opening and a saved geographic site with a custom structure. Reimport changed sources while inspecting both views and collaborating; measure patch size, affected query work and dual-view residency rather than copying the whole source.

- [ ] **R0565.** **[stage 09](09-stage.md) — L1/L2/L3/L4** — **Intention and defined deliverable:** Imported base plus customization, alignment and explicit comparison — **Correctness/failure evidence and performance obligation:** B06/B19: re-import/conflict/deleted selection, independent accepted/draft state, unchanged physical calibration. Measure patch size, affected-query work and dual-view memory; no entire-source copy for a one-object override. Q17 preserves authored engineering placement after source replacement, with explicit verified surface-follow/reconcile behavior.

- [ ] Run the actual native/backend/persistence/transport path for every 09.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] Verify every local **09.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0645.** compare a customization at declared 1:1 physical scale to its unchanged base, inspect alignment error, move the virtual model and re-import without silently rewriting the installation or accepted geometry.

- [ ] Advance only when **all 09.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **09.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

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
