use serde::{Deserialize, Serialize};

use crate::karma::{CanonicalHash, TypedUid, canonical_hash};
use crate::simulation::{Cause, Quantity};

pub const BUILD: &str = env!("LINCE_EXECUTION_BUILD_HASH");
pub const MAX_SPANS: usize = 50_000;
pub const MAX_STEPS: u64 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub from_ms: i64,
    pub until_ms: i64,
    pub timezone: String,
}

impl Window {
    pub fn month(year: i32, month: u32, timezone: String) -> Result<Self, String> {
        use chrono::{Datelike, TimeZone};
        let zone: chrono_tz::Tz = timezone.parse().map_err(|_| "unknown Calendar timezone")?;
        let first =
            chrono::NaiveDate::from_ymd_opt(year, month, 1).ok_or("invalid Calendar month")?;
        let next = first
            .checked_add_months(chrono::Months::new(1))
            .ok_or("Calendar month overflow")?;
        let boundary = |date: chrono::NaiveDate| {
            let midnight = date.and_hms_opt(0, 0, 0).unwrap();
            (0..180)
                .find_map(|minute| {
                    zone.from_local_datetime(&(midnight + chrono::TimeDelta::minutes(minute)))
                        .earliest()
                })
                .map(|time| time.timestamp_millis())
                .ok_or("Calendar date is unavailable in this timezone")
        };
        if first.year() < 1 {
            return Err("invalid Calendar year".into());
        }
        Ok(Self {
            from_ms: boundary(first)?,
            until_ms: boundary(next)?,
            timezone,
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.from_ms < 0
            || self.until_ms <= self.from_ms
            || self.until_ms > (i64::MAX >> crate::hlc::COUNTER_BITS)
            || self.until_ms - self.from_ms > 366 * 86_400_000
        {
            return Err("projection window must be within 366 days".into());
        }
        self.timezone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| "unknown Calendar timezone".to_string())?;
        Ok(())
    }

    pub fn date(&self, at_ms: i64) -> Option<String> {
        Some(
            chrono::DateTime::from_timestamp_millis(at_ms)?
                .with_timezone(&self.timezone.parse::<chrono_tz::Tz>().ok()?)
                .format("%Y-%m-%d")
                .to_string(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub window: Window,
    pub actor: Option<String>,
}

impl Context {
    pub fn key(&self) -> Result<CanonicalHash, crate::karma::KarmaBoundaryError> {
        canonical_hash(
            "lince.projection.v1",
            &(&self.actor, BUILD, chrono_tz::IANA_TZDB_VERSION),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub id: String,
    pub record: TypedUid,
    pub from_ms: i64,
    pub until_ms: i64,
    pub quantity: Quantity,
    pub cause: Cause,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceLink {
    pub record: TypedUid,
    pub occurrence: crate::simulation::RuleOccurrence,
    pub consequence: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Incomplete {
    Budget {},
    ExternalEffects {},
    RuleFailure {},
    UnavailableRuntime {},
    PastWindow {},
    UnsupportedFilter {},
    UnsupportedUnit {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Status {
    Updating {},
    Ready { base_ms: i64, expires_ms: i64 },
    Incomplete { base_ms: i64, reason: Incomplete },
}
