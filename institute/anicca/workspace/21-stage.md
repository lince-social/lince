# 21 — Specialist physical models and coupled studies — deferred

Finish each named specialist slice only when its actual authoring/experiment workflow and independent reference proof work on the shared workspace primitives. A protein laboratory, manufacturing/civil study, circuit/device, living/chemical system, nuclear model and astronomical study remain distinct mathematics with the same identity, job, result, authority and resource machinery.

## Required workflow milestones

- [ ] **21.M01.** For the next authorized microscopic slice, run the selected molecular/protein minimization/dynamics experiment with complete parameter coverage, actual sample/ensemble/sampling evidence and a reference result; keep electronic structure, reaction and folding claims separately gated.

- [ ] **21.M02.** For each selected nonlinear/flexible/failure, manufacturing/civil or coupled thermal-structural/fluid/acoustic/geotechnical slice, author a useful bounded case, validate the declared quantity/state transfer and inspect failure/conservation/uncertainty. Geometry export or a generic adapter slot does not complete process physics.

- [ ] **21.M03.** For each selected circuit/device/electrothermal, chemical/multiphase/living/tree or nuclear-power slice, set the required domain data/conditions, run and inspect a named validated result with an independent endpoint and optional explicit model export.

- [ ] **21.M04.** For the astronomical slice, load versioned optional ephemerides and supported time/frame/dynamics inputs, run a bounded valid study and compare reference results without changing the already working generic scale/navigation engine.

- [ ] **21.M05.** Regress the stable personal/offline/hosted workspace after every slice. Preserve source rights, explicit worker/payer, fair resources, honest unsupported ranges and independent coarse/fine fidelity; never mark all specialist rows complete because one solver runs.

## Entry and scope

