# Transfer

This is the main tracker for the Transfer feature.

The detailed notes live in:

- [Visibility](transfer-visibility.md)
- [Simulation And Settlement](transfer-simulation-settlement.md)
- [Intent And Discovery](transfer-intent-discovery.md)
- [Networking And Sync](transfer-networking.md)

## MVP Build Order

Recommended first implementation order:

1. Transfer header with quantity and immutable agreement/settlement modes.
2. Transfer parties.
3. Transfer items with Transfer-specific title/description and source Record snapshots.
4. Transfer interactions and dependencies.
5. Transfer event log and validator.
6. Coordinator-backed event sync cursor for participating Cells.
7. Agreement levels with edit invalidation for connected items.
8. Messages.
9. Karma Transfer quantity tokens.
10. Transfer quantity influence facts.
11. Delivery/receipt confirmations.
12. Individual/full settlement.
13. Server-backed Transfer sand contract and typed backend actions.
14. Transfer sand list/detail/create/agreement UI.
15. Basic peer/contact table and Transfer discovery cache.
16. Optional SQL views or sand queries for richer quantity projections.
17. Visibility subjects, rules, and fields across all Transfer surfaces.

## Not First

- External integrations.
- Global reputation.
- Full peer-to-peer federation.
- Hardcoded expiration.
- Role-based agreement.
- Complex legal-contract language.
- Field-level visibility. Visibility subjects, rules, and field filtering are intentionally last so the Transfer shape can settle first.

## Cross-File Map

- Product shape, core assumptions, agreement, events, messages, and the main checklist live in this file.
- Visibility tables and UI behavior live in [Visibility](transfer-visibility.md).
- Karma activation is implemented in this tracker.
- Remaining reversal/dispute settlement work lives in [Simulation And Settlement](transfer-simulation-settlement.md).
- Intent, discovery, proposal editing, and state model live in [Intent And Discovery](transfer-intent-discovery.md).
- Remaining peer discovery and future federation work lives in [Networking And Sync](transfer-networking.md).

## Core Model

Transfer is a protocol for making Lince Record quantity changes and Record relationships socially valid before they become final database changes.

Current principles:

- Transfer deals only with Lince data for now.
- Quantity stays central and generic. Specialized units can be represented by metadata/extensions later.
- A Transfer is a structured promise before execution, and may group smaller item-level interactions.
- Records do not permanently become Needs or Contributions. Need, Contribution, support, task, information, and reservation are Transfer item roles.
- A Transfer item carries Transfer-specific title/description and may hide the private source Record fields.
- Parent Transfers group child Transfers without forcing one agreement or settlement policy on every child.
- Counteroffers are edits. Agreement returns only when the relevant parties accept the edited state again.
- Expiration is not hardcoded in Transfer for the first version; Karma can activate or neutralize Transfers by quantity.
- Role-based agreement, legal-contract language, external payments, delivery integrations, calendars, and external messaging are out of MVP scope.

The intended flow remains:

1. Someone creates a Transfer from one or more Records.
2. Transfer items describe what those Records mean in this Transfer.
3. Visibility decides which party or Organ can see which fields.
4. Parties edit the parts they are allowed to edit.
5. Connected edits invalidate earlier agreement for affected items/interactions.
6. Parties raise agreement levels when they accept the current visible state.
7. Satisfied agreement policy activates the relevant item, interaction, or group.
8. Transfer influence appears in simulation as plus/minus quantity.
9. Parties confirm delivery and receipt.
10. Settlement applies the actual Lince data changes.

## Implemented Shape

Transfer currently has two backend layers:

- The current widget projection, still used by the Transfer sand.
- A structured, UI-agnostic data model for Transfer services and Transfer packages.

The current widget projection is still present while the sand moves to structured projections:

