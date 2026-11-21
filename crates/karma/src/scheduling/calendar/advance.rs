//! Calendar schedule advancement and discontinuity reporting.

use chrono::Datelike;
use serde::{Deserialize, Serialize};

use super::{
    CivilDateTime, DayOfMonth, FoldPolicy, GapPolicy, LocalTimeResolution, TimeZoneId,
    TimeZoneProvider, TzdbRevision,
};
use crate::{
    Cadence, InactiveGapPolicy, InvalidDay, KarmaBoundaryError, MissedPolicy, NoOccurrence,
    OverloadPolicy, RephasePolicy, TimerPolicy, TimestampMs,
};

const MAX_SEARCH_STEPS: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CalendarBoundaryKind {
    Unique,
    GapShiftedForward,
    FoldFirst,
    FoldSecond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarBoundary {
    pub requested_local: CivilDateTime,
    pub intended_at: TimestampMs,
    pub kind: CalendarBoundaryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CalendarDiscontinuity {
    Gap {
        local: CivilDateTime,
        before: TimestampMs,
        first_valid_after: TimestampMs,
    },
    Fold {
        local: CivilDateTime,
        first: TimestampMs,
        second: TimestampMs,
    },
    FoldFirst {
        local: CivilDateTime,
        instant: TimestampMs,
    },
    FoldSecond {
        local: CivilDateTime,
        instant: TimestampMs,
    },
    InvalidDayOfMonth {
        year: i32,
        month: u32,
        day: DayOfMonth,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarAdvance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary: Option<CalendarBoundary>,
    pub skipped: Vec<CalendarDiscontinuity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause: Option<CalendarDiscontinuity>,
}

impl CalendarAdvance {
    fn boundary(boundary: CalendarBoundary, skipped: Vec<CalendarDiscontinuity>) -> Self {
        Self {
            boundary: Some(boundary),
            skipped,
            pause: None,
        }
    }

    fn retired(skipped: Vec<CalendarDiscontinuity>) -> Self {
        Self {
            boundary: None,
            skipped,
            pause: None,
        }
    }

    pub fn is_retired(&self) -> bool {
        self.boundary.is_none() && self.pause.is_none()
    }

    fn paused(reason: CalendarDiscontinuity, skipped: Vec<CalendarDiscontinuity>) -> Self {
        Self {
            boundary: None,
            skipped,
            pause: Some(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarSchedule {
    pub anchor: CivilDateTime,
    pub cadence: Cadence,
    pub timezone: TimeZoneId,
    pub tzdb: TzdbRevision,
    pub gap: GapPolicy,
    pub fold: FoldPolicy,
    pub timer: TimerPolicy,
    pub missed: MissedPolicy,
    pub inactive_gap: InactiveGapPolicy,
    pub rephase: RephasePolicy,
    pub overload: OverloadPolicy,
}

impl CalendarSchedule {
    /// Advances the schedule from its anchor or past the previous boundary.
    ///
    /// The result reports a boundary, a pause, or retirement.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched timezone revision, an invalid previous boundary
    /// or provider resolution, and an exhausted search budget.
    pub fn next_after(
        &self,
        provider: &dyn TimeZoneProvider,
        previous: Option<CalendarBoundary>,
    ) -> Result<CalendarAdvance, KarmaBoundaryError> {
        if provider.revision() != &self.tzdb {
            return Err(KarmaBoundaryError::invalid_definition(
                "calendar schedule tzdb revision does not match provider",
            ));
        }

        if let Some(previous) = previous
            && let LocalTimeResolution::Fold { second, .. } =
                self.validate_previous(provider, previous)?
            && previous.kind == CalendarBoundaryKind::FoldFirst
            && self.fold == FoldPolicy::Both
        {
            return Ok(CalendarAdvance::boundary(
                CalendarBoundary {
                    requested_local: previous.requested_local,
                    intended_at: second,
                    kind: CalendarBoundaryKind::FoldSecond,
                },
                Vec::new(),
            ));
        }

        let mut skipped = Vec::new();
        let mut after_local = previous.map(|boundary| boundary.requested_local);
        for _ in 0..MAX_SEARCH_STEPS {
            let candidate = match self.next_local_candidate(after_local)? {
                LocalCandidate::Value(value) => value,
                LocalCandidate::Retired => {
                    return Ok(CalendarAdvance::retired(skipped));
                }
                LocalCandidate::InvalidDay { year, month, day } => {
                    let discontinuity =
                        CalendarDiscontinuity::InvalidDayOfMonth { year, month, day };
                    match self.cadence.invalid_day {
                        InvalidDay::Skip => {
                            skipped.push(discontinuity);
                            after_local = Some(CivilDateTime::end_of_month(year, month)?);
                            continue;
                        }
                        InvalidDay::Pause => {
                            return Ok(CalendarAdvance::paused(discontinuity, skipped));
                        }
                        InvalidDay::Clamp => {
                            return Err(KarmaBoundaryError::invalid_definition(
                                "calendar candidate generation violated month-day policy",
                            ));
                        }
                    }
                }
            };
            let (intended_at, kind, excluded) = match self.resolve(provider, candidate)? {
                LocalTimeResolution::Unique { instant } => {
                    (instant, CalendarBoundaryKind::Unique, None)
                }
                LocalTimeResolution::Gap {
                    before,
                    first_valid_after,
                } => {
                    let gap = CalendarDiscontinuity::Gap {
                        local: candidate,
                        before,
                        first_valid_after,
                    };
                    match self.gap {
                        GapPolicy::Skip => {
                            skipped.push(gap);
                            after_local = Some(candidate);
                            continue;
                        }
                        GapPolicy::ShiftForward => (
                            first_valid_after,
                            CalendarBoundaryKind::GapShiftedForward,
                            None,
                        ),
                        GapPolicy::Pause => return Ok(CalendarAdvance::paused(gap, skipped)),
                    }
                }
                LocalTimeResolution::Fold { first, second } => match self.fold {
                    FoldPolicy::First | FoldPolicy::Both => (
                        first,
                        CalendarBoundaryKind::FoldFirst,
                        (self.fold == FoldPolicy::First).then_some(
                            CalendarDiscontinuity::FoldSecond {
                                local: candidate,
                                instant: second,
                            },
                        ),
                    ),
                    FoldPolicy::Second => (
                        second,
                        CalendarBoundaryKind::FoldSecond,
                        Some(CalendarDiscontinuity::FoldFirst {
                            local: candidate,
                            instant: first,
                        }),
                    ),
                    FoldPolicy::Pause => {
                        return Ok(CalendarAdvance::paused(
                            CalendarDiscontinuity::Fold {
                                local: candidate,
                                first,
                                second,
                            },
                            skipped,
                        ));
                    }
                },
            };
            validate_after_previous(previous, intended_at)?;
            skipped.extend(excluded);
            return Ok(CalendarAdvance::boundary(
                CalendarBoundary {
                    requested_local: candidate,
                    intended_at,
                    kind,
                },
                skipped,
            ));
        }
        Err(KarmaBoundaryError::invalid_definition(
            "calendar resolution exceeded deterministic search budget",
        ))
    }

    fn resolve(
        &self,
        provider: &dyn TimeZoneProvider,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        provider.resolve_local(&self.timezone, local)?.validate()
    }

    fn validate_previous(
        &self,
        provider: &dyn TimeZoneProvider,
        previous: CalendarBoundary,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        if !self
            .cadence
            .produces_civil(self.anchor, previous.requested_local)
        {
            return Err(KarmaBoundaryError::invalid_input(
                "previous calendar boundary is not on this schedule cadence",
            ));
        }
        let resolution = self.resolve(provider, previous.requested_local)?;
        let matches = match (previous.kind, resolution) {
            (CalendarBoundaryKind::Unique, LocalTimeResolution::Unique { instant }) => {
                previous.intended_at == instant
            }
            (
                CalendarBoundaryKind::GapShiftedForward,
                LocalTimeResolution::Gap {
                    first_valid_after, ..
                },
            ) => self.gap == GapPolicy::ShiftForward && previous.intended_at == first_valid_after,
            (CalendarBoundaryKind::FoldFirst, LocalTimeResolution::Fold { first, .. }) => {
                matches!(self.fold, FoldPolicy::First | FoldPolicy::Both)
                    && previous.intended_at == first
            }
            (CalendarBoundaryKind::FoldSecond, LocalTimeResolution::Fold { second, .. }) => {
                matches!(self.fold, FoldPolicy::Second | FoldPolicy::Both)
                    && previous.intended_at == second
            }
            _ => false,
        };
        if matches {
            Ok(resolution)
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "previous calendar boundary does not match schedule/provider resolution",
            ))
        }
    }

    fn next_local_candidate(
        &self,
        after: Option<CivilDateTime>,
    ) -> Result<LocalCandidate, KarmaBoundaryError> {
        let lower = after.unwrap_or(self.anchor);
        let mut index = self.cadence.index_floor_civil(self.anchor, lower);
        for _ in 0..MAX_SEARCH_STEPS {
            match self.cadence.civil_at_or_reason(self.anchor, index) {
                Ok(candidate) => {
                    if candidate >= self.anchor && after.is_none_or(|previous| candidate > previous)
                    {
                        return Ok(LocalCandidate::Value(candidate));
                    }
                }
                Err(NoOccurrence::InvalidMonthDay { year, month, day }) => {
                    let ahead = after.is_none_or(|previous| {
                        let date = previous.as_naive().date();
                        (year, month) > (date.year(), date.month())
                    });
                    if ahead {
                        return Ok(LocalCandidate::InvalidDay {
                            year,
                            month,
                            // Cadence reports the anchor's day, which is in 1..=31.
                            day: DayOfMonth::new(day as u8)?,
                        });
                    }
                }
                Err(NoOccurrence::Retired) => return Ok(LocalCandidate::Retired),
                Err(NoOccurrence::Exhausted) => {
                    return Err(KarmaBoundaryError::invalid_definition(
                        "calendar cadence ran out of representable range",
                    ));
                }
            }
            index = index.checked_add(1).ok_or_else(|| {
                KarmaBoundaryError::invalid_definition("calendar cadence overflows")
            })?;
        }
        Err(KarmaBoundaryError::invalid_definition(
            "calendar resolution exceeded deterministic search budget",
        ))
    }
}

enum LocalCandidate {
    Value(CivilDateTime),
    Retired,
    InvalidDay {
        year: i32,
        month: u32,
        day: DayOfMonth,
    },
}

fn validate_after_previous(
    previous: Option<CalendarBoundary>,
    instant: TimestampMs,
) -> Result<(), KarmaBoundaryError> {
    if previous.is_some_and(|previous| instant <= previous.intended_at) {
        Err(KarmaBoundaryError::invalid_definition(
            "calendar provider produced a non-increasing UTC boundary",
        ))
    } else {
        Ok(())
    }
}
