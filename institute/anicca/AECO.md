# AECO and connected infrastructure in Lince

Proposal for refinement, 2026-10-12. This is a design and implementation plan, not an implemented feature or a change to [Lince.lingua](Lince.lingua). The Global Infrastructure work remains last in the broader roadmap, as requested. Its prerequisites should improve ordinary Lince workflows before we assemble a city-scale experience.

AECO means Architecture, Engineering, Construction and Operations. The requested scope also includes manufacturing, supply chains, transport, commerce and civic coordination. Treat AECO as the document's name, not a boundary on what the foundations can support.

## Recommended direction

Make Lince able to describe **who can do what, with which resources, where, when, under which commitments, based on which evidence**. Attach these descriptions to maps and models, and let specialized tools supply geometry, measurements and simulations.

Use existing Records, assertions, Needs, Contributions, Transfers, Rules, Signals, Organs, Places, Areas and workspaces wherever their semantics fit. Extend shared capabilities when a missing invariant affects several workflows. Package industry vocabulary, forms, saved queries, processes and views as Trails or other existing preset mechanisms. Keep format conversion, machinery protocols and specialist solvers behind adapters.

The core should know about a bounded resource transformation or a capacity reservation. It should not need a new hardcoded Record kind for a blast furnace, port crane, railway siding or municipal pothole report.

“Minimal workflow-specific code” cannot mean no domain semantics. Production yields, consignment ownership, railway access and public decision authority have different rules. Those differences belong in explicit schemas, policies, process definitions and specialist adapters; hiding them inside arbitrary tags would make the system unreliable.

Recommended first proof, pending the owner's answer: one supplier, one factory, two stock locations, one production process, one delivery and one public offer. Start with reviewed proposals. Reuse its foundations for a small civic workspace before attempting a whole city.

## 1. Global Infrastructure: the experience we eventually want

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

## 2. Production, stock, logistics, ordering and public commerce

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

## 3. Existing foundations and the gaps to verify

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

[Simulation.md](Simulation.md) tracks concrete correctness and isolation work in the existing simulator. Complete or verify its relevant requirements before relying on simulation results for operations. This proposal does not reopen that document's retired feature sequence.

[Camera-reconstruction.md](Camera-reconstruction.md) already separates geometry, appearance and meaning, and treats network camera integration as deferred. Reuse those boundaries; this document does not make live reconstruction a prerequisite for stock or civic workflows.

## 4. Reusable base capabilities to add or strengthen

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
- Follow [Camera-reconstruction.md](Camera-reconstruction.md) for replay-first reconstruction and its separately deferred network feeds. Shared stream contracts can be built without promising online reconstruction of whole cities.
- Allow specialist solvers to consume explicitly mapped, versioned inputs and return typed results with units, model identity, assumptions, coverage and execution status. They must not write live accepted state directly from a scenario.
- Distinguish Lince's deterministic workflow/resource simulation from thermal, structural, fluid, traffic or equipment simulation. Their solver validity, step coordination and calibration require separate evidence.

**Proof:** loss of a feed shows stale/unavailable state without deleting an asset; source motion does not become stock motion; an interrupted specialist run cannot publish a complete forecast; simulations cannot issue live commands.

## 5. Interoperability: choose by purpose

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

For UE5, Unity and Blender, support two paths: **exported data** with identity/semantic manifests, and **a live connector** for a specific project that needs operational exchange. A project file can include engine scripts, plugins, custom shaders and assumptions that an exported mesh cannot preserve. Do not plan to run arbitrary UE/Unity projects inside Bevy. A remote viewer can be a later adapter when its interaction and authentication contract is defined.

Start with the least integration that proves a workflow: a versioned model export and event file/API from one source. Add an editor plugin only when stable IDs or useful round-trip changes cannot be preserved through that export. Start with one-way ingestion; prove reconciliation before bidirectional writeback.

Imported assets, models, map data, camera data and derived exports need their own rights/attribution metadata. Embedded dependencies require LICENSE and available credits in their Sand. Publicly reachable data is not automatically reusable or redistributable data.

## 6. Civic workspaces and public commerce need real governance

The proposal is an official Organ operating a shared workspace with layers whose authors and authority are visible. It can publish selected accepted Records while citizens contribute evidence and propose changes through scoped workflows.

Reusable requirements are proposal target/revision, proposed patch or Transfer, actor, evidence, affected area, reviewers, review result, reason and application receipt. Reuse existing proposal paths where they support these requirements. Do not assume existing Transfer-specific sharing already provides general civic proposal sharing.

Allow official, operator, community and individual claims to coexist. A review can accept, reject, request evidence or identify a duplicate. Duplicate reports can share an issue without exposing private reporters. Display status and reasons publicly to the extent permitted. Evidence retention and moderation belong to the workspace's explicit policies.

