//! Validated civil-time values and calendar fields.

use std::{collections::BTreeSet, fmt};

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike, Weekday};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::KarmaBoundaryError;

const DATETIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f";
const TIME_FORMAT_ERROR: &str = "civil time must use HH:MM:SS.sss";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema)]
#[schemars(with = "String")]
pub struct CivilDateTime(NaiveDateTime);

impl CivilDateTime {
    #[allow(
        clippy::too_many_arguments,
        reason = "Calendar construction takes the explicit scheduling parameters"
    )]
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
        let parsed = NaiveDateTime::parse_from_str(value, DATETIME_FORMAT)
            .map_err(|_| KarmaBoundaryError::invalid_input("civil datetime is invalid"))?;
        let civil = Self::from_naive(parsed)?;
        if civil.to_string() != value {
            return Err(KarmaBoundaryError::invalid_input(
                "civil datetime must use YYYY-MM-DDTHH:MM:SS.sss",
            ));
        }
        Ok(civil)
    }

    pub const fn as_naive(self) -> NaiveDateTime {
        self.0
    }

    /// Drops submillisecond precision.
    ///
    /// # Errors
    ///
    /// Rejects years outside `0000..=9999`.
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

    pub(super) fn end_of_month(year: i32, month: u32) -> Result<Self, KarmaBoundaryError> {
        let day = (28..=31)
            .rev()
            .find(|&day| NaiveDate::from_ymd_opt(year, month, day).is_some())
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_definition("calendar month has no valid date")
            })?;
        Self::new(year, month, day, 23, 59, 59, 999)
    }
}

impl fmt::Display for CivilDateTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.format(DATETIME_FORMAT).write_to(formatter)
    }
}

impl Serialize for CivilDateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
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
            return Err(KarmaBoundaryError::invalid_input(TIME_FORMAT_ERROR));
        };
        if fraction.len() != 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(KarmaBoundaryError::invalid_input(
                "civil time must use exactly three fractional digits",
            ));
        }
        let mut fields = whole.split(':');
        let [Some(hour), Some(minute), Some(second), None] =
            [fields.next(), fields.next(), fields.next(), fields.next()]
        else {
            return Err(KarmaBoundaryError::invalid_input(TIME_FORMAT_ERROR));
        };
        if [hour, minute, second].iter().any(|field| field.len() != 2) {
            return Err(KarmaBoundaryError::invalid_input(TIME_FORMAT_ERROR));
        }
        let parse = |field: &str| {
            field
                .parse::<u32>()
                .map_err(|_| KarmaBoundaryError::invalid_input("civil time is invalid"))
        };
        let result = Self::new(
            parse(hour)?,
            parse(minute)?,
            parse(second)?,
            parse(fraction)?,
        )?;
        // Integer parsing accepts a leading plus. Check it after validating the ranges.
        if [hour, minute, second]
            .iter()
            .any(|field| field.starts_with('+'))
        {
            return Err(KarmaBoundaryError::invalid_input(TIME_FORMAT_ERROR));
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
}

impl fmt::Display for CivilTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:02}:{:02}:{:02}.{:03}",
            self.hour(),
            self.minute(),
            self.second(),
            self.millisecond()
        )
    }
}

impl Serialize for CivilTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema, Serialize)]
#[serde(transparent)]
#[schemars(!transparent)]
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

impl<'de> Deserialize<'de> for WeekdaySet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let days = BTreeSet::<CivilWeekday>::deserialize(deserializer)?;
        Self::new(days).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
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

impl<'de> Deserialize<'de> for DayOfMonth {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u8::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}
