//! Validated occurrence envelopes and their durable identities.

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    CanonicalHash, KarmaBoundaryError, OccurrenceBatch, OccurrenceBatchEmission,
    SemanticScheduleTick, TimestampMs, canonical_hash,
};

mod calendar;

pub use calendar::{
    CALENDAR_COALESCED_BATCH_HASH_DOMAIN, CalendarCoalescedBatch, CalendarCoalescedBatchSchema,
    SEMANTIC_CALENDAR_TICK_HASH_DOMAIN, SemanticCalendarTick, SemanticCalendarTickSchema,
};

pub const KARMA_OCCURRENCE_HASH_DOMAIN: &str = "karma.occurrence.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum KarmaOccurrenceSchema {
    #[serde(rename = "karma.occurrence.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum KarmaOccurrenceSource {
    ScheduleTick {
        schedule_occurrence_hash: CanonicalHash,
        tick: SemanticScheduleTick,
    },
    ScheduleCoalesced {
        schedule_occurrence_hash: CanonicalHash,
        batch: OccurrenceBatch,
    },
    CalendarTick {
        schedule_occurrence_hash: CanonicalHash,
        tick: SemanticCalendarTick,
    },
    CalendarCoalesced {
        schedule_occurrence_hash: CanonicalHash,
        batch: CalendarCoalescedBatch,
    },
}

impl KarmaOccurrenceSource {
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::ScheduleTick { .. } => "schedule-tick",
            Self::ScheduleCoalesced { .. } => "schedule-coalesced",
            Self::CalendarTick { .. } => "calendar-tick",
            Self::CalendarCoalesced { .. } => "calendar-coalesced",
        }
    }

    pub fn source_identity(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        match self {
            Self::ScheduleTick { tick, .. } => tick.tick_hash(),
            Self::ScheduleCoalesced { batch, .. } => batch.batch_hash(),
            Self::CalendarTick { tick, .. } => tick.tick_hash(),
            Self::CalendarCoalesced { batch, .. } => batch.batch_hash(),
        }
    }

    fn validated_logical_at(&self) -> Result<TimestampMs, KarmaBoundaryError> {
        match self {
            Self::ScheduleTick { tick, .. } => Ok(tick.intended_at),
            Self::ScheduleCoalesced { batch, .. } => {
                if batch.emission != OccurrenceBatchEmission::Coalesced {
                    return Err(KarmaBoundaryError::invalid_input(
                        "schedule-coalesced occurrence requires a coalesced batch",
                    ));
                }
                batch.range.last()
            }
            Self::CalendarTick { tick, .. } => Ok(tick.boundary.intended_at),
            Self::CalendarCoalesced { batch, .. } => batch.validated_logical_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KarmaOccurrenceEnvelope {
    pub schema: KarmaOccurrenceSchema,
    pub source: KarmaOccurrenceSource,
    pub logical_at: TimestampMs,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_occurrence_hash: Option<CanonicalHash>,
}

#[derive(Deserialize)]
struct KarmaOccurrenceEnvelopeWire {
    schema: KarmaOccurrenceSchema,
    source: KarmaOccurrenceSource,
    logical_at: TimestampMs,
    #[serde(default)]
    parent_occurrence_hash: Option<CanonicalHash>,
}

impl KarmaOccurrenceEnvelope {
    pub fn schedule_tick(
        schedule_occurrence_hash: CanonicalHash,
        tick: SemanticScheduleTick,
        parent_occurrence_hash: Option<CanonicalHash>,
    ) -> Result<Self, KarmaBoundaryError> {
        Self::new(
            KarmaOccurrenceSource::ScheduleTick {
                schedule_occurrence_hash,
                tick,
            },
            parent_occurrence_hash,
        )
    }

    pub fn schedule_coalesced(
        schedule_occurrence_hash: CanonicalHash,
        batch: OccurrenceBatch,
        parent_occurrence_hash: Option<CanonicalHash>,
    ) -> Result<Self, KarmaBoundaryError> {
        Self::new(
            KarmaOccurrenceSource::ScheduleCoalesced {
                schedule_occurrence_hash,
                batch,
            },
            parent_occurrence_hash,
        )
    }

    pub fn calendar_tick(
        schedule_occurrence_hash: CanonicalHash,
        tick: SemanticCalendarTick,
        parent_occurrence_hash: Option<CanonicalHash>,
    ) -> Result<Self, KarmaBoundaryError> {
        Self::new(
            KarmaOccurrenceSource::CalendarTick {
                schedule_occurrence_hash,
                tick,
            },
            parent_occurrence_hash,
        )
    }

    pub fn calendar_coalesced(
        schedule_occurrence_hash: CanonicalHash,
        batch: CalendarCoalescedBatch,
        parent_occurrence_hash: Option<CanonicalHash>,
    ) -> Result<Self, KarmaBoundaryError> {
        Self::new(
            KarmaOccurrenceSource::CalendarCoalesced {
                schedule_occurrence_hash,
                batch,
            },
            parent_occurrence_hash,
        )
    }

    fn new(
        source: KarmaOccurrenceSource,
        parent_occurrence_hash: Option<CanonicalHash>,
    ) -> Result<Self, KarmaBoundaryError> {
        let logical_at = source.validated_logical_at()?;
        Ok(Self {
            schema: KarmaOccurrenceSchema::V1,
            source,
            logical_at,
            parent_occurrence_hash,
        })
    }

    pub fn occurrence_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(KARMA_OCCURRENCE_HASH_DOMAIN, self)
    }

    pub fn source_identity(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.source.source_identity()
    }

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.schema != KarmaOccurrenceSchema::V1 {
            return Err(KarmaBoundaryError::invalid_input(
                "unsupported Karma occurrence schema",
            ));
        }
        if self.logical_at != self.source.validated_logical_at()? {
            return Err(KarmaBoundaryError::invalid_input(
                "Karma occurrence logical time disagrees with its source",
            ));
        }
        if let Some(parent) = &self.parent_occurrence_hash
            && self.occurrence_hash()? == *parent
        {
            return Err(KarmaBoundaryError::invalid_input(
                "Karma occurrence cannot name itself as its parent",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for KarmaOccurrenceEnvelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = KarmaOccurrenceEnvelopeWire::deserialize(deserializer)?;
        let envelope = Self {
            schema: wire.schema,
            source: wire.source,
            logical_at: wire.logical_at,
            parent_occurrence_hash: wire.parent_occurrence_hash,
        };
        envelope.validate().map_err(de::Error::custom)?;
        Ok(envelope)
    }
}
