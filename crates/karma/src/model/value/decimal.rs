//! Exact decimal arithmetic, rounding, and canonical encoding.

use std::{cmp::Ordering, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeStruct};

use crate::{
    KarmaBoundaryError,
    exact::{MAX_DECIMAL_SCALE, format_decimal, parse_canonical_decimal},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
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

fn round_ratio(numer: i128, denom: i128, scale: u8, rounding: Rounding) -> Option<RoundedDecimal> {
    if denom == 0 || scale > MAX_DECIMAL_SCALE {
        return None;
    }
    let (numer, denom) = if denom < 0 {
        (numer.checked_neg()?, denom.checked_neg()?)
    } else {
        (numer, denom)
    };
    let quotient = numer / denom;
    let remainder = numer % denom;
    let exact = remainder == 0;
    let increment = if exact {
        false
    } else {
        let twice = remainder.checked_abs()?.checked_mul(2)?;
        match rounding {
            Rounding::TowardZero => false,
            Rounding::AwayFromZero => true,
            Rounding::HalfUp => twice >= denom,
            Rounding::HalfEven => twice > denom || (twice == denom && quotient % 2 != 0),
        }
    };
    let mantissa = if increment {
        quotient.checked_add(numer.signum())?
    } else {
        quotient
    };
    Some(RoundedDecimal {
        value: DecimalValue { scale, mantissa },
        exact,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema)]
#[schemars(with = "DecimalValueWire")]
pub struct DecimalValue {
    scale: u8,
    mantissa: i128,
}

impl DecimalValue {
    pub const fn from_integer(value: i128) -> Self {
        Self {
            scale: 0,
            mantissa: value,
        }
    }

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
        Ok(Self { scale, mantissa })
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

    pub fn canonical(self) -> String {
        self.to_string()
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

    /// Compares numeric values across scales, including fractional remainders.
    pub fn exact_numeric_cmp(self, rhs: Self) -> Ordering {
        match self.scale.cmp(&rhs.scale) {
            Ordering::Equal => self.mantissa.cmp(&rhs.mantissa),
            Ordering::Greater => rhs.exact_numeric_cmp(self).reverse(),
            Ordering::Less => {
                let factor = 10_i128.pow(u32::from(rhs.scale - self.scale));
                self.mantissa
                    .cmp(&(rhs.mantissa / factor))
                    .then_with(|| 0.cmp(&(rhs.mantissa % factor)))
            }
        }
    }

    pub fn rescale(self, scale: u8) -> Option<Self> {
        if scale == self.scale {
            return Some(self);
        }
        if scale > MAX_DECIMAL_SCALE {
            return None;
        }
        let factor = pow10(u32::from(scale.abs_diff(self.scale)))?;
        let mantissa = if scale > self.scale {
            self.mantissa.checked_mul(factor)?
        } else if self.mantissa % factor == 0 {
            self.mantissa / factor
        } else {
            return None;
        };
        Some(Self { scale, mantissa })
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
        self.canonical().parse::<f64>().unwrap_or(f64::NAN)
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
        let (mut left, mut right) = (self.mantissa, other.mantissa);
        let denom = if shift < 0 {
            let mut remaining = shift.unsigned_abs();
            for factor in [&mut left, &mut right] {
                while remaining > 0 && *factor % 10 == 0 && *factor != 0 {
                    *factor /= 10;
                    remaining -= 1;
                }
            }
            pow10(remaining)?
        } else {
            1
        };
        let mut numer = left.checked_mul(right)?;
        if shift > 0 {
            numer = numer.checked_mul(pow10(shift.unsigned_abs())?)?;
        }
        round_ratio(numer, denom, scale, rounding)
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
        #[allow(
            clippy::cast_possible_truncation,
            reason = "MAX_DECIMAL_SCALE bounds fraction_digits before this cast"
        )]
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
        format_decimal(f, self.scale, self.mantissa)
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct DecimalValueWire {
    scale: u8,
    value: String,
}

impl Serialize for DecimalValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut wire = serializer.serialize_struct("DecimalValueWire", 2)?;
        wire.serialize_field("scale", &self.scale)?;
        wire.serialize_field("value", &format_args!("{self}"))?;
        wire.end()
    }
}

impl<'de> Deserialize<'de> for DecimalValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = DecimalValueWire::deserialize(deserializer)?;
        Self::parse_canonical(wire.scale, &wire.value).map_err(de::Error::custom)
    }
}
