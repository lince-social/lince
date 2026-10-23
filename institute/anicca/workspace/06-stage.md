# 06 — Offline city coverage, package closure and bounded streaming

Finish with an offline personal locality: coarse global context, navigable selected city coverage and detailed favorite places coexist in one workspace. The user can see what is available, save a region, search/bookmark places and enter a supported place without downloading or allocating the whole city.

## Required workflow milestones

- [ ] **06.M01.** Save a chosen city/route/place profile within an explicit storage budget, go fully offline and navigate between meaningful saved locations and supplied/authored interiors. Missing optional detail remains visible as partial coverage without starting network requests.

- [ ] **06.M02.** Find and revisit custom landmarks through search/bookmarks/map and street-level views. Labels, styles, transforms and permitted model dependencies needed by the selected offline profile are included; a missing real height/building source is not fabricated.

- [ ] **06.M03.** Export a selected authorized portable workspace snapshot, import it on another admitted client and compare with the directory form. Resume an interrupted transfer and refuse mixed-rights/private whole-pack disclosure.

- [ ] **06.M04.** Navigate for the agreed long session, reverse/cancel requests and reconnect a slow visitor while continuing small edits. Cache, queues, decoded/upload peaks and persistent download counters remain bounded and the same loader serves local and hosted use.

## Entry and scope

