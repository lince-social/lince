use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use super::{
    CalendarBoundary, CalendarDiscontinuity, CalendarSchedule, CanonicalHash, DurationMs,
    KarmaBoundaryError, MissedPolicy, TimeZoneProvider, TimestampMs, canonical_hash,
};

pub const CALENDAR_SCHEDULE_OCCURRENCE_HASH_DOMAIN: &str = "karma.calendar-schedule-occurrence.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CalendarScheduleOccurrenceSchema {
    #[serde(rename = "karma.calendar-schedule-occurrence.v1")]
    V1,
}

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

    pub fn validate_for(
        &self,
        schedule: &CalendarSchedule,
        provider: &dyn TimeZoneProvider,
    ) -> Result<(), KarmaBoundaryError> {
        match resolve_calendar_cursor(schedule, provider, self.previous)? {
            CalendarCursorResolution::Armed(expected) if expected == *self => Ok(()),
            CalendarCursorResolution::Armed(_) => Err(KarmaBoundaryError::invalid_input(
                "calendar cursor disagrees with pinned schedule/provider resolution",
            )),
            CalendarCursorResolution::Paused { .. } => Err(KarmaBoundaryError::invalid_input(
                "calendar cursor claims a boundary where the schedule pauses",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum CalendarCursorResolution {
    Armed(CalendarCursor),
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
    let resolved = schedule.next_after(provider, previous)?;
    match (resolved.boundary, resolved.pause) {
        (Some(next), None) => Ok(CalendarCursorResolution::Armed(CalendarCursor {
            previous,
            next,
            skipped_before_next: resolved.skipped,
        })),
        (None, Some(reason)) => Ok(CalendarCursorResolution::Paused {
            previous,
            reason,
            skipped: resolved.skipped,
        }),
        _ => Err(KarmaBoundaryError::invalid_definition(
            "calendar provider returned an incoherent advance",
        )),
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarScheduleOccurrence {
    pub schema: CalendarScheduleOccurrenceSchema,
    pub activation_hash: CanonicalHash,
    pub sequence: u64,
    pub claimed_cursor_revision: u64,
    pub lease_fencing_token: u64,
    pub observed_at: TimestampMs,
    pub catch_up: CalendarCatchUp,
}

#[derive(Deserialize)]
struct CalendarScheduleOccurrenceWire {
    schema: CalendarScheduleOccurrenceSchema,
    activation_hash: CanonicalHash,
    sequence: u64,
    claimed_cursor_revision: u64,
    lease_fencing_token: u64,
    observed_at: TimestampMs,
    catch_up: CalendarCatchUp,
}

impl CalendarScheduleOccurrence {
    pub fn new(
        activation_hash: CanonicalHash,
        sequence: u64,
        claimed_cursor_revision: u64,
        lease_fencing_token: u64,
        observed_at: TimestampMs,
        catch_up: CalendarCatchUp,
    ) -> Result<Self, KarmaBoundaryError> {
        if sequence == 0 || claimed_cursor_revision == 0 || lease_fencing_token == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "calendar occurrence sequence, cursor revision, and fencing token must be positive",
            ));
        }
        if catch_up.observed_at != observed_at || catch_up.emission.is_none() {
            return Err(KarmaBoundaryError::invalid_input(
                "calendar occurrence requires an emitted catch-up at the same observed time",
            ));
        }
        Ok(Self {
            schema: CalendarScheduleOccurrenceSchema::V1,
            activation_hash,
            sequence,
            claimed_cursor_revision,
            lease_fencing_token,
            observed_at,
            catch_up,
        })
    }

    pub fn occurrence_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(CALENDAR_SCHEDULE_OCCURRENCE_HASH_DOMAIN, self)
    }
}

impl<'de> Deserialize<'de> for CalendarScheduleOccurrence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        let wire = CalendarScheduleOccurrenceWire::deserialize(deserializer)?;
        if wire.schema != CalendarScheduleOccurrenceSchema::V1 {
            return Err(D::Error::custom(
                "unsupported calendar schedule occurrence schema",
            ));
        }
        Self::new(
            wire.activation_hash,
            wire.sequence,
            wire.claimed_cursor_revision,
            wire.lease_fencing_token,
            wire.observed_at,
            wire.catch_up,
        )
        .map_err(D::Error::custom)
    }
}

