//! When something is expected to happen. One algebra, one type.
//!
//! There used to be two. A [`Cadence`] answered "on which instants is this
//! declared?" in UTC with no provider, and a `CalendarRule` answered "when does
//! this next fire?" in civil time through a pinned tzdb. The split was never a
//! statement about time; it was a statement about the crate graph — a read path
//! cannot reach the scheduler's timezone registry, so a second type dodged the
//! import. That is letting a dependency edge dictate the domain model.
//!
//! There is one concept here: *a rule for producing instants*. Saying "this
//! happens on the 14th" is that rule bounded to one occurrence and then retired.
//! Saying "this happens monthly until June" is the same rule with a later bound.
//! Whether an instant then *fires* is a separate question with a separate
//! answer — see the note on consequence below — and it is a property of what the
//! schedule is attached to, never a reason for a second kind of schedule.
//!
//! # The shape of a step
//!
//! A step is a *sum* of components, not a choice between them, so
//! `1 month + 1 day + 1 second + 10 milliseconds` is one rule rather than four
//! competing ones. Components apply from the largest unit down:
//!
//! 1. **Calendar** — years and months, resolved against the anchor's own
//!    day-of-month, with a short month handled by [`InvalidDay`].
//! 2. **Fixed** — weeks, days, hours, minutes, seconds and milliseconds, added
//!    to the calendar result as wall-clock time.
//! 3. **Landing** — optionally roll forward whole days until the instant falls
//!    on an allowed weekday.
//!
//! Landing is applied *last and per-occurrence*, never fed back into the
//! series. A rule that lands on Friday still steps monthly from its anchor; if
//! landing advanced the phase, every occurrence would drift later than the last
//! and a monthly rule would slowly become a "whenever" rule.
//!
//! # Wall-clock, not elapsed
//!
//! Every component is added in *wall-clock* space, which is why the generator
//! works on [`NaiveDateTime`] and not on an instant. "Every day at 09:00" means
//! 09:00 on each day, including the day that is 23 or 25 hours long. Turning a
//! wall-clock instant into a real one is the zone's job, and the zone is
//! injected rather than imported: a read path passes UTC, where the two spaces
//! coincide, and the scheduler passes a real tzdb. Same rule, same arithmetic,
//! one honest difference in who resolves the answer.
//!
//! # Consequence is not part of the schedule
//!
//! What must survive the merge is that a declaration and an effect are not the
//! same thing. A schedule saying rent is due does nothing; a rule that pays rent
//! spends authority. If that distinction were erased, either declarations would
//! start firing or every declaration would drag the grant machinery behind it.
//! So it lives as a field on whatever *binds* a schedule to an action — declare
//! only, propose for review, or apply under a named grant — and not as a second
//! schedule type.
//!
//! Nothing here is domain-specific. A monthly rent, a weekly backup review, and
//! a quarterly stock count are the same shape.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::{CivilDateTime, CivilWeekday, KarmaBoundaryError, WeekdaySet};

/// The largest number of instants a single derivation will return. A read path
/// asking for "the next century of a daily rule" must not be able to allocate
/// an unbounded vector, so the window is clamped rather than trusted.
pub const MAX_DERIVED_OCCURRENCES: usize = 512;

/// How many candidate instants may be *examined* before a derivation gives up.
///
/// This is a separate budget from [`MAX_DERIVED_OCCURRENCES`] and it is the one
/// that matters for a fast rule. A 10 ms step that lands on Friday collapses
/// millions of candidates onto a handful of Fridays, so a result-count cap alone
/// would spin for hours while the answer set barely grew. Bounding the scan
/// keeps the worst case linear in this constant instead of in the window.
const MAX_SCAN_STEPS: usize = 65_536;

/// Milliseconds in a day, for the fixed half of a step. This is wall-clock: a
/// "day" added here means the same clock time tomorrow, and what that is worth
/// in elapsed seconds is the zone's business, not this module's.
const MS_PER_DAY: i64 = 86_400_000;

/// What a step means when the calendar part lands on a day the month does not
/// have — the "31st of February" question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InvalidDay {
    /// Pull back to the month's last day. A rule anchored on the 31st means the
    /// end of the month, not "skip me seven times a year".
    #[default]
    Clamp,
    /// Produce nothing that month. Chosen when the date is the point — a
    /// contract that only falls due on a real 31st.
    Skip,
    /// Stop and ask. Only meaningful where a schedule drives execution: silently
    /// clamping or skipping an effect is a decision nobody made, so the runtime
    /// is allowed to refuse to guess.
    Pause,
}

