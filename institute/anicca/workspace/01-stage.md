# 01 — Fixtures, budgets and delivery contracts

Finish with a reproducible implementation brief: the existing native canvas still runs, its current Areas/gravity/motion cooling have a measured baseline, and the future scale/hosting journey has inspectable bounded fixture data and an exact replay script. This stage prepares evidence and scope; it does not claim that the planned spatial engine already works.

## Required workflow milestones

- [ ] **01.M01.** Open the current native workspace through the repository run workflow; exercise an existing Area, move a Sand and let existing motion cooling settle. Capture behavior, hardware, idle work and current failure cases before changing architecture.

- [ ] **01.M02.** Inspect the supplied reference dataset without a private home upload or whole-city download: a coarse globe, optional outer context, a street, an entered building, a 40 mm ball and mounted-content locations. Define the inward/outward journey and the independent non-geographic equivalent.

- [ ] **01.M03.** Write the exact native actions, expected outputs and resource/error thresholds for the local host, one identified editor and one public visitor. Distinguish future missing features from current regressions; every unknown has an owner and a stage.

## Entry and scope

- [ ] Start from [00-General.md](00-General.md); this stage prepares fixtures/contracts only. Confirm existing task authorization before application implementation, summarize the first slice for owner refinement and preserve all unrelated working-tree changes.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/xtask/`; `crates/interface/src/native/`; `crates/engine/src/workspace_sync.rs`; `crates/transport/tests/workspace_live.rs`.

## Ordered implementation tasks


- [ ] **01.T01.** Re-audit the baseline paths and record actual tests/behavior, current canvas unit mapping, asset delivery, accepted revisions, permissions and simulation isolation. Identify reuse versus missing contracts before adding services.

- [ ] **01.T02.** Select tiny public/synthetic core fixtures B25–B28/B38 with declared sources/frames and supported operations: solar/Earth/street/supplied-house/40 mm ball, authored relief, blank grid, independent room and sparse invented non-geographic scene. Choose coarse Earth layers, core resource profile and actual hardware before coding; private home data or a whole-city download is not required. Larger room/cave/city coverage remains stages [06](06-stage.md)–[09](09-stage.md) acceptance. Keep the future B07–B12/B16–B18/B21/B37 science requirements in the catalogue; selecting physical datasets, protocols, numerical tolerances and workers is deferred to stages [16](16-stage.md)–[21](21-stage.md).

- [ ] **01.T03.** Refine the core-applicable C01–C17, K1–K8, I1–I6 and E1–E8, directory-format minimum and initial scalar/ID/frame encodings. Freeze the small native edit/import/save/compare flow with automatic ready application and selected/local-variant scope. Preserve explicit future material/laboratory output slots in the requirements without implementing unused schemas or evaluators. Make the core fixture inspectable by hand; stages [16](16-stage.md)–[17](17-stage.md) select the later paired numerical proof.

- [ ] **01.T04.** Freeze H1–H10 and B33–B36 as phased core hosting specifications: host, identified editor, restricted public visitor, separate source Organ, different detail budgets, partial private/public assets, revoke and host loss/restart. Record entry/status examples, counters, authority selectors and existing-module extensions. B37 preserves the deferred numerical participation contract without running or selecting an experiment at [stage 01](01-stage.md). Public guest/model streaming are proposed capabilities, not claims of current behavior.

- [ ] **01.T05.** Specify B38 alongside B25: one complete room root and a tiny sparse invented non-geographic structure with arbitrary endpoints and a comparable scale ratio. Declare frame/unit/precision, representation/error, navigation/up and resource expectations independently of Earth identifiers, a fixed ball diameter or map providers. Select the small parameters and metamorphic comparisons before implementing stages [03](03-stage.md)–[05](05-stage.md).

## Worked contract example

- [ ] **01.X01.** Turn this proposed budget/replay skeleton into measured fixture specifications before dispatch. The numbers are engineering targets, not observed results; the actions require native workflow evidence in their delivery stages. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn main() {
      let coarse_earth_bytes = 32_u64 * 1024 * 1024;
      let fixture_limit_bytes = 64_u64 * 1024 * 1024;
      let automatic_cache_limit_bytes = 128_u64 * 1024 * 1024;
      assert!(coarse_earth_bytes <= fixture_limit_bytes);
      assert!(fixture_limit_bytes <= automatic_cache_limit_bytes);
      let required_actions = [
          "inspect_ball",
          "leave_building",
          "walk_street",
          "view_globe",
          "view_optional_outer_context",
          "return_to_same_ball",
          "repeat_without_geographic_profile",
          "revoke_guest",
      ];
      assert_eq!(required_actions.len(), 8);
  }
  ```

