# 02 — Durable models, accepted edits and basic workspace entry

Finish with a small useful native workspace: create repeated objects, edit only the selected copy, undo, save and reopen without losing identity. An invited editor and restricted public visitor can enter that same tiny document under real workspace policy; this is bounded document collaboration before the larger spatial journey.

## Required workflow milestones

- [ ] **02.M01.** Create two copies of a supplied furnishing/part, attach a Record to one, edit its local placement and reopen the workspace. The other copy, stable occurrence IDs and Record association survive reorder/undo/save/reload.

- [ ] **02.M02.** Import a part with required permitted definitions. Reuse a verified matching definition, retain a conflicting same-name version and show unavailable dependencies without installing code or guessing physical properties.

- [ ] **02.M03.** Invite an identified editor and admit an explicit restricted public visitor. Let the editor make an allowed tiny-document change, deny the visitor the same mutation, and retain both editors' drafts when their expected bases conflict.

- [ ] **02.M04.** Interrupt save/transfer/commit acknowledgement and restart/reconnect. Recover the last accepted root and recorded request outcome without duplicate edits, widened grants or automatically submitted offline drafts.

## Entry and scope

- [ ] Confirm the preceding [stage 01](01-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 01 summary](01-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/workspace_model/ (proposed)`; `crates/nucleus/src/canvas/`; `crates/engine/src/`; `crates/store/`; `crates/transport/`; `crates/interface/src/native/workspace_sync/`.

## Ordered implementation tasks


- [ ] **02.T01.** Implement definition revisions, stable instance slots/occurrence handles, compact element IDs and optional Record bindings for a tiny repeated scene. Extend existing canonical/asset contracts instead of duplicating identity.

- [ ] **02.T02.** Implement directory save/load, required-profile/closure validation, atomic root publication/recovery and resource bounds. Expose create/open/save, tree/inspector and a revision-conflict flow in native UI. Evaluate compact shared authoring chunks for undo; instrument snapshot pointer/copy/retention costs without allocating an object per future numerical site.

- [ ] **02.T03.** Bridge one existing canvas placement and Room into a model handle; preserve normal selection/undo. Test save failure, crash recovery, repeated definitions, cycle rejection and stale edit results.

- [ ] **02.T04.** Add the minimum shared typed command/receipt, job-generation record and controlled completion/clock boundaries needed by these cases. Prove retry/idempotency and late-result rejection with B20. Instrument admission/queue/commit bytes and latency; defer unused scheduler/database features.

- [ ] **02.T05.** Expose the same typed operation availability, validation, created/changed stable identities and receipt through native controls and the existing authorized programmatic route. Return a minimal source change set and enforce the new modules' dependency boundaries through the existing check tooling; no additional universal command server or whole-application layer rewrite is required. Extend B01/B20/B22 with equivalent outcomes and denied/stale cases.

- [ ] **02.T06.** Extend the existing workspace identity/policy/typed command path to reference the tiny model document without embedding its graph in the live layout. Implement explicit identified and restricted-public principal handling, current viewer/editor/manager/ceiling checks and original-author revalidation. Reuse host configuration and native create/publish/invite/redeem/revoke controls; private hosting and optional public publication are explicit choices. Prove no guest route reaches trusted-local `None` authority and no failed/expired invitation widens access.

- [ ] **02.T07.** Add the minimal bounded entry/profile description, finite tiny-document size/required capability inspection, local cost/status view and reservation needed by B33. Capture authorized projection/manifest generation and actor/device admission in requests/cursors; retain receipts and host/workspace/base in local drafts. Exercise same-identity reconnect, lost replies and revoked-then-regranted admission with B34/B35. [stage 05](05-stage.md) extends this same path to sparse geographic assets and richer resource estimates.

- [ ] **02.T08.** Complete the first B39 definition-closure/reuse subset with the tiny document: import required permitted definitions, reuse verified identical references, preserve conflicting versions and provenance, and show a simple native imported/reused/unavailable summary. Keep definitions, instance overrides and domain/environment bindings distinct. Do not overwrite an existing shared definition by name or initialize a physical solver; Deferred [stage 16](16-stage.md)/[stage 18](18-stage.md) add actual material/gravity behavior to the same contract after [stage 15](15-stage.md); preserve existing implemented behavior and passive definitions early.

## Worked contract example

- [ ] **02.X01.** Extend this acceptance-order example through the actual authorized command/store/receipt path, including actor/device revalidation, idempotency and crash recovery. These example types are not selected production APIs. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  #[derive(Debug, PartialEq)]
  enum Outcome {
      Accepted,
      Conflict,
      Denied,
  }

  fn accept_edit(expected: u64, current: u64, can_edit: bool) -> Outcome {
      if !can_edit {
          return Outcome::Denied;
      }
      if expected != current {
          return Outcome::Conflict;
      }
      Outcome::Accepted
  }

  fn main() {
      assert_eq!(accept_edit(7, 7, true), Outcome::Accepted);
      assert_eq!(accept_edit(7, 8, true), Outcome::Conflict);
      assert_eq!(accept_edit(7, 7, false), Outcome::Denied);
  }
  ```

## Architecture and task requirements

### Confirmed intent and context recovery

- [ ] **R0017.** **C06 — Shared mechanisms, specialized mathematics.** The core handles identity, units/frames/clocks, support/selections, quantities/fields, relations/constraints, operators/dependencies, representations and domain state. Interfaces and runtimes share those contracts. Chemistry, CAD, buildings, factories, games and tracked displays are profiles/recipes and adapters, not privileged root types.

- [ ] **R0024.** **C13 — One local and hosted workspace model.** Hosting composes existing workspace identity/policy, model references, native Sands, typed commands, queries, artifacts and activities. Local/offline roots and remote participation use the same contracts; neither multiplayer nor an application domain owns another model engine. Private invitations and optional public visits without login are supported, with identified authorized editors.

- [ ] **R0026.** **C15 — Access does not propagate through a reference.** A workspace invite does not lend its host's Organ login, Protein data access, native execution, desktop or source credentials. Public guests have an explicit restricted principal. Other-Organ Sands retain their own source/session/policy; optional unavailable data remains localized. Assets, metadata, derived outputs and participant presence have disclosure checks.

- [ ] **R0027.** **C16 — Shared state has explicit authority and recovery.** Accepted edits and each active simulation domain/run have designated authority and revision/epoch checks. Optional avatars/cursors/selections are bounded presence; physical interaction is separately authorized. On initial host loss, keep permitted cached viewing and recoverable local drafts, revalidate on return and never replay mutations automatically. Later co-hosting requires explicit fenced ownership transfer; no automatic replacement writer or compute donation is implied.

### Proposed defaults for refinement

- [ ] **R0057.** Authoring exit/application — **Proposed default:** Automatically apply ready, applicable candidates on exit to the selected local variant; pending/invalid outputs retain the identified prior model and diagnostics — **When it may change:** Confirmed behavior; active-run transfer/restart/defer requires a supported domain policy

- [ ] **R0060.** Native persistence — **Proposed default:** Workspace manifests and content-addressed payloads; directory layout first, portable packed form next — **When it may change:** Measured access patterns justify an encoding change

- [ ] **R0062.** Hosting — **Proposed default:** Existing Cell/Organ and workspace identity/policy; private invitations plus optional public visiting; one shared model/document implementation — **When it may change:** Source audit determines the smallest session, authorization, asset and sparse-subscription extensions; no vendor service or second world engine

- [ ] **R0065.** Initial host loss — **Proposed default:** Permitted cached viewing and recoverable local drafts; revalidate and reconcile when the same authority returns — **When it may change:** Explicit future co-host transfer can extend availability only after its fencing/recovery proof

### Source baseline and shared families

- [ ] **R0069.** Canvas state and Room: `crates/nucleus/src/canvas/`, `crates/interface/src/native/canvas_host/`, `topology/rooms/` — **What to reuse:** Stable placements, scoped edits/undo, room-local poses, content/proxy roles — **Required extension or boundary:** Current Room bounds are 16 rooms, 256 members and depth 8. A streamed world cannot be a giant Room placement list. Add asset-backed spaces and materialize only active occurrences.

- [ ] **R0076.** Canonical artifacts: `crates/nucleus/src/karma/canonical.rs`, existing receipt/artifact delivery paths — **What to reuse:** Domain-separated SHA-256, canonical JSON for exact metadata, transfer identity and permission paths — **Required extension or boundary:** Canonical JSON currently rejects floats. Reuse that rule for manifests with explicit exact/binary scalar encodings; do not force geometry arrays into Record JSON. Verify delivery can carry general blobs before reusing it.

- [ ] **R0079.** Hosted workspaces/login: `crates/engine/src/workspace_sync.rs`, `login.rs`, `login_workspace.rs`; native `workspace_sync/`; `crates/transport/tests/workspace_live.rs` — **What to reuse:** Viewer/editor/manager audiences, role union/ceiling, proposals, actor admission generations, receipts, native policy/review and real-network test fixtures — **Required extension or boundary:** Current layout is bounded to 256 elements/exposed Records and 256 KiB; presence is capped at 64 participants. Preserve small live-content guards, add paged model references and interest subscriptions. The live transport authenticates with a granted identity or credentials; public guest admission is an explicit missing path, not `actor: None`. Tests are inspected reuse evidence, not certification of the larger model workload.

### Layer map and dependency boundaries

- [ ] **R0084.** The implementation is one system with six responsibility layers. The layers are domain-neutral and share the K/I/E contracts, accepted identities and receipts. They do not introduce separate databases or independent authoritative documents per workflow. The stages [01](01-stage.md)–[21](21-stage.md) sequence below delivers small vertical slices through the required layers; do not finish a universal framework in isolation before exposing useful native interactions.

- [ ] **R0085.** **L0 — Execution and evidence** — **Shared responsibility and contract:** E1–E8: admission budgets, jobs, buffers, I/O/completion, instrumentation, controlled clocks/faults — **Inputs → outputs:** Explicit work request + resource/validity contract → bounded execution, status, artifact and evidence — **First proofs and later reuse:** Minimal scheduler/metrics/fault harness at stages [01](01-stage.md)–[02](02-stage.md); grows only when streaming, geometry or science needs a capability. Hosting adds whole-host/per-participant admission, local entry checks and shared resource counters through E1/E8.

- [ ] **R0086.** **L1 — Durable change and artifacts** — **Shared responsibility and contract:** Identity envelopes, authorized expected-revision commands, receipts, immutable sources, recovery, dependencies and selective delivery — **Inputs → outputs:** Draft/command + actor/base → accepted revision/receipt or explicit conflict — **First proofs and later reuse:** Repeated tiny scene and save/recovery in [stage 02](02-stage.md); material assignment, laboratory runs and operations use the same acceptance boundary. The same accepted heads/receipts serve local and hosted projections; sessions, live grants and reconnect cursors stay outside portable source snapshots.

- [ ] **R0087.** **L2 — Model semantics** — **Shared responsibility and contract:** K1–K8: composition, typed frames/units/clocks, supports/selections, values/fields, relations, derivations, representations and domain state — **Inputs → outputs:** Validated typed definitions → resolvable model and purpose-specific evaluation inputs — **First proofs and later reuse:** stages [02](02-stage.md)–[04](04-stage.md) fixtures; no house, spacecraft, protein or ERP vocabulary hard-coded as a root requirement. No host, guest or network-specific object subclasses; model scopes and source references carry the same identities in every view.

- [ ] **R0088.** **L3 — Interface and interaction** — **Shared responsibility and contract:** I1–I6: views/bindings, surfaces/composition, selection/regions, tool transactions, anchors/registration and demand — **Inputs → outputs:** Model/query/result + view/tool context → native presentation, typed intentions and explicit demand — **First proofs and later reuse:** Tree/inspector first; flat/spatial/sequence/sketch/plot views reuse focus, commands, undo and status. Native invitation/entry/cost/status/presence controls bind the same permissioned operations and I6 leases.

- [ ] **R0089.** **L4 — Specialist profiles and evaluators** — **Shared responsibility and contract:** Typed domain payloads, validators, kernels, laws and mappings behind shared contracts — **Inputs → outputs:** A selected operation/experiment with supported inputs → geometry, applicable model or independent results — **First proofs and later reuse:** Surface/material/molecule/protein/CAD/circuit/continuum/orbital implementations; every profile owns its mathematical proof. H7 exposes admitted run observations and granted inputs without requiring every visitor to install or execute the evaluator.

- [ ] **R0090.** **L5 — Workflow composition** — **Shared responsibility and contract:** Saved arrangements of definitions, tools, query subscriptions, protocols, policies and output bindings — **Inputs → outputs:** User goal + selected capabilities/data → useful complete workflow — **First proofs and later reuse:** Room, protein laboratory, material control view, factory, city and solar-system workspaces; no forked save or runtime machinery. A shared visit, collaborative CAD session or laboratory observation composes the same capabilities and host contract.

- [ ] **R0091.** L0 services and the inert L1/L2 types are usable without a GUI. L3 depends on model contracts and host/output adapters, never on a specific domain's internal solver state. L4 evaluators consume captured inputs and return artifacts/results; they do not own L1 heads or directly mutate a live UI. L5 recipes depend on declared capabilities, not domain-casts hidden in the core. Runtime workers may execute concurrently while accepted changes are serialized only within their declared authority/revision scope.

- [ ] **R0092.** A layer interface declares ownership, units, revision/epoch, ordering, error/status, maximum sizes, cancellation, authorization and failure recovery. A layer cannot obtain hidden work by recursively inspecting arbitrary objects. Adapters declare dependencies and demand explicitly. Shared identities relate definitions and views; dense solver buffers retain compact local indexing with verified mappings back to those identities.

### Capability coverage and reusable workflow recipes

- [ ] **R0108.** The capability names below are scope anchors, not a new universal class hierarchy. Their delivery remains in S-stages with named proofs. A recipe can end at an authored object, an applicable model, an experiment, an operational event or a presentation; no macro destination is obligatory.

- [ ] **R0117.** **W09 — Hosting and participation** — **Shared primitives and thin specialization:** Existing workspace/Organ policy, explicit principals, I1–I6 views/presence, E1–E8 admission/acceptance/streams and H1–H10 — **Minimum proof / later expansion:** B33–B36: bounded entry into the same local/hosted space, public/private access, Protein federation, edits and recovery; B37 extends result/physical-run participation

- [ ] **R0118.** All workflows must share typed command validation, revision/conflict handling, unit/frame interpretation, source/result provenance, job lifecycle, resource admission, native tool state, subscription demand and persistence. Specialized geometry or equations remain in their profile. Reuse is proved across two different workflows; shared field labels or duplicated helper code are insufficient.

- [ ] **R0119.** W09 is available as a composition of those mechanisms for every workflow, not a prerequisite to local use. Each profile declares view/edit/interact/run capabilities and required dependencies; the same visitor can inspect a result while lacking permission or an installed adapter to recompute it. The native host and visitor surfaces must expose those distinctions.

### K1. Identity, definitions, occurrences and revisions

- [ ] **R0120.** Use distinct typed identities for a logical object, definition revision, instance slot, occurrence, representation, subelement, selection, derivation and run. Reuse existing ID conventions where their validation and namespace fit. Names, positions, Bevy entity IDs, triangle numbers, array offsets and source scene paths are never durable identity.

- [ ] **R0121.** A **definition** describes reusable authored content: a molecule/protein specimen, experimental protocol, behavior/material model, part, building module, room asset, tree species model or scene. Its immutable revision pins its structure and dependencies.

- [ ] **R0122.** An **instance slot** is a stable child placement in a definition, referencing an exact child revision, placement and explicit overrides. Reordering a list does not change slot identity. Expanding the same definition in two places produces different occurrences.

- [ ] **R0123.** An **occurrence** identifies a resolved instance in a composition: root instance plus stable slot path. It includes the selected composition revision when used in a pick or edit. Large repeated populations can use stable instance keys in columnar buffers instead of one allocated object per occurrence.

- [ ] **R0124.** A **Record binding** links a model object/occurrence/selection to one or more existing Records, under policy. The reverse may link a Record to many representations. Creating an atom, rivet, mesh vertex or map feature does not automatically create a Record or persistent ECS entity.

- [ ] **R0125.** A **revision** identifies immutable content. A logical object's current accepted head is a separately authorized reference. Hash identity does not replace logical identity: moving a chair changes its revision, not which chair it is.

- [ ] **R0126.** Shared definitions are pinned by default. “Update instances” presents affected instances, overridden fields and unresolved bindings. A private local override does not edit the shared definition. “Make unique” creates a fork with explicit lineage.

- [ ] **R0127.** Keep five graphs distinct: composition is an acyclic graph; each evaluated frame has one parent in an acyclic frame forest; derivation dependencies are acyclic within a revision; domain constraints/connections may contain cycles; semantic/Record links may also contain cycles. Expansion has depth/count budgets and rejects recursive instancing. Cyclic physical feedback belongs to an explicit solver domain, not a recursive derivation evaluator.

- [ ] **R0128.** Definition containment, spatial inclusion, constituent composition, mechanical attachment, ownership, operational custody and causation are different relation kinds. A door can be contained in a building, hinged to a wall, made of several materials and owned by someone else. None of those relations should accidentally impose another one's lifecycle or transform.

- [ ] **R0129.** Hosted views retain logical document/object/occurrence identity while carrying an authorized projection revision, authority epoch and captured base. A permitted subset is explicitly identified as a projection; it is never presented as the full canonical document. Mutable workspace membership and current grants are separately owned authority records, not identity encoded into every geometry payload.

- [ ] **R0130.** Owner refinement, 2026-10-20: importing a model includes its required declarative definitions, including materials, supported behavior definitions, units and source references, within the selected authorized closure. Reuse a verified matching definition/revision already available in the destination, and import missing definitions without silently replacing existing ones. Matching uses stable source identity/revision and validated typed content; a shared display name such as steel is insufficient. Same-name/different-grade or same-ID/different-revision candidates preserve their lineage and explicit binding. Equivalent permitted binary content may share storage while distinct logical identity, rights and provenance remain intact. Missing/restricted mandatory definitions produce a visible incomplete import rather than invented material values. Importing definitions does not install/execute source code, borrow source credentials or import the entire containing world.

- [ ] **R0131.** Separate definitions from environment bindings and evolving state. For an ordinary newly placed model, a declared inherited role such as gravity binds once to the destination's current compatible environment/settings; importing a second gravity definition must not add another force or preserve the source acceleration implicitly. Body dimensions, material definitions and intrinsic parameters remain those of the imported model. Explicit local overrides stay identified. Temperature, damage, velocity and other per-object/run state use the selected initialization/restore contract and cannot be collapsed into material-definition equality. An explicitly restored scientific study or running-state handoff retains K8/H7 pinning/transfer rules; it is distinct from ordinary model placement. B39 exercises definition reuse and state/environment separation in stages.

### K4. Quantities, fields, evidence and unknown values

- [ ] **R0155.** A quantity declares a physical quantity kind, dimension, unit, numeric representation and validity. Dimensions alone are insufficient: torque and energy need different kinds despite equal SI dimensions. Temperature values and temperature differences have different conversion rules; affine units are not multiplied like vectors. Counts, fractions, concentrations and currencies also need explicit basis/context.

- [ ] **R0156.** Reuse Lince's exact accounting for quantities that must conserve exact accepted balances. Preserve exact authored dimensions where useful. Numerical geometry and solvers can use f32/f64 arrays with declared units and error; do not replace every simulation number with arbitrary-precision arithmetic. Never round an operational balance through a GPU float.

- [ ] **R0157.** A **field** is a typed value over a support and optional time axis. Its descriptor includes quantity kind/unit, scalar/vector/tensor/components, basis/orientation, reference or deformed configuration, sample association, interpolation/extrapolation policy, validity mask, uncertainty/provenance and payload references. A constant material parameter, a temperature grid, point observations and a displacement result use the same envelope with different supported evaluators.

- [ ] **R0158.** Distinguish per-node, per-cell, per-face, integration-point and graph-site samples. Choosing a visually smooth interpolation does not make it physically valid. Complex/frequency-dependent values, spectra and distributions need typed domain payloads, not strings that masquerade as scalars.

- [ ] **R0160.** Distinguish unknown, withheld, missing coverage, stale, not applicable, unsupported, disputed and confirmed zero. Use explicit masks/status; do not encode unknown conductivity as zero or missing coordinates as NaN.

- [ ] **R0161.** Evidence records source/method/revision, measured/computed/estimated/authored status, units, conditions, validity interval, uncertainty or unknown uncertainty, and rights. Capture time, received time, accepted revision time and simulation time remain separate.

### K6. Operators, derivations and incremental evaluation

- [ ] **R0172.** An **operator** is a versioned supported computation with typed input/output ports, parameters, required capabilities and limits. A **derivation** records one invocation against exact inputs, settings, software and evidence. Examples include an extrusion, a mesh simplification, a geodetic conversion, crystal homogenization, collision generation, a solver mesh and a physical run.

- [ ] **R0173.** Use two execution classes: short pure operations that can safely run within a bounded edit budget; and asynchronous jobs that prepare outputs and publish only after validation. Existing Rule/Command systems schedule authorized behavior; this is not a new universal programming language or arbitrary code embedded in a model.

- [ ] **R0174.** Inputs include source revisions, selected regions, units/frame maps, material models, boundary conditions and adapter version. The cache key includes every output-affecting setting. A tool version change cannot reuse an old result by filename alone.

- [ ] **R0175.** Dependency edges are declared, including references to other documents and local datasets. Invalidate downstream products on relevant edits. Definition changes do not rebuild unrelated city tiles. Revisioned deterministic selectors make dependencies inspectable.

- [ ] **R0176.** Jobs have resource estimates/limits, priority, progress, cancellation, output-size limits and a captured input generation. At completion, verify hashes, domains, mappings and expected revisions. Late output is retained as an older result or discarded; it never overwrites a newer head.

- [ ] **R0177.** Distinguish numerical reproducibility within a tolerance from bitwise determinism. Store solver build, precision, platform, thread configuration and seed where relevant. A Merkle hash of inputs does not guarantee floating-point runs match across GPUs.

- [ ] **R0178.** Imports are derivations too: original bytes, source identity, adapter version, declared units/frame, imported semantics, loss/unsupported report and source-to-model mappings. Keep original data where rights permit.

- [ ] **R0179.** Incremental evaluation includes acceptance, projections and resource movement: return changed identities/fields, maintain reverse dependencies and publish a bounded change set so a local edit does not rebuild unrelated scene assets or inspectors. Exact input keys retain subnanometer changes; no universal millimeter/world-grid rounding is allowed. Derived approximate display reuse declares its error-qualified equivalence separately. The [stage 10 CAD requirements](10-stage.md#architecture-and-task-requirements) SolidCraft contract specifies preview progress, in-flight sharing and measured whole-path costs without making a serial CAD timeline the common execution graph.

### Workspace documents, artifact lifecycle and hosting

- [ ] **R0202.** Extend a workspace with revisioned **workspace model documents** for composition, provenance, dependencies and delivery. It describes relationships from a molecular structure to a site/world without replacing standard scientific payloads or inventing an entire CAD kernel. The same logical schema serves a tiny material sample and a sparse planet.

- [ ] **R0203.** Keep the existing workspace envelope and authority separate from the immutable model format. The envelope binds its stable workspace ID, owner/serving authority, model head references, shared presentation presets and current policy reference. Documents carry authored model data and declared rights/provenance; live memberships, credentials, grants, invitations and resource reservations remain in existing engine/store session and policy machinery. An authorized published projection is a separately identified artifact with explicit source/disclosure lineage. Importing a portable workspace snapshot never recreates the originating Organ's authority or silently makes it public.

### Document layout and manifest

- [ ] **R0204.** Proposed names under the workspace framing: editable directory `name.workspace/`; portable selected snapshot `name.lince-workspace`. A normal Lince workspace stores the same manifests/chunks in its existing asset storage and references document roots. Saving or hosting does not require exporting a packed file. These names are provisional. A portable snapshot contains selected authorized content, not live login credentials, invitation secrets or authority to host/edit its originating workspace.

- [ ] **R0205.** The directory form contains a small root manifest, sharded immutable descriptor pages, content-addressed binary/source payloads, and license/provenance attachments. A derived-data cache and local edit journal live outside the portable source snapshot unless explicitly included. Large object indexes and time series must be paged; the root cannot enumerate every feature or atom.

- [ ] **R0206.** `format`, `schema`, `required_profiles` — **Required meaning:** Protocol/schema identity and supported mandatory domain features; not an application binary version

- [ ] **R0207.** `document_id`, `parents`; external head `root_revision` — **Required meaning:** Stable logical document and optional lineage; the head envelope points to a snapshot hash outside the content being hashed

- [ ] **R0208.** `roots`, `descriptor_pages` — **Required meaning:** Root instances and bounded descriptors/index pages; child definitions are pinned by revision

- [ ] **R0209.** `frames`, `units`, `clock_refs` — **Required meaning:** Coordinate/unit/time definitions or versioned references needed by the selected content

- [ ] **R0210.** `definitions`, `instance_slots`, `selections`, `relations` — **Required meaning:** Paged model composition and domain references; arrays remain external payloads

- [ ] **R0211.** `representations`, `derivations`, `result_sets` — **Required meaning:** Purpose-specific outputs, exact inputs, maps and execution/validity status

- [ ] **R0212.** `model_outputs`, `consumer_bindings`, `study_refs` — **Required meaning:** Optional paged typed output/applicability contracts, pinned consumer adoption and root specimen/protocol/run/analysis references; none requires a macro owner

- [ ] **R0213.** `payloads` — **Required meaning:** Media/profile, hash kind, decoded byte count, codec, bounds/shape/type where applicable, dependency references. Also declare encoded length/bounds for the selected transport form and the verified relationship between transport and canonical payload identities.

- [ ] **R0214.** `coverage`, `detail_policy`, `offline_profiles` — **Required meaning:** Available space/time/source resolution, required closure and user residency choices

- [ ] **R0215.** `sources`, `evidence`, `rights` — **Required meaning:** External namespaces/revisions, methods, license/attribution and disclosure requirements

- [ ] **R0216.** `record_bindings` — **Required meaning:** Policy-controlled links; absent bindings are valid for a standalone scientific asset

- [ ] **R0217.** `entry_profiles`, `capability_refs` — **Required meaning:** Optional paged descriptions of permitted useful subsets, required/optional purposes, payload/resource bounds and supported view/edit/result operations; current admission remains external

- [ ] **R0218.** `presentation_bindings`, `source_bindings` — **Required meaning:** Shared view/Sand/model source references and explicit external identities; participant-local state, credentials and active stream grants are excluded

- [ ] **R0219.** A snapshot is a Merkle DAG of descriptors/payloads, not a recursively hashed arbitrary object graph. The root content never contains its own hash. A separate head/envelope carries the root hash and optional signature over that hash/context; signatures are outside the signed content. Cyclic semantic/constraint relations use stable object IDs resolved through the snapshot's revision table, rather than cyclic content-hash references. Cross-document references pin a target snapshot/object.

- [ ] **R0220.** Distinguish required dependencies for the selected capability/profile from declared deferred/optional content. A valid coarse world root may reference unloaded detailed regions without claiming them offline. Validate structural references separately from a requested offline closure; only claim a profile complete when every required descriptor/payload, style, transform resource and input is present and authorized. Optional external content stays explicit and cannot execute or download itself.

- [ ] **R0221.** An extension/profile has a namespace, schema version, validator and capability declaration. Unknown optional payloads may be preserved opaquely within size budgets; mandatory unsupported profiles make the affected object read-only/unsupported. Neither case permits silently re-exporting an incomplete model as semantically equivalent. The original unsupported bytes remain available when permitted.

### Encoding and portability

- [ ] **R0222.** Use Lince's existing canonical JSON and domain-separated SHA-256 for structured manifests after a conformance audit. Preserve its prohibition on bare floats. Define canonical exact decimal atoms as bounded normalized coefficient/exponent pairs, signed i128 cell-address strings and binary64 atoms as 16 lowercase hexadecimal bit digits where required. Normalize zero and forbid nonfinite geometric values. Each field schema accepts exactly its declared numeric kind; decimal versus binary data is not silently interchangeable. Bound digit/exponent lengths and prove known byte/hash vectors. A JSON debug view is not a second source of truth.

- [ ] **R0223.** Put dense geometry, site tables, instance populations, fields and trajectories in typed binary chunks or retained standard formats. The native array envelope states component type, little-endian order, shape/strides, quantity unit, frame, association, validity mask and byte length. Validate multiplication/offset arithmetic before allocation. Geometric coordinates must be finite; field invalid values use a mask. Compression is a transport/storage choice, not physical precision.

- [ ] **R0224.** Distinguish a canonical structured hash from a raw payload hash in types and schemas even when both use `sha256:` text. Raw payload identity hashes its decoded bytes; the descriptor pins the interpretation. Geometry codec parameters, quantization errors and mappings are explicit derived metadata. Recompression does not change decoded identity.

- [ ] **R0227.** Publish format schemas, canonical byte/hash vectors, valid/invalid tiny samples and a Rust validation/inspection tool through cargo xtask. Round trips preserve logical/subelement identity, units, arrays and supported semantics. Do not force conversion of crystals to triangles, CAD history to glTF or solver fields to screenshots to save them.

### Commit, recovery, sharing and garbage collection

- [ ] **R0230.** Write new chunks/descriptors to staging, validate their closure, flush durable data, then atomically compare-and-swap the authorized root revision. A successful save receipt refers to durable reachable content. A crash leaves the old root usable and orphan staging collectible. The store and blob filesystem need a recoverable commit protocol; do not pretend a SQL transaction atomically commits arbitrary files.

- [ ] **R0231.** Edits carry actor, scope, expected base, typed operation and inputs. Dragging updates a local preview; release commits one undoable transaction. Cancel discards the preview. Native/API/automation/import paths run the same validation. Unsaved drafts and failed operations remain recoverable without becoming accepted heads.

- [ ] **R0232.** Undo of authored geometry creates a new consistent edit/revision through existing undo machinery. Published operational adjustments use the accepted event/compensation semantics in F2–F4. Undo never claims to reverse a physical manufacturing step.

- [ ] **R0233.** For concurrent geometry edits, start with explicit edit scopes and optimistic revision conflicts. Preserve both branches and show affected selections. Generic text CRDT merging of Booleans, constraints or material assignments is not a valid geometry merge. Commutative independent edits may be merged only where demonstrated.

- [ ] **R0235.** Reuse existing Transfers/artifact delivery where verified for content delivery. A root publication, body transfer, rights grant, receipt and accepted model revision are separate states. Partial downloads can be verified/resumed without publishing an incomplete mandatory root.

- [ ] **R0236.** References are not read authority. Enforce policy on manifests, chunks, spatial bounds, counts, thumbnails, indexes and results. Private data can influence a derived output, so publication of that output needs its own authorized projection/disclosure decision. Do not mark it public merely because it contains fewer triangles.

- [ ] **R0237.** Dedupe within an appropriate authority/encryption scope. Global content-address probing must not reveal private asset existence or cross-tenant equality. Reuse existing authenticated encryption/key handling where needed; do not invent cryptography in this format. Revocation can stop future reads/updates, not retract plaintext already exported to another party.

- [ ] **R0238.** Pin accepted roots, user offline selections, unsynced local edits, retained source history and required scientific inputs. Mark-and-sweep unreachable chunks with a grace period and snapshot-safe readers. Rebuildable caches have a separate eviction policy. Disk exhaustion fails before replacing a valid root; it must not erase pinned sources to make a bake fit.

### Workspace hosting and participation: H1–H10

- [ ] **R0239.** Owner-approved refinement, 2026-10-20. Hosting is an access and execution arrangement for the same workspace, model documents, views, Sands, queries and activities used locally. H1–H10 specify how existing K/I/E contracts cross a participant or Organ boundary; they add no multiplayer-only object hierarchy, geometry format, ownership database or simulation engine. A blank grid, a laboratory, a supplied city block and a solar-system view can each be the entry view. Local use remains available without hosting or an account on another Organ. Stages [01](01-stage.md)–[05](05-stage.md) introduce the minimum hosted proof, [stage 17](17-stage.md) adds shared experiment observation, and later stages extend their own specialist capabilities through this implementation.

### H1. Workspace identity, hosting and invitations

- [ ] **R0240.** A workspace has a stable identity and governing Organ, accepted references to one or more model documents, permitted views/presets and mounted content, plus its existing policy. A document is a revisioned model/dependency graph; a view is a participant's presentation of selected data. A participant session identifies a person or explicitly restricted guest, device/endpoint, workspace and current admission generation. A host is a reachable Cell currently permitted to serve that workspace. Distinguish these identities from a native desktop-workspace slot, a physical-workstation calibration, a connection ID and an executable worker. Hosting never creates a second authoritative copy of a document merely to display it remotely.

- [ ] **R0241.** Extend existing host configuration, workspace policy and native publishing/invitation controls. A hosting activity declares the workspace, serving Cell/Organ authority, discoverability, admission limits, permitted sources and lifetime. It can run without an open model viewport on an explicitly configured Cell. Closing a visitor view releases that visitor's demand; it does not stop another participant's activity or an explicitly retained run. Show who supplies storage, bandwidth and computation. A headless host does not allocate render targets or GPU geometry unless a requested activity needs them. Keep initial deployment within the existing Cell/Organ transport; no mandatory hosted service or separate world server is introduced.

- [ ] **R0242.** Owner-confirmed A03: when the owner closes a workspace and no other view/activity explicitly retains the run, a resumable activity pauses/checkpoints at a supported boundary unless Keep running was enabled. The retained activity still has its owner, permission, lifetime and admitted resource budget. A nonrestartable adapter must expose its actual declared stop/finish behavior, not claim a resumable pause. Closing a visitor view cannot stop another participant's retained activity; looking away within an open workspace does not itself pause an active run.

- [ ] **R0243.** An invitation resolves a workspace identity, verified serving authority/endpoint route, intended audience/grant or invitation reference, validity/revocation generation and optional entry view/frame/selection. A saved viewpoint is a navigation suggestion, never a relocation of accepted objects. Reuse existing identity, login and invitation/token mechanisms where they fit; a new envelope must have bounded parsing and conformance vectors. Public links confer only public access. A restricted bearer invitation, if supported, explicitly describes the narrow grant redeemed and its expiry; possession never becomes an unrestricted Organ login. Do not embed passwords or live source credentials in a workspace file, URL preview, export or invitation log. Invite/redeem/revoke and expiry states must be inspectable in native controls.

### H2. Principals, scoped access and public visiting

- [ ] **R0244.** Support private identified invitations and optionally published visits without a login. Public visiting is an explicit workspace publication choice. Identified editing still requires the current editor grant; a guest can navigate, inspect and use only the public capabilities deliberately exposed. Represent a public guest as a restricted principal throughout the request path. Current trusted-local `actor: None` paths are not anonymous access: the guest route must never reach them as an authority shortcut. Transport endpoint authentication and person authorization are separately checked. Joining a public workspace does not create a broad account or grant visibility into its Organ.

- [ ] **R0245.** Reuse existing role union, viewer/editor/manager audiences, workspace ceiling, property grants, proposal review and original-author authentication checks. Evaluate the participant's applicable role authority together with the workspace ceiling, target/source disclosure and the requested operation. Extend selectors/bindings to stable model documents, definitions, occurrences, regions and typed properties as required, without a Record per vertex/site. Spatial membership, an Area effect or a physical body is never an access grant. A spatial region selector has explicit frame/revision and supported semantics, so projected triangle membership cannot silently decide ownership. Grant changes have a generation/epoch that closes obsolete previews, subscriptions and reusable delivery authorization.

- [ ] **R0246.** The model target of a grant carries document identity, stable definition/occurrence-prefix or named-selection identity, captured selection revision where required, typed property paths and allowed operations. Implement its bounded evaluator in the existing policy/command family; unsupported selectors deny the affected operation rather than expand it to the whole document. Authorization scope ownership is explicit and separate from composition/frame containment. Moving, instancing, copying or publishing across scopes checks source disclosure and destination mutation rights; a new parent or proxy cannot launder a private asset. Reuse an existing Record binding for authority only when that binding explicitly declares this role. An ordinary descriptive link must not unexpectedly grant or revoke model access.

- [ ] **R0247.** Authorize discovery, entry metadata, bounds, counts, thumbnails, index pages, chunks, result fields and commands on the serving side. The client hiding an object is insufficient. Names, cost totals and rejection diagnostics must not disclose denied contents. Derived render/collision proxies and scientific summaries require their own allowed disclosure; fewer triangles or aggregated values do not make private inputs public. A source blob/pack containing both allowed and denied data cannot be served wholesale: publish a validated authorized projection or use payload partitioning at the required access granularity. Cache attachment and deduplication stay within the permitted authority/encryption scope. Hash possession proves neither read permission nor provenance.

### H6. Collaborative tools, presence and shared behavior

- [ ] **R0271.** Use I4/E2 typed intents, original actor, expected revisions, selection/source context and receipts for every local or remote edit. Reuse proposal review for sensitive operations; review cannot lend the manager's authority to the original author. Independent changes combine only for proven disjoint/commutative scopes. Retain conflicting drafts and show affected selections. The existing text collaboration mechanism remains appropriate for supported text; geometry/constraint/material operations keep their own validation. An optional short edit lease can coordinate a tool, but it is bounded, expires after disconnect and never grants write authority or replaces expected-revision checks. Undo is an authorized new operation against the relevant current base and cannot erase another person's subsequent accepted work.

- [ ] **R0272.** A permitted-subset editor can carry a bounded opaque projection-context reference instead of receiving a private full-document revision catalogue. The host resolves that reference to the captured source base, visible target revisions and validated dependency scope, then applies normal current-authority/conflict checks. The context is not an access grant. Expired or unavailable contexts require a refreshed projection; never reinterpret an old edit against the latest state. A hidden unrelated edit can be rebased only when the existing independent-change proof establishes that the allowed target/dependencies are unchanged. H5 projection cursors and these contexts therefore preserve both privacy and expected-base semantics.

- [ ] **R0273.** Default camera pose, navigation, inspector expansion, local tool draft, source credentials and playback position belong to the participant. Sharing them is an explicit supported binding, such as follow-view, shared playback or an optional preview stream. Show requested versus displayed solved values under B31; only exact validated final input can commit. Publish at most the supported allowed draft fields to observers, never raw credentials/private selections. C02 automatic ready adoption still applies within its unchanged captured scope/base/authority; a multiplayer conflict or disconnected authority preserves a recoverable pending draft instead of silently widening the update.

- [ ] **R0274.** Owner-approved presence includes optional lightweight avatars, cursors and selections. Use the same presentation/anchor/activity infrastructure: canonical frame references, bounded update cadence, expiry and authorized visibility. Presence bodies are noncolliding presentations by default. Physical avatars, shared force tools or camera-bound terrain/Area effects require a separately granted interaction activity/body with supported domain semantics and quotas. A visiting camera cannot move accepted geometry or trigger operational Actions simply because it crossed a drawn Area. Hidden/private locations and selections remain private unless deliberately shared; participant rosters and location detail have their own allowed disclosure. Do not load or keep awake the neighborhood of every connected avatar; a permitted distant participant can use a coarse marker.

### H9. Content validation, execution and application surfaces

- [ ] **R0282.** Validate entry envelopes, descriptor/index pages, archive entries, typed arrays, geometry and codec sizes before allocation or native calls. Carry declared decoded limits, checked arithmetic, expansion/dependency depth, deadlines and output caps through the actual parser/worker path. Understated sizes, corrupt chunks, cyclic references, redirects or disallowed endpoint changes fail boundedly and leave the prior usable state intact. Preserve separate typed artifact identities for canonical SHA-256 descriptors/payloads and a transfer backend's hash scheme; verify the binding instead of reinterpreting an equal-looking digest string. Do not weaken existing Blob Sync accepted-offer checks to create a universal private hash server. Extend/reuse its verified transport/storage mechanisms behind workspace-principal and payload-scope authorization, with revocation and partial/resume proofs.

- [ ] **R0283.** Imported definitions and familiar Sands are data until a supported action/activity is authorized. Keep existing compiled Sands and declared formula limits; installed kernels/plugins, executable shaders, FMUs, commands, OS actions and automation have explicit capabilities and isolation. A workspace cannot auto-install native code or run its owner's automation under the visitor's account. A read-only visit cannot borrow the host's Cell/Fiote/window authority. Where an adapter cannot enforce a memory/time/cancellation limit, identify its trusted-worker boundary and block the untrusted capability rather than claim a hard bound. Preflight scanning and a valid signature never replace these runtime checks.

- [ ] **R0284.** For Park and assigned application panels, distinguish shared placement, authorized pixel/media publication and remote application input. Sharing the room or a window mount grants none of the latter capabilities implicitly. Reuse existing capture/output/input adapters with a source-specific view/control grant, focused input ownership and withdrawal on revoke/lock. Never grant generic desktop, filesystem, microphone, camera or physical-output control through spatial navigation. Streaming/capture needs an admitted rate/lifetime and appears in both parties' resource view. Ordinary native Sand interaction still uses the actual participant's command policy.

### H10. Disconnect, offline use, reconnection and host recovery

- [ ] **R0285.** Owner-approved first behavior: when the host becomes unavailable, retain permitted cached viewing and recoverable local drafts, with the last accepted revision and disconnected status visible. Offline availability follows actual saved closure and retention/expiry policy; a live stream or missing source does not become available by implication. A draft retains original host/workspace/document, base, author and scope. It is not an accepted shared edit or an operational side effect. Do not claim continuing authoritative shared physics, reservations or edits while its authority is unreachable. Cached results remain inspectable under their own rights and recorded run state.

- [ ] **R0286.** Reconnect verifies the same workspace/serving authority, current participant/device admission and source epochs, then resumes a valid bounded cursor or obtains a new authorized snapshot. Retain and reconcile drafts through the existing preview/submit/review flow. Query recorded idempotent outcomes before retrying uncertain requests; do not replay mutations, Area crossings or automation automatically. A revoked-then-regranted login is a new admission generation, not permission to apply an old pending proposal. A revoked session stops future delivery and removes compliant live/private presentations; revocation cannot retract plaintext already exported. Optional future co-hosts require explicit setup and the H7 transfer proofs; they are not the initial disconnect policy.

- [ ] **R0287.** Scope network controls clearly. Full workspace-offline mode leaves/stops its remote participation and starts no invitations, source/asset requests, heartbeat or background reconnect; locally permitted content/drafts remain usable. A narrower source's cache-only mode is labeled as source-specific and does not silently claim that an otherwise live multiplayer session is offline. Existing user-configured source/offline settings and permissioned Protein access remain independent unless the user selects a broader network policy. Leaving releases participant leases, queued work and optional streams after their bounded grace/fence; saved offline packs and durable edits remain. A host-retained activity has its own explicit owner, stop policy and budget after its initiating visitor leaves.

- [ ] **R0288.** Host restart recovers durable accepted heads, receipts, admission/invitation generations and admitted job outcomes before advertising a live workspace. Incomplete staged assets remain unaccepted and boundedly collectible. Interrupted workers report restartable, failed or indeterminate state according to their contract; the UI never represents stale observations as a continuing live run. Preserve public/private publication choices across restart, with no fallback to a public/full-access workspace when credentials or policy are unavailable. B33–B37 provide the phased hosting evidence; documentation and passing tests for adjacent functionality do not certify these extensions.

### Interface contracts I1–I6

- [ ] **R0305.** **I4 — Tool and transaction.** A tool declares active edit scope, typed inputs, bounded preview, validation, commit/cancel/undo and failure recovery. Mouse/keyboard/other input and automation produce intentions through the same command path. Long evaluation uses a job handle instead of holding the input loop or a durable transaction open. Leaving an authoring context uses the C02 draft/adoption policy; pending computations and conflicts remain visible. Partial previews are not accepted state. Display eligibility and commit eligibility are separate: compatible intermediate results may advance a moving preview with their solved values visible, while only validated exact final inputs can commit. Scope/source/selection changes or cancellation close the preview generation; see [stage 10 CAD requirements](10-stage.md#architecture-and-task-requirements) and B31. Remote tools use the same actor/base/scope command and receipt. A shared preview is an optional expiring projection; conflict/undo and disconnected drafts follow H6/H10 rather than replacing another participant's accepted work.

### Domain workflows as implementations of the primitives

- [ ] **R0358.** The catalogue describes eventual capability coverage, not execution order. Geometric/CAD/room/world and structural microscopic authoring belong to the core stages; new material interpretation, microscopic experiments and physical/chemical/biological predictions belong to deferred stages [16](16-stage.md)–[21](21-stage.md). A shared profile/format extension preserves those future paths without requiring all their evaluators before the core stabilizes.

- [ ] **R0359.** Each workflow below reuses K1–K8, the common transaction/native inspector, format and job lifecycle. Its specialized representation, validator and solver are explicit. A new domain profile earns shared complexity when two distinct workflows need the same invariant; otherwise retain a thin domain module.

- [ ] **R0370.** Concrete cross-scale reuse is a chain of references and derivations: crystal/sample → property evidence/model → material specification → part-region assignment → assembly occurrence → building/site placement → geographic/astronomical context. Composition describes what is assembled; derivation describes what is inferred; spatial placement describes where it is. These are three different edges. Imported/measured content can enter at any level and missing lower levels remain absent. No step expands all finer levels merely because the user opens a larger assembly.

### Implementation boundaries and dependency decisions

- [ ] **R0524.** Proposed Rust ownership: introduce a small `crates/workspace_model` model/evaluation library for K1–K8 contracts, exact descriptors, supported numerical buffers and pure validation/query code. It may depend on the existing pure `nucleus` types/hash/quantity helpers, but never the reverse. It must not depend on Bevy, a database, network access or a heavyweight scientific kernel. Start with the types used by the first two fixtures; do not implement every profile speculatively.

- [ ] **R0525.** The crate boundary is not a delivery request for every future evaluator. Stages [01](01-stage.md)–[15](15-stage.md) implement only exercised identity/geometry/query/definition/hosting contracts and current runtime integration. New physical-material schemas beyond passive preservation, property databases, thermodynamic state, scientific workers and numerical run orchestration are deferred stages [16](16-stage.md)–[21](21-stage.md). Do not add empty worker crates or scientific dependencies just to reserve a name. Current operational simulation is audited for its own isolation and is not silently replaced with a physical solver.

- [ ] **R0526.** `nucleus` defines only the small inert model-reference/binding DTOs needed by existing canvas/Record contracts, using its own ID/hash/quantity conventions. The model library validates/resolves them. Keep specialist schema/evaluation in `workspace_model`; nucleus never imports that crate, so the dependency remains acyclic. Preserve numeric-domain boundaries instead of copying the accounting type system into scientific arrays.

- [ ] **R0527.** `engine` coordinates authorized mutations, expected revisions, receipts, publication and jobs. `store` persists roots, metadata, bindings, indexes and job state using existing asset/blob facilities where sufficient. Large arrays stay outside frequently synchronized Record rows. Schema changes use new append-only migrations and the repository's checksum/guard procedure. Extend its existing workspace policy/login/command families for explicit public principals, scoped entry, model grants and host/activity admission; do not duplicate them in the pure model crate.

- [ ] **R0528.** `crates/interface/src/native/` owns Bevy rendering, input, native tools, inspection and adapters from current canvas/Room/Areas. The core never exposes Bevy entity IDs as durable references. `crates/map` remains a provider of geographic sources and scene context, not the owner of every model. Extend native workspace/policy/Protein session controls for invitations, cost/preflight/status, presence, recovery and source-specific sign-in. Park keeps capture/input and physical-output authority.

- [ ] **R0529.** `transport` and the existing Cell/Organ wire own authenticated serving, explicit guest admission, sparse subscriptions and prioritized bounded asset/observation delivery. Reuse verified blob mechanisms behind workspace-principal scope; existing accepted-offer/peer authorization stays intact. `store` retains durable publication/invitation/admission generations, receipts, draft bases and cost/job counters where required, using append-only schema changes. Model decoding/evaluation never opens a network connection by itself.

- [ ] **R0530.** Deferred physical runtime/job orchestration can reuse verified simulation infrastructure where its semantics fit. Scientific workers carry molecular/continuum/other numerical dependencies only in stages [16](16-stage.md)–[21](21-stage.md). Core exact-CAD/IFC geometry may still need a bounded worker at stages [10](10-stage.md)–[11](11-stage.md); that is a geometry execution boundary, not a scientific-simulation prerequisite. Add worker crates/features only when selected fixtures need them; default map/room viewing must not compile or initialize every solver.

- [ ] **R0531.** Shared bounded asset/job services handle content storage, cancellation, demand, budgets and policy. Domain adapters supply typed payload validation, algorithms and capability declarations. Do not build a second unbounded cache/job system inside each Castle.

- [ ] **R0532.** Pins/limits are checked against the current workspace before changing dependencies. Use cargo check, required tests and cargo xtask/Nix workflows; warnings are errors. Embedded dependency licenses/credits are included in the relevant Sand. The old web frontend is not modified.

### Performance research and adaptation boundaries

- [ ] **R0534.** [TigerBeetle architecture](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/ARCHITECTURE.md) — **Documented mechanism:** Explicit runtime limits, startup allocation sized from them, and an ordered state machine — **Concrete adaptation and limit in Lince:** Bound admitted queues/buffers and short acceptance work. Scope serialization per authority; parallelize independent numerical work. A growing user-authored world needs bounded active sets, not a globally fixed population or a single simulation core.

- [ ] **R0535.** [TigerStyle](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md) — **Documented mechanism:** Separate control/data work and amortize costs through batching — **Concrete adaptation and limit in Lince:** Resolve schemas/units/dependencies outside hot loops, batch narrow commits/transfers and reuse admitted numerical buffers. Runtime/profile changes can reserve new capacity at safe boundaries. The repository's own coding instructions still govern implementation.

- [ ] **R0536.** [TigerBeetle protocol-aware deterministic simulation testing](https://tigerbeetle.com/blog/2026-08-20-protocol-aware-dst/) — **Documented mechanism:** Production state-machine logic runs with controlled time/I/O and injected faults, checking internal invariants — **Concrete adaptation and limit in Lince:** Replay Lince command/job/publication/subscription/recovery logic under adverse completion orders and failures. This is separate from claiming bitwise determinism or scientific validity for every CAD/GPU solver.

- [ ] **R0537.** [SpacetimeDB key architecture](https://spacetimedb.com/docs/intro/key-architecture/) and [transaction semantics](https://spacetimedb.com/docs/databases/transactions-atomicity/) — **Documented mechanism:** Reducers execute application changes within transactions — **Concrete adaptation and limit in Lince:** Keep authorized local changes close to accepted data and publish coherent outcomes. Heavy geometry/science executes outside a transaction; its validated output is accepted by a narrow revision-checked command. Reuse existing Lince Actions/store paths.

### Execution contracts E1–E8

- [ ] **R0541.** **E1 — Admission, ownership and overload.** Every active domain/view/job declares limits for resident CPU/GPU bytes, scratch/staging, decoded output, pending work, input/output rate and time. Reserve capacity before dispatch, including peak replacement buffers while the old accepted artifact remains usable. Reuse buffers in hot phases; grow/reconfigure pools only at an explicit safe admission boundary. On saturation, queue within a bound, offer lower requested coverage/fidelity, pause or reject with a reason; never silently evict pinned sources or change physics. An unused profile has no active workers, subscriptions, physics bodies or GPU allocation merely because its descriptor exists. H3 reserves participant and host capacity from the same pools, including preflight, unauthorized/malformed request handling, transient queues and serving/decode costs. Enforce per-principal limits plus global limits so anonymous reconnects or many individually small joins cannot defeat admission.

- [ ] **R0542.** **E2 — Ordered acceptance with parallel evaluation.** A typed command includes operation/idempotency identity, actor, scope, expected revision and causal inputs. One coherent acceptance order exists within the relevant local authority/store. Validate, durably commit and publish a receipt plus an atomic projection delta; retries resolve to the recorded result. Preparing long jobs captures immutable inputs, commits a job request, releases the transaction and dispatches admitted work. Workers can execute concurrently on isolated/disjoint state, but their callbacks cannot overwrite accepted heads without matching generation, permissions and revision. Cancellation is an ordered state transition too. Across authorities preserve explicit pending/handoff/conflict states, not an invented globally serial transaction. Hosted commands revalidate participant/device/source admission at acceptance. Use domain/authority epochs to reject superseded execution writers; reconnect resolves durable request outcomes before any retry.

- [ ] **R0543.** **E3 — Compile semantics into numerical work.** Resolve field kinds, dimensions, conditions, source/mapping revisions, material candidates and kernel selection once per evaluation generation. Compile a bounded evaluation plan with typed buffer layouts, dependencies, precision, prepared constants and cost estimates. Hot loops process contiguous/appropriate sparse data and compact indices rather than JSON, dynamic property lookup or Record objects per element. CPU SIMD, GPU kernels or an external scientific library implement a declared operation; their data layout need not match the portable schema. Validate layouts/lengths and finite coordinates before native/unsafe access. A changed source invalidates only products whose dependency keys include it.

- [ ] **R0547.** **E7 — Controlled failures and reproducibility.** Factor nondeterministic command/job/storage/network completion and clocks behind testable boundaries. Seed schedules that inject lost/duplicate/out-of-order completions, cancel/delete/revoke races, interrupted writes, source revision changes, slow clients, missing chunks and resource exhaustion. Run real validation/acceptance/state transitions, not only mocks of desired behavior. Check internal invariants and user-visible receipts; retain failing seed/schedule and minimize the reproducer. Distinguish deterministic control replay, identical accepted artifact delivery, numerical agreement within tolerance, statistical experiment reproducibility and bitwise kernels. Each solver declares only the level its tests establish. B33–B37 add public/identified participants, stale invitation/manifests, dishonest sizes, two-Organ login contexts, slow asset/result streams and host loss/restart to the real controlled boundaries.

- [ ] **R0549.** These E-contracts are shared services/policies, not a demand to rewrite the operating system, GPU driver, database, numerical kernels or every existing Castle. Build the minimum path needed by the next S-stage and instrument its boundaries. Numerical work may exploit many cores/accelerators while accepted mutations remain coherent. Local/offline use and private datasets remain first-class.

### Fixture phases and the stabilization boundary

- [ ] **R0771.** B34/B35 — **Core proof through [stage 15](15-stage.md):** Mixed-rights retained geometry/asset/result artifacts and interrupted bounded geometry/transfer jobs suffice; no new scientific execution required for private-result security or host recovery — **Deferred proof in stages [16](16-stage.md)–[21](21-stage.md):** Scientific inputs/observations/checkpoint/result disclosure and interrupted numerical workers added at the stage introducing them

- [ ] **R0773.** B39 — **Core proof through [stage 15](15-stage.md):** [stage 02](02-stage.md) bounded required-definition import/reuse/conflicts/rights; preserve existing implemented environment behavior and distinguish definitions from state without evaluating new material laws — **Deferred proof in stages [16](16-stage.md)–[21](21-stage.md):** [stage 16](16-stage.md) physical-material matching/applicability and [stage 18](18-stage.md) new live body/environment/gravity/state cases; inherited destination setting applies once

### Confirmed owner decisions, 2026-10-18, 2026-10-20 and 2026-10-21

- [ ] **R0828.** **Q1 — Automatic application on exit.** A ready applicable candidate automatically applies to the selected scope when leaving its authoring context. Pending/invalid output retains the identified accepted model and visible status. Late completion uses the captured generation/base/scope/authority policy; active runs use a supported transfer/restart/defer path.

- [ ] **R0835.** **Q8 — Workspace feature/refactor framing.** Use the existing workspace identity and native content as the container for modeling, hosting and participation. The pure model-library/file-name proposals use workspace terminology; no second branded product or independent multiplayer model engine. Preserve every micro-to-macro, topology, CAD, AECO and scientific workflow already specified.

- [ ] **R0836.** **Q9 — Optional public visits and identified editors.** Support private invitations and deliberately published visits without login, with explicit restricted guest principals. Editing requires identified current authority under existing role/audience/ceiling/source policy. A private host never becomes public as a recovery fallback.

- [ ] **R0838.** **Q11 — Initial host-loss behavior.** Keep permitted cached viewing and recoverable local drafts, then revalidate/reconcile with the same authority on return. No automatic mutation replay, simulation writer election or co-host takeover. Explicit configured co-hosting can later extend H7/H10 after ownership-transfer proofs.

- [ ] **R0842.** **Q15 — Import definitions; reuse compatible definitions and destination environment.** Imported objects bring their required permitted material/behavior/other declarative definitions. Reuse verified matches already present and import missing definitions with provenance, keeping conflicting versions explicit. Ordinary inherited environmental roles such as gravity use the destination's current compatible setting once; definitions, configuration and evolving object/run state remain distinct. A matching name alone cannot merge materials, and import never grants execution or source credentials.

- [ ] **R0843.** **Q16 — Pause background work unless retained.** Closing the owner's workspace pauses/checkpoints resumable activities at a supported boundary when no other view/activity retains them, unless Keep running is enabled. Other participants' retained demand remains valid; nonrestartable adapters report their actual stop/finish contract. Camera visibility alone does not pause an admitted activity.

### Subsequent fixture and adapter decisions

- [ ] **R0854.** Select the exact public/identified principal envelope, supported model grant selectors, invitation validity/lifetime, projection cursor retention, activity stop policies and measured host/client limits during [stage 01](01-stage.md)/[stage 02](02-stage.md). These are implementation details within H1–H10, not reasons to reopen public visits, presence or cached-view/draft recovery. Choose the smallest asset/stream transport extension from B33–B36 and use B37 to validate each later run profile; co-host availability and native executable distribution are not silently added to the initial release.

### Accepted architecture/detail choices and stage-specific evidence

- [ ] **R0856.** **A01 — Agreed: definitions, environment and state.** Import the object's required material/behavior definitions, reuse verified compatible definitions and import missing ones. For ordinary placement, inherited environmental roles such as gravity adopt the destination's current compatible setting. Preserve intrinsic properties, explicit overrides and separate object/run state; do not merge by name or apply gravity twice. Explicit study restore/state handoff still uses K8/H7. See Q15/B39; stages [stage 02](02-stage.md)/[stage 16](16-stage.md)/[stage 18](18-stage.md).

- [ ] **R0858.** **A03 — Agreed: background lifetime.** Pause/checkpoint resumable work when the owner closes the workspace and nothing else explicitly retains it, unless Keep running was enabled. Respect other participants' leases, supported restart capabilities and the admitted lifetime/budget. Looking away is not closing the workspace. See Q16; stages [stage 02](02-stage.md)/[stage 17](17-stage.md)/[stage 18](18-stage.md) and H1/H10.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0726.** B01 — **Fixture:** A reused part with two instance slots, one local override and a Record binding — **Proof required:** Stable identity under reorder, fork/update review, conflict, undo and save/reload

### Owned fixture specifications

- [ ] **R0740.** B15 — **Fixture:** Truncated/hostile pack, duplicate entries, oversized decode, cyclic references and disk exhaustion — **Proof required:** Bounded rejection, intact previous root, no path escape/network/credential execution

### Owned fixture specifications

- [ ] **R0745.** B20 — **Fixture:** Seeded production command/job/save/projection logic with controlled I/O/time — **Proof required:** Crash/retry/duplicate/reorder/revoke/new-draft races preserve accepted roots, at-most-once accepted effects within authority, correct receipts and no stale model adoption; retain failing seed/schedule

### Owned fixture specifications

- [ ] **R0749.** B24 — **Fixture:** A repeated typed recipe with a one-parameter edit, purpose-specific products and old/new generations; selected mesh operations at [stage 10](10-stage.md) — **Proof required:** Exact dependency/cache identity, bounded undo/scratch/output residency, stale-result rejection and affected-only reevaluation; coarse/fine display preserves accepted dimensions/properties; later touching/coplanar/thin-wall/self-difference cases prove topology and achieved metric error separately

### Owned fixture specifications

- [ ] **R0759.** B34 — **Fixture:** A workspace on Organ A with public/private model regions, mixed-permission source packs, a private run result and Sands querying public/restricted data on Organ B — **Proof required:** Guest never reaches trusted-local authority; role/ceiling/source intersection, no denied bounds/count/hash/thumbnail/result leakage, no whole private blob for a partial view, original-author editing, own B login/context, independently usable Protein access and distinct placement/pixel/input/worker grants

### Owned fixture specifications

- [ ] **R0760.** B35 — **Fixture:** Lost commit reply, stale invitation/manifest, revocation during delivery, revoke/regrant, host crash/restart and reconnect with cached data/drafts — **Proof required:** Durable idempotent outcomes and last valid heads, no unauthorized late attach, admission/epoch recheck, no replayed mutation/Area effect, visible permitted offline closure and draft reconciliation, no cap reset or unintended public fallback, released queues/leases and explicit interrupted jobs

### Owned fixture specifications

- [ ] **R0764.** B39 — **Fixture:** Imported model with required definitions/materials, a verified existing match, a conflicting same-name/revision candidate and distinct environment/state — **Proof required:** Authorized bounded closure, reuse without lost logical provenance, no name-based overwrite, unsupported/missing status, independent overrides/current state; later material and gravity tests bind the destination environment once while retaining imported intrinsic definitions and explicit local overrides

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Furnish and annotate a tiny shared room document with reused parts, one local override and mixed-access content. Measure the actual create/edit/undo/save/invite/reconnect flow with concurrent readers; do not substitute an empty manifest or a policy-unit test for entering and editing the workspace.

- [ ] **R0558.** **[stage 02](02-stage.md) — L0/L1, minimum L2/L3** — **Intention and defined deliverable:** Native open/edit/save/undo of a repeated definition; shared typed command/receipt/job-generation boundary — **Correctness/failure evidence and performance obligation:** B01/B15/B20: expected-revision conflict, retry/idempotency, crash-safe root, draft/cancel and stale callback. Small-command latency uses the stated ≤200 ms proposal; record peak commit/replacement bytes and bounded queue behavior. Native/programmatic availability, created IDs and minimal change sets use the same accepted path; dependency checks keep new core code independent of presentation adapters. B33/B34/B35 first prove identified/public admission, invitation/entry metadata, document references, scoped reads/edits and durable reconnect outcomes on this tiny document. B39 begins required-definition import/reuse/conflict/provenance without starting a solver.

- [ ] Run the actual native/backend/persistence/transport path for every 02.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0594.** a repeated part survives reorder, undo, reload and two competing edit bases with stable identities. A corrupt or incomplete new snapshot cannot destroy the last valid saved root.

- [ ] Advance only when **all 02.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
