# Transfer

This document is a design note for the complete Transfer feature after the review comments in `transfer_notes.md`.

The current direction is:

- Transfer deals only with Lince data for now.
- Quantity stays central. Services, promises, knowledge, tasks, permissions, and transportation can still be represented in Lince through Records, Record bodies, Transfer items, Karma, and Transfer history.
- A Transfer is a process and a grouping of smaller interactions, not necessarily one indivisible transaction. Transfers can also be nested under a larger parent Transfer.
- Visibility is a first-class table, not a loose property.
- Agreement is invalidated by edits to the connected items that were agreed to.
- Karma should initially only activate/deactivate preconfigured Transfers by changing Transfer quantity, not invent parties or visibility.
- Transfer should expose enough data for SQL and sands to calculate richer quantity views. The Transfer core should not overbuild those projections at first.
- A Lince Cell is modeled as an Organ used by one person. In networking language, that personal Organ can still act as a p2p node that caches and relays public/subscribed Transfer information from other nodes.

## Core Model

A Transfer is a structured promise before it is executed. It can group one or more item-level interactions between Records, parties, personal Organs, or shared Organs.

The basic shape is:

1. Someone creates a Transfer from one or more Records.
2. Each Transfer item describes how a Record means something inside this Transfer.
3. Visibility decides which party, personal Organ, or shared Organ can see which fields.
4. Parties edit their parts of the proposal.
5. Edits reset agreement only for the connected items affected by the edit.
6. Parties raise agreement levels when they accept the current visible state.
7. When the selected agreement policy is satisfied, the relevant item or group becomes active.
8. Transfer influence appears in simulation as plus/minus quantity.
9. Parties confirm delivery and receipt.
10. Settlement applies the actual Lince data changes.

The important distinction is:

- A Record describes something in a personal or shared Organ.
- A Transfer item describes what that Record means in this Transfer.
- A Transfer interaction describes a movement, promise, or dependency between items.
- A parent Transfer groups child Transfers under a larger subject without forcing one agreement policy on every child.

Example:

The Record can be `Play Guitar`, used every day by Karma as a personal task. In one Transfer, that same Record can appear as a Need with item title `Play guitar for a live audience in a restaurant`. The other parties may see only the item title and description, not the private source Record head.

## Corrected Assumptions

### Quantity Still Works

The earlier claim that quantity is weak for services, promises, knowledge, permissions, access, transportation, or tasks was wrong for Lince.

In Lince, those can work because:

- A promise is exactly what a Transfer is before execution.
- Knowledge can live in a Record body or linked Record.
- A service can be represented by a Record and a quantity of hours, sessions, attempts, visits, or completed units.
- A permission can be represented by a Record state, link, or assignment.
- Transportation can be represented by a Record, route description, quantity, and delivery/receipt events.
- A task can be represented by a Record and later generalized Kanban/work metadata.

Quantity should stay generic. Specialized units like kilograms can be represented by Transfer extensions when needed. The core should not hardcode every unit or domain.

### Need And Contribution Are Transfer Roles

Need and Contribution are contextual. A Record does not permanently become one or the other.

A Transfer item should include:

- The source Record id.
- The role in this Transfer: need, contribution, support, task, information, reservation, etc.
- A Transfer-specific title.
- A Transfer-specific description.
- A visibility policy that can hide the source Record fields.

This lets a private Record be affected by a public Transfer without exposing the private Record identity or head.

### Expiration Should Not Be Hardcoded Yet

Expiration should not be a built-in Transfer property in the first version.

If a Transfer should expire, Karma can change its quantity or active marker:

- `0`: neutral/inactive
- `-1`: active/open/proposed, depending on local convention
- `0` again: inactive/closed by rule

This keeps time behavior inside Karma and avoids hardcoding Transfer-specific expiry before the model stabilizes.

### Counteroffers Are Just Edits

The UX should not be centered on "counteroffers".

Everyone can edit the parts they are allowed to edit. Any edit invalidates agreement levels for connected items because nobody should remain agreed to terms they have not seen.

