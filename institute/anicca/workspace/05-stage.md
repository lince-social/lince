# 05 — Continuous sparse scale, navigation and hosted places

Finish with the first complete spatial journey. Inspect a ping-pong ball inside a supplied building, zoom out through its street and a simple Earth globe to optional outer context, then return to that same ball. The building was always located in the larger scene; Earth and the ball are fixture data, not fixed engine scale levels.

## Required workflow milestones

- [ ] **05.M01.** Travel continuously inward and outward through the ball/building/street/globe/optional-solar reference scene without a scene swap or identity/size change; repeat on an independent room and invented scene with unrelated roots and scale endpoints.

- [ ] **05.M02.** Switch Orbit, Fly and Walk explicitly while keeping supported pose/focus/selection. Walk through a doorway, look toward the horizon and read/use mounted Sands. Use the optional character/avatar presentation for that navigation, keeping its visual body separate from new physical dynamics. Unsupported or missing support stops/reconciles clearly instead of allowing penetration.

- [ ] **05.M03.** Walk past a supplied castle/custom structure at the location of a fixture house; see its silhouette stand out from the ground at the appropriate projected scale. Switch to a map/overview and find the same authored landmarks at home/work/university/community fixture locations, then return to the same occurrence. Full from-scratch authoring arrives in stage 07.

- [ ] **05.M04.** Place the same relief/green-mountain recipe in geographic and blank invented scenes, preserving source/custom layers. Missing terrain stays without local relief and missing building/interior geometry stays absent.

- [ ] **05.M05.** Host this journey for an identified editor and restricted public visitor with independent visual budgets/views. Apply an allowed local customization, use a Sand from another Organ with the visitor's own login, deny private content, hit a download cap and recover after host loss without replaying mutations.

- [ ] **05.M06.** Reverse the camera rapidly, close a view and revisit offline. Release unneeded RAM/GPU after bounded demand/fences; retain only bounded compressed cache, durable edits and selected offline packs. Record peak/steady costs against the starter profile.

## Entry and scope

