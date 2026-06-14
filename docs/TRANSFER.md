# Transfer

This is the main tracker for the Transfer feature.

The detailed notes live in:

- [Core Model](transfer-core-model.md)
- [Visibility](transfer-visibility.md)
- [Data Model](transfer-data-model.md)
- [Agreement And Events](transfer-agreement-events.md)
- [Karma](transfer-karma.md)
- [Simulation And Settlement](transfer-simulation-settlement.md)
- [Intent And Discovery](transfer-intent-discovery.md)
- [Networking And Sync](transfer-networking.md)
- [Work Metadata](transfer-work-metadata.md)
- [Transfer Sand](transfer-sand.md)

## MVP Build Order

Recommended first implementation order:

1. Visibility subjects, rules, and fields.
2. Transfer header with quantity and immutable agreement/settlement modes.
3. Transfer parties linked to visibility subjects.
4. Transfer items with Transfer-specific title/description and source Record snapshots.
5. Transfer interactions and dependencies.
6. Transfer event log and validator.
7. Coordinator-backed event sync cursor for participating Cells.
8. Agreement levels with edit invalidation for connected items.
9. Messages.
10. Karma link that changes Transfer quantity only.
11. Transfer quantity influence facts.
12. Delivery/receipt confirmations.
13. Individual/full settlement.
14. Server-backed Transfer sand contract and typed backend actions.
15. Transfer sand list/detail/create/agreement UI.
16. Basic peer/contact table and Transfer discovery cache.
17. Optional SQL views or sand queries for richer quantity projections.
18. Optional work metadata attachment.

## Not First

- External integrations.
- Global reputation.
- Full peer-to-peer federation.
- Hardcoded expiration.
- Role-based agreement.
- Complex legal-contract language.

## Cross-File Map

- Product shape and corrected assumptions live in [Core Model](transfer-core-model.md).
- Visibility tables and UI behavior live in [Visibility](transfer-visibility.md).
- Transfer tables, typed enums, nesting, and interactions live in [Data Model](transfer-data-model.md).
- Agreement, history events, signed events, and messages live in [Agreement And Events](transfer-agreement-events.md).
- Karma activation lives in [Karma](transfer-karma.md).
- Simulation, confirmation, and settlement live in [Simulation And Settlement](transfer-simulation-settlement.md).
- Intent, discovery, proposal editing, and state model live in [Intent And Discovery](transfer-intent-discovery.md).
- P2P cache, peer discovery, and server topology live in [Networking And Sync](transfer-networking.md).
- Generalized Kanban/work metadata lives in [Work Metadata](transfer-work-metadata.md).
- Transfer sand requirements live in [Transfer Sand](transfer-sand.md).

## Status

- [x] Transfer is treated as structured data, not a single immediate transaction.
- [x] Quantity stays central.
- [x] Records can participate in Transfers without becoming permanently Need or Contribution objects.
- [/] Transfer items carry their own title and description.
- [x] Transfer can be nested under a parent Transfer.
- [ ] Visibility is first-class data.
- [ ] A Lince Cell is modeled as an Organ used by one person.
- [x] Personal Organs can publish and consume p2p Transfer summaries.
- [ ] Agreement is invalidated by edits to connected items.
- [ ] Agreement policies are typed in Rust, not passed around as raw strings.
- [x] Event kinds are typed in Rust, not passed around as raw strings.
- [ ] Event payloads are deserialized into typed Rust values at the boundary.
- [ ] Karma only activates/deactivates preconfigured Transfers for now.
- [x] Transfer stores enough facts for SQL views and sands to project richer quantity views.
- [ ] Simulation uses plus/minus influence facts.
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
- [ ] A Transfer can contain multiple interactions.
- [x] A Transfer can contain multiple items.
- [ ] A Transfer item can represent a Need.
- [ ] A Transfer item can represent a Contribution.
- [ ] A Transfer item can represent support.
- [ ] A Transfer item can represent a task.
- [ ] A Transfer item can represent information.
- [ ] A Transfer item can represent a reservation.
- [x] A parent Transfer can group child Transfers.
- [x] A parent Transfer can expose aggregate state.
- [x] Child Transfers can keep their own policies.
- [ ] Child Transfers can have dependencies.

### Typed Options

- [ ] Agreement type is a Rust enum.
- [ ] Settlement mode is a Rust enum.
- [ ] Agreement level is a Rust enum.
- [x] Transfer role is a Rust enum.
- [ ] Transfer direction is a Rust enum.
- [ ] Transfer interaction kind is a Rust enum.
- [ ] Participation kind is a Rust enum.
- [ ] Confirmation kind is a Rust enum.
- [x] Event kind is a Rust enum.
- [x] Storage strings are parsed into Rust types at the boundary.
- [x] Storage strings are serialized from Rust types at the boundary.
- [ ] Raw `get("field")` access is avoided in the design.

### Visibility

- [ ] Visibility is modeled with tables.
- [ ] Visibility applies to Records.
- [ ] Visibility applies to Transfers.
- [ ] Visibility applies to Transfer items.
- [ ] Visibility applies to Transfer events.
- [ ] Visibility applies to fields, not only whole rows.
- [ ] A subject can be a user.
- [ ] A subject can be an Organ.
- [ ] A subject can be public.
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
- [ ] Agreement level 0 means no current agreement.
- [ ] Agreement level 1 means first review/align.
- [ ] Agreement level 2 means commitment/activation threshold.
- [ ] Agreement state is tracked per item or interaction.
- [ ] Agreement state can also be derived for a parent Transfer.

### History And Events

- [x] Transfer events are append-only.
- [ ] Event hashes can chain together.
- [x] Signed events are documented for future use.
- [ ] Event validation can be deterministic.
- [ ] Event payloads can be typed.
- [ ] Messages are separate from generic comments.
- [ ] Messages belong to a Transfer.
- [ ] Messages can belong to a specific interaction.

### Karma

- [ ] Karma can turn a Transfer on by changing quantity.
- [ ] Karma can turn a Transfer off by changing quantity.
- [ ] Karma does not invent visibility.
- [ ] Karma does not invent parties.
- [ ] Karma does not silently settle a Transfer.
- [ ] Karma-generated actions are bounded.

### Simulation

- [ ] Transfer influence is modeled with plus/minus facts.
- [ ] Actual quantity remains separate from projected quantity.
- [ ] Proposed outgoing can be projected.
- [ ] Proposed incoming can be projected.
- [ ] Reserved outgoing can be projected.
- [ ] Reserved incoming can be projected.
- [ ] Available can be projected.
- [ ] Planned can be projected.
- [ ] Surplus can be projected.
- [ ] SQL views can be used for richer projections.
- [ ] Sands can choose their own projection view.

### Settlement

- [x] Delivery confirmation is modeled.
- [x] Receipt confirmation is modeled.
- [x] Settlement is idempotent.
- [x] Individual settlement exists.
- [ ] Full settlement exists.
- [ ] Settlement can be derived from interaction state.
- [x] Settlement can apply Record quantity changes.
- [ ] Settlement can consume reserved influence facts.

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

### Roadmap

- [x] The doc is split into multiple files.
- [x] The main file is a tracker.
- [x] The main file has many checkboxes.
- [x] The plan can grow without becoming one monolith.
- [ ] The schema and Rust models still need implementation.
- [ ] The UI still needs Transfer screens.
- [ ] The reducer still needs to be written.
- [x] The views for visibility and projection still need implementation.
- [ ] The networking protocol still needs implementation.