Agreement returns only when the relevant parties accept the new state.

### Default Agreement Is Individual

A Transfer may contain many interactions:

- `A -> B`
- `B -> C`
- `B -> D`
- `A -> D`

The default mode should be `individual`: each connected interaction can reach agreement and settle independently.

Other agreement modes can exist:

- `full`: every required party and item must agree before anything is active.
- `percentage`: a configured percentage of parties must agree.
- `dependency`: a connected chain or cycle must agree together.

Role-based agreement should be deferred.

## Visibility

Visibility should be the first concrete design because it affects every other table.

We need to control:

- Which Records can be seen.
- Which Transfers can be seen.
- Which Transfer items can be seen.
- Which properties of each Transfer or item can be seen.
- Which parties, personal Organs, or shared Organs can see them.

This must support field-level checkboxes. For example, a party may see `transfer_item.title` and `transfer_item.description`, but not `record.head`, `record.body`, `record_id`, exact quantity, location, or other parties.

### Visibility Tables

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

Field examples:

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

### Visibility Function Example

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

### Visibility UX

The UI should expose visibility as checkboxes grouped by object:

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

## Transfer Tables

The current `transfer` and `transfer_item` tables are too small for the complete feature, but they point in the right direction.

### transfer

The Transfer header groups interactions and carries the activation quantity.

```sql
CREATE TABLE transfer (
    id INTEGER PRIMARY KEY,
    parent_transfer_id INTEGER REFERENCES transfer(id) ON DELETE CASCADE,
    local_uuid TEXT NOT NULL UNIQUE,
    title TEXT,
    description TEXT,
    quantity REAL NOT NULL DEFAULT 0,
    agreement_type TEXT NOT NULL DEFAULT 'individual',
    settlement_mode TEXT NOT NULL DEFAULT 'individual',
    coordinator_organ_id INTEGER,
    created_by_subject_id INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    settled_at TEXT,
    freestyle_data_structure TEXT,
    CHECK (
        agreement_type IN ('individual', 'full', 'dependency')
        OR agreement_type GLOB 'percentage:*'
    ),
    CHECK (settlement_mode IN ('individual', 'full'))
) STRICT;
```

Notes:

- `quantity = 0` means neutral/inactive by default.
- `quantity = -1` can mean active/open when Karma turns the Transfer on.
- No hardcoded `expires_at` for now.
- `agreement_type` should be treated as immutable after creation unless the Transfer is reset.
- `parent_transfer_id` lets a large Transfer group smaller Transfers without forcing them to share one agreement or settlement policy.

### Nested Transfers

Nested Transfers solve the "big party" problem. A parent Transfer can represent the large subject, while child Transfers represent separable parts with their own policies.

Example:

```text
Party Transfer
  - Food Transfer: individual agreement
  - Music Transfer: full agreement
  - Venue Transfer: percentage:0.75 agreement
  - Cleanup Transfer: dependency agreement
```

The parent should not automatically impose its agreement policy on children. It should mostly provide:

- Shared title/context.
- Shared visibility defaults.
- A place to see aggregate state.
- Optional parent-level messages.
- Optional parent-level work metadata.
- Optional dependencies between child Transfers.

Child Transfers keep their own:

- `agreement_type`
- `settlement_mode`
- parties
- visibility overrides
- interactions
- events
- quantity influence

Parent state can be derived from children:

```rust
pub enum ParentTransferState {
    Draft,
    Active,
    PartiallyAgreed,
    Agreed,
    PartiallySettled,
    Settled,
    Disputed,
}

pub async fn derive_parent_transfer_state(
    db: &SqlitePool,
    parent_transfer_id: i64,
) -> Result<ParentTransferState, Error> {
    let child_states = load_child_transfer_states(db, parent_transfer_id).await?;

    if child_states.iter().any(TransferState::is_disputed) {
        return Ok(ParentTransferState::Disputed);
    }

    if child_states.iter().all(TransferState::is_settled) {
        return Ok(ParentTransferState::Settled);
    }

    if child_states.iter().any(TransferState::is_settled) {
        return Ok(ParentTransferState::PartiallySettled);
    }

    if child_states.iter().all(TransferState::is_agreed) {
        return Ok(ParentTransferState::Agreed);
    }

    if child_states.iter().any(TransferState::is_agreed) {
        return Ok(ParentTransferState::PartiallyAgreed);
    }

    Ok(ParentTransferState::Active)
}
```

