# Spacematter: modeling space, matter and behavior in Lince

Architecture refinement, 2026-10-16. This document preserves the merged topology, CAD, AECO, production, civic and infrastructure ambitions. It proposes the model, storage, native editing, computation and delivery contracts needed to make them buildable. It is a plan, not a statement that these capabilities exist. The implementation sequence is the only task schedule; later domain catalogues specify its requirements. Refine the proposal with the owner, summarize the agreed scope before implementation and obtain implementation approval. Remove completed tasks only after their frontend, backend and relevant evidence are complete; retain deferred requirements and blockers.

## 1. The intended system and the first useful result

Spacematter lets people author, import, inspect, combine and simulate models from microscopic structures through materials, pieces, assemblies, buildings, streets, cities, planets and astronomical systems. Smaller supported descriptions can include isotopes, particle populations or physical fields when their domain requires them. Atomic balls are one representation, not the smallest compulsory building block of the file format. Arbitrary addressable extent does not imply populated content, available knowledge or a valid solver at every scale.

The priority is an offline personal world: very coarse Earth context, a navigable city, and detailed attractive places the user cares about. Home, work, university and community spaces can have authored interiors, material assignments, mounted Sands and useful behavior while the rest of the world remains simple. Fictional places and alternative designs have the same authoring capabilities. Earth is an optional reference system, not a required root for every model.

Both directions of work are first-class. A person can assemble a wall from an empirical material and measured dimensions, then add finer evidence later; another can study a crystal and explicitly derive one property used by that wall. Zooming never invents atoms, unseen rooms, survey accuracy or material behavior. Refinement can add geometry, knowledge or a better physical model independently.

The architectural goal is a small common model language plus domain implementations. Geometry, material science, CAD, geography, biology and operational workflows share identity, units, frames, selections, dependencies, revisions and results. They retain the representations and algorithms their mathematics requires. A general field interface does not make a molecular force field, a rigid-body force, an electromagnetic field and a stock balance interchangeable.

Recommended first demonstration: open a coarse offline world, move through one downloaded city, enter a modeled home, cut an opening, mount a Sand, sculpt a nearby cave, simplify selected geometry, assign documented materials to a reusable part, and compare a customization against an unchanged imported/measured base. Subsequent proofs connect a microscopic structure to a material model, a dimensioned assembly and a checked physical study. Generic behavior must support interactive authored experiences without requiring a separate game architecture.

### Proposed defaults for refinement

These are engineering recommendations, not owner-approved hardware or measured promises. They let implementation start with a concrete proposal instead of repeatedly asking what “efficient” means.

| Decision | Proposed default | When it may change |
| --- | --- | --- |
| Delivery order | Offline city and useful room first; materials, microscopic editing, exact CAD and scientific studies follow | Owner selects another first demonstration |
| Native modeling | Planar patches and indexed polygon meshes first; exact CAD solids through an isolated kernel; sparse implicit volumes when a workflow needs them | A fixture proves a cheaper representation meets the same requirements |
| Physical units | Dimensioned local values; canonical descriptors preserve exact authored values; solver arrays use declared floating-point units | A solver requires another declared unit system |
| Render precision | Evaluate `big_space`; persist a Lince frame/address contract independent of its ECS types | Compatibility or precision proof fails |
| Initial microscopic material bridge | A documented crystal elastic tensor, orientation and an explicit homogenization model; also a small molecule editor fixture | Owner chooses another physical question with suitable evidence |
| First continuum studies | Small-strain elasticity and heat conduction, each independently verified before coupling | An alternative study has inputs and reference results |
| CAD kernel | OCCT worker is the leading evaluation candidate; retain a kernel-neutral contract and compare a Rust candidate on identical fixtures | Build, licensing, robustness or deployment evidence favors another kernel |
| Native persistence | Spacematter manifests and content-addressed payloads; directory layout first, portable packed form next | Measured access patterns justify an encoding change |
| Scientific runtime | Explicit bounded jobs; installed/versioned adapters; one local physical domain first | Multiple domains pass isolation and resource tests |

## 2. Source baseline and shared families

This inspection establishes reuse candidates and present limits. Recheck before coding because the working tree is shared and other work is changing it. No existing subsystem is certified for the larger scope solely by its name.

| Existing family and source | What to reuse | Required extension or boundary |
| --- | --- | --- |
| Canvas state and Room: `crates/nucleus/src/canvas/`, `crates/interface/src/native/canvas_host/`, `topology/rooms/` | Stable placements, scoped edits/undo, room-local poses, content/proxy roles | Current Room bounds are 16 rooms, 256 members and depth 8. A streamed world cannot be a giant Room placement list. Add asset-backed spaces and materialize only active occurrences. |
| Terrain: `crates/nucleus/src/terrain.rs`, `terrain/formula.rs`, native `topology/terrain/` | Bounded compiled formulas, cursor/camera/item bindings, additive floor effects and editor | Height queries remain one surface profile. Existing coordinate clamps and canvas sizes are not physical universe limits. Preserve guards while adding frame-local contracts. |
| Areas: native `area.rs`, `area_target.rs`, `area_effects.rs`, `topology/influence.rs` | Region effects, off-center 2D attraction target, binding and immunity semantics | Extract common region queries, add a third target coordinate and supported region shapes; preserve capability-specific behavior. |
| Geometry and physics: native `topology/assets.rs`, `splats.rs`, `physics.rs` | Imports, appearance, picking and existing Avian/Parry collision paths | Current Bevy is pinned to 0.19.1 and Avian to 0.7.0 with f64. Rendering, collision, source geometry and canonical poses need separate owners. Verify compatibility rather than changing these pins speculatively. |
| Physical workstation: `crates/nucleus/src/physical_workspace.rs`, native `physical_workspace/`, [Shader-effects.md](Shader-effects.md) | Measured planar surfaces, calibration revisions, assigned content, dedicated outputs and replay | Model-to-installation registration is explicit. A virtual edit does not move or recalibrate a real installation. Current hardware/output bounds remain meaningful. |
| Mapping: `crates/map/src/{source,geometry,scene}.rs`, native `map_castle/`, offline map tests | Local/archive sources, PMTiles, bounded tile/download/cache controls, geographic context | Current normalization removes buildings and caps data zoom at 14. Elevation, detailed buildings, semantic city packages and offline route solving require their own inputs and proofs. |
| Records, quantities, evidence and policy: nucleus, engine and store | IDs, exact accounting, grants, receipts, typed values, publication and accepted changes | A model element need not be a Record. Add bindings and typed domain validation rather than another ownership/accounting system. |
| Canonical artifacts: `crates/nucleus/src/karma/canonical.rs`, existing receipt/artifact delivery paths | Domain-separated SHA-256, canonical JSON for exact metadata, transfer identity and permission paths | Canonical JSON currently rejects floats. Reuse that rule for manifests with explicit exact/binary scalar encodings; do not force geometry arrays into Record JSON. Verify delivery can carry general blobs before reusing it. |
| Simulation: `crates/simulation/src/{scenario,world,worker,limits}.rs`, native `simulation_castle/` | Captured bases, bounded jobs, checks, evidence and comparison concepts | Current workers orchestrate operational Actions and local Cells. They are not PDE/MD solvers or evidence of a sandbox for arbitrary native libraries. Audit isolation and add a physical-job contract. |
| Media and vision: [Computer-Vision-Lab.md](Computer-Vision-Lab.md), Signals | Demand, capture timestamps, bounded streams and input provenance | Reconstruction remains separate deferred work. Import/capture, tracking, geometry inference and accepted object identity have different confidence and failure modes. |

The shared families proposed by this pass are: durable model objects and occurrences; frame/unit transformations; spatial support and named selections; typed quantities and fields; domain relations and constraints; revisioned derivations; representations and detail; and isolated runtime state. The following contracts define their members and behavior before project code changes.

## 3. Model kernel: the primitives and their invariants

### K1. Identity, definitions, occurrences and revisions

Use distinct typed identities for a logical object, definition revision, instance slot, occurrence, representation, subelement, selection, derivation and run. Reuse existing ID conventions where their validation and namespace fit. Names, positions, Bevy entity IDs, triangle numbers, array offsets and source scene paths are never durable identity.

- A **definition** describes reusable authored content: a molecule, part, building module, room asset, tree species model or scene. Its immutable revision pins its structure and dependencies.
- An **instance slot** is a stable child placement in a definition, referencing an exact child revision, placement and explicit overrides. Reordering a list does not change slot identity. Expanding the same definition in two places produces different occurrences.
- An **occurrence** identifies a resolved instance in a composition: root instance plus stable slot path. It includes the selected composition revision when used in a pick or edit. Large repeated populations can use stable instance keys in columnar buffers instead of one allocated object per occurrence.
- A **Record binding** links a model object/occurrence/selection to one or more existing Records, under policy. The reverse may link a Record to many representations. Creating an atom, rivet, mesh vertex or map feature does not automatically create a Record or persistent ECS entity.
- A **revision** identifies immutable content. A logical object's current accepted head is a separately authorized reference. Hash identity does not replace logical identity: moving a chair changes its revision, not which chair it is.
- Shared definitions are pinned by default. “Update instances” presents affected instances, overridden fields and unresolved bindings. A private local override does not edit the shared definition. “Make unique” creates a fork with explicit lineage.

Keep five graphs distinct: composition is an acyclic graph; each evaluated frame has one parent in an acyclic frame forest; derivation dependencies are acyclic within a revision; domain constraints/connections may contain cycles; semantic/Record links may also contain cycles. Expansion has depth/count budgets and rejects recursive instancing. Cyclic physical feedback belongs to an explicit solver domain, not a recursive derivation evaluator.

Definition containment, spatial inclusion, constituent composition, mechanical attachment, ownership, operational custody and causation are different relation kinds. A door can be contained in a building, hinged to a wall, made of several materials and owned by someone else. None of those relations should accidentally impose another one's lifecycle or transform.

### K2. Frames, precision, units and clocks

A frame descriptor includes ID/revision, parent, axis order, handedness, unit scale, spatial reference and validity interval where needed. Rigid local transforms use translation and a normalized rotation. Unit conversion, a physical deformation, display enlargement, a mirror and a geodetic coordinate transformation are separate operations with separate semantics.

For a Cartesian parent, a point transforms as `p_parent = t + R (s_unit p_local)`. Large translations use an integer cell address plus a bounded local residual: `t = cell × cell_extent + residual`. Evaluate relative transforms through a nearby common ancestor before converting to floating point. Never flatten an atom in a remote building into one enormous meter-valued vector and subtract nearly equal numbers afterward.