A citizen might propose a repair, identify an accessibility barrier, offer materials or propose a resource Transfer. Approval of the report does not necessarily approve spending, procurement, land access or physical execution; those decisions use their own responsible actors and commitments.

Public commerce should expose useful terms and availability while minimizing disclosure of private inputs. Support revision-bound offers, units, quality/specifications, delivery/place windows, partial fulfillment, cancellation, returns/disputes and settlement status using the same resource and Transfer semantics. Public search and ordering need quotas and abuse handling so they cannot reserve unlimited resources or enumerate private workspaces.

Do not make Lince a full tax, payments, cadastral or railway-safety system as a hidden consequence of this proposal. Define export/connectors for those responsibilities when an actual deployment needs them. Lince can coordinate commitments and record evidence without claiming it legally proves title or replaces the responsible authority's decision.

## 7. Criticism and improvements to the original idea

1. **“A representation of the present moment” is too strong.** Different sources arrive at different times and can disagree. Improve it to an inspectable accepted state with source ages, plus observations and scenarios that remain distinguishable.
2. **A realistic city model is not the hardest dependency.** Stable identity, resource accounting, permissions and reconciliation are what make it operational. Prove those with a table and a small model before paying the cost of city streaming.
3. **Open data should be selectable.** Publish offers, public works and aggregate capabilities without requiring exact stock, contracts, private interiors or camera footage to be public. Make publication a reviewable view with an explicit license.
4. **A dependency graph is insufficient for supply-chain prediction.** Add buffers, lead times, yields, capacity and substitutes. Show missing data and reasons rather than promising consequences from any linked Record.
5. **Do not over-generalize every business rule into a new language.** Reuse bounded definitions and existing Rule/Command machinery. Extract a shared primitive after the factory and civic cases demonstrate the same need.
6. **Do not make every scene element a Record automatically.** Keep static details in asset metadata. Create or link Records for objects people coordinate, query, own, maintain or attach evidence to; expand semantic coverage when needed.
7. **Camera detail can increase exposure without increasing operational certainty.** Begin with recorded evidence and calibrated viewpoints. Add live feeds for a named purpose with retention and access policies.
8. **A city should not require a single giant trusted workspace.** Permit independently governed layers and partial federation. One Organ can certify its decisions without owning everyone else's observations.
9. **“All workflows” needs an extraction test.** A foundation earns its complexity when two distinct workflows use it without duplicating backend invariants. Until then keep the domain detail in a preset or adapter.
10. **3D must earn its interface cost.** It helps locating equipment, reviewing a spatial conflict or understanding a route. Receiving goods, approving terms and resolving an evidence conflict usually also need fast forms and tables.

Suggested additions to test the breadth of the base: preventive maintenance, lot recall, waste/by-product offers, accessibility reports, shared equipment booking and utility outages. These stress the same resource, evidence, capacity and spatial contracts without requiring six new domain engines.

## 8. Sequential implementation plan

This is a future sequence for the requested proposal, not authorization to begin feature coding. Global Infrastructure assembly remains the last stage. Finish each stage's backend behavior, native interaction and meaningful checks before moving to the next. Do not change the old web crate.

| Stage | Work | Completion gate |
| --- | --- | --- |
| 0 | Choose the first workflow, authoritative data source, hardware, acceptable errors and publication policy. Audit existing behavior and simulation prerequisites. | Written fixture, expected events/quantities, permissions and measurable acceptance targets. |
| 1 | F1–F2: stable external identity, the minimum missing types, dated evidence and reconciliation. | One source import/re-import with a conflict review, preserving identities and accepted revisions. |
| 2 | F3: dimensions of resource balances and existing reservation/accounting integration. Add stock forms and views. | Receive, move, reserve, count and partially fulfill a lot; concurrent attempts preserve invariants. |
| 3 | F4: one versioned input/output process and capacity schedule. | A production execution with actual input/output/waste evidence, quarantine and interruption handling. |
| 4 | Logistics, ordering and public commerce presets over F3–F4 and F6. | A public offer results in a revision-bound order, partial delivery and explicit remaining obligations. |
| 5 | F5: one supplier disruption and alternative scenario over accepted data. | Explain buffer exhaustion and incomplete downstream impact; scenarios produce no live effects. |
| 6 | F6–F7: one external adapter and scoped automation, extending earlier query/publication work. | Disconnect/reconnect, deduplication, conflicts, revocation and unknown remote outcomes are visible and correct. |
| 7 | A small civic workspace using the same evidence/review/publication contracts. | A citizen report and proposed contribution go through official review with selective public disclosure. |
| 8 | F8: one site model with geographic/local frames and Record/Area bindings. | Correct picking/alignment, permission checks and binding preservation across re-import. |
| 9 | F9: a measured neighborhood-scale scene using a declared tiled subset. | Bounded working set, cancellation/eviction, progressive inspection and usable 2D fallback. |
| 10 | F10: separately scoped observations/cameras or specialist solver, chosen for one real need. | Its independent replay/calibration/isolation/retention gates pass; no implied whole-city reconstruction. |
| 11 | Assemble Global Infrastructure views across independently governed sites and layers. | Shared selections connect modeled places, relevant Records, commitments, proposals and consequences without bypassing authority or loading everything. |