## Architecture and task requirements

### The intended system and the first useful result

- [ ] **R0004.** Lince workspaces let people author, import, inspect, combine and simulate models from microscopic structures through materials, pieces, assemblies, buildings, streets, cities, planets and astronomical systems. Smaller supported descriptions can include isotopes, particle populations or physical fields when their domain requires them. Atomic balls are one representation, not the smallest compulsory building block of the file format. Arbitrary addressable extent does not imply populated content, available knowledge or a valid solver at every scale.

- [ ] **R0005.** The existing workspace is the unit of use: it can hold flat native content, model documents, independent laboratory roots, spatial views and referenced data from other Organs. Hosting exposes an authorized projection of those same contents to participants. Modeling, ordinary work and multiplayer share persistence, accepted edits, source access, demand and resource policy; no separately named product or compulsory geographic/world root is introduced. Each participant has independent views and local interaction state while accepted shared scopes keep their owning authority.

- [ ] **R0006.** Owner clarification, 2026-10-20: scale handling is scene-independent. The ping-pong-ball/Earth/solar-system journey is a reference acceptance example whose required capabilities must work together; its objects, hierarchy and endpoints are not engine facts or fixed zoom stages. The same frame, precision, representation, query, navigation and demand contracts apply to a room-only scene, an invented assembly spanning comparable scale ratios, a microscopic root or another authored world. Scene extent and supported precision/resource profiles bound actual operations; no chosen example establishes a universal smallest object, largest scene or required root. Earth remains available as the geographic preset/data choice already agreed, without allocation in an unrelated scene.

- [ ] **R0007.** The priority is an offline personal world: very coarse Earth context, a navigable city, and detailed attractive places the user cares about. Home, work, university and community spaces can have authored interiors, material assignments, mounted Sands and useful behavior while the rest of the world remains simple. Fictional places and alternative designs have the same authoring capabilities. Earth is an optional reference system, not a required root for every model.

- [ ] **R0008.** Both directions of work are first-class. A person can assemble a wall from an empirical material and measured dimensions, then add finer evidence later; another can study a crystal and explicitly derive one property used by that wall. Zooming never invents atoms, unseen rooms, survey accuracy or material behavior. Refinement can add geometry, knowledge or a better physical model independently.

- [ ] **R0009.** The architectural goal is a small common model language plus domain implementations. Geometry, material science, CAD, geography, biology and operational workflows share identity, units, frames, selections, dependencies, revisions and results. They retain the representations and algorithms their mathematics requires. A general field interface does not make a molecular force field, a rigid-body force, an electromagnetic field and a stock balance interchangeable.

- [ ] **R0010.** Owner-confirmed delivery emphasis, updated 2026-10-21: shared foundations → early scene-independent sparse scale/navigation and hosting → offline locality, authored rooms/caves, low-poly optimization, source comparison, CAD/assemblies, structural microscopic editing and operational composition → stabilization gate → deferred material-property evaluation, laboratory experiments and new physical simulation. The paired direct-material, micro-derived-model and independent protein workflows remain required in the deferred tail and share the same implementation. [stage 05](05-stage.md) still proves the optional solar-system/default-Earth/street/house/40 mm ball example and B38 independent scenes early. Default ground has no relief and absent building geometry stays absent until supplied data or user authoring changes it; the globe itself remains curved. Generic behavior must support interactive authored experiences without requiring a separate game architecture.

### Confirmed intent and context recovery

- [ ] **R0019.** **C08 — Completion requires intent and evidence.** Every implementation slice proves the relevant user action, backend invariant, native UI, failure handling and resource behavior. A generic extension point or a favorable library benchmark never completes a domain feature.

### Source baseline and shared families

- [ ] **R0068.** This inspection establishes reuse candidates and present limits. Recheck before coding because the working tree is shared and other work is changing it. No existing subsystem is certified for the larger scope solely by its name.

- [ ] **R0082.** The shared families proposed by this pass are: durable model objects and occurrences; frame/unit transformations; spatial support and named selections; typed quantities and fields; domain relations and constraints; revisioned derivations; representations and detail; and isolated runtime state. The following contracts define their members and behavior before project code changes.

