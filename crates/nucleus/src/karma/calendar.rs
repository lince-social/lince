use std::{
    collections::BTreeSet,
    fmt,
    num::{NonZeroU32, NonZeroU64},
};

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike, Weekday};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::{
    Cadence, CanonicalHash, InactiveGapPolicy, InvalidDay, KarmaBoundaryError, MissedPolicy,
    NoOccurrence, OverloadPolicy, RephasePolicy, TimerPolicy, TimestampMs,
};

const MAX_CALENDAR_SEARCH_STEPS: usize = 4_096;
const MAX_TIME_ZONE_ID_BYTES: usize = 255;
const MAX_TZDB_VERSION_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDateTime(NaiveDateTime);

impl CivilDateTime {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        millisecond: u32,
    ) -> Result<Self, KarmaBoundaryError> {
        if !(0..=9_999).contains(&year) {
            return Err(KarmaBoundaryError::invalid_input(
                "civil year must be within 0000..=9999",
            ));
        }
        let date = NaiveDate::from_ymd_opt(year, month, day)
            .ok_or_else(|| KarmaBoundaryError::invalid_input("civil date is invalid"))?;
        let value = date
            .and_hms_milli_opt(hour, minute, second, millisecond)
            .ok_or_else(|| KarmaBoundaryError::invalid_input("civil time is invalid"))?;
        Ok(Self(value))
    }

    pub fn parse_canonical(value: &str) -> Result<Self, KarmaBoundaryError> {
        let parsed = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.3f")
            .map_err(|_| KarmaBoundaryError::invalid_input("civil datetime is invalid"))?;
        let civil = Self::new(
            parsed.year(),
            parsed.month(),
            parsed.day(),
            parsed.hour(),
            parsed.minute(),
            parsed.second(),
            parsed.and_utc().timestamp_subsec_millis(),
        )?;
        if civil.canonical_string() != value {
            return Err(KarmaBoundaryError::invalid_input(
                "civil datetime must use YYYY-MM-DDTHH:MM:SS.sss",
            ));
        }
        Ok(civil)
    }

    pub const fn as_naive(self) -> NaiveDateTime {
        self.0
    }

    pub fn from_naive(value: NaiveDateTime) -> Result<Self, KarmaBoundaryError> {
        Self::new(
            value.year(),
            value.month(),
            value.day(),
            value.hour(),
            value.minute(),
            value.second(),
            value.and_utc().timestamp_subsec_millis(),
        )
    }

    fn from_date_and_time(date: NaiveDate, time: CivilTime) -> Result<Self, KarmaBoundaryError> {
        Self::new(
            date.year(),
            date.month(),
            date.day(),
            time.hour(),
            time.minute(),
            time.second(),
            time.millisecond(),
        )
    }

    fn canonical_string(self) -> String {
        self.0.format("%Y-%m-%dT%H:%M:%S%.3f").to_string()
    }
}

impl fmt::Display for CivilDateTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical_string())
    }
}

impl Serialize for CivilDateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.canonical_string())
    }
}