- `transfer` is still the minimal header with `id` and `quantity`.
- Transfer metadata still lives in `transfer_identity`.
- The current sand still reads one contribution side plus one need side from `transfer_item`.
- Parent/child grouping and dependency-capable edges still use `transfer_relation`.
- Tree behavior uses `transfer_tree_config`, including branch mode, record sync mode, source record, sync role, sync quantity, sync counterparty, target Organ, and live/copy sync state.
- Event sync uses signed package import/export and cursor/outbox/cache tables.
- The Transfer sand is a real widget backed by a dedicated contract and typed backend actions. Its manifest still says `requires_server: false` because it runs as an official local widget, but the workflow uses server-side widget actions and streams.

### Implemented Data

The current widget-facing schema/model surface includes:

- `transfer`: base Transfer row and activation quantity.
- `transfer_item`: contribution/need actor snapshots, source Record ids, item heads, side quantities, agreement levels, date, and location.
- `transfer_node_identity`: local signing label and keypair.
- `transfer_identity`: stable Transfer UID, source/parent UID, state, title, coordinator/proposer/counterparty labels, side actor labels/public keys, target Organ, and source/target base URLs.
- `transfer_relation`: relation edges between Transfer UIDs, currently used for parent trees and accepted for dependencies in imported packages.
- `transfer_tree_config`: branch mode, reservation policy override, and record-sync configuration for Transfer trees.
- `transfer_event`: append-only event rows with actor label, optional actor public key, kind, payload JSON, previous event id/uid, event uid, and signature.
- `transfer_local_settlement`: idempotent local settlement by `(transfer_id, local_actor_label)`.
- `transfer_settlement`: older/global settlement shape retained in schema.
- `transfer_sync_cursor`: last mirrored event per peer label.
- `transfer_sync_outbox`: pending package posts to remote base URLs.
- `transfer_gossip_package`: cached public/permitted Transfer packages discovered from other nodes.

The structured backend schema adds:

- `transfer_party`: participants, coordinators, observers, and placeholders.
- `transfer_structured_item`: Transfer-specific item roles, source Record refs, title/description, snapshots, quantity, unit, metadata, and version.
- `transfer_interaction`: item/party links for contribution paths, dependencies, unblocking, replacement, and information flow.
- `transfer_agreement`: scoped agreement by Transfer, item, or interaction with agreed versions and invalidation event.
- `transfer_confirmation`: scoped delivery and receipt confirmations.
- `transfer_structured_settlement`: idempotent structured settlement effects.
- `transfer_quantity_influence`: plus/minus planned, active, consumed, released, or invalidated quantity facts.
- `record_transfer_availability`: explicit SQL projection/cache for Record availability after active hard Transfer reservations.
- `transfer_message`: Transfer and interaction-level messages.
- `transfer_visibility_subject`, `transfer_visibility_rule`, and `transfer_visibility_field`: field-level visibility.
- `work_metadata`, `work_subject`, and `work_assignment`: generic Kanban/Transfer work fields, assignable subjects, and assignment links.

Existing Transfers are backfilled into structured parties, items, interactions, and agreement rows during migration.
Kanban Record sidecar work data is backfilled into the same generic work metadata tables.

Implemented Rust domain enums live in `domain::clean::transfer` for agreement type, settlement mode, agreement level, Transfer role, direction, interaction kind, participation kind, confirmation kind, Transfer state, relation kind, and dependency kind.

The structured model is introduced by `20260614133000_structured_transfer_model.sql`, and the persistence test suite runs the embedded migrations against in-memory SQLite to verify the tables are created.
Generic work metadata is introduced by `20260617120000_generic_work_metadata.sql`.

### Implemented Workflow

The current Transfer sand can:

- Configure/reset the local signing party.
- Create Records for use in Transfers.
- Create a Transfer proposal from a local Record.
- Duplicate a public proposal into a local Transfer.
- Update the local side's item title, Record link, and quantity.
- Sign agreement in two levels using contribution/need side ownership.
- Inactivate a Transfer and reset side agreement levels.
- Confirm delivery from the contribution side.
- Confirm receipt from the need side.
- Apply idempotent local settlement to the local Record quantity.
- Create child Transfers under a parent Transfer.
- Create and sync Transfer trees from Record trees.
- Configure branch mode and record sync mode.
- Edit Transfer-level work metadata: start datetime, end datetime, estimate, completion notes, local assignees, and external assignees.
- Show structured Transfer items as collapsible work rows and edit each item's work metadata with the same fields.
- Show structured Transfer interactions as collapsible work rows and edit each interaction's work metadata with the same fields when interaction rows exist.
- Post/import Transfer packages.
- Toggle public proposal ingress.
- Render Transfer summaries, detail, tree metadata, agreement state, work metadata, history, delivery/receipt state, package import/export, and settlement actions.

This implements the "proposal before settlement" rule: creating and editing a Transfer writes Transfer data and events only. Record quantity mutation happens in `settle-local` after agreement, delivery, and receipt checks.

Work metadata is packaged with Transfers for now. Transfer packages include Transfer-level work metadata, structured item work metadata, structured interaction work metadata, metadata JSON, and assignment snapshots. Saving Transfer, item, or interaction work metadata updates `work_metadata`, `work_subject`, and `work_assignment`, participates in package sync, and can be discovered through `/transfer/packages/since`; it does not append signed Transfer events.

Transfer does not expose `task_type` or `status` from `work_metadata` in the sand for now. `task_type` is Kanban-flavored, and Transfer status should usually be derived from agreement, delivery, receipt, and settlement state.

External assignees are snapshots unless they include stable remote identity. A package may carry remote base URL, public key, subject UID, display name, and Organ name. Assignments with stable `remote_base_url + remote_subject_uid` reuse the same `work_subject`; display-name-only assignments remain non-durable snapshots.

Remaining work metadata work: add broader service-level tests around Transfer widget work metadata actions with a small widget-service test harness that creates a migrated in-memory DB, users, Transfers, structured items/interactions, calls widget actions, and asserts tables/snapshots.

### Implemented Events And Packages

Events are append-only and signed with the local Transfer node key. The implemented event kinds are:

- `transfer_created`
- `item_created`
- `agreement_changed`
- `delivery_confirmed`
- `receipt_confirmed`
- `settlement_applied`

The `transfer_event` table also accepts the broader structured event vocabulary needed by the package model and future UI flows: `transfer_quantity_changed`, `transfer_inactivated`, `item_edited`, `interaction_created`, `interaction_edited`, `visibility_changed`, `message_sent`, `settlement_reverted`, `dispute_opened`, and `dispute_resolved`. Most of those event handlers are still planned; the schema now reserves the stable event names so package/history data does not need another table rewrite.

Transfer packages carry identity, item, relation, tree config, and event data between nodes. Nodes can receive addressed packages directly, accept public initial proposal packages when ingress is enabled, or cache unrelated public packages as gossip. Startup and heartbeat tasks maintain a local transfer sync cache and flush the sync outbox.

### Agreement, Events, And Messages

Agreement is about the current state of connected items/interactions, not about abstract counteroffers.

Agreement levels:

| Level | Meaning |
| ----- | ------- |
| `0` | No current agreement, or agreement invalidated by an edit. |
| `1` | First agreement: the party reviewed the current visible proposal and is aligned. |
| `2` | Commitment threshold: the party accepts its part and the interaction can activate if policy is satisfied. |

Implemented facts:

- Agreement level is typed in Rust and stored as `0`, `1`, or `2`.
- The structured schema stores scoped `transfer_agreement` rows for Transfer, item, or interaction agreement.
- Agreement rows can store agreed item/interaction versions and invalidation event references.
- `transfer_event` stores signed append-only events with UID/signature fields and validation state.
- `transfer_message` exists for Transfer and interaction-level messages.

Remaining implementation work:

- Make edit actions on structured items/interactions increment versions and invalidate connected `transfer_agreement` rows.
- Derive settlement readiness from structured agreement, confirmation, interaction, and settlement rows instead of the legacy contribution/need projection.
- Deserialize `transfer_event.payload_json` into typed payload structs/enums at package/action boundaries.
- Reject or mark invalid event/package data whose payload shape does not match `event_kind`.
- Populate and verify deterministic `previous_event_hash` and `event_hash` values.
- Implement message send/display actions in the Transfer sand using `transfer_message` plus `message_sent` events.

### Implemented Networking

The current network model is practical package sync, not full federation:

- Local Lince coordinates writes for its Transfers.
- Organs/remote base URLs can receive Transfer packages through `/transfer/packages`.
- Nodes can expose package updates through `/transfer/packages/since`.
- Participating nodes can mirror imported event logs.
- Nodes track sync progress with `transfer_sync_cursor`.
- Failed/queued posts are retried through `transfer_sync_outbox`.
- Public or permitted packages can be cached in `transfer_gossip_package`.

Topic-based peer discovery, contact-list-based peer discovery, full field-level visibility filtering, and coordinator migration remain planned work.

### Implemented Settlement

Settlement is local and idempotent per actor. The contribution side applies a negative delta to its local Record; the need side applies a positive delta to its local Record. Settlement requires:

- both sides at agreement level `2`,
- a delivery-confirmed event,
- a receipt-confirmed event,
- a local Record selected for the settling side,
- no existing `transfer_local_settlement` for the same Transfer and actor.

The current sand settlement path consumes or releases local plus/minus influence facts for the Transfer being settled or inactivated. Reservation-aware views must explicitly join `record_transfer_availability`; arbitrary SSE views are not rewritten.

Implemented simulation settlement keeps Record quantity and availability separate:

| Quantity | Meaning |
| -------- | ------- |
| `record.quantity` | Actual settled Record quantity. |
| `record_transfer_availability.proposed_outgoing_quantity` | Planned negative Transfer influence. |
| `record_transfer_availability.proposed_incoming_quantity` | Planned positive Transfer influence. |
| `record_transfer_availability.reserved_quantity` | Active hard outgoing reservation from Transfers. |
| `record_transfer_availability.reserved_incoming_quantity` | Active positive Transfer influence, informational only. |
| `record_transfer_availability.available_quantity` | Actual quantity minus active hard outgoing reservation. |
| `record_transfer_availability.planned_quantity` | Simple projection: actual plus incoming influence minus outgoing influence. |

The active configuration has a default `transfer_reservation_policy`:

| Policy | Meaning |
| ------ | ------- |
| `none` | Never show staged Transfer quantity changes. Only final settlement changes Record quantity. |
| `soft` | Track proposal intent without reducing availability. |
| `hard_on_proposal` | Reserve outgoing quantity when a proposal is created. |
| `hard_on_consume` | Reserve outgoing quantity when a proposal is duplicated/consumed into a local Transfer. |
| `hard_on_lock` | Reserve outgoing quantity when both sides lock/accept agreement terms. |

Each Transfer can override the default in `transfer_tree_config.reservation_policy`. `NULL` means inherit from the nearest parent Transfer override; a root Transfer with no override uses the active configuration default. Structured items use the effective policy of their owning Transfer. Child Transfers inherit the effective policy down to leaves unless they override it.

For v1, only outgoing local contribution/source-record quantities reserve local stock. Need/request Transfers do not reserve local stock unless a later workflow explicitly backs them with a concrete local source Record.

SSE views send exactly the columns selected by their saved SQL. Reservation-aware views must explicitly join `record_transfer_availability`:

```sql
SELECT
    record.*,
    availability.proposed_outgoing_quantity,
    availability.proposed_incoming_quantity,
    availability.reserved_quantity,
    availability.reserved_incoming_quantity,
    availability.available_quantity,
    availability.planned_quantity
FROM record
LEFT JOIN record_transfer_availability availability
    ON availability.record_id = record.id;
```

