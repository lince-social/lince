# 11 — Assemblies, building systems, drawings and fabrication data

Finish able to assemble reused parts into a designed room/building system, inspect fit and geometric motion, produce associative drawings/takeoff and exchange a bounded manufacturing or building workflow. Physical strength, heating and new joint dynamics remain deferred.

## Required workflow milestones

- [ ] **11.M01.** Assemble a reused door/panel/mounting part, position it with geometric mates/joint ports and check clearances/interference. Edit a source dimension and inspect coherent occurrences/drawings without starting a new dynamics solver.

- [ ] **11.M02.** Build wall/slab/roof/opening/furnishing presets from the same definitions and source relations used for free parts. Generate plans/sections/elevations and count/area/volume takeoff at the accepted revision; unknown weight/properties remain unknown.

- [ ] **11.M03.** Import/reimport a declared IFC/BCF subset and carry stable identities/unsupported data honestly. Produce the selected cut-list/manufacturing exchange without executing equipment or claiming process physics.

- [ ] **11.M04.** Prove each selected further geometry subset independently: sheet-metal unfold/refold with bend assumptions, composite/lattice geometry and corridor/terrain/utility/geotechnical data authoring. Keep numerical process/ground-response predictions in the deferred specialist stage.

## Entry and scope

- [ ] Confirm the preceding [stage 10](10-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 10 summary](10-stage.md#completion-summary); [stage 07 summary](07-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed)`; `crates/interface/src/native/`; `selected CAD/exchange adapter or bounded worker`; `crates/nucleus/src/record.rs`.

## Ordered implementation tasks


- [ ] **11.T01.** Extend CAD operations in bounded sets: sweep/loft, fillet/chamfer, shell/offset and patterns, each with a failure/precision fixture. Add assembly mates/joint ports as geometric relations, kinematic position inspection, constraints, interference and clearance tools. Constraint/pose solving uses authored geometry and does not execute new joint dynamics, stress/strength or mass/inertia prediction.

- [ ] **11.T02.** Build native wall/slab/roof/opening/furniture and system presets from the same definitions/relations and preserved material references/visual assignments. Add plans/sections/elevations, associative dimensions/tolerances and revisioned dimensional/count/area/volume takeoff/drawings. Read an explicit existing/imported density or recorded mass only with provenance; absent data remains unknown and new material-property evaluation waits for [stage 16](16-stage.md). No atomistic or constitutive setup is required for a building preset.

- [ ] **11.T03.** Add a declared IFC/BCF import/issue subset with source identities, units, spaces, layers and openings. Support cut lists, process/material requirements and an initial manufacturing export/postprocessor only for a selected real workflow.

- [ ] **11.T04.** Further CAD/manufacturing and civil authoring profiles: sheet-metal unfold/refold with bend assumptions, composite/lattice generation, manufacturing drawing/tolerance exchange and corridor/terrain/utility/geotechnical tools. Select each bounded operation set and fixture; reuse source/history/material/selection contracts and keep process/toolpath execution separate. Deliver these geometry/representation/exchange subsets here as separately scoped bounded operations. Preserve their process/material assumptions without implementing forming/process physics, geotechnical response, optimization or live equipment control; new physical predictions remain [stage 21](21-stage.md) requirements.

## Worked contract example

- [ ] **11.X01.** Derive checked dimensional/count takeoff from accepted source geometry and instance membership, with opening/overlap semantics defined per item. Missing density/mass is not zero and does not trigger a material-property engine before stage 16. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn total_panel_area_mm2(width_mm: u64, height_mm: u64, instances: u64) -> Option<u64> {
      width_mm.checked_mul(height_mm)?.checked_mul(instances)
  }

  fn main() {
      assert_eq!(total_panel_area_mm2(900, 2100, 4), Some(7_560_000));
      let recorded_mass_kg: Option<f64> = None;
      assert!(recorded_mass_kg.is_none());
      assert_eq!(total_panel_area_mm2(u64::MAX, 2, 1), None);
  }
  ```

## Architecture and task requirements

### Domain workflows as implementations of the primitives

- [ ] **R0365.** Building/civil work — **Authored/source support:** Spaces/elements, surfaces/volumes, alignment/network — **Specialized operation/constraint:** Layers/openings, corridors/systems, takeoff and domain checks — **Shared outputs and reuse:** Geography, plans/sections, analytical models and Record/procurement links

### Buildings, AECO and local spatial design

- [ ] **R0446.** A building model composes site/building/storey/space references, element definitions/occurrences, materials/layers, openings, connections, systems and constraints. Spatial containment and physical decomposition stay separate. A room volume can be bounded by shared walls; the wall is not duplicated as a physical object per adjacent room. Reference centerlines, nominal dimensions and as-built surveys have distinct provenance.

- [ ] **R0447.** Native presets build walls/slabs/roofs, openings/doors/windows, stairs and reusable furnishings from the geometry/CAD primitives. Add layered assemblies, reference levels/grids, plans/sections and quantity takeoff. Takeoff uses declared measurement rules and source revision, deduplicates instances/openings, and reports uncertainty or unsupported geometry; a visual bounding box is not accepted billable volume.

- [ ] **R0448.** Structural, MEP, thermal zones and circulation are different analytical representations. A duct network has ports/connectivity and pressure/flow assumptions; a structural wall has load paths/boundaries; a thermal wall has layers and contact conditions. Their geometries can map to the same wall without pretending IFC properties implement their solvers. Maintain fire/accessibility/clearance/regulatory rule versions and jurisdiction in issue/check adapters; an imported classification is not a claim of code compliance.

- [ ] **R0449.** Civil design uses alignment curves/profiles, cross-sections, corridors, terrain cuts/fills, parcels/rights and utility networks as specialized supports/relations. Geotechnical layers and groundwater are volumetric/property inputs with source uncertainty. Connect streets and neighborhoods through these spatial/network primitives rather than baking their meaning into a decorative city mesh. Sheet-metal unfolding, composite layups, additive lattices and other manufacturing specializations similarly need material/process-aware operators and their own acceptance fixtures; preserve them as domain expansion scope rather than claiming generic extrusion completes CAD.

- [ ] **R0450.** Import a declared IFC subset with stable source identity, placement, units, types/occurrences, material layers, spaces and opening semantics. Preserve GlobalId/source namespace and unsupported report. Use an isolated IfcOpenShell candidate worker when justified; its [geometry settings](https://docs.ifcopenshell.org/ifcopenshell/geometry_settings.html) show why unit/material/opening configuration matters. Export/viewpoint issues through a bounded IFC/BCF workflow as needed.

- [ ] **R0451.** AECO construction scheduling, procurement, resources, production, operations, maintenance, civic participation and commerce remain in F1–F10 below. Their forms/queries reuse Records, evidence, quantities, commitments and permissions. They do not require a detailed model to work, and personal room authoring does not require a factory ledger first.

### Interoperability: choose by purpose

- [ ] **R1026.** Engineering asset semantics — **Candidate and documented role:** [buildingSMART IFC](https://technical.buildingsmart.org/standards/ifc/) describes built-environment data — **Proposed boundary in Lince:** Import selected identities, relationships, properties and geometry through a bounded worker; retain original source and unsupported-property report.

- [ ] **R1027.** Model-linked issue exchange — **Candidate and documented role:** [buildingSMART BCF](https://technical.buildingsmart.org/standards/bcf/) supports model-based issue communication — **Proposed boundary in Lince:** Map issues, viewpoints and relevant element references to Records/proposals; do not assume it transfers the whole model.

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Reuse one source assembly as both a standalone part and a building element, revise it and inspect downstream drawings/takeoff/exchange. Use enough repeated real modules to measure unique-source versus occurrence cost and actual fit/selection/edit responsiveness.

- [ ] **R0567.** **[stage 11](11-stage.md) — L2/L3/L4/L5** — **Intention and defined deliverable:** Reused assemblies, building-system presets, drawings/dimensional takeoff and bounded fabrication/civil geometry exchange — **Correctness/failure evidence and performance obligation:** B10/B14/B39 core subsets: preserved definition/material references, selection and occurrences stay coherent across forms/geometry. Measure geometric/count/area/volume evaluation as selected assembly size grows; missing physical properties remain unknown. Source CAD stays independent of display LOD and unsupported exchange is explicit; no new material/physical solver is required.

- [ ] Run the actual native/backend/persistence/transport path for every 11.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0656.** reuse one assembly carrying material references/visual assignments as a free part and building element; drawings and dimensional/count/area/volume takeoff use the same accepted revision, while missing physical data stays unknown. IFC re-import preserves supported identity or asks for repair. Source/cut-list/toolpath export is separate from physical process simulation and equipment execution. New material-property interpretation and dynamic assemblies remain deferred.

- [ ] Advance only when **all 11.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
