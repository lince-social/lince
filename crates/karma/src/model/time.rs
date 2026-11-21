//! Signed millisecond spans and UTC instants encoded as canonical RFC 3339 text.

use std::{fmt, ops::RangeInclusive};

use chrono::{DateTime, SecondsFormat};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::failure::KarmaBoundaryError;

const RFC3339_MILLIS: RangeInclusive<i64> = -62_167_219_200_000..=253_402_300_799_999;
const CANONICAL_TIMESTAMP_LEN: usize = "0000-01-01T00:00:00.000Z".len();

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Default,
    schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct DurationMs(i64);

impl DurationMs {
    pub const ZERO: Self = Self(0);

    pub const fn new(milliseconds: i64) -> Self {
        Self(milliseconds)
    }

    pub const fn get(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema)]
#[schemars(with = "String")]
pub struct TimestampMs(i64);

impl TimestampMs {
    /// Constructs an instant from Unix milliseconds.
    ///
    /// # Errors
    /// Returns an error when the instant falls outside UTC years 0000 through 9999.
    pub fn from_millis(milliseconds: i64) -> Result<Self, KarmaBoundaryError> {
        if RFC3339_MILLIS.contains(&milliseconds) {
            Ok(Self(milliseconds))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "timestamp is outside canonical RFC3339 year range 0000..=9999",
            ))
        }
    }

    pub const fn as_millis(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, duration: DurationMs) -> Option<Self> {
        self.0
            .checked_add(duration.get())
            .filter(|value| RFC3339_MILLIS.contains(value))
            .map(Self)
    }

    /// Parses a UTC timestamp with a four-digit year and three fractional digits.
    ///
    /// # Errors
    /// Rejects invalid dates, leap seconds, and noncanonical RFC 3339 spellings.
    pub fn parse_canonical(value: &str) -> Result<Self, KarmaBoundaryError> {
        let parsed = DateTime::parse_from_rfc3339(value)
            .map_err(|_| KarmaBoundaryError::invalid_input("timestamp must be valid RFC3339"))?;
        let timestamp = Self::from_millis(parsed.timestamp_millis())?;
        // RFC 3339 parsing already validates the digits and punctuation. An uppercase
        // T and Z at this length require exactly three fractional digits.
        if value.len() != CANONICAL_TIMESTAMP_LEN
            || value.as_bytes().get(10) != Some(&b'T')
            || !value.ends_with('Z')
            || parsed.timestamp_subsec_millis() >= 1_000
        {
            return Err(KarmaBoundaryError::invalid_input(
                "timestamp must use UTC Z and exactly three fractional digits",
            ));
        }
        Ok(timestamp)
    }

    fn canonical_string(self) -> String {
        DateTime::from_timestamp_millis(self.0)
            .expect("validated TimestampMs must be representable by chrono")
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    }
}

impl fmt::Display for TimestampMs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical_string())
    }
}

impl Serialize for TimestampMs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.canonical_string())
    }
}

impl<'de> Deserialize<'de> for TimestampMs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_canonical(&value).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_text_rejects_alternate_spellings_and_leap_seconds() {
        for value in [
            "0000-01-01T00:00:00.000Z",
            "1969-12-31T23:59:59.999Z",
            "9999-12-31T23:59:59.999Z",
        ] {
            assert_eq!(
                TimestampMs::parse_canonical(value).unwrap().to_string(),
                value
            );
        }
        for value in [
            "2016-12-31T23:59:60.000Z",
            "2016-12-31t23:59:59.000Z",
            "2016-12-31 23:59:59.000Z",
            "2016-12-31T23:59:59.000z",
        ] {
            assert_eq!(
                TimestampMs::parse_canonical(value).unwrap_err().message,
                "timestamp must use UTC Z and exactly three fractional digits",
                "{value}"
            );
        }
    }
}
