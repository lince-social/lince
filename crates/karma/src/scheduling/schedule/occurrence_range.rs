//! Validated elapsed occurrence runs and their partitions.

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::{DurationMs, KarmaBoundaryError, TimestampMs};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "OccurrenceRangeWire")]
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

    pub(super) fn through(
        first: TimestampMs,
        interval: NonZeroU64,
        observed: TimestampMs,
    ) -> Option<(Self, u64)> {
        if observed < first {
            return None;
        }
        // TimestampMs bounds the difference, and the last boundary is at or before observed.
        let elapsed = observed.as_millis().abs_diff(first.as_millis());
        let interval_ms = interval.get();
        let count = elapsed / interval_ms + 1;
        Some((
            Self {
                first,
                interval_ms,
                count,
            },
            elapsed,
        ))
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
        self.at(self.count - 1)
            .ok_or_else(|| KarmaBoundaryError::invalid_input("occurrence range overflows"))
    }

    fn at(&self, index: u64) -> Option<TimestampMs> {
        let offset = i64::try_from(self.interval_ms.checked_mul(index)?).ok()?;
        self.first.checked_add(DurationMs::new(offset))
    }

    pub(super) fn split_at(
        &self,
        count: u64,
    ) -> Result<(Option<Self>, Option<Self>), KarmaBoundaryError> {
        if count == 0 {
            return Ok((None, Some(self.clone())));
        }
        let remaining = self
            .count
            .checked_sub(count)
            .ok_or_else(|| KarmaBoundaryError::invalid_input("occurrence prefix exceeds range"))?;
        let prefix = Some(Self {
            count,
            ..self.clone()
        });
        let suffix = if remaining == 0 {
            None
        } else {
            let first = self
                .at(count)
                .ok_or_else(|| KarmaBoundaryError::invalid_input("occurrence suffix overflows"))?;
            Some(Self {
                first,
                count: remaining,
                ..self.clone()
            })
        };
        Ok((prefix, suffix))
    }
}

#[derive(Deserialize)]
struct OccurrenceRangeWire {
    first: TimestampMs,
    interval_ms: u64,
    count: u64,
}

impl TryFrom<OccurrenceRangeWire> for OccurrenceRange {
    type Error = KarmaBoundaryError;

    fn try_from(wire: OccurrenceRangeWire) -> Result<Self, Self::Error> {
        Self::new(wire.first, wire.interval_ms, wire.count)
    }
}
