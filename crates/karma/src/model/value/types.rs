//! Value descriptors and validated port contracts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::CandidateRoute;
use crate::{
    DurationMs, KarmaBoundaryError, LocalId, ReferenceKind, Slug, TypedUid,
    exact::MAX_DECIMAL_SCALE,
};

pub const MAX_VALUE_TYPE_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum ValueType {
    Bool,
    I64,
    Decimal {
        scale: u8,
    },
    Probability,
    Confidence,
    Text,
    Duration,
    Timestamp,
    Quantity {
        scale: u8,
        unit: TypedUid,
    },
    Reference {
        target: ReferenceKind,
    },
    List {
        item: Box<ValueType>,
    },
    Set {
        item: Box<ValueType>,
    },
    Map {
        key: Box<ValueType>,
        value: Box<ValueType>,
    },
    Datum {
        value: Box<ValueType>,
    },
    Estimate {
        value: Box<ValueType>,
    },
    Candidate {
        route: CandidateRoute,
        template: Slug,
        fields: BTreeMap<LocalId, ValueType>,
    },
}

impl ValueType {
    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        self.validate_at_depth(0)
    }

    fn validate_at_depth(&self, depth: usize) -> Result<(), KarmaBoundaryError> {
        if depth > MAX_VALUE_TYPE_DEPTH {
            return Err(KarmaBoundaryError::invalid_definition(format!(
                "value type nesting exceeds {MAX_VALUE_TYPE_DEPTH}"
            )));
        }
        match self {
            Self::Decimal { scale } => validate_scale(*scale),
            Self::Quantity { scale, unit } => {
                validate_scale(*scale)?;
                if unit.kind() != ReferenceKind::Unit {
                    return Err(KarmaBoundaryError::invalid_definition(
                        "quantity unit must be a unit reference",
                    ));
                }
                Ok(())
            }
            Self::List { item }
            | Self::Set { item }
            | Self::Datum { value: item }
            | Self::Estimate { value: item } => item.validate_at_depth(depth + 1),
            Self::Map { key, value } => {
                key.validate_at_depth(depth + 1)?;
                value.validate_at_depth(depth + 1)?;
                if key.is_canonical_map_key() {
                    Ok(())
                } else {
                    Err(KarmaBoundaryError::invalid_definition(
                        "map key type must have a canonical scalar ordering",
                    ))
                }
            }
            Self::Candidate { fields, .. } => {
                for value_type in fields.values() {
                    value_type.validate_at_depth(depth + 1)?;
                }
                Ok(())
            }
            Self::Bool
            | Self::I64
            | Self::Probability
            | Self::Confidence
            | Self::Text
            | Self::Duration
            | Self::Timestamp
            | Self::Reference { .. } => Ok(()),
        }
    }

    fn is_canonical_map_key(&self) -> bool {
        matches!(
            self,
            Self::Bool
                | Self::I64
                | Self::Decimal { .. }
                | Self::Text
                | Self::Duration
                | Self::Timestamp
                | Self::Reference { .. }
        )
    }

    pub fn is_ordered_scalar(&self) -> bool {
        matches!(
            self,
            Self::I64
                | Self::Decimal { .. }
                | Self::Probability
                | Self::Confidence
                | Self::Text
                | Self::Duration
                | Self::Timestamp
                | Self::Quantity { .. }
                | Self::Reference { .. }
        )
    }

    pub fn is_additive(&self) -> bool {
        matches!(
            self,
            Self::I64 | Self::Decimal { .. } | Self::Duration | Self::Quantity { .. }
        )
    }

    pub fn is_negatable(&self) -> bool {
        self.is_additive()
    }
}

fn validate_scale(scale: u8) -> Result<(), KarmaBoundaryError> {
    if scale <= MAX_DECIMAL_SCALE {
        Ok(())
    } else {
        Err(KarmaBoundaryError::invalid_definition(format!(
            "decimal scale {scale} exceeds maximum {MAX_DECIMAL_SCALE}"
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum Sensitivity {
    Public,
    Shared,
    Private,
    Secret,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct PortContract {
    pub value_type: ValueType,
    pub sensitivity: Sensitivity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freshness: Option<DurationMs>,
}

impl PortContract {
    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        self.value_type.validate()?;
        if self.freshness.is_some_and(|duration| duration.get() < 0) {
            return Err(KarmaBoundaryError::invalid_definition(
                "port freshness cannot be negative",
            ));
        }
        Ok(())
    }
}