If child Transfers need ordering, use dependencies between the child Transfers instead of forcing everything into one agreement policy:

```sql
CREATE TABLE transfer_child_dependency (
    id INTEGER PRIMARY KEY,
    parent_transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    blocked_child_transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    required_child_transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    dependency_kind TEXT NOT NULL DEFAULT 'must_settle',
    CHECK (dependency_kind IN ('must_agree', 'must_activate', 'must_deliver', 'must_receive', 'must_settle'))
) STRICT;
```

### Typed Transfer Options

SQL can store options as text, but application code should not pass arbitrary strings around. Parse at the boundary, then use Rust types.

Agreement type:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgreementType {
    Individual,
    Full,
    Percentage(f64),
    Dependency,
}

impl AgreementType {
    pub fn as_storage_string(self) -> String {
        match self {
            Self::Individual => "individual".to_string(),
            Self::Full => "full".to_string(),
            Self::Percentage(value) => format!("percentage:{value}"),
            Self::Dependency => "dependency".to_string(),
        }
    }
}

impl std::str::FromStr for AgreementType {
    type Err = TransferParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "individual" => Ok(Self::Individual),
            "full" => Ok(Self::Full),
            "dependency" => Ok(Self::Dependency),
            value if value.starts_with("percentage:") => {
                let percentage = value
                    .trim_start_matches("percentage:")
                    .parse::<f64>()
                    .map_err(|_| TransferParseError::InvalidAgreementType(value.to_string()))?;

                if !(0.0..=1.0).contains(&percentage) {
                    return Err(TransferParseError::InvalidAgreementPercentage(percentage));
                }

                Ok(Self::Percentage(percentage))
            }
            value => Err(TransferParseError::InvalidAgreementType(value.to_string())),
        }
    }
}
```

Other options should follow the same rule:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementMode {
    Individual,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgreementLevel {
    None = 0,
    First = 1,
    Second = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferRole {
    Need,
    Contribution,
    Support,
    Task,
    Information,
    Reservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Incoming,
    Outgoing,
    Mutual,
    Informational,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferInteractionKind {
    ContributesTo,
    DependsOn,
    Unblocks,
    Replaces,
    Informs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipationKind {
    Participant,
    Coordinator,
    Observer,
    Placeholder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationKind {
    Delivery,
    Receipt,
}
```

Each enum should implement `FromStr` and a storage serializer like `as_storage_str` or `as_storage_string`. The rest of the application should receive these Rust types, not raw storage strings.

Repository rows can still use storage strings, but conversion should happen immediately:

```rust
pub struct TransferRow {
    pub id: i64,
    pub parent_transfer_id: Option<i64>,
    pub agreement_type: String,
    pub settlement_mode: String,
}

pub struct Transfer {
    pub id: i64,
    pub parent_transfer_id: Option<i64>,
    pub agreement_type: AgreementType,
    pub settlement_mode: SettlementMode,
}

impl TryFrom<TransferRow> for Transfer {
    type Error = TransferParseError;

    fn try_from(row: TransferRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            parent_transfer_id: row.parent_transfer_id,
            agreement_type: row.agreement_type.parse()?,
            settlement_mode: row.settlement_mode.parse()?,
        })
    }
}
```

