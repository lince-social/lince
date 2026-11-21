//! Segment validation and local-time resolution on a contiguous UTC timeline.

use serde::{Deserialize, Serialize};

use super::MAX_TZDB_SEGMENTS_PER_ZONE;
use crate::{CivilDateTime, KarmaBoundaryError, LocalTimeResolution, TimestampMs};

const MILLIS_PER_SECOND: i64 = 1_000;
const MAX_OFFSET_SECONDS: u32 = 86_400;
const MAX_OFFSET_MS: i64 = MAX_OFFSET_SECONDS as i64 * MILLIS_PER_SECOND;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtcOffsetSegment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utc_start: Option<TimestampMs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utc_end: Option<TimestampMs>,
    pub offset_seconds: i32,
}

impl UtcOffsetSegment {
    pub fn new(
        utc_start: Option<TimestampMs>,
        utc_end: Option<TimestampMs>,
        offset_seconds: i32,
    ) -> Result<Self, KarmaBoundaryError> {
        let segment = Self {
            utc_start,
            utc_end,
            offset_seconds,
        };
        segment.validate()?;
        Ok(segment)
    }

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.offset_seconds.unsigned_abs() > MAX_OFFSET_SECONDS {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact UTC offset exceeds 24 hours",
            ));
        }
        if matches!((self.utc_start, self.utc_end), (Some(start), Some(end)) if start >= end) {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact segment must have an increasing UTC interval",
            ));
        }
        Ok(())
    }

    fn candidate(&self, local: i64) -> Option<TimestampMs> {
        let utc = local - self.offset_ms();
        if self.utc_start.is_some_and(|start| utc < start.as_millis())
            || self.utc_end.is_some_and(|end| utc >= end.as_millis())
        {
            return None;
        }
        TimestampMs::from_millis(utc).ok()
    }

    fn offset_ms(&self) -> i64 {
        i64::from(self.offset_seconds) * MILLIS_PER_SECOND
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeZoneDefinition {
    pub segments: Vec<UtcOffsetSegment>,
}

impl TimeZoneDefinition {
    pub fn new(segments: Vec<UtcOffsetSegment>) -> Result<Self, KarmaBoundaryError> {
        let definition = Self { segments };
        definition.validate()?;
        Ok(definition)
    }

    pub(super) fn validate(&self) -> Result<(), KarmaBoundaryError> {
        let count = self.segments.len();
        if !(1..=MAX_TZDB_SEGMENTS_PER_ZONE).contains(&count) {
            return Err(KarmaBoundaryError::invalid_definition(format!(
                "timezone artifact zone needs 1..={MAX_TZDB_SEGMENTS_PER_ZONE} segments"
            )));
        }
        if self.segments[0].utc_start.is_some() || self.segments[count - 1].utc_end.is_some() {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact zone must cover the complete UTC timeline",
            ));
        }
        for (index, segment) in self.segments.iter().enumerate() {
            segment.validate()?;
            if index > 0 && segment.utc_start.is_none() {
                return Err(KarmaBoundaryError::invalid_definition(
                    "only the first timezone segment may omit utc_start",
                ));
            }
            if index + 1 < count && segment.utc_end.is_none() {
                return Err(KarmaBoundaryError::invalid_definition(
                    "only the final timezone segment may omit utc_end",
                ));
            }
        }
        for [left, right] in self.segments.array_windows() {
            if left.utc_end != right.utc_start {
                return Err(KarmaBoundaryError::invalid_definition(
                    "timezone artifact UTC segments must be contiguous",
                ));
            }
        }
        self.validate_local_multiplicity()
    }

    fn validate_local_multiplicity(&self) -> Result<(), KarmaBoundaryError> {
        let mut events = Vec::with_capacity(2 * (self.segments.len() - 1));
        for segment in &self.segments {
            let offset = segment.offset_ms();
            if let Some(start) = segment.utc_start {
                events.push((start.as_millis() + offset, 1));
            }
            if let Some(end) = segment.utc_end {
                events.push((end.as_millis() + offset, -1));
            }
        }
        // End events precede starts at equal times for half-open intervals.
        events.sort_unstable();
        let mut active = 1;
        for (_, delta) in events {
            active += delta;
            if !(0..=2).contains(&active) {
                return Err(KarmaBoundaryError::invalid_definition(
                    "timezone artifact has invalid local-time interval multiplicity",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn resolve_local(
        &self,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        let local = local.as_naive().and_utc().timestamp_millis();
        // A candidate can be at most 24 hours away, and UTC segments are disjoint and ordered.
        let start = self.segments.partition_point(|segment| {
            segment
                .utc_end
                .is_some_and(|end| end.as_millis() <= local - MAX_OFFSET_MS)
        });
        let mut candidates = self.segments[start..]
            .iter()
            .take_while(|segment| {
                segment
                    .utc_start
                    .is_none_or(|start| start.as_millis() <= local + MAX_OFFSET_MS)
            })
            .filter_map(|segment| segment.candidate(local));
        match (candidates.next(), candidates.next(), candidates.next()) {
            (Some(instant), None, None) => Ok(LocalTimeResolution::Unique { instant }),
            (Some(first), Some(second), None) => Ok(LocalTimeResolution::Fold { first, second }),
            (None, None, None) => self.resolve_gap(local),
            _ => Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact maps one local instant to more than two UTC instants",
            )),
        }
    }

    fn resolve_gap(&self, local: i64) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        let mut previous = None;
        let mut next = None;
        for segment in &self.segments {
            let offset = segment.offset_ms();
            if let Some(end) = segment.utc_end {
                let at = end.as_millis() + offset;
                if at <= local && previous.is_none_or(|(best, _)| at >= best) {
                    previous = Some((at, end));
                }
            }
            if let Some(start) = segment.utc_start {
                let at = start.as_millis() + offset;
                if at > local && next.is_none_or(|(best, _)| at < best) {
                    next = Some((at, start));
                }
            }
        }
        let (Some((_, transition)), Some((_, first_valid_after))) = (previous, next) else {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact has an unexplained local-time hole",
            ));
        };
        if transition != first_valid_after {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact local gap does not have one UTC transition boundary",
            ));
        }
        // Canonical UTC timestamps are strictly above i64::MIN.
        let before = TimestampMs::from_millis(transition.as_millis() - 1)?;
        Ok(LocalTimeResolution::Gap {
            before,
            first_valid_after,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maximum_offsets_keep_utc_order_and_half_open_fold_endpoints() {
        let transition = TimestampMs::from_millis(0).expect("epoch");
        let zone = TimeZoneDefinition::new(vec![
            UtcOffsetSegment {
                utc_start: None,
                utc_end: Some(transition),
                offset_seconds: 86_400,
            },
            UtcOffsetSegment {
                utc_start: Some(transition),
                utc_end: None,
                offset_seconds: -86_400,
            },
        ])
        .expect("valid two-segment fold");
        let local = |time| CivilDateTime::parse_canonical(time).expect("canonical civil time");
        assert_eq!(
            zone.resolve_local(local("1969-12-31T00:00:00.000")),
            Ok(LocalTimeResolution::Fold {
                first: TimestampMs::from_millis(-2 * MAX_OFFSET_MS).expect("earlier instant"),
                second: transition,
            }),
        );
        assert_eq!(
            zone.resolve_local(local("1970-01-02T00:00:00.000")),
            Ok(LocalTimeResolution::Unique {
                instant: TimestampMs::from_millis(2 * MAX_OFFSET_MS).expect("later instant"),
            }),
        );
    }
}