- [ ] Confirm the preceding [stage 20](20-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 15 summary](15-stage.md#completion-summary); [stage 17 summary](17-stage.md#completion-summary); [stage 18 summary](18-stage.md#completion-summary); [stage 19 summary](19-stage.md#completion-summary); [stage 20 summary](20-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Keep this stage deferred until the [core stabilization gate](15-stage.md#advance-gate) passes and this numerical slice has implementation authorization. Capture actual inputs, reference/error/quantity/resource limits, data rights, worker trust/location and payer before dispatch; regress affected stable-core workflows.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `selected specialist profile/worker per validated slice`; `crates/workspace_model/ (proposed shared contracts)`; `crates/engine/src/`; `crates/interface/src/native/`.

## Ordered implementation tasks

- [ ] **R0713.** **Deferred final specialist expansion.** Keep every row unbuilt until its own bounded feature scope, data rights, worker trust, reference problem and numerical/resource criteria are established after [stage 15](15-stage.md). This retained scope includes accurate physical process extensions to core manufacturing/civil geometry; those extensions cannot become an earlier geometry prerequisite.

- [ ] **R0714.** Implement the following rows in this order unless the owner explicitly reprioritizes them. Each is a separate feature slice with native authoring/setup, typed inputs, offline dependencies, solver/result adapter and reference validation. None is complete merely because [stage 19](19-stage.md)'s protocol could host it or [stage 11](11-stage.md) can draw/export its geometry.

- [ ] **21.T01.** Molecular/protein dynamics/minimization and a selected fresh microscopic study, supporting an independent laboratory result endpoint and optional explicit reusable-model export. Require full parameter coverage, ensemble/step/sample/replica/sampling/convergence evidence and a reference comparison. Add electronic structure, reaction and folding methods as distinct supported calculations, not side effects of a molecule edit. Use the same protocol/job/result services proved in [stage 17](17-stage.md), rather than another laboratory engine.

- [ ] **21.T02.** Nonlinear/flexible constitutive models and failure: select hyperelastic/viscoelastic/plastic/contact/fracture subsets individually, preserve history variables and validate transfer/restart/energy behavior.

- [ ] **21.T03.** Physical manufacturing/civil process extensions to [stage 11](11-stage.md)'s geometric profiles: select forming, thermal/process/material evolution, geotechnical response and related optimization only as named validated operations with measured inputs and reference outputs. Preserve sheet-metal bend assumptions, composite/lattice source, manufacturing tolerances and corridor/utility/geotechnical source identity; a successful geometric unfold, toolpath export or soil label does not establish physical process/ground-response accuracy. Keep equipment/tool execution separately authorized.

- [ ] **21.T04.** Thermal-structural one-way then two-way coupling: interface maps, conservation, substeps, convergence, checkpoint/rollback and a coupled reference case. Add fluids/acoustics/geotechnical domains with their own model cards and fixtures.

- [ ] **21.T05.** Electronic/circuit and device-material studies: compact-model/netlist workflow first, electrothermal mapping next, specialist field/device/fabrication models only with declared inputs and benchmarks.

- [ ] **21.T06.** Chemical/multiphase and living-material/tree studies: separate kinetics/equilibrium/transport from procedural geometry and physiological growth; include calibrated parameters and conservation/validity checks.

- [ ] **21.T07.** Nuclear-power-related transport/depletion/heat/thermal-hydraulic and later supported dynamic coupling: evaluated isotope data coverage, source provenance, statistical/convergence diagnostics and reference cases for each claimed scope.

- [ ] **21.T08.** Supported astronomical/dynamical/cosmological studies beyond [stage 05](05-stage.md)'s sparse scale visualization: versioned optional ephemerides, supported epoch/time conversions and moving-frame physical interpretation; named dynamics/cosmological models, reference comparisons and bounded local/aggregate fidelity. [stage 05](05-stage.md) already tests sparse observable-universe-sized addresses, distant simultaneous views and depth/resource behavior; extending actual scientific content must preserve those proofs.

## Worked contract example

- [ ] **21.X01.** Select physically meaningful exchanged quantities, signs, units and reference tolerances per coupling/model. This energy residual is one diagnostic, not a universal solver, sufficient convergence test or validation against reality. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn balanced_energy(input_j: f64, output_j: f64, stored_change_j: f64, tolerance_j: f64) -> bool {
      if ![input_j, output_j, stored_change_j, tolerance_j]
          .iter()
          .all(|v| v.is_finite())
          || tolerance_j < 0.0
      {
          return false;
      }
      (input_j - output_j - stored_change_j).abs() <= tolerance_j
  }

  fn main() {
      assert!(balanced_energy(100.0, 30.0, 70.0, 0.01));
      assert!(!balanced_energy(100.0, 30.0, 60.0, 0.01));
  }
  ```

## Architecture and task requirements

### Capability coverage and reusable workflow recipes

- [ ] **R0116.** **W08 — Further specialist systems** — **Shared primitives and thin specialization:** Domain profiles for circuits/chips, chemical processes, living/tree models, nuclear and astronomical calculations — **Minimum proof / later expansion:** Every [stage 21](21-stage.md) row retains named input/model/solver/validation requirements; hosting a plugin does not complete it

### Domain workflows as implementations of the primitives

- [ ] **R0367.** Chip/tree/nuclear/other study — **Authored/source support:** Domain graph/regions/fields with known inputs — **Specialized operation/constraint:** Domain-specific equations, constitutive laws and coupling — **Shared outputs and reuse:** Same job/result/provenance contracts, distinct validation and supported scope

- [ ] **R0368.** Astronomical composition — **Authored/source support:** Sparse nested frames, state/field/catalogue payloads — **Specialized operation/constraint:** Frame/time conversion and selected dynamics — **Shared outputs and reuse:** Local high-precision inspection, bounded residency and evidence-backed state

### Molecular, crystalline and smaller-scale authoring

- [ ] **R0375.** A microscopic job declares potential/force field and parameter coverage, charge/spin where needed, periodic boundaries, long-range treatment, integration step, constraints, ensemble/thermostat/barostat, seed and sampling/equilibration. Reject unparameterized species or units. [LAMMPS documents distinct unit systems](https://docs.lammps.org/units.html) and [periodic boundaries](https://docs.lammps.org/boundary.html); [OpenMM documents force-field setup and simulation choices](https://docs.openmm.org/latest/userguide/application/02_running_sims.html). These are solver candidates, not implicit new runtime dependencies.

- [ ] **R0377.** Population/distribution or quantum-field-like domain payloads can be added under the same typed field/operator contracts when a concrete study requires them. No molecular parser is claimed to model all subatomic physics.

### Electrical/electronic, chemical, living and other specialist models

- [ ] **R0452.** A chip/board combines geometry/material regions, circuits/connectivity and domain models. Netlist nodes/ports are logical connections, not mesh adjacency. Begin with a bounded circuit/thermal example and versioned compact device models; semiconductor transport, fabrication/doping and electromagnetic fields require separate adapters and inputs. A silicon lattice does not by itself predict an arbitrary chip. [ngspice documents its compact and mixed-level device models](https://ngspice.sourceforge.io/docs/ngspice-manual.pdf).

- [ ] **R0453.** Chemical processes require species, stoichiometry, phase/equilibrium/kinetic models and conserved element/mass/charge accounting as appropriate. Molecular dynamics is not automatically a long-timescale reaction process. Reactive media, fluids and phase changes declare their own constitutive/state model and numerical method.

- [ ] **R0454.** A tree can start as a procedural branching definition with instanced leaves and low-poly geometry. Mechanical response needs wood anisotropy, branch attachments, moisture and loads; growth needs a chosen physiological/environmental model and calibration. Geometry generation, biological growth and ecosystem simulation remain distinct operators connected by typed state, rather than one undocumented “tree physics” flag.

- [ ] **R0455.** Nuclear-power-related studies remain explicitly in scope: isotope-specific composition, evaluated interaction/decay data, transport/depletion, heat generation, thermal/hydraulic response and later supported kinetics/coupling. Use versioned domain inputs, separately validated adapters and reference benchmarks. [OpenMC documents cross-section and depletion data configuration](https://docs.openmc.org/en/stable/usersguide/data.html). First prove a bounded transport/heating case with input coverage and statistical uncertainty; adding geometry or chemistry must not imply reactor-system fidelity.

- [ ] **R0456.** Traffic, weather, geotechnics, acoustics and other domains use the same profiles only when their actual quantities/geometry/solver assumptions are specified. These capabilities stay on the roadmap with validation gates; they are not counted as delivered because an extension slot exists.

### Coupling and scale/fidelity changes

- [ ] **R0521.** Start with one-way mappings: a material derivation supplies a constitutive model; a thermal result supplies a specified temperature field to a structural run. Two-way coupling is a separate capability with exchange variables, signs, interface maps, clock synchronization, subcycling, convergence criteria, rollback/checkpoints and failure policy.

- [ ] **R0522.** Intensive values such as temperature need suitable interpolation; integrated forces/heat/mass transfers need appropriate conservation. Pressure maps must account for area/normals before claiming conserved resultant force. Compare the coupled balances and interface work, not only each solver's local residual. [preCICE documents consistent/conservative mappings](https://precice.org/configuration-mapping) and [explicit/implicit coupling](https://precice.org/configuration-coupling). [FMI defines exchange interfaces, but not the master co-simulation algorithm](https://fmi-standard.org/docs/3.0.1/); adopting it does not solve stability or orchestration.

- [ ] **R0523.** State projection when changing mesh/model includes conserved quantities, internal variables and initialization error. A display LOD change needs no physical projection. A solver refinement/coarsening does. If no validated transfer exists, restart a new run from a declared base; do not fabricate lost history. Reduced-order/surrogate models require training/calibration provenance, validity bounds and an out-of-domain policy just like other material models.

### Interoperability: choose by purpose

- [ ] **R1034.** Dynamic model exchange/co-simulation — **Candidate and documented role:** [FMI](https://fmi-standard.org/docs/main/) defines model exchange, co-simulation and scheduled-execution interfaces — **Proposed boundary in Lince:** Later isolated solver worker with resource limits, unit mappings and an explicit orchestration/time-step contract.

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Select one named domain row at a time with a real task, captured data rights, reference outputs and error/resource limits; compare isolated and mixed workspace load. Retain incomplete rows and their evidence separately, including useful authoring/result inspection when the expensive solver is absent.

- [ ] **R0577.** **[stage 21](21-stage.md) — deferred L0–L5, one named specialist slice at a time** — **Intention and defined deliverable:** Fresh microscopic, nonlinear, coupled, electronic, living, nuclear and astronomical capabilities — **Correctness/failure evidence and performance obligation:** Each row chooses its specimen/protocol/model/benchmark and supported operations before implementation. Sweep error/throughput/resources, prove independent result endpoints/optional export and regress core/hosting. Physical process extensions to manufacturing/civil models are selected here; core geometry/exchange already belongs to [stage 11](11-stage.md). An adapter seam never completes a science.

- [ ] Run the actual native/backend/persistence/transport path for every 21.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0723.** a genuine domain example shares K1–K8 and persistence/native tools, has its own evidence and limitations, and cannot silently extrapolate beyond its supported model. Keep unbuilt models explicitly deferred in this document.

- [ ] Advance only when **all 21.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
