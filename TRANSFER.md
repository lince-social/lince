# Transfer

This document is a design note for the complete Transfer feature. It starts from the idea that Lince Records represent Needs and Contributions, and that a Transfer is the structured process by which one or more parties agree to move quantities, responsibilities, information, or plans between those Records.

For now, Transfer should deal only with Lince data. No payment processors, delivery providers, calendars, external signatures, or external messaging integrations are assumed. Those can come later if the internal model is solid.

## Core Idea

A Transfer is not just a row in a table. It is a shared process with state, participants, items, conditions, evidence, and history.

The simplest useful Transfer is:

1. One party frames a Record as a Need or a Contribution.
2. Another party proposes a matching Contribution or Need.
3. The parties agree on quantities, timing, location or context, and acceptance criteria.
4. The agreed quantities become reserved in simulation.
5. The parties perform the action.
6. Each party confirms the outcome.
7. Lince applies the final Record quantity changes and keeps the signed/auditable history.

The important shift is this: Transfer should not mean "move quantity immediately". It should mean "create a structured agreement that may later move quantity".

## Main Flaws In The Current Plan

### Need and Contribution are contextual

The same Record can be framed as a Need or a Contribution depending on intent. "I want to play guitar" can be a Need for an audience or a Contribution of performance. This is powerful, but it creates ambiguity.

The flaw is treating Need and Contribution as intrinsic properties of a Record. They are not. They are roles inside a Transfer.

Better model:

- A Record describes a thing, state, capability, intention, or resource.
- A Transfer item says how that Record participates in this Transfer.
- The same Record can be a need-side item in one Transfer and a contribution-side item in another.
- The framing should be explicit per Transfer item: `need`, `contribution`, `exchange`, `support`, `reservation`, `information`, or `task`.

### Quantity is not enough

Quantity works for apples. It is weak for services, promises, knowledge, permissions, access, transportation, or tasks.

The flaw is assuming every Transfer can be represented by a single number moving from one Record to another.

Better model:

- Keep `quantity` because it is central to Lince.
- Add unit metadata: apples, hours, seats, files, kilograms, messages, tasks, visits.
- Add quality/acceptance metadata: grade, condition, constraints, expected result, failure rules.
- Support non-quantity items where the transferred thing is a state change, confirmation, assignment, or piece of information.

### Agreement is not binary

The notes have first agreement, second agreement, transfer confirmation, and times. Real agreements have revisions, counteroffers, partial acceptance, cancellation, expiration, failed delivery, and disputes.

The flaw is encoding agreement as a few booleans on an item.

Better model:

- Agreement should be event based.
- Every important action appends a Transfer event.
- The current status is derived from the event log.
- A Transfer can be amended only by creating a new revision and collecting the required confirmations again.

### Cross-cell truth is hard

If Alice's Cell and Bob's Cell both have local databases, they can disagree. Alice may think a Transfer is agreed while Bob's server never saw the acceptance. Both may reserve the same apples for different Transfers. One party may go offline. A server may disappear.

The flaw is assuming a shared state exists without defining who owns it.

Better model:

- Each Transfer has one coordinator for ordering events.
- All parties keep signed copies of the event log.
- The coordinator can be a personal Cell, an Organ server, or a temporary rendezvous server.
- Other Cells can verify state by replaying signed events, not by blindly trusting the coordinator's current database.

### Automation can create false commitments

Karma can create Transfer proposals automatically. That is useful, but dangerous. If Karma can silently agree, reserve, or execute, Lince may create commitments people did not understand.

The flaw is treating an automated proposal like a human agreement.

Better model:

- Karma may draft and publish proposals.
- Karma may reserve only under explicit policy.
- Karma may agree only when the user has pre-authorized a bounded rule.
- Every automated action must include the rule id, input state, simulation state, and expiry.
- Automation should default to "proposal", not "commitment".

### Public Needs can leak sensitive metadata

Making a Need visible can reveal shortage, location, schedule, personal priorities, social relations, business demand, or future plans.

The flaw is thinking only about the Record content and not the Transfer metadata.

Better model:

- Visibility is part of every Transfer and every Transfer item.
- Public discovery can use redacted summaries.
- Exact quantities, deadlines, locations, and counterparties can stay private until a proposal is accepted.
- A Cell should support progressive disclosure: self, trusted Cells, Organ, public.

### Simulation can diverge from reality

