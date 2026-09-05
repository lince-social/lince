use std::{cmp::Ordering, num::NonZeroU32};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::{DurationMs, KarmaBoundaryError, TimestampMs};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RationalRate {
    numerator: u64,
    denominator: u64,
}

impl RationalRate {
    pub fn new(numerator: u64, denominator: u64) -> Result<Self, KarmaBoundaryError> {
        if denominator == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "rational denominator must be non-zero",
            ));
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    pub const fn numerator(self) -> u64 {
        self.numerator
    }

    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let divisor = gcd(self.denominator, rhs.denominator);
        let left_multiplier = rhs.denominator / divisor;
        let right_multiplier = self.denominator / divisor;
        let numerator = self
            .numerator
            .checked_mul(left_multiplier)?
            .checked_add(rhs.numerator.checked_mul(right_multiplier)?)?;
        let denominator = self.denominator.checked_mul(left_multiplier)?;
        Self::new(numerator, denominator).ok()
    }

    pub fn checked_mul_u64(self, multiplier: u64) -> Option<Self> {
        let divisor = gcd(multiplier, self.denominator);
        let reduced_multiplier = multiplier / divisor;
        let denominator = self.denominator / divisor;
        Self::new(self.numerator.checked_mul(reduced_multiplier)?, denominator).ok()
    }
}

impl PartialOrd for RationalRate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RationalRate {
    fn cmp(&self, other: &Self) -> Ordering {
        (u128::from(self.numerator) * u128::from(other.denominator))
            .cmp(&(u128::from(other.numerator) * u128::from(self.denominator)))
    }
}

#[derive(Serialize, Deserialize)]
struct RationalRateWire {
    numerator: u64,
    denominator: u64,
}

impl Serialize for RationalRate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RationalRateWire {
            numerator: self.numerator,
            denominator: self.denominator,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RationalRate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RationalRateWire::deserialize(deserializer)?;
        Self::new(wire.numerator, wire.denominator).map_err(de::Error::custom)
    }
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    if left == 0 { 1 } else { left }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerPolicy {
    required_resolution_ms: u32,
    max_lateness_ms: u32,
    coalesce_window_ms: u32,
}

impl TimerPolicy {
    pub fn new(
        required_resolution_ms: u32,
        max_lateness_ms: u32,
        coalesce_window_ms: u32,
    ) -> Result<Self, KarmaBoundaryError> {
        if required_resolution_ms == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "required timer resolution must be at least 1ms",
            ));
        }
        if coalesce_window_ms > max_lateness_ms {
            return Err(KarmaBoundaryError::invalid_input(
                "timer coalescing window cannot exceed maximum lateness",
            ));
        }
        Ok(Self {
            required_resolution_ms,
            max_lateness_ms,
            coalesce_window_ms,
        })
    }

    pub const fn required_resolution_ms(self) -> u32 {
        self.required_resolution_ms
    }

    pub const fn max_lateness_ms(self) -> u32 {
        self.max_lateness_ms
    }

    pub const fn coalesce_window_ms(self) -> u32 {
        self.coalesce_window_ms
    }
}

#[derive(Serialize, Deserialize)]
struct TimerPolicyWire {
    required_resolution_ms: u32,
    max_lateness_ms: u32,
    coalesce_window_ms: u32,
}