pub fn advance_calendar_cursor(
    schedule: &CalendarSchedule,
    provider: &dyn TimeZoneProvider,
    cursor: &CalendarCursor,
    observed_at: TimestampMs,
    catch_up_budget: NonZeroU32,
) -> Result<Option<CalendarCatchUp>, KarmaBoundaryError> {
    cursor.validate_for(schedule, provider)?;
    if observed_at < cursor.next.intended_at {
        return Ok(None);
    }

    let mut due = Vec::new();
    let mut discontinuities = cursor.skipped_before_next.clone();
    let mut current = cursor.clone();
    let (next_cursor, discontinuity_pause) = loop {
        due.push(current.next);
        let next = resolve_calendar_cursor(schedule, provider, Some(current.next))?;
        match next {
            CalendarCursorResolution::Armed(next) => {
                discontinuities.extend(next.skipped_before_next.iter().copied());
                if next.next.intended_at > observed_at {
                    break (Some(next), None);
                }
                if due.len() >= catch_up_budget.get() as usize {
                    return Err(KarmaBoundaryError::invalid_input(
                        "calendar catch-up exceeded the explicit boundary budget",
                    ));
                }
                current = next;
            }
            CalendarCursorResolution::Paused {
                reason, skipped, ..
            } => {
                discontinuities.extend(skipped);
                break (None, Some(reason));
            }
        }
    };

    let max_lateness_ms = i64::from(schedule.timer.max_lateness_ms());
    let late_count = due
        .iter()
        .take_while(|boundary| {
            observed_at.as_millis() - boundary.intended_at.as_millis() > max_lateness_ms
        })
        .count() as u64;
    let maximum_lateness = DurationMs::new(
        observed_at
            .as_millis()
            .checked_sub(due[0].intended_at.as_millis())
            .expect("calendar due boundary cannot follow observation"),
    );
    if schedule.missed == MissedPolicy::PauseOnLag && late_count > 0 {
        return Ok(Some(CalendarCatchUp {
            observed_at,
            due,
            emission: None,
            skipped_due: Vec::new(),
            discontinuities,
            replay_overflow: 0,
            late_count,
            maximum_lateness,
            next_cursor: Some(cursor.clone()),
            pause: Some(CalendarCatchUpPause::Lag),
        }));
    }

    let (emission, skipped_due, replay_overflow) = match schedule.missed {
        MissedPolicy::Skip => {
            let last = *due.last().expect("due calendar boundaries are non-empty");
            if observed_at.as_millis() - last.intended_at.as_millis() <= max_lateness_ms {
                let skipped = due[..due.len() - 1].to_vec();
                (Some(CalendarEmission::Individual(vec![last])), skipped, 0)
            } else {
                (None, due.clone(), 0)
            }
        }
        MissedPolicy::Coalesce => (
            Some(CalendarEmission::Coalesced(due.clone())),
            Vec::new(),
            0,
        ),
        MissedPolicy::Replay { max } => {
            let emitted_count = due.len().min(max.get() as usize);
            let skipped = due[emitted_count..].to_vec();
            (
                Some(CalendarEmission::Individual(due[..emitted_count].to_vec())),
                skipped,
                (due.len() - emitted_count) as u64,
            )
        }
        MissedPolicy::PauseOnLag => (
            Some(CalendarEmission::Individual(due.clone())),
            Vec::new(),
            0,
        ),
    };
    let pause = discontinuity_pause.map(|reason| CalendarCatchUpPause::Discontinuity { reason });
    Ok(Some(CalendarCatchUp {
        observed_at,
        due,
        emission,
        skipped_due,
        discontinuities,
        replay_overflow,
        late_count,
        maximum_lateness,
        next_cursor,
        pause,
    }))
}
