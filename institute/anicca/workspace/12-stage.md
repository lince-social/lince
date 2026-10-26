# 12 — Structural molecule, crystal and protein editing

Finish able to open a molecule, periodic crystal or protein as its own workspace root, inspect/edit supported structure and save/export without a room/world parent. This is structural authoring; property derivation and new laboratory experiments remain deferred.

## Required workflow milestones

- [ ] **12.M01.** Import a finite molecule, nonorthogonal crystal and protein through the declared SDF/CIF/PDBx subset. Select stable atoms/sites/residues in spatial, numeric and sequence views and preserve source/alternate/missing data.

- [ ] **12.M02.** Edit supported sites/bonds/cell parameters or protein selections with normal preview/undo/export. Invalid or unsupported chemistry remains an inspectable draft and never acquires a force field or folding prediction by appearance.

- [ ] **12.M03.** Open the protein workspace directly, without a material, macro consumer or Earth. Change represented amount and UI magnification while keeping the actual authored/computational population bounded; explicit supercell display expansion is separately admitted.

- [ ] **12.M04.** Reorder sites/residues and reload without losing identity/correspondence. Density/energy/experiment calculations do not start implicitly; clearly separate supported structural validation from unavailable reconstruction.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

- [ ] Confirm the preceding [stage 11](11-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 03 summary](03-stage.md#completion-summary); [stage 04 summary](04-stage.md#completion-summary); [stage 02 summary](02-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed)`; `selected bounded structure parsers/profiles`; `crates/interface/src/native/`.

## Ordered implementation tasks


- [ ] **12.T01.** Implement finite-molecule, periodic-site and bounded protein profiles with compact stable IDs, species/isotope/charge fields, bonds/conformers, lattice/symmetry/occupancy and sequence/chain/residue/site mapping as applicable. Add declared SDF/CIF/PDBx-mmCIF subsets with source/license metadata and loss/unsupported reports.

- [ ] **12.T02.** Add native point/bond/site/cell and sequence/structure selection/edit controls through I1–I5, numeric tools, supercell preview, validation diagnostics, undo and export. Keep display magnification, periodic images, physical amount and actual computational sample/population distinct.

- [ ] **12.T03.** Open/save the protein specimen as its own root with no material/macro/Earth parent. Use the same native inspector/artifact contracts for structure metadata and authored state. Preserve protocol/observable extension points in the requirements; implementation of new measurement experiments/protocol execution and result comparisons waits for deferred [stage 17](17-stage.md).

- [ ] **12.T04.** Test atom/bond/residue reorder, supported mirrored/stereochemical cases, missing/alternate sites, partial occupancies, nonorthogonal cells, duplicate symmetry expansion, invalid cells and stale structural/display jobs. Unsupported sequence-to-structure reconstruction remains an explicit draft/diagnostic. Do not compute density, energy, folding or force-field coverage as an implicit structural edit.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Preserve supported structure precisely.** Implement compact stable atom/site/residue/chain maps and finite/periodic/protein payloads with the declared SDF/CIF/PDBx-mmCIF subsets. Keep source, symmetry/occupancy, charge/species, alternate/missing sites and unsupported/loss information through import and export.

- [ ] **Cycle B — Edit independently through ordinary tools.** Connect spatial/numeric/sequence selection, supported bond/site/cell edits, validation diagnostics, preview, undo and save/reload. Open specimens as roots; physical amount, authored population, periodic display images and magnification remain separate.

- [ ] **Cycle C — Bound expansion and verify semantics.** Exercise reorder, symmetry, nonorthogonal/invalid cells, partial sites and stale display jobs. Admit any explicit supercell display expansion separately; unsupported reconstruction remains a draft and no property/force-field/folding calculation starts implicitly.

- [ ] Preserve these contracts; selected molecules/crystals/proteins are example specimens for built-in structural import/edit/selection capabilities. An independent protein editor is a valid endpoint without a macro consumer.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **12.W01 — Inspect and edit a microscopic structure** | Built-in structural profiles and native tools; 12.M01/12.M02. Select the same sites/residues in spatial/numeric/sequence views, edit, undo and export. | Supported bonds/cell edits, sequence correspondence and invalid/missing data are understandable. | Preserve site/residue IDs, units, stereochemical/symmetry semantics and loss reports for the supported subset; measure actual sites/residues/bytes and import/edit/export/selection latency. |
| **12.W02 — Work at the micro endpoint independently** | Built-in root/unit/display separation plus specimen recipe; 12.M03. Open directly, change amount/magnification and optionally request bounded display images. | The specimen is usable without Earth/material setup; represented amount versus visible/sample size is clear. | Changing grams/moles alone allocates zero extra sites/workers; explicit expansion respects its separate cap; measure source/display population and active residency. |
| **12.W03 — Reorder, reopen and preserve partial data** | Built-in identity/validation/persistence; 12.M04. Reorder sites, save/reload and supersede a structural/display operation. | Partial/alternate coverage and unsupported reconstruction stay visible and recoverable. | No identity reassignment or stale publication; missing sites remain missing; preserve source/cell/sequence maps; record rebuild/cancel peaks and zero implicit density/energy/experiment starts. |

- [ ] **Checks to select.** Unit/property tests permute sites/residues, transform units/frames and apply supported symmetry while checking identity and duplicate expansion. Parser/integration tests cover nonorthogonal/invalid cells, alternate/missing sites, partial occupancy, resource bounds and denied sequence metadata. Native replay performs actual point/bond/cell and sequence selection edits through save/export/reopen.

- [ ] **Earlier-work repair.** Retain 03.W01 units/precision, 04.W02 semantic selection and 02.W01 durable source/definition behavior. Verify the specimen does not activate geographic loaders, allocate a Record/ECS entity per microscopic site or change the earlier room/CAD authoring path.

## Concrete acceptance replay

- [ ] **12.P01 — Structural editing is a real endpoint; 12.T01/12.T02 / 12.W01.** Import the bounded finite molecule, nonorthogonal crystal and protein through their declared subsets. Select the same stable sites/residues in spatial/numeric/sequence views, make supported bond/site/cell edits, undo and export/reimport. Compare IDs, units, cell/sequence maps, supported stereochemistry and loss reports with independently specified fixture values. Record real site/residue/bond counts, parser/display bytes and selection/edit/export latency; review missing/alternate/invalid data in actual tools. Preserve 03.W01 local precision and 04.W02 authorized selection.

- [ ] **12.P02 — Amount is not computational population; 12.T03 / 12.W02.** Open the specimen directly with Earth and material consumers absent. Change represented grams/moles and display magnification while holding source/sample fixed, then explicitly request a bounded periodic-image/supercell preview. Assert unchanged authored population and zero scientific/geographic work for amount/view changes; admit the explicit expansion independently. Measure compact source/display allocations and query work as separate quantities. Review direct-root usability and amount/image distinctions; retain 02.W01 save/reopen with no per-site Record/ECS requirement.

- [ ] **12.P03 — Partial structure stays honest; 12.T04 / 12.W03.** Permute atoms/residues, test partial occupancy, missing/alternate sites, duplicate symmetry expansion and invalid cells, and finish an obsolete display job after a newer edit. Compare supported invariants and exported structure against the fixture oracle; reject stale publication without inventing missing atoms or sequence reconstruction. Record validation/cancel/rebuild peaks and explicit unsupported states. Reopen the partial source and verify ordinary room/CAD editing still works with no implicit density, energy, folding or experiment job.

## Worked contract example

- [ ] **12.X01.** Keep physical amount, stable authored sites, periodic display images and later numerical sample/replica counts separate through import, editing, persistence and admission. Increasing grams/moles must not allocate amount-times-Avogadro entities. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  #[derive(Clone, Copy)]
  struct AmountAndSample {
      represented_mol: f64,
      authored_sites: usize,
      displayed_images: usize,
  }

  fn main() {
      let small = AmountAndSample {
          represented_mol: 1e-12,
          authored_sites: 128,
          displayed_images: 1,
      };
      let bulk = AmountAndSample {
          represented_mol: 1.0,
          ..small
      };
      assert_ne!(small.represented_mol, bulk.represented_mol);
      assert_eq!(small.authored_sites, bulk.authored_sites);
      assert_eq!(small.displayed_images, bulk.displayed_images);
  }
  ```

## Architecture and task requirements

### Confirmed intent and context recovery

- [ ] **R0015.** **C04 — Quantity is separate from computational population.** A gram or a mole describes a physical amount. It never automatically allocates that number of atomic/molecular/protein instances. Explicit sample size, replicas and solver degrees of freedom determine computational cost. Statistical weights and bulk scaling belong to a declared model with assumptions.

### Proposed defaults for refinement

- [ ] **R0056.** Microscopic structure versus simulation — **Proposed default:** Structural molecule/crystal/protein editing in [stage 12](12-stage.md); direct physical-material use, crystal density/tensor bridge and independent experiment in deferred stages [16](16-stage.md)–[17](17-stage.md) — **When it may change:** No force field, property resolver or experiment is required to open/edit/save a structure; select numerical datasets/protocols at the deferred stage

### Domain workflows as implementations of the primitives

- [ ] **R0360.** Molecule/crystal/protein — **Authored/source support:** Stable sites, finite graph, chains/residues or periodic cell — **Specialized operation/constraint:** Supported structure/sequence validation, lattice/symmetry edits and selected calculation — **Shared outputs and reuse:** Authored specimen, representations, conditions, evidence and independent result or optional candidate model

### Molecular, crystalline and smaller-scale authoring

- [ ] **R0371.** Provide compact site/atom buffers with stable IDs, species/isotope/charge where relevant, coordinates and optional topology. Finite molecules have bond order, aromatic/stereochemical representation, conformers and hydrogen/protonation policy. Crystals add a nondegenerate cell matrix, fractional sites, periodic axes, occupancy and symmetry/space-group metadata. A periodic neighbor is identified by site plus image offset; minimum-image assumptions and neighbor cutoffs must match the cell.

- [ ] **R0372.** The first bounded native editors support atom/site selection and numeric placement, add/delete/species change, finite-molecule bonds, cell editing, repeat/supercell preview, symmetry-breaking confirmation, save/undo and explicit validation. A protein profile adds stable sequence/chain/residue/site correspondence, conformer/source metadata and native sequence/structure selection with explicitly supported edits; unsupported residue reconstruction or prediction remains a diagnostic draft. Render repeated sites with instancing, not one heavyweight Record per atom. A small molecule and a periodic crystal are separate fixtures, even when they share point rendering.

- [ ] **R0373.** Structural edit validation reports valence/charge/stereochemistry and cell/overlap issues at the level the selected chemistry adapter supports. Inferred bonds are marked inferred; a coordinate-only import cannot establish connectivity uniquely. An unusual structure can remain an authored draft. It cannot acquire a validated force field by being visually plausible.

- [ ] **R0374.** Support declared SDF and CIF subsets for small molecules and crystals, plus a bounded PDBx/mmCIF subset for the first independent protein specimen. Preserve supported biological assemblies, alternate conformations, occupancy and experiment metadata. [stage 12](12-stage.md) delivers structural import/edit/selection/export only. Deferred [stage 17](17-stage.md) adds the first structure-measurement/conformation-comparison experiment; full chemical preparation, fresh dynamics and folding prediction remain distinct [stage 21](21-stage.md) capabilities with separate gates. Preserve unsupported chemistry/source fields rather than silently exporting a simpler molecule. Pin dictionaries/parser versions. File import/export is one capability; energy minimization, molecular dynamics, electronic structure, reactions and folding prediction are separate adapters.

### Fixture phases and the stabilization boundary

- [ ] **R0768.** B08/B18 — **Core proof through [stage 15](15-stage.md):** [stage 12](12-stage.md) bounded structure/sequence/cell identity, validation, import/edit/export, root independence and sample-versus-amount metadata; no new protocol execution, physical prediction or density derivation — **Deferred proof in stages [16](16-stage.md)–[21](21-stage.md):** [stage 17](17-stage.md) density/first experiment/results; [stage 21](21-stage.md) selected molecular/protein dynamics, reactions/electronic/folding methods

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0733.** B08 — **Fixture:** A finite molecule and a nonorthogonal periodic crystal with occupancy/symmetry — **Proof required:** Stable site identity, format subset/loss reporting, density and stale derivation

## Workflow, correctness and performance evidence

- [ ] Instantiate the local **12.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Perform an actual supported structure edit and sequence/site selection on bounded rights-reviewed specimens, including partial occupancy, symmetry and alternate/missing sites. Measure import/edit/export/selection by real sites/residues/bytes and verify unrequested images/solver populations are not allocated.

- [ ] **R0568.** **[stage 12](12-stage.md) — L2/L3/L4, structural microscopic L5 proof** — **Intention and defined deliverable:** Bounded independent molecule/crystal/protein structural authoring, without experiment execution — **Correctness/failure evidence and performance obligation:** B08/B18 structural subset plus B22/B23: site/sequence/cell identity, declared SDF/CIF/PDBx subset, draft/undo/export/reload and no macro parent. Measure actual sites/residues/bytes; changing represented amount does not allocate copies. H2/H8 protect specimen/sequence metadata and payloads. Density, tensors, new measurements/protocols and molecular dynamics remain deferred.

- [ ] Run the actual native/backend/persistence/transport path for every 12.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] Verify every local **12.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0661.** bounded molecule, crystal and protein fixtures are authored/imported/saved/exported without silently losing supported structure. Independent specimen/laboratory-root authoring is usable without an experiment or macro destination. Invalid/unsupported chemistry remains a draft; opening/editing it starts no force field, folding prediction, property derivation or mole-sized population. Deferred [stage 17](17-stage.md) adds the first laboratory experiment.

- [ ] Advance only when **all 12.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **12.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

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
