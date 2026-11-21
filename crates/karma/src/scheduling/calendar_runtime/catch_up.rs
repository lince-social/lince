//! Ordered calendar catch-up and missed-boundary policy.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use super::{CalendarCursor, CalendarCursorResolution, resolve_calendar_cursor};
use crate::{
    CalendarBoundary, CalendarDiscontinuity, CalendarSchedule, DurationMs, KarmaBoundaryError,
    MissedPolicy, TimeZoneProvider, TimestampMs,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "boundaries", rename_all = "kebab-case")]
pub enum CalendarEmission {
    Individual(Vec<CalendarBoundary>),
    Coalesced(Vec<CalendarBoundary>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CalendarCatchUpPause {
    Lag,
    Discontinuity { reason: CalendarDiscontinuity },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarCatchUp {
    pub observed_at: TimestampMs,
    pub due: Vec<CalendarBoundary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emission: Option<CalendarEmission>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_due: Vec<CalendarBoundary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discontinuities: Vec<CalendarDiscontinuity>,
    pub replay_overflow: u64,
    pub late_count: u64,
    pub maximum_lateness: DurationMs,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<CalendarCursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause: Option<CalendarCatchUpPause>,
}

/// Collects due boundaries, their emission, and the next cursor or pause.
///
/// # Errors
///
/// Rejects an inconsistent cursor, a provider failure, or a catch-up beyond the budget.
pub fn advance_calendar_cursor(
    schedule: &CalendarSchedule,
    provider: &dyn TimeZoneProvider,
    cursor: &CalendarCursor,
    observed_at: TimestampMs,
    catch_up_budget: NonZeroU32,
) -> Result<Option<CalendarCatchUp>, KarmaBoundaryError> {
    let verified = cursor.verified(schedule, provider)?;
    let first = verified.next();
    if observed_at < first.intended_at {
        return Ok(None);
    }

    let mut due = Vec::new();
    let mut discontinuities = verified.skipped_before_next;
    let mut current = first;
    let (mut next_cursor, mut pause) = loop {
        due.push(current);
        match resolve_calendar_cursor(schedule, provider, Some(current))? {
            CalendarCursorResolution::Armed(next) => {
                current = next.next();
                if current.intended_at > observed_at {
                    discontinuities.extend_from_slice(next.skipped_before_next());
                    break (Some(next), None);
                }
                if due.len() >= catch_up_budget.get() as usize {
                    return Err(KarmaBoundaryError::invalid_input(
                        "calendar catch-up exceeded the explicit boundary budget",
                    ));
                }
                discontinuities.extend(next.skipped_before_next);
            }
            CalendarCursorResolution::Paused {
                reason, skipped, ..
            } => {
                discontinuities.extend(skipped);
                break (None, Some(CalendarCatchUpPause::Discontinuity { reason }));
            }
            CalendarCursorResolution::Retired { skipped, .. } => {
                discontinuities.extend(skipped);
                break (None, None);
            }
        }
    };

    // Due instants increase strictly, and canonical timestamp differences fit i64.
    let observed = observed_at.as_millis();
    let late_before = observed - i64::from(schedule.timer.max_lateness_ms());
    let late_count = due.partition_point(|boundary| boundary.intended_at.as_millis() < late_before);
    let (emission, skipped_due, replay_overflow) = match schedule.missed {
        MissedPolicy::Skip => match due.split_last() {
            Some((&last, earlier)) if late_count < due.len() => (
                Some(CalendarEmission::Individual(vec![last])),
                earlier.to_vec(),
                0,
            ),
            _ => (None, due.clone(), 0),
        },
        MissedPolicy::Coalesce => (
            Some(CalendarEmission::Coalesced(due.clone())),
            Vec::new(),
            0,
        ),
        MissedPolicy::Replay { max } => {
            let (emitted, skipped) = due.split_at(due.len().min(max.get() as usize));
            (
                Some(CalendarEmission::Individual(emitted.to_vec())),
                skipped.to_vec(),
                skipped.len() as u64,
            )
        }
        MissedPolicy::PauseOnLag if late_count > 0 => {
            next_cursor = Some(cursor.clone());
            pause = Some(CalendarCatchUpPause::Lag);
            (None, Vec::new(), 0)
        }
        MissedPolicy::PauseOnLag => (
            Some(CalendarEmission::Individual(due.clone())),
            Vec::new(),
            0,
        ),
    };
    Ok(Some(CalendarCatchUp {
        observed_at,
        due,
        emission,
        skipped_due,
        discontinuities,
        replay_overflow,
        late_count: late_count as u64,
        maximum_lateness: DurationMs::new(observed - first.intended_at.as_millis()),
        next_cursor,
        pause,
    }))
}
