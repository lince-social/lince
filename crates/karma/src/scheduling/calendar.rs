//! Civil time, timezone providers, and calendar schedule advancement.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

mod advance;
mod civil;
mod timezone;

pub use advance::{
    CalendarAdvance, CalendarBoundary, CalendarBoundaryKind, CalendarDiscontinuity,
    CalendarSchedule,
};
pub use civil::{CivilDateTime, CivilTime, CivilWeekday, DayOfMonth, WeekdaySet};
pub use timezone::{
    FoldPolicy, GapPolicy, LocalTimeResolution, TimeZoneId, TimeZoneProvider, TzdbRevision,
    TzdbVersion,
};

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
