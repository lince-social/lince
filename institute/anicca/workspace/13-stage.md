# 13 — Production, stock, procurement and civic workflows

Finish able to coordinate a bounded supplier-to-stock-to-production-to-delivery workflow and a civic issue using the same accepted quantities, evidence, rights and native forms. Supported resource scenarios remain isolated from actual stock and live commands.

## Required workflow milestones

- [ ] **13.M01.** Receive a lot, reserve eligible stock, record partial production and partially deliver an order with traceability. Repeated notifications and competing reservations cannot double-consume or double-spend accepted resources.

- [ ] **13.M02.** Inspect a delayed inbound supply in a bounded scenario using explicit stock, consumption, yield, capacity and calendar assumptions. Show the limiting reason/incomplete inputs without reserving materials or sending commands from the scenario.

- [ ] **13.M03.** Link records/evidence to a site/model selection and submit/review a maintenance or accessibility proposal. Private supplier/reporter/source data stays restricted; approval of evidence does not implicitly approve spending or execution.

- [ ] **13.M04.** Disconnect/reconnect an external source with late/duplicate evidence, compare source ages and reconcile through authority-owned acceptance. Native forms/tables stay fast without loading detailed 3D assets or a physical-material solver.

## Entry and scope

- [ ] Confirm the preceding [stage 12](12-stage.md#completion-summary) passed its workflow gate. Read its short completion summary and the [general stage registry](00-General.md#stage-registry); unresolved predecessor blockers keep this stage pending.

- [ ] Recover only the needed completed context from [stage 02 summary](02-stage.md#completion-summary); [stage 04 summary](04-stage.md#completion-summary); [stage 11 summary](11-stage.md#completion-summary). Open an older contract section only if the handoff cannot answer the current task; do not reload every past stage or future scientific catalogue.

- [ ] Limit implementation to the current core subset: preserve existing Areas/gravity/motion cooling and implement needed geometry/navigation/definition/hosting contracts. Keep new property evaluation, heating/thermodynamics, additional physical solvers, experiments and calculated-state recording deferred to [stages 16–21](00-General.md#deferred-simulation). A future example/field/reference below does not activate its evaluator or make it a core exit requirement.

- [ ] Keep one model/accepted-edit/query/artifact/demand implementation. Treat the following source ownership as a starting location to re-audit, not permission to create every proposed crate/profile upfront: `crates/nucleus/src/`; `crates/engine/src/`; `crates/store/`; `crates/simulation/src/`; `crates/interface/src/native/`.

## Ordered implementation tasks


- [ ] **13.T01.** Implement/verify F1–F4 and shared F6 query/publication contracts through supplier → stock → production → partial delivery, with traceability, reservations, accepted quantities, evidence and native forms/tables.

- [ ] **13.T02.** Add F5 bounded event/resource consequence calculations and F7 source reconciliation/command policy using declared quantities, calendars, yields and supported existing operational-scenario behavior. Audit that implementation's isolation rather than building a new physical solver. Prove disconnect/reconnect, duplicates, late events, competing authority, missing/private inputs and scenario isolation. Thermal/material/mechanical/traffic/process predictions beyond the existing supported calculation are deferred to stages [16](16-stage.md)–[21](21-stage.md); unavailable predictions remain explicit.

- [ ] **13.T03.** Reuse F8–F10 for a site/civic workflow: place-linked observations/issues, proposal/review/publication, maintenance/accessibility and selective public offers. Model lifecycle changes and operational completion use their own explicit acceptance policy.

- [ ] **13.T04.** Apply shared geometry/selection/definition/port contracts to one civil corridor/utility or geotechnical data-authoring workflow, with revisioned quantities and declared survey/model error. Store material labels and existing evidence without new property/ground-response calculations. [stage 11](11-stage.md) supplies geometric profiles; unsupported physical civil/manufacturing studies remain named deferred [stage 21](21-stage.md) rows with required operators/data/fixtures and cannot be completed by a room preset.

## Worked contract example

- [ ] **13.X01.** Use this explicitly simplified whole-day shortage example only with its assumptions. Implement the actual exact quantity/time/accounting contracts, partial periods, existing scenario isolation and missing/private input diagnostics; never infer a real factory forecast from graph reachability alone. Use this standalone illustrative acceptance example to refine the actual production API and regression; it is not an already implemented Lince interface or sufficient milestone evidence.

  ```rust
  fn simple_input_gap_days(
      stock_kg: u64,
      consumption_kg_per_day: u64,
      delay_days: u64,
  ) -> Option<u64> {
      if consumption_kg_per_day == 0 {
          return None;
      }
      let whole_days = stock_kg / consumption_kg_per_day;
      Some(delay_days.saturating_sub(whole_days))
  }

  fn main() {
      assert_eq!(simple_input_gap_days(30_000, 10_000, 5), Some(2));
      assert_eq!(simple_input_gap_days(30_000, 0, 5), None);
  }
  ```

## Architecture and task requirements

### Source baseline and shared families

- [ ] **R0075.** Records, quantities, evidence and policy: nucleus, engine and store — **What to reuse:** IDs, exact accounting, grants, receipts, typed values, publication and accepted changes — **Required extension or boundary:** A model element need not be a Record. Add bindings and typed domain validation rather than another ownership/accounting system.

- [ ] **R0077.** Simulation: `crates/simulation/src/{scenario,world,worker,limits}.rs`, native `simulation_castle/` — **What to reuse:** Captured bases, bounded jobs, checks, evidence and comparison concepts — **Required extension or boundary:** Current workers orchestrate operational Actions and local Cells. They are not PDE/MD solvers or evidence of a sandbox for arbitrary native libraries. Audit isolation and add a physical-job contract.

### Capability coverage and reusable workflow recipes

- [ ] **R0115.** **W07 — Operational/civic systems** — **Shared primitives and thin specialization:** Exact quantities, evidence, authority, processes, queries and spatial/model bindings — **Minimum proof / later expansion:** B14: stock/production/delivery and civic review; F1–F10 remain normative

### Domain workflows as implementations of the primitives

- [ ] **R0369.** Operational/civic work — **Authored/source support:** Records, resources, places and accepted events — **Specialized operation/constraint:** Authority, exact accounting, processes and proposals — **Shared outputs and reuse:** Model bindings, queries, scenario results and selective publication

### Subsequent fixture and adapter decisions

- [ ] **R0853.** Before external operational execution, select accepted field authority, disclosure, command scopes and reconciliation. Offline modeling, local laboratory work and previews require no remote execution grants. Globally exclusive offline reservations require an authority design rather than hopeful conflict merging.

### Production, stock, logistics, ordering and public commerce

- [ ] **R0914.** These should be compositions of the foundations below. Their interface deserves convenient names and forms even when their underlying operations are shared.

- [ ] **R0915.** Stock — **Shared capabilities:** Resources, units, custody, accepted events, reservations, reconciliation — **Thin workflow-specific layer:** Lot/serial vocabulary, receiving/counting forms, shelf and availability views

- [ ] **R0916.** Production — **Shared capabilities:** Input/output processes, capacity, scheduling, traceability, evidence — **Thin workflow-specific layer:** Recipes, yields, quality checks and production board

- [ ] **R0917.** Logistics — **Shared capabilities:** Places, network edges, capacity slots, commitments, handoff evidence — **Thin workflow-specific layer:** Shipment vocabulary, dispatch form, route and exception views

- [ ] **R0918.** Ordering — **Shared capabilities:** Offers, requirements, revisions, reservations, partial fulfillment, Transfers — **Thin workflow-specific layer:** Order form, supplier comparison and delivery status

- [ ] **R0919.** Public commerce — **Shared capabilities:** Selective publication, search, offers, terms, authorization, settlement — **Thin workflow-specific layer:** Public catalog and accessible ordering flow

- [ ] **R0920.** Construction and operations — **Shared capabilities:** Tasks, prerequisites, resources, Areas, evidence, review — **Thin workflow-specific layer:** Inspection/checklist presets, defect and maintenance views

- [ ] **R0921.** Civic coordination — **Shared capabilities:** Organ authority, proposals, evidence, review, publication — **Thin workflow-specific layer:** Citizen reporting form, decision queue and public place view

- [ ] **R0922.** Example: a replacement-parts request becomes a Need with specifications, quantity, place and time. An offer becomes a Contribution with availability and terms. An order binds the accepted revision into commitments and reservations. Receiving records actual accepted quantities and custody changes. Payment or other settlement remains its own event. A shipment being dispatched does not prove delivery or payment.

- [ ] **R0923.** Do not create parallel commerce, stock and production databases with independent quantities. Their views must derive from one accepted resource/accounting model and its explicit external reconciliation state.

### Existing foundations and the gaps to verify

- [ ] **R0924.** This is a targeted source inspection of the current shared working tree, not a runtime certification or a complete repository audit. Other work is in progress. Recheck these paths before implementing; a filename or a passing nearby test does not prove the larger capability exists.

- [ ] **R0925.** [Records](../../../crates/nucleus/src/record.rs), [extensions](../../../crates/nucleus/src/record_extension.rs), assertions and vocabulary — **Reuse:** Stable Record identity, relationships, configurable fields — **Missing contract or proof for this proposal:** Typed external identities, units/references/dates in workflow forms, operational validation and schema revisions

- [ ] **R0926.** [Facts](../../../crates/nucleus/src/fact.rs), Record revisions and operation receipts in store — **Reuse:** Exact deltas, actors, causes, revision checks and deduplication — **Missing contract or proof for this proposal:** Observation-to-acceptance reconciliation and consistent history across external sources

- [ ] **R0927.** [Transfers](../../../crates/nucleus/src/transfer.rs), [balances](../../../crates/store/src/transfer_balances.rs), [stock limits](../../../crates/store/src/transfer_stock.rs) — **Reuse:** Terms, commitments, reservations, exact quantities and availability — **Missing contract or proof for this proposal:** Verify lot/location/custody dimensions, partial handoffs and transformations without double accounting

- [ ] **R0928.** [Place and routing](../../../crates/nucleus/src/place.rs), [location](../../../crates/nucleus/src/location.rs), [Map Castle](../../../crates/interface/src/native/map_castle.rs) — **Reuse:** Geographic points, route primitives, location sharing, map selection and Record markers — **Missing contract or proof for this proposal:** Engineering frames, altitude, geographic polygons/volumes, spatial indexes and multimodal capacity-aware routes

- [ ] **R0929.** [Topology assets](../../../crates/interface/src/native/topology/assets.rs), [splats](../../../crates/interface/src/native/topology/splats.rs), Areas — **Reuse:** Imported geometry, appearance, selection and spatial interaction — **Missing contract or proof for this proposal:** Persistent source-object bindings, city tiling, alignment revisions and permission-aware spatial queries

- [ ] **R0930.** [Workspace policy](../../../crates/interface/src/native/workspace_sync/policy.rs), Organs and property grants — **Reuse:** Scoped collaboration, editing and proposal review — **Missing contract or proof for this proposal:** Prove civic layers and operational field ownership using existing policy mechanisms

- [ ] **R0931.** [Projection](../../../crates/nucleus/src/projection.rs), simulation crate, Rules and Signals — **Reuse:** Time windows, isolated execution, proposals and evidence — **Missing contract or proof for this proposal:** Quantitative resource/capacity propagation and explicitly supported specialist simulation inputs

- [ ] **R0932.** [Interface](../../../crates/interface/src/lib.rs) and native sandbox Castles — **Reuse:** Reusable native presentation and interaction — **Missing contract or proof for this proposal:** Shared operational views and accessible workflows, assembled with the current frontend split

- [ ] **R0933.** The inspected extension field kinds are text, number, boolean, select and multiselect. That alone does not supply a typed resource reference, dimensioned quantity, validity interval or lifecycle constraint. Existing Karma typed values and exact arithmetic should be evaluated before adding another type system.

- [ ] **R0934.** Audit concrete correctness, replay and isolation in the existing simulator before relying on simulation results for operations. This plan does not assume a separate simulation proposal remains present or establishes implemented behavior.

- [ ] **R0935.** `Computer-Vision-Lab.md` (previous path; locate the current merged specification before relying on it) tracks the existing vision-tool direction and defers reconstruction. Keep geometry, appearance and semantic identity separate; live reconstruction is not a prerequisite for stock or civic workflows.

### Reusable base capabilities to add or strengthen

- [ ] **R0936.** Each capability needs a backend contract and a native UI where people must configure, inspect or resolve it. These are proposed requirements; inspect current behavior before deciding which need new code.

### F1. Typed identities, schemas and relationships

- [ ] **R0937.** Give an external identity a source namespace, source object ID and Record binding. Keep source revision and imported asset hash separately from object identity. Names, mesh indexes and scene paths are insufficient identities.

- [ ] **R0938.** Support one Record represented in several models, and a modeled object associated with several Records. Model splitting or merging needs an explicit mapping review rather than automatic identity replacement.

- [ ] **R0939.** Extend configurable forms with typed references, quantities with units, instants/intervals and validated structured values where existing mechanisms cannot express them. Use stable field IDs and bind values to a named schema revision.

- [ ] **R0940.** Describe required fields, relationship cardinality, allowed transitions and permission requirements in bounded definitions. Validation runs in the backend for imports, APIs, UI actions and automations.

- [ ] **R0941.** Keep source-specific properties in namespaced extensions. Promote a concept into shared infrastructure when multiple actual workflows need its behavior, rather than copying every IFC or ERP field into the core.

- [ ] **R0942.** Show source bindings, schema errors and conflicts in a reusable inspector. Allow linking existing Records during import before creating duplicates.

- [ ] **R0943.** **Proof:** importing a renamed object or a reordered scene preserves its Record; two exporters with the same local object ID do not collide; a malformed or unauthorized state transition is rejected through both UI and API.

### F2. Evidence, time, revisions and reconciliation

- [ ] **R0944.** Preserve at least the time a source says an event happened and the time Lince received/recorded it. Add validity intervals where a claim describes a period. Operational queries need both “what was believed then?” and “what do we now know about then?”

- [ ] **R0945.** An observation carries subject, property/value/unit, source identity, source event identity, time, quality and evidence reference. An accepted change links to the observations and actor/policy that accepted it.

- [ ] **R0946.** Keep contradictory observations available according to retention and disclosure policy. Do not average incompatible claims or let the newest timestamp silently choose official truth.

- [ ] **R0947.** Distinguish unknown, stale, unavailable, withheld, disputed and confirmed absent. In public responses, avoid revealing whether an inaccessible private Record exists.

- [ ] **R0948.** Define the authority for each accepted property: for example, an ERP may own accepted stock balances, a survey may own geometry and an Organ may approve public maintenance state. An importer cannot claim authority merely because its data arrived last.

- [ ] **R0949.** Correct accepted operational history with traceable adjustment/compensation events. Undoing a UI edit is different from reversing a dispatched shipment or completed physical transformation.

- [ ] **R0950.** Publish a coherent revision or a declared bounded patch. An “as of” view must expose differing source ages and incomplete coverage rather than implying one perfectly synchronized present.

- [ ] **R0951.** **UI:** source-age badges, evidence timeline, proposed-versus-accepted changes and a conflict/reconciliation queue shared by camera observations, stock counts, imported changes and citizen reports.

- [ ] **R0952.** **Proof:** a late receiving event appears at its effective time without rewriting what was known earlier; a stale camera detection cannot overrule an accepted count; duplicate delivery notifications do not apply twice.

### F3. Resources, quantities, rights and availability

- [ ] **R0953.** Separate a resource specification, fungible lot and individually tracked asset. A material grade, a batch of that grade and a specific machine are different identities. A recipe consumes resources matching constraints, not merely identical display names.

- [ ] **R0954.** Preserve Lince's signed Need/Contribution convention. An unmet Need must not become an accidental negative physical inventory balance. Intent, accepted on-hand stock and projected future stock need distinct meanings.

- [ ] **R0955.** Reuse exact decimal accounting for accepted quantities, commitments and money. Unit conversion needs dimension checks, explicit scale/rounding and versioned conversion data. Unknown density must block an assumed mass-to-volume conversion.

- [ ] **R0956.** Describe stock by the necessary dimensions: resource/lot, location, owner, custodian and status. Owned stock at another location, consigned stock on site and quarantined stock cannot all count as freely available.

- [ ] **R0957.** Reuse existing reservation and balance paths. Specify cancellation, expiry, partial use, released remainder, inbound expectations and time-dependent capacity. Do not subtract the same reservation in two views.

- [ ] **R0958.** Describe responsibilities and rights separately: owner, operator, maintainer, custodian, workspace editor and person allowed to execute a command. A drawn Area or workspace grant does not establish physical ownership or legal title.

- [ ] **R0959.** Keep receipt, movement, consumption, production, count adjustment, quarantine, damage, waste and return as typed causes with evidence. Counters derived from them should reconcile with the current state.

- [ ] **R0960.** **UI:** reusable balances by location/lot/status, reservation detail, receive/move/count forms and a rights/responsibility inspector. Stock presets configure these views.

- [ ] **R0961.** **Proof:** two concurrent reservations cannot spend the same available stock at its authority; a partial delivery preserves the remaining commitment; a movement changes location without changing total accepted quantity; a count adjustment retains its reason.

### F4. Processes, capacity and commitments

- [ ] **R0962.** Describe a bounded process using eligible inputs, outputs, duration, capacity/resource requirements, place, prerequisites, actors, quality gates and evidence required to complete it. Keep a versioned process definition separate from each execution.

- [ ] **R0963.** Model consuming, producing, moving, using and observing a resource as different operations. Reserving a machine for two hours does not consume the machine. Record work in progress, scrap, co-products and losses explicitly.

- [ ] **R0964.** A process records expected and actual results separately. Multi-step work can stop after partial physical completion; do not simulate rollback of consumed materials.

- [ ] **R0965.** Use atomic validated acceptance for changes inside one authority/store. Across organizations, use commitments, handoff receipts and explicit pending/disputed/reconciliation states. Do not promise a single database transaction across a factory, supplier and harbor.

- [ ] **R0966.** Add capacity calendars, maintenance outages and slot reservations on top of existing work/scheduling primitives where possible. Start with explicit feasible schedules; optimization comes later through bounded solver adapters.

- [ ] **R0967.** Lifecycle definitions need visible transitions, actors, preconditions, receipts and cancellation rules. Prefer existing Rule/Command/Transfer mechanisms; add a shared transition validator only when those cannot enforce the same invariant.

- [ ] **R0968.** A reusable traceability relation connects actual input lots to actual output lots and subsequent handoffs. Support recalls and reverse tracing without pretending every relation is a quantity-preserving transformation.

- [ ] **R0969.** **UI:** process form, work board, capacity calendar, execution/evidence detail and exception queue. Recipe, dispatch and construction-checklist presets use the same mechanisms.

- [ ] **R0970.** **Proof:** producing an accepted output cannot silently omit required input accounting; replaying a completion cannot consume inputs again; a quality failure leaves the actual produced lot quarantined; an outage blocks conflicting slots.

### F5. Dependencies and consequences

- [ ] **R0971.** Core [stage 13](13-stage.md) covers bounded declared event/resource arithmetic and verified existing operational-scenario behavior, such as the inventory/yield example below. It does not infer material strength, thermal response, fluid/traffic/equipment behavior or physically accurate process yields from geometry/composition. New physical or specialist forecasting, calibrated uncertainty/ensembles and optimization remain deferred stages [16](16-stage.md)–[21](21-stage.md); preserve known supplied assumptions and mark unsupported consequences incomplete.

- [ ] **R0972.** Give dependency relations explicit meaning: requires, consumes, supplies, reserves, transports, substitutes, depends on capacity, or merely relates to. Graph reachability alone establishes no quantitative consequence.

- [ ] **R0973.** Time and quantity matter. Track lead times, inventory buffers, yields, capacities, availability windows, substitute qualifications and contractual due dates. Cycles require a stopping rule and diagnostics, not endless recursive propagation.

- [ ] **R0974.** Compute impact using a captured accepted base plus named assumptions. Include source freshness and omitted dependencies. Show reasons and the first limiting input/capacity, not just a red downstream node.

- [ ] **R0975.** Begin with deterministic bounded event/resource calculations through supported simulation behavior. Add uncertainty intervals or ensembles after defining what they mean; do not attach invented probability percentages to unknown inputs.

- [ ] **R0976.** Keep proposals for mitigation separate from commitments. Selecting an alternative supplier in a scenario does not reserve its material or place an order.

- [ ] **R0977.** **Worked example, illustrative quantities:** a factory has 30 tonnes of qualified mineral available and consumes 10 tonnes per day. An inbound supply is delayed five days. With no substitute, maintenance interruption or other demand, its buffer covers three days; the scenario predicts a two-day input gap. At a declared yield of 0.8, planned output is 8 tonnes per operating day. Actual customer delays additionally depend on finished stock, priorities, transport, receiving windows and accepted commitments. Missing those inputs must make the customer-impact result incomplete.

- [ ] **R0978.** **UI:** linked dependency graph, time plot, limiting-reason panel and baseline-versus-alternative comparison. Every result links to assumptions and evidence.

- [ ] **R0979.** **Proof:** changing a buffer changes the predicted stop time; an unrelated relation does not propagate a shortage; an unqualified substitute is excluded; unavailable private supplier data is shown as incomplete without leaking its details.

### F6. Shared queries, projections and publication

- [ ] **R0980.** Provide permission-aware filtering, sorting, grouping, aggregation, pagination and bounded subscriptions over typed properties, relationships, space and time. Operational views should share their query definitions rather than fetch every Record and filter on the client.

- [ ] **R0981.** Incrementally maintain or cache expensive projections with explicit revisions and invalidation. A cached aggregate is subject to the same access policy as its inputs and can require its own disclosure policy.

- [ ] **R0982.** Reuse a selected Record, filter and time window across a table, calendar, network view and map. Store view presets separately from accepted facts. Display unsupported predicates instead of returning a misleading empty result.

- [ ] **R0983.** Public publication is an intentional projection: selected fields, granularity, time delay, license, provenance, permitted aggregation and update policy. Public availability need not disclose exact stock, camera footage, supplier contracts or individual locations.

- [ ] **R0984.** Support machine-readable offers, requirements and accepted public events alongside human views. Choose a minimal paginated API and export format first, with stable identifiers, revision markers and bounded incremental retrieval.

- [ ] **R0985.** Test private references, spatial indexes, tile metadata, counts, caches and aggregate inference as disclosure paths. Filtering the Record inspector alone is insufficient.

- [ ] **R0986.** **UI:** shared filter/group controls, table/forms, publish preview and a source/coverage summary. People should see exactly which fields and geometry a public workspace exposes.

- [ ] **R0987.** **Proof:** private stock never appears through a public map, count, search or stale cache; a public buyer can inspect terms and initiate an order without learning the producer's private process.

### F7. External systems and automation

- [ ] **R0988.** An adapter declares its identities, supported read/write operations, units, source authority, schema mapping, freshness, credentials and failure behavior. Initially support import/read/propose; introduce execution only under a defined policy.

- [ ] **R0989.** Reuse existing capability and operation paths for execution. Separate observing, proposing, approving, publishing, reserving and physically commanding. Give each operation bounded scope and an application receipt.

- [ ] **R0990.** Handle duplicate events, reorderings, source resets, revisions, disconnects and conflicts explicitly. Durable deduplication should use source namespace plus event identity; reconnecting must reconcile a checkpoint before accepting an apparently continuous stream.

- [ ] **R0991.** Choose one owner for each accepted field. When both systems can edit, define conflict handling and origin markers to prevent echo loops. Do not overwrite an external update with a stale Lince export.

- [ ] **R0992.** Remote side effects can have unknown outcomes after a timeout. Mark them pending/unknown, inspect the remote receipt/state and retry only when the protocol supports safe deduplication. Do not claim exactly-once execution across arbitrary external systems.

- [ ] **R0993.** Give automations debounce/hysteresis where relevant, rate/budget limits, simulation preview, expected revisions, cancellation, audit and revocation. Fiote can interpret evidence and propose operations; it does not bypass these contracts.

- [ ] **R0994.** Keep native device safety/interlocks in the responsible equipment system. A Lince Area, camera inference or simulation result must not become a replacement for a certified physical control system.

- [ ] **R0995.** **UI:** connector mapping/setup, live source health, proposals and an execution/receipt queue. A timeout must be distinguishable from a rejection or completed action.

- [ ] **R0996.** **Proof:** reconnection cannot double-receive stock; changing an ERP row creates a reviewable conflict; revocation stops future commands; imported text cannot become privileged automation instructions.

### Interoperability: choose by purpose

- [ ] **R1024.** These are researched integration candidates, not claims of existing Lince support. Standard scope does not guarantee exporter/importer conformance. Select one real partner dataset, declare the supported subset and test it before expanding.

- [ ] **R1032.** Industrial information exchange — **Candidate and documented role:** [OPC UA](https://opcfoundation.org/about/opc-technologies/opc-ua/) supports industrial interoperability — **Proposed boundary in Lince:** Use a scoped gateway adapter; read/propose first, separately authorize commands.

- [ ] **R1033.** Supply-chain event exchange — **Candidate and documented role:** [GS1 EPCIS](https://ref.gs1.org/standards/epcis/2.0.1/) defines visibility-event information, including object and transformation events — **Proposed boundary in Lince:** Map external event identities, lots, places and transformations; operational acceptance and contractual settlement remain explicit.

- [ ] **R1035.** These recommendations infer useful boundaries from the standards' documented scopes. They are not an assertion that adopting a standard implements the workflow. Geometry, operational events and simulation exchange remain separate integrations.

- [ ] **R1036.** External authoring tools can supply exported data with identity/semantic manifests, or a scoped live connector when a real workflow needs it. Imported geometry does not automatically preserve source scripts, shaders or behavior.

- [ ] **R1037.** Start with the least integration that proves a workflow: a versioned model export and event file/API from one source. Add an editor plugin only when stable IDs or useful round-trip changes cannot be preserved through that export. Start with one-way ingestion; prove reconciliation before bidirectional writeback.

- [ ] **R1038.** Imported assets, models, map data, camera data and derived exports need their own rights/attribution metadata. Embedded dependencies require LICENSE and available credits in their Sand. Publicly reachable data is not automatically reusable or redistributable data.

## Fixture specifications owned here

### Owned fixture specifications

- [ ] **R0739.** B14 — **Fixture:** Supplier/stock/production/partial delivery and a civic maintenance proposal — **Proof required:** Exact accounting, rights, reconciliation, selective publication and scenario isolation

## Workflow, correctness and performance evidence

- [ ] **Real workload.** Use the proposed 10,000 operational Records as meaningful lots, reservations, processes, deliveries and maintenance evidence. Actually receive/reserve/produce/query/review with partial data and a reconnect; a table that merely paints that many rows is not sufficient.

- [ ] **R0569.** **[stage 13](13-stage.md) — L1/L2/L3/L4/L5** — **Intention and defined deliverable:** Shared operations/civic/civil workflows with accepted quantity/evidence/authority contracts — **Correctness/failure evidence and performance obligation:** B14/B19/B20: competing reservation, late/duplicate events, publication, isolated bounded resource scenarios and reconnect. Measure commit/query/subscription on the 10,000-Record candidate workload; detailed geometry and new thermal/material/physical forecasts are not prerequisites.

- [ ] Run the actual native/backend/persistence/transport path for every 13.M milestone, including the supported failure and permission cases. Retain fixture/input revisions, command receipts/output identity and a reproducible run recipe; inspect native appearance/focus/input where relevant. A standalone example, empty adapter/UI, helper test or synthetic draw-count benchmark cannot complete a workflow.

- [ ] Record named hardware/build/profile/source rights, cold/warm/offline conditions, visible input/commit/frame percentiles, applicable query/job/error results, CPU/GPU/disk/transfer/queue/staging peaks and last-demand release. Apply the stage's agreed limits, including transient old/new products; resolve failures or document a specifically owner-reviewed measured profile change before claiming success.

- [ ] Run only checks justified by the changed contracts through repository cargo xtask/Nix workflows and `cargo check` with warnings as errors. If a schema change is required, append a new migration/checksum and run the store check/migration guards; never rewrite an old migration. Keep old web code and unrelated changes untouched.

## Advance gate

- [ ] **R0666.** factory and civic flows use the same backend invariants, work without detailed 3D assets and do not double-account resources or turn an observation/simulation into an accepted transaction.

- [ ] Advance only when **all 13.M workflow milestones**, the ordered implementation scope, applicable contract/fixture requirements, native interaction and failure/resource evidence pass together. A task checkbox without its user outcome does not count. Explicitly list unresolved/unsupported/deferred scope; keep this stage blocked/pending when any required outcome fails.

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
