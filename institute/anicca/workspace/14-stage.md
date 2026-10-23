# 14 — Independently governed infrastructure and composed workflows

Finish with a useful local infrastructure workspace composed from independently governed site/city layers, operations and observations. One workspace can host a permitted view without acquiring another Organ's private data or controlling its transactions.

## Required workflow milestones

- [ ] **14.M01.** Open authored places, accepted operational tables and source-age evidence from more than one Organ in the same workspace. Independent Protein sign-in, partial offline coverage and per-field authority stay visible and usable.

- [ ] **14.M02.** Inspect a proposed site/room change and a supported operational alternative, attach the same stable selection/evidence and submit a reviewable proposal to its responsible authority. No view or scenario silently changes stock, ownership or physical equipment.

- [ ] **14.M03.** Publish an authorized public maintenance/offer view while retaining private interiors, supplier terms and reporter details. Review competing claims, duplicates, allowed ordering/partial fulfillment and explicit outcome reasons.

- [ ] **14.M04.** Demonstrate shared primitive reuse across two distinct meaningful workflows, such as furnishing a place and coordinating a site repair or factory delivery. Avoid a second save/permission/query/runtime implementation.

## Entry and scope

- [ ] Confirm the preceding [stage 13](13-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 06 summary](06-stage.md#completion-summary); [stage 09 summary](09-stage.md#completion-summary); [stage 13 summary](13-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/engine/src/workspace_sync.rs`; `crates/transport/`; `crates/interface/src/native/protein_area/`; `crates/interface/src/native/`; `crates/simulation/src/`.

## Ordered implementation tasks


- [ ] **14.T01.** Assemble independently governed site/city layers, core F1–F10 operations, observations, queries and existing scoped behaviors into one useful infrastructure workspace. Inspect retained/imported specialist evidence only where available, with its source/revision/status and without starting new science. Verify partial coverage, source ages, denied fields, disconnected work and independent authorities. Exercise H1–H10 core hosting with independent Protein access and separately budgeted existing activities from more than one Organ; no workspace lends private authority or becomes a distributed transaction coordinator. Deferred scientific profiles later regress this same composition.

- [ ] **14.T02.** Prove core domain/asset/Record reuse in at least two distinct workflows without duplicated invariants. Tighten/simplify shared primitives from that evidence before widening a profile. Do not require B21/B37 new scientific domains or coupled solver results to complete this core composition.

## Worked contract example

- [ ] **14.X01.** Preserve actual actor/source/role/ceiling and field grants at every authoritative command boundary. This small scope example is not a complete permission model and cannot substitute for current workspace/Organ policy or cross-authority recovery. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  #[derive(Clone, Copy, PartialEq)]
  struct Grant {
      authority: u64,
      document: u64,
      can_edit: bool,
  }

  fn may_edit(grant: Grant, target_authority: u64, target_document: u64) -> bool {
      grant.can_edit && grant.authority == target_authority && grant.document == target_document
  }

  fn main() {
      let local = Grant {
          authority: 1,
          document: 10,
          can_edit: true,
      };
      assert!(may_edit(local, 1, 10));
      assert!(!may_edit(local, 2, 10));
      assert!(!may_edit(local, 1, 11));
  }
  ```

## Architecture and task requirements

### Source baseline and shared families

- [ ] **R0078.** Media and vision: native `hand_camera.rs`, Signals and merged vision planning to locate before implementation — **What to reuse:** Reuse verified capture timestamps, bounded streams, demand and input provenance — **Required extension or boundary:** The former `Computer-Vision-Lab.md` path is absent in this tree; locate its current specification before relying on it. Reconstruction remains separate deferred work. Import/capture, tracking, geometry inference and accepted identity have different confidence and failure modes.

### Global Infrastructure: the experience we eventually want

- [ ] **R0904.** A person opens a city, harbor, factory or construction site, selects a place or modeled object, and sees the Records they are allowed to read: its identity, relevant Needs and Contributions, responsible parties, accepted facts, observations, commitments, transactions, maintenance, proposed changes and automations.

- [ ] **R0905.** The same information is available through lists, tables, graphs and accessible forms. A person should be able to order material or review a citizen proposal without downloading a city model or navigating in 3D.

- [ ] **R0906.** A shared city workspace can contain official layers supplied by an Organ, operator layers, citizen observations and proposed Transfers or Record changes. Reviewing a proposal can publish an accepted revision through existing authority paths. A municipality's acceptance does not erase dissenting evidence or make every contributor an official editor.

- [ ] **R0907.** Models may combine engineering meshes, terrain, imagery, point clouds, Gaussian splats and calibrated camera feeds. Each layer has its own source, rights, capture date, coordinate frame, update cadence and uncertainty. A building modeled last year and a camera frame captured now must visibly have different ages.

- [ ] **R0908.** Lince must also be useful when geometry is incomplete, a camera is unavailable, a connection is interrupted or an organization shares only aggregate information. Partial coverage is a normal state of the system.

### Four different things to keep distinguishable

- [ ] **R0909.** Representation — **Example:** A modeled warehouse, mesh or splat scan — **What it can establish:** Appearance or geometry for a stated source and revision

- [ ] **R0910.** Observation — **Example:** A scale reading, camera detection or operator count — **What it can establish:** A source's measurement or claim, with time and quality

- [ ] **R0911.** Accepted operational state — **Example:** A received lot, approved work order or settled Transfer — **What it can establish:** The state accepted under a named authority and policy

- [ ] **R0912.** Scenario — **Example:** A projected shortage after a supplier stops — **What it can establish:** A conditional result from a named base and assumptions

- [ ] **R0913.** A camera seeing a pallet does not establish its contents, ownership or availability for sale. A visually accurate mine does not establish its future output. A replayable simulation does not establish that its physical or economic assumptions are correct.

### F8. Spatial identity, frames and Areas

- [ ] **R0997.** Keep canonical geographic coordinates and engineering coordinates separate from presentation transforms. A coordinate frame specifies reference system, origin, axes, handedness, units, vertical reference where applicable and its alignment revision.

- [ ] **R0998.** Use high precision for canonical/world transforms and nearby local origins for rendering. Changing the camera origin must not change a Record's stored location. Test round trips at actual city extents.

- [ ] **R0999.** Support points, routes/lines, polygons with holes, surfaces and volumes where workflows require them. Containment, proximity, intersection and network connectivity are different predicates. A visually overlapping object is not necessarily physically inside a zone.

- [ ] **R1000.** Distinguish workspace layout Areas, measured physical regions, geographic jurisdictions, operational zones and temporary Areas. Reuse Area behavior without treating a decorative boundary as authoritative geography or ownership.

- [ ] **R1001.** Bind a Record/Area to a stable modeled asset or local anchor with an explicit follow policy. Re-importing geometry, moving a real asset and correcting alignment are different changes. Persist unresolved bindings for review rather than attaching to the nearest object silently.

- [ ] **R1002.** Define boundary and uncertainty behavior: floors, height, holes, moving assets, tracking error, entry/exit hysteresis and stale positions. Precision sufficient for city selection may be insufficient for a shelf or construction clearance.

- [ ] **R1003.** Area effects must remain subject to existing authority and command policies. Entering a zone can propose or signal work; it must not automatically settle ownership or execute an unsafe device action.

- [ ] **R1004.** Spatial queries and picking use lightweight semantic geometry when appearance uses splats. Render detail, collision geometry, measurement geometry and legal boundaries may all differ.

- [ ] **R1005.** **UI:** place/model alignment controls, layer provenance, Area editing, binding repair and inspection of exact versus approximate location. Selecting in 2D or 3D opens the same Record detail.

- [ ] **R1006.** **Proof:** changing an imported model revision preserves valid Record/Area bindings; changing a floating origin preserves geographic location; an ambiguous boundary observation does not trigger a confident entry command.

### F10. Live observations, cameras and specialist simulations

- [ ] **R1016.** Live evidence/source-age/access/reconciliation is core stages [13](13-stage.md)–[14](14-stage.md). New thermal/structural/fluid/traffic/equipment/scientific predictions and their coupling are retained deferred stages [16](16-stage.md)–[21](21-stage.md) work. A feed, a geometry model or the existing operational simulator does not implement those solvers; their absence cannot block core observation/workspace stabilization.

- [ ] **R1017.** Reuse Signal semantics for sampled properties where appropriate. High-rate media uses a separate bounded stream with identity, capture timestamps, calibration, quality and retention. Do not store every video frame as a Record or numeric Signal.

- [ ] **R1018.** A live camera can be a screen at a place or a calibrated viewpoint. Projecting its image into geometry requires calibration, time alignment and visibility/occlusion handling; a URL alone does not supply these.

- [ ] **R1019.** Keep observation, object tracking, reconstructed geometry and accepted operational changes separate. A disappearing detection can mean occlusion or tracking failure, not consumption, destruction or sale.

- [ ] **R1020.** Coordinate reconstruction with `Computer-Vision-Lab.md` (previous path; locate the current merged specification before relying on it). Require replay fixtures, calibration and explicit source demand before any live reconstruction integration. Shared stream contracts can be built without promising online reconstruction of whole cities.

- [ ] **R1021.** Allow specialist solvers to consume explicitly mapped, versioned inputs and return typed results with units, model identity, assumptions, coverage and execution status. They must not write live accepted state directly from a scenario.

- [ ] **R1022.** Distinguish Lince's deterministic workflow/resource simulation from thermal, structural, fluid, traffic or equipment simulation. Their solver validity, step coordination and calibration require separate evidence.

- [ ] **R1023.** **Proof:** loss of a feed shows stale/unavailable state without deleting an asset; source motion does not become stock motion; an interrupted specialist run cannot publish a complete forecast; simulations cannot issue live commands.

### Interoperability: choose by purpose

- [ ] **R1031.** Geospatial sensor observations — **Candidate and documented role:** [OGC SensorThings](https://www.ogc.org/standards/sensorthings/) connects sensor observations and metadata — **Proposed boundary in Lince:** Map observations, features, units and source times; keep acceptance policy in Lince.

### Civic workspaces and public commerce need real governance

- [ ] **R1039.** The proposal is an official Organ operating a shared workspace with layers whose authors and authority are visible. It can publish selected accepted Records while citizens contribute evidence and propose changes through scoped workflows.

- [ ] **R1040.** Reusable requirements are proposal target/revision, proposed patch or Transfer, actor, evidence, affected area, reviewers, review result, reason and application receipt. Reuse existing proposal paths where they support these requirements. Do not assume existing Transfer-specific sharing already provides general civic proposal sharing.

- [ ] **R1041.** Allow official, operator, community and individual claims to coexist. A review can accept, reject, request evidence or identify a duplicate. Duplicate reports can share an issue without exposing private reporters. Display status and reasons publicly to the extent permitted. Evidence retention and moderation belong to the workspace's explicit policies.

- [ ] **R1042.** A citizen might propose a repair, identify an accessibility barrier, offer materials or propose a resource Transfer. Approval of the report does not necessarily approve spending, procurement, land access or physical execution; those decisions use their own responsible actors and commitments.

- [ ] **R1043.** Public commerce should expose useful terms and availability while minimizing disclosure of private inputs. Support revision-bound offers, units, quality/specifications, delivery/place windows, partial fulfillment, cancellation, returns/disputes and settlement status using the same resource and Transfer semantics. Public search and ordering need quotas and abuse handling so they cannot reserve unlimited resources or enumerate private workspaces.

- [ ] **R1044.** Do not make Lince a full tax, payments, cadastral or railway-safety system as a hidden consequence of this proposal. Define export/connectors for those responsibilities when an actual deployment needs them. Lince can coordinate commitments and record evidence without claiming it legally proves title or replaces the responsible authority's decision.

### Criticism and improvements to the original idea

- [ ] **R1045.** **“A representation of the present moment” is too strong.** Different sources arrive at different times and can disagree. Improve it to an inspectable accepted state with source ages, plus observations and scenarios that remain distinguishable.

- [ ] **R1046.** **Operational correctness needs its own proof.** Personal city navigation and room modeling can be useful before stock workflows exist. When adding operational use, prove stable identity, resource accounting, permissions and reconciliation with a table and small fixture; visual realism cannot establish them.

- [ ] **R1047.** **Open data should be selectable.** Publish offers, public works and aggregate capabilities without requiring exact stock, contracts, private interiors or camera footage to be public. Make publication a reviewable view with an explicit license.

- [ ] **R1048.** **A dependency graph is insufficient for supply-chain prediction.** Add buffers, lead times, yields, capacity and substitutes. Show missing data and reasons rather than promising consequences from any linked Record.

- [ ] **R1049.** **Do not over-generalize every business rule into a new language.** Reuse bounded definitions and existing Rule/Command machinery. Extract a shared primitive after the factory and civic cases demonstrate the same need.

- [ ] **R1050.** **Do not make every scene element a Record automatically.** Keep static details in asset metadata. Create or link Records for objects people coordinate, query, own, maintain or attach evidence to; expand semantic coverage when needed.

- [ ] **R1051.** **Camera detail can increase exposure without increasing operational certainty.** Begin with recorded evidence and calibrated viewpoints. Add live feeds for a named purpose with retention and access policies.

- [ ] **R1052.** **A city should not require a single giant trusted workspace.** Permit independently governed layers and partial federation. One Organ can certify its decisions without owning everyone else's observations.

- [ ] **R1053.** **“All workflows” needs an extraction test.** A foundation earns its complexity when two distinct workflows use it without duplicating backend invariants. Until then keep the domain detail in a preset or adapter.

- [ ] **R1054.** **3D must earn its interface cost.** It helps locating equipment, reviewing a spatial conflict or understanding a route. Receiving goods, approving terms and resolving an evidence conflict usually also need fast forms and tables.

- [ ] **R1055.** Suggested additions to test the breadth of the base: preventive maintenance, lot recall, waste/by-product offers, accessibility reports, shared equipment booking and utility outages. These stress the same resource, evidence, capacity and spatial contracts without requiring six new domain engines.

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Combine a selected city/site, an operational workflow and a public civic projection with two source authorities and mixed online/offline demand. Exercise real find/inspect/propose/review/publish actions under concurrent edits/downloads; compare fairness and cost against the isolated workflow baselines.

- [ ] **R0570.** **[stage 14](14-stage.md) — L0–L5 core composition proof** — **Intention and defined deliverable:** Independently governed infrastructure and mature core workflows compose without duplicate foundations — **Correctness/failure evidence and performance obligation:** B14/B19/B20 and core B33–B36: partial authority/coverage, mixed offline demand, reusable commands, source rights and measured fairness versus isolated baselines. Existing or imported results are honestly labeled; no new scientific run or coupled physical solver is needed.

- [ ] Run the actual native/backend/persistence/transport path for every 14.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0669.** people inspect local accepted state, model a proposed geometric/operational change, compare supported bounded resource scenarios and submit a reviewable proposal without a giant trusted world model or hidden live side effects. Personal offline modeling remains useful independently. New physics/material accuracy is neither claimed nor required; [stage 15](15-stage.md) stabilizes the delivered core next.

- [ ] Advance only when **all 14.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
