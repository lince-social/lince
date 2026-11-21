// non-lean-yet
// Scope: struct ExecutionIdentity, const MAX_SPANS, const MAX_CACHE_BYTES, const MAX_STEPS.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::trivially_copy_pass_by_ref] Consider a value parameter for small Copy inputs.
//   Sites 90.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 63.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 211, 223.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   37, 106, 148.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 10, 32, 104, 131, 135, 165.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 79.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 7, 10,
//   14, 19, 20, 21, 25, 26, 27, 28, 32, 72, 73, 74, 75, 79, 82, 86, 94, 98, 104, 131, 135, 151,
//   159, 160, 161, 165, 178, 179, 180, 181, 182, 183, 184, 189, 190, 191, 192, 193, 194, 195, 196,
//   197, 198, 203, 204, 205, 206, 211, 212, 213, 214, 215, 216, 217, 218, 223, 224, 225, 226.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites dates: String
//   (135), month: String (104), resolve: String (32), validate: String (131).

use serde::{Deserialize, Serialize};

use crate::{
    karma::{CanonicalHash, TypedUid, canonical_hash},
    simulation::{Cause, Quantity},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIdentity(CanonicalHash);

impl ExecutionIdentity {
    pub fn parse(value: impl Into<String>) -> Result<Self, crate::karma::KarmaBoundaryError> {
        CanonicalHash::parse(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

pub const MAX_SPANS: usize = 50_000;
pub const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_STEPS: u64 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RollingWindow {
    pub past_days: u16,
    pub future_days: u16,
    pub timezone: String,
}

impl RollingWindow {
    pub fn resolve(&self, now_ms: i64) -> Result<Window, String> {
        if self.future_days == 0 || u32::from(self.past_days) + u32::from(self.future_days) > 366 {
            return Err("Choose a future window within 366 days including history".into());
        }
        use chrono::TimeZone;
        let zone: chrono_tz::Tz = self.timezone.parse().map_err(|_| "Unknown timezone")?;
        let today = chrono::DateTime::from_timestamp_millis(now_ms)
            .ok_or("Invalid current time")?
            .with_timezone(&zone)
            .date_naive();
        let boundary = |date: chrono::NaiveDate| {
            let midnight = date.and_hms_opt(0, 0, 0).ok_or("Invalid window date")?;
            (0..180)
                .find_map(|minute| {
                    zone.from_local_datetime(&(midnight + chrono::TimeDelta::minutes(minute)))
                        .earliest()
                })
                .map(|time| time.timestamp_millis())
                .ok_or("Window date is unavailable in this timezone")
        };
        let window = Window {
            from_ms: boundary(
                today
                    .checked_sub_days(chrono::Days::new(u64::from(self.past_days)))
                    .ok_or("Window overflow")?,
            )?,
            until_ms: boundary(
                today
                    .checked_add_days(chrono::Days::new(u64::from(self.future_days)))
                    .ok_or("Window overflow")?,
            )?,
            timezone: self.timezone.clone(),
        };
        window.validate()?;
        Ok(window)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub from_ms: i64,
    pub until_ms: i64,
    pub timezone: String,
}

#[derive(Clone, Copy)]
pub struct Dates(chrono_tz::Tz);

impl Dates {
    pub fn new(timezone: chrono_tz::Tz) -> Self {
        Self(timezone)
    }

    pub fn at(&self, at_ms: i64) -> Option<String> {
        Some(self.format(chrono::DateTime::from_timestamp_millis(at_ms)?))
    }

    fn format(&self, time: chrono::DateTime<chrono::Utc>) -> String {
        time.with_timezone(&self.0).format("%Y-%m-%d").to_string()
    }

    pub fn value(&self, value: &crate::schedule::TimeValue) -> chrono::NaiveDate {
        value.date(self.0)
    }

    pub fn parse(&self, value: &str) -> Option<chrono::NaiveDate> {
        Some(self.value(&crate::schedule::TimeValue::parse(value).ok()?))
    }
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
            let midnight = date.and_time(chrono::NaiveTime::MIN);
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
        self.dates().map(|_| ())
    }

    pub fn dates(&self) -> Result<Dates, String> {
        let _prepare = crate::performance::Span::new("projection-window-date-preparation");
        crate::performance::add("projection-window-date-preparations", 1);
        if self.from_ms < 0
            || self.until_ms <= self.from_ms
            || self.until_ms > (i64::MAX >> crate::hlc::COUNTER_BITS)
            || self.until_ms - self.from_ms > 366 * 86_400_000
        {
            return Err("projection window must be within 366 days".into());
        }
        self.timezone
            .parse::<chrono_tz::Tz>()
            .map(Dates)
            .map_err(|_| "unknown Calendar timezone".to_string())
    }

    pub fn date(&self, at_ms: i64) -> Option<String> {
        let time = chrono::DateTime::from_timestamp_millis(at_ms)?;
        Some(Dates(self.timezone.parse().ok()?).format(time))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub window: Window,
    pub actor: Option<String>,
}

impl Context {
    pub fn key(
        &self,
        identity: &ExecutionIdentity,
    ) -> Result<CanonicalHash, crate::karma::KarmaBoundaryError> {
        canonical_hash(
            "lince.projection.v1",
            &(&self.actor, identity.as_str(), chrono_tz::IANA_TZDB_VERSION),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scheduled {
    pub id: String,
    pub record: TypedUid,
    pub time: crate::schedule::TimeRange,
    pub quantity: Quantity,
    pub cause: Cause,
    pub head: String,
    pub slug: Option<String>,
    pub record_kind: String,
    pub preview: bool,
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
