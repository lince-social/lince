# 03 — Frames, local precision and independent views

Finish with the same object editable in flat, perspective and numeric views while the camera changes origin. A small feature remains small and precisely editable inside a distant composition; a room or microscopic specimen can open without loading Earth.

## Required workflow milestones

- [ ] **03.M01.** Inspect a meter-sized furnishing and a microscopic point fixture in their own local units. Make a numeric edit, switch unit display/view, undo and reload; exact authored distinctions remain and camera magnification never scales the object.

- [ ] **03.M02.** Open two distant close-up views, move both cameras and shift rendering origins. Picks still identify the same accepted objects and canonical poses/calibration/current supported physical state do not drift.

- [ ] **03.M03.** Observe another participant's optional cursor/selection or lightweight avatar in the correct frame. Move independently, opt out and disconnect; presence expires without becoming a colliding body or changing geometry.

- [ ] **03.M04.** Inspect a native Sand/Record view at a spatial attachment, open the same content in a readable flat/HUD presentation and edit it there. The spatial/flat views show the same accepted data and selection while local focus/tool expansion stays independent; returning preserves the original anchor, dimensions and content. Complete the interaction with keyboard/mouse and no model provider or extra device.

## Entry and scope

- [ ] Follow the general [sequential delivery and repair cycles](00-General.md#sequential-delivery-and-repair-cycles), [balanced acceptance](00-General.md#workflow-contracts-and-balanced-acceptance) and [completion compression](00-General.md#completion-compression-and-retained-capability-memory). Read this stage's [delivery cycles and retained workflows](#delivery-cycles-and-retained-workflows), keep pending design detail, and repair a required earlier workflow before advancing dependent work.

- [ ] Confirm the preceding [stage 02](02-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 02 summary](02-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed)`; `crates/interface/src/native/canvas_host/`; `crates/interface/src/native/topology/`; `crates/nucleus/src/physical_workspace.rs`.

## Ordered implementation tasks


- [ ] **03.T01.** Implement frame/unit descriptors, high-precision cell/residual placement, relative transforms and explicit canvas-to-model unit calibration. Persist magnification separately from physical placement. Add the precise-authoring contract's numeric absolute/relative position, distance/angle and parameter controls through I4/E2; prove supported unit conversions and source-preserving display precision in the bounded B23 subset.

- [ ] **03.T02.** Spike `big_space` against the pinned Bevy/Avian setup with origin shifts, nested moving frames and two distant close-up views. Select it or record a bounded replacement using the same durable contract.

- [ ] **03.T03.** Add perspective and orthographic views onto the same model/selection and keep existing flat content usable through I1/I2/I5. A root specimen view requires no Earth/building. Prove that render-origin shifts never change canonical pose, physical units, velocities or physical-workstation calibration.

- [ ] **03.T04.** Prove the same exact authored coordinate/dimension changes retain distinct canonical/derivation identities below 1 nm in a suitable local frame, through unit display, undo and save/reload. Numerical method tolerance may be coarser only when explicitly requested and reported; display rounding or a cache quantum cannot merge the sources. Add this bounded B23/B24 subset without requiring a microscopic solver.

- [ ] **03.T05.** Carry canonical/local-frame identity through the participant view/presence envelope. Implement optional bounded cursor/selection and lightweight-avatar presentation state using I2/I5/I6, with session sequence/expiry and visible opt-out/follow behavior. Reuse view-local origins and keep shared camera bindings explicit; B36 first proves independent views, correct pose/selection mapping, release and no accepted geometry/physical-body mutation.

- [ ] **03.T06.** Separate content/source identity from a presentation's view binding, frame/anchor, layout and tool state. Support model-space attachment and view-local flat/HUD placement through I1/I2/I5 without rewriting canonical pose or physical dimensions. Use existing logical layout/DPI rules for readable screen content and the declared model-unit map for spatial content. Preserve the existing flat workspace as a full interaction path.

- [ ] **03.T07.** Reuse the same source/Record state across supported presentations, with explicit focus/input ownership and bounded demand. Persist user presentation/anchor choices separately from source data; opening another view does not create another model, Record, body or independent accepted state. Closing/hiding a view releases its presentation demand while retaining demand owned by other active views. Later stage 05 streaming and stage 07 application mounts extend this same contract.

## Delivery cycles and retained workflows

- [ ] **Cycle A — Make coordinates durable.** Implement explicit unit/frame/calibration descriptors and precise relative placement behind the durable source contract. Evaluate nearby transforms around a common ancestor; select big_space or the bounded replacement only after the pinned two-view/origin-shift spike. Keep camera magnification and rendering origin separate from authored placement.

- [ ] **Cycle B — Share objects across independent views.** Add orthographic/perspective and numeric tools against the same identities, then spatial/flat/HUD presentations with independent focus/layout/anchors and aggregate demand. Closing one view must preserve another view's source and leases.

- [ ] **Cycle C — Introduce bounded presence.** Map optional cursor/selection/avatar state through canonical frames and session sequence/expiry. Exercise two distant views, origin shifts, opt-out and disconnect without creating physical bodies or accepted geometry edits.

- [ ] Preserve these contracts; a furnishing, microscopic point and distant participant are example inputs to generic frame/view capabilities.

| Retained workflow | Delivery kind and milestone coverage | Qualitative review | Hard assertions and measurements |
| --- | --- | --- | --- |
| **03.W01 — Precise editing across scale and views** | Built-in frame/unit/numeric/view tools; 03.M01/03.M02. Edit a small feature, switch units, move two distant cameras, undo and reopen. | Numeric controls and picks remain predictable; camera movement does not visibly wobble the local edit. | Authored distinctions below 1 nm remain distinct in the supported fixture; lengths/IDs/canonical state survive origin shifts; record declared local error, conversion round trips and two-view CPU/GPU/input costs. |
| **03.W02 — Independent participant presence** | Built-in view-local presence; 03.M03. Observe another cursor/avatar, move independently, opt out and disconnect. | Correct apparent position, independent control and disappearance are understandable. | Accepted geometry/body creations from presence = 0; apply sequence/expiry and permitted coverage; measure bytes/rate/retained state within the chosen presence profile. |
| **03.W03 — One Sand in spatial and readable views** | Built-in presentation bindings; 03.M04. Open spatial content in flat/HUD, edit it, close that view and return. | Readable sizing, focus/input ownership and return to the spatial anchor work through ordinary controls. | One accepted content identity/revision; anchor/dimensions survive; closing demand does not release another active view's assets; record aggregate residency, input latency and last-demand release. |

- [ ] **Checks to select.** Unit/property tests vary units, parent transforms, origin cells and magnification; compare relative evaluation against bounded references. Integration tests save/reload frame descriptors and exercise lease sharing, stale presence and denied projections. Native replay types exact values, switches view modes, picks the same object and transfers focus between spatial/flat content.

- [ ] **Earlier-work repair.** Replay 02.W01 and the affected acceptance/permission cases from 02.W02/02.W03 after coordinate or presentation changes. Save/undo/Agent operations must retain their scope and IDs even when the visible origin, unit display or focused presentation changes.

## Concrete acceptance replay

- [ ] **03.P01 — Precision independent of camera; 03.T01–03.T04 / 03.W01.** Type two distinct sub-nanometer authored values in a suitable local fixture, switch display units, undo and reopen. Open two distant close-up views and shift each render origin while picking the same occurrence/subelement. Compare accepted authored values and physical dimensions before/after view-only changes, and relative transforms against an independently calculated local reference. Record local error, coordinate ranges, view count, conversion/selection latency and aggregate frame/residency cost; review local wobble and numeric editing usability.

- [ ] **03.P02 — Bounded nonphysical presence; 03.T05 / 03.W02.** Publish permitted cursor/avatar state from one participant while another independently navigates a distant frame. Inject reordered/expired packets, opt out and disconnect. Verify frame mapping, sequence/expiry bounds, zero accepted model edits and zero bodies created by presence. Record actual packet size/rate and retained session state against the presence profile; inspect apparent placement and disappearance. Preserve 02.W02 denial/reconnect behavior.

- [ ] **03.P03 — One content source, several presentations; 03.T06/03.T07 / 03.W03.** Type into the spatial Sand through its supported input, open its flat/HUD view, continue editing and close that view while the spatial one remains. Verify one content identity/revision, independent local focus/layout, unchanged spatial anchor/dimensions and correct lease retention. Close the last view and measure release after the declared fences/grace. Review readable sizing and focus return; replay 02.W01 undo/reopen and the affected native/Agent target resolution with the focused presentation changed.

## Worked contract example

- [ ] **03.X01.** Prove relative evaluation near a common ancestor rather than subtracting giant floating-point world positions. This local example intentionally refuses a distant direct subtraction; select the real hierarchy and tolerance contract from the bounded fixtures. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  #[derive(Clone, Copy)]
  struct Axis {
      cell: i128,
      residual_m: f64,
  }

  fn nearby_delta_m(a: Axis, b: Axis, cell_m: f64) -> Option<f64> {
      if !cell_m.is_finite() || cell_m <= 0.0 {
          return None;
      }
      if !a.residual_m.is_finite() || !b.residual_m.is_finite() {
          return None;
      }
      let cells = a.cell.checked_sub(b.cell)?;
      if cells.unsigned_abs() > 1_000_000 {
          return None;
      }
      let delta = cells as f64 * cell_m + (a.residual_m - b.residual_m);
      delta.is_finite().then_some(delta)
  }

  fn main() {
      let origin = 1_i128 << 100;
      let a = Axis {
          cell: origin,
          residual_m: 0.040,
      };
      let b = Axis {
          cell: origin,
          residual_m: 0.039,
      };
      let delta = nearby_delta_m(a, b, 1024.0).unwrap();
      assert!((delta - 0.001).abs() < 1e-12);
  }
  ```

- [ ] **03.X02.** Exercise shared source identity and independent presentation state before stage 07 adds richer mounts/application surfaces. This is an illustrative replay fixture to implement through the real native/Agent services, not a new public command schema or portable workspace encoding. Keep its evidence pending until that workflow passes.

  ```json
  {
    "content": "sand:home-tasks",
    "subject": "record:home-tasks",
    "presentations": [
      {
        "id": "view:spatial",
        "binding": "model-local anchor",
        "source": "sand:home-tasks"
      },
      {
        "id": "view:focused",
        "binding": "view-local flat or HUD",
        "source": "sand:home-tasks"
      }
    ],
    "actions": [
      "edit allowed task in focused view",
      "inspect same accepted revision spatially",
      "close focused view",
      "return to original anchor"
    ],
    "expected": {
      "source_copies_created": 0,
      "canonical_pose_changed": false,
      "independent_tool_state": true
    },
    "verification": "pending"
  }
  ```

## Architecture and task requirements

### Confirmed intent and context recovery

- [ ] **R0016.** **C05 — Detail is a spectrum.** Authoring activity, display magnification, scientific fidelity, physical extent, solver sample size and output purpose are independent choices. “Micro edit” and “normal use” describe useful interactions, not two separate engines or a universal fixed scale threshold.

### Proposed defaults for refinement

- [ ] **R0054.** Physical units — **Proposed default:** Dimensioned local values; canonical descriptors preserve exact authored values; solver arrays use declared floating-point units — **When it may change:** A solver requires another declared unit system

- [ ] **R0064.** Presence and runtime — **Proposed default:** Optional lightweight avatars/cursors/selections; participant views stay local; one designated writer per active domain/run — **When it may change:** Physical interaction and worker spending have separate grants; shared simulation profiles grow at their own stages

### K2. Frames, precision, units and clocks

- [ ] **R0132.** A frame descriptor includes ID/revision, parent, axis order, handedness, unit scale, spatial reference and validity interval where needed. Rigid local transforms use translation and a normalized rotation. Unit conversion, a physical deformation, display enlargement, a mirror and a geodetic coordinate transformation are separate operations with separate semantics.

- [ ] **R0133.** For a Cartesian parent, a point transforms as `p_parent = t + R (s_unit p_local)`. Large translations use an integer cell address plus a bounded local residual: `t = cell × cell_extent + residual`. Evaluate relative transforms through a nearby common ancestor before converting to floating point. Never flatten an atom in a remote building into one enormous meter-valued vector and subtract nearly equal numbers afterward.

- [ ] **R0134.** Persist frame-local high-precision values independently of renderer origins. Bound accumulated transform error through nested rotations/unit changes; integer address range alone does not guarantee tiny relative accuracy. If a requested cross-frame measurement exceeds the available precision or source accuracy, report that limit or evaluate in a suitable local frame. Specify cell extent and residual normalization per frame, checked integer arithmetic and representable-error bounds. i128 cell coordinates are a candidate, not a reason to accept unlimited recursion or allocation.

- [ ] **R0135.** A user can inspect two tiny samples at distant locations in separate view roots. Camera-relative transforms and render copies belong to each view; one floating origin cannot simultaneously maximize precision for distant cameras. Shared immutable assets can be reused across views while their transforms, visibility and runtime state stay independent. [big_space documents nested grids and independent roots](https://docs.rs/big_space/latest/big_space/); the integration must prove the required case.

- [ ] **R0136.** Display magnification is a view transform. Showing a molecule beside a house does not give its atoms house-sized bonds, mass, collision or gravity. Moving a view inset does not relocate the physical sample. Physical resizing is a separate edit that invalidates dependent properties.

- [ ] **R0137.** Keep scene frames separate from solver frames. Each solver owns a numerically appropriate local length/time/force scale. Convert at boundaries and record the conversion. Avian and `big_space` must never both write the canonical pose or fight over the same transform component.

- [ ] **R0138.** Distinguish frame motion from object motion. A rotating/accelerating simulation frame requires the appropriate velocity transformations and inertial terms, or the adapter rejects it. A render-origin change alone never injects velocity, impulses or work.

- [ ] **R0139.** Geographic frames declare horizontal CRS, vertical datum, axis order, units, epoch and required transform resources. Use established geodesy through a bounded adapter. Missing datum grids offline must produce unavailable/declared approximate alignment, not an unnoticed ballpark transformation. [PROJ describes operation accuracy, area of use and grid availability](https://proj.org/en/stable/operations/operations_computation.html).

- [ ] **R0140.** Earth context uses geographic/geocentric references with a curved ellipsoid and explicit height datum; default zero added relief is a declared fallback rather than a measurement. Site work uses a surveyed or explicitly approximate local engineering frame. Dateline/poles, terrain heights, building elevations and curvature have explicit handling. A map projection's horizontal distortion is not acceptable engineering geometry by default.

- [ ] **R0141.** Astronomical states declare origin, orientation, epoch, time scale and geometric/apparent interpretation. Ephemerides are optional versioned inputs. At cosmological extents, proper/comoving distance and model assumptions need a domain coordinate map; a huge Euclidean scene is addressability, not cosmology. [NASA NAIF documents SPICE state/frame/time conventions](https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/C/req/spk.html).

- [ ] **R0142.** There is no single universe-sized absolute epsilon. Geometry, picking, alignment, contact and solver tolerances are dimensioned, bounded relative to the local feature scale and reported separately. Precision cannot exceed source accuracy.

- [ ] **R0143.** A clock has a time scale/epoch and a local numerical coordinate. Interactive ticks, scientific femtoseconds, calendar instants and astronomical epochs need conversion contracts, not a single millisecond counter. Persist exact event ordering/tick indices where required; use time windows or epoch-relative values to avoid subtracting huge timestamps. Views, recordings and physical runs choose their clocks without changing operational history.

- [ ] **R0144.** Scene scale policy is data-driven: root/frame units and valid numerical ranges, relative bounds, purpose-specific error and admitted demand determine work. The common model/view engine contains no Earth identity, mandatory astronomical parent, fixed ball size or named distance thresholds for switching scene stages. Geographic ellipsoids, datum/CRS handling and astronomical states are optional profiles/adapters over K2, not conditions for using scale. Standard unit conversions and explicitly selected physical laws retain their own constants and conventions. A complete local room can be a root indefinitely; attaching a larger context uses the same explicit frame/composition bridge and preserves local physical dimensions and identity/correspondence.

### Canvas evolution and native workflows

- [ ] **R0289.** Replace the flat-plane assumption through a staged spatial document/view separation. Preserve useful 2D work: the ordinary canvas becomes an orthographic/layout view onto the same selected model/Records, while perspective, plan/section, molecule, CAD sketch and geographic views choose appropriate navigation and tools. Do not maintain parallel truth in a “3D canvas database.”

- [ ] **R0290.** A view selects document/base revision, root/occurrence, display frame, projection, detail policy, visible layers, selected time/result and tool context. Presentation transforms are not canonical placement. A workspace may contain views into several models; a model can appear in several workspaces under different grants. Spatial views persist explicit navigation mode, camera/pivot/frame, speed preferences and safe near/depth policy separately from model placement; Orbit/Fly/Walk change controls inside one scene. Geographic perspective is a globe and supports street/horizon/interior views. The ordinary blank grid needs no geographic root or active Earth resources.

- [ ] **R0291.** Represent Sands/Castles as existing content objects with a bounded surface adapter: display bounds, pointer/keyboard focus, local hit coordinates, minimum readable presentation, lifecycle/demand and optional physical body. Content remains usable in a normal flat/native view.

- [ ] **R0292.** Surface mounts are explicit constraints/attachments, not gravity exceptions inferred from proximity. Mounted content follows a verified surface frame; unmounted content follows its chosen motion/layout policy. A UI card's interaction collider and mass are authored interaction parameters, not a claim that digital text has material density.

- [ ] **R0293.** Initially mount readable planar content to a planar/tangent patch with an explicit normal offset and sidedness. Curved/deforming content requires its own tessellation, hit-coordinate map and input proof; a curved model surface does not establish supported curved UI or physical projection. A source topology change preserves focus/content state while the mount is repaired.

- [ ] **R0294.** The compositor/window-management path owns real OS surfaces, focus, input and output windows. Model views can display assigned textures/proxies where supported; they do not take ownership of arbitrary application windows or bypass output/pairing policies.

- [ ] **R0295.** Selection and commands share stable occurrence IDs across perspective, map, tree, table, sketch and section views. Entering an assembly changes editing context, not ownership or source identity. Units, active frame and edit target stay visible in tool properties.

- [ ] **R0296.** Provide shared native transaction tools: select/subelement select; transform; numeric dimensions; align/snap with typed references; inspect; create/instance; preview/cancel/commit; undo/redo; source/override repair; and explicit bake. Domain tools plug into these interactions rather than inventing independent save/undo systems.

- [ ] **R0297.** Focused presets expose relevant controls: room tools show walls/openings/mounts; chemistry shows atom/bond/lattice tools; CAD shows sketches/features/constraints; material studies show conditions/models/results; geography shows region/detail/offline controls. Expert data stays inspectable without making every user configure a universal graph.

- [ ] **R0298.** Preserve keyboard operation, accessible tree/forms, text labels for statuses, configurable shortcuts and recovery from a trapped camera/tool. Objects can be found and edited through a list without precise 3D picking. Colors and shading are not the only representation of error or uncertainty.

- [ ] **R0299.** Build a bridge from current placements/Room metadata to model handles. Existing bounds remain admission limits for live UI content, not limits on static world definitions. Prove one end-to-end replacement path before retiring an old code path; avoid a prolonged duplicate frontend or a single unreviewable canvas rewrite. The old `web` crate is not the implementation destination.

- [ ] **R0300.** The same bridge extends a hosted workspace through stable model references and H1–H10. The host's live Sand placement cap does not count every inactive mesh occurrence in a city, nor does static model capacity authorize unbounded live Sands. Retain those independently budgeted representations and materialize only current permitted demand.

### Interface contracts I1–I6

- [ ] **R0301.** These are shared technological primitives for interface behavior. They do not assume construction, games, laboratory science, physical projection or any single input device. Domain controls specialize typed bindings and tool capabilities; they cannot bypass the shared contracts.

- [ ] **R0302.** **I1 — View and binding.** A view pins or follows a declared model/query/result revision, support, frame, representation, clock selection and read policy. A binding addresses a typed value or authorized operation. Tables, sequence/structure views, plots, sketches and spatial scenes can select the same stable subject. View navigation/magnification does not edit physical state. Each view shows which accepted/draft/result state it represents; switching views preserves tool/focus state or cancels it explicitly. A hosted view also identifies its participant/principal, projection epoch and last usable/materializing revision. Local camera/inspector/playback state is shared only through an explicit allowed binding; the same policy governs a tree/table view. Orbit focus is a declared target/bounds; movement scale and Walk up/frame policy come from the selected scene/context. They never assume Earth's center or a fixed zoom altitude. Navigation up and physical gravity remain distinct declared inputs; a scene without suitable Walk support can still expose supported Orbit/Fly controls.

- [ ] **R0308.** Native completion means the same command/selection can be exercised through at least two suitable presentations when its stage requires them, with coherent revision and status. A global “edit mode” boolean is insufficient: editing scope/activity belongs to a view/context, so a protein draft, ordinary room view and independent experiment can coexist without sharing mutable tool state.

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0837.** **Q10 — Optional lightweight shared presence.** Provide optional avatars, cursors and selections using shared view/presentation/anchor/activity primitives. Navigation/local tool state stays participant-owned unless explicitly shared. Physical bodies, live topology/Area effects and operational control need their own supported authorized activity.

### Content and presentation identity

- [ ] Keep content identity, canonical object placement, per-presentation placement and view-local interaction state independently owned. A HUD pin is a presentation choice; it does not move a world object, apply a force or require Earth. A spatial label, flat inspector and focused Sand can refer to the same authorized subject at different readable scales.

- [ ] Preserve one authoritative edit path and source revision across native views; query/select/edit results retain their stable subject/base. Sharing source data does not share another participant's private camera, HUD layout or credentials. Use existing visibility and demand rules for saved presentations rather than implicitly creating mutations when a camera moves.

- [ ] Respect the content adapter's supported presentation/focus behavior. For a source with one active input owner, transferring focus does not duplicate its producer or application process. Use existing compositor/output mapping for application surfaces in stage 07; native content/view contracts do not depend on a projector, camera driver or world model.

### Presence identity and articulated representations

- [ ] Preserve the distinction between participant/Actor identity, the selected model occurrence and its optional visible avatar. Replacing or hiding the appearance does not change grants or create a physical body. Stage 05 adds the first/third-person camera/controller use; stage 11 supplies editable rigs and anatomical representations through these same frame, identity and demand contracts. The richer future appearance is not a prerequisite for this stage's lightweight presence.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0738.** B13 — **Fixture:** Sparse astronomical parent with two distant tiny inspection views — **Proof required:** Cell/frame error, independent origins, no canonical drift or extent-sized allocation

### Owned fixture specifications

- [ ] **R0748.** B23 — **Fixture:** A bounded numeric/snap/constraint edit at microscopic and meter scales, in rotated/distant local frames and different unit displays — **Proof required:** Same supported authored intent through spatial/numeric/programmatic controls, no source rounding or camera-induced drift, explicit tie/stale handling, dimensioned tolerance, underconstraint/conflict diagnostics and measured candidates/iterations/latency

### Owned fixture specifications

- [ ] **R0761.** B36 — **Fixture:** Independent participant origins/interests, optional avatars/cursors/selections, continuous shared previews, bulk transfers, slow readers and rapid navigation — **Proof required:** Correct frame/source identity, private local state, bounded authorized interest/fanout/queues and host/client resources, control responsiveness under serving load, replaceable presence versus reliable accepted edits, gap/reset recovery, exact final commits and last-demand release; no implicit avatar physics or operational authority

## Workflow, correctness and performance evidence

- [ ] Instantiate the local **03.W** acceptance rows before their implementation: pin the real fixture/profile, applicable hard assertions/numeric limits, quality rubric and unit/integration/fault/native/performance evidence. Apply the existing owned requirements as well as the matrix. Record each relevant previously verified workflow exercised in composition and any temporary regression/repair; measure through actual services rather than counting checked tasks.

- [ ] **Real workload.** Inspect and edit small components in two distant authored assemblies while keeping the existing native canvas usable. Vary empty address extent at a fixed populated/active set and measure transforms, view preparation, input response and retained bytes.

- [ ] **R0559.** **[stage 03](03-stage.md) — L2/L3 on admitted L0 work** — **Intention and defined deliverable:** Local/frame/unit-accurate views, numeric controls and presentation magnification on the same subject — **Correctness/failure evidence and performance obligation:** B13/B23 and view replay: origin changes preserve physical state; root specimen needs no Earth/building. Measure transform/view preparation versus active occurrences, p95/p99 and bytes; sparse extent must not allocate its empty volume. Include subnanometer key/source distinctions; fixed-world rounding cannot substitute for a precision contract. B36 starts participant-local camera/frame presence and expiry without source/physics mutation. Prepare B38 parameterized frames/units/origins without a geographic parent or named scale bands.

- [ ] Run the actual native/backend/persistence/transport path for every 03.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

- [ ] **Presentation coherence evidence.** Extend B13/B22/B23/B36 with the same native content across spatial and readable flat/HUD views: actual edit/selection, local focus, save/reload, view close and independent participant state. Record aggregate active-view bytes/work and source reuse, preserving existing frame/input budgets; geometry magnification or a second source copy cannot substitute for the workflow.

## Advance gate

- [ ] Verify every local **03.W** contract and required milestone with both hard assertions/measurements and recorded qualitative review. Confirm all required earlier capabilities still hold using current affected proofs and retained unchanged evidence; close recorded regressions at their owning stages before advancement. Optional examples and deferred calculations remain explicitly scoped, and no required workflow can disappear during summary compression.

- [ ] **R0600.** a local millimeter edit and a microscopic magnified inspection remain numerically stable at distant roots. No giant root f64 coordinate is used as the source of truth.

- [ ] Advance only when **all 03.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

- [ ] Request owner review of the concrete workflow/evidence where implementation approval or a changed scope/budget is needed; reuse approval already present. Do not replace missing behavior with weaker acceptance wording or silently pull deferred science into a core milestone.

## Completion summary

- [ ] Keep this handoff factual throughout implementation: record verified slices and evidence while the stage remains pending/in progress, retaining full detail for unfinished scope. After the gate passes, finalize one or two short paragraphs describing what was implemented, actual module/interface ownership and why; include a consequential rejected alternative briefly when useful. Add a distinct compact paragraph for every **03.W** workflow, preserving original milestone IDs, built-in versus recipe scope, required behavior, hard bounds, qualitative criteria and evidence links. Update this stage's registry status in `00-General.md` only with verified evidence; a regression changes current verification to `Repair required` until repaired.

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
