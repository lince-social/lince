//! Probabilities and confidence in exact parts per billion.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::failure::KarmaBoundaryError;

const PARTS_PER_BILLION: u32 = 1_000_000_000;

fn parse_parts(value: &str, noun: &str) -> Result<u32, KarmaBoundaryError> {
    let Some((whole, fraction)) = value.split_once('.') else {
        return Err(KarmaBoundaryError::invalid_input(format!(
            "{noun} must have exactly nine fractional digits"
        )));
    };
    if fraction.len() != 9
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || !matches!(whole, "0" | "1")
        || (whole == "1" && fraction != "000000000")
    {
        return Err(KarmaBoundaryError::invalid_input(format!(
            "{noun} must be a canonical value between 0.000000000 and 1.000000000"
        )));
    }
    if whole == "1" {
        return Ok(PARTS_PER_BILLION);
    }
    fraction
        .parse()
        .map_err(|_| KarmaBoundaryError::invalid_input(format!("{noun} contains invalid digits")))
}

fn format_parts(formatter: &mut fmt::Formatter<'_>, parts: u32) -> fmt::Result {
    let whole = parts / PARTS_PER_BILLION;
    let fraction = parts % PARTS_PER_BILLION;
    write!(formatter, "{whole}.{fraction:09}")
}

macro_rules! unit_interval_type {
    ($name:ident, $noun:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema,
        )]
        #[schemars(with = "String")]
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
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                format_parts(f, self.0)
            }
        }

        impl FromStr for $name {
            type Err = KarmaBoundaryError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                parse_parts(value, $noun).map(Self)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(de::Error::custom)
            }
        }
    };
}

unit_interval_type!(Probability, "probability");
unit_interval_type!(Confidence, "confidence");
