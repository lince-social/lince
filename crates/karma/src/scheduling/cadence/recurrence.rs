//! Indexed civil recurrence and candidate resolution shared by range queries.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

use super::{Cadence, CadenceBound, InvalidDay, MAX_SCAN_STEPS, NoOccurrence, StepSize};
use crate::{CivilDateTime, CivilWeekday};

impl Cadence {
    pub fn civil_at(&self, anchor: CivilDateTime, index: u64) -> Option<CivilDateTime> {
        self.civil_at_or_reason(anchor, index).ok()
    }

    /// Resolves an indexed candidate, including weekday landing.
    ///
    /// # Errors
    ///
    /// Reports a skipped month day, retirement at a bound, or an exhausted date range.
    pub fn civil_at_or_reason(
        &self,
        anchor: CivilDateTime,
        index: u64,
    ) -> Result<CivilDateTime, NoOccurrence> {
        let size = self.every.size();
        let recurrence = Recurrence::new(
            self,
            anchor.as_naive(),
            size.unwrap_or_else(|| self.every.representable_size()),
        );
        if recurrence.retired_at(index) {
            return Err(NoOccurrence::Retired);
        }
        size.ok_or(NoOccurrence::Exhausted)?;
        recurrence.civil_at(index)
    }

    pub fn index_floor_civil(&self, anchor: CivilDateTime, at: CivilDateTime) -> u64 {
        Recurrence::new(self, anchor.as_naive(), self.every.representable_size())
            .index_floor(at.as_naive())
    }

    pub fn produces_civil(&self, anchor: CivilDateTime, at: CivilDateTime) -> bool {
        let Some(size) = self.every.size() else {
            return false;
        };
        if at < anchor {
            return false;
        }
        let recurrence = Recurrence::new(self, anchor.as_naive(), size);
        let start = recurrence.index_floor(at.as_naive());
        let counts_valid = recurrence.counts_valid_candidates();
        let mut produced = if counts_valid {
            recurrence.ordinal_at(start)
        } else {
            0
        };
        for index in (start..).take(MAX_SCAN_STEPS) {
            let ordinal = if counts_valid { produced } else { index };
            if self.count_limit().is_some_and(|limit| ordinal >= limit) {
                break;
            }
            match recurrence.civil_at(index) {
                Ok(candidate) if candidate == at => return true,
                Ok(candidate) if candidate > at => break,
                Ok(_) if counts_valid && index < MAX_SCAN_STEPS as u64 => produced += 1,
                Ok(_) | Err(NoOccurrence::InvalidMonthDay { .. }) => {}
                Err(NoOccurrence::Retired | NoOccurrence::Exhausted) => break,
            }
        }
        false
    }
}

pub(super) struct Recurrence<'a> {
    cadence: &'a Cadence,
    pub(super) anchor: NaiveDateTime,
    size: StepSize,
}

impl<'a> Recurrence<'a> {
    pub(super) fn new(cadence: &'a Cadence, anchor: NaiveDateTime, size: StepSize) -> Self {
        Self {
            cadence,
            anchor,
            size,
        }
    }

    pub(super) fn base_at(&self, index: u64) -> Option<NaiveDateTime> {
        let mut base = self.anchor;
        if self.size.months > 0 {
            let (year, month) = self.month_at(index)?;
            let day = self.anchor.day();
            let last = last_day_of_month(year, month)?;
            let day = if self.cadence.invalid_day == InvalidDay::Clamp {
                day.min(last)
            } else {
                day
            };
            base = NaiveDate::from_ymd_opt(year, month, day)?.and_time(base.time());
        }
        if self.size.fixed > 0 {
            let offset = i64::try_from(index).ok()?.checked_mul(self.size.fixed)?;
            base = base.checked_add_signed(Duration::milliseconds(offset))?;
        }
        Some(base)
    }

    fn civil_at(&self, index: u64) -> Result<CivilDateTime, NoOccurrence> {
        let base = match self.base_at(index) {
            Some(base) => base,
            None => {
                let (year, month) = self.month_at(index).ok_or(NoOccurrence::Exhausted)?;
                return Err(NoOccurrence::InvalidMonthDay {
                    year,
                    month,
                    day: self.anchor.day(),
                });
            }
        };
        let landed = self.land(base).ok_or(NoOccurrence::Exhausted)?;
        if self.past_bound(landed) {
            return Err(NoOccurrence::Retired);
        }
        CivilDateTime::from_naive(landed).map_err(|_| NoOccurrence::Exhausted)
    }

    fn month_at(&self, index: u64) -> Option<(i32, u32)> {
        if self.size.months == 0 {
            return None;
        }
        let offset = i32::try_from(index.checked_mul(u64::from(self.size.months))?).ok()?;
        let total = self
            .anchor
            .year()
            .checked_mul(12)?
            .checked_add(self.anchor.month0() as i32)?
            .checked_add(offset)?;
        Some((total.div_euclid(12), total.rem_euclid(12) as u32 + 1))
    }

    pub(super) fn index_floor(&self, target: NaiveDateTime) -> u64 {
        if target <= self.anchor {
            return 0;
        }
        if self.size.months > 0 {
            let years = i64::from(target.year()) - i64::from(self.anchor.year());
            let months = i64::from(target.month0()) - i64::from(self.anchor.month0());
            ((years * 12 + months).max(0) / i64::from(self.size.months)) as u64
        } else if self.size.fixed > 0 {
            let millis = (target - self.anchor).num_milliseconds().max(0);
            (millis / self.size.fixed) as u64
        } else {
            0
        }
    }

    pub(super) fn bounded_landing(&self, base: NaiveDateTime) -> Option<NaiveDateTime> {
        self.land(base).filter(|&landed| !self.past_bound(landed))
    }

    fn past_bound(&self, landed: NaiveDateTime) -> bool {
        matches!(self.cadence.bound, CadenceBound::Until { at } if landed >= at.as_naive())
    }

    fn land(&self, mut at: NaiveDateTime) -> Option<NaiveDateTime> {
        if let Some(allowed) = &self.cadence.land_on {
            for _ in 0..7 {
                let weekday = CivilWeekday::from(at.weekday());
                if allowed.iter().any(|day| day == weekday) {
                    break;
                }
                at = at.checked_add_signed(Duration::days(1))?;
            }
        }
        Some(at)
    }

    pub(super) fn calendar_exhausted(&self, index: u64) -> bool {
        self.month_at(index).is_none()
    }

    pub(super) fn counts_valid_candidates(&self) -> bool {
        self.cadence.count_limit().is_some()
            && self.size.months > 0
            && self.cadence.invalid_day != InvalidDay::Clamp
    }

    pub(super) fn retired_at(&self, index: u64) -> bool {
        self.cadence
            .count_limit()
            .is_some_and(|limit| self.ordinal_at(index) >= limit)
    }

    fn ordinal_at(&self, index: u64) -> u64 {
        if !self.counts_valid_candidates() {
            return index;
        }
        (0..index.min(MAX_SCAN_STEPS as u64))
            .filter(|&earlier| self.base_at(earlier).is_some())
            .count() as u64
    }
}

fn last_day_of_month(year: i32, month: u32) -> Option<u32> {
    let (next_year, next_month) = if month == 12 {
        (year.checked_add(1)?, 1)
    } else {
        (year, month + 1)
    };
    Some(
        NaiveDate::from_ymd_opt(next_year, next_month, 1)?
            .pred_opt()?
            .day(),
    )
}
