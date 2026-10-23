# 15 — Core stabilization and workflow acceptance gate

Finish with the useful local/offline/hosted modeling workspace working as a whole before any new physical-material or scientific capability is scheduled. Completion requires observed user workflows and measured recovery/resource behavior, including the intended scale journey and authored landmarks.

## Required workflow milestones

- [ ] **15.M01.** From the ball in a saved place, travel outward to coarse globe/optional outer context and back; explicitly switch Orbit/Fly/Walk, walk the street with supported optional character/avatar presence, recognize the authored castle, find several favorite-place models in the overview, enter the building and use a mounted Sand.

- [ ] **15.M02.** Edit/simplify/reimport the authored place, batch-edit selected matching objects and modify an exact CAD part. Preserve dimensions, stable IDs, undo/reload, protected openings and authored-versus-source/physical calibration.

- [ ] **15.M03.** Repeat meaningful navigation/editing with an identified editor and restricted public visitor, a separately authenticated Organ source, cold/warm/offline data, caps/denials, slow transfer and host restart. Recover receipts/drafts without stale mutation replay or hidden authority changes.

- [ ] **15.M04.** Use an independent microscopic structure editor and the operational/civic workflow without loading a world or new solver. Existing Areas, gravity, motion cooling and support still work and idle work settles.

- [ ] **15.M05.** Pass the agreed mixed-workload budgets on named hardware or obtain an explicitly reviewed measured profile change. Leave no blocker in claimed core scope; absent deferred heating/material/dynamics features are neither advertised nor prerequisites.

## Entry and scope

