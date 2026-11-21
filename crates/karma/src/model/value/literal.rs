//! Literal typing and comparison for evaluated values.

use std::{cmp::Ordering, collections::BTreeMap};

use serde::{Deserialize, Serialize};

use super::{CandidateRoute, DecimalValue, ValueType};
use crate::{
    Confidence, DatumState, DurationMs, KarmaBoundaryError, LocalId, Probability, ReferenceKind,
    ResolvedReference, Slug, TimestampMs, TypedUid,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum LiteralValue {
    Bool {
        value: bool,
    },
    I64 {
        value: i64,
    },
    Decimal {
        value: DecimalValue,
    },
    Probability {
        value: Probability,
    },
    Confidence {
        value: Confidence,
    },
    Text {
        value: String,
    },
    Duration {
        value: DurationMs,
    },
    Timestamp {
        value: TimestampMs,
    },
    Quantity {
        amount: DecimalValue,
        unit: TypedUid,
    },
    Reference {
        value: ResolvedReference,
    },
    Datum {
        value_type: Box<ValueType>,
        state: DatumState,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Box<LiteralValue>>,
    },
    Candidate {
        route: CandidateRoute,
        template: Slug,
        fields: BTreeMap<LocalId, LiteralValue>,
    },
}

impl LiteralValue {
    pub fn value_type(&self) -> Result<ValueType, KarmaBoundaryError> {
        match self {
            Self::Bool { .. } => Ok(ValueType::Bool),
            Self::I64 { .. } => Ok(ValueType::I64),
            Self::Decimal { value } => Ok(ValueType::Decimal {
                scale: value.scale(),
            }),
            Self::Probability { .. } => Ok(ValueType::Probability),
            Self::Confidence { .. } => Ok(ValueType::Confidence),
            Self::Text { .. } => Ok(ValueType::Text),
            Self::Duration { .. } => Ok(ValueType::Duration),
            Self::Timestamp { .. } => Ok(ValueType::Timestamp),
            Self::Quantity { amount, unit } => {
                if unit.kind() != ReferenceKind::Unit {
                    return Err(KarmaBoundaryError::invalid_definition(
                        "quantity literal unit must be a unit reference",
                    ));
                }
                Ok(ValueType::Quantity {
                    scale: amount.scale(),
                    unit: unit.clone(),
                })
            }
            Self::Reference { value } => Ok(ValueType::Reference {
                target: value.target.kind(),
            }),
            Self::Datum {
                value_type,
                state,
                value,
            } => {
                let actual = match (state, value) {
                    (DatumState::Value, Some(value)) => Some(value.value_type()?),
                    (DatumState::Value, None) => {
                        return Err(KarmaBoundaryError::invalid_definition(
                            "datum in value state requires a value",
                        ));
                    }
                    (_, Some(_)) => {
                        return Err(KarmaBoundaryError::invalid_definition(
                            "non-value datum state cannot carry a value",
                        ));
                    }
                    (_, None) => None,
                };
                if actual
                    .as_ref()
                    .is_some_and(|actual| actual != &**value_type)
                {
                    return Err(KarmaBoundaryError::invalid_definition(
                        "datum value does not match its declared inner type",
                    ));
                }
                value_type.validate()?;
                Ok(ValueType::Datum {
                    value: actual.map_or_else(|| value_type.clone(), Box::new),
                })
            }
            Self::Candidate {
                route,
                template,
                fields,
            } => {
                let fields = fields
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), value.value_type()?)))
                    .collect::<Result<_, KarmaBoundaryError>>()?;
                let value_type = ValueType::Candidate {
                    route: *route,
                    template: template.clone(),
                    fields,
                };
                value_type.validate()?;
                Ok(value_type)
            }
        }
    }

    pub(crate) fn semantic_cmp(&self, other: &Self) -> Option<Ordering> {
        let ordering = match (self, other) {
            (Self::I64 { value: left }, Self::I64 { value: right }) => left.cmp(right),
            (Self::Decimal { value: left }, Self::Decimal { value: right }) => left.cmp(right),
            (Self::Probability { value: left }, Self::Probability { value: right }) => {
                left.cmp(right)
            }
            (Self::Confidence { value: left }, Self::Confidence { value: right }) => {
                left.cmp(right)
            }
            (Self::Text { value: left }, Self::Text { value: right }) => left.cmp(right),
            (Self::Duration { value: left }, Self::Duration { value: right }) => left.cmp(right),
            (Self::Timestamp { value: left }, Self::Timestamp { value: right }) => left.cmp(right),
            (Self::Quantity { amount: left, .. }, Self::Quantity { amount: right, .. }) => {
                left.cmp(right)
            }
            (Self::Reference { value: left }, Self::Reference { value: right }) => {
                left.target.cmp(&right.target)
            }
            _ => return None,
        };
        Some(ordering)
    }
}
