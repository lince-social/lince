# Karma

For now, Karma should not invent a Transfer's parties, visibility, or proposal shape.

Karma should change Transfer quantity from neutral to active, using preconfigured data:

- Parties already exist.
- Visibility already exists.
- Transfer items already exist.
- Interactions already exist.
- Karma only changes the activation quantity.

Example:

```text
if record Apple quantity < 7:
    set transfer Buy Apples quantity = -1

if transfer Buy Apples has been active for too long:
    set transfer Buy Apples quantity = 0
```

## Table

```sql
CREATE TABLE transfer_karma_link (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    karma_id INTEGER NOT NULL REFERENCES karma(id) ON DELETE CASCADE,
    activation_quantity REAL NOT NULL DEFAULT -1,
    neutral_quantity REAL NOT NULL DEFAULT 0,
    UNIQUE (transfer_id, karma_id)
) STRICT;
```

## Function Example

```rust
pub async fn apply_karma_transfer_quantity(
    db: &SqlitePool,
    karma_id: i64,
    transfer_id: i64,
    new_quantity: f64,
) -> Result<(), Error> {
    ensure_karma_is_linked_to_transfer(db, karma_id, transfer_id).await?;

    update_transfer_quantity(db, transfer_id, new_quantity).await?;
    append_transfer_event(
        db,
        VisibilitySubject::system_karma(karma_id),
        TransferEventKind::TransferQuantityChanged,
        json!({ "transfer_id": transfer_id, "quantity": new_quantity }),
    )
    .await?;

    Ok(())
}
```

This keeps automation bounded and makes the Transfer's visibility and parties inspectable before Karma activates it.