Event kind should also be typed:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferEventKind {
    TransferCreated,
    TransferQuantityChanged,
    PartyAdded,
    ItemCreated,
    ItemEdited,
    InteractionCreated,
    InteractionEdited,
    VisibilityChanged,
    AgreementChanged,
    MessageSent,
    DeliveryConfirmed,
    ReceiptConfirmed,
    SettlementApplied,
    SettlementReverted,
    DisputeOpened,
    DisputeResolved,
}
```

The same applies to event payloads. Avoid `payload.get("field_name")` throughout the code. Deserialize into a typed payload at the boundary:

```rust
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransferEventPayload {
    ItemEdited {
        item_id: i64,
        changed_fields: Vec<String>,
    },
    AgreementChanged {
        interaction_id: Option<i64>,
        item_id: Option<i64>,
        agreement_level: AgreementLevel,
    },
    TransferQuantityChanged {
        transfer_id: i64,
        quantity: f64,
    },
}

pub fn parse_transfer_event_payload(
    event_kind: TransferEventKind,
    payload_json: &str,
) -> Result<TransferEventPayload, TransferParseError> {
    let payload = serde_json::from_str::<TransferEventPayload>(payload_json)?;
    ensure_payload_matches_event_kind(event_kind, &payload)?;
    Ok(payload)
}
```

### transfer_party

A Transfer party connects a Transfer to a visibility subject. The subject controls identity and field access; the party row says how that subject participates in this Transfer.

```sql
CREATE TABLE transfer_party (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id) ON DELETE CASCADE,
    party_label TEXT,
    participation_kind TEXT NOT NULL DEFAULT 'participant',
    can_be_replaced INTEGER NOT NULL DEFAULT 0,
    position REAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (transfer_id, subject_id),
    CHECK (participation_kind IN ('participant', 'coordinator', 'observer', 'placeholder'))
) STRICT;
```

Function example:

```rust
pub async fn add_transfer_party(
    db: &SqlitePool,
    actor: VisibilitySubject,
    transfer_id: i64,
    subject_id: i64,
    participation_kind: ParticipationKind,
) -> Result<(), Error> {
    ensure_can_edit_transfer_parties(db, actor, transfer_id).await?;
    insert_transfer_party(db, transfer_id, subject_id, participation_kind.as_storage_str()).await?;
    append_transfer_event(
        db,
        actor,
        TransferEventKind::PartyAdded,
        json!({ "transfer_id": transfer_id, "subject_id": subject_id }),
    )
    .await?;
    Ok(())
}
```

### transfer_item

A Transfer item describes how a Record participates in a Transfer.

```sql
CREATE TABLE transfer_item (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    source_record_id INTEGER REFERENCES record(id),
    source_organ_id INTEGER,
    role TEXT NOT NULL,
    direction TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    record_head_snapshot TEXT,
    record_body_snapshot TEXT,
    quantity REAL NOT NULL DEFAULT 0,
    unit TEXT,
    quantity_policy TEXT NOT NULL DEFAULT 'exact',
    location TEXT,
    expected_start_at TEXT,
    expected_end_at TEXT,
    expected_duration_seconds REAL,
    position REAL,
    version INTEGER NOT NULL DEFAULT 1,
    state TEXT NOT NULL DEFAULT 'draft',
    freestyle_data_structure TEXT,
    CHECK (role IN ('need', 'contribution', 'support', 'task', 'information', 'reservation')),
    CHECK (direction IN ('incoming', 'outgoing', 'mutual', 'informational')),
    CHECK (quantity_policy IN ('exact', 'up_to', 'at_least', 'range', 'all_available', 'surplus_over')),
    CHECK (state IN ('draft', 'active', 'delivered', 'received', 'settled', 'cancelled', 'disputed'))
) STRICT;
```

The item `title` and `description` are not duplicates of Record head/body. They are the public or contextual meaning of the Record inside this Transfer.

### transfer_extension

Like `record_extension`, uncommon or domain-specific Transfer data can live in extensions.

```sql
CREATE TABLE transfer_extension (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    freestyle_data_structure TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (transfer_id, namespace),
    CHECK (length(trim(namespace)) > 0),
    CHECK (json_valid(freestyle_data_structure))
) STRICT;

