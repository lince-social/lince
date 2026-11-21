//! Elapsed schedule lattice, timer windows, and validated cursor state.

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use super::{
    InactiveGapPolicy, MissedPolicy, OverloadPolicy, RationalRate, RephasePolicy, TimerPolicy,
};
use crate::{DurationMs, KarmaBoundaryError, TimestampMs};

mod advance;

pub use advance::{RephasedElapsed, ScheduleAdvance, ScheduleEmission, ScheduleReactivation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename(serialize = "ElapsedScheduleWire"),
    try_from = "ElapsedScheduleWire"
)]
pub struct ElapsedSchedule {
    anchor: TimestampMs,
    interval_ms: NonZeroU64,
    timer: TimerPolicy,
    missed: MissedPolicy,
    inactive_gap: InactiveGapPolicy,
    rephase: RephasePolicy,
    overload: OverloadPolicy,
}

impl ElapsedSchedule {
    pub fn new(
        anchor: TimestampMs,
        interval_ms: u64,
        timer: TimerPolicy,
        missed: MissedPolicy,
        inactive_gap: InactiveGapPolicy,
        rephase: RephasePolicy,
        overload: OverloadPolicy,
    ) -> Result<Self, KarmaBoundaryError> {
        let interval_ms = NonZeroU64::new(interval_ms)
            .filter(|interval| interval.get() <= i64::MAX as u64)
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_input(
                    "elapsed schedule interval must be within 1..=i64::MAX milliseconds",
                )
            })?;
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
        self.interval_ms.get()
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
        RationalRate::with_denominator(1_000, self.interval_ms)
    }

    pub fn initial_cursor(
        &self,
        activated_at: TimestampMs,
    ) -> Result<ScheduleCursor, KarmaBoundaryError> {
        Ok(ScheduleCursor {
            last_intended_at: None,
            next_intended_at: self.boundary_strictly_after(activated_at)?,
        })
    }

    pub fn cursor(
        &self,
        last_intended_at: Option<TimestampMs>,
        next_intended_at: TimestampMs,
    ) -> Result<ScheduleCursor, KarmaBoundaryError> {
        let cursor = ScheduleCursor {
            last_intended_at,
            next_intended_at,
        };
        cursor.validate_for(self)?;
        Ok(cursor)
    }

    pub fn arm_window(&self, intended_at: TimestampMs) -> Result<ArmWindow, KarmaBoundaryError> {
        if !self.is_boundary(intended_at) {
            return Err(KarmaBoundaryError::invalid_input(
                "timer arm must name an elapsed schedule boundary",
            ));
        }
        let latest = intended_at
            .checked_add(DurationMs::new(i64::from(self.timer.coalesce_window_ms())))
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_input("timer arm window overflows timestamp")
            })?;
        Ok(ArmWindow {
            earliest: intended_at,
            latest,
        })
    }

    fn boundary_strictly_after(
        &self,
        timestamp: TimestampMs,
    ) -> Result<TimestampMs, KarmaBoundaryError> {
        if timestamp < self.anchor {
            return Ok(self.anchor);
        }
        let delta = timestamp.as_millis().abs_diff(self.anchor.as_millis());
        let index = delta / self.interval_ms() + 1;
        let offset = self
            .interval_ms()
            .checked_mul(index)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| KarmaBoundaryError::invalid_input("schedule offset overflows"))?;
        self.anchor
            .checked_add(DurationMs::new(offset))
            .ok_or_else(|| KarmaBoundaryError::invalid_input("schedule boundary overflows"))
    }

    fn is_boundary(&self, timestamp: TimestampMs) -> bool {
        timestamp >= self.anchor
            && timestamp
                .as_millis()
                .abs_diff(self.anchor.as_millis())
                .is_multiple_of(self.interval_ms())
    }

    fn interval_i64(&self) -> i64 {
        // Construction caps the interval at i64::MAX.
        self.interval_ms().cast_signed()
    }
}

#[derive(Deserialize)]
struct ElapsedScheduleWire {
    anchor: TimestampMs,
    interval_ms: u64,
    timer: TimerPolicy,
    missed: MissedPolicy,
    inactive_gap: InactiveGapPolicy,
    rephase: RephasePolicy,
    overload: OverloadPolicy,
}

impl TryFrom<ElapsedScheduleWire> for ElapsedSchedule {
    type Error = KarmaBoundaryError;

    fn try_from(wire: ElapsedScheduleWire) -> Result<Self, Self::Error> {
        Self::new(
            wire.anchor,
            wire.interval_ms,
            wire.timer,
            wire.missed,
            wire.inactive_gap,
            wire.rephase,
            wire.overload,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScheduleCursorWire")]
pub struct ScheduleCursor {
    #[serde(skip_serializing_if = "Option::is_none")]
    last_intended_at: Option<TimestampMs>,
    next_intended_at: TimestampMs,
}

impl ScheduleCursor {
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

impl TryFrom<ScheduleCursorWire> for ScheduleCursor {
    type Error = &'static str;

    fn try_from(wire: ScheduleCursorWire) -> Result<Self, Self::Error> {
        if wire
            .last_intended_at
            .is_some_and(|last| last >= wire.next_intended_at)
        {
            return Err("last intended time must precede next intended time");
        }
        Ok(Self {
            last_intended_at: wire.last_intended_at,
            next_intended_at: wire.next_intended_at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmWindow {
    pub earliest: TimestampMs,
    pub latest: TimestampMs,
}