The apple donation example shows two quantities: real apples and apples available for own use. This is necessary, but it creates a problem. Multiple simulations can reserve the same surplus, and real events can invalidate planned Transfers.

The flaw is not distinguishing actual state from planned state.

Better model:

- Actual quantity is what Lince believes exists now.
- Reserved outgoing quantity is promised to Transfers.
- Reserved incoming quantity is expected from Transfers.
- Available quantity is actual minus outgoing reservations plus accepted incoming reservations, depending on view.
- Simulated quantity is produced by running a scenario over actual state plus proposed, agreed, and predicted Transfers.

## Real-Life Transfer Lifecycle

### 1. Intent

A party creates an intent from a Record.

Examples:

- "I need 14 apples by Friday."
- "I can contribute 10 apples this week."
- "I can perform guitar at an event."
- "I need someone to break this project into smaller Needs."
- "I can help unblock a chain of Transfers by contributing transport."

Intent is not yet a Transfer. It is discoverable material that can become one.

Required data:

- Source Record id.
- Framing: Need or Contribution.
- Quantity and unit, if relevant.
- Visibility.
- Expiration.
- Preferred counterparties or Organs.
- Constraints.

### 2. Discovery

Other parties find the intent through a Cell, Organ, search view, shared board, direct link, or Karma rule.

Discovery should allow multiple possible responses. If a Need is public, many Contributions may compete or combine. If a Contribution is public, many Needs may want part of it.

Important rule: discovery should not mutate Records. It only creates candidate links.

### 3. Proposal

A Transfer proposal is created when one or more intents are connected.

The proposal should say:

- Who is involved.
- Which Records are involved.
- Which side each Record plays.
- Quantities and units.
- Timing.
- Location or context.
- Acceptance criteria.
- Visibility and privacy rules.
- Expiration.
- Whether partial fulfillment is allowed.
- Whether substitutes are allowed.
- Which Cell or Organ coordinates the event log.

At this stage, the proposal can still be cheap and exploratory.

### 4. Negotiation

Negotiation creates revisions. A revision is a complete version of the proposed Transfer terms.

Avoid mutable agreement fields. Instead:

- Revision 1: Alice proposes.
- Revision 2: Bob counters with a lower quantity.
- Revision 3: Alice accepts Bob's quantity but changes the delivery window.
- Revision 4: Bob accepts.

Only one revision can be the active candidate. Older revisions remain in history.

### 5. Agreement

A Transfer is agreed when the required parties have accepted the same revision.

For two parties, this is simple: both accepted revision N.

For many parties, agreement needs a policy:

- `all_parties`: every listed participant must accept.
- `threshold`: at least N of M participants must accept.
- `role_required`: every required role must accept, optional observers do not.
- `organ_policy`: the Organ defines its own quorum.

Agreement should freeze the terms. After agreement, changes require a new revision and new acceptance.

### 6. Reservation

After agreement, Lince reserves affected quantities in simulation.

Reservation means:

- The real Record quantity does not change yet.
- The available quantity changes for planning.
- Other Transfer proposals can see that the quantity is already committed.
- The user can still override the reservation, but Lince should show the consequence.

Reservation needs priority. A Transfer that is agreed should outrank a draft. A Transfer with a deadline may outrank a low-priority recurring donation. A user should be able to manually reprioritize.

### 7. Action

The parties do the real-world action or the Lince-only action.

Because this version is Lince-only, "action" means Lince can record:

- A task was marked done.
- A Record quantity was changed.
- A Record was linked, assigned, copied, imported, or exposed.
- A party claims delivery.
- A party claims receipt.
- A party attaches Lince evidence, such as notes, linked Records, comments, or history rows.

No external proof is assumed.

### 8. Confirmation

Confirmation should not be a single global boolean. Different parties confirm different facts.

Examples:

- Contributor confirms "I delivered".
- Receiver confirms "I received".
- Receiver confirms "I accept quality".
- Contributor confirms "I accept the settlement".
- Organ confirms "this complied with local policy".

The Transfer settles when the confirmation policy is satisfied.

### 9. Settlement

Settlement applies the final Lince data changes:

- Move or adjust Record quantities.
- Create History rows.
- Create Record links.
- Close or update related Needs.
- Create follow-up Records if the Transfer generates new work.
- Mark reservations as consumed.
- Emit events for subscriptions and Karma.

Settlement should be idempotent. Replaying the same settled Transfer must not apply the quantity change twice.

### 10. Aftermath