impl Serialize for TimerPolicy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TimerPolicyWire {
            required_resolution_ms: self.required_resolution_ms,
            max_lateness_ms: self.max_lateness_ms,
            coalesce_window_ms: self.coalesce_window_ms,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TimerPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TimerPolicyWire::deserialize(deserializer)?;
        Self::new(
            wire.required_resolution_ms,
            wire.max_lateness_ms,
            wire.coalesce_window_ms,
        )
        .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MissedPolicy {
    Skip,
    Coalesce,
    Replay { max: NonZeroU32 },
    PauseOnLag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InactiveGapPolicy {
    SkipToNextAnchor,
    ReplayByMissedPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RephasePolicy {
    PreserveAnchor,
    FromLastIntended,
    FromChange,
    ImmediateIfOverdue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverloadPolicy {
    PauseAndAsk,
    RejectActivation,
    DegradeWithinGrant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElapsedSchedule {
    anchor: TimestampMs,
    interval_ms: u64,
    timer: TimerPolicy,
    missed: MissedPolicy,
    inactive_gap: InactiveGapPolicy,
    rephase: RephasePolicy,
    overload: OverloadPolicy,
}

impl ElapsedSchedule {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        anchor: TimestampMs,
        interval_ms: u64,
        timer: TimerPolicy,
        missed: MissedPolicy,
        inactive_gap: InactiveGapPolicy,
        rephase: RephasePolicy,
        overload: OverloadPolicy,
    ) -> Result<Self, KarmaBoundaryError> {
        if interval_ms == 0 || interval_ms > i64::MAX as u64 {
            return Err(KarmaBoundaryError::invalid_input(
                "elapsed schedule interval must be within 1..=i64::MAX milliseconds",
            ));
        }
        Ok(Self {
            anchor,
            interval_ms,
            timer,
            missed,
            inactive_gap,
            rephase,
            overload,
        })
    }

    pub const fn anchor(&self) -> TimestampMs {
        self.anchor
    }

    pub const fn interval_ms(&self) -> u64 {
        self.interval_ms
    }

    pub const fn timer(&self) -> TimerPolicy {
        self.timer
    }

    pub const fn missed_policy(&self) -> MissedPolicy {
        self.missed
    }

    pub const fn inactive_gap_policy(&self) -> InactiveGapPolicy {
        self.inactive_gap
    }

    pub const fn rephase_policy(&self) -> RephasePolicy {
        self.rephase
    }

    pub const fn overload_policy(&self) -> OverloadPolicy {
        self.overload
    }

    pub fn semantic_rate(&self) -> RationalRate {
        RationalRate::new(1_000, self.interval_ms).expect("validated interval is non-zero")
    }

    pub fn initial_cursor(
        &self,
        activated_at: TimestampMs,
    ) -> Result<ScheduleCursor, KarmaBoundaryError> {
        let next = self.boundary_strictly_after(activated_at)?;
        ScheduleCursor::new(self, None, next)
    }

    pub fn cursor(
        &self,
        last_intended_at: Option<TimestampMs>,
        next_intended_at: TimestampMs,
    ) -> Result<ScheduleCursor, KarmaBoundaryError> {
        ScheduleCursor::new(self, last_intended_at, next_intended_at)
    }

    pub fn arm_window(&self, intended_at: TimestampMs) -> Result<ArmWindow, KarmaBoundaryError> {
        if !self.is_boundary(intended_at) {
            return Err(KarmaBoundaryError::invalid_input(
                "timer arm must name an elapsed schedule boundary",
            ));
        }
        let latest = intended_at
            .checked_add(DurationMs::new(i64::from(self.timer.coalesce_window_ms)))
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_input("timer arm window overflows timestamp")
            })?;
        Ok(ArmWindow {
            earliest: intended_at,
            latest,
        })
    }

    pub fn advance(
        &self,
        cursor: &ScheduleCursor,
        observed_at: TimestampMs,
    ) -> Result<Option<ScheduleAdvance>, KarmaBoundaryError> {
        cursor.validate_for(self)?;
        if observed_at < cursor.next_intended_at {
            return Ok(None);
        }
        let elapsed = observed_at.as_millis() - cursor.next_intended_at.as_millis();
        let count = u64::try_from(elapsed).expect("non-negative timestamp difference fits u64")
            / self.interval_ms
            + 1;
        let due = OccurrenceRange::new(cursor.next_intended_at, self.interval_ms, count)?;
        let late_count = late_count(&due, observed_at, self.timer.max_lateness_ms);
        let maximum_lateness = DurationMs::new(
            observed_at
                .as_millis()
                .checked_sub(due.first().as_millis())
                .expect("observed due timestamp cannot precede first boundary"),
        );

        if self.missed == MissedPolicy::PauseOnLag && late_count > 0 {
            return Ok(Some(ScheduleAdvance {
                observed_at,
                due,
                emission: None,
                skipped: None,
                replay_overflow: 0,
                late_count,
                maximum_lateness,
                cursor: cursor.clone(),
                paused: true,
            }));
        }

        let (emission, skipped, replay_overflow) = match self.missed {
            MissedPolicy::Skip => {
                if late_count < count {
                    let emitted = due.suffix(count - 1)?.expect("due range is non-empty");
                    (
                        Some(ScheduleEmission::Individual(emitted)),
                        due.prefix(count - 1)?,
                        0,
                    )
                } else {
                    (None, Some(due.clone()), 0)
                }
            }
            MissedPolicy::Coalesce => (Some(ScheduleEmission::Coalesced(due.clone())), None, 0),
            MissedPolicy::Replay { max } => {
                let emitted_count = count.min(u64::from(max.get()));
                let emitted = due.prefix(emitted_count)?.expect("replay max is non-zero");
                let overflow = count - emitted_count;
                (
                    Some(ScheduleEmission::Individual(emitted)),
                    due.suffix(emitted_count)?,
                    overflow,
                )
            }
            MissedPolicy::PauseOnLag => (Some(ScheduleEmission::Individual(due.clone())), None, 0),
        };
        let last = due.last()?;
        let next = last
            .checked_add(DurationMs::new(self.interval_i64()))
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_input("next schedule boundary overflows timestamp")
            })?;
        let next_cursor = ScheduleCursor::new(self, Some(last), next)?;
        Ok(Some(ScheduleAdvance {
            observed_at,
            due,
            emission,
            skipped,
            replay_overflow,
            late_count,
            maximum_lateness,
            cursor: next_cursor,
            paused: false,
        }))
    }