- [ ] Confirm the preceding [stage 05](05-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 05 summary](05-stage.md#completion-summary); [stage 04 summary](04-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/map/`; `crates/interface/src/native/map_castle/`; `crates/engine/src/blob_sync/`; `crates/transport/`; `crates/xtask/`.

## Ordered implementation tasks


- [ ] **06.T01.** Extend [stage 05](05-stage.md)'s shared map/archive provider, source/coverage and residency path to selected real city data and a larger offline profile. Keep default coarse Earth available and elevation, imagery, routes, supplied 3D models and authored places separate. Add declared elevation/building/3D source adapters that preserve supported height/geometry and report losses; do not route required shape data through the current basemap normalization that drops buildings. Preserve flat ground/footprints where geometry is absent; no procedural relief or building extrusion is inferred from a place name.

- [ ] **06.T02.** Extend region/layer budgets, cost preview, offline pins, partial coverage and progress/cancel/resume. Online on-demand fetching is allowed only through configured sources and budgets; explicit offline mode prevents new network work. Include styles/labels/transforms/available shape data required by the chosen package. Reuse [stage 05](05-stage.md)'s loader/query/demand implementation instead of a second map cache/runtime.

- [ ] **06.T03.** Implement paged spatial/semantic indexes, coarse-parent fallback, permitted dedupe and bounded decode/upload/request queues. Add local place search/bookmarks and saved-route inspection; route solving remains a separately tested subset when selected.

- [ ] **06.T04.** Reuse the E4 coherent query/projection cache and I6 demand leases for the selected map/table subset: initial snapshot, atomic revision deltas, overlap/refcounts, grant changes and reconnect/resnapshot. Prove B19 before widening predicates; do not introduce another general database or per-feature polling loop.

- [ ] **06.T05.** Add the portable `.lince-workspace` import/export slice, selected-profile dependency closure, attribution/loss reports, canonical vectors and cargo xtask inspection/validation. Prove directory/packed equivalence, partial transfer/resume and B15 hostile-file rejection before range-streaming the packed form.

- [ ] **06.T06.** Prove 30-minute navigation under the default working-set cap, rapid request cancellation, offline closure, missing-tile fallback, disk-full behavior and resource release. Do not claim building/terrain detail from current normalized basemap tiles.

- [ ] **06.T07.** Extend B33–B36/H3–H5 to selected-profile offline publication and hosted packed/chunk delivery. Verify access-compatible payload partitioning/projections, mandatory/optional closure, transfer/artifact hash binding, finite versus unknown external totals, actual/estimated accounting and persistent caps through 30-minute reconnect/slow-reader sessions. Expand participant load only against measured host-wide budgets and existing admission guards. A pack mixing denied data cannot be served whole to satisfy a partial view.

## Worked contract example

- [ ] **06.X01.** Extend reservation arithmetic through the actual encoded/decoded/disk/in-flight and persistent session counters. Reconnect cannot reset spending; unknown-size sources need bounded reads and cannot bypass offline mode. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn reserve_request(offline: bool, spent: u64, in_flight: u64, next: u64, cap: u64) -> bool {
      if offline {
          return false;
      }
      spent
          .checked_add(in_flight)
          .and_then(|total| total.checked_add(next))
          .is_some_and(|total| total <= cap)
  }

  fn main() {
      assert!(reserve_request(false, 40, 10, 50, 100));
      assert!(!reserve_request(false, 40, 10, 51, 100));
      assert!(!reserve_request(true, 0, 0, 1, 100));
      assert!(!reserve_request(false, u64::MAX, 1, 0, u64::MAX));
  }
  ```

## Architecture and task requirements

### Confirmed intent and context recovery

- [ ] **R0022.** **C11 — Sparse demand and durable offline data.** Unneeded heavy RAM/GPU payloads release under bounded demand/grace, while small required indexes, durable edits, a bounded compressed disk cache and saved offline packs remain. Configured sources may fetch needed detail online within budgets; offline mode prevents new requests. No addressable extent implies resident content.

### Source baseline and shared families

- [ ] **R0074.** Mapping: `crates/map/src/{source,geometry,scene}.rs`, native `map_castle/`, offline map tests — **What to reuse:** Local/archive sources, PMTiles, bounded tile/download/cache controls, geographic context — **Required extension or boundary:** Current normalization removes buildings and caps data zoom at 14. Elevation, detailed buildings, semantic city packages and offline route solving require their own inputs and proofs.

### Encoding and portability

- [ ] **R0225.** Use standard ZIP64 as the leading packed-container candidate, with opaque hash-named entries, a bounded manifest/directory and independently stored or compressed payload chunks. Never require decompressing the whole world to inspect a room. Directory access is implemented first; packed range reads, duplicate/path checks and large-offset validation must pass before remote streaming uses the packed form. Existing PMTiles/other indexed archives can remain separate payloads.

- [ ] **R0226.** A packed export is an immutable selected snapshot. Include the complete required dependency closure for its chosen offline profile and a report of optional, omitted, restricted or unavailable data. An external URL is not offline availability. A descriptor cannot secretly fetch dependencies when explicit offline mode is enabled.

- [ ] **R0228.** Bound entry count and directory bytes per packed file. A very large snapshot can reference several immutable bounded packs through its paged catalogue; it must not require reading an unbounded central directory to locate one room. Pack references are explicit hashed dependencies, not recursively auto-extracted archives. User-selected offline export either includes its required packs/chunks within the accepted budget or reports the unsatisfied closure before claiming completion.

- [ ] **R0229.** Do not introduce a new general binary container before the directory schema and real access patterns are proven. If ZIP64 fails the fixtures, record the measured reason and specify a seekable replacement at that gate. [OpenUSD's layers, references and deferred payloads](https://openusd.org/release/intro.html) are useful composition precedents; an optional USD adapter remains valuable. The native format additionally needs Lince identity/policy, scientific validity and offline closure contracts that adopting a scene format alone does not establish.

### Domain workflows as implementations of the primitives

- [ ] **R0366.** City/world — **Authored/source support:** Geographic features, source packages and nested sites — **Specialized operation/constraint:** CRS conversion, spatial partition and quality-driven refinement — **Shared outputs and reuse:** Bounded offline representations, semantic queries and detailed-place instances

### Offline world and city pipeline

- [ ] **R0494.** Use a sparse hierarchy of spatial indexes and representation tiles, with independent semantic indexes. Existing map tiles provide context; elevation, imagery, building footprints/heights, detailed interiors, routes and authored sites are distinct layers. A coarse basemap cannot supply terrain heights or a metric interior. Public Earth context can coexist with private homes and fictional overlays under separate grants.

- [ ] **R0495.** [stage 05](05-stage.md) starts with a tiny default available coarse Earth package and supplied test-site data, plus optional solar context. [stage 06](06-stage.md) expands selected real city coverage and explicit high-detail place packages through the same loaders. Missing height/volumes/interiors remain flat/absent until data or authoring supplies them; map labels and zoom never invent geometry. A package records source/revision/capture date, CRS, spatial/time coverage, achieved resolution/error, completeness, rights, required transforms/fonts/styles and dependencies. Placeholders can indicate missing content; generated approximations are labeled authored/estimated and never presented as observations.

- [ ] **R0496.** Reuse PMTiles/local archives where they fit; [PMTiles supports indexed tile pyramids and range access](https://docs.protomaps.com/pmtiles/). A source's permitted retrieval/export and attribution must be checked before city extraction; do not bulk-scrape a tile service simply because its endpoint is public. Source/dataset licenses travel with cached/exported subsets.

- [ ] **R0497.** Offline city navigation includes the existing flat map view, globe/street/interior spatial navigation where supported content is present, local place search/bookmarks and saved routes. These share accepted place/model references; the flat renderer is not the sole navigation experience. If turn-by-turn/walking route solving is selected, include a routable graph, turn/access restrictions, elevation/accessibility where available and a local routing adapter; map tile residency alone does not provide routing. Show graph freshness and unsupported paths. GPS/location availability is independent of Wi-Fi and map download state.

### Performance research and adaptation boundaries

- [ ] **R0539.** [SpacetimeDB scope/persistence](https://spacetimedb.com/docs/intro/what-is-spacetimedb/) — **Documented mechanism:** In-memory application state, a durable commit log and interested-client mirroring — **Concrete adaptation and limit in Lince:** Keep the active metadata/index working set responsive and recoverable. Lince's offline/sparse universe must not require all source assets or historical results in memory, a permanent server connection or one trusted global authority.

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0832.** **Q5 — Release active data; retain bounded offline data.** Release unneeded RAM/GPU resources under explicit bounded demand; retain a compressed disk cache and saved offline packs. Disk pinning is distinct from decoded/GPU residency, and durable edits are never discarded by view eviction.

### F9. Efficient city and site representations

- [ ] **R1007.** Keep large geometry/media out of frequently replicated Record rows. Records reference versioned asset manifests and bounded metadata; blob/asset delivery follows its own permissions and lifecycle.

- [ ] **R1008.** Partition by spatial bounds and useful semantic groups. Stream visible/needed tiles, use level of detail, instancing and simplified picking proxies, and evict unused CPU/GPU resources. Do not spawn an always-active entity and label for every object in a city.

- [ ] **R1009.** Query operational information independently from rendering. Aggregate map markers and status by scale, then disclose individual assets on demand. A stock ledger must open without its warehouse mesh.

- [ ] **R1010.** Publish patches or replacement tiles with a coherent manifest revision. Support cancellation, download resumption where available, checksum verification, bounded queues and invalidation when source assets change.

- [ ] **R1011.** Budget meshes, textures, splats, decoding, upload rate, tile requests, labels, camera feeds and per-frame CPU work independently. Fewer triangles alone cannot guarantee good frame times.

- [ ] **R1012.** Keep authored geometry and captured appearance as separate switchable layers. Avoid rendering both as competing surfaces without alignment/occlusion rules. Reconstruction artifacts need capture time and uncertainty.

- [ ] **R1013.** Validate coordinate correctness, semantic picking and retained identity alongside visual quality. A model that looks correct but selects the wrong warehouse is not acceptable.

- [ ] **R1014.** **UI:** layers, progressive loading, simplified fallback, visible source ages and resource diagnostics. Offer 2D and table workflows on hardware unable to display the detailed scene.

- [ ] **R1015.** **Proof:** a bounded neighborhood can be inspected while the rest of the city is unloaded; rapid navigation cancels obsolete requests; closing a view releases its resources; a hidden layer's metadata does not leak private assets.

### Interoperability: choose by purpose

- [ ] **R1028.** Urban semantic models — **Candidate and documented role:** [OGC CityGML](https://www.ogc.org/standards/citygml/) provides a 3D urban information model — **Proposed boundary in Lince:** Map appropriate city objects and attributes into shared spatial/identity contracts.

- [ ] **R1029.** Large geospatial scene delivery — **Candidate and documented role:** [OGC 3D Tiles](https://www.ogc.org/standards/3dtiles/) addresses streaming large 3D geospatial datasets — **Proposed boundary in Lince:** Implement a declared tiled delivery subset, independent of operational Records. Existing splat rendering does not prove tiled splat interoperability.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0727.** B02 — **Fixture:** Coarse world, a city index and a pinned detailed site, with one missing tile — **Proof required:** Offline closure, bounded active set, coarse fallback and meaningful coverage

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Plan and inspect a route between several saved places using real city feature/index data and a bounded detailed site, including one missing tile. Use the agreed candidate 100,000-feature index only through relevant search/navigation/edit actions and a bounded visible subset; record cold/warm/offline and 30-minute outcomes.

- [ ] **R0562.** **[stage 06](06-stage.md) — L0/L1/L3/L4, local-world L5 proof** — **Intention and defined deliverable:** Offline coarse world/city and a selected detailed package, with explicit residency and demand — **Correctness/failure evidence and performance obligation:** B02/B15/B19: coherent partial coverage, closure, reconnect/resnapshot, grants and packed/directory equivalence. Meet the local-open/navigation budgets or record an agreed revised profile; show decoded/upload/queue peaks and zero network in explicit offline mode. Extend B33–B36 to authorized packed closures and long hosted sessions, preserving source authentication, persistent download caps and bounded slow-reader reset/recovery.

- [ ] Run the actual native/backend/persistence/transport path for every 06.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0629.** an offline city and a high-detail place can coexist without loading the city at full resolution or making automatic network requests. Sparse extent alone does not increase resident cost.

- [ ] Advance only when **all 06.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
