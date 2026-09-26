use crate::{Engine, actions::ActionOutcome, error::EngineError};
use chrono::{DateTime, Utc};
use nucleus::{Cause, DecimalValue, NewFact};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use store::sqlx::{Sqlite, Transaction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantityOperation {
    Set,
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl QuantityOperation {
    pub const ALL: [Self; 5] = [
        Self::Set,
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
    ];

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Set => "=",
            Self::Add => "+",
            Self::Subtract => "−",
            Self::Multiply => "×",
            Self::Divide => "÷",
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            Self::Set => "",
            Self::Add => "+=",
            Self::Subtract => "-=",
            Self::Multiply => "*=",
            Self::Divide => "/=",
        }
    }

    pub fn parse(value: &str) -> Option<(Self, DecimalValue)> {
        if value.len() > 128 {
            return None;
        }
        let (operation, operand) = Self::ALL
            .into_iter()
            .skip(1)
            .find_map(|operation| {
                value
                    .strip_prefix(operation.prefix())
                    .map(|operand| (operation, operand))
            })
            .unwrap_or((Self::Set, value.strip_prefix('=').unwrap_or(value)));
        let operand = DecimalValue::parse_inferred(operand.trim()).ok()?;
        (operation != Self::Divide || !operand.is_zero()).then_some((operation, operand))
    }

    pub fn evaluate(self, current: DecimalValue, operand: DecimalValue) -> Option<DecimalValue> {
        match self {
            Self::Set => Some(operand),
            Self::Add => current.aligned_add(operand),
            Self::Subtract => current.aligned_sub(operand),
            Self::Multiply | Self::Divide => {
                for scale in 0..=nucleus::karma::MAX_DECIMAL_SCALE {
                    let result = if self == Self::Multiply {
                        current.mul_exact(operand, scale, nucleus::karma::Rounding::HalfEven)
                    } else {
                        current.div_exact(operand, scale, nucleus::karma::Rounding::HalfEven)
                    };
                    if let Some(result) = result.filter(|r| r.exact) {
                        return Some(result.value);
                    }
                }
                None
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordChanges {
    pub quantity: Option<String>,
    pub assert: Vec<String>,
    pub retract: Vec<String>,
    #[serde(default)]
    pub assign: Vec<String>,
    #[serde(default)]
    pub unassign: Vec<String>,
}

impl RecordChanges {
    pub fn is_empty(&self) -> bool {
        self.quantity.is_none()
            && self.assert.is_empty()
            && self.retract.is_empty()
            && self.assign.is_empty()
            && self.unassign.is_empty()
    }

    pub fn validate(&self) -> bool {
        self.assert.len() + self.retract.len() + self.assign.len() + self.unassign.len() <= 16
            && self
                .assert
                .iter()
                .chain(&self.retract)
                .chain(&self.assign)
                .chain(&self.unassign)
                .all(|name| !name.trim().is_empty() && name.len() <= 128 && !name.contains(','))
            && self
                .quantity
                .as_ref()
                .is_none_or(|value| QuantityOperation::parse(value).is_some())
            && !self.assert.iter().any(|name| self.retract.contains(name))
            && !self.assign.iter().any(|name| self.unassign.contains(name))
    }

    pub fn merge(&mut self, other: &Self) -> bool {
        if self.quantity.is_some() && other.quantity.is_some() && self.quantity != other.quantity {
            return false;
        }
        if self.assign.iter().any(|name| other.unassign.contains(name))
            || self.unassign.iter().any(|name| other.assign.contains(name))
            || self.assert.iter().any(|name| other.retract.contains(name))
            || self.retract.iter().any(|name| other.assert.contains(name))
        {
            return false;
        }
        let mut next = self.clone();
        if other.quantity.is_some() {
            next.quantity = other.quantity.clone();
        }
        next.assign.extend(other.assign.clone());
        next.unassign.extend(other.unassign.clone());
        next.assign.sort();
        next.assign.dedup();
        next.unassign.sort();
        next.unassign.dedup();
        next.assert.extend(other.assert.clone());
        next.retract.extend(other.retract.clone());
        next.assert.sort();
        next.assert.dedup();
        next.retract.sort();
        next.retract.dedup();
        if !next.validate() {
            return false;
        }
        *self = next;
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordState {
    pub quantity: String,
    pub assertions: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub assignees: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionPreview {
    pub target: String,
    pub changes: RecordChanges,
    pub expected: RecordState,
}

fn invalid(message: &str) -> EngineError {
    EngineError::Conflict {
        code: "area_transition_invalid",
        message: message.into(),
    }
}

async fn state(
    tx: &mut Transaction<'_, Sqlite>,
    target: &str,
    changes: &RecordChanges,
) -> Result<RecordState, EngineError> {
    let exists: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM record WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(target)
    .fetch_one(&mut **tx)
    .await?;
    if exists != 1 {
        return Err(EngineError::UnknownRecord(target.into()));
    }
    let quantity = store::records::quantity_in_transaction(tx, target)
        .await?
        .ok_or_else(|| EngineError::UnknownRecord(target.into()))?
        .to_string();
    let mut assertions = BTreeMap::new();
    for predicate in changes.assert.iter().chain(&changes.retract) {
        let ids: Vec<String> = store::sqlx::query_scalar(
            "SELECT uid FROM record_assertion WHERE subject_uid = ? AND predicate_uid = ?
             AND object_uid IS NULL AND role = 'ordinary' AND retracted_at IS NULL
             ORDER BY uid LIMIT 129",
        )
        .bind(target)
        .bind(predicate)
        .fetch_all(&mut **tx)
        .await?;
        if ids.len() > 128 {
            return Err(invalid("Too many Assertions to change through an Area"));
        }
        assertions.insert(predicate.clone(), ids);
    }
    let mut assignees = BTreeMap::new();
    if !changes.assign.is_empty() || !changes.unassign.is_empty() {
        let predicate: String = store::sqlx::query_scalar(
            "SELECT uid FROM concept WHERE canonical_name = 'assigned-to'",
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| invalid("Create the assigned-to concept before configuring assignments"))?;
        for person in changes.assign.iter().chain(&changes.unassign) {
            let valid: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid = ? AND kind = 'person' AND deleted_at IS NULL)")
                .bind(person).fetch_one(&mut **tx).await?;
            if !valid {
                return Err(invalid("Area assignments require an existing Person"));
            }
            let ids: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid = ? AND predicate_uid = ? AND object_uid = ? AND role = 'ordinary' AND retracted_at IS NULL ORDER BY uid LIMIT 129")
                .bind(target).bind(&predicate).bind(person).fetch_all(&mut **tx).await?;
            if ids.len() > 128 {
                return Err(invalid("Too many assignments to change through an Area"));
            }
            assignees.insert(person.clone(), ids);
        }
    }
    Ok(RecordState {
        quantity,
        assertions,
        assignees,
    })
}

impl Engine {
    async fn canonical_area_changes(
        &self,
        mut changes: RecordChanges,
    ) -> Result<RecordChanges, EngineError> {
        if !changes.validate() || changes.is_empty() {
            return Err(invalid(
                "Choose a valid quantity, Assertion, or assignment change",
            ));
        }
        if let Some(value) = &mut changes.quantity {
            let (operation, operand) = QuantityOperation::parse(value)
                .ok_or_else(|| invalid("Invalid quantity operation"))?;
            *value = format!("{}{operand}", operation.prefix());
        }
        for name in changes.assert.iter_mut().chain(&mut changes.retract) {
            *name = store::concepts::resolve(&self.store.pool, name.trim())
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(name.clone()))?;
        }
        for person in changes.assign.iter_mut().chain(&mut changes.unassign) {
            *person = self.resolve(person).await?;
        }
        changes.assign.sort();
        changes.assign.dedup();
        changes.unassign.sort();
        changes.unassign.dedup();
        changes.assert.sort();
        changes.assert.dedup();
        changes.retract.sort();
        changes.retract.dedup();
        if !changes.validate() {
            return Err(invalid(
                "An Area cannot add and remove the same Assertion or person at once",
            ));
        }
        Ok(changes)
    }

    pub(crate) async fn preview_area_transition(
        &self,
        target: String,
        changes: RecordChanges,
        constraints: RecordChanges,
    ) -> Result<TransitionPreview, EngineError> {
        let target = self.resolve(&target).await?;
        self.reject_direct_transfer_record_mutation(&target).await?;
        let changes = self.canonical_area_changes(changes).await?;
        if !constraints.is_empty() {
            let mut constraints = self.canonical_area_changes(constraints).await?;
            if !constraints.merge(&changes) {
                return Err(invalid("Overlapping Areas request conflicting changes"));
            }
        }
        let mut tx = self.store.pool.begin().await?;
        let expected = state(&mut tx, &target, &changes).await?;
        if let Some(value) = &changes.quantity {
            let (operation, operand) = QuantityOperation::parse(value)
                .ok_or_else(|| invalid("Invalid quantity operation"))?;
            let current = DecimalValue::parse_inferred(&expected.quantity)
                .map_err(|_| invalid("Invalid saved quantity"))?;
            operation.evaluate(current, operand).ok_or_else(|| {
                invalid("Quantity result is too large or cannot be represented exactly")
            })?;
        }
        tx.commit().await?;
        Ok(TransitionPreview {
            target,
            changes,
            expected,
        })
    }

    pub(crate) async fn apply_area_transition(
        &self,
        request_id: String,
        preview: TransitionPreview,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if request_id.is_empty()
            || request_id.len() > 128
            || !request_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(invalid("Invalid Area request identity"));
        }
        if self.resolve(&preview.target).await? != preview.target {
            return Err(invalid("Area changes require a stable Record identity"));
        }
        self.reject_direct_transfer_record_mutation(&preview.target)
            .await?;
        let changes = self.canonical_area_changes(preview.changes.clone()).await?;
        if changes != preview.changes {
            return Err(invalid("Area changes must match their preview"));
        }
        let payload = serde_json::to_string(&preview).map_err(EngineError::Json)?;
        if payload.len() > 32768 {
            return Err(invalid("Area request is too large"));
        }
        let signer = self.signer.lock().await.clone();
        let mut tx = store::write_tx(&self.store.pool).await?;
        let previous: Option<(String, String)> = store::sqlx::query_as(
            "SELECT actor_uid, payload FROM interface_area_transition WHERE request_id = ?",
        )
        .bind(&request_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((owner, previous)) = previous {
            if owner != actor.as_deref().unwrap_or_default() || previous != payload {
                return Err(invalid(
                    "Area request identity was already used for another change",
                ));
            }
            return Ok(ActionOutcome {
                data: Some(serde_json::json!({"replayed": true})),
                ..Default::default()
            });
        }
        let current = state(&mut tx, &preview.target, &changes).await?;
        if current != preview.expected {
            return Err(EngineError::Conflict {
                code: "area_transition_stale",
                message:
                    "The Record changed after the Area preview. Review and arm the Area again."
                        .into(),
            });
        }
        store::assertions::transition_unary(
            &mut tx,
            &preview.target,
            &changes.retract,
            &changes.assert,
            actor.as_deref(),
        )
        .await?;
        if !current.assignees.is_empty() {
            let predicate: String = store::sqlx::query_scalar(
                "SELECT uid FROM concept WHERE canonical_name = 'assigned-to'",
            )
            .fetch_one(&mut *tx)
            .await?;
            for person in &changes.unassign {
                for assertion in &current.assignees[person] {
                    store::assertions::retract_tx(&mut tx, assertion, actor.as_deref()).await?;
                }
            }
            for person in &changes.assign {
                if current.assignees[person].is_empty() {
                    store::assertions::insert_tx(
                        &mut tx,
                        &nucleus::new_uid("a"),
                        store::assertions::NewAssertion {
                            subject_uid: &preview.target,
                            predicate_uid: &predicate,
                            object_uid: Some(person),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity: None,
                            unit_uid: None,
                            asserted_by: actor.as_deref(),
                        },
                    )
                    .await?;
                }
            }
        }
        let mut fact = None;
        if let Some(quantity) = &changes.quantity {
            let before = DecimalValue::parse_inferred(&current.quantity)
                .map_err(|_| invalid("Invalid saved quantity"))?;
            let (operation, operand) = QuantityOperation::parse(quantity)
                .ok_or_else(|| invalid("Invalid quantity operation"))?;
            let target = operation.evaluate(before, operand).ok_or_else(|| {
                invalid("Quantity result is too large or cannot be represented exactly")
            })?;
            if target != before {
                fact = crate::append::append_one_in_transaction(
                    &mut tx,
                    NewFact {
                        actor_uid: actor.clone(),
                        ..NewFact::quantity(
                            preview.target.clone(),
                            store::exact::difference(target, before)?,
                            Cause::user_edit(),
                        )
                    },
                    now,
                    signer.as_ref(),
                )
                .await?;
            }
        }
        store::sqlx::query("INSERT INTO interface_area_transition (request_id, actor_uid, payload, created_at) VALUES (?, ?, ?, ?)")
            .bind(request_id).bind(actor.as_deref().unwrap_or_default()).bind(payload).bind(now.to_rfc3339())
            .execute(&mut *tx).await?;
        tx.commit().await?;
        let mut outcome = ActionOutcome::default();
        if let Some(fact) = fact {
            outcome.facts = self.observe_committed_fact(fact, now).await?;
        }
        Ok(outcome)
    }
}