Stages can revise an earlier schema when concrete evidence exposes a missing requirement, but should not start a second subsystem to avoid finishing the first. For example, develop publication in stage 4 as needed; stage 6 hardens its external integration rather than postponing an essential part of commerce.

The reuse gate after stage 7 is especially important: the civic workflow should use the same evidence, proposals, queries and publication infrastructure. Resource transformation can remain unused there; not every preset needs every foundation.

No stage requires backwards compatibility with older Lince versions. Database migration history still stays append-only: add a higher-numbered migration, append its exact SHA-384 entry, run `cargo check -p store` and migration_guard tests before continuing. Other implementation checks use `cargo check`, treat warnings as errors and run through the repository's cargo xtask/nix workflow. No Node.js toolchain is needed for the native UI.

## 9. Acceptance, performance and security evidence

Choose dataset sizes and targets in stage 0. The following are proposed validation dimensions, not measured performance or delivery promises.

**Correctness:** exact quantities/unit errors; lot/custody/location distinctions; competing reservations; partial fulfillment; process input/output accounting; duplicate and out-of-order source events; conflicting authority; effective-versus-received time; stale revisions; coordinate round trips; re-import bindings; scenario incompleteness and replay isolation.

**Security:** denied field edits through every entry point; publication of private references/counts/tile metadata; cached results after revocation; forged source identities; hostile imports and bounded archive/decode allocation; asset-path traversal; connector network destination and credential handling; observation content treated as data; command scope and revoked grants; simulations unable to trigger live effects.

**Performance:** measure accepted operational latency, query latency and scene frame time separately. Record CPU/RAM/GPU limits, asset bytes, network volume, first useful view time, p95/p99 query/frame latency, cancellation delay and released resources after closure. Exercise many semantic Records, one dense site, a large unloaded city extent, many overlapping splats and slow/disconnected sources as different fixtures. A single small scene does not establish city scalability.

For an initial experiment, a useful candidate is 10,000 operational Records, a 100,000-feature spatial index with only a bounded neighborhood rendered, and a 30-minute navigation/reconnect session on named hardware. These counts are suggested stress fixtures, not minimum production capacity. Set numeric latency and memory limits before measuring; retain results and decide whether to simplify or optimize from evidence.

Every stage should retain its fixture, expected result, observed result and any coverage limitation. Add tests for meaningful invariants and observed risks rather than tests that only mirror a form's layout. A dependency-heavy Sand includes its embedded license/credit material before completion.

## 10. Questions for refinement

The following decisions remain open. The default proposal is a factory-first proof and reviewed proposals; it is not a claim that the owner selected those options.

### Decisions that change the first implementation

1. Which first complete workflow should guide extraction: factory stock → production → delivery, civic report → official review, or harbor/rail capacity → disruption planning?
2. What is the first real data source or sample dataset, and who is authoritative for accepted stock, ownership/responsibility and location? A sample with stable external IDs is more useful than a photorealistic scene alone.
3. Should the first release only import and propose, or execute bounded routine operations? Which exact operation needs execution: a reservation, an ERP update, an order submission or a device command?
4. What target hardware, disconnected-use requirement and useful scale should define “efficient”: one factory, a neighborhood or a whole city? What spatial error is acceptable for its first interaction?

### Decisions that change governance or later architecture

5. Does “public commerce” mean public catalogs with authenticated participants, or ordering by people who do not yet have a Lince identity? Payment processing and guest ordering would add distinct integration/abuse requirements.
6. Is Lince intended to become the system of record for operations, or coordinate existing ERP/MES/GIS systems? Which fields, if any, should remain externally authoritative?
7. Should an official Organ curate only its own accepted layer, or also moderate a common citizen-reporting layer? What should happen to rejected or disputed evidence?
8. Which information should be public immediately, public with a delay/aggregation, partner-only or private? Are live camera feeds actually necessary to the first public use case?
9. Should suppliers share exact stock and recipes, or only available capacity/offers and expected delivery windows? How useful must disruption forecasting remain under partial disclosure?
10. Is editing engineering geometry in Lince necessary, or are selection, annotation, Areas and sending proposed changes back to the authoring system enough? Native CAD editing would substantially expand the scope.
11. Which specialist simulation is needed first: resource flow, transport scheduling, equipment physics or structural/thermal analysis? Which data and solver would justify trusting its result?
12. What should work offline: inspection, capturing proposals, or making binding reservations? Offline evidence capture is much easier than globally exclusive resource allocation.

Recommended scope constraint: complete the first operational chain and the civic reuse gate before promising a globally integrated, continuously reconstructed city. That produces useful shared features while preserving the ambition of the final infrastructure experience.
