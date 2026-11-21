//! Fixed-scale decimals and bounded exact numbers.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::failure::KarmaBoundaryError;

mod unit_interval;

pub use unit_interval::{Confidence, Probability};

pub const MAX_DECIMAL_SCALE: u8 = 18;

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
        format_decimal(f, SCALE, self.mantissa)
    }
}

impl<const SCALE: u8> FromStr for FixedDecimal<SCALE> {
    type Err = KarmaBoundaryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_scale::<SCALE>()?;
        let mantissa = parse_canonical_decimal(SCALE, value)?;
        Ok(Self { mantissa })
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
    let (whole, fraction) = if scale == 0 {
        (unsigned, "")
    } else {
        let Some((whole, fraction)) = unsigned.split_once('.') else {
            return Err(decimal_error(scale));
        };
        if fraction.len() != usize::from(scale) {
            return Err(decimal_error(scale));
        }
        (whole, fraction)
    };
    let mut digits = whole.bytes().chain(fraction.bytes());
    if whole.is_empty()
        || (whole.len() > 1 && whole.starts_with('0'))
        || !digits.clone().all(|byte| byte.is_ascii_digit())
    {
        return Err(decimal_error(scale));
    }
    // Accumulate negative numbers with their sign so i128::MIN is representable.
    let mantissa = digits
        .try_fold(0_i128, |mantissa, byte| {
            let digit = i128::from(byte - b'0');
            mantissa
                .checked_mul(10)?
                .checked_add(if negative { -digit } else { digit })
        })
        .ok_or_else(|| KarmaBoundaryError::invalid_input("decimal overflows i128"))?;
    if negative && mantissa == 0 {
        return Err(KarmaBoundaryError::invalid_input(
            "negative zero is not canonical",
        ));
    }
    Ok(mantissa)
}

pub(super) fn format_decimal(
    formatter: &mut fmt::Formatter<'_>,
    scale: u8,
    mantissa: i128,
) -> fmt::Result {
    if scale == 0 {
        return write!(formatter, "{mantissa}");
    }
    let magnitude = mantissa.unsigned_abs();
    let divisor = 10_u128.pow(u32::from(scale));
    let whole = magnitude / divisor;
    let fraction = magnitude % divisor;
    let sign = if mantissa < 0 { "-" } else { "" };
    write!(
        formatter,
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
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de, const SCALE: u8> Deserialize<'de> for FixedDecimal<SCALE> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::FixedDecimal;

    #[test]
    fn signed_mantissa_limits_parse_with_a_fraction() {
        type Decimal = FixedDecimal<2>;

        for (text, mantissa) in [
            ("-1701411834604692317316873037158841057.28", i128::MIN),
            ("1701411834604692317316873037158841057.27", i128::MAX),
        ] {
            let value = text.parse::<Decimal>().expect("representable signed limit");
            assert_eq!(value.mantissa(), mantissa);
            assert_eq!(value.to_string(), text);
        }
        for text in [
            "-1701411834604692317316873037158841057.29",
            "1701411834604692317316873037158841057.28",
        ] {
            let error = text.parse::<Decimal>().expect_err("out-of-range mantissa");
            assert_eq!(error.message, "decimal overflows i128");
        }
    }
}
