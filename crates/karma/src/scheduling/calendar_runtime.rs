//! Calendar cursor verification, catch-up, and durable occurrence records.

use serde::{Deserialize, Serialize};

use crate::{
    CalendarAdvance, CalendarBoundary, CalendarDiscontinuity, CalendarSchedule, KarmaBoundaryError,
    TimeZoneProvider,
};

mod catch_up;
mod occurrence;

pub use catch_up::{
    CalendarCatchUp, CalendarCatchUpPause, CalendarEmission, advance_calendar_cursor,
};
pub use occurrence::{
    CALENDAR_SCHEDULE_OCCURRENCE_HASH_DOMAIN, CalendarScheduleOccurrence,
    CalendarScheduleOccurrenceSchema,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarCursor {
    #[serde(skip_serializing_if = "Option::is_none")]
    previous: Option<CalendarBoundary>,
    next: CalendarBoundary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    skipped_before_next: Vec<CalendarDiscontinuity>,
}

impl CalendarCursor {
    pub const fn previous(&self) -> Option<CalendarBoundary> {
        self.previous
    }

    pub const fn next(&self) -> CalendarBoundary {
        self.next
    }

    pub fn skipped_before_next(&self) -> &[CalendarDiscontinuity] {
        &self.skipped_before_next
    }

    /// Checks that the stored cursor matches the pinned provider's next boundary.
    ///
    /// # Errors
    ///
    /// Rejects a changed resolution or a boundary on a paused or retired schedule.
    pub fn validate_for(
        &self,
        schedule: &CalendarSchedule,
        provider: &dyn TimeZoneProvider,
    ) -> Result<(), KarmaBoundaryError> {
        self.verified(schedule, provider).map(drop)
    }

    fn verified(
        &self,
        schedule: &CalendarSchedule,
        provider: &dyn TimeZoneProvider,
    ) -> Result<Self, KarmaBoundaryError> {
        match resolve_calendar_cursor(schedule, provider, self.previous)? {
            CalendarCursorResolution::Armed(expected) if expected == *self => Ok(expected),
            CalendarCursorResolution::Armed(_) => Err(KarmaBoundaryError::invalid_input(
                "calendar cursor disagrees with pinned schedule/provider resolution",
            )),
            CalendarCursorResolution::Paused { .. } | CalendarCursorResolution::Retired { .. } => {
                Err(KarmaBoundaryError::invalid_input(
                    "calendar cursor claims a boundary where the schedule is inactive",
                ))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum CalendarCursorResolution {
    Armed(CalendarCursor),
    Retired {
        #[serde(skip_serializing_if = "Option::is_none")]
        previous: Option<CalendarBoundary>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<CalendarDiscontinuity>,
    },
    Paused {
        #[serde(skip_serializing_if = "Option::is_none")]
        previous: Option<CalendarBoundary>,
        reason: CalendarDiscontinuity,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<CalendarDiscontinuity>,
    },
}

pub fn resolve_calendar_cursor(
    schedule: &CalendarSchedule,
    provider: &dyn TimeZoneProvider,
    previous: Option<CalendarBoundary>,
) -> Result<CalendarCursorResolution, KarmaBoundaryError> {
    let CalendarAdvance {
        boundary,
        pause,
        skipped,
    } = schedule.next_after(provider, previous)?;
    match (boundary, pause) {
        (Some(next), None) => Ok(CalendarCursorResolution::Armed(CalendarCursor {
            previous,
            next,
            skipped_before_next: skipped,
        })),
        (None, Some(reason)) => Ok(CalendarCursorResolution::Paused {
            previous,
            reason,
            skipped,
        }),
        (None, None) => Ok(CalendarCursorResolution::Retired { previous, skipped }),
        _ => Err(KarmaBoundaryError::invalid_definition(
            "calendar provider returned an incoherent advance",
        )),
    }
}
