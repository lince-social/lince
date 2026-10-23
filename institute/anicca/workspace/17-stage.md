# 17 — Micro-derived models and independent laboratory experiments — deferred

Finish with both paths working on one implementation: edit a bounded microscopic description to produce an applicable reusable model, and run a standalone protein experiment whose results are valuable without any macro consumer. Represented bulk amount never multiplies atoms by a mole.

## Required workflow milestones

- [ ] **17.M01.** Derive ideal crystal density from declared cell population/occupancy and volume, rotate a documented stiffness tensor and compute the supported Voigt/Reuss/Hill bridge with explicit assumptions and independently checked references.

- [ ] **17.M02.** Edit a selected microscopic source, leave the editor and automatically apply a ready applicable candidate only to the captured local consumer scope. Pending/invalid/cancelled/stale results keep the previous accepted model identified and cannot apply later to a changed scope.

- [ ] **17.M03.** Open the protein lab as a root, set up the selected bounded structural-measurement/conformation-comparison experiment, inspect plots/results/control comparison and export evidence. Do not advertise that measurement as fresh folding or molecular dynamics.

- [ ] **17.M04.** Let an authorized remote participant observe the bounded result without installing its solver or receiving denied inputs. Respect separate start/cancel/spending grants, owner-close/Keep running and interrupted/restartable status.

- [ ] **17.M05.** Replay direct steel/wood use, micro-derived automatic local adoption and the independent experiment through the same command/job/query/view machinery while the stable workspace remains responsive.

## Entry and scope

