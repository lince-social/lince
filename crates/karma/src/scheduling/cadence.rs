//! Cadence rules, step arithmetic, and schedule bounds.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{CivilDateTime, KarmaBoundaryError, WeekdaySet};

mod range;
mod recurrence;

pub use range::Derived;

pub const MAX_DERIVED_OCCURRENCES: usize = 512;

const MAX_SCAN_STEPS: usize = 65_536;

const MS_PER_DAY: i64 = 86_400_000;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum InvalidDay {
    #[default]
    Clamp,
    Skip,
    Pause,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(default)]
#[derive(schemars::JsonSchema)]
pub struct CadenceStep {
    pub years: u32,
    pub months: u32,
    pub weeks: u32,
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub milliseconds: u32,
}

impl CadenceStep {
    pub fn calendar_months(&self) -> Option<u32> {
        self.years.checked_mul(12)?.checked_add(self.months)
    }

    pub fn fixed_milliseconds(&self) -> Option<i64> {
        // Even the maximum fixed fields sum to less than 3e18 milliseconds.
        let days = i64::from(self.weeks) * 7 + i64::from(self.days);
        Some(
            days * MS_PER_DAY
                + i64::from(self.hours) * 3_600_000
                + i64::from(self.minutes) * 60_000
                + i64::from(self.seconds) * 1_000
                + i64::from(self.milliseconds),
        )
    }

    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    fn size(&self) -> Option<StepSize> {
        Some(StepSize {
            months: self.calendar_months()?,
            fixed: self.fixed_milliseconds()?,
        })
    }

    fn representable_size(&self) -> StepSize {
        StepSize {
            months: self.calendar_months().unwrap_or_default(),
            fixed: self.fixed_milliseconds().unwrap_or_default(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum CadenceBound {
    #[default]
    Unbounded,
    Count {
        occurrences: u64,
    },
    Until {
        at: CivilDateTime,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Cadence {
    pub every: CadenceStep,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub land_on: Option<WeekdaySet>,
    #[serde(default)]
    pub invalid_day: InvalidDay,
    #[serde(default)]
    pub bound: CadenceBound,
}

impl Cadence {
    pub fn every(step: CadenceStep) -> Self {
        Self {
            every: step,
            land_on: None,
            invalid_day: InvalidDay::default(),
            bound: CadenceBound::default(),
        }
    }

    pub fn once() -> Self {
        Self::every(CadenceStep::default()).taking(1)
    }

    pub fn every_days(days: u32) -> Self {
        Self::every(CadenceStep {
            days,
            ..Default::default()
        })
    }

    pub fn every_weeks(weeks: u32) -> Self {
        Self::every(CadenceStep {
            weeks,
            ..Default::default()
        })
    }

    pub fn every_months(months: u32) -> Self {
        Self::every(CadenceStep {
            months,
            ..Default::default()
        })
    }

    pub fn every_years(years: u32) -> Self {
        Self::every(CadenceStep {
            years,
            ..Default::default()
        })
    }

    pub fn landing_on(mut self, weekdays: WeekdaySet) -> Self {
        self.land_on = Some(weekdays);
        self
    }

    pub fn with_invalid_day(mut self, policy: InvalidDay) -> Self {
        self.invalid_day = policy;
        self
    }

    pub fn taking(mut self, occurrences: u64) -> Self {
        self.bound = CadenceBound::Count { occurrences };
        self
    }

    pub fn until(mut self, at: CivilDateTime) -> Self {
        self.bound = CadenceBound::Until { at };
        self
    }

    pub fn validate(&self) -> Result<(), CadenceError> {
        self.validated_size().map(|_| ())
    }

    fn validated_size(&self) -> Result<StepSize, CadenceError> {
        let size = self.every.size().ok_or(CadenceError::StepTooLarge)?;
        if matches!(self.bound, CadenceBound::Count { occurrences: 0 }) {
            return Err(CadenceError::EmptyBound);
        }
        if size.months == 0
            && size.fixed == 0
            && !matches!(self.bound, CadenceBound::Count { occurrences: 1 })
        {
            return Err(CadenceError::ZeroInterval);
        }
        Ok(size)
    }

    fn count_limit(&self) -> Option<u64> {
        match self.bound {
            CadenceBound::Count { occurrences } => Some(occurrences),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct StepSize {
    months: u32,
    fixed: i64,
}

impl StepSize {
    fn approximate_span_ms(self) -> i64 {
        i64::from(self.months)
            .saturating_mul(31 * MS_PER_DAY)
            .saturating_add(self.fixed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoOccurrence {
    InvalidMonthDay { year: i32, month: u32, day: u32 },
    Retired,
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadenceError {
    ZeroInterval,
    StepTooLarge,
    EmptyBound,
}

impl fmt::Display for CadenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ZeroInterval => {
                "a rule with no step produces one instant; bound it to one occurrence or give it \
                 a step"
            }
            Self::StepTooLarge => "the step is too large to represent",
            Self::EmptyBound => "a rule bounded to zero occurrences would never produce anything",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CadenceError {}

impl From<CadenceError> for KarmaBoundaryError {
    fn from(error: CadenceError) -> Self {
        KarmaBoundaryError::invalid_definition(error.to_string())
    }
}
