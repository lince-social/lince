use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::failure::KarmaBoundaryError;

pub const MAX_DECIMAL_SCALE: u8 = 18;
const PARTS_PER_BILLION: u32 = 1_000_000_000;

/// Exact fixed-scale decimal. The scale is part of the Rust type and its wire
/// value always contains exactly `SCALE` fractional digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FixedDecimal<const SCALE: u8> {
    mantissa: i128,
}

impl<const SCALE: u8> FixedDecimal<SCALE> {
    pub fn from_mantissa(mantissa: i128) -> Result<Self, KarmaBoundaryError> {
        validate_scale::<SCALE>()?;
        Ok(Self { mantissa })
    }

    pub fn mantissa(self) -> i128 {
        self.mantissa
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.mantissa
            .checked_add(rhs.mantissa)
            .map(|mantissa| Self { mantissa })
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.mantissa
            .checked_sub(rhs.mantissa)
            .map(|mantissa| Self { mantissa })
    }

    fn canonical_string(self) -> String {
        canonical_decimal_string(SCALE, self.mantissa)
    }
}

fn validate_scale<const SCALE: u8>() -> Result<(), KarmaBoundaryError> {
    if SCALE <= MAX_DECIMAL_SCALE {
        Ok(())
    } else {
        Err(KarmaBoundaryError::invalid_definition(format!(
            "decimal scale {SCALE} exceeds maximum {MAX_DECIMAL_SCALE}"
        )))
    }
}

impl<const SCALE: u8> fmt::Display for FixedDecimal<SCALE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical_string())
    }
}

impl<const SCALE: u8> FromStr for FixedDecimal<SCALE> {
    type Err = KarmaBoundaryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_scale::<SCALE>()?;
        let mantissa = parse_canonical_decimal(SCALE, value)?;
        Self::from_mantissa(mantissa)
    }
}

pub(super) fn parse_canonical_decimal(scale: u8, value: &str) -> Result<i128, KarmaBoundaryError> {
    if scale > MAX_DECIMAL_SCALE {
        return Err(KarmaBoundaryError::invalid_input(format!(
            "decimal scale {scale} exceeds maximum {MAX_DECIMAL_SCALE}"
        )));
    }
    let (negative, unsigned) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    if unsigned.is_empty() || value.starts_with('+') {
        return Err(decimal_error(scale));
    }
    let (whole, fraction) = if scale == 0 {
        if unsigned.contains('.') {
            return Err(decimal_error(scale));
        }
        (unsigned, "")
    } else {
        let Some((whole, fraction)) = unsigned.split_once('.') else {
            return Err(decimal_error(scale));
        };
        if fraction.contains('.') || fraction.len() != usize::from(scale) {
            return Err(decimal_error(scale));
        }
        (whole, fraction)
    };
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || (whole.len() > 1 && whole.starts_with('0'))
    {
        return Err(decimal_error(scale));
    }
    let digits = format!("{}{whole}{fraction}", if negative { "-" } else { "" });
    let mantissa = digits
        .parse::<i128>()
        .map_err(|_| KarmaBoundaryError::invalid_input("decimal overflows i128"))?;
    if negative && mantissa == 0 {
        return Err(KarmaBoundaryError::invalid_input(
            "negative zero is not canonical",
        ));
    }
    Ok(mantissa)
}

pub(super) fn canonical_decimal_string(scale: u8, mantissa: i128) -> String {
    let magnitude = mantissa.unsigned_abs();
    if scale == 0 {
        return if mantissa < 0 {
            format!("-{magnitude}")
        } else {
            magnitude.to_string()
        };
    }
    let divisor = 10_u128.pow(u32::from(scale));
    let whole = magnitude / divisor;
    let fraction = magnitude % divisor;
    let sign = if mantissa < 0 { "-" } else { "" };
    format!(
        "{sign}{whole}.{fraction:0width$}",
        width = usize::from(scale)
    )
}

fn decimal_error(scale: u8) -> KarmaBoundaryError {
    KarmaBoundaryError::invalid_input(format!(
        "decimal must be canonical with exactly {scale} fractional digits"
    ))
}

impl<const SCALE: u8> Serialize for FixedDecimal<SCALE> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.canonical_string())
    }
}

impl<'de, const SCALE: u8> Deserialize<'de> for FixedDecimal<SCALE> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

macro_rules! unit_interval_type {
    ($name:ident, $noun:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            pub const ZERO: Self = Self(0);
            pub const ONE: Self = Self(PARTS_PER_BILLION);

            pub fn from_parts_per_billion(value: u32) -> Result<Self, KarmaBoundaryError> {
                if value <= PARTS_PER_BILLION {
                    Ok(Self(value))
                } else {
                    Err(KarmaBoundaryError::invalid_input(concat!(
                        $noun,
                        " must be between zero and one"
                    )))
                }
            }

            pub fn parts_per_billion(self) -> u32 {
                self.0
            }

            fn canonical_string(self) -> String {
                let whole = self.0 / PARTS_PER_BILLION;
                let fraction = self.0 % PARTS_PER_BILLION;
                format!("{whole}.{fraction:09}")
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.canonical_string())
            }
        }

        impl FromStr for $name {
            type Err = KarmaBoundaryError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let Some((whole, fraction)) = value.split_once('.') else {
                    return Err(KarmaBoundaryError::invalid_input(concat!(
                        $noun,
                        " must have exactly nine fractional digits"
                    )));
                };
                if fraction.len() != 9
                    || !fraction.bytes().all(|byte| byte.is_ascii_digit())
                    || !matches!(whole, "0" | "1")
                    || (whole == "1" && fraction != "000000000")
                {
                    return Err(KarmaBoundaryError::invalid_input(concat!(
                        $noun,
                        " must be a canonical value between 0.000000000 and 1.000000000"
                    )));
                }
                let fractional = fraction.parse::<u32>().map_err(|_| {
                    KarmaBoundaryError::invalid_input(concat!($noun, " contains invalid digits"))
                })?;
                let parts = if whole == "1" {
                    PARTS_PER_BILLION
                } else {
                    fractional
                };
                Self::from_parts_per_billion(parts)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.canonical_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(de::Error::custom)
            }
        }
    };
}

unit_interval_type!(Probability, "probability");
unit_interval_type!(Confidence, "confidence");

// There is deliberately no currency type here.
//
// A currency is a unit like any other. `Quantity { amount, unit }` already says
// "this many of that thing", and a unit is an ordinary Record, so `10.00 @brl`
// and `2.5 @kg` are the same shape and travel the same code path. A separate
// money type bought nothing but a second spelling of the same idea, plus a
// three-uppercase-letters rule that only ISO 4217 cares about.
//
// Converting between two units is a rule — multiply by a rate — not a kernel
// feature. That is what makes "my ledger's own token is worth five of theirs"
// expressible as someone's data instead of requiring a change here.