- [ ] Confirm the preceding [stage 16](16-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 12 summary](12-stage.md#completion-summary); [stage 16 summary](16-stage.md#completion-summary); [stage 02 summary](02-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Keep this stage deferred until the [core stabilization gate](15-stage.md#advance-gate) passes and this numerical slice has implementation authorization. Capture actual inputs, reference/error/quantity/resource limits, data rights, worker trust/location and payer before dispatch; regress affected stable-core workflows.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed science/material profiles)`; `crates/engine/src/`; `crates/interface/src/native/`; `selected bounded experiment worker`.

## Ordered implementation tasks

- [ ] **R0683.** **Deferred until [stage 15](15-stage.md), [stage 16](16-stage.md) and the applicable [stage 12](12-stage.md) structural prerequisites pass.** Select the crystal/reference tensor and first independent protein protocol with actual sample/replica/output budgets here. Preserve all W01–W03 endpoints and the automatic local-adoption intent; no micro-derived physical model is required for core stabilization.

- [ ] **17.T01.** Implement ideal crystal density from declared cell contents/occupancies and volume, counting symmetry-expanded sites exactly once. Distinguish ideal theoretical density from measured porous/bulk density and reject missing masses/cell conditions.

- [ ] **17.T02.** Implement crystal tensor orientation and the documented Voigt/Reuss/Hill effective-property bridge with units, notation, assumptions and evidence. Use isotropic synthetic fixtures and one rights-reviewed real dataset; compare independent reference outputs.

- [ ] **17.T03.** Complete W02/B17 through the shared draft/output/applicability/adoption contract: inspect consumers, edit a bounded source, leave/apply under the selected policy, validate a candidate, update selected bindings and report a receipt. Handle pending/invalid output, cancellation, a newer draft, changed grants/base and unsupported running-state transfer.

- [ ] **17.T04.** Complete the first W03/B18 experiment as an independent root: specimen/protocol/conditions/observables, admitted run, retained evidence and native result/plot/comparison/export views. A bounded structure-measurement or conformation-comparison protocol can be first; state its actual result and reference. Fresh physical dynamics/prediction needs its own [stage 21](21-stage.md) model/data/validation gate.

- [ ] **17.T05.** Provide one derivation/experiment inspector shared by both endpoints. Editing the source invalidates relevant outputs; users can recompute, choose another candidate or retain a labeled approximation. Material pickers and microscope-derived outputs converge on the same binding code.

- [ ] **17.T06.** Verify amount/sample/replica distinctions: the chosen representative calculation remains bounded as represented bulk quantity changes; only explicit solver sample/replica requests allocate corresponding numerical populations. Record adopted-model cost separately from experiment and render costs.

- [ ] **17.T07.** Prove the existing shared command/job/artifact/dependency implementation for W01–W03, with a bounded coherent inspector/plot projection and demand lease. Reuse the E4/I6 implementation already exercised by [stage 06](06-stage.md) locality rather than establishing another query/runtime service. Extend the relevant B19/B22/B24 subset for actual numerical inputs. Cache applicability/results with exact typed input, adapter and purpose keys. Display-detail changes cannot invalidate applicable density or change the represented sample.

- [ ] **17.T08.** Complete the first paired native acceptance replay: direct steel/wood assignment, supported microscopic density edit automatically adopted into the selected local variant, and independent protein result/comparison/export. Test ready/pending/invalid exit, shared-definition isolation, undo/reload and late completion under B20. Keep running-state adoption explicitly pending until the relevant [stage 18](18-stage.md)/[stage 19](19-stage.md) domain supports it.

- [ ] **17.T09.** Start B37 with the actual bounded protocol/result from this stage: one designated execution authority, visible requester/worker/resource payer, permitted observation in another participant's view without its solver, separate start/cancel/spending grants, epoch-tagged results and source-specific disclosure. A disconnected observer does not restart or duplicate the job; current source/adoption changes use the same result-generation checks. Reuse H7 job/observation channels; fresh shared dynamics remains [stage 18](18-stage.md)/[stage 21](21-stage.md) work. Verify Q16's last-owner-view pause/checkpoint and explicit Keep running lifetime/resource behavior, retaining other participants' leases and honest nonrestartable status. Q20/A06 now agrees optional scoped Record in [stage 20](20-stage.md); retain protocol inputs, actual sample/conditions, essential outputs and minimal reproducibility evidence here, without automatic dense recording or a mandatory replay frontend.

## Worked contract example

- [ ] **17.X01.** Construct cell mass from supported site masses/occupancies and symmetry multiplicities exactly once, and volume from the physical cell determinant with declared units. The scalar example is ideal density only; porosity, stability and bulk constitutive behavior require separate evidence/models. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn ideal_density_kg_m3(cell_mass_kg: f64, cell_volume_m3: f64) -> Option<f64> {
      if !cell_mass_kg.is_finite()
          || !cell_volume_m3.is_finite()
          || cell_mass_kg <= 0.0
          || cell_volume_m3 <= 0.0
      {
          return None;
      }
      let density = cell_mass_kg / cell_volume_m3;
      (density.is_finite() && density > 0.0).then_some(density)
  }

  fn main() {
      assert!((ideal_density_kg_m3(8e-26, 1e-29).unwrap() - 8000.0).abs() < 1e-9);
      assert_eq!(ideal_density_kg_m3(8e-26, 0.0), None);
  }
  ```

## Architecture and task requirements

### Confirmed intent and context recovery

- [ ] **R0013.** **C02 — Authoring at any scale.** Entering a microscopic editor changes a bounded authored specimen/model. Leaving it can publish an applicable reusable behavior model for selected consumers, under the explicit exit/application policy below. Other authoring scales use the same draft, evaluation, validation and adoption machinery.

- [ ] **R0014.** **C03 — Laboratory as an end goal.** A protein/molecule/crystal study can be a root document and workspace with its own experiments, comparisons and results. It requires no parent macro object, material assignment, building, Earth model or exported scalar property.

### Authoring, ordinary use and independent experiments

- [ ] **R0030.** This section specifies final behavior across workflows. Core draft/geometry/receipt handling is delivered as needed in stages [02](02-stage.md)–[15](15-stage.md); new physical material interpretation, micro-to-property derivation and experiment execution are deferred to stages [16](16-stage.md)–[21](21-stage.md). A core editing milestone does not need a laboratory result to prove its own transactions.

- [ ] **R0031.** Microscopic content has two equally valid uses. **Model authoring** produces a reusable description or behavior for other consumers. **Experimental work** studies the specimen itself. One document can support both, but neither use requires the other. A native editor at any scale uses the same activity states: inspect, author a draft, evaluate a bounded preview/job, run a selected study, and inspect results. Opening or zooming into content does not automatically start its most expensive solver.

- [ ] **R0032.** Change a pillar's rod from steel to wood in a dropdown — **Authored input:** Selected occurrence/region, material candidate, orientation/conditions where required — **Accepted output/use:** A revisioned material binding and invalidated affected products — **Computation that is actually requested:** Applicability/unit/coverage checks and needed local reevaluation; no atom population

- [ ] **R0033.** Enter a microscopic model, change structure/parameters and return to ordinary use — **Authored input:** A bounded specimen or model draft plus the selected derivation/evidence — **Accepted output/use:** An applicable reusable property/constitutive/behavior model adopted by explicit consumers — **Computation that is actually requested:** Only the representative sample/derivation chosen for that model; no automatic bulk replication

- [ ] **R0034.** Open a protein laboratory and compare experiments — **Authored input:** Standalone specimen, protocol, conditions, controls, observable definitions and runs — **Accepted output/use:** Measurements, trajectories, analyses, evidence and comparison views — **Computation that is actually requested:** Explicitly budgeted specimens, replicas and supported calculations; no macro consumer required

- [ ] **R0035.** Reuse a laboratory finding in another model — **Authored input:** A selected result, interpretation, applicability and provenance — **Accepted output/use:** A new candidate model linked to the experiment — **Computation that is actually requested:** An explicit export/derivation and adoption operation; the original experimental record remains intact

- [ ] **R0036.** “Control room” describes how a detailed view can configure a coarser consumer. It is an interaction/derivation relationship, not physical containment. A microscopic change needs a supported interpretation: an edited cell can change calculated ideal density; an applicable law parameter can update its numerical model; a speculative atom edit cannot supply a new elastic tensor without a selected calculation, dataset or explicit authored approximation. Missing knowledge remains visible. A macro material picker and a micro-derived model both bind the same typed model slot once validated.

### Shared draft → model → application contract

- [ ] **R0037.** **Create draft.** Capture the source revision, selected objects/regions, editing scope and affected consumers. Draft edits have normal preview/cancel/undo. Save an invalid/incomplete draft with diagnostics without changing the accepted model used by other objects.

- [ ] **R0038.** **Declare outputs.** Operators expose typed output slots and their purpose: reusable model, geometry, observation/measurement, result field, trajectory or report. A model output has an applicability/units/state contract. A laboratory result set is already a valid endpoint and does not need to become a material value.

- [ ] **R0039.** **Prepare evaluation.** Resolve parameters, source/evidence revisions, units/frames, material conditions and numerical method. An estimate declares sample size/replicas, bytes, CPU/GPU/time and optional network inputs. Cheap validation/compilation can run inline within a budget; expensive predictions are bounded asynchronous jobs.

- [ ] **R0040.** **Produce candidate.** Capture output status, coverage, uncertainty, assumptions, source mapping, tool/build and cache key. A measured/imported candidate, a computed candidate and a user-invented candidate retain their status. A completed calculation does not automatically establish experimental validation.

- [ ] **R0041.** **Validate intended use.** Check the consumer's requested quantities, conditions, constitutive/history requirements and permissions. A wood assignment may need grain orientation or missing engineering data. Report incomplete coverage rather than guessing plausible physics. Material appearance is a separately selectable output.

- [ ] **R0042.** **Adopt at a safe boundary.** Publish immutable model/source artifacts and apply a narrow expected-revision command to the explicitly selected bindings. Return a receipt identifying old/new revisions, scope, dependencies invalidated and any affected running domain. The source model and its consumers can be in different documents with separately authorized heads; preserve pending/partial application and use explicit recovery/compensation across authorities rather than claiming one distributed transaction.

- [ ] **R0043.** **Keep running-state semantics explicit.** Replacing a material can change mass/inertia, internal variables or boundary behavior. An active domain declares supported state transfer, restart-from-base, next-run-only adoption or continued use of its pinned prior model. Never silently reset velocities, erase plastic history or fabricate energy/mass conservation. An unsupported transfer leaves the prior run inspectable and marks the replacement pending for that run.

- [ ] **R0044.** Publishing an edited specimen/source, publishing a reusable model and adopting that model are separate outcomes with traceable revisions/receipts. A laboratory can accept its new specimen while completed or running experiments remain pinned to the old specimen. A consumer can retain an older accepted model while a newly edited source awaits a valid derivation. Show those exact revisions and the pending relationship; never label an old result as a fresh prediction of the new source. The same distinction applies to CAD/geometry authoring and derived collision/solver products.

- [ ] **R0045.** Owner-confirmed exit behavior, 2026-10-18: leaving an editor automatically applies a ready, applicable candidate to the scope explicitly selected when editing began. If evaluation is incomplete/invalid, return to ordinary use with the prior accepted model still identified, a visible pending/diagnostic state and a clear recovery action; never silently present the draft as applied. Auto-application after an asynchronous result is allowed only for the captured exit policy, unchanged generation/base/scope, current authority and a supported domain transition. Otherwise retain the candidate for review. Cancelling or a newer draft prevents late application.

- [ ] **R0046.** Owner-confirmed default scope is a local variant for the selected occurrence/region, preserving other consumers. Editing an explicitly shared definition shows its consumer set and uses the intentional shared-update operation. Automatic local adoption and the paired modeling proof are agreed behavior; the latter is now delivered after [stage 15](15-stage.md) under the 2026-10-21 priority. Do not reopen these choices or insert another permission prompt for an ordinary edit.

- [ ] **R0047.** Keep physical amount, represented population, rendered instances, numerical sample size and replica count as distinct dimensioned/count values. A bulk model may use amount/volume/mass to scale an extensive quantity only when its law permits that scaling. A larger amount can require a larger continuum domain or better sampling for the chosen question, but never triggers `amount × Avogadro` entity creation. A microscopic laboratory can deliberately request more particles/replicas within its own admission budget. Its cost estimate describes that request directly.

### Standalone laboratory contract

- [ ] **R0048.** A laboratory workspace composes a specimen definition/revision, experimental protocol, conditions, initial/boundary state, independent/control parameters, observable definitions, selected evaluator, resource policy, runs and analysis. Specimens, protocols and analyses are reusable definitions with ordinary instances/overrides. A run pins them; later edits create another run or candidate rather than rewriting completed evidence.

- [ ] **R0049.** Native tools must support specimen/sequence/structure selection, parameter/condition editing, protocol setup, bounded parameter sweeps/replicates, run/pause/cancel where supported, comparison/plots, export and provenance inspection. Store full results where requested or explicit summaries/sampling with a retention policy. Show incomplete/failed/unsupported cases beside completed ones. Measured data, structural analysis, fresh physical simulation and folding prediction are distinct supported experiment types, each with declared coverage and validation.

- [ ] **R0050.** A protein profile retains sequence, chain/residue/site identity, supported bonds, assemblies, conformers/alternate sites, source uncertainty and missing regions. It can connect sequence and structure views through stable selections. Numeric results and trajectories reference those identities; a source edit can invalidate their mapping. Its first bounded experiment can be structure measurements/conformation comparison against a reference, with fresh molecular dynamics and prediction added under their own gates. A structural measurement alone must not be advertised as validated folding dynamics.

### Capability coverage and reusable workflow recipes

- [ ] **R0110.** **W02 — Detailed authoring for a consumer** — **Shared primitives and thin specialization:** Draft, operator, candidate model, explicit output binding and adoption receipt — **Minimum proof / later expansion:** Deferred [stage 17](17-stage.md)/B17: bounded micro edit changes a supported consumer through a valid model; quantity never expands particle population

- [ ] **R0111.** **W03 — Independent microscopic laboratory** — **Shared primitives and thin specialization:** Root specimen, protocol, selected evaluator, runs/results/analysis using K/I/E — **Minimum proof / later expansion:** [stage 12](12-stage.md) structural authoring is independent; deferred [stage 17](17-stage.md)/B18 executes the first experiment with no macro parent. Dynamics, reactions, electronic structure and folding retain later gates

### K4. Quantities, fields, evidence and unknown values

- [ ] **R0159.** Tensor transformations follow their physical definition. Rotating a crystal rotates its constitutive tensor; it does not merely rotate a color legend. Declare stress/strain conventions, Voigt/Kelvin notation and engineering shear factors in adapters.

### H7. Shared runs and execution authority

- [ ] **R0275.** This is the retained contract for deferred stages [17](17-stage.md)–[21](21-stage.md) numerical profiles. Core hosting stages [02](02-stage.md)–[15](15-stage.md) needs accepted edits, geometry/asset jobs, existing supported behavior and bounded presence; it does not implement a new scientific runtime or require a remote solver to join. Reuse current activity ownership/lifetime where applicable; add each scientific run/epoch/observation capability only at its deferred stage.

- [ ] **R0276.** Start with one designated execution authority per active domain/run. Its admitted local or explicitly configured worker advances that run's clock/state; participants request permitted inputs and observe bounded results. A weak visitor computer can inspect a run without the solver or original microscopic payload when the published output contract permits it. Visual detail does not select a different physical state, and joining a workspace does not start dormant simulations. Forking a study creates an independently admitted run with its own inputs/state, even when its authored definitions are shared. Changes to a material/force/collider follow the existing safe domain-adoption policy and record the applied run revision.

- [ ] **R0277.** Represent domain ownership with authority/run identity and an epoch/fencing check for accepted commands and observations. A delayed old worker/connection cannot resume writing after restart or a supported ownership change. Initial recovery uses durable request/outcome/checkpoint state and explicitly reports interrupted or indeterminate execution. Never elect a replacement simulation writer merely because another visitor stayed connected. Later co-hosting/worker relocation must prove explicit ownership transfer, compatible checkpoint/state and exclusion of the previous writer before becoming available. An epoch counter alone is insufficient across disconnected authorities: transfer needs a shared authoritative lease/grant or a protocol that establishes exclusion; without that proof the proposed replacement remains unavailable. Distributed domain coupling remains a separately validated operator, not generic peer replication.

- [ ] **R0278.** Every activity identifies who requested it, whose authority accepts it, which machine supplies CPU/GPU/storage/network and whose budget is charged. Viewing permission alone does not grant run/start/cancel/steer or spending permission. Resource use on another participant's machine is an explicit scoped grant; no automatic compute donation. Scientific output and live physical state use their declared read/disclosure policy, including effects derived from hidden geometry. A public collision or response model that reveals private structure requires deliberate publication or a safe substitute; lower visual LOD cannot resolve that disclosure question. Shared input latency, prediction/reconciliation and determinism guarantees are profile-specific future refinements where required, not an assumption that heterogeneous clients rerun every solver identically.

### Domain workflows as implementations of the primitives

- [ ] **R0361.** Standalone laboratory — **Authored/source support:** Root specimen + versioned protocol, conditions, controls and observables — **Specialized operation/constraint:** Bounded measurement/experiment/sweep/analysis — **Shared outputs and reuse:** Runs, comparisons, plots, trajectories and reports as complete endpoints; optional explicit model export

### Molecular, crystalline and smaller-scale authoring

- [ ] **R0376.** A laboratory document can start with a specimen/protocol and end with analysis, using C03 and W03 without material slots or a macro world. Material-control views can instead export applicable models through W02. Both reuse the same editor, job, result, provenance and adoption contracts; model export is an optional extra operation after an experiment.

### Solver job contract

- [ ] **R0515.** A physical job has immutable inputs, adapter/build/profile, selected supports/materials, physical equations, boundary/initial conditions, nondimensionalization/unit maps, mesh/discretization, time/solver settings, stopping criteria and limits. Its output contains status, actual covered time/space, residuals/convergence, balance diagnostics, uncertainties, result fields, checkpoint compatibility and logs.

- [ ] **R0516.** Use explicit states: queued, preparing, running, converged/completed, incomplete, failed, cancelled and superseded. Numerical convergence, successful process exit, validation against reality and accepted operational application are separate facts. A simulation result is invalid for the current model when an affecting revision changed, even if its bytes are intact.

- [ ] **R0517.** Run scientific kernels through bounded adapters. Control read-only inputs, scratch directory, memory/CPU/GPU/time/output quotas and cancellation; default-deny network/live credentials. A separate process is not alone a sandbox: use the platform's actual restrictions or label a worker trusted with its capabilities visible. An FMU/native plugin is executable code, not an inert geometry file. Installed adapters and their licenses are resolved explicitly; importing a document never downloads/executes an arbitrary solver.

- [ ] **R0518.** Checkpoints pin solver/binary/state format and supported restart conditions. Replay may need exact stored inputs and outputs when the original solver is unavailable; distinguish inspectable results, restartable runs and fully reproducible jobs. Remote/HPC execution is a later deployment of this protocol with explicit transfer/credentials/cost and cancellation outcomes. Hosted observation and explicit worker placement use H7 now as each relevant profile becomes available; later remote/HPC execution extends that same contract with credentials, cost and fencing proofs instead of creating another job API.

### Fixture phases and the stabilization boundary

- [ ] **R0770.** B17 — **Core proof through [stage 15](15-stage.md):** Generic draft/candidate/receipt/generation/cancel/scope machinery exercised by B20/B31/B40 geometry edits; this is not completed micro-to-material adoption — **Deferred proof in stages [16](16-stage.md)–[21](21-stage.md):** [stage 17](17-stage.md) actual property derivation/adoption; stages [18](18-stage.md)–[19](19-stage.md) active-domain state transfer/restart/defer and Q19 exceptional recovery

- [ ] **R0772.** B37 — **Core proof through [stage 15](15-stage.md):** Retain the H7 requirement and reuse core authority/admission interfaces; no B37 scientific-run execution is a core gate — **Deferred proof in stages [16](16-stage.md)–[21](21-stage.md):** [stage 17](17-stage.md) first remote experiment observation; [stage 18](18-stage.md) shared new physical interaction; [stage 19](19-stage.md) continuum observation; [stage 20](20-stage.md) optional capture/replay; [stage 21](21-stage.md) each later specialist profile

### Verification obligations

- [ ] **R0817.** Core intention checks include the applicable C01–C17 identity/geometry/definition/scale/hosting flows, structural root independence and amount-versus-population distinction. Deferred stages [16](16-stage.md)–[17](17-stage.md) add direct steel/wood physical assignment without microscopic content, a supported micro-derived consumer change and a standalone protein experiment through the same command/job/query/undo services. Later numerical sample/replica increases are separately admitted. Invalid/pending candidates never acquire a false applied receipt, cancelled generations never apply later, and out-of-scope consumers remain untouched in either phase.

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0830.** **Q3 — Paired modeling proof with holistic primitives, deferred after stabilization.** Direct physical-material use, supported microscopic-model adoption and an independent bounded protein experiment remain required endpoints on shared identity, units/frames, selections, operations, acceptance, jobs, results, views and demand. Domain mathematics stays in profiles. The 2026-10-21 priority explicitly supersedes the previous immediate-after-[stage 05](05-stage.md) order: stages [16](16-stage.md)–[17](17-stage.md) deliver the paired numerical proof after [stage 15](15-stage.md); locality, authoring, CAD and structural micro editing stabilize first. This defers delivery without deleting the workflows or introducing a second core.

### Subsequent fixture and adapter decisions

- [ ] **R0851.** At deferred stages [16](16-stage.md)–[17](17-stage.md), select the first material study/protein experiment with a reference result, conditions, supported operations, dataset rights and actual resource/error limits. Start with bounded structure measurement/conformation comparison, then separately validated dynamics; folding remains [stage 21](21-stage.md). Crystal density/tensor lineage is the proposed first material bridge, never a prerequisite for direct material use or the core workspace. [stage 12](12-stage.md) selects only structural import/edit/export fixtures and parser/representation limits.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0734.** B09 — **Fixture:** Synthetic isotropic tensor and one documented crystal dataset — **Proof required:** Tensor rotation/notation, bounds/estimates and independently checked derived values

### Owned fixture specifications

- [ ] **R0742.** B17 — **Fixture:** A bounded microscopic draft, cached candidate model and two independent consumers — **Proof required:** Ready/pending/invalid exit, local variant/shared update, expected-revision adoption, cancelled/stale job rejection and explicit active-run transition; represented amount never creates a mole-sized numerical population

### Owned fixture specifications

- [ ] **R0743.** B18 — **Fixture:** A root protein specimen/protocol with control and changed-condition/conformer runs — **Proof required:** No material/macro parent, stable sequence/structure selection, explicit experiment type, result/plot/comparison/export as endpoints and optional model export; reference result and actual sample/replica counts

### Owned fixture specifications

- [ ] **R0762.** B37 — **Fixture:** One bounded [stage 17](17-stage.md) experiment observed remotely, then a [stage 18](18-stage.md) physical domain with a weak viewer and separate comparison run — **Proof required:** One fenced execution authority, permitted output without mandatory solver installation, observe/interact/start/cancel/spending separation, explicit worker/payer, run/source/clock tags, unchanged shared physics under visual LOD, no automatic compute donation/failover and safe interrupted/restart/checkpoint state

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Use a rights-reviewed crystal and protein protocol with reference outputs, actual sample/replica limits and explicit retention. Change represented amount at fixed numerical sample, then deliberately request a larger sample separately; meter adoption, experiment and display costs independently.

- [ ] **R0573.** **[stage 17](17-stage.md) — deferred L0–L5 paired proof** — **Intention and defined deliverable:** W01–W03: direct material use, automatic local micro-derived adoption and a standalone bounded experiment — **Correctness/failure evidence and performance obligation:** B09/B16–B20/B22/B24 and first B37: checked density/tensor derivation, pending/invalid/ready exit, generation races, protocol/results/reference, actual sample/replica/retention budgets and independent endpoint. Reuse authority/admission, Q16 lifetime and Q19 recovery. Q20 optional replay is [stage 20](20-stage.md); retain essential run evidence here without whole-workspace recording.

- [ ] Run the actual native/backend/persistence/transport path for every 17.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0693.** W01 ordinary material use, W02 detailed authoring for a consumer and W03 laboratory results all work through shared K/I/E implementations. A laboratory needs no macro destination. One microscopic description supplies a checked reusable property where applicable without atomistic replication per bulk amount. The UI distinguishes structural measurement, imported computed evidence, homogenization, fresh physical prediction and invented behavior.

- [ ] Advance only when **all 17.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