- [ ] **R0083.** Hosting reuses these families through participant/session authority, permitted query projections, bounded asset delivery and activities. Its H1–H10 requirements attach to L0–L5 and the existing engine/transport/native ownership below. They are cross-boundary contracts, not another model layer or a universal collaboration service outside current workspace policy.

### Performance research and adaptation boundaries

- [ ] **R0533.** Research checked 2026-10-18. The recommendations below infer useful design boundaries from primary documentation/source; they do not select a replacement database/framework, import vendor benchmarks as Lince targets or authorize copying a codebase. Preserve the native Rust direction and existing accepted-state/asset contracts unless a measured stage proves a missing mechanism.

### Adapter conformance record

- [ ] **R0550.** Every selected adapter records: supported schema/operations; exact software/build/license/target platforms; units/frames/conventions; source and subelement identity; unsupported/loss behavior; deterministic/tolerance reproducibility; resource/cancellation guarantees; failure and crash recovery; and reference fixtures/results. “Library supports X” is not a completed Lince capability until persistence, native interaction and this record pass.

- [ ] **R0551.** Choose one implementation per initial purpose. A renderer, geometric kernel, mesher and numerical solver solve different problems; selecting all of them at once is unnecessary. Time-box a candidate evaluation to the fixture list and make a recorded selection or fallback decision. A failed optional dependency must not invalidate a saved document or prevent inspecting retained results.

### Slice contract and evidence ledger

- [ ] **R0555.** Before coding a slice, instantiate its work card in this document with: **intent and exact user action; C/K/I/E/H requirements; inputs and units/frame/revision; accepted outputs and native states; owning module/authority; required/deferred profile operations; dependencies; fixture and reference result; numeric performance/error/resource thresholds; failure/recovery cases; checks/observed evidence; participant/source authority, activity location and resource payer where hosted; and remaining decisions**. Use existing stage tasks and requirements as the specification, not a newly invented summary. A library/API/empty UI scaffold is not completion.

- [ ] **R0556.** The stage registry starts Pending for core delivery and Deferred for the numerical tail. Move to In progress only within agreed implementation authorization, then Verified or Blocked with concrete evidence/reason; retain Deferred for unstarted future scope. Record a completed capability and proof location before removing its implementation checklist, so the remaining document does not lose the governing invariant. The ledger does not add a new permission prompt for routine authorized work. Never mark a feature built solely because a test stub, generic profile slot or tiny unrelated benchmark passes.

- [ ] **R0578.** Each stage has an intention regression: preserve at least one relevant earlier user flow while adding the next shared capability. Broaden checks only for actual changes/failures/unresolved concerns. Performance runs must record build/profile/device/data/accuracy and compare like for like; vendor throughput numbers are not thresholds. Failures change the implementation or explicitly agreed budget, not the meaning of the feature.

### Owned fixture specifications

- [ ] **R0724.** These are synthetic/public-test proposals; a real personal place is an additional acceptance fixture, not a prerequisite for reproducible tests.

### Fixture phases and the stabilization boundary

- [ ] **R0775.** Proposed baseline profile: Linux desktop/laptop with four CPU cores, 16 GiB system RAM, integrated/shared-memory graphics, 1280×720 model viewport and SSD. It is a test profile, not an assumption about the owner's hardware. [stage 01](01-stage.md) records the actual device, graphics driver, build/settings and a baseline before changing anything. Add a lower-memory profile only after measuring the first one.

- [ ] **R0776.** Larger offline city profile — **Proposed starting value:** Proposed ceiling 8 GiB selected/pinned content and a separately selected cache cap up to 2 GiB — **Enforcement/evidence:** An optional larger profile for explicit region saving, not an automatic default allocation/download. [stage 05](05-stage.md) uses the much smaller starter/cache profile below; choose coverage/detail from actual bytes

- [ ] **R0777.** Model CPU residency — **Proposed starting value:** 1 GiB incremental model/view assets and indexes, excluding explicitly launched scientific workers — **Enforcement/evidence:** Measure decoded data plus job/staging peaks; report total application memory separately

- [ ] **R0778.** Model GPU residency — **Proposed starting value:** 512 MiB managed model resources — **Enforcement/evidence:** On integrated graphics this also consumes shared RAM; budget both views together, not once per view

