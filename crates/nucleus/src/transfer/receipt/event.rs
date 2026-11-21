//! Public event payloads, signing input, and validation against a receipt scope.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ApprovalRole, Measurement, RecordedFloat, Scope, Stage, digest};
use crate::{DecimalValue, karma::CanonicalHash, transfer::outcome::Role};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    Need {
        measurement: Measurement,
        requested: DecimalValue,
        need_commitment: CanonicalHash,
    },
    Statement {
        role: Role,
        measurement: Measurement,
        quantity: DecimalValue,
        need: Option<CanonicalHash>,
        withdrawn: bool,
    },
    Settled {
        quantity: RecordedFloat,
    },
    SettlementStatus {
        quantity: RecordedFloat,
        remaining_after: RecordedFloat,
    },
    Cancelled {
        quantity: DecimalValue,
        revision: u64,
    },
    Compensated {
        role: Role,
        settlement: CanonicalHash,
        application: CanonicalHash,
    },
    Dispute {
        role: Role,
        active: bool,
    },
}

impl Content {
    pub fn stage(&self) -> Stage {
        match self {
            Self::Need { .. } => Stage::Need,
            Self::Statement {
                role: Role::Giver, ..
            } => Stage::Giver,
            Self::Statement {
                role: Role::Receiver,
                ..
            } => Stage::Receiver,
            Self::Settled { .. } => Stage::Settled,
            Self::SettlementStatus { .. } => Stage::SettlementStatus,
            Self::Cancelled { .. } => Stage::Cancelled,
            Self::Compensated { .. } => Stage::Compensated,
            Self::Dispute { .. } => Stage::Dispute,
        }
    }

    pub fn role(&self) -> ApprovalRole {
        match self {
            Self::Need { .. }
            | Self::Statement {
                role: Role::Receiver,
                ..
            }
            | Self::Dispute {
                role: Role::Receiver,
                ..
            }
            | Self::Compensated {
                role: Role::Receiver,
                ..
            } => ApprovalRole::Receiver,
            Self::Statement {
                role: Role::Giver, ..
            }
            | Self::Dispute {
                role: Role::Giver, ..
            }
            | Self::Compensated {
                role: Role::Giver, ..
            } => ApprovalRole::Giver,
            Self::Settled { .. } | Self::SettlementStatus { .. } | Self::Cancelled { .. } => {
                ApprovalRole::Origin
            }
        }
    }

    pub fn measurement(&self) -> Option<&Measurement> {
        match self {
            Self::Need { measurement, .. } | Self::Statement { measurement, .. } => {
                Some(measurement)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub scope: CanonicalHash,
    pub source_commitment: CanonicalHash,
    pub recorded_at: i64,
    pub issued_at: i64,
    pub supersedes: Option<CanonicalHash>,
    pub content: Content,
}

impl Event {
    pub fn id(&self) -> Result<CanonicalHash, String> {
        digest("lince.transfer.receipt.event.v1", self)
    }

    pub fn slot(&self, scope: &Scope) -> Result<CanonicalHash, String> {
        if let Content::Compensated {
            role, settlement, ..
        } = &self.content
        {
            return digest(
                "lince.transfer.receipt.compensation-slot.v1",
                &(
                    &scope.source.origin,
                    &scope.source.occurrence,
                    role,
                    settlement,
                ),
            );
        }
        digest(
            "lince.transfer.receipt.slot.v1",
            &(
                &scope.source.origin,
                &scope.source.occurrence,
                self.content.stage(),
                self.content.role(),
                self.content.measurement(),
            ),
        )
    }

    #[cfg(test)]
    pub(super) fn validate(&self, scope: &Scope, now: i64) -> Result<(), String> {
        self.validate_for_scope(scope, &scope.id()?, now)
    }

    pub(in crate::transfer) fn validate_for_scope(
        &self,
        scope: &Scope,
        scope_id: &CanonicalHash,
        now: i64,
    ) -> Result<(), String> {
        if &self.scope != scope_id
            || self.recorded_at <= 0
            || (matches!(
                self.content,
                Content::Settled { .. }
                    | Content::SettlementStatus { .. }
                    | Content::Cancelled { .. }
                    | Content::Compensated { .. }
                    | Content::Dispute { .. }
            ) && (self.recorded_at < scope.source.activated_at
                || self.recorded_at > self.issued_at))
            || self.issued_at < scope.issued_at
            || self.issued_at >= scope.expires_at
            || self.issued_at > now.saturating_add(300)
            || !scope.stages.contains(&self.content.stage())
        {
            return Err("Receipt event is outside its approved scope or term".into());
        }
        if let Some(measurement) = self.content.measurement()
            && !scope.measurements.contains(measurement)
        {
            return Err("Receipt event uses an unapproved measurement".into());
        }
        match &self.content {
            Content::Need { requested, .. } if !requested.is_positive() => {
                Err("A public Need binding requires a positive requested amount".into())
            }
            Content::Statement {
                quantity,
                withdrawn,
                ..
            } if quantity.is_negative() || (*withdrawn && !quantity.is_zero()) => {
                Err("Use a nonnegative outcome; withdrawal requires zero".into())
            }
            Content::Settled { quantity } => quantity.value().map(|_| ()),
            Content::SettlementStatus {
                quantity,
                remaining_after,
            } => {
                quantity.value()?;
                remaining_after.value().map(|_| ())
            }
            Content::Cancelled { quantity, revision }
                if !quantity.is_positive() || *revision <= scope.source.agreement_revision =>
            {
                Err("Cancellation requires a positive amount and a later agreed revision".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SignedEvent {
    pub event: Event,
    pub key_id: String,
    pub signature: String,
}

impl SignedEvent {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        Ok(digest(
            "lince.transfer.receipt.signed-event.v1",
            &(&self.event, &self.key_id),
        )?
        .as_str()
        .as_bytes()
        .to_vec())
    }
}