- [ ] Confirm the preceding [stage 04](04-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 02 summary](02-stage.md#completion-summary); [stage 03 summary](03-stage.md#completion-summary); [stage 04 summary](04-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed)`; `crates/map/`; `crates/interface/src/native/topology/`; `crates/interface/src/native/map_castle/`; `crates/engine/src/blob_sync/`; `crates/transport/`.

## Ordered implementation tasks

- [ ] **R0607.** This stage implements the generic K2/K3/K7/I1/I6 scale contracts. B25 supplies the solar/globe/local reference example, B26–B28 its surface/navigation/resource cases, and B38 independent roots and a non-geographic scale range. Example adapters and dataset constants stay outside generic scale/refinement code. Preserve the required Earth/globe integration without imposing it on room, laboratory or invented-world workspaces.

- [ ] **05.T01.** Complete B25's tiny reproducible dataset: a default available coarse Earth ellipsoid/context, optional sparse solar-system package at a declared epoch, a bounded geographic test street, a supplied low-poly house with a passable doorway/interior, a 40 mm ball and mounted Sands. Keep real context, synthetic authored content and missing coverage distinguishable. The house/ball are supplied fixture assets through existing import/simple source operations; a full room/CAD editor is not a prerequisite.

- [ ] **05.T02.** Implement the body/context → globe → local-frame presentation bridge with source CRS/vertical metadata, explicit local units and unchanged object dimensions/identity. Prove default Earth remains useful when the solar package is added/removed, flat workspace/protein roots require neither, and adding an outer context does not rewrite site/ball data. Test known moving-frame transforms without claiming a live orbital model; optional scientific ephemerides/time/dynamics retain [stage 21](21-stage.md) requirements.

- [ ] **05.T03.** Render a true globe/horizon with global coverage including dateline/poles, and keep the current flat map as an optional view. Default local ground has zero added relief and missing building/terrain/interior geometry stays absent. Reuse map stores/provider contracts for contextual layers; do not make the current Mercator latitude clamp, zoom-14 source cap or screen camera the spatial model's limit. Validate the selected globe hierarchy, curvature/seams and declared reference conversions on small fixtures.

- [ ] **05.T04.** Implement the minimum common surface-source/layer path: constant fallback, one supplied elevation/mesh source and one user-authored deformation/model, exposed through bounded native numeric/formula and placement controls reusing the existing inspector/terrain command family. Give the same green mountain source a geographic anchor and a standalone flat-workspace occurrence. Preserve base/custom layer separation, additions/masks/replacements, bounds/correspondence, save/undo and generation-checked render/collision products. Complete the B26 subset here; complex sculpt tools and three-way re-import remain stages [07](07-stage.md)–[09](09-stage.md) work.

- [ ] **05.T05.** Add view-driven sparse refinement with cheap covering representations, camera-relative rendering, conservative frustum/horizon bounds, bounded labels, appropriate near/depth strategy and no per-object allocation for inactive content. Keep selected coverage visible while finer products load; do not load all siblings/layers or remesh Earth for a local edit. Publish cheap authored proxies/summary invalidation so changed relief appears at a meaningful projected scale before its fine geometry is needed.

- [ ] **05.T06.** Add explicit Orbit/Fly/Walk controls, target-based zoom/dolly and bounded adjustable movement speed in one scene. Preserve pose/selection across supported switches. Implement the bounded query-based Walk controller and ready support neighborhood required to traverse the street/doorway/interior; missing support, streamed replacement and changed-source penetration stop/reconcile visibly. General rigid/flexible simulation stays in stages [18](18-stage.md)–[19](19-stage.md) and is not inferred from this controller.

- [ ] **05.T07.** Make mounted content visible/readable/interactive as its projection grows through the existing surface/input/focus adapters. Faraway content uses bounded representations/markers; native render targets, media streams and detailed colliders need explicit demand. Show the same ball/Sand/object IDs in spatial, numeric and flat views, without geography-specific save/undo/selection implementations.

- [ ] **05.T08.** Enforce C07/C09–C12, E1–E8 and I6 for this slice: bounded active RAM/GPU/in-flight work; releasable decoded payloads; retained compressed disk cache/saved offline packs; one authority-aware on-demand provider path. When online, fetch needed allowed detail within visible budgets; offline mode prevents new requests. Cancellation, denied/missing data, failed saves, device loss and late generations preserve accepted sources and usable coarse coverage.

- [ ] **05.T09.** Replay B13/B23/B25–B28 inward and outward, cold/warm/offline, with rapid reversal, two views, custom bounds changes, poles/dateline, an oblique horizon view and the close-up ball. Record actual extent versus active bytes/work, queue/upload/decode counts, projected error/shape seams, input/frame percentiles, source accuracy, local dimensional error and release latency. Exercise sparse observable-universe-sized addresses without populated content, and prove measured costs track active data rather than address range.

- [ ] **05.T10.** Reuse immutable source/purpose meshes across repeated fixture occurrences where valid, retaining Bevy asset handles and updating placement/style separately. A local customization publishes dirty assets/bounds/ranges rather than reuploading the world. Run B30's bounded supplied-mesh/instance subset, recording unique geometry, per-instance bytes, uploads and two-view lifetime; general parametric surface planning is not a [stage 05](05-stage.md) prerequisite.

- [ ] **05.T11.** Complete the H3/H4 native entry card and host resource controls on the same scale fixture: known/unknown published and additional-download sizes, peak disk/RAM/GPU, continuing traffic/activity owner, supported profiles and external sign-in status. Add default cheap checks, an optional bounded trusted benchmark, explicit ready/adjusted/partial/refused states and continuing device/host/participant admission. Include changed manifests, inaccurate sizes, data/decode peaks, cancellation and persisted download counters. Use the existing sparse loader and configured network/offline policy; no second visitor cache or automatic provider/code installation.

- [ ] **05.T12.** Complete the B33–B36 hosted route: an identified remote editor and restricted public visitor share this same supplied street/house/ball/Sands with independent views/detail budgets; an accepted local customization arrives through a scoped delta/asset change. Reuse verified asset transport behind workspace-principal authorization and bounded control/asset/presence streams. Include a Sand using a separate Organ's public data and another requiring the visitor's own login; missing optional access stays localized. Exercise cap reached, slow reader, frame-origin changes, host disconnect/restart, cached viewing/local drafts, reconnect reconciliation, revoke and last-lease release. Record host and client results against the hosted starter subprofile; this proves hosting/navigation, not shared rigid-body science.

- [ ] **05.T13.** Prove B38 through the same frame/query/refinement/navigation/loader implementation as B25, with the geographic/astronomical profile absent. Parameterize hierarchy bounds, object identities, local units, error and outer context; test forward/reverse navigation, independent view origins, exact local dimension preservation, source/summary changes and last-demand release. Keep the synthetic scene within the same admitted small-fixture budgets, reusing source assets. No name-based dispatch, fixed Earth-relative zoom thresholds, compulsory parent world, global fixed geometric epsilon or hidden provider request may make the test pass.

## Worked contract example

- [ ] **05.X01.** Drive purpose-specific refinement from supported projected error, conservative bounds/near policy and admitted demand, never a hardcoded Earth/ball zoom ladder. This estimate is display error, not source survey accuracy or scientific fidelity. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn projected_error_px(error_m: f64, distance_m: f64, height_px: f64, fov_y: f64) -> Option<f64> {
      if ![error_m, distance_m, height_px, fov_y]
          .iter()
          .all(|v| v.is_finite())
      {
          return None;
      }
      if error_m < 0.0
          || distance_m <= 0.0
          || height_px <= 0.0
          || !(0.0..std::f64::consts::PI).contains(&fov_y)
          || fov_y == 0.0
      {
          return None;
      }
      let pixels = error_m * height_px / (2.0 * distance_m * (fov_y / 2.0).tan());
      pixels.is_finite().then_some(pixels)
  }

  fn main() {
      let distant = projected_error_px(1.0, 1000.0, 720.0, 1.0).unwrap();
      let nearby = projected_error_px(1.0, 10.0, 720.0, 1.0).unwrap();
      assert!((nearby / distant - 100.0).abs() < 1e-10);
  }
  ```

## Architecture and task requirements

### Confirmed intent and context recovery

- [ ] **R0018.** **C07 — Cost follows requested work.** An ordinary material dropdown or room view activates only its dependencies. Unopened microscopic payloads and inactive scientific adapters have no continuing compute demand. Astronomical address range does not imply populated or resident content.

- [ ] **R0020.** **C09 — Scene-independent scale, proved early.** Frames, units, local precision, projected representation choice, navigation and sparse demand work for any supported scene, with no fixed ball-to-solar endpoints, named zoom ladder or compulsory geographic hierarchy. [stage 05](05-stage.md) proves the shared implementation with the tiny B25 solar/globe/street/house/ball reference example and B38 non-geographic scene/room cases before expanding locality and authoring. Those examples establish required capability coverage, not engine limits or object types; later astronomy, materials and laboratories are not prerequisites.

- [ ] **R0021.** **C10 — Spatial navigation and local usability.** Explicit Orbit/Fly/Walk controls support oblique/horizon views, streets, supplied/authored interiors and progressively usable Sands. Mode changes preserve supported camera pose/context; geographic zoom is not limited to top-down maps or provider tile zoom.

- [ ] **R0023.** **C12 — Flat defaults and shared surface sources.** Earth is curved with zero added local relief by default; no missing mountains/valleys/building volumes/interiors are invented. Imported topography and user-made landscapes use the same support/field/operator/layer mechanisms. A standalone blank grid/fictional scene/protein root needs no geographic context.

- [ ] **R0025.** **C14 — Participation has an accountable cost.** Entry previews identify known/unknown download, disk, RAM/GPU, continuing traffic and computation for the selected permitted profile. Client-wide and host-wide/per-participant admission enforce continuing limits, including transient peaks and changed content. Optional preflight refines an estimate; it cannot certify arbitrary content or future performance. Cost follows requested data/work, not addressable extent or total workspace content.

### Proposed defaults for refinement

- [ ] **R0055.** Render precision and navigation — **Proposed default:** Evaluate `big_space` behind scene-independent Lince frames; continuous traversal of data-defined scales, explicit Orbit/Fly/Walk, projected-error detail and releasable residency — **When it may change:** Select the hierarchy/depth/controller implementation from B25–B28 evidence; no automatic mode switch

- [ ] **R0063.** Participant resources — **Proposed default:** Per-profile entry estimate and optional local benchmark, enforced by the same device/host/participant budgets throughout use — **When it may change:** Actual [stage 01](01-stage.md) hardware/fixtures select thresholds; unknown capability costs remain visible

### Source baseline and shared families

- [ ] **R0080.** Protein/Organ sessions: `crates/interface/src/native/protein_area/{sessions,login}.rs`, `sessions/tests.rs`; `crates/transport/src/session.rs` — **What to reuse:** Per-Organ shared login/subscription lifecycle, source-specific locked/disconnected states, no automatic mutation replay and independent unsubscribe — **Required extension or boundary:** Key reuse by the same participant/device authentication context; a host must not share its credentials across visiting principals. Add explicit public projection access where configured and preserve independently usable remote Protein queries.

- [ ] **R0081.** Asset transport: `crates/engine/src/blob_sync/{network,worker}.rs`, `crates/nucleus/src/blob_sync.rs`; `crates/transport/src/live.rs` — **What to reuse:** Verified transfer/storage, accepted-offer authorization, bounded manifests, rechecks during blob serving, existing Iroh/QUIC live transport and bounded queues — **Required extension or boundary:** Blob Sync is an offered file-copy protocol with peer/offer checks, not a workspace-scoped arbitrary asset server. The live loop refreshes bounded snapshots and serializes writes; add scoped range/chunk delivery and prioritized sparse streams with coherent revoke/recovery. Keep canonical artifact hashes and transfer hashes typed and verified separately.

### Capability coverage and reusable workflow recipes

- [ ] **R0114.** **W06 — Continuous sparse scale and offline locality** — **Shared primitives and thin specialization:** Shared frame/surface/representation maps, sparse indexes, camera modes, demand and package closure — **Minimum proof / later expansion:** [stage 05](05-stage.md)/B25–B28: solar/globe/street/interior/ball continuity; [stage 06](06-stage.md)/B02 expands real coverage. Blank and microscopic roots remain independent. B38 verifies scene-independent scale with different endpoints and no geographic profile.

### K7. Representations, quality and scale transitions

- [ ] **R0180.** One modeled object can have several representations for distinct purposes: editable source, measurement, render, picking, collision, solver discretization, aggregate properties and searchable semantics. Each descriptor references its source revision, support/frame, bounds, available capabilities, cost, error/uncertainty, rights and generation recipe. A representation is never promoted to a stronger purpose because it looks convincing. Cheap revisioned bounds/coarse summaries make authored changes discoverable while fine payloads are unallocated; summary invalidation is part of the accepted dependency path. [stage 05](05-stage.md) proves view-dependent scale continuity through these contracts before broad world/scientific datasets.

- [ ] **R0181.** Analytic curves/planes and polygon boundary meshes — **Strong use:** Rooms, direct low-poly/freeform editing, surfaces, runtime meshes — **Limits and required conversions:** Open/nonmanifold meshes may have no solid volume; check orientation, self-intersection and topology before physical use

- [ ] **R0182.** Feature history and boundary-representation solids — **Strong use:** Dimensioned mechanical CAD, exact surface relationships, manufacturing measures — **Limits and required conversions:** Kernel tolerances still apply; tessellate for rendering, mesh for PDEs, generate suitable contact proxies

- [ ] **R0183.** Height fields — **Strong use:** Large terrain and existing additive floor effects — **Limits and required conversions:** One height per chart position cannot describe caves/overhangs; add separate surface/volume supports

- [ ] **R0184.** Sparse implicit/volumetric fields — **Strong use:** Caves, density, porosity, sculpting, fluids or image volumes — **Limits and required conversions:** Cell/field resolution controls accuracy; extraction and collision are derived and need seam/mapping checks

- [ ] **R0185.** Atomic/molecular graph and periodic sites — **Strong use:** Chemistry, conformers, lattices, microscopic studies — **Limits and required conversions:** Bond display is not a complete electronic state or force field; preserve cell/symmetry/conditions

- [ ] **R0186.** Beam/shell/solid meshes, particles and discrete networks — **Strong use:** Structural, thermal, fluid, circuit and other specialist models — **Limits and required conversions:** Their domain and element/solver assumptions are explicit; a display triangle is not automatically a valid element

- [ ] **R0187.** Point clouds, images and Gaussian splats — **Strong use:** Captured appearance, evidence, reconstruction inputs — **Limits and required conversions:** Unknown/unseen space remains unknown; use measured/proxy geometry for support or collision

- [ ] **R0188.** Geographic features and spatial tile hierarchies — **Strong use:** World/city context and sparse streaming — **Limits and required conversions:** Source resolution, CRS and capture age constrain measurement and semantics

- [ ] **R0189.** Do not use a single scalar “LOD” as the whole policy. Independently control visual error, semantic coverage, physical/solver model, temporal rate, spatial extent and offline residency. Changing visual detail never silently changes mass, inertia, topology, accepted facts or an in-progress solver. Participants can choose different display representations of the same shared accepted/run state; changing a visitor's profile cannot change the authoritative collider or numerical model.

- [ ] **R0190.** Two distinct transitions require different evidence: **representation reduction** approximates a given source for a purpose; **physical coarse-graining** constructs an effective model with assumptions. A low-poly tree and a reduced biomechanics model can share a source but need different validation. Returning from a coarse representation restores retained detail; returning from a coarse physical model cannot reconstruct unique missing microscopic state without an explicit lifting/initialization model.

- [ ] **R0191.** Supported analytic/parametric geometry may compile to a backend-neutral display plan separating shared boundary sampling/connectivity from position/normal evaluation. Bounds, trim/source mapping, visual error, work limits and local numeric frame belong to that product. CPU and optional GPU implementations fulfill the same requested purpose with measured errors; neither changes accepted dimensions or becomes a collision/solver certificate. [Stage 10 CAD requirements](10-stage.md#architecture-and-task-requirements) details staged surface planning, retained occurrence assets and fallback; supplied meshes in [stage 05](05-stage.md) do not require implementing every CAD tessellator first.

### H3. Entry manifest, cost estimate and resource reservation

- [ ] **R0248.** Serve a small bounded entry description before downloading detailed content. A private description is available only after the appropriate admission; a public teaser contains only deliberately published fields. The description pins its workspace/document heads, host/admission epoch, authorized coverage, entry view, required/optional profiles and operations, selected dependency pages, payload identities and encoded/decoded size bounds, codecs, source/attribution references, external-connection needs and supported activity locations. Live credentials and session tokens remain outside immutable portable manifests. Signatures authenticate their stated issuer/content; they are not resource or safety certification. Page large catalogues and cost summaries rather than walking every world dependency to open an entry panel.

- [ ] **R0249.** The native entry card identifies **publisher-declared**, **locally verified**, **measured** and **unknown** values. Show these quantities separately:

- [ ] **R0250.** Published size — **Meaning and accounting rule:** Exact bytes only for a specified finite authorized snapshot/profile and dependency closure; changing external collections report known coverage and unknown/optional remainder

- [ ] **R0251.** Additional entry download — **Meaning and accounting rule:** Missing permitted encoded payloads for the selected profile, after verified local reuse; show metadata/transfer overhead separately and meter actual traffic/retries

- [ ] **R0252.** Optional/downloadable detail — **Meaning and accounting rule:** Higher representations, other regions, datasets and retained results; references to them do not reserve or download the whole collection

- [ ] **R0253.** Peak additional disk — **Meaning and accounting rule:** Retained new bytes plus unpacking, staging, temporary indexes and simultaneous replacement; account existing cache/pins and scratch separately without summing shared blobs twice

- [ ] **R0254.** Expected and capped RAM/GPU — **Meaning and accounting rule:** Selected decoded representations, instance/view data, queues, staging and simultaneous old/new products; shared-memory GPU usage is reflected in system accounting

- [ ] **R0255.** Continuing traffic and computation — **Meaning and accounting rule:** Asset-update/download caps, upload/rate limits, media/result rates, local/host/remote activity owner and requested worker budget; total duration-dependent cost is not disguised as a finite file size

- [ ] **R0256.** Capabilities and coverage — **Meaning and accounting rule:** Supported navigation/edit/result features, required installed adapters, unavailable optional features, and external Organ/source sign-in requirements

- [ ] **R0257.** Reserve local and host capacity before starting the corresponding work, through E1 rather than a second visitor quota service. The effective limits respect device-wide usage, all open workspaces, participant limits and host-wide/per-principal limits. Charge asset changes, retries, prefetch and reconnect continuation to the applicable persistent counters; reconnecting cannot reset a download cap or create repeated free admission. Distinguish per-entry/session caps, rate limits and retained-cache caps in the UI. Commit shared-capacity reservations through one coherent E1 admission decision so simultaneous joins cannot each spend the same available capacity. Under pressure, defer optional detail, offer a lower visual profile, pause or refuse the requested activity. Never silently alter accepted geometry, physical laws or a running scientific discretization to meet a visitor's display budget.

- [ ] **R0258.** Perform cheap local capability/storage/profile checks by default. Offer a short separately bounded benchmark using trusted installed workloads to refine a rendering/decode estimate; do not run supplied workspace code for the check. Report ready, ready with described adjustments, partial capabilities or unable to enter the required profile, with accessible reasons. Hardware/driver availability and unknown kernel costs limit what can be predicted; an estimate is not a promise of FPS or whole-PC safety. Keep runtime caps, bounded decoders, worker isolation and visible failures active after entry. The host normally receives only requested capabilities/budgets and needed admission information, not a raw hardware inventory or unrelated local data. Unknown mandatory sizes/costs require a bounded supported path or explicit refusal, never an unlimited download to discover the size.

- [ ] **R0259.** Capability negotiation describes actual schema and supported operations for the selected purpose: viewing a published mesh/result can have a smaller closure than editing its exact CAD source or rerunning its protocol. An unavailable optional kernel makes those operations unavailable; it does not block a supported representation view. Required/optional status is chosen by the declared capability profile and is not changed after the user enters to hide missing semantics.

### H4. Entry, materialization and native states

- [ ] **R0260.** Implement one observable flow: resolve/verify host → inspect permitted teaser or authenticate → select/check profile and costs → reserve/admit → subscribe to a coherent authorized snapshot → materialize minimum useful coverage → refine/live participation. Loading, waiting for required content, optional missing content, source login required, budget reached, disconnected, revoked and left are distinct native states with progress/cancel/retry where appropriate. Recheck admission, head/manifest generation and resource reservation when entering; a changed invitation or source cannot exploit the gap after preflight. Recompute changed requirements within the existing limits, or return an updated entry state before starting additional mandatory work.

- [ ] **R0261.** Reuse the normal source/asset loader for local storage, offline packs and remote hosts. Enter with cheap permitted representations and progressively fetch the selected nearby dependencies. Frame/CRS resources, styles, licenses, required content and supported Walk collision/query neighborhoods belong to the chosen capability closure. A visitor can inspect available coarse content while an optional Sand waits for its Organ; a mandatory missing dependency disables the affected capability explicitly. Walk and physical interaction wait for valid support instead of crossing unloaded geometry. Following another participant cannot bypass those readiness checks or force an unbounded download. Every visitor chooses their own independent visual profile and explicit Orbit/Fly/Walk mode.

- [ ] **R0262.** Native host controls provide invite/revoke, preview as a selected audience, connected-participant status, actual aggregate/per-participant resources, caps and activity/stop policy. Preview uses the real authorization projection, not a cosmetic client filter. The visitor can inspect expected/actual cost, adjust allowed detail, stop optional streams, save an authorized offline profile, leave and find recoverable drafts. A low-detail or accessibility-oriented tree/table view remains a valid participant presentation. Entry never requires loading every live Sand, installing every specialist kernel or precomputing a simulation.

- [ ] **R0263.** Use the visitor's saved defaults in the normal invitation/login/enter flow, with a compact cost/access summary and expandable controls. Cheap checking is automatic; benchmarking and advanced profile editing are optional. Repeated entry within unchanged accepted settings should remain a normal workspace action, with no recurring technical wizard or per-region confirmation. A newly required capability or exceeded limit has a visible adjustment/refusal state; it never silently spends beyond those settings.

### H8. Protein, other Organs and external dependencies

- [ ] **R0279.** Keep Protein queries/subscriptions and direct workspace entry interoperable and independently usable. Extend their existing session lifecycle, source identity, policy checks and I6 demand; Protein remains the semantic query path while large typed arrays/assets use the bounded payload path. A workspace may compose local data, public remote data, private remote queries, published snapshots and read-only references from several authorities. It never requires moving all underlying data to the inviter's Organ.

- [ ] **R0280.** Reuse one remote session across Areas/Sands only for the same Organ and authenticated participant/device context. Visiting A does not authenticate as A when querying B. Public access uses B's explicit public principal/projection; restricted access uses the visitor's B session or a narrowly delegated/published projection B actually permits. A must not forward its own login/password or unscoped capability on the visitor's behalf. A configured proxy/delegation must preserve requester, source, permitted fields/actions, expiry and accounting; it cannot turn a workspace read grant into access to the host's private network or credentials. Show B's verified identity when requesting sign-in. Closing a Sand unsubscribes its demand without logging out unrelated authorized views.

- [ ] **R0281.** An unavailable/revoked B dependency produces source-specific missing/sign-in/offline status and preserves independently usable A content. Reconnect rechecks B's authority and captured source versions before resubscription or application. Never replay queued mutations merely because a shared login reconnects. Optional source connections follow the participant's configured allowlist and network budget; an imported reference cannot acquire credentials, subscribe to a provider or perform arbitrary network work. An authorized offline publication includes its chosen dependency closure, provenance and retention rights; a live link is not offline availability. Required dependencies disable only the explicitly declared unsupported capability/profile, with no silent substitution of another person's data.

### Interface contracts I1–I6

- [ ] **R0307.** **I6 — Demand and activity.** Views/tools/jobs own leases for data, streams, representations, decoded buffers and computation. Aggregate leases across consumers, cancel obsolete generations and release upstream demand when the last relevant lease ends. A simulation/protocol can retain its own lease after its view closes; a hidden view alone cannot keep an entire laboratory/world active indefinitely. An activity exposes its owner, budget, cadence and stop policy. Query subscriptions and dense scientific result streams share lifecycle/status infrastructure but retain their ordering/retention semantics. Participant sessions own revocable leases and scoped admission; closing one view/session releases only its demand. Native entry/resource/activity controls expose both visitor and host costs, external Organ status and explicit stop policy.

### Scene-independent scale and the solar-to-local reference example

- [ ] **R0457.** Owner refinement, 2026-10-18, preserved under the 2026-10-21 priority: scale is an early architectural acceptance test immediately after foundations. A small optional solar-system package extends the available spatial context around default Earth. One continuous view travels toward Earth, through successively useful geographic coverage, down a street and into a supplied/authored house containing a 40 mm ball and mounted Sands, and reverses that journey. [stage 05](05-stage.md) delivers this bounded vertical slice alongside B38 scene-independent examples. Neither the final astronomy stage nor material/laboratory work may delay it, and disconnected globe/local viewers do not pass.

- [ ] **R0458.** Clarification, 2026-10-20: the preceding journey defines one reference dataset and integration test. The reusable capability is continuous traversal across the scales actually declared by any scene. Implement generic frames, spatial/purpose queries, error-based refinement, view control and resource demand first, then supply Earth/solar adapters and fixture data through those contracts. There is no engine sequence of solar → planet → city → room → ball, and no global zoom minimum/maximum derived from their sizes. The geographic example remains required evidence that its adapters interoperate; B38 separately rejects a design that only succeeds for that example.

- [ ] **R0459.** “Continuous” means stable model identity and placement, uninterrupted camera motion, progressively refined appearance, persistent selected/focused content and no compulsory scene reload, teleport or separate map application. It does not promise missing data or instant high resolution. Coarse valid coverage remains visible while a finer product is queued, unavailable or over budget. The fixture includes its detailed test site locally so the intended navigation can be proved without depending on worldwide street/interior coverage.

### Generic scale contract and independent-scene evidence

- [ ] **R0460.** A scene declares its root frames/units, occupied bounds/indexes, available representations and capabilities. An ordinary local root, an arbitrary large structure and a scene composed of several independent model roots all use the same core. A separate view may magnify a microscopic subject without physically enlarging it or creating a containing planet.

- [ ] **R0461.** Coordinate/precision policy, numeric/snap tolerances, projected error, depth handling, motion speed and residency work from the declared frame/purpose/geometry, not object names or Earth-specific size bands. When an operation exceeds its declared numerical/resource capabilities, report the limit rather than clamp to a ball/planet-sized range or invent accuracy. No scene data implies simulation of its entire spatial extent.

- [ ] **R0462.** Optional adapters may choose appropriate specialized algorithms: geodetic/cube-face paging for an ellipsoid, Cartesian spatial indexes for arbitrary meshes/assemblies, implicit-field paging or compact site queries. They provide the common K3/K7 bounds/query/representation contract. The generic model crate must not import the geographic provider, and ordinary scene navigation must not require a celestial body, geographic coordinates, map access or an orbital clock.

- [ ] **R0463.** Adding/removing a larger outer context does not rescale existing physical objects or initialize unrelated data/workers. Explicit physical resizing and display magnification retain different meanings. Camera-relative render copies are local to each view; run/canonical state remains independent of camera distance and source labels.

- [ ] **R0464.** A room-only scene remains a room-only scene as the camera moves out; the engine does not insert Earth, a solar system or invented intermediate objects to fill missing scales. Simulations likewise select domains/laws, conditions, numerical validity and coupling explicitly. Their supported equations can operate on other authored scenes and parameter sets; a camera distance or named scale band never silently selects a new physical law or molecular population.

- [ ] **R0465.** B38 uses a room-only root and a sparse invented non-geographic structure with data-defined large and small features spanning a ratio comparable to B25. Reuse small source assets and vary bounds, frame offsets, unit expression and arbitrary identities. Navigate both directions, extend the outer context, change representation error and exercise two views without enabling the geographic/astronomical profile. Preserve stable selection, local dimensional accuracy, supported navigation and bounded residency. Re-expressing units preserves physical dimensions; deliberate physical resizing does not promise unchanged mass/dynamics.

- [ ] **R0466.** The same acceptance mechanism must handle endpoints other than a ball or solar system. Compare cost against active items/bytes, representation work and precision/error rather than a successful scripted camera tour. Earth/solar starter package sizes and the synthetic 40 mm/1 μm assertion are fixture-specific thresholds; a larger/smaller authored scene uses its own declared validity/error profile and the same admission service.

### Default content and optional spatial roots

- [ ] **R0467.** **Earth is the default available geographic dataset.** Use an ellipsoid plus very coarse land/water outlines and selected labels; load only the small overview and the indexes needed by the current view. Default local ground has no relief. A coarse outline does not imply measured terrain, buildings or interiors. [Natural Earth's 1:110m vectors](https://www.naturalearthdata.com/downloads/110m-physical-vectors/) are a suitable small-scale context candidate; the provider [publishes its map data as public domain](https://www.naturalearthdata.com/about/terms-of-use/). [stage 01](01-stage.md) selects exact layers, versions and notices, then measures the packed/decoded result; the package must satisfy the starter budget.

- [ ] **R0468.** **The solar system is an optional data/profile package.** Small body descriptors, declared radii, simple shared meshes/materials, positions at a declared epoch and useful bounded labels are enough for the first proof. Loading it does not start N-body integration, install a scientific solver or populate interplanetary space. At true geometric scale many bodies are subpixel: optional selection markers are presentation aids and never change physical radius/distance. Live ephemerides, orbital dynamics and deeper astronomical interpretation retain their separate [stage 21](21-stage.md) gates.

- [ ] **R0469.** **An unlocated flat workspace is first-class.** A blank grid/plane, a fictional green mountain scene or a root protein specimen needs no Earth, solar package, datum conversion or geographic subscriptions. Geographic view, local authoring and ordinary flat work use the same K/I/E identities, supports, fields, operators and acceptance path. Default availability of Earth data does not force its allocation in an unrelated workspace or change the owner's launch-workspace preference.

- [ ] **R0470.** **Adding context does not rewrite local objects.** Earth definitions/body references and site-local coordinates remain stable when a solar package supplies surrounding context. Resolve placements through explicit composition/frame bridges; changed occurrence paths receive correspondence rather than pretending every path is unchanged. A private house or a ball never acquires a different size/location because a larger parent view becomes available. The same package pattern can later add another planet or fictional large-scale context.

### Globe, local frames and common surface sources

- [ ] **R0471.** The geographic spatial view is a globe with perspective navigation and a real horizon. The existing flat Map camera/normalized Mercator coordinates are a source/presentation adapter, not the canonical world frame or a way to achieve this by enlarging a map texture. Keep the useful flat map/layout view as another view onto the same permitted objects. Declared source CRS/axis/vertical-datum/epoch remains separate from the ellipsoid/body frame and local engineering frame. Geographic source zoom caps remain provider coverage limits, not limits on camera distance, authored detail or local-object inspection.

- [ ] **R0472.** Use the K2 frame/address contract for body/context placement, an Earth-centered frame for globe context, and appropriate local tangent/site frames for small geometry and edits. Convert near a relevant common ancestor before producing render/solver coordinates. [PROJ documents geodetic-to-geocentric conversion](https://proj.org/en/stable/operations/conversions/cart.html) and [local East/North/Up frames](https://proj.org/en/stable/operations/conversions/topocentric.html); select the ellipsoid explicitly rather than relying on a library default. A proposed default is a WGS84 ellipsoid, with datasets retaining their actual reference metadata. The ellipsoid defines global curvature. “Flat by default” means zero added local relief, not a flat Earth or an assertion of observed sea-level elevation.

- [ ] **R0473.** A **surface-source adapter** produces a K3 support/query capability and K7 representations with bounds, units/frame, source revision, coverage/unknown masks, error and provenance. A constant plane/ellipsoid, an imported elevation tile, a formula, a sculpted control mesh and a supplied building all enter through these contracts. There is no separate Earth-only terrain-edit system. Imported physical data remains measured/imported; a hand-built mountain remains authored, and an absent height remains absent even when a zero-relief fallback is displayed.

- [ ] **R0474.** Owner-confirmed A04: when better terrain/scan/source geometry replaces a base, preserve an authored/measured engineering pose by default and show any placement/support mismatch for reconciliation. An explicitly surface-relative placement may follow a verified successor under its declared anchor policy; ambiguous correspondence still needs repair. Intentional parent movement and ordinary wall-mounted Sand following retain their existing transform/attachment semantics. A source re-import or changed elevation cannot silently relocate every authored model or rewrite physical calibration.

- [ ] **R0475.** Height sources can compose displacement in their declared chart when that mathematics is valid. Mesh/implicit/solid sources retain their own surface/volume operations and correspondence. Caves, walls, overhangs and interiors do not become one height per geographic coordinate.

- [ ] **R0476.** Zero-relief ground, roads and available building footprints can render as flat contextual layers. No default building extrusion, procedural mountains/valleys or guessed interiors is generated. Supplied height/geometry or an explicit user operation can change the shape; supplied real terrain and fictional relief use the same operators, layering, bounds and derived-product lifecycle.

- [ ] **R0477.** Source coverage, local customization, masking/replacement and derived render/collision products are separate. A new map tile cannot overwrite an authored house or valley. Named source/output references and revisioned overrides retain the base; [stage 05](05-stage.md) proves a bounded local edit, while [stage 09](09-stage.md) completes the larger import/reconciliation workflow.

- [ ] **R0478.** Streaming refinement and acquiring a genuinely different source are different events. A better tessellation preserves accepted source geometry. A newly available elevation/model revision can change the ground itself and follows validation/adoption and Walk-support policy; it cannot silently put the camera under a mountain or certify a flat fallback as a surveyed surface.

### View navigation and usable local content

- [ ] **R0479.** Owner-confirmed controls are **explicit Orbit, Fly and Walk** in the same spatial scene. Switching changes input/controller behavior while preserving the canonical camera pose, selected target and view context; no automatic altitude-triggered mode switch. If Walk cannot safely start at the current pose/support, retain the current mode and expose the unmet support condition rather than teleporting the user.

- [ ] **R0480.** Orbit has an explicit body/object/surface pivot. Zoom/dolly changes viewing distance toward a resolved target, not just field-of-view magnification. Fly permits oblique approach, looking toward the horizon and moving between streets. Movement speed has bounded scene-relative controls and direct user adjustment so interplanetary travel and local centimeters are both controllable.

- [ ] **R0481.** Walk uses a local up/support frame, a finite controller footprint/eye offset and query-based kinematic movement over ready geometry. It can enter the fixture's doorway and interior. This limited navigation controller is implemented at [stage 05](05-stage.md) using verified support/collision queries; general rigid-body simulation, material dynamics and flexible-body studies stay in stages [18](18-stage.md)–[19](19-stage.md). A missing required support neighborhood pauses/clamps movement with visible status; it cannot cause a fall through an unloaded tile. A known zero-relief fallback is labeled as such and may supply its declared simple support.

- [ ] **R0482.** Terrain/building support and collision are selected by movement demand as well as the render view. Keep a bounded neighborhood for approaching ground/doorways, including behind the camera when needed by the controller. Replacement is generation-checked and keeps valid matching support until the successor is ready. Changed-source penetration needs a declared correction/stop policy, never an unannounced geographic relocation.

- [ ] **R0483.** A house and its mounted Sands have ordinary frame/anchor/occurrence references. At a distance, use an appropriate simple representation or metadata marker only when relevant and large enough; full Sand rendering, media streams and input surfaces activate under bounded view/focus demand. As projected size grows, the same objects become readable/pickable with stable focus, local hit coordinates and ordinary native controls. Do not create live interfaces for every geographically indexed Sand.

- [ ] **R0484.** Perspective from street level, upward/oblique views, entering/exiting an interior and close inspection of the ball are acceptance paths. Panoramic photography, a reconstruction service, a real-world scanner and GPS are not prerequisites. Flat map, orthographic edit and standalone workspace views remain available on the same source/model references.

### Refinement, authored visibility and bounded allocation

- [ ] **R0485.** Use a sparse spatial hierarchy appropriate to the support, with separate semantic indexes. Continents, countries, regions, cities, blocks, streets and houses are useful labels/queries, not a compulsory containment tree or a chain of layers all loaded at once. Geographic paging must cover the globe including dateline/poles; provider-specific Mercator tiles need explicit coverage/reprojection/fallback. Evaluate a bounded geodetic or cube-face terrain hierarchy behind the same tile contract, with topology/seam and bounds tests before selection.

- [ ] **R0486.** Refine by projected shape/error and actual query/demand, with independent semantic, collision and scientific fidelity. [Cesium Native's selection documentation](https://cesium.com/learn/cesium-native/ref-doc/selection-algorithm-details.html) describes view-dependent refinement and retaining usable coverage during loading; use these as algorithmic prior art. The Lince implementation must select relevant descendants without loading every sibling or all intermediate fine products. Keep usable covering fallbacks for missing selected coverage, distinguish additive/replacement layers and avoid double surfaces. Hysteresis, bounded prefetch/grace leases and per-frame decode/upload admission prevent oscillation and camera stalls. Budget the simultaneous old/new representations.

- [ ] **R0487.** Frustum and conservative horizon/occlusion tests prune work, with local BVHs/visibility for buildings/interiors. [Ellipsoidal horizon culling](https://cesium.com/blog/2013/04/25/horizon-culling/) is useful prior art; it needs conservative bounds and appropriate handling of negative elevations, subsurface views and authored cuts/protrusions. A high custom mountain cannot be culled using an obsolete unmodified-ground bound. Near/depth handling must preserve the horizon and centimeter close-ups; validate the selected reverse-depth/projection or coordinated pass strategy without changing canonical units.

- [ ] **R0488.** Authored models/terrain effects are independently indexed by spatial bounds and revision. Publish cheap coarse proxies/affected bounds and invalidate relevant ancestor summaries as part of the accepted change; keep expensive geometry generation asynchronous. Bounds include protrusions and changed silhouettes, not just the original map tile. A visible coarse representation of a modification must appear when its projected contribution is meaningful, then refine as the camera approaches. A tiny ball need not become visible from space; relevant markers are optional presentation and are clustered/bounded.

- [ ] **R0489.** A modification at any addressed scale is an ordinary support/field/operator/layer definition with explicit source mapping. Preserve additions, masks and replacements through base-tile eviction/reload, offline operation, save/undo and source changes. Local edits invalidate affected products and summary paths rather than remeshing Earth or copying an entire region. Public/private proxy bounds and aggregates follow the same authorization rules as the source content.

- [ ] **R0490.** Default active memory is demand-driven: no per-world-feature ECS entity, decoded mesh, texture, collider or active worker solely because content exists on disk. A small current path/index, coarse covering content and explicitly admitted active demand can remain. Multiple views, focused editing, unsaved work, a running experiment and a navigation-support neighborhood have their own bounded leases. These are explicit demand owners, not hidden exceptions to an otherwise global view-frustum flag.

- [ ] **R0491.** **Owner-confirmed cache behavior:** release unneeded RAM/GPU payloads after their bounded grace/in-flight users finish; retain downloaded compressed content in a bounded disk cache and explicitly saved offline packs. Offline pinning preserves disk availability and does not pin every decoded/GPU product. Measure committed/reserved memory and retained pool capacity as well as live payload counts; dropping cache handles while large unused allocations remain is not proof of release. Any shared reusable pool has an explicit counted floor/cap and trims at a safe completion boundary when policy requires it. Save dirty data before eviction, or retain it with visible admission pressure if saving fails. Parent/definition metadata and durable edits are not deleted because their render payload is released.

- [ ] **R0492.** **Owner-confirmed network behavior:** zoom may fetch needed detail from configured permitted sources when online, within visible byte/rate/concurrency/storage limits. The offline switch stops new network work and cancels obsolete requests where possible; retained data and coarse fallback still work. No camera movement triggers a whole-city/world download, credential acquisition or new provider subscription. A missing/denied/over-budget layer retains honest coverage/status. Explicit region saving provides the offline dependency closure through the same loader/cache path.

- [ ] **R0493.** [stage 05](05-stage.md)'s B25–B28 replays must show the entire outward/inward route, custom-relief visibility, explicit navigation modes, reload/undo, flat-workspace independence, near/depth precision and released allocations. [stage 06](06-stage.md) widens source coverage/offline packaging and stages [07](07-stage.md)–[09](09-stage.md) widen authoring. [stage 12](12-stage.md) proves structural microscopic editing on the same model outside geographic context. Stages [16](16-stage.md)–[17](17-stage.md) add the retained material/laboratory workflows only after [stage 15](15-stage.md) stabilization; none is a prerequisite for globe, street or room usability.

### Independent user controls

- [ ] **R0498.** The native region/layer inspector exposes: desired screen/shape error; semantic layers; active physics/model fidelity; update cadence; disk/RAM/GPU budget; offline pins; and network mode. It previews estimated bytes/coverage, then reports actual achieved detail. A user can pin a building interior at high detail, a city at medium detail and Earth at coarse detail without loading all intervening scales.

- [ ] **R0499.** Use geographic/semantic selection, not just a radius: a neighborhood polygon, a route corridor, a set of rooms, a material sample or an assembly's dependencies. A simulation's offline closure includes boundary/material/solver inputs, not merely the visible camera frustum. File absence, access denied and incomplete derived data have different internal statuses with disclosure-safe public messages.

### Runtime selection and scheduling

- [ ] **R0500.** Spatial indexes use the appropriate representation: geographic tiling for Earth context; bounded Cartesian BVHs/grids/trees inside a site; molecular neighbor lists for atomistic computations; solver partitions for physics. They share bounds/query envelopes, not one universal octree. Store only occupied/declared nodes; traversing a huge empty universe should cost according to occupied hierarchy and visible work, not total addressable volume.

- [ ] **R0501.** For perspective rendering, a candidate projected error is `e_px ≈ e_world H / (2 d tan(fov_y/2))`, with conservative camera-to-bound distance and a safe near-plane policy. Orthographic views use their world-units-per-pixel scale. Source uncertainty and sampling resolution remain separate from this approximation. Screen error controls visible representation, not scientific accuracy. [3D Tiles defines geometric error and hierarchical refinement](https://www.ogc.org/standards/3dtiles/); support a declared subset rather than claim conformance from a similar tree.

- [ ] **R0502.** Coarse parents remain resident until required children are usable. Replacement versus additive refinement is explicit per node; avoid double surfaces. Tile borders share topology/elevation conventions or tested stitching. Use hysteresis and bounded transition times to prevent rapid toggling.

- [ ] **R0503.** Prioritize direct edits/selection, physical support around active bodies, visible coarse fallback and then useful refinement. Fair queues prevent a texture decode, microscopic job or huge bake starving input. Cap requested bytes, decoded bytes, worker count, upload bytes/frame, labels, splats and simulation islands separately.

- [ ] **R0504.** Work keys combine object/input revision, purpose and parameters. Coalesce identical permitted requests; propagate demand/cancellation to workers and upstream downloads. A noncancellable library can finish within a bounded worker or be terminated; its stale result still cannot publish.

- [ ] **R0505.** Static definitions share buffers and instances. Only active editable/interactive/semantic occurrences receive ECS objects. Picking and searchable semantic indexes do not require an entity for every city feature. Large labels/markers aggregate by scale.

- [ ] **R0506.** Render precision and depth precision are different. Test reverse depth/near planes and separate scale-appropriate passes/viewports when tiny and astronomical content coexist. A precise CPU coordinate does not make a distant microscopic triangle visible or remove z-fighting.

- [ ] **R0507.** Cache keys include content, schema/tool version, detail recipe, units/frame/registration, material conditions and validity. Authorization is checked independently on each read. Policy-scoped projections cannot reuse a private aggregate from another scope.

- [ ] **R0508.** Eviction respects pins, active solver/contact requirements and unsaved edits. Offline pins retain source availability on disk; they do not pin decoded meshes/textures/colliders or GPU residency. Runtime residency is separate from offline disk residency. Releasing a view releases GPU/CPU resources and stream demand after shared users finish; do not destroy a referenced source definition.

### Performance research and adaptation boundaries

- [ ] **R0540.** BRUTAL source at `a25cdf80…`: [job scheduling](https://github.com/ahwootech/BRUTAL/blob/a25cdf80b2a15530e7da6cff744def9d841ad3c3/Brutal.Concurrency/Jobs/JobScheduler.cs), [GPU staging](https://github.com/ahwootech/BRUTAL/blob/a25cdf80b2a15530e7da6cff744def9d841ad3c3/Brutal.Vulkan.Abstractions/StagingPool.cs) — **Documented mechanism:** Cost-estimated job distribution and explicit pooled GPU-transfer resources — **Concrete adaptation and limit in Lince:** Record job costs, own buffer lifetimes and control uploads. Prefer a measured kernel/algorithm improvement over claiming that native API access alone supplies application performance. Existing Bevy/WGPU facilities and selected backend escape hatches are evaluated against the same fixtures.

### Execution contracts E1–E8

- [ ] **R0546.** **E6 — Data movement, persistence and devices.** Separate source chunks, accepted metadata, mutable run buffers, UI projections and retained output/checkpoints. Do not hash/persist/replicate every numerical integration step by default. Batch I/O and metadata acceptance within bounded latency; use completion-driven reads/writes, checksum verification and bounded decompression. Reuse staging/command/buffer resources only after their CPU/GPU users finish. Keep GPU-suitable iterative data resident where useful and avoid readbacks without an actual consumer. Record bytes copied/decoded/uploaded and synchronization waits; “zero copy” is a measured property of a specific path, not a universal promise. Device loss or worker crash preserves accepted sources and reports affected jobs/results for recovery. H3/H5 meter encoded, decoded, staged, copied and served bytes separately; reserve partial/replacement storage and validate transfer-to-artifact identity. Source access is rechecked before completed data attaches to a participant after revoke or generation change.

- [ ] **R0548.** **E8 — Whole-application cost and capability growth.** Coordinate existing task pools, render/physics schedules, adapters and scientific libraries so they do not independently reserve all cores or memory. Reserve predictable service for input/acceptance and bound background CPU/GPU/IO occupancy. Measure per-workload throughput, p95/p99 response/frame latency, allocations, cache/transfer bytes, wake-ups, queue age and idle demand against a named hardware/build baseline. A source-code-level abstraction earns its complexity when it improves two real workflows or a required invariant. Introduce native graphics/compute backends, different I/O paths, optional workers or a new database mechanism only for a named measured gap; record the rejected simpler path and retained portability/recovery contract. Shared lazy geometry/in-flight work must make progress on one admitted lane: resolve dependencies without blocking on a task that may depend on the waiter, release cache locks before evaluation and detect cyclic/reentrant waits. Parallel import preparation preserves source-order identities and per-item failures; nested libraries share admitted capacity instead of spawning independent all-core pools. B24/B30/B32 exercise these boundaries. Account shared source work once and per-participant view/queue/egress work separately. Measure host and each client under mixed edits/downloads/presence/results; avoid all-to-all broadcast and per-participant full-scene scans when permitted interest can narrow work.

### Early-scale resource subprofile

- [ ] **R0784.** [stage 05](05-stage.md) has a stricter proposed starter profile on the same baseline hardware. The following are testable engineering targets, not observed results or owner-approved measurements. Counters include simultaneous old/new fallback/detail products, source indexes, scratch/staging and in-flight transfers. Larger offline packs are user-selected content, never a consequence of merely opening the globe.

- [ ] **R0785.** These are budgets and numerical assertions for selected test data. The reference journey and 40 mm object do not set engine endpoints or universal accuracy. B38 reuses bounded source assets and applies declared local precision/error to its own arbitrary feature sizes; its scene/profile supplies the range. Retain the same resource admission and report unsupported operations honestly rather than extrapolate one successful route into unlimited precision.

- [ ] **R0786.** Default coarse Earth package — **Proposed starting bound:** ≤32 MiB packed source/style/index data; select only the context layers actually used — **Proof and adjustment rule:** Report packed and decoded bytes independently; default relief remains zero and no missing building/interior geometry is fabricated

- [ ] **R0787.** Optional solar-system starter — **Proposed starting bound:** ≤1 MiB body/catalogue/package metadata plus shared admitted procedural/render templates — **Proof and adjustment rule:** Actual fixed-epoch bodies/labels are bounded; no automatic ephemeris archive, N-body worker or extent-sized array

- [ ] **R0788.** Reproducible scale fixture content — **Proposed starting bound:** ≤64 MiB total packed Earth/solar/test-site source content — **Proof and adjustment rule:** Includes the supplied street/house/ball/Sand fixtures; detailed real-world city coverage is not required to prove continuous scale

- [ ] **R0789.** Default automatic disk cache — **Proposed starting bound:** ≤128 MiB compressed regenerable content, separate from explicitly saved offline packs — **Proof and adjustment rule:** Cap growth and show eviction/coverage; retain accepted authored sources independently. A larger cache/profile is an explicit user setting

- [ ] **R0790.** Active scale CPU/GPU — **Proposed starting bound:** ≤256 MiB incremental model CPU residency and ≤128 MiB GPU residency across the initial route/views — **Proof and adjustment rule:** Include peak replacement/upload work and count shared-memory graphics correctly; report total Lince process memory separately

- [ ] **R0791.** Unneeded active-payload release — **Proposed starting bound:** Target ≤2 s after the last demand/grace lease and relevant completion boundary — **Proof and adjustment rule:** Observe actual CPU/GPU resource ownership after the queue drains; dirty/focused/running/other-view/support leases remain explicit and bounded, with no busy retry loop

- [ ] **R0792.** Responsiveness and geometric continuity — **Proposed starting bound:** Retain the existing input/frame/open targets; local 40 mm fixture dimensions within 1 μm in its local model frame — **Proof and adjustment rule:** This is numerical preservation of a synthetic authored dimension, not geographic survey accuracy. Record projected seams/error, visible loading gaps, origin shifts, p95/p99 and controller behavior

- [ ] **R0793.** Idle and travel history — **Proposed starting bound:** Zero active decode/upload/terrain/physics work without relevant demand after settling; no continuing orbital integration in the starter — **Proof and adjustment rule:** Measure wake-ups/CPU against the base app; repeat the outward/inward route and vary address extent at fixed populated content, showing bounded steady state rather than growth with every place visited

- [ ] **R0794.** If a target fails, record the responsible product/queue/algorithm, attained error and measured hardware before agreeing a revised target. Do not hide mandatory content loss, changed physical dimensions or missing support behind an unlabeled quality reduction. The fixed fixture can use offline inputs and a controlled source for missing/slow/denied cases, then one selected real provider for a bounded integration replay.

- [ ] **R0795.** For core structural workloads, record actual imported/authored sites/residues, source/display bytes, numeric input precision and amount metadata. Numerical sample/replica/retained-frame counts, scientific restart/checkpoint buffers and solver time/error targets are selected only with their deferred study before dispatch; they are not [stage 01](01-stage.md) dataset/implementation prerequisites. Both phases use the same admission policy, including peak scratch, old/new products and subscription queues. Record steady idle demand/wake-ups as well as throughput; do not introduce another per-workflow quota service.

- [ ] **R0796.** These limits are falsifiable starting targets. If the actual baseline cannot meet them, record the bottleneck and revise the profile, representation or target before claiming completion. Asset count alone is not a performance result. A candidate stress run uses 10,000 operational Records, a 100,000-feature spatial index and a 30-minute navigate/disconnect/reconnect session; only a bounded nearby subset is rendered.

- [ ] **R0797.** Core error targets are task-specific: proposed room editable measurements within 1 mm of authored synthetic dimensions; default room simplification requested deviation 5 mm with achieved metric labeled; structural microscopic round-trip error relative to declared input precision; geographic alignment within source/transform accuracy rather than an invented universal centimeter promise. Physical scientific tolerances are selected later under the deferred numerical profile and do not block [stage 15](15-stage.md).

### Hosted starter resource subprofile

- [ ] **R0798.** This is core [stage 05](05-stage.md) hosting/navigation coverage. Simulation-specific costs remain unknown/unsupported until their deferred profile is admitted; joining, cost estimation, asset delivery and remote Protein access do not require a numerical solver or physical-material evaluation.

- [ ] **R0799.** This extends the same [stage 05](05-stage.md)/E1 budget, not an additional per-feature allowance. [stage 01](01-stage.md) selects actual hardware/network settings and baseline measurements before implementation. The initial load is one host with its optional local view, one identified remote editor and one restricted public visitor; repeat the same fixture on independent processes/devices and a controlled network. Later 8/64-participant pressure runs are separately admitted experiments and cannot be inferred from the existing small-layout capacity tests. The following thresholds are engineering proposals, not measured results or an assertion that every device can join.

- [ ] **R0800.** Entry metadata — **Proposed starting bound:** First useful authorized entry page ≤256 KiB; larger descriptor/cost catalogues are paged within the admitted working set — **Enforcement/evidence:** No full city/source enumeration or private content to compute a preview; bound parser time, page counts and dependency expansion

- [ ] **R0801.** Entry assets — **Proposed starting bound:** Reuse the ≤64 MiB packed scale fixture; a fresh participant's selected fixture profile requests at most that missing source content — **Enforcement/evidence:** Show exact selected bytes, metadata/transfer overhead and actual retries separately; enforce the visitor's chosen session/download/cache limits across source changes and reconnect

- [ ] **R0802.** Visitor memory/rendering — **Proposed starting bound:** Each visitor stays within [stage 05](05-stage.md)'s ≤256 MiB incremental CPU and ≤128 MiB GPU model budget across all its views/queues — **Enforcement/evidence:** Include partial/decode/staging/replacement and active presence; report total Lince memory separately. A lower selected profile is a valid visit, not changed scientific/physical state

- [ ] **R0803.** Host participation overhead — **Proposed starting bound:** Proposed ≤64 MiB incremental CPU bytes for the two remote sessions beyond the single shared accepted fixture/source working set — **Enforcement/evidence:** Includes subscription/egress queues, transfer staging and pending work; count shared geometry once and per-visitor work separately. Host-local rendering still has its ordinary [stage 05](05-stage.md) budget; headless serving does not allocate a viewport

- [ ] **R0804.** Replaceable presence — **Proposed starting bound:** Proposed maximum 10 updates/s per participant stream and expiry after 20 s without refresh, reusing existing bounded presence semantics — **Enforcement/evidence:** Coalesce stale samples and bound number/size of streams; absolute positions use declared frame/error. Disconnected presence cannot retain body/camera/worker authority

- [ ] **R0805.** Responsiveness and streaming — **Proposed starting bound:** Retain ≤100 ms local visible-input and ≤200 ms small acceptance processing proposals under mixed serving load — **Enforcement/evidence:** Record end-to-end latency, RTT/loss and waiting-for-assets separately; these local targets are not a WAN latency guarantee. Reserve control service and bound all lane/aggregate queues/windows

- [ ] **R0806.** Recovery and resource release — **Proposed starting bound:** Retain [stage 05](05-stage.md)'s ≤2 s last-demand/grace/completion release target and existing cancellation targets — **Enforcement/evidence:** Revoke prevents new usable delivery and cancels obsolete demand; show draft/offline status and actual tails. A delayed supported kernel/transfer cannot be counted as released while still owned

- [ ] **R0807.** Freeze finite per-participant and host-wide request/queue/byte/rate limits for the selected fixture before dispatch; the existing transport guard is not evidence for higher-rate presence or assets. Measure advertised versus actual cost, cold/warm/cache-disabled runs, host control latency, download/upload rates, queue age, decode/upload work, per-client/frame p95/p99 and steady resource release. Include caps already partly spent, concurrent admission, underestimated decoded sizes, unknown external totals, cache-full/disk-full, late source generations and reconnect without reset. Synthetic delay/bandwidth/loss profiles make regressions reproducible. Favor a lower visual profile or explicit capability refusal over exceeding limits or hiding incomplete support. Preflight never claims hardware safety or scientifically equivalent lower fidelity without its own validated model.

### Verification obligations

- [ ] **R0815.** B38 is a mandatory scene-independence regression: run the same relevant common implementation with geographic/astronomical profiles disabled, renamed/replaced scene data, non-geographic roots, different endpoints, re-expressed units and a comparable sparse extent-to-feature ratio. Preserve declared dimensions under unit/frame/view changes; physical scaling remains a separate model edit. Test arbitrary Orbit targets and explicit navigation up, no implicit celestial center/gravity, no source request for unused profiles, continuous representation refinement and bounded active cost. B25 still verifies its geographic adapters, while B38 prevents those adapters or example thresholds from becoming the core architecture.

- [ ] **R0818.** C13–C16 add a local/hosted intention regression: enter the same permitted workspace at two visual budgets; edit through the same receipt path; visit public data as an explicit guest; use another Organ's Protein source with the visitor's own session; and retain permitted cached viewing/drafts after host loss. Source credentials, private inspectors and nonpublished views stay local. Presence and a navigational camera cannot create accepted geometry changes, operational effects or worker expenditure. A viewer of a result needs only the declared published closure, not every underlying private input or solver.

- [ ] **R0825.** B33–B37 exercise real native/backend/transport boundaries with identified and explicit guest principals, not clients constructed with trusted-local authority. Test invitation/manifest changes between estimate and entry; unauthorized index/count/hash disclosure; mixed public/private source blobs; decoder size lies; revoked delivery/completion; per-Organ login separation; original-author review; restart with uncertain replies; and no replay of drafts, Area crossings or automation. Verify direct remote Protein usage still works outside a hosted workspace. Full workspace-offline mode produces no new participation/source/reconnect requests; source-only cache mode is visibly narrower. Park pixel publication and remote input are separately denied/revoked, including session lock.

- [ ] **R0826.** Record host and every client's timing and bytes during simultaneous edits, asset transfer, independent navigation and optional presence. A fast isolated renderer or a successful 64-session small-layout test does not establish hosted spatial performance. Overlapping interests must share allowed source work without leaking private equality or keeping closed views alive. A slow reader must trigger the specified coalescing/reset rather than authority starvation, lost accepted edits or unbounded queue growth. Final commits, actor/base checks and B31 preview eligibility remain correct under reordered/revoked network delivery. In B37 the authoritative run is unchanged by visitor LOD or display-frame skipping; a superseded writer cannot accept another input or publish a current result.

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0831.** **Q4 — Explicit spatial controls.** Orbit, Fly and Walk are selected explicitly within one continuous scene; keep supported camera pose, selection and focus. Street/horizon/interior views are required, and the early scale proof follows the shared foundations.

- [ ] **R0833.** **Q6 — Flat until supplied or authored.** Default ground has no mountains/valleys and absent buildings stay flat/absent. Supplied topographic/3D data or user authoring changes shape using the same primitives as a standalone fictional landscape. Earth still has globe curvature; fallback zero relief is not measured terrain.

- [ ] **R0834.** **Q7 — Budgeted online detail.** Zoom may fetch needed detail automatically from configured sources when online, within download/storage/concurrency budgets. The offline switch prevents new network work and preserves usable cached/coarse content; no per-region confirmation is required by default.

- [ ] **R0839.** **Q12 — Accountable entry and continuing resource limits.** Show selected-profile download, total-known/unknown content, peak disk/RAM/GPU, ongoing traffic and execution location/payer. Use default cheap checks, optional bounded trusted benchmarking and enforced device/host/participant budgets. Partial/unsupported capabilities and outside-Organ sign-in are explicit; no mandatory whole-workspace download or hidden local simulation.

- [ ] **R0840.** **Q13 — Shared authority, federation and execution.** Reuse existing login, workspace permissions/review, Protein/Organ sessions, typed edits, artifacts and demand. Keep per-source rights and participant credentials independent; each active run/domain has a designated fenced execution authority. Viewing never grants host credentials, native/desktop control or another machine's compute budget. H1–H10 and B33–B37 govern phased implementation.

- [ ] **R0841.** **Q14 — Scene-independent scale; example-based proof.** The solar-system/Earth/ping-pong-ball route is an integration/acceptance example. Generic frames, units, precision, sparse indexes, purpose-specific representations, navigation and demand work for any supported authored scene, including a room-only root and an invented scene spanning comparable scales. [stage 05](05-stage.md)/B38 proves that separately; no fixed endpoints, required solar/Earth parent, named zoom ladder or inferred physical simulation is built into the engine.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0750.** B25 — **Fixture:** Optional sparse solar system, default globe, bounded test street, supplied house/Sands and 40 mm ball — **Proof required:** Continuous inward/outward camera route, stable identity/physical size, globe/horizon/poles/dateline, oblique street/interior interaction, optional outer context and two-view precision; no populated-universe or hidden orbital simulation

### Owned fixture specifications

- [ ] **R0751.** B26 — **Fixture:** One surface/model recipe as imported or user-authored relief, placed both on Earth and in an independent blank/fairy-landscape workspace — **Proof required:** Same support/layer/query/acceptance machinery, flat fallback and absent-data status, coarse custom proxy/silhouette and conservative bounds, local invalidation, save/undo/offline reload and source/custom separation

### Owned fixture specifications

- [ ] **R0752.** B27 — **Fixture:** Explicit Orbit/Fly/Walk over streamed flat/terrain support, a doorway/interior and mounted content — **Proof required:** Pose/focus/selection continuity, controlled speed, Walk support readiness and replacement, visible missing/changed-source handling, readable near content and no top-down-only or teleport-only navigation

### Owned fixture specifications

- [ ] **R0753.** B28 — **Fixture:** Repeated zoom/reversal/unload across two views, disk cache/offline packs and an on-demand source — **Proof required:** Bounded metadata/decoded/GPU/in-flight bytes, late generation and denial handling, last-demand release, pins distinct from residency, zero new offline requests, safe save/device failure and no monotonic growth with visited history

### Owned fixture specifications

- [ ] **R0755.** B30 — **Fixture:** Reused supplied meshes first, then supported planar/curved/trimmed surface plans, sharp joins and two independent views — **Proof required:** Shared source/purpose geometry and correct instance transforms; retained dirty assets/changed upload bytes; common boundary samples including unequal-detail seams, normal/winding/finite/error checks, bounded CPU/optional GPU parity and fallback; one-lane shared-work progress and last-lease release

### Owned fixture specifications

- [ ] **R0758.** B33 — **Fixture:** The same tiny workspace and [stage 05](05-stage.md) route on a host, identified editor and restricted public visitor, with cold/warm caches and different detail profiles — **Proof required:** Explicit invitation/guest admission, coherent identity/heads, native entry/preflight/progress states, finite versus unknown and declared versus verified costs, persisted download/peak-memory admission, bounded useful entry and source changes without whole-world download

### Owned fixture specifications

- [ ] **R0763.** B38 — **Fixture:** Independent room root and sparse invented non-geographic structure with arbitrary large/small features over a scale ratio comparable to B25 — **Proof required:** Same frame/query/refinement/navigation/demand implementation with no geographic/astronomical profile; arbitrary identities/endpoints and unit expression, added outer context, stable local dimensions/selection, two view origins, no fixed scene/zoom ladder, no implicit physics/magnification and resource cost tied to active content

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Use the complete travel-and-use route with multiple semantically meaningful landmarks, working mounted content, two different participant profiles, cold/warm/offline inputs and deliberate source delays/denials. Grow address extent without adding content and grow the interest index without rendering every entry; verify the route, input and resource budget still work.

- [ ] **R0561.** **[stage 05](05-stage.md) — L0/L1/L2/L3/L4, scale/navigation L5 proof** — **Intention and defined deliverable:** Generic continuous traversal of data-defined scene scales; solar/globe/street/house/ball is the geographic reference example, with B38 independent non-geographic/room cases — **Correctness/failure evidence and performance obligation:** B13/B23/B25–B28: explicit Orbit/Fly/Walk, custom proxy/bounds, near/depth and local dimensional precision, optional outer context and standalone flat roots, cold/warm/offline continuity and camera reversal. Measure active bytes/jobs versus extent, frame/input latency, network/decode/upload work and release latency against the early-scale subprofile; no full-population or hidden N-body work. B30 adds supplied-mesh reuse/changed-upload measurements without requiring an exact-CAD tessellator. B33–B36 require an identified remote editor and restricted public visitor, different visual budgets, permitted asset delivery, native cost/preflight states, host limits, other-Organ data and disconnect/revoke recovery on this same route. B38 must reuse that implementation with arbitrary identities, units/bounds and non-geographic endpoints, proving the solar example is not an engine constraint.

- [ ] Run the actual native/backend/persistence/transport path for every 05.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0621.** the generic implementation navigates both the B25 solar → Earth → street → entered house → ball route and B38 independent non-geographic/room cases continuously in both directions, with explicit modes, visible customization and mounted input. Default relief is flat unless data/authoring changes it. Pass the early-scale resource profile or document a reviewed measured revision. B33–B36 prove identified/public entry and recovery on this same scene. A globe screenshot, disconnected local viewer, hardcoded tour or generic coordinate type is insufficient. Stages [06](06-stage.md)–[14](14-stage.md) extend locality and authoring next; the retained paired physical-material/protein proof is deferred to stages [16](16-stage.md)–[17](17-stage.md) after stabilization.

- [ ] Advance only when **all 05.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
