use serde::{Deserialize, Deserializer, Serialize, de};

use super::{
    CalendarBoundary, CanonicalHash, KarmaBoundaryError, OccurrenceBatch, OccurrenceBatchEmission,
    SemanticScheduleTick, TimestampMs, canonical_hash,
};

pub const KARMA_OCCURRENCE_HASH_DOMAIN: &str = "karma.occurrence.v1";
pub const SEMANTIC_CALENDAR_TICK_HASH_DOMAIN: &str = "karma.semantic-calendar-tick.v1";
pub const CALENDAR_COALESCED_BATCH_HASH_DOMAIN: &str = "karma.calendar-coalesced-batch.v1";

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

    fn logical_at(&self) -> Result<TimestampMs, KarmaBoundaryError> {
        match self {
            Self::ScheduleTick { tick, .. } => Ok(tick.intended_at),
            Self::ScheduleCoalesced { batch, .. } => batch.range.last(),
            Self::CalendarTick { tick, .. } => Ok(tick.boundary.intended_at),
            Self::CalendarCoalesced { batch, .. } => Ok(batch
                .boundaries
                .last()
                .expect("validated calendar batch is non-empty")
                .intended_at),
        }
    }

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        match self {
            Self::ScheduleTick { tick, .. } => {
                tick.tick_hash()?;
                Ok(())
            }
            Self::ScheduleCoalesced { batch, .. } => {
                if batch.emission != OccurrenceBatchEmission::Coalesced {
                    return Err(KarmaBoundaryError::invalid_input(
                        "schedule-coalesced occurrence requires a coalesced batch",
                    ));
                }
                batch.batch_hash()?;
                Ok(())
            }
            Self::CalendarTick { tick, .. } => {
                tick.tick_hash()?;
                Ok(())
            }
            Self::CalendarCoalesced { batch, .. } => {
                batch.validate()?;
                batch.batch_hash()?;
                Ok(())
            }
        }
    }
}

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
        batch.validate()?;
        Ok(batch)
    }

    pub fn batch_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(CALENDAR_COALESCED_BATCH_HASH_DOMAIN, self)
    }

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.schema != CalendarCoalescedBatchSchema::V1
            || self.batch_sequence == 0
            || self.boundaries.is_empty()
        {
            return Err(KarmaBoundaryError::invalid_input(
                "calendar coalesced batch needs a supported schema, positive sequence, and boundaries",
            ));
        }
        if self
            .boundaries
            .windows(2)
            .any(|pair| pair[0].intended_at >= pair[1].intended_at)
        {
            return Err(KarmaBoundaryError::invalid_input(
                "calendar coalesced batch boundaries must increase by intended time",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for CalendarCoalescedBatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CalendarCoalescedBatchWire::deserialize(deserializer)?;
        let batch = Self {
            schema: wire.schema,
            activation_hash: wire.activation_hash,
            batch_sequence: wire.batch_sequence,
            boundaries: wire.boundaries,
        };
        batch.validate().map_err(de::Error::custom)?;
        Ok(batch)
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
        source.validate()?;
        let logical_at = source.logical_at()?;
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
        self.source.validate()?;
        if self.logical_at != self.source.logical_at()? {
            return Err(KarmaBoundaryError::invalid_input(
                "Karma occurrence logical time disagrees with its source",
            ));
        }
        if self
            .parent_occurrence_hash
            .as_ref()
            .is_some_and(|parent| self.occurrence_hash().ok().as_ref() == Some(parent))
        {
            return Err(KarmaBoundaryError::invalid_input(
                "Karma occurrence cannot name itself as its parent",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for KarmaOccurrenceEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
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
