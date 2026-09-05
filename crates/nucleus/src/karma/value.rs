use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::{
    Confidence, DatumState, DurationMs, KarmaBoundaryError, LocalId, Probability, ReferenceKind,
    ResolvedReference, Slug, TimestampMs, TypedUid,
    exact::{MAX_DECIMAL_SCALE, canonical_decimal_string, parse_canonical_decimal},
};

pub const MAX_VALUE_TYPE_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateRoute {
    Observe,
    Recommend,
    Draft,
    Ask,
    Act,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rounding {
    HalfUp,
    HalfEven,
    TowardZero,
    AwayFromZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundedDecimal {
    pub value: DecimalValue,
    pub exact: bool,
}

fn pow10(exponent: u32) -> Option<i128> {
    10_i128.checked_pow(exponent)
}

fn round_ratio(
    mut numer: i128,
    mut denom: i128,
    scale: u8,
    rounding: Rounding,
) -> Option<RoundedDecimal> {
    if denom == 0 || scale > MAX_DECIMAL_SCALE {
        return None;
    }
    if denom < 0 {
        numer = numer.checked_neg()?;
        denom = denom.checked_neg()?;
    }

    let quotient = numer / denom;
    let remainder = numer % denom;
    if remainder == 0 {
        return Some(RoundedDecimal {
            value: DecimalValue {
                scale,
                mantissa: quotient,
            },
            exact: true,
        });
    }

    let negative = numer < 0;
    let step = if negative { -1 } else { 1 };
    let twice = remainder.checked_abs()?.checked_mul(2)?;
    let mantissa = match rounding {
        Rounding::TowardZero => quotient,
        Rounding::AwayFromZero => quotient.checked_add(step)?,
        Rounding::HalfUp => {
            if twice >= denom {
                quotient.checked_add(step)?
            } else {
                quotient
            }
        }
        Rounding::HalfEven => match twice.cmp(&denom) {
            std::cmp::Ordering::Greater => quotient.checked_add(step)?,
            std::cmp::Ordering::Less => quotient,
            std::cmp::Ordering::Equal => {
                if quotient % 2 == 0 {
                    quotient
                } else {
                    quotient.checked_add(step)?
                }
            }
        },
    };
    Some(RoundedDecimal {
        value: DecimalValue { scale, mantissa },
        exact: false,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecimalValue {
    scale: u8,
    mantissa: i128,
}

impl DecimalValue {
    pub fn from_mantissa(scale: u8, mantissa: i128) -> Result<Self, KarmaBoundaryError> {
        if scale <= MAX_DECIMAL_SCALE {
            Ok(Self { scale, mantissa })
        } else {
            Err(KarmaBoundaryError::invalid_input(format!(
                "decimal scale {scale} exceeds maximum {MAX_DECIMAL_SCALE}"
            )))
        }
    }

    pub fn parse_canonical(scale: u8, value: &str) -> Result<Self, KarmaBoundaryError> {
        let mantissa = parse_canonical_decimal(scale, value)?;
        Self::from_mantissa(scale, mantissa)
    }

    pub const fn scale(self) -> u8 {
        self.scale
    }

    pub const fn mantissa(self) -> i128 {
        self.mantissa
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        if self.scale != rhs.scale {
            return None;
        }
        self.mantissa
            .checked_add(rhs.mantissa)
            .map(|mantissa| Self {
                scale: self.scale,
                mantissa,
            })
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        if self.scale != rhs.scale {
            return None;
        }
        self.mantissa
            .checked_sub(rhs.mantissa)
            .map(|mantissa| Self {
                scale: self.scale,
                mantissa,
            })
    }

    pub fn checked_neg(self) -> Option<Self> {
        self.mantissa.checked_neg().map(|mantissa| Self {
            scale: self.scale,
            mantissa,
        })
    }

    fn canonical_string(self) -> String {
        canonical_decimal_string(self.scale, self.mantissa)
    }

    pub fn canonical(self) -> String {
        self.canonical_string()
    }

    pub const fn is_zero(self) -> bool {
        self.mantissa == 0
    }

    pub const fn is_negative(self) -> bool {
        self.mantissa < 0
    }

    pub const fn is_positive(self) -> bool {
        self.mantissa > 0
    }

    pub fn rescale(self, scale: u8) -> Option<Self> {
        if scale > MAX_DECIMAL_SCALE {
            return None;
        }
        match scale.cmp(&self.scale) {
            std::cmp::Ordering::Equal => Some(self),
            std::cmp::Ordering::Greater => {
                let factor = 10_i128.checked_pow(u32::from(scale - self.scale))?;
                let mantissa = self.mantissa.checked_mul(factor)?;
                Some(Self { scale, mantissa })
            }
            std::cmp::Ordering::Less => {
                let factor = 10_i128.checked_pow(u32::from(self.scale - scale))?;
                if self.mantissa % factor != 0 {
                    return None;
                }
                Some(Self {
                    scale,
                    mantissa: self.mantissa / factor,
                })
            }
        }
    }

    pub fn aligned_add(self, rhs: Self) -> Option<Self> {
        let scale = self.scale.max(rhs.scale);
        self.rescale(scale)?.checked_add(rhs.rescale(scale)?)
    }

    pub fn aligned_sub(self, rhs: Self) -> Option<Self> {
        let scale = self.scale.max(rhs.scale);
        self.rescale(scale)?.checked_sub(rhs.rescale(scale)?)
    }

    pub fn to_f64(self) -> f64 {
        self.canonical_string().parse::<f64>().unwrap_or(f64::NAN)
    }

    pub fn from_f64_lossy(value: f64) -> Result<Self, KarmaBoundaryError> {
        if !value.is_finite() {
            return Err(KarmaBoundaryError::invalid_input(format!(
                "cannot represent non-finite quantity {value}"
            )));
        }
        let text = format!("{value}");
        let text = if text.contains(['e', 'E']) {
            format!("{value:.*}", usize::from(MAX_DECIMAL_SCALE))
        } else {
            text
        };
        Self::parse_inferred(&text)
    }

    pub fn mul_ratio(
        self,
        numerator: i128,
        denominator: i128,
        scale: u8,
        rounding: Rounding,
    ) -> Option<RoundedDecimal> {
        if denominator == 0 || scale > MAX_DECIMAL_SCALE {
            return None;
        }
        let shift = i32::from(scale) - i32::from(self.scale);
        let mut numer = self.mantissa.checked_mul(numerator)?;
        let mut denom = denominator;
        if shift > 0 {
            numer = numer.checked_mul(pow10(shift.unsigned_abs())?)?;
        } else if shift < 0 {
            denom = denom.checked_mul(pow10(shift.unsigned_abs())?)?;
        }
        round_ratio(numer, denom, scale, rounding)
    }

    pub fn mul_exact(self, other: Self, scale: u8, rounding: Rounding) -> Option<RoundedDecimal> {
        if scale > MAX_DECIMAL_SCALE {
            return None;
        }
        let shift = i32::from(scale) - i32::from(self.scale) - i32::from(other.scale);
        let mut left = self.mantissa;
        let mut right = other.mantissa;
        let mut denom = 1_i128;
        if shift > 0 {
            let numer = left.checked_mul(right)?;
            return round_ratio(
                numer.checked_mul(pow10(shift.unsigned_abs())?)?,
                1,
                scale,
                rounding,
            );
        }
        if shift < 0 {
            let mut remaining = shift.unsigned_abs();
            while remaining > 0 && left % 10 == 0 && left != 0 {
                left /= 10;
                remaining -= 1;
            }
            while remaining > 0 && right % 10 == 0 && right != 0 {
                right /= 10;
                remaining -= 1;
            }
            denom = pow10(remaining)?;
        }
        round_ratio(left.checked_mul(right)?, denom, scale, rounding)
    }

    pub fn div_exact(self, divisor: Self, scale: u8, rounding: Rounding) -> Option<RoundedDecimal> {
        if divisor.mantissa == 0 {
            return None;
        }
        self.mul_ratio(
            pow10(u32::from(divisor.scale))?,
            divisor.mantissa,
            scale,
            rounding,
        )
    }

    pub fn parse_inferred(value: &str) -> Result<Self, KarmaBoundaryError> {
        let fraction_digits = value.split_once('.').map_or(0, |(_, f)| f.len());
        if fraction_digits > usize::from(MAX_DECIMAL_SCALE) {
            return Err(KarmaBoundaryError::invalid_input(format!(
                "decimal scale {fraction_digits} exceeds maximum {MAX_DECIMAL_SCALE}"
            )));
        }
        #[allow(clippy::cast_possible_truncation)]
        let scale = fraction_digits as u8;
        let value = if value.bytes().all(|b| matches!(b, b'-' | b'0' | b'.')) {
            value.trim_start_matches('-')
        } else {
            value
        };
        Self::parse_canonical(scale, value)
    }
}

impl fmt::Display for DecimalValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical_string())
    }
}

#[derive(Serialize, Deserialize)]
struct DecimalValueWire {
    scale: u8,
    value: String,
}

impl Serialize for DecimalValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        DecimalValueWire {
            scale: self.scale,
            value: self.canonical_string(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DecimalValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DecimalValueWire::deserialize(deserializer)?;
        Self::parse_canonical(wire.scale, &wire.value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
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
        fields: std::collections::BTreeMap<LocalId, ValueType>,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
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
        fields: std::collections::BTreeMap<LocalId, LiteralValue>,
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
            } => match (state, value) {
                (DatumState::Value, Some(value)) if value.value_type()? == **value_type => {
                    value_type.validate()?;
                    Ok(ValueType::Datum {
                        value: value_type.clone(),
                    })
                }
                (DatumState::Value, Some(_)) => Err(KarmaBoundaryError::invalid_definition(
                    "datum value does not match its declared inner type",
                )),
                (DatumState::Value, None) => Err(KarmaBoundaryError::invalid_definition(
                    "datum in value state requires a value",
                )),
                (_, Some(_)) => Err(KarmaBoundaryError::invalid_definition(
                    "non-value datum state cannot carry a value",
                )),
                (_, None) => {
                    value_type.validate()?;
                    Ok(ValueType::Datum {
                        value: value_type.clone(),
                    })
                }
            },
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

    pub(crate) fn semantic_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::I64 { value: left }, Self::I64 { value: right }) => Some(left.cmp(right)),
            (Self::Decimal { value: left }, Self::Decimal { value: right }) => {
                Some(left.cmp(right))
            }
            (Self::Probability { value: left }, Self::Probability { value: right }) => {
                Some(left.cmp(right))
            }
            (Self::Confidence { value: left }, Self::Confidence { value: right }) => {
                Some(left.cmp(right))
            }
            (Self::Text { value: left }, Self::Text { value: right }) => Some(left.cmp(right)),
            (Self::Duration { value: left }, Self::Duration { value: right }) => {
                Some(left.cmp(right))
            }
            (Self::Timestamp { value: left }, Self::Timestamp { value: right }) => {
                Some(left.cmp(right))
            }
            (Self::Quantity { amount: left, .. }, Self::Quantity { amount: right, .. }) => {
                Some(left.cmp(right))
            }
            (Self::Reference { value: left }, Self::Reference { value: right }) => {
                Some(left.target.cmp(&right.target))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sensitivity {
    Public,
    Shared,
    Private,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