Example: outgoing donation, Record quantity `10`, Transfer contribution `5`:

- `soft`: `quantity = 10`, `reserved_quantity = 0`, `available_quantity = 10`.
- `hard_on_proposal`: after proposal creation, `quantity = 10`, `reserved_quantity = 5`, `available_quantity = 5`.
- `hard_on_consume`: before duplication/consumption, available remains `10`; after local consumption, available becomes `5`.
- `hard_on_lock`: before agreement lock, available remains `10`; after both sides lock/accept terms, available becomes `5`.
- `none`: availability never changes during Transfer stages; after settlement, `record.quantity = 5`.

Full settlement is available as a Transfer-level action. Individual settlement applies only the current local party's Record side. Full settlement checks the Transfer once and applies both contribution and need Record effects when both Records are local and the Transfer is ready.

Settlement readiness includes structured interaction dependencies. Blocking structured interactions with dependency kinds such as `must_agree`, `must_deliver`, `must_receive`, or `must_settle` prevent settlement until their state is completed, satisfied, settled, or inactive. The legacy contribution/need agreement, delivery, and receipt checks still apply.

The Relation sand can store a projection view id in its widget state so it can be configured to use SQL views that include Transfer quantity projection columns.

### Implemented Karma

Karma can activate, deactivate, or neutralize a preconfigured Transfer by changing `transfer.quantity`. It does not create Transfer parties, visibility, proposal shape, items, interactions, agreement, or settlement.

Transfer quantity is exposed to Karma with two equivalent token forms:

```text
tq4
transfer-quantity-4
```

Both tokens read or write `transfer.quantity` for Transfer `4`.

In a condition, the token is replaced with the current Transfer quantity. If the Transfer does not exist, the value is `0`.

In a consequence, the token identifies which Transfer quantity receives the evaluated condition value. For example:

```text
condition: rq7 < 7
operator: =
consequence: tq4
```

If Record `7` is below `7`, Transfer `4` receives quantity `1`.

Karma rules can also depend on Transfer quantities:

```text
condition: tq4
operator: =
consequence: rq9
```

When Transfer `4` quantity changes, Karma rules that reference `tq4` or `transfer-quantity-4` in their condition can run. This mirrors the existing `rq{id}` behavior for Record quantity.

## Status

- [x] Transfer is treated as structured data, not a single immediate transaction.
- [x] Quantity stays central.
- [x] Records can participate in Transfers without becoming permanently Need or Contribution objects.
- [x] Transfer items carry their own title and description.
- [x] Transfer can be nested under a parent Transfer.
- [x] Visibility is first-class data.
- [ ] A Lince Cell is modeled as an Organ used by one person.
- [x] Personal Organs can publish and consume p2p Transfer summaries.
- [ ] Agreement is invalidated by edits to connected items.
- [x] Agreement policies are typed in Rust, not passed around as raw strings.
- [x] Event kinds are typed in Rust, not passed around as raw strings.
- [ ] Event payloads are deserialized into typed Rust values at the boundary.
- [x] Karma only activates/deactivates preconfigured Transfers for now.
- [x] Transfer stores enough facts for SQL views and sands to project richer quantity views.
- [x] Simulation can store plus/minus influence facts.
- [x] Delivery and receipt confirmations are modeled separately.
- [x] Settlement is idempotent.
- [x] Transfer proposal data is separate from final Record quantity mutation.
- [x] Transfer history is append-only.
- [x] A coordinator event log can be mirrored by participating Cells.
- [x] Signed events are documented for later use.
- [x] Discovery can cache public or permitted Transfer summaries.
- [x] A central or Organ server can introduce Cells to each other.
- [x] Direct Cell-to-Cell sync can happen after introduction.
- [x] The Transfer sand is a real server-backed workflow, not a placeholder.
- [x] The doc set is split into multiple focused files.