/// One step of a repeating rule, as a sum of components.
///
/// Every field is a count, not a duration, because months are not a fixed
/// length and must survive to the calendar arithmetic intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[derive(Serialize, Deserialize)]
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
    /// The calendar half, in whole months.
    pub fn calendar_months(&self) -> Option<u32> {
        self.years.checked_mul(12)?.checked_add(self.months)
    }

    /// The exact half, in milliseconds of wall-clock time.
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

    /// A step with no components advances nothing. That is legal for exactly one
    /// rule — the one-shot — and rejected for every other.
    pub fn is_zero(&self) -> bool {
        self.calendar_months() == Some(0) && self.fixed_milliseconds() == Some(0)
    }

    /// Roughly how long one step spans, used only to size a search horizon.
    fn approximate_span_ms(&self) -> i64 {
        let months = i64::from(self.calendar_months().unwrap_or(0));
        months
            .saturating_mul(31 * MS_PER_DAY)
            .saturating_add(self.fixed_milliseconds().unwrap_or(0))
    }
}

/// When a rule stops producing.
///
/// This is the whole of "something happens on day X, once". A promise, a single
/// dated reminder and a one-off transfer are all `Count(1)`: the rule produces
/// its anchor and retires. There is no separate one-shot type, and nothing
/// downstream has to special-case one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CadenceBound {
    /// Repeats until something outside the schedule stops it.
    #[default]
    Unbounded,
    /// Retires after this many occurrences. Zero is rejected by `validate`.
    Count { occurrences: u64 },
    /// Retires at this wall-clock instant, exclusive — half-open like every
    /// other window here, so a bound and the next rule's anchor can coincide
    /// without one instant belonging to both.
    Until { at: CivilDateTime },
}

/// How a declared change repeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cadence {
    /// The compound step, applied from the anchor by whole multiples.
    pub every: CadenceStep,
    /// If set, roll each instant forward to the next allowed weekday.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub land_on: Option<WeekdaySet>,
    /// What a short month means for the calendar half of the step.
    #[serde(default)]
    pub invalid_day: InvalidDay,
    /// When the rule retires.
    #[serde(default)]
    pub bound: CadenceBound,
}

impl Cadence {
    /// A step built from components, unbounded, with no landing rule.
    pub fn every(step: CadenceStep) -> Self {
        Self {
            every: step,
            land_on: None,
            invalid_day: InvalidDay::default(),
            bound: CadenceBound::default(),
        }
    }

    /// Exactly one occurrence, at the anchor, and then nothing.
    ///
    /// "This happens on the 14th" — a promise, a dated reminder, a one-off
    /// transfer. The step is empty because there is no second instant for it to
    /// reach, which is the one case where an empty step is not a rule that
    /// repeats one instant forever.
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

    /// Every `months`th month, on the anchor's own day-of-month.
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

    /// Roll each occurrence forward to one of these weekdays.
    pub fn landing_on(mut self, weekdays: WeekdaySet) -> Self {
        self.land_on = Some(weekdays);
        self
    }

    pub fn with_invalid_day(mut self, policy: InvalidDay) -> Self {
        self.invalid_day = policy;
        self
    }

    /// Retire after `occurrences` instants.
    pub fn taking(mut self, occurrences: u64) -> Self {
        self.bound = CadenceBound::Count { occurrences };
        self
    }

    /// Retire at this wall-clock instant, exclusive.
    pub fn until(mut self, at: CivilDateTime) -> Self {
        self.bound = CadenceBound::Until { at };
        self
    }

    /// Reject a rule that could never produce a date, or that would produce
    /// them without advancing.
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

    /// Whether the bound retires the rule before a second occurrence, which is
    /// the only circumstance under which a zero step is meaningful.
    fn produces_at_most_one(&self) -> bool {
        matches!(self.bound, CadenceBound::Count { occurrences } if occurrences <= 1)
    }

    /// The maximum number of occurrences, if the bound sets one.
    fn count_limit(&self) -> Option<u64> {
        match self.bound {
            CadenceBound::Count { occurrences } => Some(occurrences),
            _ => None,
        }
    }

    /// Whether an instant is past the rule's retirement.
    fn past_bound(&self, landed: NaiveDateTime) -> bool {
        match self.bound {
            CadenceBound::Until { at } => landed >= at.as_naive(),
            _ => false,
        }
    }

    /// Whether a calendar step can produce nothing for a given index, which is
    /// the only reason an occurrence count can drift from a candidate index.
    fn can_skip_a_candidate(&self) -> bool {
        self.invalid_day != InvalidDay::Clamp
            && self.every.calendar_months().unwrap_or(0) > 0
    }

    // ---------------------------------------------------------------- generator

