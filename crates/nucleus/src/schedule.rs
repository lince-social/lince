use chrono::{DateTime, Datelike, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, TimeZone};
pub use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const MAX_TIME_BYTES: usize = 64;
pub const MAX_INSTANT_MS: i64 = i64::MAX >> crate::hlc::COUNTER_BITS;
pub const MAX_DURATION_MS: i64 = 366 * 86_400_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimeValue {
    Date(NaiveDate),
    Instant(DateTime<FixedOffset>),
}

impl TimeValue {
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

    pub fn instant_ms(&self) -> Option<i64> {
        match self {
            Self::Date(_) => None,
            Self::Instant(time) => Some(time.timestamp_millis()),
        }
    }

    pub fn date(&self, timezone: Tz) -> NaiveDate {
        match self {
            Self::Date(date) => *date,
            Self::Instant(time) => time.with_timezone(&timezone).date_naive(),
        }
    }

    pub fn stored_date(&self) -> NaiveDate {
        match self {
            Self::Date(date) => *date,
            Self::Instant(time) => time.date_naive(),
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
    pub fn overlaps(&self, from_ms: i64, until_ms: i64) -> bool {
        self.from_ms < until_ms
            && self
                .until_ms
                .map_or(self.from_ms >= from_ms, |end| end > from_ms)
    }

    pub fn clipped(&self, from_ms: i64, until_ms: i64) -> Option<Self> {
        self.overlaps(from_ms, until_ms).then(|| Self {
            from_ms: self.from_ms.max(from_ms),
            until_ms: self.until_ms.map(|end| end.min(until_ms)),
        })
    }
}

pub fn validate_endpoints(start: Option<&str>, due: Option<&str>) -> Result<(), String> {
    let start = start.map(TimeValue::parse).transpose()?;
    let due = due.map(TimeValue::parse).transpose()?;
    if let (Some(start), Some(due)) = (start, due) {
        let reversed = match (start.instant_ms(), due.instant_ms()) {
            (Some(start), Some(due)) => start > due,
            _ => start.stored_date() > due.stored_date(),
        };
        if reversed {
            return Err("Due time must not be before start time".into());
        }
    }
    Ok(())
}

pub fn range(
    start: Option<&str>,
    due: Option<&str>,
    estimate_minutes: Option<f64>,
    occurrence_ms: Option<i64>,
) -> Result<Option<TimeRange>, String> {
    validate_endpoints(start, due)?;
    let start = start
        .map(TimeValue::parse)
        .transpose()?
        .and_then(|time| time.instant_ms());
    let due = due
        .map(TimeValue::parse)
        .transpose()?
        .and_then(|time| time.instant_ms());
    let estimate = estimate_minutes.unwrap_or(0.0);
    if !estimate.is_finite() || !(0.0..=1_000_000_000.0).contains(&estimate) {
        return Err("Estimate must be a finite, nonnegative number of minutes".into());
    }
    let estimated_ms = (estimate * 60_000.0).round() as i64;
    let duration = match (start, due) {
        (Some(start), Some(due)) => due - start,
        _ => estimated_ms,
    };
    let (from_ms, until_ms) = if let Some(occurrence) = occurrence_ms {
        (occurrence, occurrence.checked_add(duration))
    } else if let Some(start) = start {
        (
            start,
            Some(due.unwrap_or(start.checked_add(duration).ok_or("Time range overflow")?)),
        )
    } else if let Some(due) = due {
        (
            due.checked_sub(duration).ok_or("Time range overflow")?,
            Some(due),
        )
    } else {
        return Ok(None);
    };
    let until_ms = until_ms.ok_or("Time range overflow")?;
    if from_ms < 0 || until_ms > MAX_INSTANT_MS {
        return Err("Time range is outside the supported range".into());
    }
    Ok(Some(TimeRange {
        from_ms,
        until_ms: (until_ms > from_ms).then_some(until_ms),
    }))
}

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

pub fn timezone_date(value: &str, timezone: &str) -> Option<NaiveDate> {
    Some(TimeValue::parse(value).ok()?.date(timezone.parse().ok()?))
}

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
