use std::fmt;

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::failure::KarmaBoundaryError;

const MIN_RFC3339_MILLIS: i64 = -62_167_219_200_000;
const MAX_RFC3339_MILLIS: i64 = 253_402_300_799_999;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimestampMs(i64);

impl TimestampMs {
    pub fn from_millis(milliseconds: i64) -> Result<Self, KarmaBoundaryError> {
        if (MIN_RFC3339_MILLIS..=MAX_RFC3339_MILLIS).contains(&milliseconds) {
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
            .and_then(|value| Self::from_millis(value).ok())
    }

    pub fn parse_canonical(value: &str) -> Result<Self, KarmaBoundaryError> {
        let parsed = DateTime::parse_from_rfc3339(value)
            .map_err(|_| KarmaBoundaryError::invalid_input("timestamp must be valid RFC3339"))?;
        let timestamp = Self::from_millis(parsed.timestamp_millis())?;
        if timestamp.canonical_string() != value {
            return Err(KarmaBoundaryError::invalid_input(
                "timestamp must use UTC Z and exactly three fractional digits",
            ));
        }
        Ok(timestamp)
    }

    fn canonical_string(self) -> String {
        Utc.timestamp_millis_opt(self.0)
            .single()
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