    pub fn reactivate(
        &self,
        cursor: &ScheduleCursor,
        activated_at: TimestampMs,
    ) -> Result<ScheduleReactivation, KarmaBoundaryError> {
        cursor.validate_for(self)?;
        if self.inactive_gap == InactiveGapPolicy::ReplayByMissedPolicy
            || cursor.next_intended_at >= activated_at
        {
            return Ok(ScheduleReactivation {
                cursor: cursor.clone(),
                inactive_skipped: None,
            });
        }
        let next = self.boundary_strictly_after(activated_at)?;
        let skipped_count = u64::try_from(
            (next.as_millis() - cursor.next_intended_at.as_millis()) / self.interval_i64(),
        )
        .expect("boundary distance is non-negative");
        let inactive_skipped = if skipped_count == 0 {
            None
        } else {
            Some(OccurrenceRange::new(
                cursor.next_intended_at,
                self.interval_ms,
                skipped_count,
            )?)
        };
        let next_cursor = ScheduleCursor::new(self, cursor.last_intended_at, next)?;
        Ok(ScheduleReactivation {
            cursor: next_cursor,
            inactive_skipped,
        })
    }

    pub fn rephase(
        &self,
        cursor: &ScheduleCursor,
        changed_at: TimestampMs,
        new_interval_ms: u64,
        policy: RephasePolicy,
    ) -> Result<RephasedElapsed, KarmaBoundaryError> {
        cursor.validate_for(self)?;
        let (anchor, retain_last, immediate) = match policy {
            RephasePolicy::PreserveAnchor => (self.anchor, true, None),
            RephasePolicy::FromLastIntended => match cursor.last_intended_at {
                Some(last) => (last, true, None),
                None => (changed_at, false, None),
            },
            RephasePolicy::FromChange => (changed_at, false, None),
            RephasePolicy::ImmediateIfOverdue if cursor.next_intended_at <= changed_at => (
                cursor.next_intended_at,
                false,
                Some(cursor.next_intended_at),
            ),
            RephasePolicy::ImmediateIfOverdue => (self.anchor, true, None),
        };
        let schedule = Self::new(
            anchor,
            new_interval_ms,
            self.timer,
            self.missed,
            self.inactive_gap,
            policy,
            self.overload,
        )?;
        let next = match immediate {
            Some(next) => next,
            None if policy == RephasePolicy::FromChange => anchor
                .checked_add(DurationMs::new(schedule.interval_i64()))
                .ok_or_else(|| {
                    KarmaBoundaryError::invalid_input("rephased boundary overflows timestamp")
                })?,
            None => schedule.boundary_strictly_after(changed_at)?,
        };
        let last = if retain_last {
            cursor
                .last_intended_at
                .filter(|last| *last < next && schedule.is_boundary(*last))
        } else if policy == RephasePolicy::FromLastIntended {
            cursor.last_intended_at
        } else {
            None
        };
        let next_cursor = ScheduleCursor::new(&schedule, last, next)?;
        Ok(RephasedElapsed {
            schedule,
            cursor: next_cursor,
        })
    }

