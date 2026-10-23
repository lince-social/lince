# 20 — Optional scoped recording and playback — deferred

Finish able to record a selected activity for a bounded interval and inspect it later without changing accepted live state. Capture coverage, source/clock identity, missing intervals and actual storage cost remain explicit.

## Required workflow milestones

- [ ] **20.M01.** Select a rolling/hinged assembly or supported result fields, choose Record for a bounded interval and see estimated/actual storage. No whole-workspace or every-step capture starts automatically.

- [ ] **20.M02.** Scrub, play/pause, compare and export the recorded subset after closing or going offline. Distinguish actual observations from interpolation, gaps and unrecorded private fields; noncaptured history is unavailable.

- [ ] **20.M03.** Keep capture, save/undo, essential scientific evidence and live execution separate. Replay never emits current Area crossings/operational Actions or pretends to predict a changed room/model.

- [ ] **20.M04.** Cancel/partially capture, revoke sharing, disconnect a viewer or close the owner view under the admitted lifetime policy. Bound retention/seek/decode and release demand without starving stable workspace work.

## Entry and scope

- [ ] Confirm the preceding [stage 19](19-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 17 summary](17-stage.md#completion-summary); [stage 18 summary](18-stage.md#completion-summary); [stage 19 summary](19-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Keep this stage deferred until the [core stabilization gate](15-stage.md#advance-gate) passes and this numerical slice has implementation authorization. Capture actual inputs, reference/error/quantity/resource limits, data rights, worker trust/location and payer before dispatch; regress affected stable-core workflows.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed time-indexed artifacts)`; `crates/engine/src/`; `crates/transport/`; `crates/interface/src/native/`.

## Ordered implementation tasks

- [ ] **R0707.** This implements accepted Q20/A06 after a real admitted numerical activity/result exists. Saving/undo/accepted receipts and essential experiment evidence were already delivered by their earlier stages; they do not wait for this optional playback capability.

- [ ] **20.T01.** Provide an optional native Record action for selected objects/activities/fields with a named clock/time interval, input/source/run identity, capture cadence/coverage, output/sample/window limits and a visible storage estimate/actual counter. Default to admitted checkpoints/events plus selected observations; dense state/trajectory capture is a separate explicit selection. Never record the whole workspace automatically.

- [ ] **20.T02.** Retain immutable chunked/indexed observations and compatible checkpoints under existing artifact/demand/authority contracts. Bound seek/decode/retention/scratch, distinguish missing intervals and interpolation from actual samples, and retain exact units/frame/clock/source identities. Checkpoints claim only the adapter's supported restore; event logs do not imply deterministic or bitwise scientific recomputation.

- [ ] **20.T03.** Add native scrub/play/pause/compare/export for the recorded subset. Replay is read-only presentation with no live Area crossings, operational Actions or altered current simulation. To rerun a changed model, request a separate admitted calculation with captured inputs; do not relabel replay as a counterfactual prediction. Keep local playback independent of shared run authority.

- [ ] **20.T04.** Extend B18/B21/B37 only for the supported capture scope: bounded retention/seek, owner close/Keep running, cancelled/partial capture, changed grants, host loss, offline inspection and explicit publication. Private/unrecorded fields remain unavailable. Record actual storage/throughput/latency and prove capture cannot starve core editing/navigation or leave an unauthorized background lease.

## Worked contract example

- [ ] **20.X01.** Reserve source/index/checkpoint/codec/transient bytes in addition to this selected-observation payload bound. The cadence is an explicit capture choice, not a new solver step rate; dense capture and restartability need separate supported contracts. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn capture_upper_bound_bytes(
      objects: u64,
      samples_per_second: u64,
      seconds: u64,
      bytes_per_sample: u64,
  ) -> Option<u64> {
      objects
          .checked_mul(samples_per_second)?
          .checked_mul(seconds)?
          .checked_mul(bytes_per_sample)
  }

  fn main() {
      assert_eq!(capture_upper_bound_bytes(8, 20, 30, 64), Some(307_200));
      assert_eq!(capture_upper_bound_bytes(u64::MAX, 20, 30, 64), None);
  }
  ```

## Architecture and task requirements

### Proposed defaults for refinement

- [ ] **R0067.** Recording calculated behavior — **Proposed default:** Deferred [stage 20](20-stage.md): optional Record for selected objects/activities/fields, a bounded interval and visible storage budget; checkpoints/events and selected observations, dense capture only by explicit choice — **When it may change:** Saving/undo/receipts and current live state remain distinct; mandatory scientific evidence is retained without automatic whole-workspace recording

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0847.** **Q20 — Optional scoped recording.** Provide Record for chosen objects/activities/fields, a bounded interval and visible storage budget. Prefer compatible checkpoints/events and selected observations; dense capture is explicit. Distinguish saved current state, authored undo, essential evidence, live execution and playback. [stage 20](20-stage.md) delivers the optional replay capability after core stabilization; nothing automatically records the whole workspace or every integration step.

### Accepted architecture/detail choices and stage-specific evidence

- [ ] **R0861.** **A06 — Agreed: optional recording of a selected activity/subset.** Record chosen objects/activities/fields with a bounded interval and visible storage budget. Compatible checkpoints/events plus selected observations are the default approach; dense capture requires an explicit costed selection. Saving a model/current state, undoing authored edits, retaining essential experiment evidence, executing live physics and replaying recorded motion are distinct capabilities. [stage 20](20-stage.md) implements optional playback/scrubbing; [stage 17](17-stage.md) retains essential scientific evidence without automatic dense or whole-workspace capture. See Q20/Q21; no recording feature blocks core [stage 15](15-stage.md) stabilization.

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Record an explicit 30-second bounded assembly activity, navigate/edit unrelated workspace content, reopen the recording and inspect the selected event. Measure capture/encoded/index/scratch/seek cost and missing/partial/revoked cases, not only the ability to serialize frames.

- [ ] **R0576.** **[stage 20](20-stage.md) — deferred L0/L1/L2/L3/L4 capture/replay** — **Intention and defined deliverable:** Optional selected-object/activity recording and honest time-indexed inspection — **Correctness/failure evidence and performance obligation:** Q20: declared coverage/clock/source, checkpoints/events/selected observations, visible storage/sample/window limits, cancel/expiry/grant changes, bounded seek and no operational effects. Dense capture is explicit; stored replay is not a fresh prediction or arbitrary solver restart.

- [ ] Run the actual native/backend/persistence/transport path for every 20.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0712.** record a bounded activity, inspect a selected earlier instant and replay/export it with honest source/coverage and resource costs, without silently restarting a solver or affecting accepted live state. Noncaptured history and unsupported restarts remain explicit rather than invented.

- [ ] Advance only when **all 20.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
