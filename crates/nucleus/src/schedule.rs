//! Scheduling dates, civil-time conversion, and windows.
//!
//! [TimeValue] distinguishes date-only input from an offset-bearing instant.
//! [Endpoints] checks their ordering before calculating a [TimeRange], whose
//! missing end represents a point for overlap and clipping.

pub mod allocation;
mod endpoints;

pub use endpoints::{Endpoints, range, validate_endpoints};

use chrono::{DateTime, Datelike, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, TimeZone};
pub use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// Largest accepted date or timestamp input in bytes.
pub const MAX_TIME_BYTES: usize = 64;
/// Latest millisecond instant that fits in the hybrid clock.
pub const MAX_INSTANT_MS: i64 = i64::MAX >> crate::hlc::COUNTER_BITS;
/// Maximum duration of a supported schedule query window.
pub const MAX_DURATION_MS: i64 = 366 * 86_400_000;

/// A calendar date without a time, or an instant with its stored offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimeValue {
    /// A date whose value does not depend on an offset.
    Date(NaiveDate),
    /// An instant together with the offset in its input.
    Instant(DateTime<FixedOffset>),
}

impl TimeValue {
    /// Reads a canonical date or an RFC 3339 timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input or an instant outside the supported range.
    pub fn parse(value: &str) -> Result<Self, String> {
        if value.len() > MAX_TIME_BYTES {
            return Err("Scheduling time exceeds 64 bytes".into());
        }
        if value.len() == 10 {
            let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| "Use YYYY-MM-DD or a timestamp with a timezone")?;
            if !(1..=9999).contains(&date.year()) || date.format("%Y-%m-%d").to_string() != value {
                return Err("Use an exact YYYY-MM-DD date".into());
            }
            return Ok(Self::Date(date));
        }
        let time = DateTime::parse_from_rfc3339(value)
            .map_err(|_| "Use YYYY-MM-DD or an RFC3339 timestamp with a timezone")?;
        if !(0..=MAX_INSTANT_MS).contains(&time.timestamp_millis()) {
            return Err("Scheduling timestamp is outside the supported range".into());
        }
        Ok(Self::Instant(time))
    }

    /// Returns Unix milliseconds for an instant. Calendar dates have no instant.
    pub fn instant_ms(&self) -> Option<i64> {
        match self {
            Self::Date(_) => None,
            Self::Instant(time) => Some(time.timestamp_millis()),
        }
    }

    /// Interprets instants in the display timezone and keeps date-only values as-is.
    ///
    /// # Panics
    ///
    /// Panics if a manually constructed instant maps outside Chrono's local date range.
    pub fn date(&self, timezone: Tz) -> NaiveDate {
        match self {
            Self::Date(date) => *date,
            Self::Instant(time) => time.with_timezone(&timezone).date_naive(),
        }
    }

    /// Uses the instant's stored offset to find its date.
    ///
    /// # Panics
    ///
    /// Panics if a manually constructed instant has an unrepresentable local date.
    pub fn stored_date(&self) -> NaiveDate {
        match self {
            Self::Date(date) => *date,
            Self::Instant(time) => time.date_naive(),
        }
    }
    fn checked_stored_date(&self) -> Option<NaiveDate> {
        match self {
            Self::Date(date) => Some(*date),
            Self::Instant(time) => time
                .naive_utc()
                .checked_add_offset(*time.offset())
                .map(|local| local.date()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeRange {
    pub from_ms: i64,
    pub until_ms: Option<i64>,
}

impl TimeRange {
    /// Builds a range at a cursor offset. If its end overflows, it stays a point.
    pub fn at_cursor(now: i64, offset: i64, duration: i64) -> Option<Self> {
        let start = now.checked_add(offset)?;
        Some(Self {
            from_ms: start,
            until_ms: (duration > 0)
                .then(|| start.checked_add(duration))
                .flatten(),
        })
    }

    /// Tests whether the interval or point meets a half-open query window.
    pub fn overlaps(&self, from_ms: i64, until_ms: i64) -> bool {
        self.from_ms < until_ms
            && self
                .until_ms
                .map_or(self.from_ms >= from_ms, |end| end > from_ms)
    }

    /// Intersects this range with a query window while preserving points.
    pub fn clipped(&self, from_ms: i64, until_ms: i64) -> Option<Self> {
        self.overlaps(from_ms, until_ms).then(|| Self {
            from_ms: self.from_ms.max(from_ms),
            until_ms: self.until_ms.map(|end| end.min(until_ms)),
        })
    }
}

/// Lists the instants for a local time, including both occurrences after a clock fold.
///
/// # Errors
///
/// Returns an error for invalid input, a skipped time, or an unsupported instant.
pub fn civil_instants(date: &str, time: &str, timezone: &str) -> Result<Vec<String>, String> {
    let zone: Tz = timezone.parse().map_err(|_| "Unknown IANA timezone")?;
    let civil = format!("{date}T{time}");
    let naive = NaiveDateTime::parse_from_str(&civil, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(&civil, "%Y-%m-%dT%H:%M"))
        .map_err(|_| "Enter a valid date and HH:MM or HH:MM:SS time")?;
    let values = match zone.from_local_datetime(&naive) {
        LocalResult::Single(time) => vec![time.to_rfc3339()],
        LocalResult::Ambiguous(first, second) => vec![first.to_rfc3339(), second.to_rfc3339()],
        LocalResult::None => {
            return Err("This local time does not exist in the selected timezone".into());
        }
    };
    for value in &values {
        TimeValue::parse(value)?;
    }
    Ok(values)
}

/// Parses a schedule value and returns its date in the supplied timezone.
pub fn timezone_date(value: &str, timezone: &str) -> Option<NaiveDate> {
    Some(TimeValue::parse(value).ok()?.date(timezone.parse().ok()?))
}

/// Moves a value to a different date while retaining its local time.
///
/// # Errors
///
/// Returns an error for invalid input or a clock change that makes the result ambiguous or unavailable.
pub fn on_date(value: &str, date: &str, timezone: &str) -> Result<String, String> {
    let parsed = TimeValue::parse(value)?;
    let TimeValue::Date(date) = TimeValue::parse(date)? else {
        return Err("Choose a calendar date".into());
    };
    match parsed {
        TimeValue::Date(_) => Ok(date.to_string()),
        TimeValue::Instant(time) => {
            let zone: Tz = timezone.parse().map_err(|_| "Unknown IANA timezone")?;
            let civil = date.and_time(time.with_timezone(&zone).time());
            match zone.from_local_datetime(&civil) {
                LocalResult::Single(time) => Ok(time.to_rfc3339()),
                LocalResult::Ambiguous(_, _) => {
                    Err("This time occurs twice; choose an offset in Date / time".into())
                }
                LocalResult::None => {
                    Err("This local time does not exist; choose another time in Date / time".into())
                }
            }
        }
    }
}
