# 16 — Material properties, conditions and offline knowledge — deferred

After core stabilization, finish able to change a rod/pillar from steel to wood using an ordinary material picker and inspect which supported physical property/model applies, under what conditions and from which evidence. No microscopic editor or molecular population is required.

## Required workflow milestones

- [ ] **16.M01.** Choose steel or wood for a selected existing region with the normal local/shared scope. Inspect actual supported density/elastic/thermal candidates, grain orientation/conditions and affected products; keep appearance independently editable.

- [ ] **16.M02.** Use an attributed bounded offline knowledge pack, preserve conflicting candidates and reject missing/out-of-domain units/conditions. Unknown conductivity/density cannot become a plausible zero or an invented prediction.

- [ ] **16.M03.** Import material definitions and reuse verified compatible versions without name-based global replacement. Material specification, batch, assignment, environment and evolving state remain distinct; joining/viewing does not start a solver.

- [ ] **16.M04.** Keep the prior stable workspace/navigation/editing workflows usable while material lookup/validation is performed. Expose cost and diagnostics without silently activating later heat/deformation/microscopic jobs.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

- [ ] Confirm the preceding [stage 15](15-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 15 summary](15-stage.md#completion-summary); [stage 02 summary](02-stage.md#completion-summary); [stage 11 summary](11-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Keep this stage deferred until the [core stabilization gate](15-stage.md#advance-gate) passes and this numerical slice has implementation authorization. Capture actual inputs, reference/error/quantity/resource limits, data rights, worker trust/location and payer before dispatch; regress affected stable-core workflows.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed material profile)`; `crates/engine/src/`; `crates/interface/src/native/`; `selected bounded knowledge import adapters`.

## Ordered implementation tasks

- [ ] **R0677.** **Deferred until [stage 15](15-stage.md) passes.** First choose the numerical B07/B16/B39 fixtures, dataset rights, applicable quantities, offline pack, reference results and resource/error limits at this stage. Core material names/appearance and passive imported definitions do not complete this stage, and this stage is not a prerequisite for earlier CAD, locality or room authoring.

- [ ] **16.T01.** Implement material specification/sample/batch/assignment/state distinctions, region/layer/interface references and typed property candidates. Add the native material/conditions/evidence inspector and explicit invented-model authoring.

- [ ] **16.T02.** Complete W01/B16 on a simple dimensioned local rod/region created without the full CAD toolchain: change steel to wood through a native dropdown/inspector and the same authorized binding command. Expose applicable models, needed grain orientation/conditions, local/shared scope and invalidated products. This path has no microscopic editor, atomic structure or molecular solver prerequisite.

- [ ] **16.T03.** Import a small rights-reviewed pack with source IDs/revisions, exact unit conversions, methodology, applicability and unknown/conflict states. Keep visual appearance and physical properties separately editable. Extend B39 with actual material-definition reuse, same-name/different-value candidates, independent material state/conditions and an imported object using the destination's compatible environment binding; no global material replacement or duplicate forces.

- [ ] **16.T04.** Implement the first linear-elastic/conduction model descriptors and parameter validation. Test unsupported conditions, conflicting evidence, anisotropic orientation, bad units, missing density and multi-material parts without yet claiming a numerical solver.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Resolve an ordinary material assignment.** Reuse definitions, selected regions, units/conditions and the normal local/shared scope to assign steel/wood or a supported layer. Keep visual appearance, material specification, batch/environment and evolving state distinct.

- [ ] **Cycle B — Admit evidence and offline knowledge.** Load only the selected attributed bounded property pack; preserve conflicting candidates, orientation, applicability and unknown values. Validate unit/quantity kinds and conditions before exposing a candidate, using existing import/admission/rights paths.

- [ ] **Cycle C — Reuse imports and invalidate narrowly.** Exercise compatible definition reuse, local/shared replacement, offline reopen and denial/invalid inputs. Update only affected properties/products and retain the stable workspace with no microscopic or later thermal/dynamics jobs started by lookup.

- [ ] Preserve these contracts after this deferred stage is authorized; the rod/pillar and layered component are example recipes for built-in material/evidence resolution.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **16.W01 — Change a component's physical material directly** | Built-in picker/assignment/evidence tools plus component recipe; 16.M01. Replace steel/wood on selected regions and inspect supported properties/conditions. | Scope, grain/orientation, applicable versus unavailable values and independent appearance are clear. | Unit/kind/conditions and exact selected scope hold; invented/unknown-as-zero properties = 0; measure narrow lookup/commit/invalidation and zero microscopic worker starts. |
| **16.W02 — Inspect trustworthy partial knowledge offline** | Built-in bounded pack/query validation; 16.M02. Compare sourced candidates and reject incompatible inputs offline. | Provenance, conflict, uncertainty and missing coverage support an informed choice. | Rights/versions/validity remain attached; unsupported/out-of-domain data cannot become an accepted prediction; measure pack/decode/query peaks and zero offline requests. |
| **16.W03 — Import definitions without changing unrelated behavior** | Built-in definition/state/import reuse; 16.M03/16.M04. Import, reuse or retain conflicts, then reopen and navigate/edit. | Imported/reused/conflicting status and existing environment bindings are understandable. | Name alone cannot replace a definition; viewers cannot edit; new solver starts = 0; record affected changes and mixed native/resource costs against the stable profile. |

- [ ] **Checks to select.** Unit/property tests cover quantity kinds, affine versus difference units, condition ranges, orientation and explicit unknown statuses. Integration/fault cases cover conflicting versions, denied evidence, malformed/oversized packs and changed assignment scope. Native replay uses the normal material dropdown, inspects candidates, undoes/reopens and continues ordinary navigation; lookup timing is measured separately from future numerical work.

- [ ] **Earlier-work repair.** Retain 15.W01–15.W05 and the relevant 02.W01/11.W01/12.W02 definition and amount contracts. Fix regressions in stable core or direct material use before stage 17; no speculative property derivation replaces a missing empirical option.

## Concrete acceptance replay

- [ ] **16.P01 — Direct assignment remains ordinary editing; 16.T01/16.T02 / 16.W01.** After authorization, choose steel then wood for one rod/region through the normal inspector, set supported orientation/conditions, undo and reopen. Inspect candidates and affected products through the same binding command used by other clients. Compare exact selected scope, quantity kinds/units and applicability with the bounded reference pack; unselected copies and independent appearance stay unchanged. Measure lookup/commit/invalidation and native response while confirming zero microscopic/thermal/dynamics jobs. Review scope, grain/orientation and unavailable-value feedback.

- [ ] **16.P02 — Offline evidence can be incomplete; 16.T03 / 16.W02.** Import the attributed pack, disconnect fully and compare conflicting candidates, source methods, temperature/condition ranges and units. Inject incompatible kinds, out-of-domain values, missing density, denied evidence and oversized inputs. Verify each native result against the fixture's expected accepted/rejected/unknown state, including absolute-versus-difference temperature handling. Record pack/index/decode peaks and query latency with zero offline requests. Preserve 06.W01 offline closure and 02.W01 definition/source identity; no convenient zero fills an unknown.

- [ ] **16.P03 — Definitions and evolving state stay separate; 16.T03/16.T04 / 16.W03.** Import verified matching and same-name/different material definitions, assign a supported layered/anisotropic case and inspect invalid parameters. Retain conflicting versions, provenance and destination environment bindings without overwriting independent state. Measure only affected dependents and aggregate stable-workspace costs; exercise the relevant 15.W01–15.W05 workflow beside lookup. Review imported/reused/conflict and unsupported-model status. Keep actual derivation, thermal solve and additional physics pending at their owning stages.

## Worked contract example

- [ ] **16.X01.** Implement typed quantity/unit/evidence/applicability resolution rather than this scalar-only illustrative subset. A returned density does not supply a force field, stiffness tensor, friction coefficient or a valid heating law. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn applicable_scalar(
      value: Option<f64>,
      temperature_k: f64,
      valid_k: (f64, f64),
  ) -> Result<f64, &'static str> {
      if !temperature_k.is_finite()
          || !valid_k.0.is_finite()
          || !valid_k.1.is_finite()
          || valid_k.0 > valid_k.1
      {
          return Err("invalid conditions");
      }
      if temperature_k < valid_k.0 || temperature_k > valid_k.1 {
          return Err("out of domain");
      }
      value.filter(|v| v.is_finite()).ok_or("missing property")
  }

  fn main() {
      assert_eq!(
          applicable_scalar(Some(7800.0), 293.0, (280.0, 320.0)),
          Ok(7800.0)
      );
      assert_eq!(
          applicable_scalar(None, 293.0, (280.0, 320.0)),
          Err("missing property")
      );
      assert_eq!(
          applicable_scalar(Some(7800.0), 600.0, (280.0, 320.0)),
          Err("out of domain")
      );
  }
  ```

## Architecture and task requirements

### Confirmed intent and context recovery

- [ ] **R0012.** **C01 — Independent entry points.** Selecting wood instead of steel for a rod/pillar is a complete supported material-edit path. It can use documented empirical properties without a microscopic definition or a molecular solver. A detailed material investigation is an additional path into the same typed model bindings.

### Capability coverage and reusable workflow recipes

- [ ] **R0109.** **W01 — Direct material use** — **Shared primitives and thin specialization:** Binding, typed candidates, conditions, applicability and invalidation; material picker is an L3 presentation — **Minimum proof / later expansion:** Deferred [stage 16](16-stage.md)/B16: selected rod changes steel/wood without microscopic loading; later multi-material/interface/graded assignments. Core references/appearance alone do not complete physical property use

### K4. Quantities, fields, evidence and unknown values

- [ ] **R0162.** A property resolver returns the chosen candidate and its applicability evidence. Conflicting sources remain separate candidates. Resolution is governed by an explicit workspace/material policy or user choice; timestamps and numeric averages are not default scientific authority.

- [ ] **R0163.** Extrapolation or user-invented laws remain allowed when deliberately selected, with their assumptions stored in the scenario. Evidence-backed and invented behavior must remain inspectable side by side.

### K5. Relations, ports and constraints

- [ ] **R0169.** Interface properties belong at the interface: contact conductance, friction, adhesion, electrical contact and bonded/slipping behavior cannot always be inferred from the two bulk materials.

### Materials: knowledge, structure, constitutive behavior and scale bridges

- [ ] **R0336.** **Deferred delivery: stages [16](16-stage.md)–[17](17-stage.md) and the relevant stages [18](18-stage.md)–[21](21-stage.md) profiles, after [stage 15](15-stage.md) stabilization.** This section preserves the complete future physical-material requirements, including condition-dependent properties, density/tensors, heat-related parameters and constitutive laws. Core definition import/reference/provenance, material names and visual appearance use existing/shared contracts without implementing this property engine, populating a scientific database or silently deriving physical behavior. Existing supported material/physics behavior is preserved; only the new capabilities described here are deferred.

### Material identity and state

- [ ] **R0337.** Separate a material specification, a physical/digital sample or batch, a region assignment and its evolving state. A specification names composition/structure descriptors and candidate property/constitutive models. A batch adds process/history/quality evidence. A part assigns material versions to regions, layers, fibers, coatings, pores, interfaces or graded distributions. A run owns temperature, plastic strain, damage and other history variables.

- [ ] **R0338.** Composition is typed: chemical identity, isotope identity where needed, species fractions with a mass/mole/volume basis, phase fractions, lattice/crystal orientation, molecular conformers, microstructure descriptors and processing conditions. Validate normalization and charge/mass assumptions where the selected profile requires them. Defects, grain boundaries, porosity, moisture, fabrication, texture and interfaces can change behavior. Composition and folding alone cannot supply every bulk property. [NIST's structure–property research](https://www.nist.gov/programs-projects/structure-property-links-deformation-materials) supports retaining those intermediate descriptions.

### Property model contract

- [ ] **R0339.** A property candidate includes quantity kind/unit, scalar/vector/tensor/table/law, support/sample, material orientation, phase, conditions, method and uncertainty. Conditions can include temperature, pressure, strain/rate, frequency, wavelength, moisture and history. The exact dimensions depend on the physical question.

- [ ] **R0340.** A **constitutive model** is stronger than a property bag. It declares input state/gradients, outputs, internal variables, admissible parameter ranges, reference configuration, integration/update method and required tangent/derivative support. Initial subsets can implement small-strain linear elasticity and Fourier conduction. Plasticity, hyperelasticity, viscoelasticity, fracture, phase change and reactive behavior are later supported models with their own validation.

- [ ] **R0341.** Linear elasticity uses `σ = C:ε` under a stated stress/strain convention. Orthotropic/anisotropic materials retain orientation and tensor symmetry. Appropriate stability/positivity checks depend on the model; do not accept any 6×6 numeric matrix as usable elasticity.

- [ ] **R0342.** Fourier heat conduction uses `q = -K∇T`, with tensor `K` if needed. Capacity, density and interface resistance are separate inputs. Radiation/convection require boundary models and are not encoded by calling a surface “warm.”

- [ ] **R0343.** An interaction friction/restitution model for Avian is not automatically derivable from bulk Young's modulus. Adhesion/contact/wear depend on surfaces and conditions. Preserve explicit interaction parameters and their evidence.

- [ ] **R0344.** Render appearance is another model: color/roughness/metallicity/opacity or measured optical response. A physical electrical conductor need not be assigned a metallic shader; roughness is not a thermodynamic material law. Measured spectral data can drive rendering through a named approximation.

- [ ] **R0345.** Resolution returns supported/incomplete/out-of-domain plus the selected model/version and required missing inputs. User-authored fictional properties are allowed as explicit models. Bad inputs cannot silently fall back to plausible-looking defaults in a scientific result.

### Knowledge ingestion and applicability

- [ ] **R0346.** Import bounded, attributed offline subsets. Preserve external identity, source revision, original units, conversion, conditions, methodology, uncertainties, license/redistribution rules and supplied versus inferred values. Re-import exposes property-model conflicts. A provider's search hit is evidence of a record, not proof it applies to the selected alloy, specimen or temperature.

- [ ] **R0347.** [PubChem](https://pubchem.ncbi.nlm.nih.gov/) — **Useful scope:** Chemical identity, structures and source-attributed reported properties — **Boundary:** Not a complete bulk constitutive database; preserve individual property evidence

- [ ] **R0348.** [Materials Project elasticity methodology](https://docs.materialsproject.org/methodology/materials-methodology/elasticity) — **Useful scope:** Computed crystalline elastic tensors and documented calculation settings — **Boundary:** Conditions/methods and comparison to experiments matter; an ideal crystal is not an arbitrary manufactured part

- [ ] **R0349.** [NIST reference data](https://www.nist.gov/srd) and measured engineering data — **Useful scope:** Specified physical/thermophysical quantities with references — **Boundary:** Dataset rights and covered phases/conditions differ; some resources are not freely redistributable

- [ ] **R0350.** [IUCr CIF](https://www.iucr.org/what-we-do/digital-standards/cif) and [PDBx/mmCIF](https://mmcif.wwpdb.org/docs/user-guide/guide.html) — **Useful scope:** Crystal/macromolecular structures, cells, symmetry and experimental metadata — **Boundary:** Structure records do not provide every force field, folding path or bulk property

- [ ] **R0351.** Evaluated nuclear data through a selected adapter — **Useful scope:** Isotope-dependent interactions, decay and model inputs — **Boundary:** Chemical species tables cannot replace evaluated nuclear data

- [ ] **R0352.** Select adapters by the first question and license, not by the largest downloadable dataset. A useful initial pack can contain only a few traceable materials, one crystal, one finite molecule and the reference cases used by tests. Record API keys outside portable files and avoid per-frame remote lookup.

### Explicit cross-scale transfer

- [ ] **R0353.** A scale bridge is a derivation, not containment. Under C01–C05, a wall **uses** an effective material model, and a standalone specimen can remain the end goal. Neither consuming a model nor changing the represented physical amount creates a runtime child entity per atom. A direct material choice and a micro-derived model use the same typed binding/applicability path. Store the fine input, representative sample/volume, boundary conditions, ensemble, method, averaging assumptions, output law, validity range, convergence and validation evidence.

- [ ] **R0354.** Recommended first real bridge: import a documented crystal stiffness tensor with its structure/method; rotate it correctly; calculate orientation-dependent response; then optionally derive Voigt/Reuss bounds and a Hill estimate for a stated randomly oriented polycrystal assumption. Keep anisotropic crystal and effective isotropic outputs distinct. Derive `E` and `ν` from effective `K` and `G` only under the isotropic assumption. Show that porosity, texture or processing outside those assumptions requires another model.

- [ ] **R0355.** An earlier geometry-based bridge is ideal crystal density: `ρ_ideal = Σ_j occupancy_j mass_j / |det A|`, where `A` is the physical cell matrix and the sum is over that cell's full site population, with symmetry multiplicities expanded or accounted for exactly once. Here mass is per site/particle; a molar-mass input requires the declared Avogadro conversion. Changing cell/occupancy can recompute this quantity without a quantum solver. It does not predict porosity, phase stability or measured bulk density. Under the later isotropic elastic approximation, use `K_H=(K_V+K_R)/2`, `G_H=(G_V+G_R)/2`, `E=9K_HG_H/(3K_H+G_H)` and `ν=(3K_H-2G_H)/(2(3K_H+G_H))`, rejecting singular/invalid inputs. Compute the Voigt/Reuss terms from the declared stiffness/compliance convention and verify against independent reference values.

- [ ] **R0356.** This proves knowledge lineage and a bounded mathematical bridge. It does not claim that editing atom positions automatically recomputes stiffness. Geometry edits invalidate the link until a supported calculation, new measurement or explicit retained approximation justifies it. A later MD/quantum adapter can populate a new candidate against the edited structure.

- [ ] **R0357.** For computed bridges, require sample-size/ensemble and mesh/time convergence, reference units and the appropriate averaging. Density, stiffness, thermal conductivity and electronic response need different methods. Simple mixture fractions are not a universal blending rule. Downscaling an effective continuum state to microscopic initial conditions is underdetermined; require an explicit generated sample/ensemble and assumptions.

### Domain workflows as implementations of the primitives

- [ ] **R0362.** Material model — **Authored/source support:** Composition/microstructure/sample plus evidence — **Specialized operation/constraint:** Applicable constitutive law and explicit scale bridge — **Shared outputs and reuse:** Versioned law/fields assigned to regions in any part

### Fixture phases and the stabilization boundary

- [ ] **R0767.** B07/B09/B16 — **Core proof through [stage 15](15-stage.md):** Passive definition/reference/provenance and render appearance can be exercised in other core import/shape fixtures; these do not complete physical-material applicability. No physical study required. — **Deferred proof in stages [16](16-stage.md)–[21](21-stage.md):** [stage 16](16-stage.md) property selection/conditions/knowledge; [stage 17](17-stage.md) checked density/tensor bridge and direct-material/micro-derived paired proof

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0732.** B07 — **Fixture:** Two conflicting property candidates plus a layered part and a missing conductivity — **Proof required:** Conditions/units/orientation, no invented zero, evidence and explicit approximation

### Owned fixture specifications

- [ ] **R0741.** B16 — **Fixture:** A selected rod/pillar region with documented steel and wood candidates — **Proof required:** Native direct assignment, grain/condition/coverage diagnostics, unchanged source geometry/identity, explicit local/shared scope; no microscopic load or solver demand

## Workflow, correctness and performance evidence

- [ ] Instantiate the local **16.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Revise the material assignment of a dimensioned rod/pillar and a layered component, compare two evidence candidates and reopen offline. Measure lookup, narrow commit and invalidation of only affected products; prove zero microscopic worker/population demand for the direct picker.

- [ ] **R0572.** **[stage 16](16-stage.md) — deferred L1/L2/L3/L4, ordinary-use L5 proof** — **Intention and defined deliverable:** W01: direct physical-material choice and applicable property/condition/evidence resolution — **Correctness/failure evidence and performance obligation:** B07/B16/B39 material subset: steel/wood swap, conditions/orientation/coverage, local/shared scope and missing/conflicting data. Measure only affected lookup/commit/products and zero microscopic solver demand. Existing hosting commands protect permitted edits; no viewer can invoke an editor's material change.

- [ ] Run the actual native/backend/persistence/transport path for every 16.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] Verify every local **16.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0682.** direct material assignment works independently and can answer which supported property/model applies, why, and what is missing. B16 proves no hidden microscopic population or solver demand. A saved/exported pack works offline with attribution and cannot invent absent properties.

- [ ] Advance only when **all 16.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **16.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

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
