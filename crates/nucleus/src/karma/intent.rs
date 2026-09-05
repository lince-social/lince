use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    CanonicalHash, Capability, DecimalValue, DelegationGrantRevision, GrantAuthorityDecision,
    GrantAuthorityRequest, GrantBudget, GrantTarget, IntentStatus, LiteralValue, LocalId, Slug,
    TimestampMs, TypedUid, canonical_hash,
};
use crate::karma::KarmaBoundaryError;

pub const INTENT_HASH_DOMAIN: &str = "karma.intent.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KarmaIntentSchema {
    V1,
}

pub const K5_2_INTENT_STATES: [IntentStatus; 2] =
    [IntentStatus::Authorized, IntentStatus::Cancelled];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntentTransitionSchema {
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentTransition {
    pub schema: IntentTransitionSchema,
    pub intent_hash: CanonicalHash,
    pub state_revision: u64,
    pub previous_event_hash: Option<CanonicalHash>,
    pub cause_request_id: String,
    pub actor_person_uid: String,
    pub status: IntentStatus,
    pub reason: Option<String>,
}

impl IntentTransition {
    pub fn event_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash("karma.intent-event.v1", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentAmount {
    pub unit_uid: TypedUid,
    pub amount: DecimalValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BudgetDenial {
    IntentCapExhausted,
    WindowCapExhausted,
    WindowUnavailable,
    QuantityExhausted,
    QuantityRequired,
    QuantityUnitMismatch,
    QuantityScaleMismatch,
    QuantityNotPositive,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetUsage {
    pub intents: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_index: Option<u64>,
    pub window_intents: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<DecimalValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetSnapshot {
    pub budget: GrantBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_index: Option<u64>,
    pub intents_before: u64,
    pub intents_after: u64,
    pub window_intents_before: u64,
    pub window_intents_after: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity_before: Option<DecimalValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity_after: Option<DecimalValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentAuthorization {
    pub schema: KarmaIntentSchema,
    pub grant_uid: String,
    pub grant_handle_revision: u64,
    pub grant_revision_hash: CanonicalHash,
    pub request: GrantAuthorityRequest,
    pub decision: GrantAuthorityDecision,
    pub budget: BudgetSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaIntent {
    pub schema: KarmaIntentSchema,
    pub candidate_hash: CanonicalHash,
    pub program_uid: String,
    pub program_revision_hash: CanonicalHash,
    pub template: Slug,
    pub capability: Capability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<GrantTarget>,
    pub fields: BTreeMap<LocalId, LiteralValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<IntentAmount>,
    pub idempotency_key: String,
    pub deadline: TimestampMs,
    pub authorization: IntentAuthorization,
}

impl KarmaIntent {
    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.schema != KarmaIntentSchema::V1 {
            return Err(KarmaBoundaryError::invalid_input(
                "intent schema is unsupported",
            ));
        }
        if self.idempotency_key.is_empty()
            || self.idempotency_key.len() > 200
            || self.idempotency_key.trim() != self.idempotency_key
            || self.idempotency_key.chars().any(char::is_control)
        {
            return Err(KarmaBoundaryError::invalid_input(
                "intent idempotency key must contain 1 to 200 trimmed non-control bytes",
            ));
        }
        if let Some(target) = &self.target {
            target.validate()?;
        }
        if !self.authorization.decision.allowed {
            return Err(KarmaBoundaryError::invalid_input(
                "an intent cannot freeze a denied authority decision",
            ));
        }
        self.authorization.request.validate()?;
        Ok(())
    }

    pub fn intent_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.validate()?;
        canonical_hash(INTENT_HASH_DOMAIN, self)
    }
}

pub fn proposal_amount(
    fields: &BTreeMap<LocalId, LiteralValue>,
) -> Result<Option<IntentAmount>, KarmaBoundaryError> {
    let mut found = None;
    for value in fields.values() {
        if let LiteralValue::Quantity { amount, unit } = value {
            if found.is_some() {
                return Err(KarmaBoundaryError::invalid_input(
                    "a budgeted proposal must carry exactly one quantity field",
                ));
            }
            found = Some(IntentAmount {
                unit_uid: unit.clone(),
                amount: *amount,
            });
        }
    }
    Ok(found)
}

pub fn proposal_target(
    fields: &BTreeMap<LocalId, LiteralValue>,
) -> Result<Option<GrantTarget>, KarmaBoundaryError> {
    let mut found: Option<GrantTarget> = None;
    for value in fields.values() {
        let LiteralValue::Reference { value } = value else {
            continue;
        };
        let target = match value.target.kind() {
            super::ReferenceKind::Record => GrantTarget::Record(value.target.clone()),
            super::ReferenceKind::Concept | super::ReferenceKind::Unit => {
                GrantTarget::Concept(value.target.clone())
            }
            super::ReferenceKind::Person => GrantTarget::Person(value.target.clone()),
            super::ReferenceKind::Organ => GrantTarget::Organ(value.target.clone()),
            super::ReferenceKind::Place => GrantTarget::Place(value.target.clone()),
            _ => continue,
        };
        if found.is_some() {
            return Err(KarmaBoundaryError::invalid_input(
                "a proposal must carry at most one targetable reference",
            ));
        }
        found = Some(target);
    }
    Ok(found)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentAuthorizationOutcome {
    pub allowed: bool,
    pub decision: GrantAuthorityDecision,
    pub budget_denials: Vec<BudgetDenial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetSnapshot>,
}

pub fn authorize_intent(
    revision: &DelegationGrantRevision,
    request: &GrantAuthorityRequest,
    usage: &BudgetUsage,
    amount: Option<&IntentAmount>,
) -> Result<IntentAuthorizationOutcome, KarmaBoundaryError> {
    revision.validate()?;
    request.validate()?;
    let decision = revision.evaluate(request);
    let budget = &revision.spec.budget;
    let mut denials = Vec::new();

    let intents_after = usage.intents.saturating_add(1);
    if let Some(cap) = budget.max_intents
        && intents_after > cap
    {
        denials.push(BudgetDenial::IntentCapExhausted);
    }

    let window_index = budget.window_index(revision.spec.valid_from, request.logical_at);
    let window_intents_after = usage.window_intents.saturating_add(1);
    if let Some(window) = &budget.per_window {
        match window_index {
            Some(index) if usage.window_index == Some(index) => {
                if window_intents_after > window.count {
                    denials.push(BudgetDenial::WindowCapExhausted);
                }
            }
            _ => denials.push(BudgetDenial::WindowUnavailable),
        }
    }

    let mut quantity_before = None;
    let mut quantity_after = None;
    if let Some(limit) = &budget.quantity_limit {
        match amount {
            None => denials.push(BudgetDenial::QuantityRequired),
            Some(amount) if amount.unit_uid != limit.unit_uid => {
                denials.push(BudgetDenial::QuantityUnitMismatch)
            }
            Some(amount) if amount.amount.scale() != limit.limit.scale() => {
                denials.push(BudgetDenial::QuantityScaleMismatch)
            }
            Some(amount) if amount.amount.mantissa() <= 0 => {
                denials.push(BudgetDenial::QuantityNotPositive)
            }
            Some(amount) => {
                let before = match &usage.quantity {
                    Some(consumed) if consumed.scale() == limit.limit.scale() => *consumed,
                    Some(_) => {
                        denials.push(BudgetDenial::QuantityScaleMismatch);
                        DecimalValue::from_mantissa(limit.limit.scale(), 0)?
                    }
                    None => DecimalValue::from_mantissa(limit.limit.scale(), 0)?,
                };
                match before.checked_add(amount.amount) {
                    Some(after) if after.mantissa() <= limit.limit.mantissa() => {
                        quantity_before = Some(before);
                        quantity_after = Some(after);
                    }
                    _ => denials.push(BudgetDenial::QuantityExhausted),
                }
            }
        }
    }

    let allowed = decision.allowed && denials.is_empty();
    Ok(IntentAuthorizationOutcome {
        allowed,
        decision,
        budget_denials: denials,
        budget: allowed.then(|| BudgetSnapshot {
            budget: budget.clone(),
            window_index,
            intents_before: usage.intents,
            intents_after,
            window_intents_before: usage.window_intents,
            window_intents_after,
            quantity_before,
            quantity_after,
        }),
    })
}