impl<'de> Deserialize<'de> for CivilDateTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_canonical(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilTime(u32);

impl CivilTime {
    pub fn new(
        hour: u32,
        minute: u32,
        second: u32,
        millisecond: u32,
    ) -> Result<Self, KarmaBoundaryError> {
        if hour >= 24 || minute >= 60 || second >= 60 || millisecond >= 1_000 {
            return Err(KarmaBoundaryError::invalid_input("civil time is invalid"));
        }
        Ok(Self(
            (((hour * 60) + minute) * 60 + second) * 1_000 + millisecond,
        ))
    }

    pub fn parse_canonical(value: &str) -> Result<Self, KarmaBoundaryError> {
        let Some((whole, fraction)) = value.rsplit_once('.') else {
            return Err(KarmaBoundaryError::invalid_input(
                "civil time must use HH:MM:SS.sss",
            ));
        };
        if fraction.len() != 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(KarmaBoundaryError::invalid_input(
                "civil time must use exactly three fractional digits",
            ));
        }
        let fields = whole.split(':').collect::<Vec<_>>();
        if fields.len() != 3 || fields.iter().any(|field| field.len() != 2) {
            return Err(KarmaBoundaryError::invalid_input(
                "civil time must use HH:MM:SS.sss",
            ));
        }
        let parse = |field: &str| {
            field
                .parse::<u32>()
                .map_err(|_| KarmaBoundaryError::invalid_input("civil time is invalid"))
        };
        let result = Self::new(
            parse(fields[0])?,
            parse(fields[1])?,
            parse(fields[2])?,
            parse(fraction)?,
        )?;
        if result.canonical_string() != value {
            return Err(KarmaBoundaryError::invalid_input(
                "civil time must use HH:MM:SS.sss",
            ));
        }
        Ok(result)
    }

    pub const fn hour(self) -> u32 {
        self.0 / 3_600_000
    }

    pub const fn minute(self) -> u32 {
        (self.0 / 60_000) % 60
    }

    pub const fn second(self) -> u32 {
        (self.0 / 1_000) % 60
    }

    pub const fn millisecond(self) -> u32 {
        self.0 % 1_000
    }

    fn canonical_string(self) -> String {
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            self.hour(),
            self.minute(),
            self.second(),
            self.millisecond()
        )
    }
}

impl fmt::Display for CivilTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical_string())
    }
}

impl Serialize for CivilTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.canonical_string())
    }
}

impl<'de> Deserialize<'de> for CivilTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_canonical(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    if value.is_empty()
        || value.len() > MAX_TIME_ZONE_ID_BYTES
        || !value.is_ascii()
        || value.starts_with('/')
        || value.ends_with('/')
    {
        return false;
    }
    value.split('/').all(|part| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && !part.starts_with('-')
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
    })
}

impl Serialize for TimeZoneId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TzdbVersion(String);

impl TzdbVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let value = value.into();
        if !value.is_empty()
            && value.len() <= MAX_TZDB_VERSION_BYTES
            && value.is_ascii()
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

