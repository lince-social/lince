//! Timer-independent calendar ticks and coalesced batches.

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{CalendarBoundary, CanonicalHash, KarmaBoundaryError, TimestampMs, canonical_hash};

pub const SEMANTIC_CALENDAR_TICK_HASH_DOMAIN: &str = "karma.semantic-calendar-tick.v1";
pub const CALENDAR_COALESCED_BATCH_HASH_DOMAIN: &str = "karma.calendar-coalesced-batch.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SemanticCalendarTickSchema {
    #[serde(rename = "karma.semantic-calendar-tick.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticCalendarTick {
    pub schema: SemanticCalendarTickSchema,
    pub activation_hash: CanonicalHash,
    pub boundary: CalendarBoundary,
}

impl SemanticCalendarTick {
    pub fn new(activation_hash: CanonicalHash, boundary: CalendarBoundary) -> Self {
        Self {
            schema: SemanticCalendarTickSchema::V1,
            activation_hash,
            boundary,
        }
    }

    pub fn tick_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(SEMANTIC_CALENDAR_TICK_HASH_DOMAIN, self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CalendarCoalescedBatchSchema {
    #[serde(rename = "karma.calendar-coalesced-batch.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarCoalescedBatch {
    pub schema: CalendarCoalescedBatchSchema,
    pub activation_hash: CanonicalHash,
    pub batch_sequence: u64,
    pub boundaries: Vec<CalendarBoundary>,
}

#[derive(Deserialize)]
struct CalendarCoalescedBatchWire {
    schema: CalendarCoalescedBatchSchema,
    activation_hash: CanonicalHash,
    batch_sequence: u64,
    boundaries: Vec<CalendarBoundary>,
}

impl CalendarCoalescedBatch {
    pub fn new(
        activation_hash: CanonicalHash,
        batch_sequence: u64,
        boundaries: Vec<CalendarBoundary>,
    ) -> Result<Self, KarmaBoundaryError> {
        let batch = Self {
            schema: CalendarCoalescedBatchSchema::V1,
            activation_hash,
            batch_sequence,
            boundaries,
        };
        batch.validated_logical_at()?;
        Ok(batch)
    }

    pub fn batch_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(CALENDAR_COALESCED_BATCH_HASH_DOMAIN, self)
    }

    pub(super) fn validated_logical_at(&self) -> Result<TimestampMs, KarmaBoundaryError> {
        let last = self.boundaries.last().filter(|_| {
            self.schema == CalendarCoalescedBatchSchema::V1 && self.batch_sequence != 0
        }).ok_or_else(|| {
            KarmaBoundaryError::invalid_input(
                "calendar coalesced batch needs a supported schema, positive sequence, and boundaries",
            )
        })?;
        if self
            .boundaries
            .array_windows()
            .any(|[left, right]| left.intended_at >= right.intended_at)
        {
            return Err(KarmaBoundaryError::invalid_input(
                "calendar coalesced batch boundaries must increase by intended time",
            ));
        }
        Ok(last.intended_at)
    }
}

impl<'de> Deserialize<'de> for CalendarCoalescedBatch {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = CalendarCoalescedBatchWire::deserialize(deserializer)?;
        let batch = Self {
            schema: wire.schema,
            activation_hash: wire.activation_hash,
            batch_sequence: wire.batch_sequence,
            boundaries: wire.boundaries,
        };
        batch.validated_logical_at().map_err(de::Error::custom)?;
        Ok(batch)
    }
}