    fn boundary_strictly_after(
        &self,
        timestamp: TimestampMs,
    ) -> Result<TimestampMs, KarmaBoundaryError> {
        if timestamp < self.anchor {
            return Ok(self.anchor);
        }
        let delta = u64::try_from(timestamp.as_millis() - self.anchor.as_millis())
            .expect("ordered timestamp difference is non-negative");
        let index = (delta / self.interval_ms)
            .checked_add(1)
            .ok_or_else(|| KarmaBoundaryError::invalid_input("schedule index overflows"))?;
        self.boundary_at_index(index)
    }

    fn boundary_at_index(&self, index: u64) -> Result<TimestampMs, KarmaBoundaryError> {
        let offset = self
            .interval_ms
            .checked_mul(index)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| KarmaBoundaryError::invalid_input("schedule offset overflows"))?;
        self.anchor
            .checked_add(DurationMs::new(offset))
            .ok_or_else(|| KarmaBoundaryError::invalid_input("schedule boundary overflows"))
    }

    fn is_boundary(&self, timestamp: TimestampMs) -> bool {
        if timestamp < self.anchor {
            return false;
        }
        let delta = timestamp.as_millis() - self.anchor.as_millis();
        delta % self.interval_i64() == 0
    }

    fn interval_i64(&self) -> i64 {
        i64::try_from(self.interval_ms).expect("validated interval fits i64")
    }
}

#[derive(Serialize, Deserialize)]
struct ElapsedScheduleWire {
    anchor: TimestampMs,
    interval_ms: u64,
    timer: TimerPolicy,
    missed: MissedPolicy,
    inactive_gap: InactiveGapPolicy,
    rephase: RephasePolicy,
    overload: OverloadPolicy,
}

impl Serialize for ElapsedSchedule {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ElapsedScheduleWire {
            anchor: self.anchor,
            interval_ms: self.interval_ms,
            timer: self.timer,
            missed: self.missed,
            inactive_gap: self.inactive_gap,
            rephase: self.rephase,
            overload: self.overload,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ElapsedSchedule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ElapsedScheduleWire::deserialize(deserializer)?;
        Self::new(
            wire.anchor,
            wire.interval_ms,
            wire.timer,
            wire.missed,
            wire.inactive_gap,
            wire.rephase,
            wire.overload,
        )
        .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScheduleCursor {
    #[serde(skip_serializing_if = "Option::is_none")]
    last_intended_at: Option<TimestampMs>,
    next_intended_at: TimestampMs,
}

impl ScheduleCursor {
    fn new(
        schedule: &ElapsedSchedule,
        last_intended_at: Option<TimestampMs>,
        next_intended_at: TimestampMs,
    ) -> Result<Self, KarmaBoundaryError> {
        let cursor = Self {
            last_intended_at,
            next_intended_at,
        };
        cursor.validate_for(schedule)?;
        Ok(cursor)
    }

    pub const fn last_intended_at(&self) -> Option<TimestampMs> {
        self.last_intended_at
    }

    pub const fn next_intended_at(&self) -> TimestampMs {
        self.next_intended_at
    }

