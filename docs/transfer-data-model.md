# Data Model

The current `transfer` and `transfer_item` tables are too small for the complete feature, but they point in the right direction.

## transfer

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

## Nested Transfers

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

## Typed Transfer Options

SQL can store options as text, but application code should not pass arbitrary strings around. Parse at the boundary, then use Rust types.

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

## transfer_party

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

## transfer_item

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

## Extensions

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

## Interactions And Dependencies

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

CREATE TABLE transfer_dependency (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    blocked_interaction_id INTEGER NOT NULL REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    required_interaction_id INTEGER NOT NULL REFERENCES transfer_interaction(id) ON DELETE CASCADE,
    dependency_kind TEXT NOT NULL DEFAULT 'must_settle',
    CHECK (dependency_kind IN ('must_agree', 'must_activate', 'must_deliver', 'must_receive', 'must_settle'))
) STRICT;
```

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
