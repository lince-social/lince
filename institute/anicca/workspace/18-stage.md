# 18 — Additional rigid dynamics, joints and authored forces — deferred

Finish with a bounded hinged/rolling assembly and loose/mounted Sands governed by explicit additional body/joint/force settings. Existing navigation and Sand behavior remain usable, and shared physical state has one designated execution authority.

## Required workflow milestones

- [ ] **18.M01.** Configure supported body modes, mass/inertia provenance, contacts, joint limits and fixed-step pause/reset/step controls for a real assembly in the authored room/cave. Keep render/canonical/solver poses separately owned.

- [ ] **18.M02.** Compare two admitted force/clock settings with independent runtime state. Sleep only processes that can safely stop, wake affected bodies after valid support/force changes and report unsupported domain handoff/overload instead of silently changing laws.

- [ ] **18.M03.** Edit a material/collider/shape while active and apply a supported state transfer at the safe boundary. For an unsupported continuation keep last usable source/results, pause only affected evolution and offer explicit refresh/restart without silent history loss.

- [ ] **18.M04.** Interact with the same authoritative domain from permitted participants at different visual detail. A weak viewer sees allowed observations without running physics; stale writers/inputs and mere avatars/cameras cannot gain physical or spending authority.

- [ ] **18.M05.** Import a body with intrinsic definitions and inherited environment: apply destination gravity once, retain explicit overrides and keep temperature/damage/velocity separate from definition equality.

- [ ] **18.M06.** Configure a bounded articulated mechanism/body from B41 with explicit dynamic joint/contact/mass data and compare it with its authored kinematic pose. Keep one authority for the physical pose, preserve separate visual detail and safely enter/leave the supported physical mode. A visible participant avatar remains nonphysical unless separately admitted; muscle activation and tissue/cloth/hair laws are not implied.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