## Checklist

### Product Shape

- [ ] Transfer is scoped to Lince data only for the first version.
- [ ] No payment integration is assumed.
- [ ] No delivery-provider integration is assumed.
- [ ] No external messaging integration is assumed.
- [ ] No calendar integration is assumed.
- [ ] No legal-contract language is required for MVP.
- [x] The feature is described as a protocol for making Record changes socially valid.
- [x] The feature supports both personal and shared Organ use.
- [x] The feature supports one-off and grouped work.
- [x] The feature supports large subjects split into smaller child Transfers.

### Core Concepts

- [x] A Transfer is a structured promise before execution.
- [x] A Transfer can contain multiple interactions.
- [x] A Transfer can contain multiple items.
- [x] A Transfer item can represent a Need.
- [x] A Transfer item can represent a Contribution.
- [x] A Transfer item can represent support.
- [x] A Transfer item can represent a task.
- [x] A Transfer item can represent information.
- [x] A Transfer item can represent a reservation.
- [x] A parent Transfer can group child Transfers.
- [x] A parent Transfer can expose aggregate state.
- [x] Child Transfers can keep their own policies.
- [x] Child Transfers can have dependencies.

### Typed Options

- [x] Agreement type is a Rust enum.
- [x] Settlement mode is a Rust enum.
- [x] Agreement level is a Rust enum.
- [x] Transfer role is a Rust enum.
- [x] Transfer direction is a Rust enum.
- [x] Transfer interaction kind is a Rust enum.
- [x] Participation kind is a Rust enum.
- [x] Confirmation kind is a Rust enum.
- [x] Event kind is a Rust enum.
- [x] Storage strings are parsed into Rust types at the boundary.
- [x] Storage strings are serialized from Rust types at the boundary.
- [ ] Raw `get("field")` access is avoided in the design.

### Visibility

- [x] Visibility is modeled with tables.
- [x] Visibility applies to Records.
- [x] Visibility applies to Transfers.
- [x] Visibility applies to Transfer items.
- [x] Visibility applies to Transfer events.
- [x] Visibility applies to fields, not only whole rows.
- [x] A subject can be a user.
- [x] A subject can be an Organ.
- [x] A subject can be public.
- [ ] A party can see only the Transfer fields allowed for it.
- [ ] A party can see a Transfer item title without seeing the source Record head.
- [ ] A party can see a Transfer item description without seeing the source Record body.
- [ ] Visibility can hide source Record identity.
- [ ] Visibility can hide other parties.
- [ ] Visibility can hide locations and quantities.

### Agreement And Editing

- [ ] Default agreement mode is individual.
- [ ] Full agreement exists as an option.
- [ ] Percentage agreement exists as an option.
- [ ] Dependency agreement exists as an option.
- [ ] Editing a connected item invalidates earlier agreement.
- [x] Agreement level 0 means no current agreement.
- [x] Agreement level 1 means first review/align.
- [x] Agreement level 2 means commitment/activation threshold.
- [x] Agreement state is tracked per item or interaction.
- [ ] Agreement state can also be derived for a parent Transfer.

### History And Events

- [x] Transfer events are append-only.
- [x] Event hashes can chain together.
- [x] Signed events are documented for future use.
- [ ] Event validation can be deterministic.
- [ ] Event payloads can be typed.
- [x] Messages are separate from generic comments.
- [x] Messages belong to a Transfer.
- [x] Messages can belong to a specific interaction.

### Karma

- [x] Karma can turn a Transfer on by changing quantity.
- [x] Karma can turn a Transfer off by changing quantity.
- [x] Karma does not invent visibility.
- [x] Karma does not invent parties.
- [x] Karma does not silently settle a Transfer.
- [x] Karma-generated actions are bounded.

### Simulation