impl Serialize for TzdbVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TzdbRevision {
    pub version: TzdbVersion,
    pub digest: CanonicalHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CivilWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl From<Weekday> for CivilWeekday {
    fn from(value: Weekday) -> Self {
        match value {
            Weekday::Mon => Self::Monday,
            Weekday::Tue => Self::Tuesday,
            Weekday::Wed => Self::Wednesday,
            Weekday::Thu => Self::Thursday,
            Weekday::Fri => Self::Friday,
            Weekday::Sat => Self::Saturday,
            Weekday::Sun => Self::Sunday,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WeekdaySet(BTreeSet<CivilWeekday>);

impl WeekdaySet {
    pub fn new(days: impl IntoIterator<Item = CivilWeekday>) -> Result<Self, KarmaBoundaryError> {
        let days = days.into_iter().collect::<BTreeSet<_>>();
        if days.is_empty() {
            Err(KarmaBoundaryError::invalid_input(
                "weekly calendar rule needs at least one weekday",
            ))
        } else {
            Ok(Self(days))
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = CivilWeekday> + '_ {
        self.0.iter().copied()
    }
}

impl Serialize for WeekdaySet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WeekdaySet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let days = BTreeSet::<CivilWeekday>::deserialize(deserializer)?;
        Self::new(days).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DayOfMonth(u8);

impl DayOfMonth {
    pub fn new(day: u8) -> Result<Self, KarmaBoundaryError> {
        if (1..=31).contains(&day) {
            Ok(Self(day))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "calendar day of month must be within 1..=31",
            ))
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Serialize for DayOfMonth {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for DayOfMonth {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u8::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InvalidMonthDayPolicy {
    Skip,
    ClampToLastDay,
    Pause,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CalendarRule {
    DailyAt {
        every_days: NonZeroU32,
        time: CivilTime,
    },
    WeeklyAt {
        every_weeks: NonZeroU32,
        weekdays: WeekdaySet,
        time: CivilTime,
    },
    MonthlyAt {
        every_months: NonZeroU32,
        day: DayOfMonth,
        time: CivilTime,
        invalid_day: InvalidMonthDayPolicy,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GapPolicy {
    Skip,
    ShiftForward,
    Pause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarBoundaryKind {
    Unique,
    GapShiftedForward,
    FoldFirst,
    FoldSecond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarBoundary {
    pub requested_local: CivilDateTime,
    pub intended_at: TimestampMs,
    pub kind: CalendarBoundaryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CalendarDiscontinuity {
    Gap {
        local: CivilDateTime,
        before: TimestampMs,
        first_valid_after: TimestampMs,
    },
    Fold {
        local: CivilDateTime,
        first: TimestampMs,
        second: TimestampMs,
    },
    FoldFirst {
        local: CivilDateTime,
        instant: TimestampMs,
    },
    FoldSecond {
        local: CivilDateTime,
        instant: TimestampMs,
    },
    InvalidDayOfMonth {
        year: i32,
        month: u32,
        day: DayOfMonth,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarAdvance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary: Option<CalendarBoundary>,
    pub skipped: Vec<CalendarDiscontinuity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause: Option<CalendarDiscontinuity>,
}

impl CalendarAdvance {
    fn boundary(boundary: CalendarBoundary, skipped: Vec<CalendarDiscontinuity>) -> Self {
        Self {
            boundary: Some(boundary),
            skipped,
            pause: None,
        }
    }

    fn retired(skipped: Vec<CalendarDiscontinuity>) -> Self {
        Self {
            boundary: None,
            skipped,
            pause: None,
        }
    }

    pub fn is_retired(&self) -> bool {
        self.boundary.is_none() && self.pause.is_none()
    }

    fn paused(reason: CalendarDiscontinuity, skipped: Vec<CalendarDiscontinuity>) -> Self {
        Self {
            boundary: None,
            skipped,
            pause: Some(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarSchedule {
    pub anchor: CivilDateTime,
    pub cadence: Cadence,
    pub timezone: TimeZoneId,
    pub tzdb: TzdbRevision,
    pub gap: GapPolicy,
    pub fold: FoldPolicy,
    pub timer: TimerPolicy,
    pub missed: MissedPolicy,
    pub inactive_gap: InactiveGapPolicy,
    pub rephase: RephasePolicy,
    pub overload: OverloadPolicy,
}

impl CalendarSchedule {
    pub fn next_after(
        &self,
        provider: &dyn TimeZoneProvider,
        previous: Option<CalendarBoundary>,
    ) -> Result<CalendarAdvance, KarmaBoundaryError> {
        if provider.revision() != &self.tzdb {
            return Err(KarmaBoundaryError::invalid_definition(
                "calendar schedule tzdb revision does not match provider",
            ));
        }

        let previous_resolution = previous
            .map(|boundary| self.validate_previous(provider, boundary))
            .transpose()?;

        if let Some(previous) = previous
            && previous.kind == CalendarBoundaryKind::FoldFirst
            && self.fold == FoldPolicy::Both
        {
            let resolution = previous_resolution.expect("previous resolution was validated");
            let LocalTimeResolution::Fold { first, second } = resolution else {
                return Err(KarmaBoundaryError::invalid_definition(
                    "provider no longer resolves previous fold consistently",
                ));
            };
            debug_assert_eq!(first, previous.intended_at);
            return Ok(CalendarAdvance::boundary(
                CalendarBoundary {
                    requested_local: previous.requested_local,
                    intended_at: second,
                    kind: CalendarBoundaryKind::FoldSecond,
                },
                Vec::new(),
            ));
        }

        let mut skipped = Vec::new();
        let mut after_local = previous.map(|boundary| boundary.requested_local);
        for _ in 0..MAX_CALENDAR_SEARCH_STEPS {
            let candidate = match self.next_local_candidate(after_local)? {
                LocalCandidate::Value(value) => value,
                LocalCandidate::Retired => {
                    return Ok(CalendarAdvance::retired(skipped));
                }
                LocalCandidate::InvalidDay { year, month, day } => {
                    let discontinuity =
                        CalendarDiscontinuity::InvalidDayOfMonth { year, month, day };
                    match self.cadence.invalid_day {
                        InvalidDay::Skip => {
                            skipped.push(discontinuity);
                            after_local = Some(month_search_marker(year, month)?);
                            continue;
                        }
                        InvalidDay::Pause => {
                            return Ok(CalendarAdvance::paused(discontinuity, skipped));
                        }
                        InvalidDay::Clamp => {
                            return Err(KarmaBoundaryError::invalid_definition(
                                "calendar candidate generation violated month-day policy",
                            ));
                        }
                    }
                }
            };
            let resolution = provider.resolve_local(&self.timezone, candidate)?;
            validate_resolution(resolution)?;
            match resolution {
                LocalTimeResolution::Unique { instant } => {
                    validate_after_previous(previous, instant)?;
                    return Ok(CalendarAdvance::boundary(
                        CalendarBoundary {
                            requested_local: candidate,
                            intended_at: instant,
                            kind: CalendarBoundaryKind::Unique,
                        },
                        skipped,
                    ));
                }
                LocalTimeResolution::Gap {
                    before,
                    first_valid_after,
                } => match self.gap {
                    GapPolicy::Skip => {
                        skipped.push(CalendarDiscontinuity::Gap {
                            local: candidate,
                            before,
                            first_valid_after,
                        });
                        after_local = Some(candidate);
                    }
                    GapPolicy::ShiftForward => {
                        validate_after_previous(previous, first_valid_after)?;
                        return Ok(CalendarAdvance::boundary(
                            CalendarBoundary {
                                requested_local: candidate,
                                intended_at: first_valid_after,
                                kind: CalendarBoundaryKind::GapShiftedForward,
                            },
                            skipped,
                        ));
                    }
                    GapPolicy::Pause => {
                        return Ok(CalendarAdvance::paused(
                            CalendarDiscontinuity::Gap {
                                local: candidate,
                                before,
                                first_valid_after,
                            },
                            skipped,
                        ));
                    }
                },
                LocalTimeResolution::Fold { first, second } => match self.fold {
                    FoldPolicy::First | FoldPolicy::Both => {
                        validate_after_previous(previous, first)?;
                        let mut excluded = skipped;
                        if self.fold == FoldPolicy::First {
                            excluded.push(CalendarDiscontinuity::FoldSecond {
                                local: candidate,
                                instant: second,
                            });
                        }
                        return Ok(CalendarAdvance::boundary(
                            CalendarBoundary {
                                requested_local: candidate,
                                intended_at: first,
                                kind: CalendarBoundaryKind::FoldFirst,
                            },
                            excluded,
                        ));
                    }
                    FoldPolicy::Second => {
                        validate_after_previous(previous, second)?;
                        skipped.push(CalendarDiscontinuity::FoldFirst {
                            local: candidate,
                            instant: first,
                        });
                        return Ok(CalendarAdvance::boundary(
                            CalendarBoundary {
                                requested_local: candidate,
                                intended_at: second,
                                kind: CalendarBoundaryKind::FoldSecond,
                            },
                            skipped,
                        ));
                    }
                    FoldPolicy::Pause => {
                        return Ok(CalendarAdvance::paused(
                            CalendarDiscontinuity::Fold {
                                local: candidate,
                                first,
                                second,
                            },
                            skipped,
                        ));
                    }
                },
            }
        }
        Err(KarmaBoundaryError::invalid_definition(
            "calendar resolution exceeded deterministic search budget",
        ))
    }

    fn validate_previous(
        &self,
        provider: &dyn TimeZoneProvider,
        previous: CalendarBoundary,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        if !self.is_local_candidate(previous.requested_local) {
            return Err(KarmaBoundaryError::invalid_input(
                "previous calendar boundary is not on this schedule cadence",
            ));
        }
        let resolution = provider.resolve_local(&self.timezone, previous.requested_local)?;
        validate_resolution(resolution)?;
        let matches = match (previous.kind, resolution) {
            (CalendarBoundaryKind::Unique, LocalTimeResolution::Unique { instant }) => {
                previous.intended_at == instant
            }
            (
                CalendarBoundaryKind::GapShiftedForward,
                LocalTimeResolution::Gap {
                    first_valid_after, ..
                },
            ) => self.gap == GapPolicy::ShiftForward && previous.intended_at == first_valid_after,
            (CalendarBoundaryKind::FoldFirst, LocalTimeResolution::Fold { first, .. }) => {
                matches!(self.fold, FoldPolicy::First | FoldPolicy::Both)
                    && previous.intended_at == first
            }
            (CalendarBoundaryKind::FoldSecond, LocalTimeResolution::Fold { second, .. }) => {
                matches!(self.fold, FoldPolicy::Second | FoldPolicy::Both)
                    && previous.intended_at == second
            }
            _ => false,
        };
        if matches {
            Ok(resolution)
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "previous calendar boundary does not match schedule/provider resolution",
            ))
        }
    }

    fn is_local_candidate(&self, local: CivilDateTime) -> bool {
        self.cadence.produces_civil(self.anchor, local)
    }

    fn next_local_candidate(
        &self,
        after: Option<CivilDateTime>,
    ) -> Result<LocalCandidate, KarmaBoundaryError> {
        let lower = after.unwrap_or(self.anchor);
        let mut index = self.cadence.index_floor_civil(self.anchor, lower);
        for _ in 0..MAX_CALENDAR_SEARCH_STEPS {
            match self.cadence.civil_at_or_reason(self.anchor, index) {
                Ok(candidate) => {
                    if candidate >= self.anchor && after.is_none_or(|previous| candidate > previous)
                    {
                        return Ok(LocalCandidate::Value(candidate));
                    }
                }
                Err(NoOccurrence::InvalidMonthDay { year, month, day }) => {
                    let ahead = after.is_none_or(|previous| {
                        (year, month) > (previous.0.year(), previous.0.month())
                    });
                    if ahead {
                        return Ok(LocalCandidate::InvalidDay {
                            year,
                            month,
                            day: DayOfMonth::new(day as u8)?,
                        });
                    }
                }
                Err(NoOccurrence::Retired) => return Ok(LocalCandidate::Retired),
                Err(NoOccurrence::Exhausted) => {
                    return Err(KarmaBoundaryError::invalid_definition(
                        "calendar cadence ran out of representable range",
                    ));
                }
            }
            index = index.checked_add(1).ok_or_else(|| {
                KarmaBoundaryError::invalid_definition("calendar cadence overflows")
            })?;
        }
        Err(KarmaBoundaryError::invalid_definition(
            "calendar resolution exceeded deterministic search budget",
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalCandidate {
    Value(CivilDateTime),
    Retired,
    InvalidDay {
        year: i32,
        month: u32,
        day: DayOfMonth,
    },
}

fn validate_resolution(resolution: LocalTimeResolution) -> Result<(), KarmaBoundaryError> {
    match resolution {
        LocalTimeResolution::Unique { .. } => Ok(()),
        LocalTimeResolution::Gap {
            before,
            first_valid_after,
        } if before < first_valid_after => Ok(()),
        LocalTimeResolution::Fold { first, second } if first < second => Ok(()),
        LocalTimeResolution::Gap { .. } => Err(KarmaBoundaryError::invalid_definition(
            "timezone provider returned a non-increasing gap",
        )),
        LocalTimeResolution::Fold { .. } => Err(KarmaBoundaryError::invalid_definition(
            "timezone provider returned a non-increasing fold",
        )),
    }
}

fn validate_after_previous(
    previous: Option<CalendarBoundary>,
    instant: TimestampMs,
) -> Result<(), KarmaBoundaryError> {
    if previous.is_some_and(|previous| instant <= previous.intended_at) {
        Err(KarmaBoundaryError::invalid_definition(
            "calendar provider produced a non-increasing UTC boundary",
        ))
    } else {
        Ok(())
    }
}

fn last_day_of_month(year: i32, month: u32) -> Result<NaiveDate, KarmaBoundaryError> {
    for day in (28..=31).rev() {
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            return Ok(date);
        }
    }
    Err(KarmaBoundaryError::invalid_definition(
        "calendar month has no valid date",
    ))
}

fn month_search_marker(year: i32, month: u32) -> Result<CivilDateTime, KarmaBoundaryError> {
    CivilDateTime::from_date_and_time(last_day_of_month(year, month)?, CivilTime(86_399_999))
}