- Persist frame-local high-precision values independently of renderer origins. Bound accumulated transform error through nested rotations/unit changes; integer address range alone does not guarantee tiny relative accuracy. If a requested cross-frame measurement exceeds the available precision or source accuracy, report that limit or evaluate in a suitable local frame. Specify cell extent and residual normalization per frame, checked integer arithmetic and representable-error bounds. i128 cell coordinates are a candidate, not a reason to accept unlimited recursion or allocation.
- A user can inspect two tiny samples at distant locations in separate view roots. Camera-relative transforms and render copies belong to each view; one floating origin cannot simultaneously maximize precision for distant cameras. Shared immutable assets can be reused across views while their transforms, visibility and runtime state stay independent. [big_space documents nested grids and independent roots](https://docs.rs/big_space/latest/big_space/); the integration must prove the required case.
- Display magnification is a view transform. Showing a molecule beside a house does not give its atoms house-sized bonds, mass, collision or gravity. Moving a view inset does not relocate the physical sample. Physical resizing is a separate edit that invalidates dependent properties.
- Keep scene frames separate from solver frames. Each solver owns a numerically appropriate local length/time/force scale. Convert at boundaries and record the conversion. Avian and `big_space` must never both write the canonical pose or fight over the same transform component.
- Distinguish frame motion from object motion. A rotating/accelerating simulation frame requires the appropriate velocity transformations and inertial terms, or the adapter rejects it. A render-origin change alone never injects velocity, impulses or work.
- Geographic frames declare horizontal CRS, vertical datum, axis order, units, epoch and required transform resources. Use established geodesy through a bounded adapter. Missing datum grids offline must produce unavailable/declared approximate alignment, not an unnoticed ballpark transformation. [PROJ describes operation accuracy, area of use and grid availability](https://proj.org/en/stable/operations/operations_computation.html).
- Earth context uses geographic/geocentric references; site work uses a surveyed or explicitly approximate local engineering frame. Dateline/poles, terrain heights, building elevations and curvature have explicit handling. A map projection's horizontal distortion is not acceptable engineering geometry by default.
- Astronomical states declare origin, orientation, epoch, time scale and geometric/apparent interpretation. Ephemerides are optional versioned inputs. At cosmological extents, proper/comoving distance and model assumptions need a domain coordinate map; a huge Euclidean scene is addressability, not cosmology. [NASA NAIF documents SPICE state/frame/time conventions](https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/C/req/spk.html).
- There is no single universe-sized absolute epsilon. Geometry, picking, alignment, contact and solver tolerances are dimensioned, bounded relative to the local feature scale and reported separately. Precision cannot exceed source accuracy.

A clock has a time scale/epoch and a local numerical coordinate. Interactive ticks, scientific femtoseconds, calendar instants and astronomical epochs need conversion contracts, not a single millisecond counter. Persist exact event ordering/tick indices where required; use time windows or epoch-relative values to avoid subtracting huge timestamps. Views, recordings and physical runs choose their clocks without changing operational history.

### K3. Spatial support, topology, selections and correspondence

A **support** is the domain on which a value, material assignment, edit or boundary condition acts: point set, graph, curve, surface, volume, geographic region or discrete resource set. Its descriptor states dimension, local frame, bounds, topology representation and available queries. A support is not automatically a closed solid.

Use a capability contract instead of forcing every representation to answer every query: bounds; ray intersections; closest point; oriented normal; inside/outside classification; distance with error; adjacency; measure; and semantic subelement lookup. Every answer includes source revision and exact/approximate/unknown status. An open scan may support ray picking but have no valid volume or inside test. Bounds accelerate a query; they do not prove containment.

A **named selection** identifies a face group, volume region, lattice sites, material interface, ports, boundary or semantic objects. It can be explicit stable subelement IDs or a versioned selector with expected cardinality and evaluation rules. Unsupported or ambiguous reevaluation invalidates consumers. Nearest-face matching may be a suggested repair, never silent acceptance.

- Atom/site IDs, source feature IDs and mesh half-edge/face IDs stay in compact domain buffers. IDs survive harmless reorderings. A split or merge records `unchanged`, `generated`, `modified`, `split`, `merged`, `removed` or `ambiguous` correspondence with provenance.
- A mount or boundary condition records its owning occurrence, named selection, local chart/coordinates, orientation, offset and source revision. Mesh barycentric coordinates can locate a point in one revision; they are not durable identity after remeshing. Preserve a last resolved pose and expose repair when no unique successor exists.
- Renderer picking returns root, occurrence, selection/subelement, representation revision, view and coordinate frame. A delayed pick from an obsolete revision is resolved through a verified mapping or rejected. Invisible, unauthorized or proxy-only geometry cannot select private semantic data.
- Correspondence between source, render, collision and solver geometry is an explicit artifact. Many-to-one mapping after simplification is normal. A result can only be displayed where its mapping is valid; unsupported gaps stay visible.
- Region queries use the same frame and boundary rules for Areas, selection, authoring masks, material assignment and solver boundaries. A domain can request different sampling policies, such as an item's anchor point versus occupied-volume overlap; this policy is explicit rather than a disagreement between capabilities.
- Legal/jurisdictional regions and measured geometry can share query infrastructure but retain their distinct authority and uncertainty. A drawn Area never creates ownership.

### K4. Quantities, fields, evidence and unknown values

A quantity declares a physical quantity kind, dimension, unit, numeric representation and validity. Dimensions alone are insufficient: torque and energy need different kinds despite equal SI dimensions. Temperature values and temperature differences have different conversion rules; affine units are not multiplied like vectors. Counts, fractions, concentrations and currencies also need explicit basis/context.

Reuse Lince's exact accounting for quantities that must conserve exact accepted balances. Preserve exact authored dimensions where useful. Numerical geometry and solvers can use f32/f64 arrays with declared units and error; do not replace every simulation number with arbitrary-precision arithmetic. Never round an operational balance through a GPU float.

A **field** is a typed value over a support and optional time axis. Its descriptor includes quantity kind/unit, scalar/vector/tensor/components, basis/orientation, reference or deformed configuration, sample association, interpolation/extrapolation policy, validity mask, uncertainty/provenance and payload references. A constant material parameter, a temperature grid, point observations and a displacement result use the same envelope with different supported evaluators.

- Distinguish per-node, per-cell, per-face, integration-point and graph-site samples. Choosing a visually smooth interpolation does not make it physically valid. Complex/frequency-dependent values, spectra and distributions need typed domain payloads, not strings that masquerade as scalars.
- Tensor transformations follow their physical definition. Rotating a crystal rotates its constitutive tensor; it does not merely rotate a color legend. Declare stress/strain conventions, Voigt/Kelvin notation and engineering shear factors in adapters.
- Distinguish unknown, withheld, missing coverage, stale, not applicable, unsupported, disputed and confirmed zero. Use explicit masks/status; do not encode unknown conductivity as zero or missing coordinates as NaN.
- Evidence records source/method/revision, measured/computed/estimated/authored status, units, conditions, validity interval, uncertainty or unknown uncertainty, and rights. Capture time, received time, accepted revision time and simulation time remain separate.
- A property resolver returns the chosen candidate and its applicability evidence. Conflicting sources remain separate candidates. Resolution is governed by an explicit workspace/material policy or user choice; timestamps and numeric averages are not default scientific authority.
- Extrapolation or user-invented laws remain allowed when deliberately selected, with their assumptions stored in the scenario. Evidence-backed and invented behavior must remain inspectable side by side.

### K5. Relations, ports and constraints

Typed relations connect definitions, occurrences, selections, evidence and Records. Relations do not all execute equations. Semantic dependency, chemical bond, CAD mate, electrical net, fluid connection, thermal contact, material interface and resource handoff declare their own schemas and validators.

A **port** identifies a localized interface, supported variables and orientation/sign convention. A **constraint** states a domain equation/inequality or admissibility rule and the solver responsible for enforcing it. Shared infrastructure handles references, units, revisions, selection, visualization and diagnostics; the domain solver supplies mathematical meaning.

- A CAD coincident constraint, a rigid hinge, a covalent bond and a welded finite-element boundary can use the same reference envelope without pretending they are one spring.
- Connection loops are allowed where the domain supports them. Report unconstrained degrees of freedom, redundant/conflicting constraints and numerical singularity. A cyclic assembly is not an illegal composition cycle.
- Distinguish a dependency arrow from a conserved exchange. Energy, mass, momentum, charge and operational inventory each require the appropriate balance and units. Store port direction and whether a value is an intensive state or an extensive/integrated transfer.
- Interface properties belong at the interface: contact conductance, friction, adhesion, electrical contact and bonded/slipping behavior cannot always be inferred from the two bulk materials.
- Transforming a constrained object supplies a solver target or explicitly changes/detaches a constraint; it cannot silently overwrite an incompatible pose. Report which definition, occurrence override or constraint would change before commit.
- Constraint satisfaction, material/valence validation and legal/operational policy are different checks. An invalid creative design can be saved as a draft with diagnostics; it cannot be represented as a passed solver result or an accepted prohibited operation.

### K6. Operators, derivations and incremental evaluation

An **operator** is a versioned supported computation with typed input/output ports, parameters, required capabilities and limits. A **derivation** records one invocation against exact inputs, settings, software and evidence. Examples include an extrusion, a mesh simplification, a geodetic conversion, crystal homogenization, collision generation, a solver mesh and a physical run.

Use two execution classes: short pure operations that can safely run within a bounded edit budget; and asynchronous jobs that prepare outputs and publish only after validation. Existing Rule/Command systems schedule authorized behavior; this is not a new universal programming language or arbitrary code embedded in a model.

- Inputs include source revisions, selected regions, units/frame maps, material models, boundary conditions and adapter version. The cache key includes every output-affecting setting. A tool version change cannot reuse an old result by filename alone.
- Dependency edges are declared, including references to other documents and local datasets. Invalidate downstream products on relevant edits. Definition changes do not rebuild unrelated city tiles. Revisioned deterministic selectors make dependencies inspectable.
- Jobs have resource estimates/limits, priority, progress, cancellation, output-size limits and a captured input generation. At completion, verify hashes, domains, mappings and expected revisions. Late output is retained as an older result or discarded; it never overwrites a newer head.
- Distinguish numerical reproducibility within a tolerance from bitwise determinism. Store solver build, precision, platform, thread configuration and seed where relevant. A Merkle hash of inputs does not guarantee floating-point runs match across GPUs.
- Imports are derivations too: original bytes, source identity, adapter version, declared units/frame, imported semantics, loss/unsupported report and source-to-model mappings. Keep original data where rights permit.

### K7. Representations, quality and scale transitions

One modeled object can have several representations for distinct purposes: editable source, measurement, render, picking, collision, solver discretization, aggregate properties and searchable semantics. Each descriptor references its source revision, support/frame, bounds, available capabilities, cost, error/uncertainty, rights and generation recipe. A representation is never promoted to a stronger purpose because it looks convincing.

| Representation family | Strong use | Limits and required conversions |
| --- | --- | --- |
| Analytic curves/planes and polygon boundary meshes | Rooms, direct low-poly/freeform editing, surfaces, runtime meshes | Open/nonmanifold meshes may have no solid volume; check orientation, self-intersection and topology before physical use |
| Feature history and boundary-representation solids | Dimensioned mechanical CAD, exact surface relationships, manufacturing measures | Kernel tolerances still apply; tessellate for rendering, mesh for PDEs, generate suitable contact proxies |
| Height fields | Large terrain and existing additive floor effects | One height per chart position cannot describe caves/overhangs; add separate surface/volume supports |
| Sparse implicit/volumetric fields | Caves, density, porosity, sculpting, fluids or image volumes | Cell/field resolution controls accuracy; extraction and collision are derived and need seam/mapping checks |
| Atomic/molecular graph and periodic sites | Chemistry, conformers, lattices, microscopic studies | Bond display is not a complete electronic state or force field; preserve cell/symmetry/conditions |
| Beam/shell/solid meshes, particles and discrete networks | Structural, thermal, fluid, circuit and other specialist models | Their domain and element/solver assumptions are explicit; a display triangle is not automatically a valid element |
| Point clouds, images and Gaussian splats | Captured appearance, evidence, reconstruction inputs | Unknown/unseen space remains unknown; use measured/proxy geometry for support or collision |
| Geographic features and spatial tile hierarchies | World/city context and sparse streaming | Source resolution, CRS and capture age constrain measurement and semantics |

Do not use a single scalar “LOD” as the whole policy. Independently control visual error, semantic coverage, physical/solver model, temporal rate, spatial extent and offline residency. Changing visual detail never silently changes mass, inertia, topology, accepted facts or an in-progress solver.

Two distinct transitions require different evidence: **representation reduction** approximates a given source for a purpose; **physical coarse-graining** constructs an effective model with assumptions. A low-poly tree and a reduced biomechanics model can share a source but need different validation. Returning from a coarse representation restores retained detail; returning from a coarse physical model cannot reconstruct unique missing microscopic state without an explicit lifting/initialization model.

### K8. Domains, state, observations and scenarios

A **domain** owns the active simulation support, units, clock, boundary conditions, state variables, constitutive models, stepping policy and authority/capabilities. An occurrence may participate in different domains through explicit maps. Shared authored definitions do not imply shared mutable velocities, temperatures, random generators or solver buffers.

Separate four things throughout persistence, UI and APIs: authored/captured representation; observations with uncertainty; accepted operational state; and conditional scenario results. Observation ingestion does not automatically edit geometry, stock or ownership. Scenario outputs never become live commands or accepted facts without an explicit authorized application operation.

A scenario pins a composition base, input fields/materials, initial/boundary conditions, domain/solver versions, seed, time range, resource budget and results. Bounded time-indexed poses/fields/media reference stable objects and frames separately from static geometry. Independent views can select different bases and playback positions. Only the shared data/state contracts are scheduled here; specialized reconstruction is a later adapter with its own plan and proofs.

State streams can also reference topology revisions or typed birth/death/connectivity events when a supported process changes structure. IDs are not recycled within a run. A molecular reaction, fracture or construction sequence must not overwrite its initial graph or silently mutate accepted source geometry. Keep event checkpoints and mappings to the base so bounded seeking/replay does not require replaying an entire history. Time interpolation declares valid continuous intervals; it never blends across an unmodeled topology change or missing observation interval.

The initial domain can use one isolated local runtime. Independent clocks or physics configuration later require separately owned solver state; collision masks in a single global Avian world are not enough. A worker or isolated Bevy sub-application per active physical domain is a candidate, with a small active-domain limit and immutable snapshots sent to views. Do not instantiate an engine/world per model object.

## 4. Native Spacematter format and artifact lifecycle

Adopt a specialized **Spacematter document** for composition, provenance, dependencies and delivery. It describes relationships from a molecular structure to a site/world without replacing standard scientific payloads or inventing an entire CAD kernel. The same logical schema serves a tiny material sample and a sparse planet.

### Document layout and manifest

Proposed names: editable directory `name.spacematter/`; portable snapshot `name.spm`. A normal Lince workspace stores the same manifests/chunks in its existing asset storage and references a document root. Saving does not require exporting a packed file. Names/extensions are provisional; the contracts below are the design decision.

The directory form contains a small root manifest, sharded immutable descriptor pages, content-addressed binary/source payloads, and license/provenance attachments. A derived-data cache and local edit journal live outside the portable source snapshot unless explicitly included. Large object indexes and time series must be paged; the root cannot enumerate every feature or atom.

| Root/descriptor field | Required meaning |
| --- | --- |
| `format`, `schema`, `required_profiles` | Protocol/schema identity and supported mandatory domain features; not an application binary version |
| `document_id`, `parents`; external head `root_revision` | Stable logical document and optional lineage; the head envelope points to a snapshot hash outside the content being hashed |
| `roots`, `descriptor_pages` | Root instances and bounded descriptors/index pages; child definitions are pinned by revision |
| `frames`, `units`, `clock_refs` | Coordinate/unit/time definitions or versioned references needed by the selected content |
| `definitions`, `instance_slots`, `selections`, `relations` | Paged model composition and domain references; arrays remain external payloads |
| `representations`, `derivations`, `result_sets` | Purpose-specific outputs, exact inputs, maps and execution/validity status |
| `payloads` | Media/profile, hash kind, decoded byte count, codec, bounds/shape/type where applicable, dependency references |
| `coverage`, `detail_policy`, `offline_profiles` | Available space/time/source resolution, required closure and user residency choices |
| `sources`, `evidence`, `rights` | External namespaces/revisions, methods, license/attribution and disclosure requirements |
| `record_bindings` | Policy-controlled links; absent bindings are valid for a standalone scientific asset |

A snapshot is a Merkle DAG of descriptors/payloads, not a recursively hashed arbitrary object graph. The root content never contains its own hash. A separate head/envelope carries the root hash and optional signature over that hash/context; signatures are outside the signed content. Cyclic semantic/constraint relations use stable object IDs resolved through the snapshot's revision table, rather than cyclic content-hash references. Cross-document references pin a target snapshot/object.

Distinguish required dependencies for the selected capability/profile from declared deferred/optional content. A valid coarse world root may reference unloaded detailed regions without claiming them offline. Validate structural references separately from a requested offline closure; only claim a profile complete when every required descriptor/payload, style, transform resource and input is present and authorized. Optional external content stays explicit and cannot execute or download itself.

An extension/profile has a namespace, schema version, validator and capability declaration. Unknown optional payloads may be preserved opaquely within size budgets; mandatory unsupported profiles make the affected object read-only/unsupported. Neither case permits silently re-exporting an incomplete model as semantically equivalent. The original unsupported bytes remain available when permitted.

### Encoding and portability

1. Use Lince's existing canonical JSON and domain-separated SHA-256 for structured manifests after a conformance audit. Preserve its prohibition on bare floats. Define canonical exact decimal atoms as bounded normalized coefficient/exponent pairs, signed i128 cell-address strings and binary64 atoms as 16 lowercase hexadecimal bit digits where required. Normalize zero and forbid nonfinite geometric values. Each field schema accepts exactly its declared numeric kind; decimal versus binary data is not silently interchangeable. Bound digit/exponent lengths and prove known byte/hash vectors. A JSON debug view is not a second source of truth.
2. Put dense geometry, site tables, instance populations, fields and trajectories in typed binary chunks or retained standard formats. The native array envelope states component type, little-endian order, shape/strides, quantity unit, frame, association, validity mask and byte length. Validate multiplication/offset arithmetic before allocation. Geometric coordinates must be finite; field invalid values use a mask. Compression is a transport/storage choice, not physical precision.
3. Distinguish a canonical structured hash from a raw payload hash in types and schemas even when both use `sha256:` text. Raw payload identity hashes its decoded bytes; the descriptor pins the interpretation. Geometry codec parameters, quantization errors and mappings are explicit derived metadata. Recompression does not change decoded identity.
4. Use standard ZIP64 as the leading packed-container candidate, with opaque hash-named entries, a bounded manifest/directory and independently stored or compressed payload chunks. Never require decompressing the whole world to inspect a room. Directory access is implemented first; packed range reads, duplicate/path checks and large-offset validation must pass before remote streaming uses the packed form. Existing PMTiles/other indexed archives can remain separate payloads.
5. A packed export is an immutable selected snapshot. Include the complete required dependency closure for its chosen offline profile and a report of optional, omitted, restricted or unavailable data. An external URL is not offline availability. A descriptor cannot secretly fetch dependencies when explicit offline mode is enabled.
6. Publish format schemas, canonical byte/hash vectors, valid/invalid tiny samples and a Rust validation/inspection tool through cargo xtask. Round trips preserve logical/subelement identity, units, arrays and supported semantics. Do not force conversion of crystals to triangles, CAD history to glTF or solver fields to screenshots to save them.

Bound entry count and directory bytes per packed file. A very large snapshot can reference several immutable bounded packs through its paged catalogue; it must not require reading an unbounded central directory to locate one room. Pack references are explicit hashed dependencies, not recursively auto-extracted archives. User-selected offline export either includes its required packs/chunks within the accepted budget or reports the unsatisfied closure before claiming completion.

Do not introduce a new general binary container before the directory schema and real access patterns are proven. If ZIP64 fails the fixtures, record the measured reason and specify a seekable replacement at that gate. [OpenUSD's layers, references and deferred payloads](https://openusd.org/release/intro.html) are useful composition precedents; an optional USD adapter remains valuable. The native format additionally needs Lince identity/policy, scientific validity and offline closure contracts that adopting a scene format alone does not establish.

### Commit, recovery, sharing and garbage collection

- Write new chunks/descriptors to staging, validate their closure, flush durable data, then atomically compare-and-swap the authorized root revision. A successful save receipt refers to durable reachable content. A crash leaves the old root usable and orphan staging collectible. The store and blob filesystem need a recoverable commit protocol; do not pretend a SQL transaction atomically commits arbitrary files.
- Edits carry actor, scope, expected base, typed operation and inputs. Dragging updates a local preview; release commits one undoable transaction. Cancel discards the preview. Native/API/automation/import paths run the same validation. Unsaved drafts and failed operations remain recoverable without becoming accepted heads.
- Undo of authored geometry creates a new consistent edit/revision through existing undo machinery. Published operational adjustments use the accepted event/compensation semantics in F2–F4. Undo never claims to reverse a physical manufacturing step.
- For concurrent geometry edits, start with explicit edit scopes and optimistic revision conflicts. Preserve both branches and show affected selections. Generic text CRDT merging of Booleans, constraints or material assignments is not a valid geometry merge. Commutative independent edits may be merged only where demonstrated.
- Import/re-import performs a three-way reconciliation: previous source, new source, local layer. Stable source IDs take priority; geometric fingerprints are suggestions with confidence. Deleted/split elements leave unresolved bindings and repair actions. Authored layers are not overwritten by a newer source timestamp.
- Reuse existing Transfers/artifact delivery where verified for content delivery. A root publication, body transfer, rights grant, receipt and accepted model revision are separate states. Partial downloads can be verified/resumed without publishing an incomplete mandatory root.
- References are not read authority. Enforce policy on manifests, chunks, spatial bounds, counts, thumbnails, indexes and results. Private data can influence a derived output, so publication of that output needs its own authorized projection/disclosure decision. Do not mark it public merely because it contains fewer triangles.
- Dedupe within an appropriate authority/encryption scope. Global content-address probing must not reveal private asset existence or cross-tenant equality. Reuse existing authenticated encryption/key handling where needed; do not invent cryptography in this format. Revocation can stop future reads/updates, not retract plaintext already exported to another party.
- Pin accepted roots, user offline selections, unsynced local edits, retained source history and required scientific inputs. Mark-and-sweep unreachable chunks with a grace period and snapshot-safe readers. Rebuildable caches have a separate eviction policy. Disk exhaustion fails before replacing a valid root; it must not erase pinned sources to make a bake fit.

## 5. Canvas evolution and native workflows

Replace the flat-plane assumption through a staged spatial document/view separation. Preserve useful 2D work: the ordinary canvas becomes an orthographic/layout view onto the same selected model/Records, while perspective, plan/section, molecule, CAD sketch and geographic views choose appropriate navigation and tools. Do not maintain parallel truth in a “3D canvas database.”

A view selects document/base revision, root/occurrence, display frame, projection, detail policy, visible layers, selected time/result and tool context. Presentation transforms are not canonical placement. A workspace may contain views into several models; a model can appear in several workspaces under different grants.

- Represent Sands/Castles as existing content objects with a bounded surface adapter: display bounds, pointer/keyboard focus, local hit coordinates, minimum readable presentation, lifecycle/demand and optional physical body. Content remains usable in a normal flat/native view.
- Surface mounts are explicit constraints/attachments, not gravity exceptions inferred from proximity. Mounted content follows a verified surface frame; unmounted content follows its chosen motion/layout policy. A UI card's interaction collider and mass are authored interaction parameters, not a claim that digital text has material density.
- Initially mount readable planar content to a planar/tangent patch with an explicit normal offset and sidedness. Curved/deforming content requires its own tessellation, hit-coordinate map and input proof; a curved model surface does not establish supported curved UI or physical projection. A source topology change preserves focus/content state while the mount is repaired.
- The compositor/window-management path owns real OS surfaces, focus, input and output windows. Model views can display assigned textures/proxies where supported; they do not take ownership of arbitrary application windows or bypass output/pairing policies.
- Selection and commands share stable occurrence IDs across perspective, map, tree, table, sketch and section views. Entering an assembly changes editing context, not ownership or source identity. Units, active frame and edit target stay visible in tool properties.
- Provide shared native transaction tools: select/subelement select; transform; numeric dimensions; align/snap with typed references; inspect; create/instance; preview/cancel/commit; undo/redo; source/override repair; and explicit bake. Domain tools plug into these interactions rather than inventing independent save/undo systems.
- Focused presets expose relevant controls: room tools show walls/openings/mounts; chemistry shows atom/bond/lattice tools; CAD shows sketches/features/constraints; material studies show conditions/models/results; geography shows region/detail/offline controls. Expert data stays inspectable without making every user configure a universal graph.
- Preserve keyboard operation, accessible tree/forms, text labels for statuses, configurable shortcuts and recovery from a trapped camera/tool. Objects can be found and edited through a list without precise 3D picking. Colors and shading are not the only representation of error or uncertainty.

Build a bridge from current placements/Room metadata to model handles. Existing bounds remain admission limits for live UI content, not limits on static world definitions. Prove one end-to-end replacement path before retiring an old code path; avoid a prolonged duplicate frontend or a single unreviewable canvas rewrite. The old `web` crate is not the implementation destination.

## 6. Geometry, topology and attractive low-poly spaces

### Authoring representations and exactness

The first editable spatial source uses analytic planar patches plus polygon/half-edge boundary meshes with stable local element IDs. Preserve polygons for editing; triangulation is a render derivation. Support open surfaces intentionally. Watertight, oriented, non-self-intersecting shells are required before claiming a solid volume. A displayed mesh can still be useful when those tests fail, with unsupported solid operations explicit.

Provide create plane/wall/floor, move vertex/edge/face, split/connect, extrude, inset/opening and numeric/snap controls in bounded increments. Room walls, slabs, ceilings and desks favor analytic planes/extrusions. A cave fixture uses an explicit connected mesh with floor/ceiling/overhang queries first; volumetric sculpting follows when the shape needs topology-changing unions/cuts.

Exact mechanical CAD has a different source representation and adapter, described below. An exact planar primitive can be shared; a low-poly boundary mesh does not become an exact NURBS solid by renaming it. “Exact CAD” still has kernel tolerances and operation-specific numerical failure.

Sparse volumes use bounded local blocks and versioned scalar fields; they do not allocate a voxel array over the planet. Extract mesh/collider products only in changed regions with neighbor halos and shared boundary conventions. [OpenVDB's sparse-field model](https://www.openvdb.org/documentation/doxygen/overview.html) is a reference/adapter candidate, not a mandatory C++ dependency for the first room.

### Live mathematical topology and anchors

Preserve existing height effects as an explicit additive profile: `h(u,v) = h_base(u,v) + Σ a_i w_i(u,v) f_i(u,v)`. Here chart coordinates are transformed into each effect's local frame, `f` and falloff `w` have declared dimensionless inputs, and `a` has length units. Equal positive and negative contributions with the same support and shape cancel independently of order. Do not reinterpret these as Boolean union/subtraction; implicit solid operations have different semantics and do not generally cancel.

For selected non-height surfaces, a bounded displacement operator can evaluate `p' = p + n_ref d(p)` from a captured reference surface. Declare which patches it affects, the normal/frame, support and foldover/self-intersection checks. Independent displacements that claim additive behavior sample the same reference revision. Sequential deformations are an ordered operator chain and make no cancellation promise. Topology-changing cuts/Boolean operations are separate operators.

Bindings resolve a fixed frame, cursor hit/working plane, camera or stable item occurrence plus offset/orientation policy. A camera-bound region can follow the camera's actual position or its floor projection; the mode is explicit. Cursor ray misses, deleted anchors or unavailable streamed objects preserve the last valid effect pose and show unresolved status; no jump to world origin. Freeze, retarget or detach is a native edit. A camera moving through a place moves its influence under the selected binding, with all Area capabilities sharing the resulting region query.

Dirty only tiles/patches intersecting the old and new effect bounds; retain seam halos. Limit formula size/evaluation count and reject nonfinite output through existing bounded expression infrastructure. While dragging, coarser preview is allowed with its status shown. Settled explicit bakes pin an effect/source revision; later source motion marks them stale and offers rebake, rather than silently changing a finished model or losing the live source.

### Low-poly appearance, simplification and physical use

Low-poly quality is an authored style: deliberate planar regions, silhouettes, readable scale, restrained materials/lighting, consistent face orientation and chosen sharp/smooth boundaries. Fewer polygons alone do not make a scene attractive or faster. Preserve UV seams, material interfaces, narrow passages and meaningful architectural edges.

Simplification takes selected supports, triangle/vertex budget, dimensioned shape-error limit and protected features. Return a preview, source-to-result mapping, achieved counts/error, violations and timing. Distinguish a heuristic simplifier metric from a measured maximum deviation or a conservative bound. Sampling a Hausdorff-like distance is not proof of a global maximum.

Use constrained collapse/QEM or another proven library as the leading surface candidate; protect borders, mounted patches, openings, sharp edges and alignment references. Reject collapses that invert faces or destroy required passages. The [original QEM work](https://www.cs.cmu.edu/~garland/quadrics/) establishes the simplification family; additional quality checks are Lince requirements, not guaranteed by that algorithm.

Render mesh, static collision mesh, dynamic convex/compound proxy and measurement source have independent budgets and recipes. A very thin wall must remain a boundary even when visually subpixel. Closed dynamic bodies need valid mass/inertia from a supported solid/material model or explicit user values; an open scan has no justified automatic volume. Never use appearance splats as a solid collider.

A collision update is transactional at a physics step boundary: retain the old collider until the replacement is ready, map contacts where valid, resolve new penetration deliberately, then wake affected bodies. Visual LOD changes do not swap physical support underneath a sleeping Sand. Protect navigable clearances and mount orientation through save/reload and rebake.

Detailed topology requirements T1–T6 below remain normative and use these contracts.

## 7. Materials: knowledge, structure, constitutive behavior and scale bridges

### Material identity and state

Separate a material specification, a physical/digital sample or batch, a region assignment and its evolving state. A specification names composition/structure descriptors and candidate property/constitutive models. A batch adds process/history/quality evidence. A part assigns material versions to regions, layers, fibers, coatings, pores, interfaces or graded distributions. A run owns temperature, plastic strain, damage and other history variables.

Composition is typed: chemical identity, isotope identity where needed, species fractions with a mass/mole/volume basis, phase fractions, lattice/crystal orientation, molecular conformers, microstructure descriptors and processing conditions. Validate normalization and charge/mass assumptions where the selected profile requires them. Defects, grain boundaries, porosity, moisture, fabrication, texture and interfaces can change behavior. Composition and folding alone cannot supply every bulk property. [NIST's structure–property research](https://www.nist.gov/programs-projects/structure-property-links-deformation-materials) supports retaining those intermediate descriptions.

### Property model contract

A property candidate includes quantity kind/unit, scalar/vector/tensor/table/law, support/sample, material orientation, phase, conditions, method and uncertainty. Conditions can include temperature, pressure, strain/rate, frequency, wavelength, moisture and history. The exact dimensions depend on the physical question.

A **constitutive model** is stronger than a property bag. It declares input state/gradients, outputs, internal variables, admissible parameter ranges, reference configuration, integration/update method and required tangent/derivative support. Initial subsets can implement small-strain linear elasticity and Fourier conduction. Plasticity, hyperelasticity, viscoelasticity, fracture, phase change and reactive behavior are later supported models with their own validation.

- Linear elasticity uses `σ = C:ε` under a stated stress/strain convention. Orthotropic/anisotropic materials retain orientation and tensor symmetry. Appropriate stability/positivity checks depend on the model; do not accept any 6×6 numeric matrix as usable elasticity.
- Fourier heat conduction uses `q = -K∇T`, with tensor `K` if needed. Capacity, density and interface resistance are separate inputs. Radiation/convection require boundary models and are not encoded by calling a surface “warm.”
- An interaction friction/restitution model for Avian is not automatically derivable from bulk Young's modulus. Adhesion/contact/wear depend on surfaces and conditions. Preserve explicit interaction parameters and their evidence.
- Render appearance is another model: color/roughness/metallicity/opacity or measured optical response. A physical electrical conductor need not be assigned a metallic shader; roughness is not a thermodynamic material law. Measured spectral data can drive rendering through a named approximation.
- Resolution returns supported/incomplete/out-of-domain plus the selected model/version and required missing inputs. User-authored fictional properties are allowed as explicit models. Bad inputs cannot silently fall back to plausible-looking defaults in a scientific result.

### Knowledge ingestion and applicability

Import bounded, attributed offline subsets. Preserve external identity, source revision, original units, conversion, conditions, methodology, uncertainties, license/redistribution rules and supplied versus inferred values. Re-import exposes property-model conflicts. A provider's search hit is evidence of a record, not proof it applies to the selected alloy, specimen or temperature.

| Knowledge family | Useful scope | Boundary |
| --- | --- | --- |
| [PubChem](https://pubchem.ncbi.nlm.nih.gov/) | Chemical identity, structures and source-attributed reported properties | Not a complete bulk constitutive database; preserve individual property evidence |
| [Materials Project elasticity methodology](https://docs.materialsproject.org/methodology/materials-methodology/elasticity) | Computed crystalline elastic tensors and documented calculation settings | Conditions/methods and comparison to experiments matter; an ideal crystal is not an arbitrary manufactured part |
| [NIST reference data](https://www.nist.gov/srd) and measured engineering data | Specified physical/thermophysical quantities with references | Dataset rights and covered phases/conditions differ; some resources are not freely redistributable |
| [IUCr CIF](https://www.iucr.org/what-we-do/digital-standards/cif) and [PDBx/mmCIF](https://mmcif.wwpdb.org/docs/user-guide/guide.html) | Crystal/macromolecular structures, cells, symmetry and experimental metadata | Structure records do not provide every force field, folding path or bulk property |
| Evaluated nuclear data through a selected adapter | Isotope-dependent interactions, decay and model inputs | Chemical species tables cannot replace evaluated nuclear data |

Select adapters by the first question and license, not by the largest downloadable dataset. A useful initial pack can contain only a few traceable materials, one crystal, one finite molecule and the reference cases used by tests. Record API keys outside portable files and avoid per-frame remote lookup.

### Explicit cross-scale transfer

A scale bridge is a derivation, not containment. A wall **uses** an effective material model; it does not need a runtime child entity for each atom. Store the fine input, representative sample/volume, boundary conditions, ensemble, method, averaging assumptions, output law, validity range, convergence and validation evidence.

Recommended first real bridge: import a documented crystal stiffness tensor with its structure/method; rotate it correctly; calculate orientation-dependent response; then optionally derive Voigt/Reuss bounds and a Hill estimate for a stated randomly oriented polycrystal assumption. Keep anisotropic crystal and effective isotropic outputs distinct. Derive `E` and `ν` from effective `K` and `G` only under the isotropic assumption. Show that porosity, texture or processing outside those assumptions requires another model.

An earlier geometry-based bridge is ideal crystal density: `ρ_ideal = Σ_j occupancy_j mass_j / |det A|`, where `A` is the physical cell matrix and the sum is over that cell's full site population, with symmetry multiplicities expanded or accounted for exactly once. Here mass is per site/particle; a molar-mass input requires the declared Avogadro conversion. Changing cell/occupancy can recompute this quantity without a quantum solver. It does not predict porosity, phase stability or measured bulk density. Under the later isotropic elastic approximation, use `K_H=(K_V+K_R)/2`, `G_H=(G_V+G_R)/2`, `E=9K_HG_H/(3K_H+G_H)` and `ν=(3K_H-2G_H)/(2(3K_H+G_H))`, rejecting singular/invalid inputs. Compute the Voigt/Reuss terms from the declared stiffness/compliance convention and verify against independent reference values.

This proves knowledge lineage and a bounded mathematical bridge. It does not claim that editing atom positions automatically recomputes stiffness. Geometry edits invalidate the link until a supported calculation, new measurement or explicit retained approximation justifies it. A later MD/quantum adapter can populate a new candidate against the edited structure.

For computed bridges, require sample-size/ensemble and mesh/time convergence, reference units and the appropriate averaging. Density, stiffness, thermal conductivity and electronic response need different methods. Simple mixture fractions are not a universal blending rule. Downscaling an effective continuum state to microscopic initial conditions is underdetermined; require an explicit generated sample/ensemble and assumptions.

## 8. Domain workflows as implementations of the primitives

Each workflow below reuses K1–K8, the common transaction/native inspector, format and job lifecycle. Its specialized representation, validator and solver are explicit. A new domain profile earns shared complexity when two distinct workflows need the same invariant; otherwise retain a thin domain module.

| Workflow | Authored/source support | Specialized operation/constraint | Shared outputs and reuse |
| --- | --- | --- | --- |
| Molecule/crystal | Stable sites, finite graph or periodic cell | Chemical validation, lattice/symmetry edits, selected microscopic calculation | Representations, conditions, evidence, candidate properties and invalidation |
| Material model | Composition/microstructure/sample plus evidence | Applicable constitutive law and explicit scale bridge | Versioned law/fields assigned to regions in any part |
| CAD part/assembly | Sketch/feature graph, B-rep and instance slots | Constraint solve, solid operations, mates/tolerances | Measurement, render/collision/solver products, manufacturing views |
| Room/cave | Planar/polygon boundary or sparse implicit support | Extrusion/opening, sculpt/displacement, mount and simplification | Same surface queries/selections, material regions, content and local dynamics |
| Building/civil work | Spaces/elements, surfaces/volumes, alignment/network | Layers/openings, corridors/systems, takeoff and domain checks | Geography, plans/sections, analytical models and Record/procurement links |
| City/world | Geographic features, source packages and nested sites | CRS conversion, spatial partition and quality-driven refinement | Bounded offline representations, semantic queries and detailed-place instances |
| Chip/tree/nuclear/other study | Domain graph/regions/fields with known inputs | Domain-specific equations, constitutive laws and coupling | Same job/result/provenance contracts, distinct validation and supported scope |
| Astronomical composition | Sparse nested frames, state/field/catalogue payloads | Frame/time conversion and selected dynamics | Local high-precision inspection, bounded residency and evidence-backed state |
| Operational/civic work | Records, resources, places and accepted events | Authority, exact accounting, processes and proposals | Model bindings, queries, scenario results and selective publication |

Concrete cross-scale reuse is a chain of references and derivations: crystal/sample → property evidence/model → material specification → part-region assignment → assembly occurrence → building/site placement → geographic/astronomical context. Composition describes what is assembled; derivation describes what is inferred; spatial placement describes where it is. These are three different edges. Imported/measured content can enter at any level and missing lower levels remain absent. No step expands all finer levels merely because the user opens a larger assembly.

### Molecular, crystalline and smaller-scale authoring

Provide compact site/atom buffers with stable IDs, species/isotope/charge where relevant, coordinates and optional topology. Finite molecules have bond order, aromatic/stereochemical representation, conformers and hydrogen/protonation policy. Crystals add a nondegenerate cell matrix, fractional sites, periodic axes, occupancy and symmetry/space-group metadata. A periodic neighbor is identified by site plus image offset; minimum-image assumptions and neighbor cutoffs must match the cell.

The first native editor supports atom/site selection and numeric placement, add/delete/species change, finite-molecule bonds, cell editing, repeat/supercell preview, symmetry-breaking confirmation, save/undo and explicit validation. Render repeated sites with instancing, not one heavyweight Record per atom. A small molecule and a periodic crystal are separate fixtures, even when they share point rendering.

Structural edit validation reports valence/charge/stereochemistry and cell/overlap issues at the level the selected chemistry adapter supports. Inferred bonds are marked inferred; a coordinate-only import cannot establish connectivity uniquely. An unusual structure can remain an authored draft. It cannot acquire a validated force field by being visually plausible.

Support a declared SDF subset for small molecules and CIF for crystals first; add PDBx/mmCIF when macromolecules need biological assemblies, alternate conformations, occupancy and experiment metadata. Preserve unsupported chemistry/source fields rather than silently exporting a simpler molecule. Pin dictionaries/parser versions. File import/export is one capability; energy minimization, molecular dynamics, electronic structure, reactions and folding prediction are separate adapters.

A microscopic job declares potential/force field and parameter coverage, charge/spin where needed, periodic boundaries, long-range treatment, integration step, constraints, ensemble/thermostat/barostat, seed and sampling/equilibration. Reject unparameterized species or units. [LAMMPS documents distinct unit systems](https://docs.lammps.org/units.html) and [periodic boundaries](https://docs.lammps.org/boundary.html); [OpenMM documents force-field setup and simulation choices](https://docs.openmm.org/latest/userguide/application/02_running_sims.html). These are solver candidates, not implicit new runtime dependencies.

Population/distribution or quantum-field-like domain payloads can be added under the same typed field/operator contracts when a concrete study requires them. No molecular parser is claimed to model all subatomic physics.

### Mechanical CAD, assemblies, drawings and fabrication

Use parameterized sketches/feature history as source and a robust B-rep kernel as evaluator. Begin with lines/arcs/circles, workplanes, geometric/dimension constraints, extrude/revolve, through-hole/opening, Boolean union/subtract/intersect and reused instances. Add sweep/loft, fillet/chamfer, shell/offset, patterns and freeform surfaces in bounded subsequent operation sets with fixtures. Preserve direct mesh/freeform workflows alongside parametric CAD.

A sketch solver reports residuals and remaining degrees of freedom, distinguishes underconstrained from inconsistent/singular systems, and identifies relevant conflicting constraints. Parameter expressions are bounded, typed and unit-checked. Recompute the affected feature subgraph; an upstream failure leaves the last accepted solid inspectable and marks downstream products stale. Preview failure never destroys the previous revision.

Choose the kernel through a short recorded bake-off, not an unbounded research task: two intersecting/tangent solids, thin-wall cut, near-coincident faces, a dimensioned bracket, failed fillet, assembly instance, STEP exchange and worker cancellation/crash. [OCCT's modeling/data-exchange documentation](https://occt3d.com/dev/doc/overview/html/) and [truck_modeling](https://docs.rs/truck-modeling/latest/truck_modeling/) are candidates. The default recommendation is an OCCT worker behind a Rust protocol, with exact build/license/target checks before selection. Heavy kernel dependencies must not enter every map/material viewer.

Persistent topology needs semantic feature references, kernel history and explicit remapping. Give a generated opening or named mounting patch its own intent identity. A face ordinal is insufficient; geometric fingerprints alone are ambiguous on symmetric parts. Kernel naming mechanisms help but do not guarantee that every face has a unique successor. [OCAF documents topological naming and evolution](https://occt3d.com/dev/doc/overview/html/occt_user_guides__ocaf.html). Stop affected mounts, boundary conditions or downstream features for repair instead of attaching to the wrong face.

Assembly tools reuse definition/instance slots plus mate/joint ports. Support rigid placement, revolute/prismatic constraints, limits, interference/clearance and measurement; separate assembly kinematics from deformable/contact analysis. Parameterized reusable connections are domain presets, not copied part databases.

Retain 2D drafting, plan/section/elevation, associative dimensions, tolerances, annotations, drawing sheets and model-linked issues as representations of the same source. Measured versus nominal versus manufactured geometry remain distinguishable. Manufacturing intent can include stock, material grade, datum/tolerance specification, cut lists, tooling/process references and a bounded CAM/postprocessor adapter. Toolpaths and machine execution are separate from design; do not auto-command equipment on save/export.

The first exchange proof is a named STEP geometry/assembly subset plus a render export. Declare losses: an ordinary STEP or GLB round trip need not preserve native feature history, solver constraints or all manufacturing annotations. Source revisions and interoperability reports preserve that distinction.

### Buildings, AECO and local spatial design

A building model composes site/building/storey/space references, element definitions/occurrences, materials/layers, openings, connections, systems and constraints. Spatial containment and physical decomposition stay separate. A room volume can be bounded by shared walls; the wall is not duplicated as a physical object per adjacent room. Reference centerlines, nominal dimensions and as-built surveys have distinct provenance.

Native presets build walls/slabs/roofs, openings/doors/windows, stairs and reusable furnishings from the geometry/CAD primitives. Add layered assemblies, reference levels/grids, plans/sections and quantity takeoff. Takeoff uses declared measurement rules and source revision, deduplicates instances/openings, and reports uncertainty or unsupported geometry; a visual bounding box is not accepted billable volume.

Structural, MEP, thermal zones and circulation are different analytical representations. A duct network has ports/connectivity and pressure/flow assumptions; a structural wall has load paths/boundaries; a thermal wall has layers and contact conditions. Their geometries can map to the same wall without pretending IFC properties implement their solvers. Maintain fire/accessibility/clearance/regulatory rule versions and jurisdiction in issue/check adapters; an imported classification is not a claim of code compliance.

Civil design uses alignment curves/profiles, cross-sections, corridors, terrain cuts/fills, parcels/rights and utility networks as specialized supports/relations. Geotechnical layers and groundwater are volumetric/property inputs with source uncertainty. Connect streets and neighborhoods through these spatial/network primitives rather than baking their meaning into a decorative city mesh. Sheet-metal unfolding, composite layups, additive lattices and other manufacturing specializations similarly need material/process-aware operators and their own acceptance fixtures; preserve them as domain expansion scope rather than claiming generic extrusion completes CAD.

Import a declared IFC subset with stable source identity, placement, units, types/occurrences, material layers, spaces and opening semantics. Preserve GlobalId/source namespace and unsupported report. Use an isolated IfcOpenShell candidate worker when justified; its [geometry settings](https://docs.ifcopenshell.org/ifcopenshell/geometry_settings.html) show why unit/material/opening configuration matters. Export/viewpoint issues through a bounded IFC/BCF workflow as needed.

AECO construction scheduling, procurement, resources, production, operations, maintenance, civic participation and commerce remain in F1–F10 below. Their forms/queries reuse Records, evidence, quantities, commitments and permissions. They do not require a detailed model to work, and personal room authoring does not require a factory ledger first.

### Electrical/electronic, chemical, living and other specialist models

A chip/board combines geometry/material regions, circuits/connectivity and domain models. Netlist nodes/ports are logical connections, not mesh adjacency. Begin with a bounded circuit/thermal example and versioned compact device models; semiconductor transport, fabrication/doping and electromagnetic fields require separate adapters and inputs. A silicon lattice does not by itself predict an arbitrary chip. [ngspice documents its compact and mixed-level device models](https://ngspice.sourceforge.io/docs/ngspice-manual.pdf).

Chemical processes require species, stoichiometry, phase/equilibrium/kinetic models and conserved element/mass/charge accounting as appropriate. Molecular dynamics is not automatically a long-timescale reaction process. Reactive media, fluids and phase changes declare their own constitutive/state model and numerical method.

A tree can start as a procedural branching definition with instanced leaves and low-poly geometry. Mechanical response needs wood anisotropy, branch attachments, moisture and loads; growth needs a chosen physiological/environmental model and calibration. Geometry generation, biological growth and ecosystem simulation remain distinct operators connected by typed state, rather than one undocumented “tree physics” flag.

Nuclear-power-related studies remain explicitly in scope: isotope-specific composition, evaluated interaction/decay data, transport/depletion, heat generation, thermal/hydraulic response and later supported kinetics/coupling. Use versioned domain inputs, separately validated adapters and reference benchmarks. [OpenMC documents cross-section and depletion data configuration](https://docs.openmc.org/en/stable/usersguide/data.html). First prove a bounded transport/heating case with input coverage and statistical uncertainty; adding geometry or chemistry must not imply reactor-system fidelity.

Traffic, weather, geotechnics, acoustics and other domains use the same profiles only when their actual quantities/geometry/solver assumptions are specified. These capabilities stay on the roadmap with validation gates; they are not counted as delivered because an extension slot exists.

## 9. World data, detail selection and bounded execution

### Offline world and city pipeline

Use a sparse hierarchy of spatial indexes and representation tiles, with independent semantic indexes. Existing map tiles provide context; elevation, imagery, building footprints/heights, detailed interiors, routes and authored sites are distinct layers. A coarse basemap cannot supply terrain heights or a metric interior. Public Earth context can coexist with private homes and fictional overlays under separate grants.

Start with an offline coarse world package, a city subset and explicit high-detail place packages. A package records source/revision/capture date, CRS, spatial/time coverage, achieved resolution/error, completeness, rights, required transforms/fonts/styles and dependencies. Placeholders can indicate missing content; generated approximations are labeled authored/estimated and never presented as observations.

Reuse PMTiles/local archives where they fit; [PMTiles supports indexed tile pyramids and range access](https://docs.protomaps.com/pmtiles/). A source's permitted retrieval/export and attribution must be checked before city extraction; do not bulk-scrape a tile service simply because its endpoint is public. Source/dataset licenses travel with cached/exported subsets.

Offline city navigation includes map browsing, local place search/bookmarks and saved routes as the first concrete subset. If turn-by-turn/walking route solving is selected, include a routable graph, turn/access restrictions, elevation/accessibility where available and a local routing adapter; map tile residency alone does not provide routing. Show graph freshness and unsupported paths. GPS/location availability is independent of Wi-Fi and map download state.

### Independent user controls

The native region/layer inspector exposes: desired screen/shape error; semantic layers; active physics/model fidelity; update cadence; disk/RAM/GPU budget; offline pins; and network mode. It previews estimated bytes/coverage, then reports actual achieved detail. A user can pin a building interior at high detail, a city at medium detail and Earth at coarse detail without loading all intervening scales.

Use geographic/semantic selection, not just a radius: a neighborhood polygon, a route corridor, a set of rooms, a material sample or an assembly's dependencies. A simulation's offline closure includes boundary/material/solver inputs, not merely the visible camera frustum. File absence, access denied and incomplete derived data have different internal statuses with disclosure-safe public messages.

### Runtime selection and scheduling

Spatial indexes use the appropriate representation: geographic tiling for Earth context; bounded Cartesian BVHs/grids/trees inside a site; molecular neighbor lists for atomistic computations; solver partitions for physics. They share bounds/query envelopes, not one universal octree. Store only occupied/declared nodes; traversing a huge empty universe should cost according to occupied hierarchy and visible work, not total addressable volume.

For perspective rendering, a candidate projected error is `e_px ≈ e_world H / (2 d tan(fov_y/2))`, with conservative camera-to-bound distance and a safe near-plane policy. Orthographic views use their world-units-per-pixel scale. Source uncertainty and sampling resolution remain separate from this approximation. Screen error controls visible representation, not scientific accuracy. [3D Tiles defines geometric error and hierarchical refinement](https://www.ogc.org/standards/3dtiles/); support a declared subset rather than claim conformance from a similar tree.

- Coarse parents remain resident until required children are usable. Replacement versus additive refinement is explicit per node; avoid double surfaces. Tile borders share topology/elevation conventions or tested stitching. Use hysteresis and bounded transition times to prevent rapid toggling.
- Prioritize direct edits/selection, physical support around active bodies, visible coarse fallback and then useful refinement. Fair queues prevent a texture decode, microscopic job or huge bake starving input. Cap requested bytes, decoded bytes, worker count, upload bytes/frame, labels, splats and simulation islands separately.
- Work keys combine object/input revision, purpose and parameters. Coalesce identical permitted requests; propagate demand/cancellation to workers and upstream downloads. A noncancellable library can finish within a bounded worker or be terminated; its stale result still cannot publish.
- Static definitions share buffers and instances. Only active editable/interactive/semantic occurrences receive ECS objects. Picking and searchable semantic indexes do not require an entity for every city feature. Large labels/markers aggregate by scale.
- Render precision and depth precision are different. Test reverse depth/near planes and separate scale-appropriate passes/viewports when tiny and astronomical content coexist. A precise CPU coordinate does not make a distant microscopic triangle visible or remove z-fighting.
- Cache keys include content, schema/tool version, detail recipe, units/frame/registration, material conditions and validity. Authorization is checked independently on each read. Policy-scoped projections cannot reuse a private aggregate from another scope.
- Eviction respects pins, active solver/contact requirements and unsaved edits. Runtime residency is separate from offline disk residency. Releasing a view releases GPU/CPU resources and stream demand after shared users finish; do not destroy a referenced source definition.

## 10. Motion, flexible bodies and scientific execution

### Interactive dynamics

Reuse existing gravity, terrain support and cooling for ordinary unmounted Sands. Extend through explicit body modes: layout-only, static, kinematic, rigid and supported deformable. Layout attraction is an interaction rule; scientific forces have physical units and a named model. Default UI mass/drag can be practical without being reported as measured material physics.

Local rigid bodies have collision shapes, mass/inertia source, pose/velocity, contact parameters, joints, integration/substep settings and sleep/wake thresholds. Fixed-step simulation is decoupled from render frequency; cap catch-up and show throttled time rather than silently taking an unstable giant step. Fast/thin geometry needs a tested continuous-collision or substep policy. Failed collision generation leaves the previous support in place.

A dynamic terrain/collider change, altered gravity, force region movement, material/contact change or new joint can wake affected bodies. Cooling uses settled motion/force thresholds and a dwell period; it cannot erase external force or terminate ongoing thermal/chemical evolution. “Time castle” behavior can be reused where its actual semantics fit, rather than adding another independent decay scheduler.

For distant/unloaded regions, select frozen, analytic, reduced-order or recompute-on-activation policy. Each declares carried state and supported time range. Crossing domain boundaries needs handoff of identity, pose, velocity and appropriate conserved state at a known instant; never simulate the same body in two domains. First implementation can stop at a boundary with a visible unsupported handoff instead of a silent teleport. Large-world gravity is not automatically Newtonian N-body gravity; it is a selected domain model.

### Solver job contract

A physical job has immutable inputs, adapter/build/profile, selected supports/materials, physical equations, boundary/initial conditions, nondimensionalization/unit maps, mesh/discretization, time/solver settings, stopping criteria and limits. Its output contains status, actual covered time/space, residuals/convergence, balance diagnostics, uncertainties, result fields, checkpoint compatibility and logs.

Use explicit states: queued, preparing, running, converged/completed, incomplete, failed, cancelled and superseded. Numerical convergence, successful process exit, validation against reality and accepted operational application are separate facts. A simulation result is invalid for the current model when an affecting revision changed, even if its bytes are intact.

Run scientific kernels through bounded adapters. Control read-only inputs, scratch directory, memory/CPU/GPU/time/output quotas and cancellation; default-deny network/live credentials. A separate process is not alone a sandbox: use the platform's actual restrictions or label a worker trusted with its capabilities visible. An FMU/native plugin is executable code, not an inert geometry file. Installed adapters and their licenses are resolved explicitly; importing a document never downloads/executes an arbitrary solver.

Checkpoints pin solver/binary/state format and supported restart conditions. Replay may need exact stored inputs and outputs when the original solver is unavailable; distinguish inspectable results, restartable runs and fully reproducible jobs. Remote/HPC jobs can later implement the same protocol with explicit transfer/credentials/cost and cancellation outcomes.

### Continuum/discretization requirements

Structural studies need reference/deformed configurations, material regions/orientation, contacts, load cases, supports and beam/shell/solid idealization. Thermal studies need conductivity/capacity/density, interfaces, sources and boundaries. Mesh generation preserves named regions and boundary IDs, checks quality/orientation/connectivity and reports defects. [Gmsh describes physical groups and meshing contracts](https://gmsh.info/doc/texinfo/); it is a candidate adapter rather than a demand to embed every mesher.

Compare mesh/time refinement against quantities of interest, not just an attractive stress heatmap. Check rigid-body modes, singular supports, near-incompressibility/locking where relevant, linear/nonlinear applicability, and energy/mass balance within declared tolerance. Flexible interactive previews may use approximate methods, but a fast spring mesh is not automatically a validated engineering solid. [NIST's verification/validation guidance](https://www.nist.gov/publications/summary-industrial-verification-validation-and-uncertainty-quantification-procedures) informs the evidence discipline; each domain still needs its own tests.

### Coupling and scale/fidelity changes

Start with one-way mappings: a material derivation supplies a constitutive model; a thermal result supplies a specified temperature field to a structural run. Two-way coupling is a separate capability with exchange variables, signs, interface maps, clock synchronization, subcycling, convergence criteria, rollback/checkpoints and failure policy.

Intensive values such as temperature need suitable interpolation; integrated forces/heat/mass transfers need appropriate conservation. Pressure maps must account for area/normals before claiming conserved resultant force. Compare the coupled balances and interface work, not only each solver's local residual. [preCICE documents consistent/conservative mappings](https://precice.org/configuration-mapping) and [explicit/implicit coupling](https://precice.org/configuration-coupling). [FMI defines exchange interfaces, but not the master co-simulation algorithm](https://fmi-standard.org/docs/3.0.1/); adopting it does not solve stability or orchestration.

State projection when changing mesh/model includes conserved quantities, internal variables and initialization error. A display LOD change needs no physical projection. A solver refinement/coarsening does. If no validated transfer exists, restart a new run from a declared base; do not fabricate lost history. Reduced-order/surrogate models require training/calibration provenance, validity bounds and an out-of-domain policy just like other material models.


## 11. Implementation boundaries and dependency decisions

Proposed Rust ownership: introduce a small `crates/spacematter` model/evaluation library for K1–K8 contracts, exact descriptors, supported numerical buffers and pure validation/query code. It may depend on the existing pure `nucleus` types/hash/quantity helpers, but never the reverse. It must not depend on Bevy, a database, network access or a heavyweight scientific kernel. Start with the types used by the first two fixtures; do not implement every profile speculatively.

- `nucleus` defines only the small inert model-reference/binding DTOs needed by existing canvas/Record contracts, using its own ID/hash/quantity conventions. The model library validates/resolves them. Keep specialist schema/evaluation in `spacematter`; nucleus never imports that crate, so the dependency remains acyclic. Preserve numeric-domain boundaries instead of copying the accounting type system into scientific arrays.
- `engine` coordinates authorized mutations, expected revisions, receipts, publication and jobs. `store` persists roots, metadata, bindings, indexes and job state using existing asset/blob facilities where sufficient. Large arrays stay outside frequently synchronized Record rows. Schema changes use new append-only migrations and the repository's checksum/guard procedure.
- `crates/interface/src/native/` owns Bevy rendering, input, native tools, inspection and adapters from current canvas/Room/Areas. The core never exposes Bevy entity IDs as durable references. `crates/map` remains a provider of geographic sources and scene context, not the owner of every model.
- Physical runtime/job orchestration can reuse verified simulation infrastructure. Separate scientific/domain workers carry OCCT, IFC, molecular or continuum dependencies and capabilities. Add worker crates/features only when selected fixtures need them; default map viewing must not compile or initialize every solver.
- Shared bounded asset/job services handle content storage, cancellation, demand, budgets and policy. Domain adapters supply typed payload validation, algorithms and capability declarations. Do not build a second unbounded cache/job system inside each Castle.
- Pins/limits are checked against the current workspace before changing dependencies. Use cargo check, required tests and cargo xtask/Nix workflows; warnings are errors. Embedded dependency licenses/credits are included in the relevant Sand. The old web frontend is not modified.

### Adapter conformance record

Every selected adapter records: supported schema/operations; exact software/build/license/target platforms; units/frames/conventions; source and subelement identity; unsupported/loss behavior; deterministic/tolerance reproducibility; resource/cancellation guarantees; failure and crash recovery; and reference fixtures/results. “Library supports X” is not a completed Lince capability until persistence, native interaction and this record pass.

Choose one implementation per initial purpose. A renderer, geometric kernel, mesher and numerical solver solve different problems; selecting all of them at once is unnecessary. Time-box a candidate evaluation to the fixture list and make a recorded selection or fallback decision. A failed optional dependency must not invalidate a saved document or prevent inspecting retained results.

## 12. One sequential implementation plan

Tasks are ordered. Domain details T1–T6 and F1–F10 below expand these stages, not separate implementation schedules. Each stage implements its backend contract, native user flow, persistence and meaningful proof before its checkbox is complete. The long-term stages are retained scope, not a promise to deliver every science at once. New scientific capabilities need their own completed row; providing an adapter slot never checks them off.

### S0. Freeze fixtures, budgets and the initial contract

- [ ] Re-audit the baseline paths and record actual tests/behavior, current canvas unit mapping, asset delivery, accepted revisions, permissions and simulation isolation. Identify reuse versus missing contracts before adding services.
- [ ] Select a real city subset, a measured or synthetic room/cave and initial material sources. Use the default fixture/budget table below until the owner supplies their region/device. Never require private home data to run the test suite.
- [ ] Approve the K1–K8 meanings, directory-format minimum, initial scalar/ID/frame encodings and native edit/import/save/compare flow. Make the first fixtures tiny enough to inspect by hand.

Gate: executable fixture specifications and a written implementation scope, not a months-long schema project. Unknown data rights, unit conversions or worker trust are recorded decisions; they do not become silent defaults.

### S1. Durable objects, typed edits and a minimal native document

- [ ] Implement definition revisions, stable instance slots/occurrence handles, compact element IDs and optional Record bindings for a tiny repeated scene. Extend existing canonical/asset contracts instead of duplicating identity.
- [ ] Implement directory save/load, required-profile/closure validation, atomic root publication/recovery and resource bounds. Expose create/open/save, tree/inspector and a revision-conflict flow in native UI.
- [ ] Bridge one existing canvas placement and Room into a model handle; preserve normal selection/undo. Test save failure, crash recovery, repeated definitions, cycle rejection and stale edit results.

Gate: a repeated part survives reorder, undo, reload and two competing edit bases with stable identities. A corrupt or incomplete new snapshot cannot destroy the last valid saved root.

### S2. Coordinate precision, view state and the canvas bridge

- [ ] Implement frame/unit descriptors, high-precision cell/residual placement, relative transforms and explicit canvas-to-model unit calibration. Persist magnification separately from physical placement.
- [ ] Spike `big_space` against the pinned Bevy/Avian setup with origin shifts, nested moving frames and two distant close-up views. Select it or record a bounded replacement using the same durable contract.
- [ ] Add perspective and orthographic views onto the same model/selection and keep existing flat content usable. Prove that render-origin shifts never change canonical pose, physical units, velocities or physical-workstation calibration.

Gate: a local millimeter edit and a microscopic magnified inspection remain numerically stable at distant roots. No giant root f64 coordinate is used as the source of truth.

### S3. Spatial queries, named selections, mounts and Areas

- [ ] Implement capability-based surface queries and stable named selections/correspondence for planes and meshes; extend picking to carry occurrence/revision/frame. Complete the query/identity part of T1.
- [ ] Implement T4–T6 using shared region queries, independent 3D targets, existing immunity and cursor/camera/item bindings. Keep source/target membership, falloff and explicit sampling policy consistent.
- [ ] Connect content mounts to verified local surface frames with unresolved/repair behavior. Add native region/target/mount inspectors, undo and persistence. Test rotated Rooms, multiple surfaces, stale picks, deleted anchors and all immunity cases.

Gate: a sphere can influence above/below; a wall patch addresses its own surface; moving an off-center target leaves the region unchanged; mounting on a removed face never silently selects another face.

### S4. Offline Earth context, city and bounded streaming

- [ ] Reuse existing map/archive download and storage controls; add source/coverage descriptors, a coarse offline world and selected city profile. Keep elevation, imagery, routes and authored places separate.
- [ ] Add region/layer budgets, cost preview, offline pins, partial coverage, progress/cancel/resume and explicit no-network operation. Include styles/labels/transforms required to render the chosen package.
- [ ] Implement paged spatial/semantic indexes, coarse-parent fallback, permitted dedupe and bounded decode/upload/request queues. Add local place search/bookmarks and saved-route inspection; route solving remains a separately tested subset when selected.
- [ ] Add the portable `.spm` import/export slice, selected-profile dependency closure, attribution/loss reports, canonical vectors and cargo xtask inspection/validation. Prove directory/packed equivalence, partial transfer/resume and B15 hostile-file rejection before range-streaming the packed form.
- [ ] Prove 30-minute navigation under the default working-set cap, rapid request cancellation, offline closure, missing-tile fallback, disk-full behavior and resource release. Do not claim building/terrain detail from current normalized basemap tiles.

Gate: an offline city and a high-detail place can coexist without loading the city at full resolution or making automatic network requests. Sparse extent alone does not increase resident cost.

### S5. Native room authoring and useful content

- [ ] Implement the first geometry operations in T1–T2: analytic planar patches and polygon mesh source, vertex/edge/face selection, numeric/snapped move, extrusion, opening and edit preview/cancel/commit.
- [ ] Add purposeful flat/sharp/smooth appearance, readable content placement and native Room/building-element presets. Keep measurement source and render triangulation distinct.
- [ ] Mount Sands, Record views and supported assigned content/windows through existing lifecycle/input/output paths; preserve explicit gravity/body policy for unmounted content. Verify save/reload and moving the containing space.

Gate: author the room fixture with a passable opening and useful wall content; orthographic/perspective/table selection identifies the same object. Virtual movement leaves calibrated physical outputs unchanged.

### S6. Caves, selective low-poly optimization and derived collision

- [ ] Build the cave with floor/ceiling/overhangs using bounded connected mesh editing; add selected implicit-volume operations only when the fixture needs topology-changing sculpting.
- [ ] Implement T3's preview/simplify/bake workflow with counts, dimensional error, protected boundaries/passages/mounts, source retention, correspondence and cancel/undo. Distinguish estimated versus bounded error.
- [ ] Produce purpose-specific render/collision proxies, cache only affected regions and implement step-boundary collider replacement/wake policy. Preserve additive live terrain effects and explicit stale/rebake state.

Gate: room and cave remain attractive, navigable and mounted correctly after simplification and reload. New geometry cannot remove physical support before its collider is valid. Record measured errors/costs on the named hardware.

### S7. Imported bases, authored layers, alignment and comparison

- [ ] Implement pinned source/base layers, sparse typed overrides, hide/replace versus additive composition, independent instance runtime and edit-target inspection. Support overlay and side-by-side views with shared or independent cameras.
- [ ] Add manual measured registration from declared frames/control points with known scale, residuals and withheld checks; allow explicit estimated alignment. Store model-to-physical association separately from projector/camera calibration.
- [ ] Implement source re-import three-way conflict/mapping review, split/deletion repair and preserved last poses. Test that a source rename/reorder does not erase authored work or Record bindings.

Gate: compare a customization at declared 1:1 physical scale to its unchanged base, inspect alignment error, move the virtual model and re-import without silently rewriting the installation or accepted geometry.

### S8. Materials, conditions and an offline knowledge pack

- [ ] Implement material specification/sample/batch/assignment/state distinctions, region/layer/interface references and typed property candidates. Add the native material/conditions/evidence inspector and explicit invented-model authoring.
- [ ] Import a small rights-reviewed pack with source IDs/revisions, exact unit conversions, methodology, applicability and unknown/conflict states. Keep visual appearance and physical properties separately editable.
- [ ] Implement the first linear-elastic/conduction model descriptors and parameter validation. Test unsupported conditions, conflicting evidence, anisotropic orientation, bad units, missing density and multi-material parts without yet claiming a numerical solver.

Gate: an assigned material can answer which supported property/model applies, why, and what is missing. A saved/exported pack works offline with attribution and cannot invent absent properties.

### S9. Molecule and crystal editing

- [ ] Implement finite-molecule and periodic-site profiles with compact stable IDs, species/isotope/charge fields, bonds/conformers and lattice/symmetry/occupancy as applicable. Add the declared SDF/CIF subsets and loss/unsupported reports.
- [ ] Add native point/bond/site/cell edits, numeric controls, supercell preview, validation diagnostics, undo and export. Keep display magnification distinct from physical geometry and periodic image identity distinct from source sites.
- [ ] Test atom/bond reorder, mirrored/stereochemical cases supported by the adapter, partial occupancies, nonorthogonal cells, duplicate symmetry expansion, invalid cells and stale material derivations. Add PDBx/mmCIF as a separately bounded later profile for macromolecules.

Gate: a molecule and a crystal are authored/imported/saved/exported without silently losing their supported structure. Invalid or unsupported chemistry remains an explicit draft, never a claimed parameterized simulation.

### S10. Microscopic structure to reusable material behavior

- [ ] Implement ideal crystal density from declared cell contents/occupancies and volume, counting symmetry-expanded sites exactly once. Distinguish ideal theoretical density from measured porous/bulk density and reject missing masses/cell conditions.
- [ ] Implement crystal tensor orientation and the documented Voigt/Reuss/Hill effective-property bridge with units, notation, assumptions and evidence. Use isotropic synthetic fixtures and one rights-reviewed real dataset; compare independent reference outputs.
- [ ] Provide a native derivation inspector linking structure → method/conditions → candidate model → assigned part. Editing the source invalidates outputs; users explicitly choose recomputation, another candidate or a labeled approximation.

Gate: one microscopic description supplies a quantitatively checked material property without an atomistic runtime in every part. The UI accurately separates geometric derivation, imported computed evidence, homogenization and fresh physical prediction.

### S11. Dimensioned sketches and exact CAD solids

- [ ] Run the recorded OCCT/Rust-kernel bake-off; choose one buildable, licensed adapter with process-failure/cancellation behavior. Keep its dependencies outside the common viewer.
- [ ] Implement sketch constraints/DOF diagnostics and the initial extrude/revolve/hole/Boolean feature set, native parameter/feature tools, dimension units, failed-feature recovery and incremental recompute.
- [ ] Add stable semantic feature/face references plus ambiguous remapping repair, material regions and tessellation. Prove the named STEP/render exchange subset and retain original source/feature data with honest loss reports.

Gate: a dimensioned bracket with a hole can be modified, measured and instanced without breaking its mounting intent. A failed Boolean, kernel crash or renamed face cannot replace a valid accepted solid with corrupt output.

### S12. Assemblies, building systems, drawings and fabrication

- [ ] Extend CAD operations in bounded sets: sweep/loft, fillet/chamfer, shell/offset and patterns, each with a failure/precision fixture. Add assembly mates/joint ports, constraints, interference and clearance tools.
- [ ] Build native wall/slab/roof/opening/furniture and system presets from the same definitions/materials/relations. Add plans/sections/elevations, associative dimensions/tolerances and revisioned takeoff/drawings.
- [ ] Add a declared IFC/BCF import/issue subset with source identities, units, spaces, layers and openings. Support cut lists, process/material requirements and an initial manufacturing export/postprocessor only for a selected real workflow.

Gate: reuse one multi-material assembly as a free part and a building element; drawings/takeoff refer to the same accepted revision, and an IFC re-import preserves supported identity or asks for repair. Fabrication data remains separate from equipment execution.

### S13. Interactive rigid motion, joints and altered local physics

- [ ] Extend existing Avian-based interaction with explicit body modes, mass/inertia provenance, contact parameters, joints/limits, fixed-step/substep controls and pause/reset/step. Keep render/canonical/solver transforms independently owned.
- [ ] Reuse Rules, Signals, Areas and bindings for scoped behavior; add named gravity/force model overrides with stored units/assumptions. Preserve ordinary Sand cooling and wake on relevant support/force changes.
- [ ] Implement declared sleep/unload policies and domain boundaries; independent clocks use isolated runtime state, not collision masks alone. Add a second comparison domain only after the first passes the bounded resource fixture.

Gate: a hinged/rolling assembly and loose/mounted Sands behave predictably in a room/cave, survive collider edits, and can be compared under different force settings without shared velocities or altered operational state.

### S14. Verified continuum studies and flexible bodies

- [ ] Choose one mesher/solver adapter for the stated linear-elastic and conduction subsets. Implement job inputs/results/statuses, named boundary/material maps, unit conversion, cancellation/quotas/checkpoints and retained logs.
- [ ] Provide native setup and result views with explicit loads, supports, conditions, scalar/tensor legends, convergence and stale/invalid coverage. Add deformable playback/preview with mappings to source and independently generated render geometry.
- [ ] Verify a patch/uniaxial case, slender cantilever in its applicable regime and an analytic slab heat-flow case; perform mesh/time refinement and balance checks. Keep approximate interactive flexibility labeled separately from these checked continuum models.

Gate: repeat a bounded study offline from captured inputs and obtain the declared quantitative tolerance; incomplete, singular, divergent, unsupported or stale runs cannot appear as passed current results.

### S15. AECO production, stock, procurement, logistics and civic work

- [ ] Implement/verify F1–F4 and shared F6 query/publication contracts through supplier → stock → production → partial delivery, with traceability, reservations, accepted quantities, evidence and native forms/tables.
- [ ] Add F5 bounded consequence calculations and F7 source reconciliation/command policy for the workflow. Prove disconnect/reconnect, duplicates, late events, competing authority, missing/private inputs and scenario isolation.
- [ ] Reuse F8–F10 for a site/civic workflow: place-linked observations/issues, proposal/review/publication, maintenance/accessibility and selective public offers. Model lifecycle changes and operational completion use their own explicit acceptance policy.
- [ ] Apply geometry/selection/material/port contracts to one civil corridor/utility or geotechnical workflow, with revisioned quantities and declared survey/model error. Retain unsupported civil/manufacturing profiles as named deferred rows with required operators/data/fixtures; they cannot be marked delivered by the room preset.

Gate: factory and civic flows use the same backend invariants, work without detailed 3D assets and do not double-account resources or turn an observation/simulation into an accepted transaction.

### S16. Full globe, large extents and multiple scale-appropriate views

- [ ] Extend the bounded site/geographic bridge to globe curvature/terrain seams, vertical references, dateline/poles and versioned offline transformation resources. Validate against reference control points and declared local error.
- [ ] Add sparse solar-system/astronomical frame fixtures with supported time conversions and optional versioned ephemerides. Prove a tiny local inspection at remote coordinates and independent view origins without flattening all content into one vector.
- [ ] Exercise sparse empty extents, nested moving frames, geometry/depth precision and simultaneous CPU/GPU/physics budgets. Expose what is addressable versus populated, observed versus invented and physically supported versus only visualized.

Gate: the architecture can address observable-universe-sized extents while retaining bounded local precision and working set. This gate does not claim populated astronomical content, relativistic correctness or universal physics.

### S17. Further physical models and coupled studies

Implement the following rows in this order unless the owner explicitly reprioritizes them. Each is a separate feature slice with native authoring/setup, typed inputs, offline dependencies, solver/result adapter and reference validation. None is complete merely because S14's protocol could host it.

- [ ] Molecular dynamics/minimization and one fresh microscopic property study: full parameter coverage, ensemble/step/sampling/convergence evidence and comparison to a known reference. Add electronic structure or reaction/folding methods as distinct supported calculations, not side effects of a molecule edit.
- [ ] Nonlinear/flexible constitutive models and failure: select hyperelastic/viscoelastic/plastic/contact/fracture subsets individually, preserve history variables and validate transfer/restart/energy behavior.
- [ ] Further CAD/manufacturing and civil authoring profiles: sheet-metal unfold/refold with bend assumptions, composite/lattice generation, manufacturing drawing/tolerance exchange and corridor/terrain/utility/geotechnical tools. Select each bounded operation set and fixture; reuse source/history/material/selection contracts and keep process/toolpath execution separate.
- [ ] Thermal-structural one-way then two-way coupling: interface maps, conservation, substeps, convergence, checkpoint/rollback and a coupled reference case. Add fluids/acoustics/geotechnical domains with their own model cards and fixtures.
- [ ] Electronic/circuit and device-material studies: compact-model/netlist workflow first, electrothermal mapping next, specialist field/device/fabrication models only with declared inputs and benchmarks.
- [ ] Chemical/multiphase and living-material/tree studies: separate kinetics/equilibrium/transport from procedural geometry and physiological growth; include calibrated parameters and conservation/validity checks.
- [ ] Nuclear-power-related transport/depletion/heat/thermal-hydraulic and later supported dynamic coupling: evaluated isotope data coverage, source provenance, statistical/convergence diagnostics and reference cases for each claimed scope.
- [ ] Supported astronomical/dynamical/cosmological studies beyond coordinate visualization: named physical models, time/frame/coordinate conventions, reference comparisons and bounded local/aggregate fidelity.

Gate per row: a genuine domain example shares K1–K8 and persistence/native tools, has its own evidence and limitations, and cannot silently extrapolate beyond its supported model. Keep unbuilt models explicitly deferred in this document.

### S18. Composed connected infrastructure

- [ ] Assemble independently governed site/city layers, F1–F10 operations, observations, queries, scoped behaviors and specialist results into one useful infrastructure workspace. Verify partial coverage, source ages, denied fields, disconnected work and independent authorities.
- [ ] Prove domain/asset/Record reuse in at least two distinct workflows without duplicated invariants. Tighten or simplify the common primitives from that evidence before widening a profile.

Gate: people can inspect local accepted state, model a proposed physical change, compare a scenario and submit a reviewable proposal without a giant trusted world model or hidden live side effects. The personal offline modeling experience remains useful independently.

## 13. Fixtures, budgets and evidence

### Shared fixture set

These are synthetic/public-test proposals; a real personal place is an additional acceptance fixture, not a prerequisite for reproducible tests.

| ID | Fixture | Proof required |
| --- | --- | --- |
| B01 | A reused part with two instance slots, one local override and a Record binding | Stable identity under reorder, fork/update review, conflict, undo and save/reload |
| B02 | Coarse world, a city index and a pinned detailed site, with one missing tile | Offline closure, bounded active set, coarse fallback and meaningful coverage |
| B03 | A 5 m × 4 m × 2.7 m room with a 0.9 m × 2.1 m opening and wall-mounted content | Dimensions, support/collision, local mounts, flat/perspective interaction and saved calibration separation |
| B04 | A cave with two surfaces at the same horizontal location, a ceiling, overhang and narrow passage | Surface queries, no height-only assumption, protected passage under simplification |
| B05 | A circle/sphere with an independent off-center/high target, rotated frame and moving anchor | Region/target separation, above/below membership, all immunity modes, correct binding and undo |
| B06 | A measured/imported room with authored opening and at least one withheld registration check point | Alignment residual/error, source age, re-import split/deletion conflicts and 1:1 comparison |
| B07 | Two conflicting property candidates plus a layered part and a missing conductivity | Conditions/units/orientation, no invented zero, evidence and explicit approximation |
| B08 | A finite molecule and a nonorthogonal periodic crystal with occupancy/symmetry | Stable site identity, format subset/loss reporting, density and stale derivation |
| B09 | Synthetic isotropic tensor and one documented crystal dataset | Tensor rotation/notation, bounds/estimates and independently checked derived values |
| B10 | Dimensioned bracket, repeated assembly and an intentionally failing feature | Constraints/DOF, B-rep validity, face repair, STEP loss report and worker recovery |
| B11 | Loose Sands, a hinged assembly and altered gravity in two comparison domains | Fixed stepping, sleep/wake, support edit, state isolation and unload policy |
| B12 | Elastic patch/cantilever and conduction slab | Units, analytical comparison, convergence, balanced quantities and failed-run visibility |
| B13 | Sparse astronomical parent with two distant tiny inspection views | Cell/frame error, independent origins, no canonical drift or extent-sized allocation |
| B14 | Supplier/stock/production/partial delivery and a civic maintenance proposal | Exact accounting, rights, reconciliation, selective publication and scenario isolation |
| B15 | Truncated/hostile pack, duplicate entries, oversized decode, cyclic references and disk exhaustion | Bounded rejection, intact previous root, no path escape/network/credential execution |

Proposed baseline profile: Linux desktop/laptop with four CPU cores, 16 GiB system RAM, integrated/shared-memory graphics, 1280×720 model viewport and SSD. It is a test profile, not an assumption about the owner's hardware. Stage S0 records the actual device, graphics driver, build/settings and a baseline before changing anything. Add a lower-memory profile only after measuring the first one.

| Budget/target | Proposed starting value | Enforcement/evidence |
| --- | --- | --- |
| Offline demonstration disk | 8 GiB selected/pinned source content, separately capped 2 GiB regenerable cache | Coarse world/city/site distribution is chosen from actual byte estimates; reduce selected coverage/detail rather than promise a fixed city fits |
| Model CPU residency | 1 GiB incremental model/view assets and indexes, excluding explicitly launched scientific workers | Measure decoded data plus job/staging peaks; report total application memory separately |
| Model GPU residency | 512 MiB managed model resources | On integrated graphics this also consumes shared RAM; budget both views together, not once per view |
| Visible frame latency | p95 ≤33.3 ms, p99 ≤50 ms at the baseline viewport | Target for the documented navigation fixture, not a universal performance claim; record CPU/GPU timing and visual settings |
| Direct interaction | p95 ≤100 ms visible response; ≤200 ms for small validated commits, excluding declared background computation | Preview and commit measured separately; queue/start/finish status for expensive work |
| First useful view | ≤3 s cold local open, ≤1 s warm after the required root/index is available | Display useful coarse content without waiting for optional refinements; record asset and OS-cache conditions |
| Job scheduling | Initially two heavy background jobs, one expensive decode per source, bounded uploads per frame | Byte/upload/tile caps derive from the fixture; preserve input responsiveness and fair queues |
| Cancellation | Local UI acknowledgement ≤100 ms, obsolete dispatch stops ≤250 ms; cooperative workers yield/check ≤100 ms | Noncooperative kernels have a process timeout/termination policy, proposed 2 s; pending termination is visible |
| Physics | One active local domain first; at most two for the isolation proof | Cap bodies/contacts/substeps from measured fixture; unsupported overload pauses/throttles rather than changing physical laws |
| Scientific worker default | Two CPU cores, 2 GiB RAM, explicit wall-time/output cap per selected study | User may change these per run; GPU/remote allocations are explicit and separately metered |

These limits are falsifiable starting targets. If the actual baseline cannot meet them, record the bottleneck and revise the profile, representation or target before claiming completion. Asset count alone is not a performance result. A candidate stress run uses 10,000 operational Records, a 100,000-feature spatial index and a 30-minute navigate/disconnect/reconnect session; only a bounded nearby subset is rendered.

Error targets are task-specific: proposed room editable measurements within 1 mm of authored synthetic dimensions; default room simplification maximum requested deviation 5 mm with the achieved metric labeled; microscopic round-trip absolute/relative error relative to declared input precision; geographic alignment within the selected source/transform accuracy rather than an invented universal centimeter promise. Scientific tolerances are set for quantities of interest before running benchmarks. For initial simple analytic elasticity/conduction cases, propose ≤1% relative error after convergence away from mathematical singularities and separate tighter unit/identity/conservation checks where justified.

### Verification obligations

Correctness combines geometry/topology validity, stable identity, units/frame/clock transformations, material applicability, mapping, exact accounting, revision conflicts and actual user interactions. Use property/metamorphic tests where meaningful: unit changes preserve physical predictions; rigid frame changes preserve lengths; view magnification does not change physics; permutation preserves identity; equal/opposite additive effects cancel; display simplification does not change accepted mass/stock; origin shifts preserve canonical state.

Scientific evidence distinguishes code verification, numerical convergence, parameter calibration, validation against measurements and uncertainty. A successful solver exit satisfies none of the others automatically. Capture reference inputs/expected results, tolerances, build/platform/settings, observed outputs and applicability. Avoid calling an unvalidated approximation “physical truth.”

The first quantitative checks have explicit reference problems: a uniform linear-elastic bar in axial load gives elongation `ΔL=FL/(EA)` with free lateral boundaries and the specified support; a sufficiently slender, small-deflection cantilever compares tip response to `δ=FL³/(3EI)` within that beam idealization; a homogeneous conduction slab with insulated sides and fixed face temperatures gives `|q|=k|ΔT|/L`. Keep force versus traction and heat flux versus total power distinct (`P=|q|A` for a uniform face). Exclude singular point-load/clamp neighborhoods from a false pointwise stress convergence claim. Use an isotropic tensor and proper rigid rotations to verify constitutive invariance before testing anisotropy.

Security checks target actual boundaries: denied edits across UI/API/import/automation; private metadata/index/result disclosure; stale caches after grants change; source/observation forgery; bounded parsers/archives/decoders; integer overflow in shape/offset math; traversal/symlink/duplicate-entry handling; cyclic expansion; unintended network requests; credential scope; executable worker capabilities; cancellation and quota exhaustion. Retained caches/results respect their own rights. Simulations cannot emit operational Actions into the live Cell.

Performance separates steady-state frame time, input/commit latency, streaming/decode/upload, queries, save/recovery and solver throughput. Record p95/p99, cancellation latency, CPU/RAM/VRAM, bytes read/transferred, peak staging and released demand/resources after closure. Test one dense room, a sparse city, many instances, overlapping appearance layers and a long navigation session separately.

Run only checks appropriate to changed contracts; retain reproducible cargo xtask/native replay fixtures and inspect rendered room/cave outputs where appearance matters. A headless math test does not establish low-poly visual quality or usable mounted input. Every stage leaves a fixture/result and any blocked or unsupported scope in this plan.

## 14. Decisions still requiring owner or implementation evidence

The architectural defaults above are concrete proposals. Remaining choices should not reopen the agreed ambition or turn into a general “which features do you want?” questionnaire.

1. Which real city/site and target device/storage profile should replace B02/B03 and the proposed budgets? Until answered, use public/synthetic fixtures and label the profile provisional.
2. Confirm the first material bridge/study or name a more useful one. The recommendation is crystal density/tensor lineage, then a common-material elastic/thermal part; molecule editing remains included regardless of that choice.
3. Select actual datasets, adapters and licenses through the specified bounded evaluations. Each unselected library has a deadline at the stage where it is needed and a fallback preserving source data/inspectable results.
4. Before external operational execution, select field authority, disclosure, command scopes and reconciliation. Offline local modeling needs none of those remote execution grants. Globally exclusive offline reservations require an authority design rather than hopeful conflict merging.

## 15. Detailed topology and operational specifications

The following retained catalogues are normative domain detail. They use the architecture above; stage references point into the single sequence. They preserve the original surface, influence, AECO, production, stock, logistics, public-commerce and civic requirements. Where an old high-level phrase admits several interpretations, K1–K8 and the stage gate provide the precise contract.


## Detailed topology requirements

These remaining requirements expand stages S3, S5 and S6. The existing height-based floor, mathematical effects, generic bindings, gravity and native Floor editor are the starting point.

### Topology experience

Customize the canvas as a useful, attractive low-poly 3D space: a model of a real room, a cave, or an invented environment. Floors, walls, ceilings and overhangs can hold Sands and other canvas content. A room should remain recognizable and pleasant with few polygons. Low-poly appearance is a deliberate visual style, not simply a coarse sampling grid.

Shape the space while editing, then explicitly simplify and bake a chosen version for efficient long-term use. Preserve the editable source so optimization can be undone or repeated with different settings. Moving through a finished space should not require rebuilding its geometry every frame.

This work extends the existing Room and physical-workstation foundations. Read [Shader-effects.md](Shader-effects.md) alongside this plan. Reuse the existing Room hierarchy, local poses, floor/wall/desk/computer roles, proxy surfaces, mounted content and window/output management where applicable. A virtual room model and a calibrated physical installation are related representations, with an explicit association between their surfaces.

### Topology constraints

- Keep independently editable mathematical effects and their existing anchors. An optimized static surface is another representation of a chosen shape, not a replacement for all live effects.
- Separate editable geometry, rendered geometry and collision geometry. They share a revision and surface identities but may use different levels of detail.
- Preserve silhouettes, intentional sharp edges, doorways, cave passages and mounting surfaces during simplification. Triangle count alone does not establish quality or efficiency.
- Prefer simple planar geometry for room walls and desks, and locally detailed meshes for caves. Avoid remeshing an entire room just to move a Sand or edit its appearance.
- Shader effects determine appearance; geometry and collision determine support and physical boundaries. A convincing shader does not establish a navigable cave or a calibrated real wall.
- Physical meters, Room-local coordinates, canvas coordinates and projector/camera coordinates retain their existing distinct meanings. Moving a virtual Room must not silently change a physical installation's calibration.

### T1. Surface queries, identity and bounded authoring

- [ ] Replace height-only assumptions with shared surface queries: ray intersections, closest surface, normals and physical support. Allow several surfaces at the same canvas position, including cave floors, ceilings and overhangs.
- [ ] Define stable surface identities within the existing Room model. Keep mounts attached to their intended surface when a Room moves; track source revisions when geometry changes.
- [ ] Implement analytic planar patches and polygon boundary meshes through the shared geometry contracts. First tools create a planar wall/floor, move vertices/edges/faces, split/connect, extrude and carve an opening, with numeric controls and snapping. Volumetric sculpting has a separate bounded workflow and cost proof; it is not required to move a Sand or create a wall.
- [ ] Specify interaction with live mathematical effects: which surfaces an effect deforms, how its bounds work in 3D, and what happens when an effect changes a previously baked surface. Recommended behavior: retain the bake until explicit rebuild, and clearly indicate that its source has changed.

Acceptance: a room with walls and an opening, and a cave with a ceiling, can be queried and collided with correctly. Existing hills and valleys continue to work. The first editing tools and effect-to-surface rules are agreed before coding them.

### T2. Model and furnish a low-poly room and cave

- [ ] Add native edit-mode controls for the agreed tools, with selection, numeric dimensions, snapping, preview, cancel, undo/redo and persistence.
- [ ] Provide flat shading and deliberate sharp/smooth edge control, with restrained materials and lighting. Review actual room and cave fixtures for silhouettes, readable content and attractive facets.
- [ ] Mount Sands, Records and supported window/content surfaces on walls, ceilings and other modeled faces using the existing Room-local placement path. Mounts are explicit attachments; ordinary unmounted Sands retain gravity.
- [ ] Define a stable face-relative mounting frame and an explicit surface offset. Topology edits that remove a mounting surface must report an unresolved attachment and preserve the content's last pose rather than silently reassigning it.
- [ ] Reuse physical-workstation surface assignments and dedicated output windows. Associate measured planar walls/desks with modeled surfaces without changing their calibration merely because the virtual model moves. Curved modeled surfaces do not imply curved physical projection support.

Acceptance: create a recognizable room and a cave, place useful content on their walls, save/reload, and move the Room without breaking local mounts. Verify the physical projection mapping separately through the existing calibrated surface path.

### T3. Simplify and bake a settled shape

- [ ] Add an explicit low-poly optimization tool with a preview, triangle budget and maximum shape-error control. Show before/after counts and protected features before applying.
- [ ] Protect openings, boundaries, sharp edges and mounting/calibration patches. Offer selective optimization of part of a space instead of forcing a whole-scene operation.
- [ ] Produce cached render meshes and suitable collision meshes, preserving stable surface associations or an explicit remapping. Reject or require adjustment when simplification exceeds the allowed error or closes an opening.
- [ ] Keep the editable source and bake settings. Support undo, save/reload and deliberate rebaking after source edits. A stale asynchronous bake must never overwrite a newer geometry revision.
- [ ] Perform optimization on bounded background jobs with cancellation. Reuse geometry while idle; release obsolete meshes and collider caches. Establish budgets for triangles, jobs and memory before implementation.

Acceptance: simplified room and cave fixtures remain attractive, retain passable openings and stable wall content, and have measured render/collision costs. Record hardware, triangle counts, memory, frame-time percentiles, bake duration and shape error. Choose numerical targets during refinement; do not claim efficiency from polygon counts alone.

### T4. Share Area region geometry across capabilities

- [ ] Separate an Area's drawn shape, affected region and force target/direction while reusing the existing InfluenceArea model.
- [ ] Support a selected surface patch, a plane with explicit thickness, an extruded shape and a sphere. Define boundary inclusion and the coordinate frame consistently, including rotated Rooms and camera-bound Areas.
- [ ] Use the same region query for attraction, filtering, sorting, mutations, sound and immunity, preserving each capability's existing semantics.

Acceptance: a sphere includes objects above and below its center; a wall patch addresses its selected surface rather than unrelated surfaces behind it; all capabilities agree about region membership.

### T5. Reuse and extend the independent attraction target

Existing source already provides `AttractionTarget::Point`, an offset from the Area center, and a native draggable target handle in `crates/interface/src/native/area_target.rs`. Do not rebuild the off-center 2D feature as a separate system. Verify its current discoverability and behavior first.

- [ ] Make the intended interaction clear: draw a circle, then position its attraction target near the top of the circle without moving or resizing the circle. The target may also be outside the affected region.
- [ ] Extend the existing target representation and controls with a third local coordinate, allowing a target above, below or beside the affected objects. Provide a draggable handle, numeric coordinates and Reset to center through the existing Area editor.
- [ ] Preserve the distinction between selection and destination: region membership selects which Sands receive a force; the independent point determines its direction. Moving the target must not alter the affected region or filtering.
- [ ] Resolve targets in the Area/Room frame and preserve existing attraction, repulsion, force falloff, immunity, bindings, persistence and undo behavior. Collision still prevents a Sand from passing through walls or ceilings.

Acceptance: a circular Area attracts toward a point near its top edge; moving that point leaves the circle unchanged. A sphere inside a cave can attract toward a higher point, with correct collisions and consistent behavior in both canvas views.

### T6. Reuse Area immunity for influence boundaries

- [ ] Reuse the existing None, External, Internal, All, Containment and Isolation immunity behavior with the new shared region queries. Expose it through existing Area controls rather than adding a parallel obstruction/permission system.
- [ ] Demonstrate a cave or room boundary that blocks incoming influence, retains outgoing influence, or isolates inside from outside according to the selected existing mode. Keep the source/target membership rules consistent across supported capabilities.

Immunity classifies the source and affected object by region membership; it does not currently trace a ray through a wall. Automatic line-of-sight or material-dependent sound blocking is deferred unless explicitly requested. This preserves the requested primitive reuse without claiming that immunity already implements geometric visibility.

Acceptance: exercise internal/external influence and containment across a rotated room or cave region, including immunity combined with an off-center target.


## Global Infrastructure: the experience we eventually want

A person opens a city, harbor, factory or construction site, selects a place or modeled object, and sees the Records they are allowed to read: its identity, relevant Needs and Contributions, responsible parties, accepted facts, observations, commitments, transactions, maintenance, proposed changes and automations.

The same information is available through lists, tables, graphs and accessible forms. A person should be able to order material or review a citizen proposal without downloading a city model or navigating in 3D.

A shared city workspace can contain official layers supplied by an Organ, operator layers, citizen observations and proposed Transfers or Record changes. Reviewing a proposal can publish an accepted revision through existing authority paths. A municipality's acceptance does not erase dissenting evidence or make every contributor an official editor.

Models may combine engineering meshes, terrain, imagery, point clouds, Gaussian splats and calibrated camera feeds. Each layer has its own source, rights, capture date, coordinate frame, update cadence and uncertainty. A building modeled last year and a camera frame captured now must visibly have different ages.

Lince must also be useful when geometry is incomplete, a camera is unavailable, a connection is interrupted or an organization shares only aggregate information. Partial coverage is a normal state of the system.

### Four different things to keep distinguishable

| Layer | Example | What it can establish |
| --- | --- | --- |
| Representation | A modeled warehouse, mesh or splat scan | Appearance or geometry for a stated source and revision |
| Observation | A scale reading, camera detection or operator count | A source's measurement or claim, with time and quality |
| Accepted operational state | A received lot, approved work order or settled Transfer | The state accepted under a named authority and policy |
| Scenario | A projected shortage after a supplier stops | A conditional result from a named base and assumptions |

A camera seeing a pallet does not establish its contents, ownership or availability for sale. A visually accurate mine does not establish its future output. A replayable simulation does not establish that its physical or economic assumptions are correct.

## Production, stock, logistics, ordering and public commerce

These should be compositions of the foundations below. Their interface deserves convenient names and forms even when their underlying operations are shared.

| Workflow | Shared capabilities | Thin workflow-specific layer |
| --- | --- | --- |
| Stock | Resources, units, custody, accepted events, reservations, reconciliation | Lot/serial vocabulary, receiving/counting forms, shelf and availability views |
| Production | Input/output processes, capacity, scheduling, traceability, evidence | Recipes, yields, quality checks and production board |
| Logistics | Places, network edges, capacity slots, commitments, handoff evidence | Shipment vocabulary, dispatch form, route and exception views |
| Ordering | Offers, requirements, revisions, reservations, partial fulfillment, Transfers | Order form, supplier comparison and delivery status |
| Public commerce | Selective publication, search, offers, terms, authorization, settlement | Public catalog and accessible ordering flow |
| Construction and operations | Tasks, prerequisites, resources, Areas, evidence, review | Inspection/checklist presets, defect and maintenance views |
| Civic coordination | Organ authority, proposals, evidence, review, publication | Citizen reporting form, decision queue and public place view |

Example: a replacement-parts request becomes a Need with specifications, quantity, place and time. An offer becomes a Contribution with availability and terms. An order binds the accepted revision into commitments and reservations. Receiving records actual accepted quantities and custody changes. Payment or other settlement remains its own event. A shipment being dispatched does not prove delivery or payment.

Do not create parallel commerce, stock and production databases with independent quantities. Their views must derive from one accepted resource/accounting model and its explicit external reconciliation state.

## Existing foundations and the gaps to verify

This is a targeted source inspection of the current shared working tree, not a runtime certification or a complete repository audit. Other work is in progress. Recheck these paths before implementing; a filename or a passing nearby test does not prove the larger capability exists.

| Existing anchor | Reuse | Missing contract or proof for this proposal |
| --- | --- | --- |
| [Records](../../crates/nucleus/src/record.rs), [extensions](../../crates/nucleus/src/record_extension.rs), assertions and vocabulary | Stable Record identity, relationships, configurable fields | Typed external identities, units/references/dates in workflow forms, operational validation and schema revisions |
| [Facts](../../crates/nucleus/src/fact.rs), Record revisions and operation receipts in store | Exact deltas, actors, causes, revision checks and deduplication | Observation-to-acceptance reconciliation and consistent history across external sources |
| [Transfers](../../crates/nucleus/src/transfer.rs), [balances](../../crates/store/src/transfer_balances.rs), [stock limits](../../crates/store/src/transfer_stock.rs) | Terms, commitments, reservations, exact quantities and availability | Verify lot/location/custody dimensions, partial handoffs and transformations without double accounting |
| [Place and routing](../../crates/nucleus/src/place.rs), [location](../../crates/nucleus/src/location.rs), [Map Castle](../../crates/interface/src/native/map_castle.rs) | Geographic points, route primitives, location sharing, map selection and Record markers | Engineering frames, altitude, geographic polygons/volumes, spatial indexes and multimodal capacity-aware routes |
| [Topology assets](../../crates/interface/src/native/topology/assets.rs), [splats](../../crates/interface/src/native/topology/splats.rs), Areas | Imported geometry, appearance, selection and spatial interaction | Persistent source-object bindings, city tiling, alignment revisions and permission-aware spatial queries |
| [Workspace policy](../../crates/interface/src/native/workspace_sync/policy.rs), Organs and property grants | Scoped collaboration, editing and proposal review | Prove civic layers and operational field ownership using existing policy mechanisms |
| [Projection](../../crates/nucleus/src/projection.rs), simulation crate, Rules and Signals | Time windows, isolated execution, proposals and evidence | Quantitative resource/capacity propagation and explicitly supported specialist simulation inputs |
| [Interface](../../crates/interface/src/lib.rs) and native sandbox Castles | Reusable native presentation and interaction | Shared operational views and accessible workflows, assembled with the current frontend split |

The inspected extension field kinds are text, number, boolean, select and multiselect. That alone does not supply a typed resource reference, dimensioned quantity, validity interval or lifecycle constraint. Existing Karma typed values and exact arithmetic should be evaluated before adding another type system.

Audit concrete correctness, replay and isolation in the existing simulator before relying on simulation results for operations. This plan does not assume a separate simulation proposal remains present or establishes implemented behavior.

[Computer-Vision-Lab.md](Computer-Vision-Lab.md) tracks the existing vision-tool direction and defers reconstruction. Keep geometry, appearance and semantic identity separate; live reconstruction is not a prerequisite for stock or civic workflows.

## Reusable base capabilities to add or strengthen

Each capability needs a backend contract and a native UI where people must configure, inspect or resolve it. These are proposed requirements; inspect current behavior before deciding which need new code.

### F1. Typed identities, schemas and relationships

- Give an external identity a source namespace, source object ID and Record binding. Keep source revision and imported asset hash separately from object identity. Names, mesh indexes and scene paths are insufficient identities.
- Support one Record represented in several models, and a modeled object associated with several Records. Model splitting or merging needs an explicit mapping review rather than automatic identity replacement.
- Extend configurable forms with typed references, quantities with units, instants/intervals and validated structured values where existing mechanisms cannot express them. Use stable field IDs and bind values to a named schema revision.
- Describe required fields, relationship cardinality, allowed transitions and permission requirements in bounded definitions. Validation runs in the backend for imports, APIs, UI actions and automations.
- Keep source-specific properties in namespaced extensions. Promote a concept into shared infrastructure when multiple actual workflows need its behavior, rather than copying every IFC or ERP field into the core.
- Show source bindings, schema errors and conflicts in a reusable inspector. Allow linking existing Records during import before creating duplicates.

**Proof:** importing a renamed object or a reordered scene preserves its Record; two exporters with the same local object ID do not collide; a malformed or unauthorized state transition is rejected through both UI and API.

### F2. Evidence, time, revisions and reconciliation

- Preserve at least the time a source says an event happened and the time Lince received/recorded it. Add validity intervals where a claim describes a period. Operational queries need both “what was believed then?” and “what do we now know about then?”
- An observation carries subject, property/value/unit, source identity, source event identity, time, quality and evidence reference. An accepted change links to the observations and actor/policy that accepted it.
- Keep contradictory observations available according to retention and disclosure policy. Do not average incompatible claims or let the newest timestamp silently choose official truth.
- Distinguish unknown, stale, unavailable, withheld, disputed and confirmed absent. In public responses, avoid revealing whether an inaccessible private Record exists.
- Define the authority for each accepted property: for example, an ERP may own accepted stock balances, a survey may own geometry and an Organ may approve public maintenance state. An importer cannot claim authority merely because its data arrived last.
- Correct accepted operational history with traceable adjustment/compensation events. Undoing a UI edit is different from reversing a dispatched shipment or completed physical transformation.
- Publish a coherent revision or a declared bounded patch. An “as of” view must expose differing source ages and incomplete coverage rather than implying one perfectly synchronized present.

**UI:** source-age badges, evidence timeline, proposed-versus-accepted changes and a conflict/reconciliation queue shared by camera observations, stock counts, imported changes and citizen reports.

**Proof:** a late receiving event appears at its effective time without rewriting what was known earlier; a stale camera detection cannot overrule an accepted count; duplicate delivery notifications do not apply twice.

### F3. Resources, quantities, rights and availability

- Separate a resource specification, fungible lot and individually tracked asset. A material grade, a batch of that grade and a specific machine are different identities. A recipe consumes resources matching constraints, not merely identical display names.
- Preserve Lince's signed Need/Contribution convention. An unmet Need must not become an accidental negative physical inventory balance. Intent, accepted on-hand stock and projected future stock need distinct meanings.
- Reuse exact decimal accounting for accepted quantities, commitments and money. Unit conversion needs dimension checks, explicit scale/rounding and versioned conversion data. Unknown density must block an assumed mass-to-volume conversion.
- Describe stock by the necessary dimensions: resource/lot, location, owner, custodian and status. Owned stock at another location, consigned stock on site and quarantined stock cannot all count as freely available.
- Reuse existing reservation and balance paths. Specify cancellation, expiry, partial use, released remainder, inbound expectations and time-dependent capacity. Do not subtract the same reservation in two views.
- Describe responsibilities and rights separately: owner, operator, maintainer, custodian, workspace editor and person allowed to execute a command. A drawn Area or workspace grant does not establish physical ownership or legal title.
- Keep receipt, movement, consumption, production, count adjustment, quarantine, damage, waste and return as typed causes with evidence. Counters derived from them should reconcile with the current state.

**UI:** reusable balances by location/lot/status, reservation detail, receive/move/count forms and a rights/responsibility inspector. Stock presets configure these views.

**Proof:** two concurrent reservations cannot spend the same available stock at its authority; a partial delivery preserves the remaining commitment; a movement changes location without changing total accepted quantity; a count adjustment retains its reason.

### F4. Processes, capacity and commitments

- Describe a bounded process using eligible inputs, outputs, duration, capacity/resource requirements, place, prerequisites, actors, quality gates and evidence required to complete it. Keep a versioned process definition separate from each execution.
- Model consuming, producing, moving, using and observing a resource as different operations. Reserving a machine for two hours does not consume the machine. Record work in progress, scrap, co-products and losses explicitly.
- A process records expected and actual results separately. Multi-step work can stop after partial physical completion; do not simulate rollback of consumed materials.
- Use atomic validated acceptance for changes inside one authority/store. Across organizations, use commitments, handoff receipts and explicit pending/disputed/reconciliation states. Do not promise a single database transaction across a factory, supplier and harbor.
- Add capacity calendars, maintenance outages and slot reservations on top of existing work/scheduling primitives where possible. Start with explicit feasible schedules; optimization comes later through bounded solver adapters.
- Lifecycle definitions need visible transitions, actors, preconditions, receipts and cancellation rules. Prefer existing Rule/Command/Transfer mechanisms; add a shared transition validator only when those cannot enforce the same invariant.
- A reusable traceability relation connects actual input lots to actual output lots and subsequent handoffs. Support recalls and reverse tracing without pretending every relation is a quantity-preserving transformation.

**UI:** process form, work board, capacity calendar, execution/evidence detail and exception queue. Recipe, dispatch and construction-checklist presets use the same mechanisms.

**Proof:** producing an accepted output cannot silently omit required input accounting; replaying a completion cannot consume inputs again; a quality failure leaves the actual produced lot quarantined; an outage blocks conflicting slots.

### F5. Dependencies and consequences

- Give dependency relations explicit meaning: requires, consumes, supplies, reserves, transports, substitutes, depends on capacity, or merely relates to. Graph reachability alone establishes no quantitative consequence.
- Time and quantity matter. Track lead times, inventory buffers, yields, capacities, availability windows, substitute qualifications and contractual due dates. Cycles require a stopping rule and diagnostics, not endless recursive propagation.
- Compute impact using a captured accepted base plus named assumptions. Include source freshness and omitted dependencies. Show reasons and the first limiting input/capacity, not just a red downstream node.
- Begin with deterministic bounded event/resource calculations through supported simulation behavior. Add uncertainty intervals or ensembles after defining what they mean; do not attach invented probability percentages to unknown inputs.
- Keep proposals for mitigation separate from commitments. Selecting an alternative supplier in a scenario does not reserve its material or place an order.

**Worked example, illustrative quantities:** a factory has 30 tonnes of qualified mineral available and consumes 10 tonnes per day. An inbound supply is delayed five days. With no substitute, maintenance interruption or other demand, its buffer covers three days; the scenario predicts a two-day input gap. At a declared yield of 0.8, planned output is 8 tonnes per operating day. Actual customer delays additionally depend on finished stock, priorities, transport, receiving windows and accepted commitments. Missing those inputs must make the customer-impact result incomplete.

**UI:** linked dependency graph, time plot, limiting-reason panel and baseline-versus-alternative comparison. Every result links to assumptions and evidence.

**Proof:** changing a buffer changes the predicted stop time; an unrelated relation does not propagate a shortage; an unqualified substitute is excluded; unavailable private supplier data is shown as incomplete without leaking its details.

### F6. Shared queries, projections and publication

- Provide permission-aware filtering, sorting, grouping, aggregation, pagination and bounded subscriptions over typed properties, relationships, space and time. Operational views should share their query definitions rather than fetch every Record and filter on the client.
- Incrementally maintain or cache expensive projections with explicit revisions and invalidation. A cached aggregate is subject to the same access policy as its inputs and can require its own disclosure policy.
- Reuse a selected Record, filter and time window across a table, calendar, network view and map. Store view presets separately from accepted facts. Display unsupported predicates instead of returning a misleading empty result.
- Public publication is an intentional projection: selected fields, granularity, time delay, license, provenance, permitted aggregation and update policy. Public availability need not disclose exact stock, camera footage, supplier contracts or individual locations.
- Support machine-readable offers, requirements and accepted public events alongside human views. Choose a minimal paginated API and export format first, with stable identifiers, revision markers and bounded incremental retrieval.
- Test private references, spatial indexes, tile metadata, counts, caches and aggregate inference as disclosure paths. Filtering the Record inspector alone is insufficient.

**UI:** shared filter/group controls, table/forms, publish preview and a source/coverage summary. People should see exactly which fields and geometry a public workspace exposes.

**Proof:** private stock never appears through a public map, count, search or stale cache; a public buyer can inspect terms and initiate an order without learning the producer's private process.

### F7. External systems and automation

- An adapter declares its identities, supported read/write operations, units, source authority, schema mapping, freshness, credentials and failure behavior. Initially support import/read/propose; introduce execution only under a defined policy.
- Reuse existing capability and operation paths for execution. Separate observing, proposing, approving, publishing, reserving and physically commanding. Give each operation bounded scope and an application receipt.
- Handle duplicate events, reorderings, source resets, revisions, disconnects and conflicts explicitly. Durable deduplication should use source namespace plus event identity; reconnecting must reconcile a checkpoint before accepting an apparently continuous stream.
- Choose one owner for each accepted field. When both systems can edit, define conflict handling and origin markers to prevent echo loops. Do not overwrite an external update with a stale Lince export.
- Remote side effects can have unknown outcomes after a timeout. Mark them pending/unknown, inspect the remote receipt/state and retry only when the protocol supports safe deduplication. Do not claim exactly-once execution across arbitrary external systems.
- Give automations debounce/hysteresis where relevant, rate/budget limits, simulation preview, expected revisions, cancellation, audit and revocation. Fiote can interpret evidence and propose operations; it does not bypass these contracts.
- Keep native device safety/interlocks in the responsible equipment system. A Lince Area, camera inference or simulation result must not become a replacement for a certified physical control system.

**UI:** connector mapping/setup, live source health, proposals and an execution/receipt queue. A timeout must be distinguishable from a rejection or completed action.

**Proof:** reconnection cannot double-receive stock; changing an ERP row creates a reviewable conflict; revocation stops future commands; imported text cannot become privileged automation instructions.

### F8. Spatial identity, frames and Areas

- Keep canonical geographic coordinates and engineering coordinates separate from presentation transforms. A coordinate frame specifies reference system, origin, axes, handedness, units, vertical reference where applicable and its alignment revision.
- Use high precision for canonical/world transforms and nearby local origins for rendering. Changing the camera origin must not change a Record's stored location. Test round trips at actual city extents.
- Support points, routes/lines, polygons with holes, surfaces and volumes where workflows require them. Containment, proximity, intersection and network connectivity are different predicates. A visually overlapping object is not necessarily physically inside a zone.
- Distinguish workspace layout Areas, measured physical regions, geographic jurisdictions, operational zones and temporary Areas. Reuse Area behavior without treating a decorative boundary as authoritative geography or ownership.
- Bind a Record/Area to a stable modeled asset or local anchor with an explicit follow policy. Re-importing geometry, moving a real asset and correcting alignment are different changes. Persist unresolved bindings for review rather than attaching to the nearest object silently.
- Define boundary and uncertainty behavior: floors, height, holes, moving assets, tracking error, entry/exit hysteresis and stale positions. Precision sufficient for city selection may be insufficient for a shelf or construction clearance.
- Area effects must remain subject to existing authority and command policies. Entering a zone can propose or signal work; it must not automatically settle ownership or execute an unsafe device action.
- Spatial queries and picking use lightweight semantic geometry when appearance uses splats. Render detail, collision geometry, measurement geometry and legal boundaries may all differ.

**UI:** place/model alignment controls, layer provenance, Area editing, binding repair and inspection of exact versus approximate location. Selecting in 2D or 3D opens the same Record detail.

**Proof:** changing an imported model revision preserves valid Record/Area bindings; changing a floating origin preserves geographic location; an ambiguous boundary observation does not trigger a confident entry command.

### F9. Efficient city and site representations

- Keep large geometry/media out of frequently replicated Record rows. Records reference versioned asset manifests and bounded metadata; blob/asset delivery follows its own permissions and lifecycle.
- Partition by spatial bounds and useful semantic groups. Stream visible/needed tiles, use level of detail, instancing and simplified picking proxies, and evict unused CPU/GPU resources. Do not spawn an always-active entity and label for every object in a city.
- Query operational information independently from rendering. Aggregate map markers and status by scale, then disclose individual assets on demand. A stock ledger must open without its warehouse mesh.
- Publish patches or replacement tiles with a coherent manifest revision. Support cancellation, download resumption where available, checksum verification, bounded queues and invalidation when source assets change.
- Budget meshes, textures, splats, decoding, upload rate, tile requests, labels, camera feeds and per-frame CPU work independently. Fewer triangles alone cannot guarantee good frame times.
- Keep authored geometry and captured appearance as separate switchable layers. Avoid rendering both as competing surfaces without alignment/occlusion rules. Reconstruction artifacts need capture time and uncertainty.
- Validate coordinate correctness, semantic picking and retained identity alongside visual quality. A model that looks correct but selects the wrong warehouse is not acceptable.

**UI:** layers, progressive loading, simplified fallback, visible source ages and resource diagnostics. Offer 2D and table workflows on hardware unable to display the detailed scene.

**Proof:** a bounded neighborhood can be inspected while the rest of the city is unloaded; rapid navigation cancels obsolete requests; closing a view releases its resources; a hidden layer's metadata does not leak private assets.

### F10. Live observations, cameras and specialist simulations

- Reuse Signal semantics for sampled properties where appropriate. High-rate media uses a separate bounded stream with identity, capture timestamps, calibration, quality and retention. Do not store every video frame as a Record or numeric Signal.
- A live camera can be a screen at a place or a calibrated viewpoint. Projecting its image into geometry requires calibration, time alignment and visibility/occlusion handling; a URL alone does not supply these.
- Keep observation, object tracking, reconstructed geometry and accepted operational changes separate. A disappearing detection can mean occlusion or tracking failure, not consumption, destruction or sale.
- Coordinate reconstruction with [Computer-Vision-Lab.md](Computer-Vision-Lab.md). Require replay fixtures, calibration and explicit source demand before any live reconstruction integration. Shared stream contracts can be built without promising online reconstruction of whole cities.
- Allow specialist solvers to consume explicitly mapped, versioned inputs and return typed results with units, model identity, assumptions, coverage and execution status. They must not write live accepted state directly from a scenario.
- Distinguish Lince's deterministic workflow/resource simulation from thermal, structural, fluid, traffic or equipment simulation. Their solver validity, step coordination and calibration require separate evidence.

**Proof:** loss of a feed shows stale/unavailable state without deleting an asset; source motion does not become stock motion; an interrupted specialist run cannot publish a complete forecast; simulations cannot issue live commands.

## Interoperability: choose by purpose

These are researched integration candidates, not claims of existing Lince support. Standard scope does not guarantee exporter/importer conformance. Select one real partner dataset, declare the supported subset and test it before expanding.

| Purpose | Candidate and documented role | Proposed boundary in Lince |
| --- | --- | --- |
| Runtime mesh delivery | [Khronos glTF/GLB](https://www.khronos.org/gltf/) targets runtime 3D asset delivery | Render geometry/materials; preserve a separate semantic/identity manifest. A GLB does not supply process logic or operational truth. |
| Engineering asset semantics | [buildingSMART IFC](https://technical.buildingsmart.org/standards/ifc/) describes built-environment data | Import selected identities, relationships, properties and geometry through a bounded worker; retain original source and unsupported-property report. |
| Model-linked issue exchange | [buildingSMART BCF](https://technical.buildingsmart.org/standards/bcf/) supports model-based issue communication | Map issues, viewpoints and relevant element references to Records/proposals; do not assume it transfers the whole model. |
| Urban semantic models | [OGC CityGML](https://www.ogc.org/standards/citygml/) provides a 3D urban information model | Map appropriate city objects and attributes into shared spatial/identity contracts. |
| Large geospatial scene delivery | [OGC 3D Tiles](https://www.ogc.org/standards/3dtiles/) addresses streaming large 3D geospatial datasets | Implement a declared tiled delivery subset, independent of operational Records. Existing splat rendering does not prove tiled splat interoperability. |
| Composed authoring scenes | [OpenUSD](https://openusd.org/release/intro.html) supports layered scene composition and interchange | Use an optional conversion/composition worker when a source requires it; retain layer references and semantic mappings. Do not assume prim paths are durable Record IDs. |
| Geospatial sensor observations | [OGC SensorThings](https://www.ogc.org/standards/sensorthings/) connects sensor observations and metadata | Map observations, features, units and source times; keep acceptance policy in Lince. |
| Industrial information exchange | [OPC UA](https://opcfoundation.org/about/opc-technologies/opc-ua/) supports industrial interoperability | Use a scoped gateway adapter; read/propose first, separately authorize commands. |
| Supply-chain event exchange | [GS1 EPCIS](https://ref.gs1.org/standards/epcis/2.0.1/) defines visibility-event information, including object and transformation events | Map external event identities, lots, places and transformations; operational acceptance and contractual settlement remain explicit. |
| Dynamic model exchange/co-simulation | [FMI](https://fmi-standard.org/docs/main/) defines model exchange, co-simulation and scheduled-execution interfaces | Later isolated solver worker with resource limits, unit mappings and an explicit orchestration/time-step contract. |

These recommendations infer useful boundaries from the standards' documented scopes. They are not an assertion that adopting a standard implements the workflow. Geometry, operational events and simulation exchange remain separate integrations.

External authoring tools can supply exported data with identity/semantic manifests, or a scoped live connector when a real workflow needs it. Imported geometry does not automatically preserve source scripts, shaders or behavior.

Start with the least integration that proves a workflow: a versioned model export and event file/API from one source. Add an editor plugin only when stable IDs or useful round-trip changes cannot be preserved through that export. Start with one-way ingestion; prove reconciliation before bidirectional writeback.

Imported assets, models, map data, camera data and derived exports need their own rights/attribution metadata. Embedded dependencies require LICENSE and available credits in their Sand. Publicly reachable data is not automatically reusable or redistributable data.

## Civic workspaces and public commerce need real governance

The proposal is an official Organ operating a shared workspace with layers whose authors and authority are visible. It can publish selected accepted Records while citizens contribute evidence and propose changes through scoped workflows.

Reusable requirements are proposal target/revision, proposed patch or Transfer, actor, evidence, affected area, reviewers, review result, reason and application receipt. Reuse existing proposal paths where they support these requirements. Do not assume existing Transfer-specific sharing already provides general civic proposal sharing.

Allow official, operator, community and individual claims to coexist. A review can accept, reject, request evidence or identify a duplicate. Duplicate reports can share an issue without exposing private reporters. Display status and reasons publicly to the extent permitted. Evidence retention and moderation belong to the workspace's explicit policies.

A citizen might propose a repair, identify an accessibility barrier, offer materials or propose a resource Transfer. Approval of the report does not necessarily approve spending, procurement, land access or physical execution; those decisions use their own responsible actors and commitments.

Public commerce should expose useful terms and availability while minimizing disclosure of private inputs. Support revision-bound offers, units, quality/specifications, delivery/place windows, partial fulfillment, cancellation, returns/disputes and settlement status using the same resource and Transfer semantics. Public search and ordering need quotas and abuse handling so they cannot reserve unlimited resources or enumerate private workspaces.

Do not make Lince a full tax, payments, cadastral or railway-safety system as a hidden consequence of this proposal. Define export/connectors for those responsibilities when an actual deployment needs them. Lince can coordinate commitments and record evidence without claiming it legally proves title or replaces the responsible authority's decision.

## Criticism and improvements to the original idea

1. **“A representation of the present moment” is too strong.** Different sources arrive at different times and can disagree. Improve it to an inspectable accepted state with source ages, plus observations and scenarios that remain distinguishable.
2. **Operational correctness needs its own proof.** Personal city navigation and room modeling can be useful before stock workflows exist. When adding operational use, prove stable identity, resource accounting, permissions and reconciliation with a table and small fixture; visual realism cannot establish them.
3. **Open data should be selectable.** Publish offers, public works and aggregate capabilities without requiring exact stock, contracts, private interiors or camera footage to be public. Make publication a reviewable view with an explicit license.
4. **A dependency graph is insufficient for supply-chain prediction.** Add buffers, lead times, yields, capacity and substitutes. Show missing data and reasons rather than promising consequences from any linked Record.
5. **Do not over-generalize every business rule into a new language.** Reuse bounded definitions and existing Rule/Command machinery. Extract a shared primitive after the factory and civic cases demonstrate the same need.
6. **Do not make every scene element a Record automatically.** Keep static details in asset metadata. Create or link Records for objects people coordinate, query, own, maintain or attach evidence to; expand semantic coverage when needed.
7. **Camera detail can increase exposure without increasing operational certainty.** Begin with recorded evidence and calibrated viewpoints. Add live feeds for a named purpose with retention and access policies.
8. **A city should not require a single giant trusted workspace.** Permit independently governed layers and partial federation. One Organ can certify its decisions without owning everyone else's observations.
9. **“All workflows” needs an extraction test.** A foundation earns its complexity when two distinct workflows use it without duplicating backend invariants. Until then keep the domain detail in a preset or adapter.
10. **3D must earn its interface cost.** It helps locating equipment, reviewing a spatial conflict or understanding a route. Receiving goods, approving terms and resolving an evidence conflict usually also need fast forms and tables.

Suggested additions to test the breadth of the base: preventive maintenance, lot recall, waste/by-product offers, accessibility reports, shared equipment booking and utility outages. These stress the same resource, evidence, capacity and spatial contracts without requiring six new domain engines.