- [ ] **R0779.** Visible frame latency — **Proposed starting value:** p95 ≤33.3 ms, p99 ≤50 ms at the baseline viewport — **Enforcement/evidence:** Target for the documented navigation fixture, not a universal performance claim; record CPU/GPU timing and visual settings

- [ ] **R0780.** Direct interaction — **Proposed starting value:** p95 ≤100 ms visible response; ≤200 ms for small validated commits, excluding declared background computation — **Enforcement/evidence:** Preview and commit measured separately; queue/start/finish status for expensive work

- [ ] **R0781.** First useful view — **Proposed starting value:** ≤3 s cold local open, ≤1 s warm after the required root/index is available — **Enforcement/evidence:** Display useful coarse content without waiting for optional refinements; record asset and OS-cache conditions

- [ ] **R0782.** Job scheduling — **Proposed starting value:** Initially two heavy background jobs, one expensive decode per source, bounded uploads per frame — **Enforcement/evidence:** Byte/upload/tile caps derive from the fixture; preserve input responsiveness and fair queues

- [ ] **R0783.** Cancellation — **Proposed starting value:** Local UI acknowledgement ≤100 ms, obsolete dispatch stops ≤250 ms; cooperative workers yield/check ≤100 ms — **Enforcement/evidence:** Noncooperative kernels have a process timeout/termination policy, proposed 2 s; pending termination is visible

### Verification obligations

- [ ] **R0814.** Correctness combines geometry/topology validity, stable identity, units/frame/clock transformations, material applicability, mapping, exact accounting, revision conflicts and actual user interactions. Use property/metamorphic tests where meaningful: unit changes preserve physical predictions; rigid frame changes preserve lengths; view magnification does not change physics; permutation preserves identity; equal/opposite additive effects cancel; display simplification does not change accepted mass/stock; origin shifts preserve canonical state.

- [ ] **R0821.** Security checks target actual boundaries: denied edits across UI/API/import/automation; private metadata/index/result disclosure; stale caches after grants change; source/observation forgery; bounded parsers/archives/decoders; integer overflow in shape/offset math; traversal/symlink/duplicate-entry handling; cyclic expansion; unintended network requests; credential scope; executable worker capabilities; cancellation and quota exhaustion. Retained caches/results respect their own rights. Simulations cannot emit operational Actions into the live Cell.

- [ ] **R0822.** Performance separates steady-state frame time, input/commit latency, streaming/decode/upload, queries, save/recovery and solver throughput. Record p95/p99, cancellation latency, CPU/RAM/VRAM, bytes read/transferred, peak staging and released demand/resources after closure. Test one dense room, a sparse city, many instances, overlapping appearance layers and a long navigation session separately.

- [ ] **R0827.** Run only checks appropriate to changed contracts; retain reproducible cargo xtask/native replay fixtures and inspect rendered room/cave outputs where appearance matters. A headless math test does not establish low-poly visual quality or usable mounted input. Every stage leaves a fixture/result and any blocked or unsupported scope in this plan.

### Subsequent fixture and adapter decisions

- [ ] **R0850.** Name the real city/site, target device and storage/error profile replacing public/synthetic B02/B03 and provisional budgets. Define the desired ordinary-use and laboratory workloads independently.

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Prepare home/work/university/community fixture locations and a street route with supplied low-poly landmarks, plus an invented scene using unrelated roots/units. Name data rights, packed/decoded sizes and the active subset to be used by real navigation/editing; an arbitrary batch of colored squares is not this workload.

- [ ] **R0557.** **[stage 01](01-stage.md) — L0 and contracts** — **Intention and defined deliverable:** Freeze bounded core native/scale/hosting actions, hardware and source rights; retain future science as deferred scope — **Correctness/failure evidence and performance obligation:** Record pre-change timing/memory/wake-up baseline and core budgets. B01/B03/B05/B13/B15/B19/B20/B22–B36/B38–B40 are used in the stage-specific subsets below. Select B25–B28/B38 small data; B37 and the numerical portions of B07–B09/B11–B12/B16–B18/B21 are catalogued but their scientific datasets/protocols/limits wait for stages [16](16-stage.md)–[21](21-stage.md). No material/lab implementation or dataset selection blocks [stage 01](01-stage.md).

- [ ] Run the actual native/backend/persistence/transport path for every 01.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0585.** executable fixture specifications and a written implementation scope, not a months-long schema project. Unknown data rights, unit conversions or worker trust are recorded decisions; they do not become silent defaults.

- [ ] Advance only when **all 01.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
