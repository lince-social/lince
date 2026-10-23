# 18 — Additional rigid dynamics, joints and authored forces — deferred

Finish with a bounded hinged/rolling assembly and loose/mounted Sands governed by explicit additional body/joint/force settings. Existing navigation and Sand behavior remain usable, and shared physical state has one designated execution authority.

## Required workflow milestones

- [ ] **18.M01.** Configure supported body modes, mass/inertia provenance, contacts, joint limits and fixed-step pause/reset/step controls for a real assembly in the authored room/cave. Keep render/canonical/solver poses separately owned.

- [ ] **18.M02.** Compare two admitted force/clock settings with independent runtime state. Sleep only processes that can safely stop, wake affected bodies after valid support/force changes and report unsupported domain handoff/overload instead of silently changing laws.

- [ ] **18.M03.** Edit a material/collider/shape while active and apply a supported state transfer at the safe boundary. For an unsupported continuation keep last usable source/results, pause only affected evolution and offer explicit refresh/restart without silent history loss.

- [ ] **18.M04.** Interact with the same authoritative domain from permitted participants at different visual detail. A weak viewer sees allowed observations without running physics; stale writers/inputs and mere avatars/cameras cannot gain physical or spending authority.

- [ ] **18.M05.** Import a body with intrinsic definitions and inherited environment: apply destination gravity once, retain explicit overrides and keep temperature/damage/velocity separate from definition equality.

## Entry and scope

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

- [ ] **Real workload.** Use a hinged panel or rolling part in a furnished room/cave, modify support and compare an altered-force case while another participant observes. Measure steps/contacts/substeps/wake-ups, input latency, source replacement and idle cost; a large pile without editing/interaction/recovery is insufficient.

- [ ] **R0574.** **[stage 18](18-stage.md) — deferred L0/L2/L3/L4** — **Intention and defined deliverable:** Additional rigid-body dynamics/joints/altered forces with explicit domain ownership — **Correctness/failure evidence and performance obligation:** B11/B17/B21/B37/B39 live subsets: state transfer/restart/defer, safe support changes, sleep/wake, independent clocks and single-authority permitted interaction. Measure bodies/contacts/substeps/wake-ups and resources; visitor LOD never changes physics. Existing Sand/Area behavior already passed [stage 15](15-stage.md) and must remain usable.

- [ ] Run the actual native/backend/persistence/transport path for every 18.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0700.** a hinged/rolling assembly and loose/mounted Sands behave predictably in a room/cave, survive collider edits, and can be compared under different force settings without shared velocities or altered operational state.

- [ ] Advance only when **all 18.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