- [ ] Confirm the preceding [stage 14](14-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 05 summary](05-stage.md#completion-summary); [stage 06 summary](06-stage.md#completion-summary); [stage 07 summary](07-stage.md#completion-summary); [stage 10 summary](10-stage.md#completion-summary); [stage 12 summary](12-stage.md#completion-summary); [stage 14 summary](14-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/xtask/`; `crates/interface/src/native/`; `crates/engine/src/`; `crates/transport/`; `stage-owned contract modules`.

## Ordered implementation tasks

- [ ] **R0670.** This is the mandatory boundary between core delivery and the deferred simulation tail. Stabilization verifies delivered behavior; it does not build omitted scientific features, rewrite proven contracts or run every conceivable test.

- [ ] **15.T01.** Run the applicable earlier native/backend checks on the unchanged named fixture/profile, correcting failures before proceeding: accepted geometry/identity, undo/reload/import variants, precise multi-scale navigation, low-poly room/cave appearance, working mounted input, existing Areas/independent targets/immunity, current Sand gravity/motion cooling and supported support replacement. Exercise B38 without geography and B40 selected-copy editing. Verify core use needs no property database, force field, thermodynamic state or new scientific worker.

- [ ] **15.T02.** Replay offline city/place use and the bounded hosted route with independent views, editor/public visitor, other-Organ Protein data, a denied source, changed permission, slow transfer, cap reached, disconnect/reconnect and host restart. Preserve receipts/drafts/last valid sources, honest partial states, no mutation replay and no implicit participant execution/control grant. Check existing operational scenarios remain isolated from accepted stock and live Actions.

- [ ] **15.T03.** Measure the mixed core workload against its stage-specific and host/device budgets, including cold/warm open, input/frame percentiles, queries/edits, transient old/new staging, disk/CPU/GPU/queue peaks, cancellation and last-demand release. Compare idle wake-ups and repeated travel history with the baseline. Fix regressions or record a specifically reviewed profile change with measured causes; no speculative simulation benchmark is an early gate.

- [ ] **15.T04.** Record the core capability/evidence matrix, remaining core blockers and the exact boundary for stages [16](16-stage.md)–[21](21-stage.md). Keep every incomplete requirement in this document. Distinguish existing supported physics from the newly deferred behavior in native capability/status labels and profile manifests; do not advertise physical accuracy from an appearance/geometry check. Freeze the stable contracts supported by evidence while allowing later compatible architectural refinement under the repository's no-backward-compatibility policy.

## Worked contract example

- [ ] **15.X01.** Populate evidence from actual named native/backend/transport replays and measured thresholds. Booleans/checkmarks produced by this example are not proof; retain reproducible inputs, outputs, screenshots where needed, timings and failure cases before changing stage status. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  struct WorkflowEvidence {
      replay_passed: bool,
      native_input_passed: bool,
      recovery_passed: bool,
      budgets_measured: bool,
      blockers: usize,
  }

  fn may_advance(evidence: &WorkflowEvidence) -> bool {
      evidence.replay_passed
          && evidence.native_input_passed
          && evidence.recovery_passed
          && evidence.budgets_measured
          && evidence.blockers == 0
  }

  fn main() {
      let pending = WorkflowEvidence {
          replay_passed: false,
          native_input_passed: false,
          recovery_passed: false,
          budgets_measured: false,
          blockers: 0,
      };
      assert!(!may_advance(&pending));
  }
  ```

## Architecture and task requirements

### The intended system and the first useful result

- [ ] **R0002.** **Delivery boundary, owner-confirmed 2026-10-21.** Stabilize the useful workspace before expanding its simulation of reality. Stages [01](01-stage.md)–[14](14-stage.md) deliver the core and [stage 15](15-stage.md) verifies that stabilization; stages [16](16-stage.md)–[21](21-stage.md) are explicitly deferred until that gate passes and their implementation scope is authorized. New material-property resolution/derivation, temperature/heating/cooling as thermodynamic state, thermal conduction, material-driven mass/inertia/contact changes, new rigid/joint/flexible/continuum physics, microscopic experiments and coupled scientific models belong only to that tail. Existing implemented Areas/influence, Sand gravity, motion cooling/sleep and terrain support remain functional and receive integration/regression fixes throughout. Existing motion cooling reduces unnecessary dynamics work; it does not calculate temperature. No current capability is removed or newly disabled by this ordering.

- [ ] **R0003.** Before [stage 15](15-stage.md), imported material/behavior definitions can be preserved, referenced, deduplicated and disclosed under existing rights; native tools can edit appearance, names and supported geometry. These actions do not resolve missing physical properties, predict heat or instantiate a new physical solver. Geometry validation, CAD constraint solving, meshing/simplification, bounded Walk support queries and safe replacement of existing support/colliders are core modeling/navigation work. Their mathematical algorithms do not imply an accurate material/physical simulation. Keep only the shared contract fields exercised by a core fixture; detailed future science sections preserve requirements without requiring their implementation, dataset selection, worker installation or runtime allocation early.

### Confirmed intent and context recovery

- [ ] **R0028.** **C17 — Stabilization before additional simulation.** Existing implemented Areas, Sand gravity/motion cooling and support stay integrated in every applicable core regression. Stages [01](01-stage.md)–[15](15-stage.md) do not depend on new physical material evaluation, heating/thermal evolution, additional rigid/flexible/joint solvers or scientific experiments. Core geometry, appearance, definition transport, precise authoring and structural microscopic editing remain usable independently. Stages [16](16-stage.md)–[21](21-stage.md) retain all new material/science/physics/recording capabilities as deferred final stages. References to those capabilities in a kernel/hosting contract, fixture catalogue or workflow description cannot pull them into an earlier gate.

### Proposed defaults for refinement

- [ ] **R0052.** Delivery order — **Proposed default:** Shared foundations → early scene-independent scale/hosting proof → offline locality, modeling/CAD, structural micro editing and operations → [stage 15](15-stage.md) stabilization → stages [16](16-stage.md)–[21](21-stage.md) deferred materials/laboratory/new physics/science — **When it may change:** Owner-confirmed 2026-10-21 priority supersedes the previous immediate paired-workflow schedule; every retained feature still has a stage

### Owned fixture specifications

- [ ] **R0725.** The canonical fixture specifications retain full eventual proof obligations; use the ownership links in [00-General.md](00-General.md#fixture-ownership) and the current declared subset. Core stages stages [01](01-stage.md)–[15](15-stage.md) and deferred stages [16](16-stage.md)–[21](21-stage.md) retain separate completion obligations. A B-number reference in an early stage selects only its stated core subset; it never requires the entire future scientific fixture. No scientific dataset/solver needs to be acquired merely to finish [stage 01](01-stage.md) or a hosted visit.

### Fixture phases and the stabilization boundary

- [ ] **R0766.** B01–B06, B10, B13–B15, B19/B20, B22–B36, B38/B40 — **Core proof through [stage 15](15-stage.md):** Their stage-specific identity/geometry/query/operation/hosting subsets, with no new physical solver. B10 assembly mates are geometric; B13 addresses do not require N-body dynamics. Existing Sand/Area/support behavior remains a core regression. — **Deferred proof in stages [16](16-stage.md)–[21](21-stage.md):** Only later physical/material/solver claims appended to these fixtures, such as mass-derived consequences, physically evolving assemblies or scientific worker failures

- [ ] **R0774.** No row can mark a deferred fixture complete from its core metadata subset. The [stage 15](15-stage.md) evidence matrix identifies the actual completed subset and remaining tail explicitly; all affected core/hosting/operational intentions continue as regressions when science is introduced.

### Verification obligations

- [ ] **R0813.** **Phase rule.** Apply these checks to delivered capabilities, using the stage subset requirements and [fixture ownership](00-General.md#fixture-ownership). Stages [01](01-stage.md)–[15](15-stage.md) verify existing physics integration and actual geometry/streaming/hosting/operations. They do not implement or pass new constitutive laws, heating, numerical experiments or multiple scientific domains. The retained scientific requirements below become gates only with stages [16](16-stage.md)–[21](21-stage.md); early profiling uses real core work, not a placeholder physics benchmark.

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0848.** **Q21 — Stabilize before additional physical simulation.** stages [01](01-stage.md)–[15](15-stage.md) deliver and stabilize workspace/scale/modeling/hosting/operational capabilities and existing Areas, gravity and motion cooling. Stages [16](16-stage.md)–[21](21-stage.md) retain all new physical-material evaluation/derivation, heating/thermal behavior, further rigid/flexible/joint physics, laboratory experiments, optional calculated-state recording and accurate specialist simulation as explicitly deferred final stages. Passive definitions/appearance and geometry/navigation are not physical accuracy. Earlier scope, fixtures or descriptive sections cannot override this order.

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Replay the actual personal-place and shared-workspace stories while operational queries and a structure edit remain usable. Increase real indexed/instanced content within the frozen dataset/profile and prove select/edit/save/revisit/recover actions under the cap; passing a 30,000-square renderer test alone cannot advance this gate.

- [ ] **R0571.** **[stage 15](15-stage.md) — L0–L5 core stabilization** — **Intention and defined deliverable:** Resolve core regressions and prove a useful offline/local/hosted modeling workspace before numerical expansion — **Correctness/failure evidence and performance obligation:** Replay stage-specific B01–B40 core subsets below, appearance/input review and mixed workloads on named hardware. Verify old Areas/gravity/motion cooling, precision/identity/undo, partial/offline closure, native permissions, guest/Organ isolation, recovery and measured budget/resource release. No missing deferred material/thermal/physics feature blocks this gate, and no stub counts as delivered.

- [ ] Run the actual native/backend/persistence/transport path for every 15.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0675.** the intended local/offline/hosted workspace is usable and measured, with no unresolved blocker in its claimed core scope and no dependence on new material/thermal/scientific execution. Only then may an authorized stages [16](16-stage.md)–[21](21-stage.md) slice begin. All deferred features remain unbuilt required scope; core stabilization neither completes nor removes them.

- [ ] Advance only when **all 15.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
