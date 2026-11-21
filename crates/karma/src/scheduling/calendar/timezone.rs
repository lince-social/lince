//! Timezone identity, pinned revisions, and provider resolution.

use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize, de};

use super::{CalendarSchedule, CivilDateTime};
use crate::{CanonicalHash, KarmaBoundaryError, TimestampMs};

const MAX_TIME_ZONE_ID_BYTES: usize = 255;
const MAX_TZDB_VERSION_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema, Serialize)]
#[serde(transparent)]
#[schemars(!transparent)]
pub struct TimeZoneId(String);

impl TimeZoneId {
    pub fn new(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let value = value.into();
        if valid_time_zone_id(&value) {
            Ok(Self(value))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "timezone must be a canonical IANA-style identifier",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_time_zone_id(value: &str) -> bool {
    value.len() <= MAX_TIME_ZONE_ID_BYTES
        && value.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
        })
}

impl<'de> Deserialize<'de> for TimeZoneId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema, Serialize)]
#[serde(transparent)]
#[schemars(!transparent)]
pub struct TzdbVersion(String);

impl TzdbVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let value = value.into();
        if !value.is_empty()
            && value.len() <= MAX_TZDB_VERSION_BYTES
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+')
            })
        {
            Ok(Self(value))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "tzdb version must be non-empty printable version ASCII",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for TzdbVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct TzdbRevision {
    pub version: TzdbVersion,
    pub digest: CanonicalHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum GapPolicy {
    Skip,
    ShiftForward,
    Pause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum FoldPolicy {
    First,
    Second,
    Both,
    Pause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LocalTimeResolution {
    Unique {
        instant: TimestampMs,
    },
    Gap {
        before: TimestampMs,
        first_valid_after: TimestampMs,
    },
    Fold {
        first: TimestampMs,
        second: TimestampMs,
    },
}

pub trait TimeZoneProvider: Send + Sync {
    fn revision(&self) -> &TzdbRevision;

    fn resolve_local(
        &self,
        timezone: &TimeZoneId,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError>;

    fn minimum_interval_ms(
        &self,
        _schedule: &CalendarSchedule,
    ) -> Result<NonZeroU64, KarmaBoundaryError> {
        Err(KarmaBoundaryError::invalid_definition(
            "calendar provider does not attest a minimum schedule interval",
        ))
    }
}

impl LocalTimeResolution {
    pub(super) fn validate(self) -> Result<Self, KarmaBoundaryError> {
        let message = match self {
            Self::Gap {
                before,
                first_valid_after,
            } if before >= first_valid_after => "timezone provider returned a non-increasing gap",
            Self::Fold { first, second } if first >= second => {
                "timezone provider returned a non-increasing fold"
            }
            Self::Unique { .. } | Self::Gap { .. } | Self::Fold { .. } => return Ok(self),
        };
        Err(KarmaBoundaryError::invalid_definition(message))
    }
}