After settlement, Lince can help the parties learn:

- Was the Transfer late?
- Was it partially fulfilled?
- Did it unblock dependent Transfers?
- Did it create new Needs?
- Should a recurring Karma rule be adjusted?
- Should trust metadata change?

This should stay as Lince data. Avoid building reputation as a global score too early. A local trust note or Organ-specific reliability view is safer than a universal rating.

## State Machine

Suggested Transfer states:

| State | Meaning |
| --- | --- |
| `draft` | Local idea, not visible to others yet. |
| `published` | Visible as an intent or open proposal. |
| `proposed` | A specific counterparty or group has been invited. |
| `negotiating` | At least one counterproposal exists. |
| `agreed` | Required parties accepted the same revision. |
| `reserved` | Simulation has reserved the relevant quantities. |
| `in_action` | The parties are performing the agreed action. |
| `delivered` | Contributor claims delivery or completion. |
| `received` | Receiver claims receipt. |
| `accepted` | Receiver accepts the delivered result. |
| `settled` | Lince has applied the final data changes. |
| `cancelled` | Transfer ended before settlement by valid cancellation. |
| `expired` | Transfer ended because an expiry condition was reached. |
| `disputed` | Parties disagree about state, delivery, or settlement. |
| `superseded` | A newer Transfer or revision replaced this one. |

The state should be derived from events when possible. Stored status can exist as a cache, but the event log is the source of truth.

## Data Model Direction

The current `transfer` and `transfer_item` tables are too small for the complete feature. They can be a seed, but the real model probably needs multiple tables.

### transfer

The transfer header.

Useful fields:

- `id`
- `local_uuid`
- `coordinator_cell_id`
- `coordinator_organ_id`
- `title`
- `purpose`
- `status_cache`
- `visibility`
- `created_by_party_id`
- `created_at`
- `updated_at`
- `expires_at`
- `settled_at`
- `active_revision_id`
- `settlement_event_id`
- `freestyle_data_structure`

### transfer_party

A participant in the Transfer.

Useful fields:

- `id`
- `transfer_id`
- `party_kind`: user, Cell, Organ, public role, external placeholder.
- `local_user_id`
- `organ_id`
- `cell_id`
- `display_name_snapshot`
- `role`: proposer, contributor, receiver, coordinator, observer, mediator.
- `required_for_agreement`
- `required_for_settlement`
- `public_key_snapshot`, later if signed events are added.

### transfer_revision

A complete version of the terms.

Useful fields:

- `id`
- `transfer_id`
- `revision_number`
- `created_by_party_id`
- `created_at`
- `message`
- `terms_hash`
- `expires_at`
- `freestyle_data_structure`

The revision should be immutable once proposed.

### transfer_item

An item inside a revision.

Useful fields:

- `id`
- `transfer_revision_id`
- `record_id`
- `record_cell_id`
- `record_organ_id`
- `record_head_snapshot`
- `record_body_snapshot`
- `side`: need, contribution, exchange, support, information, task.
- `direction`: incoming, outgoing, mutual, informational.
- `quantity`
- `unit`
- `quantity_policy`: exact, up_to, at_least, range, all_available, surplus_over.
- `quality_terms`
- `acceptance_criteria`
- `partial_allowed`
- `substitution_allowed`
- `simulation_policy`
- `reservation_priority`
- `freestyle_data_structure`

Snapshots matter because the source Record can change after the proposal. The Transfer needs to remember what was agreed at the time.

### transfer_event

The append-only history.

Useful fields:

- `id`
- `transfer_id`
- `revision_id`
- `event_type`
- `actor_party_id`
- `created_at`
- `payload_json`
- `previous_event_hash`
- `event_hash`
- `signature`, later.
- `received_from_cell_id`, later.

Event types:

- `created`
- `published`
- `invited`
- `revision_proposed`
- `revision_accepted`
- `revision_rejected`
- `reservation_created`
- `reservation_released`
- `delivery_claimed`
- `receipt_claimed`
- `quality_accepted`
- `settlement_requested`
- `settled`
- `cancel_requested`
- `cancelled`
- `expired`
- `disputed`
- `message`
- `metadata_changed`

### transfer_reservation

The simulation hold on Record quantities.

Useful fields:

- `id`
- `transfer_id`
- `revision_id`
- `transfer_item_id`
- `record_id`
- `direction`
- `quantity`
- `state`: planned, reserved, consumed, released, invalidated.
- `priority`
- `created_at`
- `expires_at`
- `consumed_at`