    /// The `index`th wall-clock instant this rule produces, before landing,
    /// counting the anchor as zero.
    ///
    /// This is the single source of truth for what a schedule means. The UTC
    /// derivation and the timezone-resolved scheduler both call it; if they
    /// disagreed, one of them would be lying about the same declaration.
    ///
    /// Calendar months are resolved from the anchor by multiplication, never by
    /// stepping one month at a time. Stepping would make January 31st clamp to
    /// February 28th and then carry the 28th forward forever; multiplying keeps
    /// every month measured against the anchor's own day.
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

    /// The `index`th civil instant this rule produces, landing applied.
    ///
    /// `None` means this index produces nothing — a short month under
    /// [`InvalidDay::Skip`], a retired rule, or a date outside the representable
    /// calendar. A caller that has to *act* on the difference wants
    /// [`Self::civil_at_or_reason`] instead.
    pub fn civil_at(&self, anchor: CivilDateTime, index: u64) -> Option<CivilDateTime> {
        self.civil_at_or_reason(anchor, index).ok()
    }

    /// The `index`th civil instant, or why there isn't one.
    ///
    /// The three failures are genuinely different and a scheduler must tell them
    /// apart: a skipped month means keep looking, a retired rule means stop, and
    /// an exhausted calendar means stop for a reason nobody chose. Collapsing
    /// them into `None` is what makes a schedule either loop forever on a short
    /// month or quietly stop on one.
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
                // The month exists; it just has no such day. Report which, so a
                // pausing scheduler can say what it stopped on.
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

    /// The calendar year and month the `index`th step lands in, ignoring the day.
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

    /// The lowest index that could reach `at`, for a caller walking candidates.
    pub fn index_floor_civil(&self, anchor: CivilDateTime, at: CivilDateTime) -> u64 {
        let months = self.every.calendar_months().unwrap_or(0);
        let fixed = self.every.fixed_milliseconds().unwrap_or(0);
        self.index_floor(anchor.as_naive(), at.as_naive(), months, fixed)
    }

    /// Whether this rule produces exactly this civil instant from this anchor.
    ///
    /// Used to check that a boundary handed back by a caller is one this
    /// schedule could have produced, rather than trusting it.
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
        // Start from a lower bound on the index rather than from zero, so a far
        // date on a fast rule is not a linear walk from the anchor.
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

    /// Whether the bound has already retired the rule by this index.
    ///
    /// A count is of occurrences *produced*, not of indices tried, and the two
    /// part company the moment a short month yields nothing under `Skip`. Twelve
    /// payments means twelve payments; if February silently spent one of them,
    /// the rule would end a month early and nobody would be able to see why.
    fn retired_by(&self, anchor: NaiveDateTime, index: u64) -> bool {
        let Some(limit) = self.count_limit() else {
            return false;
        };
        self.ordinal_of(anchor, index) >= limit
    }

    /// How many occurrences this rule produced strictly before `index`.
    ///
    /// Equal to `index` unless the step can skip, which is the only way the two
    /// diverge — so the walk below is never entered by the common rule, and when
    /// it is, it is bounded by the count the author asked for plus the months
    /// that yielded nothing.
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

