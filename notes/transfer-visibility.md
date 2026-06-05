# Visibility

Visibility is the first concrete design because it affects every other table.

## Requirements

We need to control:

- Which Records can be seen.
- Which Transfers can be seen.
- Which Transfer items can be seen.
- Which properties of each Transfer or item can be seen.
- Which parties, personal Organs, or shared Organs can see them.

This must support field-level checkboxes. For example, a party may see `transfer_item.title` and `transfer_item.description`, but not `record.head`, `record.body`, `record_id`, exact quantity, location, or other parties.

## Tables

```sql
CREATE TABLE transfer_visibility_subject (
    id INTEGER PRIMARY KEY,
    subject_kind TEXT NOT NULL,
    local_user_id INTEGER,
    organ_id INTEGER,
    display_name_snapshot TEXT,
    CHECK (subject_kind IN ('user', 'organ', 'public'))
) STRICT;

CREATE TABLE transfer_visibility_rule (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id) ON DELETE CASCADE,
    scope_kind TEXT NOT NULL,
    scope_id INTEGER,
    can_discover INTEGER NOT NULL DEFAULT 0,
    can_view INTEGER NOT NULL DEFAULT 0,
    can_edit INTEGER NOT NULL DEFAULT 0,
    can_agree INTEGER NOT NULL DEFAULT 0,
    can_confirm_delivery INTEGER NOT NULL DEFAULT 0,
    can_confirm_receipt INTEGER NOT NULL DEFAULT 0,
    can_settle INTEGER NOT NULL DEFAULT 0,
    CHECK (scope_kind IN ('transfer', 'transfer_item', 'transfer_event', 'record'))
) STRICT;

CREATE TABLE transfer_visibility_field (
    id INTEGER PRIMARY KEY,
    visibility_rule_id INTEGER NOT NULL REFERENCES transfer_visibility_rule(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    visible INTEGER NOT NULL DEFAULT 0,
    editable INTEGER NOT NULL DEFAULT 0,
    redaction_label TEXT,
    UNIQUE (visibility_rule_id, field_name)
) STRICT;
```

## Field Examples

```text
transfer.title
transfer.description
transfer.quantity
transfer.status
transfer_item.role
transfer_item.title
transfer_item.description
transfer_item.quantity
transfer_item.unit
transfer_item.record_id
transfer_item.record_head_snapshot
transfer_item.record_body_snapshot
transfer_item.location
transfer_item.delivery_window
transfer_party.display_name
transfer_party.organ_id
transfer_event.message
record.head
record.body
record.quantity
```

## Function Example

```rust
pub struct VisibleTransferItem {
    pub id: i64,
    pub role: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub record_id: Option<i64>,
    pub record_head_snapshot: Option<String>,
    pub record_body_snapshot: Option<String>,
}

pub async fn visible_transfer_item(
    db: &SqlitePool,
    subject: VisibilitySubject,
    item_id: i64,
) -> Result<VisibleTransferItem, Error> {
    let item = load_transfer_item(db, item_id).await?;
    let fields = load_visible_fields(db, subject, "transfer_item", item_id).await?;

    Ok(VisibleTransferItem {
        id: item.id,
        role: fields.value("transfer_item.role", item.role),
        title: fields.value("transfer_item.title", item.title),
        description: fields.value("transfer_item.description", item.description),
        quantity: fields.value("transfer_item.quantity", item.quantity),
        unit: fields.value("transfer_item.unit", item.unit),
        record_id: fields.value("transfer_item.record_id", item.record_id),
        record_head_snapshot: fields.value("transfer_item.record_head_snapshot", item.record_head_snapshot),
        record_body_snapshot: fields.value("transfer_item.record_body_snapshot", item.record_body_snapshot),
    })
}
```

## UX Example

```text
Subject: Restaurant Organ

[x] Can discover this Transfer
[x] Can view Transfer title
[x] Can view Transfer item title
[x] Can view Transfer item description
[ ] Can view source Record id
[ ] Can view source Record head
[ ] Can view source Record body
[x] Can view quantity
[ ] Can view other invited Organs
[x] Can send messages
[x] Can agree
[x] Can confirm receipt
```

This supports the guitar example: the restaurant can contribute to the private `Play Guitar` Record without knowing the private Record head.