### transfer_dependency

A Transfer can depend on other Transfers.

Useful fields:

- `transfer_id`
- `depends_on_transfer_id`
- `dependency_kind`: must_settle, must_agree, should_settle, blocks, alternative_to.
- `quantity_effect`

This is where the "unclogging Transfers" idea becomes concrete.

## Metadata Beyond Date And Location

Date and location are not enough. A Transfer needs metadata that makes the social agreement legible.

Important metadata:

- Purpose: why this Transfer exists.
- Framing: Need, Contribution, exchange, support, donation, task assignment, information request.
- Parties and roles.
- Coordinator: which Cell or Organ orders the event log.
- Visibility: private, invited, trusted Cells, Organ, public.
- Disclosure rules: what is public before agreement and after agreement.
- Expiration: when the proposal dies.
- Delivery window: earliest and latest expected action time.
- Recurrence: whether this Transfer is one-off or generated by a repeating Karma.
- Units and quality terms.
- Acceptance criteria.
- Partial fulfillment policy.
- Substitution policy.
- Cancellation policy.
- Dispute policy.
- Reservation priority.
- Simulation policy.
- Dependency list.
- Source Karma rule, if automated.
- Source view/search, if discovered from a view.
- Record snapshots.
- Terms hash.
- Human notes.
- Audit visibility: who can inspect history.

## Consensus

Consensus does not need to mean blockchain or global agreement. For Lince, consensus should mean: the involved parties can prove they accepted the same Transfer terms and can replay the same event log to reach the same state.

The practical model:

1. A Transfer has a coordinator.
2. The coordinator orders events.
3. Each event references the previous event hash.
4. Each actor signs or at least authors their own events.
5. Each Cell stores the events it has seen.
6. The current state is derived by a deterministic reducer.
7. A state transition is valid only if the required events exist.

For the first implementation, signatures can be deferred. The shape should still be event-log-friendly so signatures can be added later without redesigning everything.

### Agreement consensus

A revision is agreed when all required agreement parties accepted the same `revision_id`.

If Alice accepts revision 3 and Bob accepts revision 4, there is no agreement.

If an Organ policy requires a coordinator approval, user acceptance alone is not enough.

### Settlement consensus

Settlement should require a separate policy. Agreement says "we intend to do this". Settlement says "we believe this happened and Lince may mutate final data".

Possible settlement policies:

- Contributor delivery plus receiver acceptance.
- Receiver acceptance only.
- All required parties confirmation.
- Coordinator confirmation.
- Time-based auto-settle after no dispute.
- Karma-authorized auto-settle for low-risk local Transfers.

### Conflict handling

Conflicts should become visible data, not hidden errors.

Examples:

- Two accepted Transfers reserve the same quantity.
- A Record quantity changes and invalidates a reservation.
- One Cell receives events out of order.
- A party accepts a revision that was already superseded.
- A coordinator disappears.
- Two coordinators claim authority for the same Transfer.

Recommended behavior:

- Keep all events.
- Mark invalid or conflicting events as rejected by the reducer.
- Show the reason.
- Allow a new revision or cancellation to resolve the conflict.

## Cells, Organs, And Servers

Lince should not require one central global server. But real-world Transfers need some place where parties can discover each other, exchange events, and resolve ordering.

### Option 1: One Cell only

Everything happens inside one local Lince database.

Good for:

- Personal planning.
- Simulation.
- Local automation.
- Transfers between Records owned by the same user.

Bad for:

- Multi-party agreement.
- Cross-Cell trust.
- Public discovery.

### Option 2: Organ server as coordinator

An Organ server hosts the Transfer event log and acts as source of truth for a community, group, cooperative, company, or market.

Good for:

- MVP.
- Simple mental model.
- Public or Organ-local discovery.
- Shared moderation.
- Easier conflict resolution.

Bad for:

- Server dependency.
- Organ operators see metadata.
- If the server disappears, live coordination stops unless clients have mirrored logs.

This should probably be the first real implementation for cross-user Transfers.

### Option 3: Start on server, continue directly

A server introduces parties and coordinates the first proposal. Once both Cells know each other's endpoints and have the Transfer event log, they can sync directly.

Good for:

- Discovery through an Organ.
- Reduced server dependency after matching.
- More private follow-up negotiation.

Bad for:

- Harder networking.
- NAT, offline clients, and identity rotation make this harder.
- Direct sync still needs conflict rules.

This is a good second phase.

### Option 4: Federated source of truth

Each party's Cell stores and serves its own signed events. No single server is fully authoritative; the Transfer state emerges from the merged event logs.

Good for:

- Resilience.
- Data sovereignty.
- Cross-Organ Transfers.

Bad for:

- More complex consensus.
- Harder UX when parties disagree.
- Event ordering and revocation need careful rules.

This is the long-term direction, but not the MVP.

### Recommendation

Use a coordinator-per-Transfer model.

For MVP:

- The coordinator is the local Cell or one Organ server.
- The coordinator database is the operational source of truth.
- Parties can export/import or mirror the event log.

For later:

- Add signed events.
- Add direct Cell-to-Cell sync.
- Allow coordinator migration if the original server is gone.

## Simulation

Simulation is the feature that makes Transfers larger than simple transactions.

A Record should be able to show multiple quantity views:

| Quantity | Meaning |
| --- | --- |
| Actual | Current known real quantity. |
| Reserved outgoing | Quantity promised to agreed Transfers. |
| Reserved incoming | Quantity expected from agreed Transfers. |
| Proposed outgoing | Quantity requested by draft/proposed Transfers. |
| Proposed incoming | Quantity expected from draft/proposed Transfers. |
| Available | Actual minus relevant outgoing holds. |
| Planned | Actual plus incoming minus outgoing across selected scenario. |
| Surplus | Quantity above a user-defined threshold. |

Example:

- You have 30 apples.
- Karma says donate all apples above 20.
- A donation Transfer reserves 10 outgoing apples.
- Actual is 30.
- Available for own use is 20.
- Donation capacity is 10.
- If you eat one apple, actual becomes 29.
- Depending on policy, either own-use availability becomes 19 and donation stays 10, or donation drops to 9 and own-use availability stays 20.

That policy must be explicit. Good names:

- `protect_reservation`: real changes reduce unreserved quantity first.
- `surplus_reservation`: real changes reduce reserved surplus first.
- `proportional`: real changes reduce all planned uses proportionally.
- `manual`: user must resolve the conflict.

### Predictive Transfers

The farmer/tool-maker example needs predicted state.

Lince can simulate:

- Frequency rows reducing or increasing quantities.
- Karma rules triggering future Transfer proposals.
- Agreed Transfers settling in the future.
- Dependent Transfers unlocking other Transfers.

This creates a planning graph:

- Record states are nodes.
- Transfers are edges.
- Karma rules create conditional edges.
- Time moves the graph forward.

The output should not be "this will happen". It should be "under these assumptions, this chain becomes possible on this date".

## Karma Interaction

Karma should be allowed to create Transfer proposals, but it must be bounded.

Useful Karma actions:

- Draft a Transfer when a Record crosses a threshold.
- Publish a Need to a wider visibility after no response.
- Invite a trusted Cell when a Need appears.
- Reserve surplus for donation.
- Cancel expired proposals.
- Escalate visibility from self to Organ to public.
- Run simulation and warn about blocked chains.
- Suggest contributing to a dependency that unlocks later Transfers.

Risky Karma actions:

- Agreeing on behalf of a user.
- Settling a Transfer.
- Publishing sensitive Needs.
- Reserving scarce resources.
- Creating loops of Transfers.

Policy:

- Karma-generated Transfers should record `source_karma_id`.
- Karma should include a snapshot of the condition that fired.
- User-authorized automation should have limits: max quantity, allowed counterparties, max frequency, visibility cap, expiry, and settlement policy.
- Karma loops should be visible in the Transfer dependency graph.

## User Experience

The core UX should not start as a complex market screen. It should start from Records.

### From a Record

The user should be able to choose:

- "I need this"
- "I can contribute this"
- "Offer surplus"
- "Request contribution"
- "Create Transfer"
- "Simulate Transfers"

The Transfer creation flow should ask only the minimum:

- Quantity or scope.
- Counterparty or visibility.
- Timing.
- Acceptance criteria.
- Whether to reserve on agreement.

Advanced metadata can live behind details.

### Transfer view

A Transfer page should show:

- Current state.
- Parties and roles.
- Items on each side.
- Agreement progress.
- Reservation impact.
- Timeline of events.
- Dependencies.
- Simulation preview.
- Available actions for the current user.

The most important UI principle: show what is committed, what is only proposed, and what is simulated.