- [ ] Confirm the preceding [stage 17](17-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 15 summary](15-stage.md#completion-summary); [stage 16 summary](16-stage.md#completion-summary); [stage 17 summary](17-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Keep this stage deferred until the [core stabilization gate](15-stage.md#advance-gate) passes and this numerical slice has implementation authorization. Capture actual inputs, reference/error/quantity/resource limits, data rights, worker trust/location and payer before dispatch; regress affected stable-core workflows.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/interface/src/native/topology/physics.rs`; `crates/workspace_model/ (proposed domain contracts)`; `crates/engine/src/`; `selected isolated runtime/worker boundary`.

## Ordered implementation tasks

- [ ] **R0694.** **Deferred new physics after [stage 15](15-stage.md) stabilization and relevant [stage 16](16-stage.md) material contracts.** Existing Sand gravity, motion cooling, Areas and support already work in the core; this stage expands supported body/joint/force and scientific-state behavior rather than making those old paths wait for replacement.

- [ ] **18.T01.** Extend existing Avian-based interaction with explicit body modes, mass/inertia provenance, contact parameters, joints/limits, fixed-step/substep controls and pause/reset/step. Keep render/canonical/solver transforms independently owned.

- [ ] **18.T02.** Reuse Rules, Signals, Areas and bindings for scoped behavior; add named gravity/force model overrides with stored units/assumptions. Preserve ordinary Sand cooling and wake on relevant support/force changes.

- [ ] **18.T03.** Implement declared sleep/unload policies and domain boundaries; independent clocks use isolated runtime state, not collision masks alone. Add a second comparison domain only after the first passes the bounded resource fixture.

- [ ] **18.T04.** Prove E5/B17/B21 model-adoption behavior on an affected active domain: transfer supported state at a step boundary, restart, defer to the next run or retain the pinned prior model according to the selected policy. Display that outcome; never silently change mass/inertia/history or stop unrelated evolving processes.

- [ ] **18.T05.** Complete B37 for shared rigid/Area interaction: one designated domain authority and fenced run epoch, permitted commands at the supported step boundary, bounded timestamped observations and independent participant visual detail. Show activity location/payer and distinguish observe/interact/start/cancel/compute grants. Test missing client detail, stale input, authority restart, source/collider replacement, avatar presence versus physical body, denied camera-bound effects and separately admitted comparison runs. Preserve the initial cached-view/draft policy on host loss; no visitor automatically takes over physics. Complete B39's live gravity/environment case: an imported body brings its material/behavior definitions, binds inherited gravity once to the destination setting, preserves intrinsic properties and keeps object/run state separate. Verify explicit local overrides and lifecycle pause/retention; Q19/A02 uses the agreed last-usable-version/pending-candidate recovery, pauses unsupported affected evolution safely and offers refresh/restart without a new mandatory editor mode or silent history loss.

- [ ] **18.T06.** Extend stage 11's articulation with explicit rig/source-to-physical-body/joint maps for the selected B41 subset, mass/inertia provenance, motor/actuator parameters and tested state transfer. Resolve animation, kinematic targets, joint motors and dynamic poses through one declared authority/composition order; do not let a render clip overwrite a solver step or apply root motion twice. Prove joint limits, contact, unit/frame transforms, rebind/source changes, restart/host epochs and weak-viewer behavior independently of detailed anatomy. Physical locomotion/control is separately validated; authored walking/flying remains available without it.

- [ ] Recover [stage 11's articulation handoff](11-stage.md#completion-summary) for 18.T06 and open only its required rig/pose/control mapping details. A physical adapter extends the existing authored model instead of replacing its editor or creating a second joint definition.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Admit a single physical domain.** Select supported body modes, mass/inertia evidence, contacts, joints/forces and fixed-step clock controls for a real authored assembly. Separate canonical/render/physical poses, record writer epoch and expose pause/reset/step without changing existing Sand/Area semantics.

- [ ] **Cycle B — Transfer state and compare settings.** Validate supported shape/material/support edits at safe boundaries, preserve last usable results and pause only affected evolution when transfer is unsupported. Compare independent force/clock settings, sleep/wake and destination environment inheritance without duplicated gravity.

- [ ] **Cycle C — Share and articulate safely.** Reuse result/interaction grants for weak observers and authorized participants at different visual detail. Map B41's authored joints to admitted physical bodies explicitly, keep one pose/root writer and prove bounded overload, stale-input rejection and core responsiveness.

- [ ] Preserve these contracts after authorization; hinged/rolling components and an articulated mechanism are example recipes over built-in physical-domain and state-transfer capabilities.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **18.W01 — Operate a physical assembly and compare settings** | Built-in dynamics/domain/clock tools plus room assembly recipe; 18.M01/18.M02. Pause/step a hinge or rolling part, compare force settings and observe sleep/wake. | Physical mode, active domain, clock and overload/paused state are understandable. | Chosen reference/constraint/contact checks pass; independent states do not leak; count bodies/contacts/substeps/wake-ups and measure step/input/idle/peak costs within admitted limits. |
| **18.W02 — Edit or import without corrupting evolving state** | Built-in state-transfer/environment binding; 18.M03/18.M05. Change a collider/material, import definitions and continue or explicitly restart affected evolution. | Transfer support, retained results and refresh/restart scope are clear. | Last usable source survives unsupported transfer; gravity binds once; intrinsic definitions and velocity/temperature/damage stay distinct; measure safe-boundary replacement, interruption and staging peaks. |
| **18.W03 — Share authoritative articulated interaction** | Built-in participation/pose authority plus B41 recipe; 18.M04/18.M06. Observer watches, permitted participant interacts and an authored mechanism enters/leaves physical mode. | Authored versus dynamic control, observation-only access and visual detail are distinguishable. | One accepted writer/epoch and one root-motion owner; stale/denied inputs = 0; display LOD/presence cannot alter dynamics; measure observation bytes, admitted solver work and core responsiveness. |

- [ ] **Checks to select.** Unit/reference tests verify supported integration/joint/force cases, units, destination binding and sleep/wake conditions. Integration/fault tests cover edit transfer, epoch loss, delayed input, unsupported handoff, contact overload and separate clocks. Native replay configures the authored assembly, steps/edits it and switches articulated control while a low-budget visitor observes; benchmark active and settled work separately.

- [ ] **Earlier-work repair.** Retain 15.W01–15.W05 core, 11.W03/11.W05 authored articulation and 17.W03 authority/lifetime. Authoring/kinematic walking remains usable when a physical domain is paused or absent; a new dynamics failure is repaired before later numerical work depends on it.

## Concrete acceptance replay

- [ ] **18.P01 — One admitted physical domain first; 18.T01–18.T03 / 18.W01.** After authorization, configure the bounded hinge/rolling assembly from explicit body/mass/inertia/contact/force inputs. Pause/step/reset, compare the chosen reference, reach supported limits, sleep and wake after a force/support change. Add the second clock/force comparison only after the first passes. Compare independent state/step/constraint/contact behavior; count bodies, contacts, substeps and wake-ups with active/settled CPU/input costs. Review domain/clock/overload controls while preserving existing 01.W01 Sand motion cooling and 05.W02 kinematic navigation.

- [ ] **18.P02 — Editing an evolving body has a declared outcome; 18.T04/18.T05 / 18.W02.** Edit/import a supported collider/material/shape while active, exercise valid transfer at the boundary and an unsupported continuation requiring affected pause/refresh. Check the fixture's source/state mapping, retained last usable version, destination gravity applied once and explicit override behavior. Measure interruption, state-transfer/replacement latency and old-new peaks; review the accepted/pending/restart distinction. Preserve 16.W01 material scope and 17.W01 captured adoption without resetting unrelated evolution or accepted history.

- [ ] **18.P03 — Articulation and multiplayer have one writer; 18.T05/18.T06 / 18.W03.** Map the authored B41 mechanism to admitted physical bodies, switch supported control mode and let a permitted participant interact while a weak viewer uses coarser visuals. Inject stale input/epochs, authority restart, denied camera-bound influence and delayed observations. Verify one pose/root authority, no double root motion and no physics/payer grant from mere presence or observation. Record step/observation bytes, constraints and mixed input/frame/peak costs; review actual mode transitions while retaining 11.W03 authored posing and 17.W03 lifetime rules.

## Worked contract example

- [ ] **18.X01.** Fence commands/completions/observations through real run/source/authority admission. This epoch check cannot elect a replacement writer or establish exclusion across disconnected authorities; require the explicit supported authority-transfer protocol. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  #[derive(Clone, Copy)]
  struct Writer {
      authority: u64,
      epoch: u64,
  }

  fn current_writer(candidate: Writer, admitted: Writer) -> bool {
      candidate.authority == admitted.authority && candidate.epoch == admitted.epoch
  }

  fn main() {
      let admitted = Writer {
          authority: 7,
          epoch: 4,
      };
      assert!(current_writer(admitted, admitted));
      assert!(!current_writer(
          Writer {
              epoch: 3,
              ..admitted
          },
          admitted
      ));
      assert!(!current_writer(
          Writer {
              authority: 8,
              ..admitted
          },
          admitted
      ));
  }
  ```

## Architecture and task requirements

### Proposed defaults for refinement

- [ ] **R0061.** Scientific runtime — **Proposed default:** Deferred stages [17](17-stage.md)–[21](21-stage.md): explicit bounded jobs, installed/versioned adapters and one admitted physical domain first — **When it may change:** Core geometry/streaming uses only its needed bounded jobs; multiple scientific domains require later isolation/resource tests

### Capability coverage and reusable workflow recipes

- [ ] **R0113.** **W05 — Motion and physical studies** — **Shared primitives and thin specialization:** Domain state/clock, laws/ports/boundaries, resource leases and result mappings — **Minimum proof / later expansion:** Existing gravity/motion cooling/Areas regress throughout; new B11/B12/B21 rigid/flexible/thermal studies and altered scientific laws are deferred stages [18](18-stage.md)–[21](21-stage.md)

### K8. Domains, state, observations and scenarios

- [ ] **R0192.** Core delivery uses existing activity/state behavior and only the inert clock/revision/status fields exercised by its geometry/streaming/operational fixture. Additional physical-domain ownership, material/thermal history, scientific scenarios and numerical state transfer below are deferred stages [17](17-stage.md)–[21](21-stage.md); optional calculated-state recording is [stage 20](20-stage.md). These future contracts must not turn into a new runtime prerequisite for core authoring or hosted entry.

- [ ] **R0193.** A **domain** owns the active simulation support, units, clock, boundary conditions, state variables, constitutive models, stepping policy and authority/capabilities. An occurrence may participate in different domains through explicit maps. Shared authored definitions do not imply shared mutable velocities, temperatures, random generators or solver buffers.

- [ ] **R0194.** Terminology for this plan: **simulation** means computing changes over time from selected behavior rules. Existing falling Sands use such computation; a rolling assembly or moving conveyor is a further example when its behavior is implemented; it need not be a separate application or a special experiment screen. A stationary model can simply be viewed/edited without starting every possible calculation. A deliberate scientific study additionally pins inputs, conditions, method and results. The interface should describe the actual action with controls such as Physics, Run, Pause and Record and useful status, rather than require users to understand the broad word simulation before normal editing.

- [ ] **R0195.** Geometry means an object's shape; a mesh is one representation using vertices/edges/faces or a domain-specific numerical grid. Display, contact/support and scientific calculation can use different derived representations of the same accepted shape. Editing a shape can make a previous derived product or calculation outdated. This is a validity/update problem, not permission for an application crash. Ordinary supported live edits update automatically through bounded jobs and safe boundaries, preserving usable support and clear pending status. The accepted A02/Q19 exception retains the last usable accepted version/result while preparing the edited candidate; pause affected evolution if continuing is unsafe/unsupported and offer refresh/restart with pinned input/state details. Do not silently restart or create a new scenario/mesh dialog for every normal edit. Core geometry/support recovery is implemented at the stage needing it; scientific state transfer is deferred with its solver.

- [ ] **R0196.** **Recording** is separate from calculating live behavior. It retains selected motion/state/events for later inspection, such as scrubbing back to a box falling off a conveyor. The owner accepts optional Record for chosen objects/activities/fields, a bounded interval and visible storage budget, using compatible checkpoints/events plus selected observations; dense state/trajectory capture is explicit and costed. Declare what can be replayed, interpolated, restarted or merely inspected. A recording is not a new physical prediction, and playback cannot issue live commands. [stage 20](20-stage.md) delivers this capability after stabilization; saving/undo/receipts and essential scientific input/result evidence do not imply automatic whole-workspace recording or persistence of every integration step. See A06/Q20.

- [ ] **R0197.** Separate four things throughout persistence, UI and APIs: authored/captured representation; observations with uncertainty; accepted operational state; and conditional scenario results. Observation ingestion does not automatically edit geometry, stock or ownership. Scenario outputs never become live commands or accepted facts without an explicit authorized application operation.

- [ ] **R0198.** A scenario pins a composition base, input fields/materials, initial/boundary conditions, domain/solver versions, seed, time range, resource budget and results. Bounded time-indexed poses/fields/media reference stable objects and frames separately from static geometry. Independent views can select different bases and playback positions. Only the shared data/state contracts are scheduled here; specialized reconstruction is a later adapter with its own plan and proofs.

- [ ] **R0199.** State streams can also reference topology revisions or typed birth/death/connectivity events when a supported process changes structure. IDs are not recycled within a run. A molecular reaction, fracture or construction sequence must not overwrite its initial graph or silently mutate accepted source geometry. Keep event checkpoints and mappings to the base so bounded seeking/replay does not require replaying an entire history. Time interpolation declares valid continuous intervals; it never blends across an unmodeled topology change or missing observation interval.

- [ ] **R0200.** The initial domain can use one isolated local runtime. Independent clocks or physics configuration later require separately owned solver state; collision masks in a single global Avian world are not enough. A worker or isolated Bevy sub-application per active physical domain is a candidate, with a small active-domain limit and immutable snapshots sent to views. Do not instantiate an engine/world per model object.

- [ ] **R0201.** For shared use, that runtime has the designated execution authority in H7; visitors subscribe to permitted observations or request separately granted inputs. Joining or changing visual detail does not instantiate a local copy of the solver. Host/worker recovery uses run/authority epochs and explicit checkpoint status, with no automatic new writer on disconnect.

### Low-poly appearance, simplification and physical use

- [ ] **R0332.** Render mesh, static collision mesh, dynamic convex/compound proxy and measurement source have independent budgets and recipes. A very thin wall must remain a boundary even when visually subpixel. Closed dynamic bodies need valid mass/inertia from a supported solid/material model or explicit user values; an open scan has no justified automatic volume. Never use appearance splats as a solid collider.

### Motion, flexible bodies and scientific execution

- [ ] **R0509.** **Existing behavior now; additional simulation deferred.** Before [stage 15](15-stage.md), reuse and stabilize implemented gravity, motion cooling/sleep, terrain support and Areas, plus the bounded Walk/support and geometry replacement needed by the core. The new mass/inertia/material laws, joints/body modes, thermodynamic state/heating, flexible/continuum equations, scientific clocks/transfer and coupling below are stages [16](16-stage.md)–[21](21-stage.md) requirements. This descriptive section is not an instruction to build them during the foundations or stabilize them before they exist.

### Interactive dynamics

- [ ] **R0510.** Reuse existing gravity, terrain support and cooling for ordinary unmounted Sands. Extend through explicit body modes: layout-only, static, kinematic, rigid and supported deformable. Layout attraction is an interaction rule; scientific forces have physical units and a named model. Default UI mass/drag can be practical without being reported as measured material physics.

- [ ] **R0511.** Local rigid bodies have collision shapes, mass/inertia source, pose/velocity, contact parameters, joints, integration/substep settings and sleep/wake thresholds. Fixed-step simulation is decoupled from render frequency; cap catch-up and show throttled time rather than silently taking an unstable giant step. Fast/thin geometry needs a tested continuous-collision or substep policy. Failed collision generation leaves the previous support in place.

- [ ] **R0512.** A relevant terrain/collider or existing influence change wakes affected bodies under existing supported semantics. New altered-gravity, material/contact and joint-triggered transitions are added with deferred [stage 18](18-stage.md), not as an early prerequisite. Cooling uses settled motion/force thresholds and a dwell period; it cannot erase external force or terminate ongoing thermal/chemical evolution. “Time castle” behavior can be reused where its actual semantics fit, rather than adding another independent decay scheduler.

- [ ] **R0513.** For distant/unloaded regions, select frozen, analytic, reduced-order or recompute-on-activation policy. Each declares carried state and supported time range. Crossing domain boundaries needs handoff of identity, pose, velocity and appropriate conserved state at a known instant; never simulate the same body in two domains. First implementation can stop at a boundary with a visible unsupported handoff instead of a silent teleport. Large-world gravity is not automatically Newtonian N-body gravity; it is a selected domain model.

- [ ] **R0514.** Shared physical participation follows H7: a designated authority owns each domain/run, accepts authorized interaction inputs and publishes permitted timestamped observations. A participant's local Walk camera and noncolliding avatar are not automatically canonical rigid bodies. Missing local detail cannot change shared physical state, and remote interpolation remains presentation rather than scientific evidence. [stage 18](18-stage.md) proves this extension after [stage 05](05-stage.md)'s bounded navigation controller.

### Execution contracts E1–E8

- [ ] **R0545.** **E5 — Independent clocks, cadence and numerical fidelity.** Render, control, contact dynamics, material evolution, experiment steps and operational events have appropriate clocks/wake conditions. Use dirty/event demand for unchanged metadata, fixed/adaptive steps where the selected numerical method supports them, and analytic/reduced propagation only within an explicit validity contract. Quiescence means the relevant model's state can safely stop; no global sleep flag may stop heat, reactions or other evolving processes. Domain boundaries own physical state handoff, history and conservation checks. Choose the algorithm and admissible error before micro-optimizing it; neighbor lists, sparse solvers and orbital propagation are different specialized work plans.

### Fixture phases and the stabilization boundary

- [ ] **R0769.** B11/B12/B21 — **Core proof through [stage 15](15-stage.md):** Preserve already implemented Sand gravity/motion cooling/Areas and support integration using bounded core tests. Core scheduler fairness is proved on real geometry/streaming jobs; it does not require multiple scientific domains. — **Deferred proof in stages [16](16-stage.md)–[21](21-stage.md):** [stage 18](18-stage.md) additional rigid/joint/force models and independent physical clocks; [stage 19](19-stage.md) flexible/elastic/thermal/heating/convergence; later state/coupling cases in [stage 21](21-stage.md)

### Deferred numerical resource and validation profile: stages 16–21

- [ ] **R0808.** These proposals are retained for the final simulation tail after [stage 15](15-stage.md). They allocate no workers/domains during core delivery, do not expand the core memory allowance automatically and are refined at the selected study with actual inputs/reference results. Preserve existing Sand/Area resource behavior during stabilization rather than applying a new numerical-domain architecture prematurely.

- [ ] **R0809.** Additional physical runtime — **Proposed starting value:** One admitted local domain first; at most two for the explicit isolation proof — **Enforcement/evidence:** Select body/contact/substep counts from [stage 18](18-stage.md) fixtures; overload reports throttled/paused state rather than silently changing physical laws

- [ ] **R0810.** Scientific worker default — **Proposed starting value:** Two CPU cores, 2 GiB RAM and an explicit wall-time/output cap per selected study — **Enforcement/evidence:** stages [16](16-stage.md)–[21](21-stage.md) select actual data/algorithm limits; CPU/GPU/remote allocations, sample/replica counts and continuing background owner are explicit and metered

- [ ] **R0811.** Initial continuum error — **Proposed starting value:** Proposed ≤1% relative error for selected analytic elasticity/conduction quantities after refinement in their valid regimes — **Enforcement/evidence:** [stage 19](19-stage.md) records convergence, balance/unit/identity checks and excluded singular neighborhoods; not a claim of general physical accuracy

- [ ] **R0812.** Optional recorded history — **Proposed starting value:** Explicit subset/cadence/window/byte budget selected before Record; no unbounded default history — **Enforcement/evidence:** [stage 20](20-stage.md) measures encoded/scratch/index/seek costs and labels interpolation/gaps; scientific reproducibility evidence and current-state saves remain separate

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0736.** B11 — **Fixture:** Loose Sands, a hinged assembly and altered gravity in two comparison domains — **Proof required:** Fixed stepping, sleep/wake, support edit, state isolation and unload policy

### Owned fixture specifications

- [ ] **R0746.** B21 — **Fixture:** A normal interactive view beside independent running scientific/control domains — **Proof required:** Fair resource admission, independent clocks/wake conditions, idle demand release, no render-rate blanket stepping, and paused/restarted/deferred model replacement without false conservation

## Workflow, correctness and performance evidence

- [ ] Instantiate the local **18.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Use a hinged panel or rolling part in a furnished room/cave, modify support and compare an altered-force case while another participant observes. Measure steps/contacts/substeps/wake-ups, input latency, source replacement and idle cost; a large pile without editing/interaction/recovery is insufficient.

- [ ] **R0574.** **[stage 18](18-stage.md) — deferred L0/L2/L3/L4** — **Intention and defined deliverable:** Additional rigid-body dynamics/joints/altered forces with explicit domain ownership — **Correctness/failure evidence and performance obligation:** B11/B17/B21/B37/B39 live subsets: state transfer/restart/defer, safe support changes, sleep/wake, independent clocks and single-authority permitted interaction. Measure bodies/contacts/substeps/wake-ups and resources; visitor LOD never changes physics. Existing Sand/Area behavior already passed [stage 15](15-stage.md) and must remain usable.

- [ ] Run the actual native/backend/persistence/transport path for every 18.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] Verify every local **18.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0700.** a hinged/rolling assembly and loose/mounted Sands behave predictably in a room/cave, survive collider edits, and can be compared under different force settings without shared velocities or altered operational state.

- [ ] Advance only when **all 18.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **18.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

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
