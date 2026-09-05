use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::{CivilDateTime, CivilWeekday, KarmaBoundaryError, WeekdaySet};

pub const MAX_DERIVED_OCCURRENCES: usize = 512;

const MAX_SCAN_STEPS: usize = 65_536;

const MS_PER_DAY: i64 = 86_400_000;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
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
        let days = i64::from(self.weeks)
            .checked_mul(7)?
            .checked_add(i64::from(self.days))?;
        let mut total = days.checked_mul(MS_PER_DAY)?;
        total = total.checked_add(i64::from(self.hours).checked_mul(3_600_000)?)?;
        total = total.checked_add(i64::from(self.minutes).checked_mul(60_000)?)?;
        total = total.checked_add(i64::from(self.seconds).checked_mul(1_000)?)?;
        total.checked_add(i64::from(self.milliseconds))
    }

    pub fn is_zero(&self) -> bool {
        self.calendar_months() == Some(0) && self.fixed_milliseconds() == Some(0)
    }

    fn approximate_span_ms(&self) -> i64 {
        let months = i64::from(self.calendar_months().unwrap_or(0));
        months
            .saturating_mul(31 * MS_PER_DAY)
            .saturating_add(self.fixed_milliseconds().unwrap_or(0))
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(tag = "kind", rename_all = "kebab-case")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        Self {
            every: CadenceStep::default(),
            land_on: None,
            invalid_day: InvalidDay::default(),
            bound: CadenceBound::Count { occurrences: 1 },
        }
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
        let months = self
            .every
            .calendar_months()
            .ok_or(CadenceError::StepTooLarge)?;
        let fixed = self
            .every
            .fixed_milliseconds()
            .ok_or(CadenceError::StepTooLarge)?;
        if let CadenceBound::Count { occurrences: 0 } = self.bound {
            return Err(CadenceError::EmptyBound);
        }
        if months == 0 && fixed == 0 && !self.produces_at_most_one() {
            return Err(CadenceError::ZeroInterval);
        }
        Ok(())
    }

    fn produces_at_most_one(&self) -> bool {
        matches!(self.bound, CadenceBound::Count { occurrences } if occurrences <= 1)
    }

    fn count_limit(&self) -> Option<u64> {
        match self.bound {
            CadenceBound::Count { occurrences } => Some(occurrences),
            _ => None,
        }
    }

    fn past_bound(&self, landed: NaiveDateTime) -> bool {
        match self.bound {
            CadenceBound::Until { at } => landed >= at.as_naive(),
            _ => false,
        }
    }

    fn can_skip_a_candidate(&self) -> bool {
        self.invalid_day != InvalidDay::Clamp && self.every.calendar_months().unwrap_or(0) > 0
    }

    fn naive_at(
        &self,
        anchor: NaiveDateTime,
        index: u64,
        months: u32,
        fixed: i64,
    ) -> Option<NaiveDateTime> {
        let mut at = anchor;
        if months > 0 {
            let offset = index.checked_mul(u64::from(months))?;
            let offset = i32::try_from(offset).ok()?;
            let total = anchor
                .year()
                .checked_mul(12)?
                .checked_add(anchor.month0() as i32)?
                .checked_add(offset)?;
            let year = total.div_euclid(12);
            let month = total.rem_euclid(12) as u32 + 1;
            let date = self.month_day(year, month, anchor.day())?;
            at = date.and_time(anchor.time());
        }
        if fixed > 0 {
            let offset = i64::try_from(index).ok()?.checked_mul(fixed)?;
            at = at.checked_add_signed(Duration::milliseconds(offset))?;
        }
        Some(at)
    }

    pub fn civil_at(&self, anchor: CivilDateTime, index: u64) -> Option<CivilDateTime> {
        self.civil_at_or_reason(anchor, index).ok()
    }

    pub fn civil_at_or_reason(
        &self,
        anchor: CivilDateTime,
        index: u64,
    ) -> Result<CivilDateTime, NoOccurrence> {
        if self.retired_by(anchor.as_naive(), index) {
            return Err(NoOccurrence::Retired);
        }
        let months = self
            .every
            .calendar_months()
            .ok_or(NoOccurrence::Exhausted)?;
        let fixed = self
            .every
            .fixed_milliseconds()
            .ok_or(NoOccurrence::Exhausted)?;
        let anchor_naive = anchor.as_naive();
        let base = match self.naive_at(anchor_naive, index, months, fixed) {
            Some(base) => base,
            None if self.calendar_exhausted(anchor_naive, index, months) => {
                return Err(NoOccurrence::Exhausted);
            }
            None => {
                let (year, month) = self
                    .month_of(anchor_naive, index, months)
                    .ok_or(NoOccurrence::Exhausted)?;
                return Err(NoOccurrence::InvalidMonthDay {
                    year,
                    month,
                    day: anchor_naive.day(),
                });
            }
        };
        let landed = self.land_naive(base);
        if self.past_bound(landed) {
            return Err(NoOccurrence::Retired);
        }
        CivilDateTime::from_naive(landed).map_err(|_| NoOccurrence::Exhausted)
    }

    fn month_of(&self, anchor: NaiveDateTime, index: u64, months: u32) -> Option<(i32, u32)> {
        let offset = index.checked_mul(u64::from(months))?;
        let offset = i32::try_from(offset).ok()?;
        let total = anchor
            .year()
            .checked_mul(12)?
            .checked_add(anchor.month0() as i32)?
            .checked_add(offset)?;
        Some((total.div_euclid(12), total.rem_euclid(12) as u32 + 1))
    }

    pub fn index_floor_civil(&self, anchor: CivilDateTime, at: CivilDateTime) -> u64 {
        let months = self.every.calendar_months().unwrap_or(0);
        let fixed = self.every.fixed_milliseconds().unwrap_or(0);
        self.index_floor(anchor.as_naive(), at.as_naive(), months, fixed)
    }

    pub fn produces_civil(&self, anchor: CivilDateTime, at: CivilDateTime) -> bool {
        let Some(months) = self.every.calendar_months() else {
            return false;
        };
        let Some(fixed) = self.every.fixed_milliseconds() else {
            return false;
        };
        if at < anchor {
            return false;
        }
        let mut index = self.index_floor(anchor.as_naive(), at.as_naive(), months, fixed);
        for _ in 0..MAX_SCAN_STEPS {
            match self.civil_at_or_reason(anchor, index) {
                Ok(candidate) if candidate == at => return true,
                Ok(candidate) if candidate > at => return false,
                Ok(_) => {}
                Err(NoOccurrence::InvalidMonthDay { .. }) => {}
                Err(NoOccurrence::Retired | NoOccurrence::Exhausted) => return false,
            }
            index += 1;
        }
        false
    }

    fn retired_by(&self, anchor: NaiveDateTime, index: u64) -> bool {
        let Some(limit) = self.count_limit() else {
            return false;
        };
        self.ordinal_of(anchor, index) >= limit
    }

    fn ordinal_of(&self, anchor: NaiveDateTime, index: u64) -> u64 {
        if !self.can_skip_a_candidate() {
            return index;
        }
        let months = self.every.calendar_months().unwrap_or(0);
        let fixed = self.every.fixed_milliseconds().unwrap_or(0);
        let mut produced = 0u64;
        for earlier in 0..index.min(MAX_SCAN_STEPS as u64) {
            if self.naive_at(anchor, earlier, months, fixed).is_some() {
                produced += 1;
            }
        }
        produced
    }

    fn index_floor(
        &self,
        anchor: NaiveDateTime,
        target: NaiveDateTime,
        months: u32,
        fixed: i64,
    ) -> u64 {
        if target <= anchor {
            return 0;
        }
        if months > 0 {
            let elapsed = (target.year() as i64 - anchor.year() as i64) * 12
                + (target.month0() as i64 - anchor.month0() as i64);
            (elapsed.max(0) / i64::from(months)) as u64
        } else if fixed > 0 {
            let ahead = (target - anchor).num_milliseconds().max(0);
            (ahead / fixed) as u64
        } else {
            0
        }
    }

    fn land_naive(&self, at: NaiveDateTime) -> NaiveDateTime {
        let Some(allowed) = &self.land_on else {
            return at;
        };
        let mut out = at;
        for _ in 0..7 {
            let weekday = CivilWeekday::from(out.weekday());
            if allowed.iter().any(|day| day == weekday) {
                return out;
            }
            out += Duration::days(1);
        }
        out
    }

    fn calendar_exhausted(&self, anchor: NaiveDateTime, index: u64, months: u32) -> bool {
        if months == 0 {
            return true;
        }
        let Some(offset) = index.checked_mul(u64::from(months)) else {
            return true;
        };
        let Ok(offset) = i32::try_from(offset) else {
            return true;
        };
        anchor
            .year()
            .checked_mul(12)
            .and_then(|value| value.checked_add(anchor.month0() as i32))
            .and_then(|value| value.checked_add(offset))
            .is_none()
    }

    fn month_day(&self, year: i32, month: u32, day: u32) -> Option<NaiveDate> {
        let last = last_day_of_month(year, month)?;
        match self.invalid_day {
            InvalidDay::Clamp => NaiveDate::from_ymd_opt(year, month, day.min(last)),
            InvalidDay::Skip | InvalidDay::Pause => NaiveDate::from_ymd_opt(year, month, day),
        }
    }

    pub fn between(
        &self,
        anchor: DateTime<Utc>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Derived, CadenceError> {
        self.validate()?;
        let mut derived = Derived::default();
        if to <= from {
            return Ok(derived);
        }

        let slack = if self.land_on.is_some() {
            Duration::days(7)
        } else {
            Duration::zero()
        };
        let scan_from = from - slack;

        let months = self.every.calendar_months().unwrap_or(0);
        let fixed = self.every.fixed_milliseconds().unwrap_or(0);
        let anchor_naive = anchor.naive_utc();

        let mut index: u64 = if self.count_limit().is_some() && self.can_skip_a_candidate() {
            0
        } else {
            self.index_floor(anchor_naive, scan_from.naive_utc(), months, fixed)
        };

        let mut found: std::collections::BTreeSet<DateTime<Utc>> = Default::default();

        let mut steps = 0usize;
        let mut reached_end = false;

        while steps < MAX_SCAN_STEPS {
            steps += 1;
            if self.retired_by(anchor_naive, index) {
                reached_end = true;
                break;
            }
            let Some(base) = self.naive_at(anchor_naive, index, months, fixed) else {
                if self.calendar_exhausted(anchor_naive, index, months) {
                    reached_end = true;
                    break;
                }
                index += 1;
                continue;
            };
            let landed = self.land_naive(base);
            if self.past_bound(landed) {
                reached_end = true;
                break;
            }
            if base >= to.naive_utc() {
                reached_end = true;
                break;
            }
            index += 1;
            if base < anchor_naive {
                continue;
            }
            let landed = Utc.from_utc_datetime(&landed);
            if landed >= from && landed < to {
                found.insert(landed);
                if found.len() > MAX_DERIVED_OCCURRENCES {
                    derived.truncated = true;
                    found.pop_last();
                    break;
                }
            }
        }

        if !reached_end && steps >= MAX_SCAN_STEPS {
            derived.truncated = true;
        }

        derived.dates = found.into_iter().collect();
        Ok(derived)
    }

    pub fn next_on_or_after(
        &self,
        anchor: DateTime<Utc>,
        after: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, CadenceError> {
        self.validate()?;
        let span = self
            .every
            .approximate_span_ms()
            .saturating_mul(2)
            .saturating_add(8 * MS_PER_DAY)
            .max(400 * MS_PER_DAY);
        let horizon = after
            .checked_add_signed(Duration::milliseconds(span))
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
        Ok(self
            .between(anchor, after, horizon)?
            .dates
            .into_iter()
            .next())
    }

    pub fn preceding(
        &self,
        anchor: DateTime<Utc>,
        before: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, CadenceError> {
        self.validate()?;
        if before <= anchor {
            return Ok(None);
        }
        let months = self.every.calendar_months().unwrap_or(0);
        let fixed = self.every.fixed_milliseconds().unwrap_or(0);
        let anchor_naive = anchor.naive_utc();
        let before_naive = before.naive_utc();

        let mut best: Option<NaiveDateTime> = None;
        let consider = |candidate: NaiveDateTime, best: &mut Option<NaiveDateTime>| {
            if candidate < before_naive {
                *best = Some(best.map_or(candidate, |held: NaiveDateTime| held.max(candidate)));
            }
        };

        if self.count_limit().is_some() && self.can_skip_a_candidate() {
            let mut index: u64 = 0;
            let mut steps = 0usize;
            while steps < MAX_SCAN_STEPS {
                steps += 1;
                if self.retired_by(anchor_naive, index) {
                    break;
                }
                let Some(base) = self.naive_at(anchor_naive, index, months, fixed) else {
                    if self.calendar_exhausted(anchor_naive, index, months) {
                        break;
                    }
                    index += 1;
                    continue;
                };
                let landed = self.land_naive(base);
                if self.past_bound(landed) {
                    break;
                }
                if base >= before_naive {
                    break;
                }
                index += 1;
                if base >= anchor_naive {
                    consider(landed, &mut best);
                }
            }
            return Ok(best.map(|at| Utc.from_utc_datetime(&at)));
        }

        let mut index = self.index_floor(anchor_naive, before_naive, months, fixed);
        let slack = if self.land_on.is_some() { 8 } else { 0 };
        let mut past_first_hit = 0usize;
        let mut steps = 0usize;
        loop {
            steps += 1;
            if steps >= MAX_SCAN_STEPS {
                break;
            }
            if !self.retired_by(anchor_naive, index)
                && let Some(base) = self.naive_at(anchor_naive, index, months, fixed)
                && base >= anchor_naive
            {
                let landed = self.land_naive(base);
                if !self.past_bound(landed) {
                    consider(landed, &mut best);
                }
            }
            if best.is_some() {
                past_first_hit += 1;
                if past_first_hit > slack {
                    break;
                }
            }
            if index == 0 {
                break;
            }
            index -= 1;
        }
        Ok(best.map(|at| Utc.from_utc_datetime(&at)))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Derived {
    pub dates: Vec<DateTime<Utc>>,
    pub truncated: bool,
}

impl Derived {
    pub fn is_empty(&self) -> bool {
        self.dates.is_empty()
    }

    pub fn len(&self) -> usize {
        self.dates.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &DateTime<Utc>> {
        self.dates.iter()
    }
}

impl IntoIterator for Derived {
    type Item = DateTime<Utc>;
    type IntoIter = std::vec::IntoIter<DateTime<Utc>>;

    fn into_iter(self) -> Self::IntoIter {
        self.dates.into_iter()
    }
}

fn last_day_of_month(year: i32, month: u32) -> Option<u32> {
    let (next_year, next_month) = if month == 12 {
        (year.checked_add(1)?, 1)
    } else {
        (year, month + 1)
    };
    let first_of_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)?;
    Some(first_of_next.pred_opt()?.day())
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

impl core::fmt::Display for CadenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::ZeroInterval => {
                "a rule with no step produces one instant; bound it to one occurrence or give it \
                 a step"
            }
            Self::StepTooLarge => "the step is too large to represent",
            Self::EmptyBound => "a rule bounded to zero occurrences would never produce anything",
        };
        f.write_str(message)
    }
}

impl std::error::Error for CadenceError {}

impl From<CadenceError> for KarmaBoundaryError {
    fn from(error: CadenceError) -> Self {
        KarmaBoundaryError::invalid_definition(error.to_string())
    }
}
