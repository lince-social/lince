# 11 — Assemblies, articulated models, building systems, drawings and fabrication data

Finish able to assemble reused parts into a designed room/building system, inspect fit and geometric motion, produce associative drawings/takeoff and exchange a bounded manufacturing or building workflow. Through the same relations and representations, customize and pose an articulated character, walk it through an authored place, inspect optional named anatomy and return to a lightweight body. Physical strength, heating, force-driven joints and validated tissue/biomechanical predictions remain deferred.

## Required workflow milestones

- [ ] **11.M01.** Assemble a reused door/panel/mounting part, position it with geometric mates/joint ports and check clearances/interference. Edit a source dimension and inspect coherent occurrences/drawings without starting a new dynamics solver.

- [ ] **11.M02.** Build wall/slab/roof/opening/furnishing presets from the same definitions and source relations used for free parts. Generate plans/sections/elevations and count/area/volume takeoff at the accepted revision; unknown weight/properties remain unknown.

- [ ] **11.M03.** Import/reimport a declared IFC/BCF subset and carry stable identities/unsupported data honestly. Produce the selected cut-list/manufacturing exchange without executing equipment or claiming process physics.

- [ ] **11.M04.** Prove each selected further geometry subset independently: sheet-metal unfold/refold with bend assumptions, composite/lattice geometry and corridor/terrain/utility/geotechnical data authoring. Keep numerical process/ground-response predictions in the deferred specialist stage.

- [ ] **11.M05.** Select an assembly with an explicit work/takeoff recipe and inspect its revision-linked component counts, dimensions/areas/volumes and missing-input diagnostics as draft work inputs. Edit an upstream dimension or instance count and identify the affected source/output revisions without loading detailed display geometry or inferring physical behavior. Stage 13 uses these outputs for linked Needs/tasks and reconciliation.

- [ ] **11.M06.** Create or import an articulated model, inspect its reference pose and joint frames, add/edit supported controls and deformation bindings, then rotate a limb numerically and drag its endpoint with bounded inverse kinematics. Distinguish source-shape edits from poses, preserve permitted motion/limits and unresolved mappings, and undo/save/reload the actual result. Repeat basic articulation on a differently structured mechanism or creature through the same operations.

- [ ] **11.M07.** Customize a character's supported proportions, clothing and hair; author/revise a short movement clip and standing/walking/flying presentation transitions with an explicit supported control profile. Tune speed, turn, stride and foot-placement settings, walk through the modeled street/doorway and inspect the body in third person. Camera inspection and body movement remain independent, and all core motion is declared authored/kinematic without starting physical cloth, muscle or lift calculations.

- [ ] **11.M08.** Open optional sourced male/female anatomical reference packs with declared coverage, isolate a detailed upper-limb subsystem and inspect named bones/joints, ligaments, tendons, muscles, fat, major vessels and skin where supplied. Pose the limb within documented relationships, compare two saved poses and attach an attributed observation to a region/movement. Let Fiote retrieve/highlight permitted structures and measurements through the same context/commands, with native follow-up. Generic reference anatomy, user-reported symptoms and any patient-specific evidence remain distinguishable; diagnosis is not a completion requirement.

- [ ] **11.M09.** Leave detailed editing and resume lightweight dressed-character navigation; retain joint motion, identity, authored source and saved annotations while internal/high-detail payloads release after their last demand. Reopen the detail offline from an admitted pack. A permitted visitor can see the coarse body without downloading private anatomy/annotations or running a solver; appearance LOD never changes accepted motion limits or shared physical authority.

- [ ] **11.M10.** Reuse the same articulated constraints/selections for a mechanism's range/clearance, a body's reach in a room and a short assembly/gesture sequence. Use normal selected-copy editing and an explicit compatible motion mapping; a differently structured rig reports unsupported mapping instead of guessed anatomical equivalence. Inspect fit, motion and source changes through the actual native/save/query path under the existing profile.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

