# Agreement And Events

Agreement is about the current state of connected items, not about an abstract counteroffer.

## Agreement Levels

| Level | Meaning |
| ----- | ------- |
| `0` | No current agreement, or agreement was invalidated by an edit. |
| `1` | First agreement: the party reviewed the current visible proposal and is aligned. |
| `2` | Second agreement: the party commits their part and the interaction can activate if policy is satisfied. |

The exact meaning of first and second agreement can evolve in the UI, but level `2` is the useful activation threshold.

## Agreement Table

```sql
CREATE TABLE transfer_agreement (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_id INTEGER REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    item_id INTEGER REFERENCES transfer_item(id) ON DELETE CASCADE,
    subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id) ON DELETE CASCADE,
    agreement_level INTEGER NOT NULL DEFAULT 0,
    agreed_item_version INTEGER,
    agreed_interaction_version INTEGER,
    agreed_at TEXT,
    invalidated_at TEXT,
    invalidated_by_event_id INTEGER,
    CHECK (agreement_level IN (0, 1, 2))
) STRICT;
```

For `individual` agreement, agreements attach to interactions or connected items.

For `full` agreement, agreements can attach to the Transfer as a whole.

For `percentage` agreement, the reducer counts agreement level `2` among visible/required subjects.

## Invalidating Agreement On Edits

```rust
pub async fn edit_transfer_item(
    db: &SqlitePool,
    actor: VisibilitySubject,
    item_id: i64,
    patch: TransferItemPatch,
) -> Result<(), Error> {
    ensure_can_edit_item(db, actor, item_id, &patch).await?;

    let changed_fields = apply_item_patch(db, item_id, patch).await?;
    let event_id = append_transfer_event(
        db,
        actor,
        TransferEventKind::ItemEdited,
        json!({ "item_id": item_id, "changed_fields": changed_fields }),
    )
    .await?;

    invalidate_connected_agreements(db, item_id, event_id).await?;
    Ok(())
}

pub async fn invalidate_connected_agreements(
    db: &SqlitePool,
    item_id: i64,
    event_id: i64,
) -> Result<(), Error> {
    let connected = load_connected_item_and_interaction_ids(db, item_id).await?;

    for target in connected {
        set_agreement_level_zero(db, target, event_id).await?;
    }

    Ok(())
}
```

This keeps the social rule explicit: nobody stays agreed to changed conditions.

## Agreement Policy Function

```rust
pub async fn agreement_policy_satisfied(
    db: &SqlitePool,
    interaction_id: i64,
) -> Result<bool, Error> {
    let interaction = load_interaction(db, interaction_id).await?;
    let transfer = load_transfer(db, interaction.transfer_id).await?;

    match transfer.agreement_type {
        AgreementType::Individual => {
            all_connected_required_subjects_at_level(db, interaction_id, AgreementLevel::Second).await
        }
        AgreementType::Full => {
            all_transfer_required_subjects_at_level(db, interaction.transfer_id, AgreementLevel::Second).await
        }
        AgreementType::Percentage(percentage) => {
            configured_percentage_at_level(
                db,
                interaction.transfer_id,
                percentage,
                AgreementLevel::Second,
            )
            .await
        }
        AgreementType::Dependency => {
            dependency_agreement_at_level(db, interaction_id, AgreementLevel::Second).await
        }
    }
}
```

## History Events

History events are needed, and the system should be able to check whether events are correct.

Events should be append-only. The current Transfer state can be cached, but it should be reproducible from the event log.

```sql
CREATE TABLE transfer_event (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    actor_subject_id INTEGER REFERENCES transfer_visibility_subject(id),
    event_kind TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    payload_json TEXT NOT NULL DEFAULT '{}',
    previous_event_hash TEXT,
    event_hash TEXT,
    validation_state TEXT NOT NULL DEFAULT 'pending',
    validation_error TEXT,
    CHECK (json_valid(payload_json)),
    CHECK (validation_state IN ('pending', 'valid', 'invalid'))
) STRICT;
```

Useful event kinds:

```text
transfer_created
transfer_quantity_changed
item_created
item_edited
interaction_created
interaction_edited
visibility_changed
agreement_changed
message_sent
delivery_confirmed
receipt_confirmed
settlement_applied
settlement_reverted
dispute_opened
dispute_resolved
```

Validation example:

```rust
pub fn validate_transfer_event(
    previous: Option<&TransferEvent>,
    event: &TransferEvent,
    state_before: &TransferState,
) -> Result<(), TransferEventError> {
    if event.previous_event_hash != previous.map(|event| event.event_hash.clone()) {
        return Err(TransferEventError::BrokenEventChain);
    }

    match event.event_kind {
        TransferEventKind::ItemEdited => validate_item_edit_event(event, state_before),
        TransferEventKind::AgreementChanged => validate_agreement_event(event, state_before),
        TransferEventKind::SettlementApplied => validate_settlement_event(event, state_before),
        TransferEventKind::VisibilityChanged => validate_visibility_event(event, state_before),
        _ => Ok(()),
    }
}
```

## Signed Events

A signed event is an event whose content is hashed and then signed by the actor's Cell or user key. It is not a legal signature. It is a technical proof that says:

- This Cell claims it created this exact event.
- The event payload was not changed after signing.
- The event belongs after a specific previous event hash.
- Another Cell can verify the event without trusting the server that relayed it.

Conceptually:

```text
event_hash = hash(transfer_id, previous_event_hash, event_kind, actor, created_at, payload_json)
signature = sign(actor_private_key, event_hash)
```

Verification:

```rust
pub fn verify_signed_transfer_event(
    event: &TransferEvent,
    actor_public_key: &PublicKey,
) -> Result<(), TransferEventError> {
    let expected_hash = calculate_transfer_event_hash(event);

    if event.event_hash != expected_hash {
        return Err(TransferEventError::EventHashMismatch);
    }

    if !verify_signature(actor_public_key, &event.event_hash, &event.signature) {
        return Err(TransferEventError::InvalidSignature);
    }

    Ok(())
}
```

The MVP can leave `signature` and public keys out of the schema or store them as optional fields. The important part is to design events so they can be signed later: deterministic payload, previous hash, event hash, actor identity, and validation.

## Messages

Transfers need messages because negotiation and execution are social.

Messages should not be mixed into arbitrary comments only. They need visibility and event history.

```sql
CREATE TABLE transfer_message (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_id INTEGER REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    sender_subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id),
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    event_id INTEGER REFERENCES transfer_event(id),
    CHECK (length(trim(body)) > 0)
) STRICT;
```

```rust
pub async fn send_transfer_message(
    db: &SqlitePool,
    sender: VisibilitySubject,
    transfer_id: i64,
    interaction_id: Option<i64>,
    body: String,
) -> Result<(), Error> {
    ensure_can_view_transfer(db, sender, transfer_id).await?;
    ensure_can_message_transfer(db, sender, transfer_id).await?;

    let message_id = insert_transfer_message(db, sender, transfer_id, interaction_id, body).await?;
    append_transfer_event(
        db,
        sender,
        TransferEventKind::MessageSent,
        json!({ "message_id": message_id }),
    )
    .await?;

    Ok(())
}
```
