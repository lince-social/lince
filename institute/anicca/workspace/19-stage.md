# 19 — Verified elasticity, flexible bodies and heating — deferred

Finish able to configure and inspect a bounded verified elastic study, steady heat conduction and transient heating/cooling from captured inputs. The user sees supported conditions, quantitative error and energy/balance evidence; motion cooling of Sands remains a separate existing behavior.

## Required workflow milestones

- [ ] **19.M01.** Set supported material regions, loads/supports and boundary maps on an accepted part; solve the named elastic patch/bar/cantilever cases and inspect source-mapped deformation/fields with honest convergence and applicability.

- [ ] **19.M02.** Set conductivity, capacity/density, initial temperature, source power and supported thermal boundaries; compare steady slab flux and transient heating with the independent reference and energy/time-refinement checks.

- [ ] **19.M03.** Cancel, exhaust a quota, change source/conditions or supply singular/unsupported inputs. Preserve accepted sources, expose failed/incomplete/stale states and never display them as passed current results.

- [ ] **19.M04.** Reopen captured inputs/results offline and share only permitted outputs with a weak visitor. Solver/run costs remain independently admitted and do not starve ordinary workspace navigation/editing.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

- [ ] Confirm the preceding [stage 18](18-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 16 summary](16-stage.md#completion-summary); [stage 17 summary](17-stage.md#completion-summary); [stage 18 summary](18-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Keep this stage deferred until the [core stabilization gate](15-stage.md#advance-gate) passes and this numerical slice has implementation authorization. Capture actual inputs, reference/error/quantity/resource limits, data rights, worker trust/location and payer before dispatch; regress affected stable-core workflows.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed material/support/result contracts)`; `selected bounded mesher/solver worker`; `crates/engine/src/`; `crates/interface/src/native/`.

## Ordered implementation tasks

- [ ] **R0701.** **Deferred until the core is stable and the selected material/run prerequisites pass.** This stage introduces verified temperature/heating/thermodynamic cooling, flexible/elastic behavior and thermal conduction within named supported models; it is separate from existing Sand motion cooling and geometry/support stabilization.

- [ ] **19.T01.** Choose one mesher/solver adapter for the stated linear-elastic and conduction subsets. Implement job inputs/results/statuses, named boundary/material maps, unit conversion, cancellation/quotas/checkpoints and retained logs. For heating, declare steady versus transient scope: transient evolution additionally requires heat capacity/density, initial temperature, sources, time integration and the applicable boundary/contact heat law. A steady conduction result alone does not deliver time-dependent heating; radiation/convection and phase change remain separate supported-law extensions.

- [ ] **19.T02.** Implement one bounded transient heating/cooling case with declared density/heat capacity/conductivity, initial temperature, source power and supported thermal boundaries. Validate a uniform insulated sample with constant capacity against `ΔT=Q/(m c_p)` in its applicable regime, plus a transient conduction reference and time refinement where spatial gradients matter. Report boundary/source energy balance; temperatures, heat flux and total power retain distinct units. Coupling to mechanical motion or deformation remains a separately validated later operator.

- [ ] **19.T03.** Provide native setup and result views with explicit loads, supports, conditions, scalar/tensor legends, convergence and stale/invalid coverage. Add deformable playback/preview with mappings to source and independently generated render geometry. Apply H7/H8 to shared result viewing and explicitly located workers, using existing observation/admission/disclosure paths; a viewer needs neither solver installation nor raw private inputs when the permitted output contract allows that view.

- [ ] **19.T04.** Verify a patch/uniaxial case, slender cantilever in its applicable regime and an analytic slab heat-flow case; perform mesh/time refinement and balance checks. Keep approximate interactive flexibility labeled separately from these checked continuum models.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Validate elasticity on captured source.** Bind named regions/materials/loads/supports to a selected admissible discretization and solve the patch/bar/cantilever references. Report units, assumptions, mapping, convergence and invalid constraints before presenting deformation as a current result.

- [ ] **Cycle B — Validate conduction and heating separately.** Capture conductivity/capacity/density, initial state, power and boundary conditions. Compare steady slab flux and transient response with independent references, energy balance and mesh/time refinement; choose actual error/work bounds from the named continuum profile.

- [ ] **Cycle C — Integrate failure, offline and weak viewing.** Cancel/exhaust quota, change source/boundaries and reject singular/unsupported inputs through the common job/result statuses. Reopen captured inputs/results offline, share permitted fields and keep stable native editing responsive under worker limits.

- [ ] Preserve these contracts after authorization; a loaded component and heated sample are example studies using built-in continuum setup/mapping/result inspection.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **19.W01 — Inspect a verified elastic result** | Built-in supported elasticity study; 19.M01. Assign regions/loads, solve references and inspect mapped deformation/fields. | Units, deformation display scale, constraints, convergence and excluded regions are understandable. | Reference error and mapping validity meet adopted bounds; compare patch/bar/cantilever quantities under refinement; record DOFs, residuals, time and peak solver/display bytes. |
| **19.W02 — Inspect verified heat flow and transient heating** | Built-in supported thermal study; 19.M02. Set initial/boundary/source data and compare steady/transient results. | Source power, time, temperature versus motion cooling and incomplete results remain clear. | Unit/sign/energy/time-refinement/reference checks pass chosen bounds; report error versus discretization/time step and worker cost, not color smoothness alone. |
| **19.W03 — Fail safely, reopen and share results** | Built-in job/status/offline/participation; 19.M03/19.M04. Cancel, invalidate, reopen offline and let a weak visitor inspect allowed fields. | Current/stale/failed/partial status and missing coverage support a safe next action. | Invalid/stale results accepted as current = 0; sources/rights survive; record quota/cancel/seek/transfer peaks and unchanged native interaction under admitted jobs. |

- [ ] **Checks to select.** Independent analytic/reference and unit/property tests cover rigid modes, load/boundary maps, energy and time/mesh convergence in supported regimes. Integration/fault tests cover singular models, missing properties, expired maps, cancellation, quota exhaustion and denied outputs. Native replay authors boundary conditions, runs/inspects both studies and reopens offline; numerical pass evidence accompanies the actual result UI.

- [ ] **Earlier-work repair.** Retain 16.W01/16.W02 property applicability, 17.W03 job isolation, 18.W02 safe source/state handling and 15.W01–15.W05. Source deformation/animation maps may be reused only with valid numerical mappings; this stage does not certify later cloth/hair/nonlinear tissue models.

## Concrete acceptance replay

- [ ] **19.P01 — Elastic results have reference and mapping proof; 19.T01/19.T03/19.T04 / 19.W01.** After authorization, set named material/load/support regions natively and solve the patch/bar/cantilever references in their stated regime. Compare displacement/stress/reactions and rigid modes with independent solutions, inspect source-mapped fields and vary mesh resolution. Record geometry/map revision, units, DOFs, residual/reference error, convergence, time and peak solver/display bytes. Review load/support legends and deformation display scale; a smooth image cannot replace the actual quantity check. Preserve 16.W01 applicability and the accepted source from 10.W01.

- [ ] **19.P02 — Heating is different from steady conduction; 19.T02–19.T04 / 19.W02.** Set conductivity/capacity/density, initial temperature, source power and supported boundaries. Compare steady slab heat flow, the uniform insulated energy reference and a spatially varying transient reference with separate mesh/time refinement. Check temperature/flux/power kinds, source/boundary signs and accumulated energy balance against adopted tolerances. Record time steps, cells/DOFs, numerical error, solve/output peaks and native setup/result cost. Review time/temperature versus existing Sand motion cooling without changing its semantics.

- [ ] **19.P03 — Failed studies remain inspectable, not current; 19.T01/19.T03/19.T04 / 19.W03.** Cancel/exhaust a quota, supply singular supports or missing material data, then change a boundary/source while the old job finishes. Assert explicit failed/partial/stale states and no invalid current-result adoption. Reopen permitted inputs/results offline and share a bounded output with a weak visitor denied raw private inputs. Measure cancellation, transfer/decode/release and mixed native response; inspect understandable recovery. Preserve 17.W03 job authority, 18.W02 source/state handling and affected 15.W01–15.W05 stable work.

## Worked contract example

- [ ] **19.X01.** Use this analytic reference only for the declared uniform insulated constant-capacity sample; negative energy can describe heat removal. Spatial gradients, phase change, convection/radiation and coupled mechanics need their own supported models and reference checks. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn uniform_temperature_rise_k(energy_j: f64, mass_kg: f64, capacity_j_kg_k: f64) -> Option<f64> {
      if ![energy_j, mass_kg, capacity_j_kg_k]
          .iter()
          .all(|v| v.is_finite())
          || mass_kg <= 0.0
          || capacity_j_kg_k <= 0.0
      {
          return None;
      }
      let capacity_j_k = mass_kg * capacity_j_kg_k;
      if !capacity_j_k.is_finite() || capacity_j_k <= 0.0 {
          return None;
      }
      let rise = energy_j / capacity_j_k;
      rise.is_finite().then_some(rise)
  }

  fn main() {
      assert_eq!(uniform_temperature_rise_k(2500.0, 0.5, 500.0), Some(10.0));
      assert_eq!(uniform_temperature_rise_k(2500.0, 0.0, 500.0), None);
  }
  ```

## Architecture and task requirements

### Proposed defaults for refinement

- [ ] **R0058.** First continuum studies — **Proposed default:** Deferred [stage 19](19-stage.md): small-strain elasticity and heat conduction, each independently verified before coupling — **When it may change:** Accurate heating/thermodynamic cooling and deformation wait for [stage 15](15-stage.md) stabilization plus the relevant material/runtime prerequisites

### Continuum/discretization requirements

- [ ] **R0519.** Structural studies need reference/deformed configurations, material regions/orientation, contacts, load cases, supports and beam/shell/solid idealization. Thermal studies need conductivity/capacity/density, interfaces, sources and boundaries. Mesh generation preserves named regions and boundary IDs, checks quality/orientation/connectivity and reports defects. [Gmsh describes physical groups and meshing contracts](https://gmsh.info/doc/texinfo/); it is a candidate adapter rather than a demand to embed every mesher.

- [ ] **R0520.** Compare mesh/time refinement against quantities of interest, not just an attractive stress heatmap. Check rigid-body modes, singular supports, near-incompressibility/locking where relevant, linear/nonlinear applicability, and energy/mass balance within declared tolerance. Flexible interactive previews may use approximate methods, but a fast spring mesh is not automatically a validated engineering solid. [NIST's verification/validation guidance](https://www.nist.gov/publications/summary-industrial-verification-validation-and-uncertainty-quantification-procedures) informs the evidence discipline; each domain still needs its own tests.

### Verification obligations

- [ ] **R0819.** **Deferred scientific verification, stages [16](16-stage.md)–[21](21-stage.md).** Scientific evidence distinguishes code verification, numerical convergence, parameter calibration, validation against measurements and uncertainty. A successful solver exit satisfies none of the others automatically. Capture reference inputs/expected results, tolerances, build/platform/settings, observed outputs and applicability. Avoid calling an unvalidated approximation physical truth. These obligations do not require constructing or certifying new scientific capabilities before [stage 15](15-stage.md).

- [ ] **R0820.** Deferred [stage 19](19-stage.md) quantitative checks have explicit reference problems: a uniform linear-elastic bar in axial load gives elongation `ΔL=FL/(EA)` with free lateral boundaries and the specified support; a sufficiently slender, small-deflection cantilever compares tip response to `δ=FL³/(3EI)` within that beam idealization; a homogeneous conduction slab with insulated sides and fixed face temperatures gives `|q|=k|ΔT|/L`. Keep force versus traction and heat flux versus total power distinct (`P=|q|A` for a uniform face). Exclude singular point-load/clamp neighborhoods from false pointwise stress convergence claims. Use an isotropic tensor and proper rigid rotations to verify constitutive invariance before testing anisotropy. Add a separately referenced transient heating case with capacity, source/boundary energy and time refinement before claiming time-dependent heating.

### Deformation views and later body/garment studies

- [ ] Reuse stage 11's accepted reference/pose/region/deformation maps when displaying this stage's admitted mechanical fields, keeping authoring geometry and visual skinning distinct from the numerical discretization. Validate supported load/boundary/source mappings and reject an unmapped tissue or garment region rather than borrowing a nearby vertex or animation weight as a physical constraint.

- [ ] Keep nonlinear physical cloth/hair, active muscles, ligaments/tendons and calibrated tissue/contact studies in [stage 21](21-stage.md#articulated-secondary-motion-and-biomechanical-studies). The linear-elastic patch/cantilever proofs here do not establish cloth drape, tendon force, human tissue accuracy or clinical validity; a detailed body can still be viewed/posed without those calculations.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0737.** B12 — **Fixture:** Elastic patch/cantilever and conduction slab — **Proof required:** Units, analytical comparison, convergence, balanced quantities and failed-run visibility

## Workflow, correctness and performance evidence

- [ ] Instantiate the local **19.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Study a selected authored component under an applicable elastic load and a bounded heated sample, including source/boundary changes and a reference comparison. Sweep numerical resolution/time step against actual quantities of interest, error, time and peak bytes; a colorful stress/temperature mesh alone is not completion.

- [ ] **R0575.** **[stage 19](19-stage.md) — deferred L0–L5** — **Intention and defined deliverable:** Verified continuum flexibility/elasticity and thermal conduction/heating setup and results — **Correctness/failure evidence and performance obligation:** B12/B18/B21/B37: named boundary/material maps, units, heat sources/capacity where transient, convergence/reference balances and failed/stale status. Measure error versus work/time/memory; new scientific work cannot starve the stable core.

- [ ] Run the actual native/backend/persistence/transport path for every 19.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] Verify every local **19.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0706.** repeat bounded elastic, steady-conduction and transient-heating studies offline from captured inputs and obtain their declared quantitative tolerances; incomplete, singular, divergent, unsupported or stale runs cannot appear as passed current results.

- [ ] Advance only when **all 19.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **19.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

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