- [ ] Confirm the preceding [stage 10](10-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 10 summary](10-stage.md#completion-summary); [stage 07 summary](07-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] For the articulated tasks, recover [stage 05's controller/presence summary](05-stage.md#completion-summary), [stage 08's simplification summary](08-stage.md#completion-summary) or [stage 04's structured-context contract](04-stage.md#structured-context-for-workspace-actors) only when the selected task needs that interface. Keep the new rig/anatomy detail here; earlier core stages do not implement it in advance.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed)`; `crates/interface/src/native/`; `selected CAD/exchange adapter or bounded worker`; `crates/nucleus/src/record.rs`.

## Ordered implementation tasks


- [ ] **11.T01.** Extend CAD operations in bounded sets: sweep/loft, fillet/chamfer, shell/offset and patterns, each with a failure/precision fixture. Add assembly mates/joint ports as geometric relations, kinematic position inspection, constraints, interference and clearance tools. Constraint/pose solving uses authored geometry and does not execute new joint dynamics, stress/strength or mass/inertia prediction.

- [ ] **11.T02.** Build native wall/slab/roof/opening/furniture and system presets from the same definitions/relations and preserved material references/visual assignments. Add plans/sections/elevations, associative dimensions/tolerances and revisioned dimensional/count/area/volume takeoff/drawings. Read an explicit existing/imported density or recorded mass only with provenance; absent data remains unknown and new material-property evaluation waits for [stage 16](16-stage.md). No atomistic or constitutive setup is required for a building preset.

- [ ] **11.T03.** Add a declared IFC/BCF import/issue subset with source identities, units, spaces, layers and openings. Support cut lists, process/material requirements and an initial manufacturing export/postprocessor only for a selected real workflow.

- [ ] **11.T04.** Further CAD/manufacturing and civil authoring profiles: sheet-metal unfold/refold with bend assumptions, composite/lattice generation, manufacturing drawing/tolerance exchange and corridor/terrain/utility/geotechnical tools. Select each bounded operation set and fixture; reuse source/history/material/selection contracts and keep process/toolpath execution separate. Deliver these geometry/representation/exchange subsets here as separately scoped bounded operations. Preserve their process/material assumptions without implementing forming/process physics, geotechnical response, optimization or live equipment control; new physical predictions remain [stage 21](21-stage.md) requirements.

- [ ] **11.T05.** Expose accepted source/selection references and revisioned takeoff outputs for explicit work recipes through K1/K4/K6, the shared query path and native inspection. Define which component occurrences/regions contribute counts, dimensions, areas or volumes and avoid double-counting nested/reused parts. Preserve source/recipe revision, units, applicability/missing data and manual input provenance. Keep draft output separate from accepted operational Records, assignments and Transfers; stage 13 owns their application/reconciliation.

- [ ] **11.T06.** Extend the shared assembly/constraint model with stable articulated links/ports, reference configurations, typed joint coordinates/limits/couplings and rig/control-to-source maps. Provide joint/control creation, numeric forward posing, bounded endpoint/constraint solving, reference/pose editing and supported rest/bind updates through the same commands/history/undo/selection. Solve dirty connected regions with explicit residual/limit/unreachable/ambiguous status; a cyclic mechanism is not silently forced into a human bone tree. Preserve source frames/units, occurrence-local state and a recoverable accepted pose on failed/stale/cancelled solving.

- [ ] **11.T07.** Implement the selected skinning, authored morph/corrective and attachment subsets with versioned source bindings, ordinary weight/region/parameter tools and native previews. Validate joint indices, finite/nonnegative normalized weights, bind transforms, topology revision, transform conventions and admitted influence counts. Shape edits invalidate/rebuild only affected bindings/products; retain the old valid result until replacement or explicit repair. Extend the declared import/export subset for rig/skin/morph/clip data and semantic maps, preserving source/licensing/unsupported fields. Reuse stage 08's simplification and source/correspondence lifecycle for pose-tested low-poly variants.

- [ ] **11.T08.** Add bounded motion channels/keyframes/clips, compatible retarget maps, blend/transition parameters and reusable controllers over the same pose operators. Expose joint/body versus camera control and authored/kinematic versus future force-driven behavior clearly. Define root-motion ownership once, local up/support, stride/turn/acceleration, standing/walking/running/flying profiles, endpoint contact targets and explicit unavailable transitions. Reuse Rules/Signals/Areas/bindings where semantics fit; controller evaluation does not invoke Fiote per frame. A short authored clip is document source; optional capture of actual runtime activity remains stage 20.

- [ ] **11.T09.** Deliver optional versioned male/female anatomical reference packs with declared regional/structure coverage, sources/rights, semantic names and model limitations, plus detailed editing/inspection of the selected B41 upper-limb subsystem. Map anatomical structures to rig/pose controls through explicit partial/many-to-many relations; retain reference, custom morphology and observed/user-reported evidence separately. Add native layer/collection isolation, sectioning, joint/pose measurements and selected region/pose-pair annotations through existing tools. Extend structured Fiote context and allowed pose/highlight operations using actual IDs, units, maps and evidence; preserve disclosure controls and ordinary use without a provider.

- [ ] **11.T10.** Implement independent anatomical coverage, display detail and calculation profile selection with shared demand/admission. Keep immutable source meshes/rig definitions/clips reusable, per-occurrence pose/controller state separate, bounds conservative across admitted poses and work proportional to visible/admitted active content. Validate LOD/source/rig/attachment replacement across representative poses before acceptance, release internal/high-detail geometry on last demand, and preserve already accepted motion constraints. Extend hosted sparse asset/presence/query paths for optional public appearance and private structure/annotations; a representation never grants physical interaction or participant-control authority.

- [ ] **11.T11.** Deliver [B41](#fixture-specifications-owned-here) end to end with native and Agent editing, save/reload/export, controller/clip/root-motion ownership, anatomy inspection, low-poly transition, hosted denial and source/rebind failure cases. Exercise the concrete shared-workflow recipes below and record unsupported operations explicitly. Keep added physical joint/muscle/tissue/cloth/hair predictions in stages 18–21; generic posing, fitting, geometric reach and semantic explanation remain usable without those calculations.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Reuse parts as assemblies and building systems.** Build typed mates/ports, geometric clearance, plans/sections/elevations, revisioned takeoff and the supported IFC/BCF/fabrication subsets over existing definitions/occurrences/operators. Verify each selected sheet-metal/lattice/civil geometry subset independently and keep unknown physical quantities explicit.

- [ ] **Cycle B — Author articulation with the same model.** Separate reference/bind configuration, pose coordinates, source deformation and per-occurrence runtime. Add supported joints/limits/couplings, bounded FK/IK, skin/morph/attachment edits and authored motion/controller composition with one root/pose owner. Exercise a differently structured mechanism through the same commands before extending detailed anatomy.

- [ ] **Cycle C — Compose optional anatomy and ordinary use.** Admit sourced coverage separately, preserve explicit anatomical-to-control maps and structured Fiote permissions, then return to a cheap dressed body and reopen detail offline. Validate the takeoff and articulated workflows together with source edits, invalid bindings, selected copies, private annotations and the unchanged core budgets.

- [ ] Preserve these contracts; assemblies, a dressed body/anatomy subset and a mechanism are example recipes. Built-in geometry, articulation and semantic tools implement them; ergonomic, wearable, robotics and movement-journal examples remain short compositions over those tools.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **11.W01 — Assemble, draw and exchange** | Built-in assembly/building/fabrication tools plus reused module recipes; 11.M01–11.M04. Revise a part and inspect mates, clearance, drawings and declared exchange subsets. | Fit, drawing legibility, unsupported operations and source relationships remain understandable. | Stable definitions/occurrences and dimensional/count/area/volume results agree at the accepted revision; validate each geometry/exchange subset; measure unique-source versus occurrence cost and affected outputs without new dynamics. |
| **11.W02 — Derive revisioned work inputs** | Built-in takeoff operator plus explicit selected recipe; 11.M05. Change a source count/dimension and inspect linked quantities/missing inputs. | Selection, recipe, revision and incomplete quantities are clear enough for later work review. | No implicit density/weight inference or operational commitment; source/output lineage and exact counts survive undo/reopen; measure narrow evaluation/invalidation without loading full display geometry. |
| **11.W03 — Pose and animate reusable articulated models** | Built-in rig/pose/deformation/motion tools plus character/mechanism recipes; 11.M06/11.M07/11.M10. Edit limits/bindings, pose numerically and by IK, author a clip and navigate. | Controls, fit, motion transitions, reachable/unreachable targets and compatible retargeting are predictable in native tools. | Joint/pose/source identities and limits survive; one root/pose writer; invalid/stale bindings never publish; record B41 joints/vertices/weights/morphs/iterations, deformation error and active frame/edit peaks. |
| **11.W04 — Inspect optional anatomy and continue with Fiote** | Built-in semantic selection/context plus sourced anatomical recipe; 11.M08. Isolate named layers, compare poses, annotate a region and query/highlight actual structures. | Coverage, reference-versus-observation, anatomical selection and native follow-up remain intelligible. | Source/region/pose/rights mappings are explicit; private disclosure and pose-only diagnosis claims = 0; record admitted pack/context sizes and bounded query/edit costs. |
| **11.W05 — Leave detail without losing the character** | Built-in representation/demand and offline/hosted reuse; 11.M09. Simplify presentation, walk the building, reopen detail offline and admit a coarse visitor. | Navigation, appearance and return to detail remain continuous and useful. | Source/annotations/motion limits survive LOD; anatomy releases after last demand; physical solver starts = 0; record source reuse, RAM/GPU/staging peaks, release and permitted guest bytes under the existing profile. |

- [ ] **Checks to select.** Unit/property checks cover assembly transforms, quantities, FK/IK constraints, normalized finite weights, morph/attachment maps and root-motion composition. Integration/fault tests cover changed sources, ambiguous face/region maps, retarget incompatibility, singular/unreachable IK, source/pose races, hostile asset bounds and grant changes. Native replay assembles/draws/exchanges, customizes/poses/animates, inspects anatomy with Fiote and resumes lightweight offline navigation.

- [ ] **Earlier-work repair.** Retain 10.W01/10.W02 CAD intent, 07.W04 supplied customization, 05.W02 body/camera use, 08.W02 source-preserving reduction and 03.W03 shared presentations. Stage 13 consumes takeoff later; stages 18–21 add physical body/cloth/hair/tissue behavior later. Neither becomes a dependency of this core authoring gate.

## Concrete acceptance replay

- [ ] **11.P01 — Reused parts become coherent outputs; 11.T01–11.T05 / 11.W01/11.W02.** Assemble the door/panel/mounting example, change one source dimension and inspect mates, clearances, drawing dimensions and revision-linked count/area/volume takeoff. Compare quantities with an independently enumerated occurrence/region reference, avoiding nested double-counting. Import/reimport the declared IFC/BCF subset and validate every selected manufacturing/civil geometry operation separately, including assumptions/losses. Record unique versus occurrence bytes and affected-only evaluation/drawing/takeoff cost; detailed meshes and unknown density must not become takeoff prerequisites. Preserve 10.W01/10.W02 native dimension/history repair.

- [ ] **11.P02 — One articulation language serves different bodies; 11.T06–11.T08 / 11.W03.** Pose the character numerically and by endpoint controls, reach an unreachable/limited target, author a short clip, customize supported bindings/attachments and walk the doorway. Repeat on the differently structured mechanism, including its declared closed-loop subset if selected. Check reference/bind/posed transforms, weights, joint limits/residuals, retarget compatibility and exactly one root/pose owner. Record actual joints/vertices/influences/morphs, solve iterations, deformation error and frame/edit peaks; review control predictability and motion transitions. Preserve 05.W02 camera/body separation and 07.W04 selected appearance.

- [ ] **11.P03 — Anatomical detail is useful and attributed; 11.T09 / 11.W04.** Admit the declared optional reference packs, isolate the selected upper-limb layers, rotate the supported limb, save two poses and attach a private region/movement annotation. Have Fiote query/highlight actual permitted named structures, then continue natively without the provider. Compare structure/control maps and measurements with the pack's documented reference rather than visual plausibility. Record covered/missing structures, pack/context bytes and selection/query latency; review layer/pose comprehension and reference-versus-user-report distinction without adding diagnosis as an outcome.

- [ ] **11.P04 — Detail can leave while capabilities remain; 11.T10/11.T11 / 11.W05.** Return to a cheap dressed body, navigate the room, reopen detail offline and admit a coarse visitor denied internal/private payloads. Pose-test the low-poly product at the declared extremes; inject stale bind/source changes, incompatible retargeting and a cancelled replacement. Verify retained source/annotations/limits, verified-only bindings, no physics activation and last-demand internal-resource release. Measure unique-source reuse, transient old-new deformation/texture buffers and aggregate host/device costs. Preserve 08.W02 source recovery and 03.W03 view ownership in the same session.

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

- [ ] **11.X02.** Turn this illustrative B41 semantic replay into the actual native/command/asset fixture and measured acceptance evidence. The IDs/actions are examples, not a new public protocol or proof of implemented anatomy, animation or medical accuracy; use the accepted reference/pose/binding generations and current authority at every mutation.

  ```json
  {
    "fixture": "B41",
    "status": "pending",
    "phase": "core-articulated-authoring",
    "body_source": "body:reference:r1",
    "instance": "body:local:1",
    "reference_pose": "pose:rest:r1",
    "anatomical_coverage": "declared upper-limb subset",
    "actions": [
      "customize_selected_body_and_compatible_garment",
      "capture_pose_A",
      "set_supported_joint_coordinate_and_capture_pose_B",
      "attach_user_report_to_region_and_pose_pair",
      "query_permitted_named_structures_with_Fiote",
      "return_to_lightweight_third_person_walk",
      "release_last_demand_for_internal_geometry",
      "save_reopen_offline_and_undo_supported_edit",
      "repeat_articulation_on_differently_structured_mechanism"
    ],
    "expect": {
      "shared_source_copies": 1,
      "unselected_occurrences_changed": 0,
      "runtime_solver_starts": 0,
      "accepted_joint_limits_changed_by_LOD": false,
      "private_annotations_sent_to_visitor": false,
      "diagnosis_inferred_from_pose": false
    }
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

### Revisioned takeoff for linked work

- [ ] Bind each output to a stable accepted model selection and the declared recipe/parameters used to interpret it. Geometry alone does not specify a construction method, resource yield or task list. Use explicit workflow/domain definitions and supplied evidence; the ordinary modeling workflow does not require a new universal planning language.

- [ ] Reevaluate only outputs affected by a source/recipe/parameter change, using existing dependency/generation rules. Instance reorder and view LOD preserve source identity and quantities; a source split/deletion or unsupported conversion produces an inspectable mapping/missing-input issue.

- [ ] Provide bounded typed outputs that native views, Fiote and the linked-work workflow can inspect under existing read/disclosure rules. Missing density, process assumptions or material data stays unknown; this quantity bridge adds no thermal, material-property or physical-process simulation before the deferred tail.

### Articulated models, rigs and anatomy

- [ ] Use the existing identity/occurrence, frame/unit, relation/constraint, selection/support, operator/dependency, representation and activity families. Add only their concrete rig/pose/deformation/controller capabilities required by 11.T06–11.T11. A character editor, anatomical atlas, robot configurator and assembly inspector are workflows over those families; no parallel character persistence, anatomical scene graph, permission system or mandatory scientific runtime is introduced.

- [ ] Keep authored structure, reference/rest configuration, deformation bind, posed coordinates and runtime/controller state separate and explicitly linked by source revision. A T-pose/A-pose is a chosen reference asset state, not an engine-wide convention. Editing an instance's pose does not rewrite its skeleton, all related instances or accepted operational facts. Use compatible scope/batch edits and visible rebind/repair when changing body proportions or joint structure.

- [ ] Represent joint motion with documented local frames, typed coordinates, units, limits and any supported coupled relations. A rig/control hierarchy can use helper controls, while anatomical bones/joints/tissue regions remain independently identified semantic structures. Their mapping may be partial or many-to-many; never infer anatomy, physical joint behavior or patient-specific geometry from a control name. Preserve stable named selections across supported source reorder and report deleted/split/ambiguous structures.

- [ ] Implement forward posing and a bounded inverse-kinematics subset over the existing constraint/operator machinery, with declared degrees of freedom, singularity/unreachable/overconstraint behavior, iteration/work bounds and residual/tolerance. Reuse verified rigid assembly/constraint subproblems where applicable; reject unsupported coupled anatomy instead of claiming a simple hinge covers every joint. Test the selected reference's actual coupled movement rather than just its rest-pose screenshot.

- [ ] Associate skin, garments, hair and accessories through reusable deformation/attachment products with source/rig/bind/pose/LOD keys. Support inexpensive skinned/attached surfaces or curves and authored corrective shapes within declared coverage. Separate editing the reference surface, editing weights/controls and setting the current pose. Physical stretching, cloth collision, moving strands, muscle contraction and tissue stress are later admitted models; their absence must not prevent normal character use.

- [ ] Keep motion channels/curves and named pose/clip assets bounded and independently reusable. Define interpolation, blend layers/weights, joint masks, playback clock and explicit rest/local/root spaces. Retarget only through a validated correspondence for compatible dimensions/controls and preserve source clip lineage; unsupported joints remain diagnosable. An authored pose/clip expresses desired motion and is not a measured trajectory or computed muscle-force result.

- [ ] Give each active occurrence one declared pose/root-motion owner and controller state. Compose authored animation, procedural endpoint adjustment and supported constraints in a documented order without applying translation twice. Body-control profiles can describe walking/running/flying or other authored movement; explicit kinematic flight is useful without claiming aerodynamic lift. Keep camera Follow/Orbit/Fly/Walk settings, participant input and later physical dynamics independently owned. Preserve reduced-motion/pause preferences and event/demand-driven idle behavior.

- [ ] Expose normal native controls for joint/control selection, body proportions, pose manipulation, endpoint targets, compatible garment/hair fitting, keyframe/clip edits and supported movement parameters. Preview/cancel/final acceptance, numeric precision, focus/keyboard use, undo, selected-copy scope and source lineage match the room/CAD tools. Do not force a conversion wizard or a custom scripting language before basic posing/customization.

- [ ] Prepare optional anatomical packs as attributed dataset/asset workflows: curated named regions, measured scale/reference coordinates, spatial representation, segmentation/rig mapping and declared evidence/limitations. Deliver male/female reference options with variable/custom bodies supported by the same primitives. These examples do not cover all human variation; a customized body is not automatically a patient-specific anatomical reconstruction or a validated mechanical model. Do not require a complete organ catalogue to inspect the selected musculoskeletal subsystem.

- [ ] Support bones and joint surfaces/relationships, ligaments, tendons, muscles, fat, major vessels and skin for the declared detailed subset. Use shared collections/regions/relations for overlapping systems, attachments and paths; anatomical layers are not assumed to be concentric shells. Missing structures or uncertain tissue boundaries stay explicit. Selective isolate/hide, section views and semantic search expose the same IDs as spatial/numeric tools without allocating every internal structure elsewhere in the body.

- [ ] Provide permitted joint angles, reference/pose comparisons, regional dimensions and authored attachment/path measurements with declared geometric assumptions. A rest-to-pose path may be authored or measured; preserve its provenance. Do not present visual muscle bulging, a geometric distance or an animation control value as muscle activation, strain, force, pathology or calibrated range-of-motion evidence.

- [ ] Allow an attributed user observation to reference a selected body/region, two poses or an explicit authored movement segment. Preserve when/what the user reported, source revisions and any uncertainty independently from the model's anatomy. Use existing Record/evidence/field policy and source/provider disclosure rules; public avatar visibility does not expose private body detail, health annotations or an external provider request.

- [ ] Let Fiote inspect and highlight actual permitted structures/relationships, compare captured poses and use the supported pose/edit operations within delegation. Return named IDs, units, coverage, reference-versus-custom/observed status and missing inputs; the Agent can explain what is represented and discuss possibilities without inventing absent evidence. The pain-between-poses example is an enabled inspection/annotation workflow, not a diagnostic deliverable. Patient-specific clinical interpretation or intervention recommendations require a separately scoped evidence/validation workflow and cannot become an early character-editor gate.

- [ ] Keep structural coverage, rendered detail and calculated behavior independent. A low-poly body can retain complete supported movement controls while detailed internal meshes are unallocated; anatomy inspection can remain static/kinematic without a physical solver. Camera distance changes visual demand, not canonical body dimensions, joint limits or force laws. Any future reduced physical model needs its own validity/state-transfer contract.

- [ ] Bound mesh/weight/morph/clip/rig/annotation payloads before decode, validate indices/counts/finite transforms/cycle semantics and reject pathological expansion or external code execution. Optional detailed packs use the same manifest/rights/offline/cache/admission path as other large assets; keep their full size/required subset/additional download visible. Do not add their decoded payloads to the starter fixture automatically. Preserve a usable cheap fallback and last accepted representation when a candidate fails.

- [ ] Measure shared source versus per-occurrence pose/instance costs, unique deformation groups, CPU/GPU skinning/upload/queue/staging peaks, conservative moving bounds, hidden/paused work and last-demand release under the existing profile. Start with a simple measured implementation using existing Bevy capabilities, retain compact source data and dirty affected subsets, and choose advanced batching/GPU paths only when evidence justifies them. Never allocate an independent world/solver/provider session per bone, strand, tissue or avatar.

- [ ] Reuse hosted presence for permitted bounded visual state and reliable workspace edits for accepted rig/body/clip source changes. A current pose sample may use replaceable presentation delivery when its semantics fit; a source edit/measurement/accepted annotation uses its own durable operation. Show independently budgeted appearances to participants and apply existing revoke/host-loss/receipt behavior. Making a visible body collide, push objects, spend compute or obey a remote controller requires a separate supported grant/activity.

### Shared workflow recipes over articulated primitives

- [ ] Deliver the bounded core action in each row through 11.T06–11.T11 and the same B41 sources/tools, retaining unsupported geometry/behavior explicitly. These are useful recipes over the shared family, not mandates for eight standalone applications; numerical claims stay at the listed later boundary.

| Workflow | Concrete core action using shared primitives | Further physical/scientific boundary |
| --- | --- | --- |
| Character and creature authoring | Customize/pose a dressed body, attach hair/equipment and walk/fly with a declared authored profile; use an alternate joint structure | Force-driven locomotion, physical secondary motion and tissue response stay deferred |
| Ergonomics and accessibility | Place different body configurations in a room/vehicle/workstation; inspect geometric reach, clearance and sight lines | Biomechanical load, injury-risk or regulatory-compliance claims require separate validated models/rules |
| Robotics and mechanisms | Pose a chain/assembly and inspect reach, joint limits, path samples and geometric interference | Actuator dynamics, autonomous planning guarantees and live equipment control are separate capabilities |
| Clothing and wearable products | Fit a compatible garment, backpack, brace or equipment mount using body parameters, bindings and CAD attachment geometry | Fabric mechanics, comfort/pressure and protective-performance predictions need later models |
| Choreography and animation | Author/revise/reuse a short gesture, facial-control or coordinated pose sequence where supplied mappings support it | Runtime motion capture/recognition and recorded observations remain independently scoped |
| Assembly and maintenance guidance | Present ordered articulated/exploded configurations, part selections and clearance/reach annotations | Instructions do not execute equipment or establish safe operational procedure automatically |
| Anatomy education and movement journals | Isolate named layers, compare poses and attach observations to structures/movements | Clinical causation, diagnosis and treatment are not inferred from a generic atlas |
| Prosthetics and exoskeleton concepts | Combine anatomical references, CAD parts and articulated constraints for geometric fit and motion inspection | Patient-specific biomechanics, strength and device validation require separate studies |

### Anatomical and articulated interoperability decisions

- [ ] Evaluate the [Khronos glTF specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html) for the selected skin, morph-target and animation transport subset, retaining Lince's authoritative identity/semantics/control data separately. A render asset is not an anatomical ontology, rig-control program or physical model. Re-audit current Bevy importer support and license/credits before selecting extensions or libraries.

- [ ] Use [Blender's skinning distinction](https://docs.blender.org/manual/en/latest/animation/armatures/skinning/introduction.html) as reference for rigid attachments versus deformed surfaces, without adopting Blender's document model or requiring an external editor. Native tools must perform the declared core workflow.

- [ ] Evaluate [NLM Visible Human reference data](https://www.nlm.nih.gov/research/visible/visible_human.html) and other explicitly sourced packs for selected anatomy coverage/rights/quality; imaging data requires segmentation, semantic identification, correspondence and articulation preparation. Do not label raw scans as a ready rigged/medically validated atlas, require downloading the full source at startup or fabricate unprovided structures. Record the chosen dataset, preparation steps, reviewer/reference checks and remaining coverage before claiming the anatomical milestone.

## Fixture specifications owned here

- [ ] B41 — **Fixture:** Articulated character, anatomical reference and mechanism: a capped supplied dressed character with hair, a reusable custom variant, compatible and incompatible rig/motion cases, a differently articulated mechanism/creature and optional male/female reference packs with detailed named upper-limb subsets. Keep units/reference frames, identities, joint/rig/source maps, weight/morph/clip payload counts, source rights/coverage and profile limits in its manifest. — **Proof required:** First/third-person navigation, ordinary customization, reference-versus-pose editing, numeric and endpoint posing, clip/controller/root-motion ownership, geometric reach/clearance, layered anatomy/annotations/Fiote context, source-preserving low-poly transition, selected-copy isolation, undo/reload/offline/hosted privacy, malformed/rebound/stale/cancelled recovery and measured demand release. Stage 05 consumes only supplied navigation/presence, stage 07 appearance and stage 11 the full declared core authoring workflow; stages 18–21 separately prove their admitted dynamics, capture and biomechanics. A cartoon pose, raw scan, provider narrative or renderer benchmark alone cannot pass this fixture.

## Workflow, correctness and performance evidence

- [ ] Instantiate the local **11.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Reuse one source assembly as both a standalone part and a building element, revise it and inspect downstream drawings/takeoff/exchange. Use enough repeated real modules to measure unique-source versus occurrence cost and actual fit/selection/edit responsiveness.

- [ ] **R0567.** **[stage 11](11-stage.md) — L2/L3/L4/L5** — **Intention and defined deliverable:** Reused assemblies, building-system presets, drawings/dimensional takeoff and bounded fabrication/civil geometry exchange — **Correctness/failure evidence and performance obligation:** B10/B14/B39 core subsets: preserved definition/material references, selection and occurrences stay coherent across forms/geometry. Measure geometric/count/area/volume evaluation as selected assembly size grows; missing physical properties remain unknown. Source CAD stays independent of display LOD and unsupported exchange is explicit; no new material/physical solver is required.

- [ ] Run the actual native/backend/persistence/transport path for every 11.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

- [ ] **Linked-input evidence.** Extend B10/B14/B22 with a selected reused assembly and an explicit count/area/volume recipe. Change its source dimension/count, inspect affected output revisions and missing inputs, and verify that reordering/LOD leaves quantities unchanged. Retain the output/source references for stage 13 without creating accepted stock changes or live commands here.

- [ ] **Articulated workflow evidence.** Record B41's actual character → appearance/rig edit → paired poses/named anatomy → delegated Fiote query/native follow-up → low-poly third-person room navigation → offline save/reopen journey. Repeat shared operators on the mechanism/alternate rig and the workflow recipes above. Review extreme supported poses/garment fit, binding/correspondence and source/observation provenance; document what the anatomical coverage supports and what remains unknown.

- [ ] **Articulated failure/resource evidence.** Exercise invalid/nonfinite transforms and weights, unsupported cycles/couplings, unreachable targets, incompatible retargeting, removed joints/regions, topology reorder, rest-shape/bind changes, overlapping clip/controller writes, source/revocation changes and cancelled/late jobs. Verify native/Agent command parity, selected-copy isolation, host/guest privacy and reduced-motion/fallback behavior. Measure the same core input/frame/admission/release limits, proving internal geometry and scientific workers are absent during ordinary lightweight walking.

## Advance gate

- [ ] Verify every local **11.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0656.** reuse one assembly carrying material references/visual assignments as a free part and building element; drawings and dimensional/count/area/volume takeoff use the same accepted revision, while missing physical data stays unknown. IFC re-import preserves supported identity or asks for repair. Source/cut-list/toolpath export is separate from physical process simulation and equipment execution. New material-property interpretation and dynamic assemblies remain deferred.

- [ ] Advance only when **all 11.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **11.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

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
