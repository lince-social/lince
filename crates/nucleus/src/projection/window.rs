use chrono::{
    DateTime, Datelike, Days, Months, NaiveDate, NaiveTime, Offset, TimeDelta, TimeZone, Utc,
};
use chrono_tz::Tz;

use super::{Dates, RollingWindow, Window};
use crate::{
    performance::{self, Span},
    schedule::{MAX_DURATION_MS, MAX_INSTANT_MS, TimeValue},
};

const MAX_WINDOW_DAYS: u32 = 366;
const MIDNIGHT_SEARCH_MINUTES: i64 = 180;

impl RollingWindow {
    /// Resolves local-day offsets around the supplied Unix timestamp in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid current time, unknown timezone, unavailable local
    /// date, or bounds outside the supported range.
    pub fn resolve(&self, now_ms: i64) -> Result<Window, String> {
        if self.future_days == 0
            || u32::from(self.past_days) + u32::from(self.future_days) > MAX_WINDOW_DAYS
        {
            return Err("Choose a future window within 366 days including history".into());
        }
        let zone: Tz = self.timezone.parse().map_err(|_| "Unknown timezone")?;
        let local = DateTime::from_timestamp_millis(now_ms)
            .ok_or("Invalid current time")?
            .with_timezone(&zone);
        let today = local
            .naive_utc()
            .checked_add_offset(local.offset().fix())
            .ok_or("Invalid current time")?
            .date();
        let boundary =
            |date| start_of_date(zone, date).ok_or("Window date is unavailable in this timezone");
        let window = Window {
            from_ms: boundary(
                today
                    .checked_sub_days(Days::new(self.past_days.into()))
                    .ok_or("Window overflow")?,
            )?,
            until_ms: boundary(
                today
                    .checked_add_days(Days::new(self.future_days.into()))
                    .ok_or("Window overflow")?,
            )?,
            timezone: self.timezone.clone(),
        };
        window.dates_with_timezone(Some(zone))?;
        Ok(window)
    }
}

impl Dates {
    /// Uses the given timezone for instant-to-date conversions.
    pub fn new(timezone: Tz) -> Self {
        Self(timezone)
    }

    /// Formats the local date for a representable Unix timestamp in milliseconds.
    pub fn at(&self, at_ms: i64) -> Option<String> {
        DateTime::from_timestamp_millis(at_ms).map(|time| self.format(time))
    }

    fn format(self, time: DateTime<Utc>) -> String {
        time.with_timezone(&self.0).format("%Y-%m-%d").to_string()
    }

    /// Keeps date-only values and converts instants to dates in this timezone.
    ///
    /// # Panics
    ///
    /// Panics if a manually constructed instant maps outside Chrono's local date range.
    pub fn value(&self, value: &TimeValue) -> NaiveDate {
        value.date(self.0)
    }

    /// Parses a schedule date or instant and converts it to a local date.
    pub fn parse(&self, value: &str) -> Option<NaiveDate> {
        TimeValue::parse(value).ok().map(|value| self.value(&value))
    }
}

impl Window {
    /// Builds a local calendar month, including any offset changes at its boundaries.
    ///
    /// # Errors
    ///
    /// Returns an error for a nonpositive year, invalid month, unknown timezone, or a
    /// month whose boundaries cannot be represented.
    pub fn month(year: i32, month: u32, timezone: String) -> Result<Self, String> {
        let zone = timezone.parse().map_err(|_| "unknown Calendar timezone")?;
        let first = NaiveDate::from_ymd_opt(year, month, 1).ok_or("invalid Calendar month")?;
        let next = first
            .checked_add_months(Months::new(1))
            .ok_or("Calendar month overflow")?;
        if first.year() < 1 {
            return Err("invalid Calendar year".into());
        }
        let boundary =
            |date| start_of_date(zone, date).ok_or("Calendar date is unavailable in this timezone");
        Ok(Self {
            from_ms: boundary(first)?,
            until_ms: boundary(next)?,
            timezone,
        })
    }

    /// Validates the range and timezone of a supplied projection window.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported timestamps, an empty or oversized range, or an
    /// unknown timezone.
    pub fn validate(&self) -> Result<(), String> {
        self.dates().map(|_| ())
    }

    /// Validates the window and prepares its timezone for repeated date conversion.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::validate`].
    pub fn dates(&self) -> Result<Dates, String> {
        self.dates_with_timezone(None)
    }

    fn dates_with_timezone(&self, timezone: Option<Tz>) -> Result<Dates, String> {
        let _prepare = Span::new("projection-window-date-preparation");
        performance::add("projection-window-date-preparations", 1);
        if self.from_ms < 0
            || self.until_ms <= self.from_ms
            || self.until_ms > MAX_INSTANT_MS
            || self.until_ms - self.from_ms > MAX_DURATION_MS
        {
            return Err("projection window must be within 366 days".into());
        }
        timezone
            .or_else(|| self.timezone.parse().ok())
            .map(Dates)
            .ok_or_else(|| "unknown Calendar timezone".into())
    }

    /// Formats the local date of a Unix timestamp using this window's timezone.
    pub fn date(&self, at_ms: i64) -> Option<String> {
        let time = DateTime::from_timestamp_millis(at_ms)?;
        Some(Dates(self.timezone.parse().ok()?).format(time))
    }
}

fn start_of_date(zone: Tz, date: NaiveDate) -> Option<i64> {
    let midnight = date.and_time(NaiveTime::MIN);
    // Search past a midnight gap and prefer the earlier occurrence in a fold.
    (0..MIDNIGHT_SEARCH_MINUTES).find_map(|minute| {
        zone.from_local_datetime(&(midnight + TimeDelta::minutes(minute)))
            .earliest()
            .map(|time| time.timestamp_millis())
    })
}