CREATE TABLE transfer_item_extension (
    id INTEGER PRIMARY KEY,
    transfer_item_id INTEGER NOT NULL REFERENCES transfer_item(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    freestyle_data_structure TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (transfer_item_id, namespace),
    CHECK (length(trim(namespace)) > 0),
    CHECK (json_valid(freestyle_data_structure))
) STRICT;
```

Example extension payload:

```json
{
  "unit_kind": "mass",
  "unit": "kg",
  "precision": 0.01
}
```

## Transfer Interactions And Dependencies

A Transfer can contain many item-level interactions. An interaction says one item affects another item.

Examples:

- `A gives to B`
- `B gives to C`
- `B gives to D`
- `A gives to D`
- `A -> B -> C`
- `A -> B -> C -> A`

```sql
CREATE TABLE transfer_interaction (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    from_item_id INTEGER NOT NULL REFERENCES transfer_item(id) ON DELETE CASCADE,
    to_item_id INTEGER NOT NULL REFERENCES transfer_item(id) ON DELETE CASCADE,
    interaction_kind TEXT NOT NULL DEFAULT 'contributes_to',
    quantity REAL,
    order_index REAL,
    agreement_group TEXT,
    settlement_group TEXT,
    state TEXT NOT NULL DEFAULT 'draft',
    CHECK (interaction_kind IN ('contributes_to', 'depends_on', 'unblocks', 'replaces', 'informs')),
    CHECK (state IN ('draft', 'active', 'delivered', 'received', 'settled', 'cancelled', 'disputed'))
) STRICT;
```

Order and dependency examples:

```sql
CREATE TABLE transfer_dependency (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    blocked_interaction_id INTEGER NOT NULL REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    required_interaction_id INTEGER NOT NULL REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    dependency_kind TEXT NOT NULL DEFAULT 'must_settle',
    CHECK (dependency_kind IN ('must_agree', 'must_activate', 'must_deliver', 'must_receive', 'must_settle'))
) STRICT;
```

Function example:

```rust
pub async fn can_activate_interaction(
    db: &SqlitePool,
    interaction_id: i64,
) -> Result<bool, Error> {
    let deps = load_interaction_dependencies(db, interaction_id).await?;

    for dep in deps {
        let required_state = load_interaction_state(db, dep.required_interaction_id).await?;
        if !dependency_is_satisfied(dep.dependency_kind, required_state) {
            return Ok(false);
        }
    }

    agreement_policy_satisfied(db, interaction_id).await
}
```

This lets `B -> C` wait for `A -> B` without requiring C to agree to A and B's private interaction, unless the agreement group says the chain or triangle must agree together.

## Agreement

Agreement is about the current state of connected items, not about an abstract counteroffer.

Recommended levels:

| Level | Meaning                                                                                                 |
| ----- | ------------------------------------------------------------------------------------------------------- |
| `0`   | No current agreement, or agreement was invalidated by an edit.                                          |
| `1`   | First agreement: the party reviewed the current visible proposal and is aligned.                        |
| `2`   | Second agreement: the party commits their part and the interaction can activate if policy is satisfied. |

The exact meaning of first and second agreement can evolve in the UI, but level `2` is the useful activation threshold.

### Agreement Table

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

### Invalidating Agreement On Edits

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

### Agreement Policy Function

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

Signatures can come later. The important MVP point is deterministic validation and a clear event chain.

### Signed Events

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

Function example:

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

## Karma

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

Table:

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

Function example:

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

## Simulation And Transfer Influence

Simulation should use a plus/minus model, but the Transfer feature should not try to own every possible quantity projection right away.

Core Transfer should store enough information for SQL queries and sands to calculate richer views. A sand that wants more than `quantity` can request/query properties like proposed, reserved, incoming, outgoing, available, planned, and surplus. That is a presentation/query problem on top of Transfer data.

A useful SQL view or sand-level query could expose:

| Quantity          | Meaning                                                         |
| ----------------- | --------------------------------------------------------------- |
| Actual            | Current Record quantity.                                        |
| Proposed outgoing | Quantity requested by draft/proposed Transfers.                 |
| Proposed incoming | Quantity expected from draft/proposed Transfers.                |
| Reserved outgoing | Quantity promised to agreed/active Transfers.                   |
| Reserved incoming | Quantity expected from agreed/active Transfers.                 |
| Available         | Actual minus relevant outgoing holds.                           |
| Planned           | Actual plus incoming minus outgoing across a selected scenario. |
| Surplus           | Quantity above a user-defined threshold.                        |

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

Function example:

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

Example SQL view for a sand:

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

Function example:

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

Function example:

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

## Intent

Intent can be concrete or open-ended.

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

Function example:

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

## P2P Discovery And Pub/Sub Cache

A Lince Cell should be able to act as a node in a wider network. A node can publish a cache of visible Transfer information, receive caches from other nodes, and choose whether to keep or relay that information.

This is close to pub/sub:

- A Cell publishes topics it wants others to discover.
- Other Cells subscribe to those topics or to specific nodes.
- A server or known node can introduce Cells to each other.
- After introduction, Cells can sync directly.
- A Cell may cache public or permitted Transfer summaries from other Cells.
- A Cell may periodically ask for updates, or keep the cached information as stale background knowledge.
- Visibility rules still decide what fields are included in the cache.

The cache is not source of truth. It is a menu or index of known Needs, Contributions, Transfers, and nodes. It helps a Cell expand its understanding of the world without forcing every node to trust every other node.

### Discovery Server Path

The practical early path:

1. A central or Organ server lists reachable Cells and public topics.
2. A Cell asks the server for nodes matching a topic.
3. The user chooses a Cell to connect to directly.
4. Lince adds that Cell to the Organ/contact list.
5. The local Cell calls the remote Cell directly for visible Transfer summaries.
6. If both Cells choose to participate in a Transfer, they sync the Transfer event log directly or through the coordinator.

This keeps the central server useful for discovery without making it the permanent source of truth for every Transfer.

### Cache Tables

```sql
CREATE TABLE cell_peer (
    id INTEGER PRIMARY KEY,
    name TEXT,
    base_url TEXT NOT NULL,
    organ_id INTEGER REFERENCES organ(id),
    public_key TEXT,
    last_seen_at TEXT,
    trust_state TEXT NOT NULL DEFAULT 'unknown',
    CHECK (trust_state IN ('unknown', 'trusted', 'blocked'))
) STRICT;

CREATE TABLE transfer_pubsub_topic (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT
) STRICT;

CREATE TABLE transfer_pubsub_subscription (
    id INTEGER PRIMARY KEY,
    topic_id INTEGER NOT NULL REFERENCES transfer_pubsub_topic(id) ON DELETE CASCADE,
    peer_id INTEGER REFERENCES cell_peer(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_sync_at TEXT
) STRICT;

CREATE TABLE transfer_discovery_cache (
    id INTEGER PRIMARY KEY,
    peer_id INTEGER NOT NULL REFERENCES cell_peer(id) ON DELETE CASCADE,
    remote_transfer_id TEXT NOT NULL,
    topic_name TEXT,
    summary_json TEXT NOT NULL,
    event_head_hash TEXT,
    fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    stale_after TEXT,
    CHECK (json_valid(summary_json)),
    UNIQUE (peer_id, remote_transfer_id)
) STRICT;
```

Function example:

```rust
pub async fn sync_transfer_topic_from_peer(
    db: &SqlitePool,
    peer: CellPeer,
    topic: String,
) -> Result<(), Error> {
    let summaries = fetch_visible_transfer_summaries(&peer.base_url, &topic).await?;

    for summary in summaries {
        if visibility_summary_is_allowed(&summary) {
            upsert_transfer_discovery_cache(db, peer.id, summary).await?;
        }
    }

    mark_subscription_synced(db, peer.id, topic).await?;
    Ok(())
}
```

This creates a path for "needs of the world" discovery while keeping each Cell in control of what it stores and republishes.

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

| State               | Meaning                                                                  |
| ------------------- | ------------------------------------------------------------------------ |
| `draft`             | Local idea, not visible unless visibility allows it.                     |
| `neutral`           | Transfer exists but quantity is neutral, normally `0`.                   |
| `active`            | Transfer quantity or agreement makes it active, often `-1`.              |
| `partially_agreed`  | Some interactions reached agreement level `2`.                           |
| `agreed`            | Required policy is satisfied.                                            |
| `in_transfer`       | Delivery/receipt process is happening and quantity influence is visible. |
| `partially_settled` | Some interactions have settled.                                          |
| `settled`           | Settlement policy completed.                                             |
| `cancelled`         | Transfer was manually cancelled or neutralized.                          |
| `disputed`          | Parties disagree about agreement, delivery, receipt, or settlement.      |

Avoid hardcoded `expired` for MVP. Expiry-like behavior can be a Karma rule that changes quantity back to neutral.

## Cells, Organs, And Servers

Lince should not require one central global server, but real Transfers need some coordination point.

Recommended model:

- Each Transfer has a coordinator.
- The coordinator can be a local Cell or an Organ server.
- The coordinator orders events and stores the operational source of truth.
- Other Cells can mirror or export/import Transfer events.

MVP:

- Local Cell for personal Transfers.
- Organ server for cross-user Transfers.
- Server as source of truth while the Transfer is active.

Later:

- Start on an Organ server for discovery, then allow direct Cell-to-Cell continuation.
- Add event signatures where the event author can be verified independently from the relay server.
- Add coordinator migration.

Cell-to-cell sync can start before full p2p federation:

- The Organ/contact list stores peer Cells.
- Each peer has a base URL and optional public key.
- A Cell can fetch visible Transfer summaries from a peer.
- A Cell can fetch the event log for a Transfer it participates in.
- The coordinator still orders writes for the active Transfer.
- Direct peers can mirror event logs and cache discovery summaries.

Do not start with full federation. The item-level agreement and visibility model is already enough complexity.

## Kanban And General Work Metadata

The aftermath section in the previous draft overlapped with Kanban. That is a real design signal: expected time, start/end date, assignees, and work state should probably become general-purpose Lince metadata that can attach to Records and Transfers.

For Transfers, useful work metadata includes:

- Expected duration.
- Start date.
- End date.
- Assignees.
- Work status.
- Completion notes.

Possible table:

```sql
CREATE TABLE work_metadata (
    id INTEGER PRIMARY KEY,
    owner_kind TEXT NOT NULL,
    owner_id INTEGER NOT NULL,
    expected_duration_seconds REAL,
    started_at TEXT,
    ended_at TEXT,
    status TEXT,
    freestyle_data_structure TEXT,
    CHECK (owner_kind IN ('record', 'transfer', 'transfer_item', 'transfer_interaction'))
) STRICT;

CREATE TABLE work_assignment (
    id INTEGER PRIMARY KEY,
    work_metadata_id INTEGER NOT NULL REFERENCES work_metadata(id) ON DELETE CASCADE,
    subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id),
    assignment_kind TEXT NOT NULL DEFAULT 'responsible',
    CHECK (assignment_kind IN ('responsible', 'observer', 'helper'))
) STRICT;
```

This keeps Kanban behavior reusable without forcing every Transfer to become a Kanban card.

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

Do not build first:

- External integrations.
- Global reputation.
- Full peer-to-peer federation.
- Hardcoded expiration.
- Role-based agreement.
- Complex legal-contract language.

## Design Principle

Transfer should be a protocol for making Record quantity changes and Record relationships socially valid before they become final database changes.

The system should let people expose only the Transfer-specific meaning they want to expose, agree only to the connected parts they understand, and see how active Transfers influence their Records before settlement.
