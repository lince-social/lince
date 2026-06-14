# Simulation And Settlement

The implemented delivery, receipt, local idempotent settlement workflow, and structured influence table now live in [TRANSFER](TRANSFER.md). This note keeps the remaining simulation and settlement work.

## Quantity Influence

Simulation should use a plus/minus model, but Transfer should not try to own every possible quantity projection.

The structured `transfer_quantity_influence` table records the facts that SQL views and sands can project:

```sql
CREATE TABLE transfer_quantity_influence (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    item_id INTEGER,
    interaction_id INTEGER,
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

Useful projections:

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

## Remaining Work

Current settlement is local and individual, and the influence table is not yet consumed by services. Still planned:

- Full settlement mode.
- Settlement derived from interaction state once interactions are modeled.
- Consuming reserved influence facts during settlement.
- Releasing or invalidating influence facts during cancellation and agreement invalidation.
- Projection views or sand-level queries for proposed, reserved, available, planned, and surplus quantities.
- Optional settlement reversal/dispute flows.
