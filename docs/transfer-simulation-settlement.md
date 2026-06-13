# Simulation And Settlement

Simulation should use a plus/minus model, but the Transfer feature should not try to own every possible quantity projection right away.

Core Transfer should store enough information for SQL queries and sands to calculate richer views. A sand that wants more than `quantity` can request/query properties like proposed, reserved, incoming, outgoing, available, planned, and surplus. That is a presentation/query problem on top of Transfer data.

## Projection View

A useful SQL view or sand-level query could expose:

| Quantity | Meaning |
| -------- | ------- |
| Actual | Current Record quantity. |
| Proposed outgoing | Quantity requested by draft/proposed Transfers. |
| Proposed incoming | Quantity expected from draft/proposed Transfers. |
| Reserved outgoing | Quantity promised to agreed/active Transfers. |
| Reserved incoming | Quantity expected from agreed/active Transfers. |
| Available | Actual minus relevant outgoing holds. |
| Planned | Actual plus incoming minus outgoing across a selected scenario. |
| Surplus | Quantity above a user-defined threshold. |

The core table only needs to record the influence facts:

```sql
CREATE TABLE transfer_quantity_influence (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_id INTEGER REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    item_id INTEGER NOT NULL REFERENCES transfer_item(id) ON DELETE CASCADE,
    record_id INTEGER NOT NULL REFERENCES record(id),
    influence REAL NOT NULL,
    influence_state TEXT NOT NULL DEFAULT 'planned',
    policy TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    consumed_at TEXT,
    CHECK (influence_state IN ('planned', 'active', 'consumed', 'released', 'invalidated')),
    CHECK (policy IN ('protect_transfer', 'surplus_transfer', 'proportional', 'manual'))
) STRICT;
```

## Projection Function

```rust
pub async fn record_quantity_with_transfer_influence(
    db: &SqlitePool,
    record_id: i64,
) -> Result<RecordQuantityProjection, Error> {
    let actual = load_record_quantity(db, record_id).await?;
    let influences = load_active_transfer_influences(db, record_id).await?;

    let incoming: f64 = influences.iter().filter(|i| i.influence > 0.0).map(|i| i.influence).sum();
    let outgoing: f64 = influences.iter().filter(|i| i.influence < 0.0).map(|i| i.influence).sum();

    Ok(RecordQuantityProjection {
        actual,
        incoming_transfer_influence: incoming,
        outgoing_transfer_influence: outgoing,
        projected: actual + incoming + outgoing,
    })
}
```

UI example:

```text
Apples
Actual: 30
Transfer influence: -10
Available: 20
```

This makes it clear that the Record quantity has an active Transfer influence while the Transfer is happening.

## SQL View Example

```sql
CREATE VIEW record_transfer_quantity_projection AS
SELECT
    record.id AS record_id,
    record.quantity AS actual_quantity,
    COALESCE(SUM(CASE
        WHEN transfer_quantity_influence.influence_state = 'planned'
             AND transfer_quantity_influence.influence < 0
        THEN transfer_quantity_influence.influence
        ELSE 0
    END), 0) AS proposed_outgoing,
    COALESCE(SUM(CASE
        WHEN transfer_quantity_influence.influence_state = 'planned'
             AND transfer_quantity_influence.influence > 0
        THEN transfer_quantity_influence.influence
        ELSE 0
    END), 0) AS proposed_incoming,
    COALESCE(SUM(CASE
        WHEN transfer_quantity_influence.influence_state = 'active'
             AND transfer_quantity_influence.influence < 0
        THEN transfer_quantity_influence.influence
        ELSE 0
    END), 0) AS reserved_outgoing,
    COALESCE(SUM(CASE
        WHEN transfer_quantity_influence.influence_state = 'active'
             AND transfer_quantity_influence.influence > 0
        THEN transfer_quantity_influence.influence
        ELSE 0
    END), 0) AS reserved_incoming
FROM record
LEFT JOIN transfer_quantity_influence
    ON transfer_quantity_influence.record_id = record.id
GROUP BY record.id;
```

This view is an example, not a required MVP artifact. The Transfer feature should make it possible; individual sands can decide how much of it they want to show.

## Confirmation

Confirmation should stay simple.

Each party can confirm:

- Their agreement level for the current connected item state.
- Delivery of their part.
- Receipt of the other part.

No extra confirmation categories are needed for MVP.

```sql
CREATE TABLE transfer_confirmation (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_id INTEGER REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    item_id INTEGER REFERENCES transfer_item(id) ON DELETE CASCADE,
    subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id),
    confirmation_kind TEXT NOT NULL,
    confirmed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    event_id INTEGER REFERENCES transfer_event(id),
    CHECK (confirmation_kind IN ('delivery', 'receipt'))
) STRICT;
```

```rust
pub async fn confirm_transfer_delivery(
    db: &SqlitePool,
    subject: VisibilitySubject,
    interaction_id: i64,
) -> Result<(), Error> {
    ensure_can_confirm_delivery(db, subject, interaction_id).await?;
    insert_transfer_confirmation(db, subject, interaction_id, ConfirmationKind::Delivery).await?;
    append_transfer_event(
        db,
        subject,
        TransferEventKind::DeliveryConfirmed,
        json!({ "interaction_id": interaction_id }),
    )
    .await?;
    try_settle_interaction(db, interaction_id).await
}
```

## Settlement

Settlement should be simple and follow the same grouping choice as agreement.

Modes:

- `individual`: when an individual interaction is resolved, its Record quantity changes can be applied.
- `full`: all required interactions must be resolved before any Record quantity changes are applied.

Settlement must be idempotent.

```sql
CREATE TABLE transfer_settlement (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_id INTEGER REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    settled_by_subject_id INTEGER REFERENCES transfer_visibility_subject(id),
    settled_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    event_id INTEGER REFERENCES transfer_event(id),
    UNIQUE (transfer_id, interaction_id)
) STRICT;
```

```rust
pub async fn try_settle_interaction(
    db: &SqlitePool,
    interaction_id: i64,
) -> Result<(), Error> {
    if settlement_already_applied(db, interaction_id).await? {
        return Ok(());
    }

    let transfer = load_transfer_by_interaction(db, interaction_id).await?;

    let can_settle = match transfer.settlement_mode {
        SettlementMode::Individual => delivery_and_receipt_confirmed(db, interaction_id).await?,
        SettlementMode::Full => all_transfer_interactions_resolved(db, transfer.id).await?,
    };

    if !can_settle {
        return Ok(());
    }

    apply_interaction_record_quantity_changes(db, interaction_id).await?;
    consume_transfer_quantity_influence(db, interaction_id).await?;
    insert_transfer_settlement(db, transfer.id, Some(interaction_id)).await?;
    Ok(())
}
```
