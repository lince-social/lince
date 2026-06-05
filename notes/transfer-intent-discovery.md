# Intent And Discovery

Intent can be concrete or open-ended.

## Intent

Sometimes the Transfer starts with a known party:

```text
Alice needs 14 apples from Farmer Organ.
```

Sometimes it starts with "someone":

```text
Someone can contribute live guitar audience time.
Someone can receive these surplus apples.
Someone can help split this Need into smaller Needs.
```

The model should allow placeholder subjects. A placeholder can later be replaced by a real user or Organ.

```sql
CREATE TABLE transfer_placeholder_subject (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    role_hint TEXT,
    replaced_by_subject_id INTEGER REFERENCES transfer_visibility_subject(id)
) STRICT;
```

## Discovery

Discovery should not mutate Records. It should create candidate links or Transfers.

```rust
pub async fn discover_transfer_candidates(
    db: &SqlitePool,
    viewer: VisibilitySubject,
    record_id: i64,
) -> Result<Vec<TransferCandidate>, Error> {
    let visible_intents = load_visible_transfer_items_for_record(db, viewer, record_id).await?;
    Ok(match_candidates_against_record(db, record_id, visible_intents).await?)
}
```

## Proposal And Editing

A proposal is the current editable Transfer state plus its event history.

There can be versions internally, but the UX should treat changes as edits, not formal counteroffers. The important rule is invalidation:

- Edit a Transfer item.
- Find connected interactions/items.
- Reset agreement levels for those connected objects.
- Keep an event explaining what changed.

This is stricter and clearer than letting old agreements survive edits.

## State Model

State should be item/interaction-oriented first, and Transfer-oriented second.

Transfer-level state is a cache derived from items, interactions, quantity, and events.

Suggested Transfer states:

| State | Meaning |
| ----- | ------- |
| `draft` | Local idea, not visible unless visibility allows it. |
| `neutral` | Transfer exists but quantity is neutral, normally `0`. |
| `active` | Transfer quantity or agreement makes it active, often `-1`. |
| `partially_agreed` | Some interactions reached agreement level `2`. |
| `agreed` | Required policy is satisfied. |
| `in_transfer` | Delivery/receipt process is happening and quantity influence is visible. |
| `partially_settled` | Some interactions have settled. |
| `settled` | Settlement policy completed. |
| `cancelled` | Transfer was manually cancelled or neutralized. |
| `disputed` | Parties disagree about agreement, delivery, receipt, or settlement. |

Avoid hardcoded `expired` for MVP. Expiry-like behavior can be a Karma rule that changes quantity back to neutral.