- [x] Transfer influence is modeled with plus/minus facts.
- [x] Actual quantity remains separate from projected quantity.
- [x] Proposed outgoing can be projected.
- [x] Proposed incoming can be projected.
- [x] Reserved outgoing can be projected for active hard local contribution reservations.
- [x] Reserved incoming can be projected.
- [x] Available can be projected for active hard local contribution reservations.
- [x] Planned can be projected with the simple formula.
- [ ] Surplus can be projected.
- [x] SQL views can explicitly join reservation availability.
- [x] Relation sand can choose its projection view.

### Settlement

- [x] Delivery confirmation is modeled.
- [x] Receipt confirmation is modeled.
- [x] Settlement is idempotent.
- [x] Individual settlement exists.
- [x] Full settlement exists for Transfers where both sides have local Records.
- [x] Settlement readiness includes structured interaction dependency state.
- [x] Settlement can apply Record quantity changes.
- [x] Settlement can consume reserved influence facts for the local Transfer path.

### Networking

- [ ] A Cell can act as a p2p node.
- [x] A node can publish visible Transfer summaries.
- [x] A node can cache visible Transfer summaries.
- [x] A node can keep cached summaries stale.
- [x] A participating Cell can mirror a Transfer event log.
- [x] A participating Cell can track its last synced event.
- [x] A coordinator orders writes while replicas sync eventually.
- [x] A central or Organ server can introduce peers.
- [x] Direct peer sync can happen after introduction.
- [ ] Peer discovery can be topic-based.
- [ ] Peer discovery can be contact-list based.
- [x] Event logs can later become signed.

### Transfer Sand

- [ ] The Transfer sand requires a server.
- [x] The Transfer sand declares the permissions it needs.
- [x] The Transfer sand has a dedicated runtime contract.
- [x] The Transfer sand has typed backend actions.
- [x] The Transfer sand can list Transfer summaries.
- [x] The Transfer sand can load one Transfer detail.
- [x] The Transfer sand can create a Transfer.
- [x] The Transfer sand can create child Transfers.
- [x] The Transfer sand can add and edit Transfer items.
- [x] The Transfer sand can link a Transfer item to a source Record.
- [ ] The Transfer sand can show Transfer-specific item title and description.
- [x] The Transfer sand can configure parties.
- [ ] The Transfer sand can configure field-level visibility.
- [ ] The Transfer sand can show item interactions.
- [ ] The Transfer sand can show dependencies.
- [x] The Transfer sand can show agreement state.
- [x] The Transfer sand can let permitted parties agree.
- [ ] The Transfer sand invalidates agreement through backend rules after connected edits.
- [ ] The Transfer sand can show Transfer messages.
- [x] The Transfer sand can show append-only Transfer history.
- [x] The Transfer sand can show delivery confirmation state.
- [x] The Transfer sand can show receipt confirmation state.
- [x] The Transfer sand can request settlement.
- [ ] The Transfer sand can show quantity influence facts when they exist.
- [x] The Transfer backend projection exposes Transfer-level work metadata when it exists.
- [x] The Transfer sand can edit Transfer work metadata.
- [x] The Transfer sand can show and edit item work metadata.
- [x] The Transfer sand can show and edit interaction work metadata.

### Roadmap

- [x] The doc is split into multiple files.
- [x] The main file is a tracker.
- [x] The main file has many checkboxes.
- [x] The plan can grow without becoming one monolith.
- [x] The schema and Rust models have a structured backend implementation.
- [x] Generic Kanban work metadata can attach to Transfers.
- [x] Generic Kanban work metadata can attach to structured Transfer items.
- [ ] The UI still needs to migrate from contribution/need projection to structured Transfer projections.
- [ ] The reducer/service layer still needs to use structured agreement invalidation and interaction state.
- [x] Explicit reservation projection is available through `record_transfer_availability`.
- [ ] Visibility-aware projection filtering still needs implementation after visibility.
- [ ] The networking protocol still needs Transfer packages over structured Transfer data.
- [ ] Contribution/need projection writes can be retired after the sand uses structured projections.
