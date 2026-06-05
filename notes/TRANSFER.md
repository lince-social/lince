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
- [Workflow And Roadmap](transfer-workflow-roadmap.md)

## Status

- [ ] Transfer is treated as structured data, not a single immediate transaction.
- [ ] Quantity stays central.
- [ ] Records can participate in Transfers without becoming permanently Need or Contribution objects.
- [ ] Transfer items carry their own title and description.
- [ ] Transfer can be nested under a parent Transfer.
- [ ] Visibility is first-class data.
- [ ] A Lince Cell is modeled as an Organ used by one person.
- [ ] Personal Organs can publish and consume p2p Transfer summaries.
- [ ] Agreement is invalidated by edits to connected items.
- [ ] Agreement policies are typed in Rust, not passed around as raw strings.
- [ ] Event kinds are typed in Rust, not passed around as raw strings.
- [ ] Event payloads are deserialized into typed Rust values at the boundary.
- [ ] Karma only activates/deactivates preconfigured Transfers for now.
- [ ] Transfer stores enough facts for SQL views and sands to project richer quantity views.
- [ ] Simulation uses plus/minus influence facts.
- [ ] Delivery and receipt confirmations are modeled separately.
- [ ] Settlement is idempotent.
- [ ] Transfer history is append-only.
- [ ] Signed events are documented for later use.
- [ ] Discovery can cache public or permitted Transfer summaries.
- [ ] A central or Organ server can introduce Cells to each other.
- [ ] Direct Cell-to-Cell sync can happen after introduction.
- [x] The doc set is split into multiple focused files.

## Checklist

### Product Shape

- [ ] Transfer is scoped to Lince data only for the first version.
- [ ] No payment integration is assumed.
- [ ] No delivery-provider integration is assumed.
- [ ] No external messaging integration is assumed.
- [ ] No calendar integration is assumed.
- [ ] No legal-contract language is required for MVP.
- [ ] The feature is described as a protocol for making Record changes socially valid.
- [ ] The feature supports both personal and shared Organ use.
- [ ] The feature supports one-off and grouped work.
- [ ] The feature supports large subjects split into smaller child Transfers.

### Core Concepts

- [ ] A Transfer is a structured promise before execution.
- [ ] A Transfer can contain multiple interactions.
- [ ] A Transfer can contain multiple items.
- [ ] A Transfer item can represent a Need.
- [ ] A Transfer item can represent a Contribution.
- [ ] A Transfer item can represent support.
- [ ] A Transfer item can represent a task.
- [ ] A Transfer item can represent information.
- [ ] A Transfer item can represent a reservation.
- [ ] A parent Transfer can group child Transfers.
- [ ] A parent Transfer can expose aggregate state.
- [ ] Child Transfers can keep their own policies.
- [ ] Child Transfers can have dependencies.

### Typed Options

- [ ] Agreement type is a Rust enum.
- [ ] Settlement mode is a Rust enum.
- [ ] Agreement level is a Rust enum.
- [ ] Transfer role is a Rust enum.
- [ ] Transfer direction is a Rust enum.
- [ ] Transfer interaction kind is a Rust enum.
- [ ] Participation kind is a Rust enum.
- [ ] Confirmation kind is a Rust enum.
- [ ] Event kind is a Rust enum.
- [ ] Storage strings are parsed into Rust types at the boundary.
- [ ] Storage strings are serialized from Rust types at the boundary.
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

- [ ] Transfer events are append-only.
- [ ] Event hashes can chain together.
- [ ] Signed events are documented for future use.
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

- [ ] Delivery confirmation is modeled.
- [ ] Receipt confirmation is modeled.
- [ ] Settlement is idempotent.
- [ ] Individual settlement exists.
- [ ] Full settlement exists.
- [ ] Settlement can be derived from interaction state.
- [ ] Settlement can apply Record quantity changes.
- [ ] Settlement can consume reserved influence facts.

### Networking

- [ ] A Cell can act as a p2p node.
- [ ] A node can publish visible Transfer summaries.
- [ ] A node can cache visible Transfer summaries.
- [ ] A node can keep cached summaries stale.
- [ ] A central or Organ server can introduce peers.
- [ ] Direct peer sync can happen after introduction.
- [ ] Peer discovery can be topic-based.
- [ ] Peer discovery can be contact-list based.
- [ ] Event logs can later become signed.

### Roadmap

- [x] The doc is split into multiple files.
- [x] The main file is a tracker.
- [x] The main file has many checkboxes.
- [x] The plan can grow without becoming one monolith.
- [ ] The schema and Rust models still need implementation.
- [ ] The UI still needs Transfer screens.
- [ ] The reducer still needs to be written.
- [ ] The views for visibility and projection still need implementation.
- [ ] The networking protocol still needs implementation.
