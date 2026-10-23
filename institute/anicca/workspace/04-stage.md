# 04 — Selections, shared regions, mounts and Areas

Finish with one shared selection/region system across spatial and flat views. Draw an influence region, put its attraction target near the top or outside it, bind it to a cursor/camera/item and interact with mounted content on a rotated modeled surface.

## Required workflow milestones

- [ ] **04.M01.** Draw a circle, move its attraction point near the top edge without moving the region, then use a sphere with a higher local target. Existing capability filters, falloff and immunity agree on the affected set.

- [ ] **04.M02.** Bind a formula/effect or Area to cursor, an explicitly shared camera or another CanvasItem with a local offset. Move that anchor and see the same bounded effect follow; equal/opposite additive topology terms cancel under the declared composition.

- [ ] **04.M03.** Select the same occurrence/subelement from scene, tree/table and numeric tools, including several matching copies. A stale/denied/deleted selection cannot silently address another object.

- [ ] **04.M04.** Mount a Sand to a verified wall frame, move/rotate its containing space and remove the supporting face. Content retains identity/last pose and has a usable repair path; ordinary unmounted Sands keep existing gravity.

## Entry and scope

- [ ] Confirm the preceding [stage 03](03-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 02 summary](02-stage.md#completion-summary); [stage 03 summary](03-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed)`; `crates/interface/src/native/area.rs`; `crates/interface/src/native/area_target.rs`; `crates/interface/src/native/topology/influence.rs`.

## Ordered implementation tasks


- [ ] **04.T01.** Implement capability-based queries and stable named selections/correspondence for planes, meshes and the bounded point/site/graph support subset exercised by core structural authoring; extend picking to carry occurrence/revision/frame. Complete the query/identity part of T1. Add the supported snap candidate/cycling and construction-frame subset of the precise-authoring contract; retain explicit source/numerical tolerance and separate persistent constraints.

- [ ] **04.T02.** Implement T4–T6 using shared region queries, independent 3D targets, existing immunity and cursor/camera/item bindings. Keep source/target membership, falloff and explicit sampling policy consistent.

- [ ] **04.T03.** Connect content mounts to verified local surface frames with unresolved/repair behavior. Add native region/target/mount inspectors, undo and persistence. Test rotated Rooms, multiple surfaces, stale picks, deleted anchors and all immunity cases.

- [ ] **04.T04.** Implement correspondence evidence/coverage and selector cardinality for the supports already required here, with explicit occurrence/chart anchors and last-pose repair. A reordered mesh preserves known IDs, and an unsupported split/merge stays unresolved; full CAD construction history and complete analytic-loop transitions are later B29/[stage 10](10-stage.md) work.

- [ ] **04.T05.** Extend existing policy and E4 queries to the model/occurrence/property scopes needed here and authorized spatial/semantic interests. Prove B34/B36 server-side denial for metadata, selection, bounds and partial asset/source references before delivery, including a public guest and a narrower identified viewer. Reuse the same query/anchor path for hosted mounted Sands and Areas; shared visibility never lends operational/host authority. Prepare B40's captured multi-selection/scope and authorized matching-copy queries without a full-world scan. Preserve per-occurrence state and distinguish a selected-set edit from a deliberate shared-definition update.

## Worked contract example

- [ ] **04.X01.** Use distinct region and target inputs throughout existing Area commands. Extend the example with frame conversion, independent target direction, invalid-coordinate rejection, policy checks and the actual supported immunity cases. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn inside_sphere(point: [f64; 3], center: [f64; 3], radius: f64) -> bool {
      if !radius.is_finite() || radius < 0.0 {
          return false;
      }
      let distance = (point[0] - center[0])
          .hypot(point[1] - center[1])
          .hypot(point[2] - center[2]);
      distance <= radius
  }

  fn main() {
      let center = [0.0, 0.0, 0.0];
      let subject = [0.0, 0.8, 0.0];
      let target_a = [0.0, 0.9, 0.0];
      let target_b = [0.0, 3.0, 1.0];
      assert!(inside_sphere(subject, center, 1.0));
      assert_ne!(target_a, target_b);
      assert!(inside_sphere(subject, center, 1.0));
  }
  ```

## Architecture and task requirements

### Source baseline and shared families

- [ ] **R0070.** Terrain: `crates/nucleus/src/terrain.rs`, `terrain/formula.rs`, native `topology/terrain/` — **What to reuse:** Bounded compiled formulas, cursor/camera/item bindings, additive floor effects and editor — **Required extension or boundary:** Height queries remain one surface profile. Existing coordinate clamps and canvas sizes are not physical universe limits. Preserve guards while adding frame-local contracts.

- [ ] **R0071.** Areas: native `area.rs`, `area_target.rs`, `area_effects.rs`, `topology/influence.rs` — **What to reuse:** Region effects, off-center 2D attraction target, binding and immunity semantics — **Required extension or boundary:** Extract common region queries, add a third target coordinate and supported region shapes; preserve capability-specific behavior.

### K3. Spatial support, topology, selections and correspondence

- [ ] **R0145.** A **support** is the domain on which a value, material assignment, edit or boundary condition acts: point set, graph, curve, surface, volume, geographic region or discrete resource set. Its descriptor states dimension, local frame, bounds, topology representation and available queries. A support is not automatically a closed solid. Constant fallback geometry, imported topography and authored landscapes provide the same capability-based surface-source contract with units/frame, coverage, bounds and provenance; geography is an adapter rather than a separate topology engine.

- [ ] **R0146.** Use a capability contract instead of forcing every representation to answer every query: bounds; ray intersections; closest point; oriented normal; inside/outside classification; distance with error; adjacency; measure; and semantic subelement lookup. Every answer includes source revision and exact/approximate/unknown status. An open scan may support ray picking but have no valid volume or inside test. Bounds accelerate a query; they do not prove containment.

- [ ] **R0147.** A **named selection** identifies a face group, volume region, lattice sites, material interface, ports, boundary or semantic objects. It can be explicit stable subelement IDs or a versioned selector with expected cardinality and evaluation rules. Unsupported or ambiguous reevaluation invalidates consumers. Nearest-face matching may be a suggested repair, never silent acceptance.

- [ ] **R0148.** Atom/site IDs, source feature IDs and mesh half-edge/face IDs stay in compact domain buffers. IDs survive harmless reorderings. A split or merge records `unchanged`, `generated`, `modified`, `split`, `merged`, `removed` or `ambiguous` correspondence with provenance.

- [ ] **R0149.** A mount or boundary condition records its owning occurrence, named selection, local chart/coordinates, orientation, offset and source revision. Mesh barycentric coordinates can locate a point in one revision; they are not durable identity after remeshing. Preserve a last resolved pose and expose repair when no unique successor exists.

- [ ] **R0150.** Renderer picking returns root, occurrence, selection/subelement, representation revision, view and coordinate frame. A delayed pick from an obsolete revision is resolved through a verified mapping or rejected. Invisible, unauthorized or proxy-only geometry cannot select private semantic data.

- [ ] **R0151.** Correspondence between source, render, collision and solver geometry is an explicit artifact. Many-to-one mapping after simplification is normal. A result can only be displayed where its mapping is valid; unsupported gaps stay visible.

- [ ] **R0152.** Region queries use the same frame and boundary rules for Areas, selection, authoring masks, material assignment and solver boundaries. A domain can request different sampling policies, such as an item's anchor point versus occupied-volume overlap; this policy is explicit rather than a disagreement between capabilities.

- [ ] **R0153.** Legal/jurisdictional regions and measured geometry can share query infrastructure but retain their distinct authority and uncertainty. A drawn Area never creates ownership.

- [ ] **R0154.** The SolidCraft refinement in [stage 10 CAD requirements](10-stage.md#architecture-and-task-requirements) makes correspondence evidence explicit: operation-origin/role history, verified numerical mapping and heuristic repair candidates have distinct status. Selectors declare whether they expect one successor, a complete boundary/set or a chosen piece. Only validated coverage composes across mappings; a missing or ambiguous intermediate step cannot gain certainty downstream. A chemical site mapping, geographic feature match and CAD face history use domain payloads over this same envelope.

### K5. Relations, ports and constraints

- [ ] **R0164.** Typed relations connect definitions, occurrences, selections, evidence and Records. Relations do not all execute equations. Semantic dependency, chemical bond, CAD mate, electrical net, fluid connection, thermal contact, material interface and resource handoff declare their own schemas and validators.

- [ ] **R0165.** A **port** identifies a localized interface, supported variables and orientation/sign convention. A **constraint** states a domain equation/inequality or admissibility rule and the solver responsible for enforcing it. Shared infrastructure handles references, units, revisions, selection, visualization and diagnostics; the domain solver supplies mathematical meaning.

- [ ] **R0166.** A CAD coincident constraint, a rigid hinge, a covalent bond and a welded finite-element boundary can use the same reference envelope without pretending they are one spring.

- [ ] **R0167.** Connection loops are allowed where the domain supports them. Report unconstrained degrees of freedom, redundant/conflicting constraints and numerical singularity. A cyclic assembly is not an illegal composition cycle.

- [ ] **R0168.** Distinguish a dependency arrow from a conserved exchange. Energy, mass, momentum, charge and operational inventory each require the appropriate balance and units. Store port direction and whether a value is an intensive state or an extensive/integrated transfer.

- [ ] **R0170.** Transforming a constrained object supplies a solver target or explicitly changes/detaches a constraint; it cannot silently overwrite an incompatible pose. Report which definition, occurrence override or constraint would change before commit.

- [ ] **R0171.** Constraint satisfaction, material/valence validation and legal/operational policy are different checks. An invalid creative design can be saved as a draft with diagnostics; it cannot be represented as a passed solver result or an accepted prohibited operation.

### H5. Sparse subscriptions and independent delivery policies

- [ ] **R0264.** Extend E4/I6 with participant-scoped spatial/semantic/time interest, allowed fields, requested purposes/error, update policy and quota. Interest is an upper demand bound, not permission. Initial state carries coherent revision/authority epoch and a replay cursor; accepted changes are atomic within their declared authority. New spatial tiles or representations carry exact source/frame/purpose keys and admission generations. Independent camera origins remain view-local; exchange canonical/local-frame references, never raw renderer-relative positions. Any lossy pose/field encoding declares scale, error and purpose rather than applying a universal world-unit rounding rule.

- [ ] **R0265.** Commands, accepted metadata and receipts — **Ordering, loss and acceptance:** Reliable authorized processing, idempotency and coherent scope ordering; reconnect queries the recorded request outcome. A lost reply cannot create another accepted effect

- [ ] **R0266.** Immutable assets — **Ordering, loss and acceptance:** Bounded verified/resumable delivery with independent priority/windows, access checks and local admission before decode/upload; partial bytes never become a falsely complete mandatory profile

- [ ] **R0267.** Presence and optional shared previews — **Ordering, loss and acceptance:** Session-scoped sequence/expiry and latest-useful-state coalescing; loss is permitted. They do not advance accepted heads, operational effects or physics

- [ ] **R0268.** Run observations/media — **Ordering, loss and acceptance:** Run/source/clock/sequence tagged with explicit sampling, coalescing, gap and retention policy; an interpolation is not a solver step or accepted measurement

- [ ] **R0269.** Reuse the current Iroh/QUIC transport behind these contracts. Evaluate separate prioritized streams/lanes for control, assets and replaceable observations, with per-lane and aggregate byte/work bounds; independent streams still share congestion, endpoint CPU and egress capacity. The current serial live loop and its message-rate/frame limits need a measured extension, not deletion of guards or a claim that stream multiplexing alone prevents starvation. No geometry blobs in giant layout snapshots or base64 Protein rows. Preserve the existing small live-Sand/layout limits while adding paged asset-backed model references; raise a limit only with evidence for that path. Share authorized source assets and query work across overlapping participants where safe, while each participant retains its own demand and bounded queue.

- [ ] **R0270.** Slow readers must not hold the host acceptance transaction or exhaust all queues. Coalesce replaceable observations; when an accepted-state replay tail exceeds its bound, move the client to a new coherent snapshot with an explicit gap/reset rather than dropping an edit and pretending continuity. Keep receipts retrievable under the declared retention/recovery policy. Metadata may identify a newly accepted artifact before a visitor has materialized it: label the last usable displayed revision and pending coverage, and preserve/reconcile valid local support. Never label an old mesh/current metadata combination as a single current scientific or physical state. Cancel obsolete ranges/work and check authority again before attaching completed data. A revoked participant cannot acquire newly usable private data through an in-flight completion or another consumer's cache lease; already delivered plaintext has the stated revocation limit.

### Interface contracts I1–I6

- [ ] **R0304.** **I3 — Selection, query and region.** Selection can address an occurrence, named subelement, typed property, graph/site set or query result. A region predicate declares its frame, supported membership/sampling rule, tolerance/uncertainty and revision. Area effects, filters, material assignment and tool masks reuse it. Force destination, drawn outline, affected region and access authority stay distinct. Hover/pick events carry view/source revision; stale events resolve through validated correspondence or fail visibly. Remote interest narrows allowed demand and never grants access. Source/actor policy applies to selectors, bounds, counts and returned subelements before delivery.

- [ ] **R0306.** **I5 — Anchor, attachment and registration.** An anchor identifies occurrence/support/frame plus local coordinates/orientation and follow policy. A label on a protein residue, a controller on a modeled mechanism and a Sand on a wall share this reference behavior. Registration relates independently sourced frames with a revision and uncertainty. A missing anchor preserves the last resolved presentation and offers repair; no nearest-object reassignment. Physical calibration, virtual placement and display attachment remain independently owned mappings.

### Live mathematical topology and anchors

- [ ] **R0325.** Preserve existing height effects as an explicit additive profile: `h(u,v) = h_base(u,v) + Σ a_i w_i(u,v) f_i(u,v)`. Here chart coordinates are transformed into each effect's local frame, `f` and falloff `w` have declared dimensionless inputs, and `a` has length units. Equal positive and negative contributions with the same support and shape cancel independently of order. Do not reinterpret these as Boolean union/subtraction; implicit solid operations have different semantics and do not generally cancel.

- [ ] **R0326.** For selected non-height surfaces, a bounded displacement operator can evaluate `p' = p + n_ref d(p)` from a captured reference surface. Declare which patches it affects, the normal/frame, support and foldover/self-intersection checks. Independent displacements that claim additive behavior sample the same reference revision. Sequential deformations are an ordered operator chain and make no cancellation promise. Topology-changing cuts/Boolean operations are separate operators.

- [ ] **R0327.** Bindings resolve a fixed frame, cursor hit/working plane, camera or stable item occurrence plus offset/orientation policy. A camera-bound region can follow the camera's actual position or its floor projection; the mode is explicit. Cursor ray misses, deleted anchors or unavailable streamed objects preserve the last valid effect pose and show unresolved status; no jump to world origin. Freeze, retarget or detach is a native edit. A camera moving through a place moves its influence under the selected binding, with all Area capabilities sharing the resulting region query.

- [ ] **R0328.** Dirty only tiles/patches intersecting the old and new effect bounds; retain seam halos. Limit formula size/evaluation count and reject nonfinite output through existing bounded expression infrastructure. While dragging, coarser preview is allowed with its status shown. Settled explicit bakes pin an effect/source revision; later source motion marks them stale and offers rebake, rather than silently changing a finished model or losing the live source.

### Performance research and adaptation boundaries

- [ ] **R0538.** [SpacetimeDB subscriptions](https://spacetimedb.com/docs/clients/subscriptions/semantics/) — **Documented mechanism:** A coherent initial subset, ordered atomic transaction updates and a consistent local client cache — **Concrete adaptation and limit in Lince:** One permission-aware projection can feed tree/table/plot/scene views. Define epochs/cursors, overlap/refcounts and resnapshot on gaps. Dense physics buffers and world blobs remain purpose-built data rather than one database row/update per particle per step.

### Execution contracts E1–E8

- [ ] **R0544.** **E4 — Coherent projections and subscriptions.** A query/view subscription declares authorized fields, typed predicates, spatial/time scope, bounds and update policy. Initialize from a coherent accepted revision plus epoch/cursor; apply each committed metadata change as an atomic delta before presenting callbacks. Multiple overlapping queries share permitted cached rows/assets with reference counts. Revocation or scope change invalidates the relevant cache/epoch; replay gaps trigger bounded resnapshot. Index-compatible queries maintain relevant subsets incrementally; unsupported/expensive predicates declare their scan cost or reject admission. Scientific/media streams may coalesce/drop superseded frames under their explicit policy, while command receipts/accepted-state deltas retain reliable ordering. A skipped visualization frame must never become a skipped accepted edit. H5 adds independent participant interests and delivery classes. Public projection cursors/metadata do not expose hidden edit counts, source hashes or private catalogue membership through a global document revision counter. Slow clients use a bounded explicit reset/resnapshot without blocking authority acceptance.

### T4. Share Area region geometry across capabilities

- [ ] **R0890.** Separate an Area's drawn shape, affected region and force target/direction while reusing the existing InfluenceArea model.

- [ ] **R0891.** Support a selected surface patch, a plane with explicit thickness, an extruded shape and a sphere. Define boundary inclusion and the coordinate frame consistently, including rotated Rooms and camera-bound Areas.

- [ ] **R0892.** Use the same region query for attraction, filtering, sorting, mutations, sound and immunity, preserving each capability's existing semantics.

- [ ] **R0893.** Acceptance: a sphere includes objects above and below its center; a wall patch addresses its selected surface rather than unrelated surfaces behind it; all capabilities agree about region membership.

### T5. Reuse and extend the independent attraction target

- [ ] **R0894.** Existing source already provides `AttractionTarget::Point`, an offset from the Area center, and a native draggable target handle in `crates/interface/src/native/area_target.rs`. Do not rebuild the off-center 2D feature as a separate system. Verify its current discoverability and behavior first.

- [ ] **R0895.** Make the intended interaction clear: draw a circle, then position its attraction target near the top of the circle without moving or resizing the circle. The target may also be outside the affected region.

- [ ] **R0896.** Extend the existing target representation and controls with a third local coordinate, allowing a target above, below or beside the affected objects. Provide a draggable handle, numeric coordinates and Reset to center through the existing Area editor.

- [ ] **R0897.** Preserve the distinction between selection and destination: region membership selects which Sands receive a force; the independent point determines its direction. Moving the target must not alter the affected region or filtering.

- [ ] **R0898.** Resolve targets in the Area/Room frame and preserve existing attraction, repulsion, force falloff, immunity, bindings, persistence and undo behavior. Collision still prevents a Sand from passing through walls or ceilings.

- [ ] **R0899.** Acceptance: a circular Area attracts toward a point near its top edge; moving that point leaves the circle unchanged. A sphere inside a cave can attract toward a higher point, with correct collisions and consistent behavior in both canvas views.

### T6. Reuse Area immunity for influence boundaries

- [ ] **R0900.** Reuse the existing None, External, Internal, All, Containment and Isolation immunity behavior with the new shared region queries. Expose it through existing Area controls rather than adding a parallel obstruction/permission system.

- [ ] **R0901.** Demonstrate a cave or room boundary that blocks incoming influence, retains outgoing influence, or isolates inside from outside according to the selected existing mode. Keep the source/target membership rules consistent across supported capabilities.

- [ ] **R0902.** Immunity classifies the source and affected object by region membership; it does not currently trace a ray through a wall. Automatic line-of-sight or material-dependent sound blocking is deferred unless explicitly requested. This preserves the requested primitive reuse without claiming that immunity already implements geometric visibility.

- [ ] **R0903.** Acceptance: exercise internal/external influence and containment across a rotated room or cave region, including immunity combined with an off-center target.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0730.** B05 — **Fixture:** A circle/sphere with an independent off-center/high target, rotated frame and moving anchor — **Proof required:** Region/target separation, above/below membership, all immunity modes, correct binding and undo

### Owned fixture specifications

- [ ] **R0744.** B19 — **Fixture:** Overlapping authorized scene/table/plot subscriptions and a changing source — **Proof required:** Coherent initial snapshot + atomic deltas, union/refcounts, bounded queues, indexed candidate counts, reconnect/gap resnapshot, grant/epoch changes and released demand

### Owned fixture specifications

- [ ] **R0747.** B22 — **Fixture:** The same stable selection/edit intent through native spatial/numeric/sequence/table views as applicable — **Proof required:** One backend operation/receipt, coherent draft/accepted/result state, accessible focus/cancel/undo, unresolved selection repair and no domain-specific save/tool-state fork

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Arrange real useful Sands on a room fixture and use camera/item-bound influence while navigating. Combine rotated regions, off-center targets, immunity and mixed participant access; measure indexed candidates and query latency instead of scanning every world object for each capability.

- [ ] **R0560.** **[stage 04](04-stage.md) — L2/L3, selected support adapters in L4** — **Intention and defined deliverable:** Shared selection/region/anchor/mount tools, validated snapping and independent Area destination — **Correctness/failure evidence and performance obligation:** B05/B22/B23: equivalent selection across views, rotated membership, target immobility of region, deleted anchor and denied/stale picks. Record query latency and candidate counts at fixed active-set size; no per-capability full-world scan. B34/B36 add server-enforced per-participant model/selection interest and denied metadata/source cases through the same queries. B40 begins captured multi-selection and authorized matching-copy scope for ordinary editing.

- [ ] Run the actual native/backend/persistence/transport path for every 04.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0606.** a sphere can influence above/below; a wall patch addresses its own surface; moving an off-center target leaves the region unchanged; mounting on a removed face never silently selects another face.

- [ ] Advance only when **all 04.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
