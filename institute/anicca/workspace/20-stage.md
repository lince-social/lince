# 20 — Optional scoped recording and playback — deferred

Finish able to record a selected activity for a bounded interval and inspect it later without changing accepted live state. Capture coverage, source/clock identity, missing intervals and actual storage cost remain explicit.

## Required workflow milestones

- [ ] **20.M01.** Select a rolling/hinged assembly or supported result fields, choose Record for a bounded interval and see estimated/actual storage. No whole-workspace or every-step capture starts automatically.

- [ ] **20.M02.** Scrub, play/pause, compare and export the recorded subset after closing or going offline. Distinguish actual observations from interpolation, gaps and unrecorded private fields; noncaptured history is unavailable.

- [ ] **20.M03.** Keep capture, save/undo, essential scientific evidence and live execution separate. Replay never emits current Area crossings/operational Actions or pretends to predict a changed room/model.

- [ ] **20.M04.** Cancel/partially capture, revoke sharing, disconnect a viewer or close the owner view under the admitted lifetime policy. Bound retention/seek/decode and release demand without starving stable workspace work.

- [ ] **20.M05.** Explicitly record a permitted B41 articulated activity for a bounded interval and compare its selected poses/joint observations offline. Preserve the distinction between authored clip targets, actual captured observations, interpolation and a user's annotations; replay cannot control the current body or reveal unrecorded/private structures.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

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

- [ ] **20.T05.** Extend the existing optional capture/result viewer to selected articulated pose/joint/region observations with source/rig/bind/run/clock identity and declared sampling/coverage. Reuse stage 11's semantic/pose selections and applicable annotation policy, keeping authored animation source usable before recording exists. Bound storage/seek/retention, preserve missing intervals and source changes, test revoked/partial/offline viewing and ensure playback emits no current controller/Area/operational command.

- [ ] Recover [stage 11's motion/semantic handoff](11-stage.md#completion-summary) for 20.T05 and the selected running activity's verified capture contract. Saved authored clips, actual observed movement and user reports keep separate provenance and independent access rules.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Capture an explicit bounded subset.** Select objects/fields, source/run/clock, cadence/window, rights and storage/output budget before Record. Store the supported checkpoints/events/observations and coverage index; keep essential study evidence and ordinary saved state independent of optional capture.

- [ ] **Cycle B — Inspect real recorded coverage.** Implement bounded seek/scrub/play/compare/export over actual captured samples, labeling interpolation/gaps/unknown fields. Reopen offline without starting live execution, Area/operational Actions or predictions for a changed model.

- [ ] **Cycle C — Exercise lifetime and articulated coverage.** Capture the bounded assembly and B41 subset while unrelated native work continues; cancel, partially finish, close the owner, revoke sharing and disconnect observers. Keep authored clip targets, observed poses and user annotations distinct through storage and replay.

- [ ] Preserve these contracts after authorization; assembly and character recordings are example recipes over built-in selected-activity capture/playback.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **20.W01 — Record and revisit selected activity** | Built-in bounded capture/seek/export; 20.M01/20.M02. Record the explicit 30-second assembly subset, close/offline and inspect a selected event. | Storage estimates, chosen coverage, scrubbing, gaps and interpolation are understandable. | Recorded fields/source/clock remain pinned; unrecorded history stays unavailable; measure samples/encoded/index/scratch bytes, seek/decode latency and retention against selected bounds. |
| **20.W02 — Replay without live effects** | Built-in capture/save/execution separation; 20.M03. Play back while the current workspace remains editable. | Recorded state and current live state cannot be confused by controls or labels. | Replay-caused live commands/Area crossings/stock changes = 0; capture does not replace save/undo or essential evidence; record mixed native latency and independent resource ownership. |
| **20.W03 — Bound partial/private articulated recording** | Built-in lifetime/permission/semantic coverage plus B41 recipe; 20.M04/20.M05. Record poses, cancel/revoke/disconnect and compare allowed observations offline. | Targets versus actual samples versus annotations, partial capture and expiry are clear. | No private/unrecorded structures disclosed; owner/Keep-running policy holds; measure partial close/release, allowed viewer bytes and stable input/frame cost. |

- [ ] **Checks to select.** Unit/property tests cover ordered time indexes, coverage/gaps, interpolation labels and bounded seek. Integration/fault cases interrupt capture/publication, change source/clock/grants, exhaust byte limits and isolate replay from command emitters. Native replay chooses Record scope, scrubs/compares/exports offline and resumes unrelated editing; validate both assembly and articulated coverage.

- [ ] **Earlier-work repair.** Retain 17.W02 essential study evidence, 18.W03 physical authority, 11.W03 authored clips and 15.W01–15.W05 core use. Optional recording cannot change current solver control or turn every earlier edit/session into permanent history.

## Concrete acceptance replay

- [ ] **20.P01 — Capture an explicit finite subset; 20.T01/20.T02 / 20.W01.** After authorization, select the assembly fields, declared clock/cadence and bounded 30-second interval, inspect estimated storage and Record. Close/go offline, seek a named event, scrub/compare/export and independently compare recorded samples/time/units with the captured input trace. Include actual gaps and label interpolation; unavailable history remains unavailable. Record sample/index/checkpoint/encoded/staging bytes, write throughput and cold/warm seek/decode cost. Review scope, storage progress and useful playback; essential 17.W02 evidence remains available without this optional capture.

- [ ] **20.P02 — Playback has no live authority; 20.T03 / 20.W02.** Replay while the current room/body remains editable and active; instrument actual controller, Area and operational command boundaries. Assert zero replay-caused commands/crossings/stock mutations and unchanged accepted live state apart from separately requested edits. Compare recorded versus current identity/clock/source labels and inspect gaps or changed source. Measure mixed native latency and independent capture/playback demand. Preserve 18.W03 writer authority, 11.W03 authored clips and normal save/undo; no replay UI silently starts a changed-model prediction.

- [ ] **20.P03 — Partial articulated capture respects rights and lifetime; 20.T04/20.T05 / 20.W03.** Record selected B41 poses/joint observations, distinguish authored targets from samples/annotations, then cancel, hit a byte cap, close the owner, revoke sharing and disconnect a visitor. Inspect retained partial coverage offline and deny private/unrecorded structures. Verify the admitted Keep-running policy and no unauthorized background lease. Record partial finalization, viewer transfer, seek and last-demand release peaks; review actual partial/revoked states while retaining 15.W01–15.W05 ordinary interaction.

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

- [ ] Instantiate the local **20.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Record an explicit 30-second bounded assembly activity, navigate/edit unrelated workspace content, reopen the recording and inspect the selected event. Measure capture/encoded/index/scratch/seek cost and missing/partial/revoked cases, not only the ability to serialize frames.

- [ ] **R0576.** **[stage 20](20-stage.md) — deferred L0/L1/L2/L3/L4 capture/replay** — **Intention and defined deliverable:** Optional selected-object/activity recording and honest time-indexed inspection — **Correctness/failure evidence and performance obligation:** Q20: declared coverage/clock/source, checkpoints/events/selected observations, visible storage/sample/window limits, cancel/expiry/grant changes, bounded seek and no operational effects. Dense capture is explicit; stored replay is not a fresh prediction or arbitrary solver restart.

- [ ] Run the actual native/backend/persistence/transport path for every 20.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] Verify every local **20.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0712.** record a bounded activity, inspect a selected earlier instant and replay/export it with honest source/coverage and resource costs, without silently restarting a solver or affecting accepted live state. Noncaptured history and unsupported restarts remain explicit rather than invented.

- [ ] Advance only when **all 20.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **20.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

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
