//! Bounded UTC range, successor, and predecessor queries.

use std::{collections::BTreeSet, vec::IntoIter};

use chrono::{DateTime, Duration, NaiveDateTime, Utc};

use super::{
    Cadence, CadenceError, MAX_DERIVED_OCCURRENCES, MAX_SCAN_STEPS, MS_PER_DAY,
    recurrence::Recurrence,
};

impl Cadence {
    /// Derives up to 512 distinct instants in the half-open window.
    ///
    /// # Errors
    ///
    /// Rejects an invalid step or occurrence bound.
    pub fn between(
        &self,
        anchor: DateTime<Utc>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Derived, CadenceError> {
        let recurrence = Recurrence::new(self, anchor.naive_utc(), self.validated_size()?);
        Ok(self.derive(&recurrence, from, to))
    }

    fn derive(
        &self,
        recurrence: &Recurrence<'_>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Derived {
        let mut found = BTreeSet::new();
        let truncated = self.scan_window(recurrence, from, to, |landed| {
            found.insert(landed);
            if found.len() > MAX_DERIVED_OCCURRENCES {
                found.pop_last();
                return true;
            }
            false
        });
        Derived {
            dates: found.into_iter().map(|at| at.and_utc()).collect(),
            truncated,
        }
    }

    pub fn next_on_or_after(
        &self,
        anchor: DateTime<Utc>,
        after: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, CadenceError> {
        let size = self.validated_size()?;
        let recurrence = Recurrence::new(self, anchor.naive_utc(), size);
        let span = size
            .approximate_span_ms()
            .saturating_mul(2)
            .saturating_add(8 * MS_PER_DAY)
            .max(400 * MS_PER_DAY);
        let horizon = after
            .checked_add_signed(Duration::milliseconds(span))
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
        if self.land_on.is_some() {
            return Ok(self.derive(&recurrence, after, horizon).into_iter().next());
        }
        // Without weekday landing, candidates are ordered by their base instants.
        let mut first = None;
        self.scan_window(&recurrence, after, horizon, |at| {
            first = Some(at.and_utc());
            true
        });
        Ok(first)
    }

    pub fn preceding(
        &self,
        anchor: DateTime<Utc>,
        before: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, CadenceError> {
        let size = self.validated_size()?;
        if before <= anchor {
            return Ok(None);
        }
        let (anchor, before) = (anchor.naive_utc(), before.naive_utc());
        let recurrence = Recurrence::new(self, anchor, size);
        let mut best = None;
        let consider = |at, best: &mut Option<NaiveDateTime>| {
            if at < before {
                *best = (*best).max(Some(at));
            }
        };

        if recurrence.counts_valid_candidates() {
            self.scan_forward(&recurrence, 0, before, |at| {
                consider(at, &mut best);
                false
            });
        } else {
            let slack = if self.land_on.is_some() { 8 } else { 0 };
            let mut hits = 0;
            for index in (0..=recurrence.index_floor(before))
                .rev()
                .take(MAX_SCAN_STEPS - 1)
            {
                if !recurrence.retired_at(index)
                    && let Some(base) = recurrence.base_at(index)
                    && base >= anchor
                    && let Some(at) = recurrence.bounded_landing(base)
                {
                    consider(at, &mut best);
                }
                if best.is_some() {
                    hits += 1;
                    if hits > slack {
                        break;
                    }
                }
            }
        }
        Ok(best.map(|at| at.and_utc()))
    }

    fn scan_window(
        &self,
        recurrence: &Recurrence<'_>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        mut visit: impl FnMut(NaiveDateTime) -> bool,
    ) -> bool {
        if to <= from {
            return false;
        }
        let slack = if self.land_on.is_some() {
            Duration::days(7)
        } else {
            Duration::zero()
        };
        let scan_from = from
            .checked_sub_signed(slack)
            .unwrap_or(DateTime::<Utc>::MIN_UTC)
            .naive_utc();
        let start = if recurrence.counts_valid_candidates() {
            0
        } else {
            recurrence.index_floor(scan_from)
        };
        let (from, to) = (from.naive_utc(), to.naive_utc());
        self.scan_forward(recurrence, start, to, |at| {
            at >= from && at < to && visit(at)
        })
    }

    /// Returns whether the visitor stopped the scan or the scan reached its budget.
    fn scan_forward(
        &self,
        recurrence: &Recurrence<'_>,
        start: u64,
        to: NaiveDateTime,
        mut visit: impl FnMut(NaiveDateTime) -> bool,
    ) -> bool {
        let limit = self.count_limit();
        let counts_valid = recurrence.counts_valid_candidates();
        let anchor = recurrence.anchor;
        let mut produced = 0;
        for index in (start..).take(MAX_SCAN_STEPS) {
            let ordinal = if counts_valid { produced } else { index };
            if limit.is_some_and(|limit| ordinal >= limit) {
                return false;
            }
            let Some(base) = recurrence.base_at(index) else {
                if recurrence.calendar_exhausted(index) {
                    return false;
                }
                continue;
            };
            let Some(landed) = recurrence.bounded_landing(base) else {
                return false;
            };
            if base >= to {
                return false;
            }
            produced += 1;
            if base >= anchor && visit(landed) {
                return true;
            }
        }
        true
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
    type IntoIter = IntoIter<DateTime<Utc>>;

    fn into_iter(self) -> Self::IntoIter {
        self.dates.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Datelike;

    use super::*;
    use crate::{CivilWeekday, WeekdaySet};

    #[test]
    fn weekday_landing_handles_the_limits_of_utc_dates() {
        let min = DateTime::<Utc>::MIN_UTC;
        let from_min = Cadence::once().landing_on(
            WeekdaySet::new([CivilWeekday::from(min.weekday())]).expect("one allowed weekday"),
        );
        assert_eq!(
            from_min
                .next_on_or_after(min, min)
                .expect("valid one-shot cadence"),
            Some(min),
        );

        let max = DateTime::<Utc>::MAX_UTC;
        let anchor = max - Duration::hours(1);
        let beyond_max = Cadence::once().landing_on(
            WeekdaySet::new([CivilWeekday::from(max.weekday().succ())])
                .expect("one allowed weekday"),
        );
        assert_eq!(
            beyond_max
                .between(anchor, anchor, max)
                .expect("valid one-shot cadence"),
            Derived::default(),
        );
        assert_eq!(
            beyond_max
                .preceding(anchor, max)
                .expect("valid one-shot cadence"),
            None,
        );
    }
}
