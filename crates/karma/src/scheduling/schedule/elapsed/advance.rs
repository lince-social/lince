//! Due-window emission, gap reactivation, and phase changes.

use serde::{Deserialize, Serialize};

use super::{ElapsedSchedule, InactiveGapPolicy, MissedPolicy, RephasePolicy, ScheduleCursor};
use crate::{DurationMs, KarmaBoundaryError, OccurrenceRange, TimestampMs};

impl ElapsedSchedule {
    /// Advances through the boundary at or immediately before the observation.
    ///
    /// A lag pause keeps the input cursor so the due window can be retried.
    ///
    /// # Errors
    ///
    /// Rejects a cursor off the schedule lattice or an unrepresentable next boundary.
    pub fn advance(
        &self,
        cursor: &ScheduleCursor,
        observed_at: TimestampMs,
    ) -> Result<Option<ScheduleAdvance>, KarmaBoundaryError> {
        cursor.validate_for(self)?;
        let Some((due, elapsed)) =
            OccurrenceRange::through(cursor.next_intended_at, self.interval_ms, observed_at)
        else {
            return Ok(None);
        };
        let count = due.count();
        let late_count = elapsed
            .saturating_sub(u64::from(self.timer.max_lateness_ms()))
            .div_ceil(self.interval_ms());

        let paused = self.missed == MissedPolicy::PauseOnLag && late_count > 0;
        let (emission, skipped, replay_overflow) = match self.missed {
            MissedPolicy::Skip if late_count < count => {
                let (skipped, latest) = due.split_at(count - 1)?;
                (latest.map(ScheduleEmission::Individual), skipped, 0)
            }
            MissedPolicy::Skip => (None, Some(due.clone()), 0),
            MissedPolicy::Coalesce => (Some(ScheduleEmission::Coalesced(due.clone())), None, 0),
            MissedPolicy::Replay { max } => {
                let emitted_count = count.min(u64::from(max.get()));
                let (emitted, skipped) = due.split_at(emitted_count)?;
                (
                    emitted.map(ScheduleEmission::Individual),
                    skipped,
                    count - emitted_count,
                )
            }
            MissedPolicy::PauseOnLag if paused => (None, None, 0),
            MissedPolicy::PauseOnLag => (Some(ScheduleEmission::Individual(due.clone())), None, 0),
        };
        let cursor = if paused {
            cursor.clone()
        } else {
            let last = due.last()?;
            let next = last
                .checked_add(DurationMs::new(self.interval_i64()))
                .ok_or_else(|| {
                    KarmaBoundaryError::invalid_input("next schedule boundary overflows timestamp")
                })?;
            ScheduleCursor {
                last_intended_at: Some(last),
                next_intended_at: next,
            }
        };
        Ok(Some(ScheduleAdvance {
            observed_at,
            due,
            emission,
            skipped,
            replay_overflow,
            late_count,
            maximum_lateness: DurationMs::new(elapsed.cast_signed()),
            cursor,
            paused,
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
        let skipped_count = next
            .as_millis()
            .abs_diff(cursor.next_intended_at.as_millis())
            / self.interval_ms();
        let inactive_skipped = Some(OccurrenceRange::new(
            cursor.next_intended_at,
            self.interval_ms(),
            skipped_count,
        )?);
        let next_cursor = ScheduleCursor {
            last_intended_at: cursor.last_intended_at,
            next_intended_at: next,
        };
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
        let last = cursor.last_intended_at;
        let (anchor, last, immediate) = match policy {
            RephasePolicy::FromLastIntended => (last.unwrap_or(changed_at), last, None),
            RephasePolicy::FromChange => (changed_at, None, None),
            RephasePolicy::ImmediateIfOverdue if cursor.next_intended_at <= changed_at => {
                (cursor.next_intended_at, None, Some(cursor.next_intended_at))
            }
            RephasePolicy::PreserveAnchor | RephasePolicy::ImmediateIfOverdue => {
                (self.anchor, last, None)
            }
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
        let last = last.filter(|&last| last < next && schedule.is_boundary(last));
        let next_cursor = schedule.cursor(last, next)?;
        Ok(RephasedElapsed {
            schedule,
            cursor: next_cursor,
        })
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