    fn validate_for(&self, schedule: &ElapsedSchedule) -> Result<(), KarmaBoundaryError> {
        if !schedule.is_boundary(self.next_intended_at) {
            return Err(KarmaBoundaryError::invalid_input(
                "next intended time is not on the elapsed schedule lattice",
            ));
        }
        if let Some(last) = self.last_intended_at
            && (!schedule.is_boundary(last) || last >= self.next_intended_at)
        {
            return Err(KarmaBoundaryError::invalid_input(
                "last intended time must be an earlier boundary on the same schedule lattice",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct ScheduleCursorWire {
    last_intended_at: Option<TimestampMs>,
    next_intended_at: TimestampMs,
}

impl<'de> Deserialize<'de> for ScheduleCursor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ScheduleCursorWire::deserialize(deserializer)?;
        if wire
            .last_intended_at
            .is_some_and(|last| last >= wire.next_intended_at)
        {
            return Err(de::Error::custom(
                "last intended time must precede next intended time",
            ));
        }
        Ok(Self {
            last_intended_at: wire.last_intended_at,
            next_intended_at: wire.next_intended_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OccurrenceRange {
    first: TimestampMs,
    interval_ms: u64,
    count: u64,
}

impl OccurrenceRange {
    pub fn new(
        first: TimestampMs,
        interval_ms: u64,
        count: u64,
    ) -> Result<Self, KarmaBoundaryError> {
        if interval_ms == 0 || interval_ms > i64::MAX as u64 || count == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "occurrence range needs a positive count and i64-sized positive interval",
            ));
        }
        let range = Self {
            first,
            interval_ms,
            count,
        };
        range.last()?;
        Ok(range)
    }

    pub const fn first(&self) -> TimestampMs {
        self.first
    }

    pub const fn interval_ms(&self) -> u64 {
        self.interval_ms
    }

    pub const fn count(&self) -> u64 {
        self.count
    }

    pub fn last(&self) -> Result<TimestampMs, KarmaBoundaryError> {
        let offset = self
            .count
            .checked_sub(1)
            .and_then(|index| self.interval_ms.checked_mul(index))
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| KarmaBoundaryError::invalid_input("occurrence range overflows"))?;
        self.first
            .checked_add(DurationMs::new(offset))
            .ok_or_else(|| KarmaBoundaryError::invalid_input("occurrence range overflows"))
    }

    fn prefix(&self, count: u64) -> Result<Option<Self>, KarmaBoundaryError> {
        if count == 0 {
            return Ok(None);
        }
        if count > self.count {
            return Err(KarmaBoundaryError::invalid_input(
                "occurrence prefix exceeds range",
            ));
        }
        Self::new(self.first, self.interval_ms, count).map(Some)
    }

    fn suffix(&self, skip: u64) -> Result<Option<Self>, KarmaBoundaryError> {
        if skip >= self.count {
            return Ok(None);
        }
        let offset = self
            .interval_ms
            .checked_mul(skip)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| KarmaBoundaryError::invalid_input("occurrence suffix overflows"))?;
        let first = self
            .first
            .checked_add(DurationMs::new(offset))
            .ok_or_else(|| KarmaBoundaryError::invalid_input("occurrence suffix overflows"))?;
        Self::new(first, self.interval_ms, self.count - skip).map(Some)
    }
}

#[derive(Deserialize)]
struct OccurrenceRangeWire {
    first: TimestampMs,
    interval_ms: u64,
    count: u64,
}

impl<'de> Deserialize<'de> for OccurrenceRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = OccurrenceRangeWire::deserialize(deserializer)?;
        Self::new(wire.first, wire.interval_ms, wire.count).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "range", rename_all = "kebab-case")]
pub enum ScheduleEmission {
    Individual(OccurrenceRange),
    Coalesced(OccurrenceRange),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleAdvance {
    pub observed_at: TimestampMs,
    pub due: OccurrenceRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emission: Option<ScheduleEmission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<OccurrenceRange>,
    pub replay_overflow: u64,
    pub late_count: u64,
    pub maximum_lateness: DurationMs,
    pub cursor: ScheduleCursor,
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleReactivation {
    pub cursor: ScheduleCursor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inactive_skipped: Option<OccurrenceRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RephasedElapsed {
    pub schedule: ElapsedSchedule,
    pub cursor: ScheduleCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmWindow {
    pub earliest: TimestampMs,
    pub latest: TimestampMs,
}

fn late_count(range: &OccurrenceRange, observed_at: TimestampMs, max_lateness_ms: u32) -> u64 {
    let threshold = match observed_at
        .as_millis()
        .checked_sub(i64::from(max_lateness_ms))
    {
        Some(value) => value,
        None => return 0,
    };
    if range.first.as_millis() >= threshold {
        return 0;
    }
    let distance = u64::try_from(threshold - range.first.as_millis())
        .expect("ordered timestamp distance is non-negative");
    let before_threshold = distance.saturating_add(range.interval_ms - 1) / range.interval_ms;
    before_threshold.min(range.count)
}