### Discovery view

Discovery should support:

- Needs looking for Contributions.
- Contributions looking for Needs.
- Suggested matches.
- Alternative proposals.
- Dependency chains.
- Visibility filters.
- Trusted Cells and Organs first.

### Simulation view

Simulation should answer:

- What will I have if all agreed Transfers settle?
- What will I have if proposed Transfers settle?
- What Transfer is blocking this plan?
- Which Need could I help with to unblock a chain?
- What is reserved, and why?

## Privacy And Trust

Transfers create social metadata. Lince should treat that as sensitive.

Privacy needs:

- Redacted public intents.
- Private negotiation.
- Role-based visibility for Transfer history.
- Ability to hide counterparties until proposal.
- Ability to reveal exact location only after agreement.
- Ability to share simulation results without sharing all Records.

Trust needs:

- Local notes about counterparties.
- Organ-specific reliability views.
- History of settled, cancelled, expired, and disputed Transfers.
- No global reputation score at first.

Global reputation is tempting but dangerous. It tends to flatten context, invite abuse, and punish people for local constraints. Lince should start with local/Organ trust data and let users decide what it means.

## Disputes And Failure

Transfers must model failure explicitly.

Common failure modes:

- No response.
- Counterparty disappears.
- Quantity changed before agreement.
- Delivery did not happen.
- Delivery happened but quality was rejected.
- Partial delivery.
- Duplicate reservation.
- Coordinator server offline.
- Organ policy violation.
- One party wants to cancel after agreement.

The system should support:

- Expiry.
- Cancellation request.
- Mutual cancellation.
- Unilateral cancellation with reason.
- Dispute state.
- Partial settlement.
- Follow-up Transfer.
- Manual override by local owner.

Do not hide failure. Failed Transfers are useful data for planning.

## Bigger Direction

The higher dream is not just "exchange apples". It is a distributed planning layer for Needs and Contributions.

Possible future features:

- Transfer chains: many small Transfers compose into one larger plan.
- Dependency investing: help someone else's Need because it unlocks your future Need.
- Public commons: Contributions offered to any matching Need under rules.
- Transfer templates: repeatable protocols for common workflows.
- Conditional offers: "I will contribute X if at least three others contribute Y."
- Group agreement: Organs agreeing to plans, not just individuals trading.
- Scenario markets without money: compare plans by feasibility, trust, timing, and resource flow.
- Mutual aid routing: find the smallest contribution that unblocks the largest chain.
- Simulation snapshots: share "this plan works if these Transfers settle".
- Local economic weather: show shortages, surplus, bottlenecks, and predictable future demand inside an Organ.

The strongest version of Transfer makes Lince a tool for collective coordination, not just a CRUD table for agreements.

## MVP Recommendation

Build the smallest version that preserves the future shape.

MVP scope:

1. Transfers are local or coordinated by one Lince server.
2. Transfer has parties, items, revisions, events, and status.
3. Agreement requires all required parties accepting the same revision.
4. Settlement requires explicit confirmation.
5. Reservations affect simulation, not actual quantity.
6. Settlement mutates Record quantities once, idempotently.
7. Karma can create proposals but cannot silently settle.
8. Visibility is basic: private, invited, Organ, public.
9. Transfer event history is append-only.

Do not build first:

- Global reputation.
- External payments.
- External delivery tracking.
- Full peer-to-peer sync.
- Cryptographic signatures, unless identity work is already happening.
- Complex legal contract language.

But design the tables so those can fit later.

## Concrete First Implementation Shape

A practical first schema could be:

- `transfer`
- `transfer_party`
- `transfer_revision`
- `transfer_item`
- `transfer_event`
- `transfer_reservation`
- `transfer_dependency`

The reducer should calculate:

- Current state.
- Active revision.
- Agreement progress.
- Settlement progress.
- Reservation state.
- Conflicts.

The UI should expose:

- Create Transfer from Record.
- Propose revision.
- Accept/reject revision.
- Reserve after agreement.
- Mark delivered.
- Mark received/accepted.
- Settle.
- Cancel/expire.
- View simulation impact.

This is enough to make Transfers real without pretending all hard distributed problems are solved.

## Design Principle

Transfer should be a protocol for making Record changes socially valid before they become database changes.

Records describe the world as a Cell understands it. Transfers describe how people intend to change that world together. Karma can suggest and automate parts of that process, but consensus, visibility, simulation, and failure must be first-class data.