    /// A cheap lower bound on the index that could reach `target`. Never
    /// overshoots, so a caller may always scan upward from it.
    fn index_floor(&self, anchor: NaiveDateTime, target: NaiveDateTime, months: u32, fixed: i64) -> u64 {
        if target <= anchor {
            return 0;
        }
        if months > 0 {
            // Months dominate, and a month is never shorter than 28 days, so
            // dividing the elapsed months by the step cannot overshoot.
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

    /// Roll forward whole days until the weekday is one the rule allows.
    ///
    /// An instant already on an allowed weekday does not move, and the time of
    /// day is preserved because whole days are added.
    fn land_naive(&self, at: NaiveDateTime) -> NaiveDateTime {
        let Some(allowed) = &self.land_on else {
            return at;
        };
        let mut out = at;
        // A non-empty set is matched within a week; the bound is a guard, not an
        // expectation.
        for _ in 0..7 {
            let weekday = CivilWeekday::from(out.weekday());
            if allowed.iter().any(|day| day == weekday) {
                return out;
            }
            out += Duration::days(1);
        }
        out
    }

    /// Whether `index` failed because the calendar ran out of representable
    /// range rather than because the month lacked the day.
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

    /// Build a date under the invalid-day policy. `Skip` and `Pause` both
    /// decline to invent a date; they differ only in what the *caller* does
    /// about it, and only a caller that executes something has a choice to make.
    fn month_day(&self, year: i32, month: u32, day: u32) -> Option<NaiveDate> {
        let last = last_day_of_month(year, month)?;
        match self.invalid_day {
            InvalidDay::Clamp => NaiveDate::from_ymd_opt(year, month, day.min(last)),
            InvalidDay::Skip | InvalidDay::Pause => NaiveDate::from_ymd_opt(year, month, day),
        }
    }

    // ------------------------------------------------------------- UTC read path

    /// Every instant this rule produces within `[from, to)`, resolved in UTC.
    ///
    /// Half-open on purpose, matching the Ledger's windows: adjacent periods
    /// tile without an occurrence being claimed by both.
    ///
    /// `anchor` sets the phase, the time of day, and — for the calendar half —
    /// the day of month. Instants before the anchor are never produced: a rule
    /// does not apply retroactively to before it was declared.
    ///
    /// This is the zone-free resolution, where wall-clock and instant coincide.
    /// A schedule that drives execution resolves the same candidates through a
    /// real timezone instead; see [`crate::karma::calendar::CalendarSchedule`].
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

        // Landing only ever moves an instant *forward*, by less than a week. So
        // a base instant just before the window can still land inside it, and
        // the scan has to start earlier than the window does. Filtering happens
        // on the landed value, never on the base.
        let slack = if self.land_on.is_some() {
            Duration::days(7)
        } else {
            Duration::zero()
        };
        let scan_from = from - slack;

        let months = self.every.calendar_months().unwrap_or(0);
        let fixed = self.every.fixed_milliseconds().unwrap_or(0);
        let anchor_naive = anchor.naive_utc();

        // Phase is measured from the anchor, never from `from`. A fortnightly
        // rule must stay on *its* fortnight, so a window that opens mid-period
        // starts at a whole multiple rather than at the window's edge.
        //
        // A counted rule with a skippable calendar step is the exception: there
        // the occurrence number and the candidate index part company, so the
        // count has to be taken from the beginning.
        let mut index: u64 = if self.count_limit().is_some() && self.can_skip_a_candidate() {
            0
        } else {
            self.index_floor(anchor_naive, scan_from.naive_utc(), months, fixed)
        };

        // A BTreeSet does the ordering and the deduplication in one place.
        // Both are load-bearing: landing can collapse several base instants onto
        // the same weekday, and two occurrences on one instant would share an
        // idempotency key, so the second would silently replay as an
        // already-applied change rather than appearing as its own date.
        let mut found: std::collections::BTreeSet<DateTime<Utc>> = Default::default();

        // Counted separately from `index`, which the floor above can start in
        // the billions. Confusing the two would report every fast rule as
        // truncated.
        let mut steps = 0usize;
        let mut reached_end = false;

        while steps < MAX_SCAN_STEPS {
            steps += 1;
            if self.retired_by(anchor_naive, index) {
                reached_end = true;
                break;
            }
            let Some(base) = self.naive_at(anchor_naive, index, months, fixed) else {
                // Either the calendar ran out of range or this month has no such
                // day under `Skip`. Range exhaustion ends the scan; a skipped
                // month must not.
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
                // Landing only moves forward, so nothing from here on can land
                // back inside the window.
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
                // Collect one past the cap so "there are more" is a fact rather
                // than a guess.
                if found.len() > MAX_DERIVED_OCCURRENCES {
                    derived.truncated = true;
                    found.pop_last();
                    break;
                }
            }
        }

        // Exhausting the scan budget without reaching `to` also means the answer
        // is a prefix — the common case for a sub-second step over a wide
        // window, where the honest report is "more than these".
        if !reached_end && steps >= MAX_SCAN_STEPS {
            derived.truncated = true;
        }

        derived.dates = found.into_iter().collect();
        Ok(derived)
    }

    /// The first instant at or after `after`, if the rule ever reaches one.
    pub fn next_on_or_after(
        &self,
        anchor: DateTime<Utc>,
        after: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, CadenceError> {
        self.validate()?;
        // One step of *this* rule has to fit, and a yearly rule needs far more
        // room than a daily one. Two spans plus a week covers the step itself
        // plus any landing roll.
        let span = self
            .every
            .approximate_span_ms()
            .saturating_mul(2)
            .saturating_add(8 * MS_PER_DAY)
            .max(400 * MS_PER_DAY);
        let horizon = after
            .checked_add_signed(Duration::milliseconds(span))
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
        Ok(self.between(anchor, after, horizon)?.dates.into_iter().next())
    }
}

/// The instants a rule produces in a window, and whether the answer is whole.
///
/// `truncated` exists so a surface can say "and more" instead of presenting a
/// prefix as the complete set. A millisecond rule over a year is the case that
/// forces it: the honest answer is always a prefix.
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

/// Why a given index produced no occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoOccurrence {
    /// The step landed in a month with no such day, and the policy declined to
    /// invent one. Keep looking — the next month may well have it.
    InvalidMonthDay { year: i32, month: u32, day: u32 },
    /// The bound has been reached. Stop; this is the rule ending as authored.
    Retired,
    /// The calendar ran out of representable range. Stop; nobody chose this.
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
