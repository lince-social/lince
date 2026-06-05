# Workflow And Roadmap

This note keeps the practical build order and "not first" decisions from `OLD_TRANSFER.md`.

## MVP Build Order

Recommended first implementation order:

1. Visibility subjects, rules, and fields.
2. Transfer header with quantity and immutable agreement/settlement modes.
3. Transfer parties linked to visibility subjects.
4. Transfer items with Transfer-specific title/description and source Record snapshots.
5. Transfer interactions and dependencies.
6. Transfer event log and validator.
7. Agreement levels with edit invalidation for connected items.
8. Messages.
9. Karma link that changes Transfer quantity only.
10. Transfer quantity influence facts.
11. Delivery/receipt confirmations.
12. Individual/full settlement.
13. Basic peer/contact table and Transfer discovery cache.
14. Optional SQL views or sand queries for richer quantity projections.
15. Optional work metadata attachment.

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
