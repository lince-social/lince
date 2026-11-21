use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    CanonicalHash, DurationMs, ElapsedSchedule, KarmaBoundaryError, OccurrenceRange,
    ScheduleAdvance, ScheduleEmission, TimestampMs, canonical_hash,
};

pub const ELAPSED_SCHEDULE_OCCURRENCE_HASH_DOMAIN: &str = "karma.elapsed-schedule-occurrence.v1";
pub const OCCURRENCE_BATCH_HASH_DOMAIN: &str = "karma.occurrence-batch.v1";
pub const SEMANTIC_SCHEDULE_TICK_HASH_DOMAIN: &str = "karma.semantic-schedule-tick.v1";
pub const MAX_OCCURRENCE_BATCH_PAGE_TICKS: u32 = 4_096;

/// Persisted version of an elapsed occurrence envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ElapsedScheduleOccurrenceSchema {
    #[serde(rename = "karma.elapsed-schedule-occurrence.v1")]
    V1,
}

/// Persisted version of a compact occurrence batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OccurrenceBatchSchema {
    #[serde(rename = "karma.occurrence-batch.v1")]
    V1,
}

/// Whether a covered range represents each boundary or one coalesced occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OccurrenceBatchEmission {
    Individual,
    Coalesced,
}

/// A validated range of elapsed schedule boundaries emitted together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OccurrenceBatch {
    pub schema: OccurrenceBatchSchema,
    pub activation_hash: CanonicalHash,
    pub batch_sequence: u64,
    pub emission: OccurrenceBatchEmission,
    pub first_schedule_ordinal: u64,
    pub range: OccurrenceRange,
}

#[derive(Deserialize)]
struct OccurrenceBatchWire {
    schema: OccurrenceBatchSchema,
    activation_hash: CanonicalHash,
    batch_sequence: u64,
    emission: OccurrenceBatchEmission,
    first_schedule_ordinal: u64,
    range: OccurrenceRange,
}

impl OccurrenceBatch {
    pub fn new(
        activation_hash: CanonicalHash,
        batch_sequence: u64,
        schedule: &ElapsedSchedule,
        emission: &ScheduleEmission,
    ) -> Result<Self, KarmaBoundaryError> {
        let (emission_kind, range) = emission_parts(emission);
        if range.interval_ms() != schedule.interval_ms() || range.first() < schedule.anchor() {
            return Err(KarmaBoundaryError::invalid_input(
                "occurrence batch range is not on its elapsed schedule lattice",
            ));
        }
        let delta = range
            .first()
            .as_millis()
            .abs_diff(schedule.anchor().as_millis());
        if !delta.is_multiple_of(schedule.interval_ms()) {
            return Err(KarmaBoundaryError::invalid_input(
                "occurrence batch first boundary is not on its elapsed schedule lattice",
            ));
        }
        let batch = Self {
            schema: OccurrenceBatchSchema::V1,
            activation_hash,
            batch_sequence,
            emission: emission_kind,
            first_schedule_ordinal: delta / schedule.interval_ms(),
            range: range.clone(),
        };
        batch.validate_structure()?;
        Ok(batch)
    }

    /// Hashes this batch's canonical content.
    ///
    /// # Errors
    ///
    /// Returns a boundary error if the batch cannot be canonically encoded.
    pub fn batch_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(OCCURRENCE_BATCH_HASH_DOMAIN, self)
    }

    pub const fn covered_boundary_count(&self) -> u64 {
        self.range.count()
    }

    pub const fn semantic_occurrence_count(&self) -> u64 {
        match self.emission {
            OccurrenceBatchEmission::Individual => self.range.count(),
            OccurrenceBatchEmission::Coalesced => 1,
        }
    }

    /// Reconstructs the timer-independent tick at one batch ordinal.
    ///
    /// # Errors
    ///
    /// Returns a boundary error for coalesced batches, invalid ordinals, or overflow.
    pub fn individual_tick(
        &self,
        batch_ordinal: u64,
    ) -> Result<SemanticScheduleTick, KarmaBoundaryError> {
        if self.emission != OccurrenceBatchEmission::Individual {
            return Err(KarmaBoundaryError::invalid_input(
                "coalesced occurrence batches do not expose individual semantic ticks",
            ));
        }
        if batch_ordinal >= self.range.count() {
            return Err(KarmaBoundaryError::invalid_input(
                "semantic tick ordinal is outside the occurrence batch",
            ));
        }
        let offset = self
            .range
            .interval_ms()
            .checked_mul(batch_ordinal)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| KarmaBoundaryError::invalid_input("semantic tick offset overflows"))?;
        let intended_at = self
            .range
            .first()
            .checked_add(DurationMs::new(offset))
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_input("semantic tick timestamp overflows")
            })?;
        let schedule_ordinal = self
            .first_schedule_ordinal
            .checked_add(batch_ordinal)
            .ok_or_else(|| KarmaBoundaryError::invalid_input("semantic tick ordinal overflows"))?;
        SemanticScheduleTick::new(self.activation_hash.clone(), schedule_ordinal, intended_at)
    }

    /// Reconstructs a bounded page, returning an empty page beyond the range.
    ///
    /// # Errors
    ///
    /// Returns a boundary error for an excessive page size or a tick that cannot be reconstructed.
    pub fn individual_page(
        &self,
        start_batch_ordinal: u64,
        limit: NonZeroU32,
    ) -> Result<Vec<SemanticScheduleTick>, KarmaBoundaryError> {
        if limit.get() > MAX_OCCURRENCE_BATCH_PAGE_TICKS {
            return Err(KarmaBoundaryError::invalid_input(format!(
                "occurrence batch page limit exceeds {MAX_OCCURRENCE_BATCH_PAGE_TICKS} ticks"
            )));
        }
        if start_batch_ordinal >= self.range.count() {
            return Ok(Vec::new());
        }
        let end = start_batch_ordinal
            .saturating_add(u64::from(limit.get()))
            .min(self.range.count());
        (start_batch_ordinal..end)
            .map(|ordinal| self.individual_tick(ordinal))
            .collect()
    }

    fn validate_envelope(
        &self,
        activation_hash: &CanonicalHash,
        batch_sequence: u64,
        emission: &ScheduleEmission,
    ) -> Result<(), KarmaBoundaryError> {
        self.validate_structure()?;
        let (emission_kind, range) = emission_parts(emission);
        if &self.activation_hash != activation_hash
            || self.batch_sequence != batch_sequence
            || self.emission != emission_kind
            || &self.range != range
        {
            return Err(KarmaBoundaryError::invalid_input(
                "occurrence batch disagrees with its schedule occurrence envelope",
            ));
        }
        Ok(())
    }

    fn validate_structure(&self) -> Result<(), KarmaBoundaryError> {
        if self.schema != OccurrenceBatchSchema::V1 || self.batch_sequence == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "occurrence batch schema must be supported and sequence must be positive",
            ));
        }
        self.first_schedule_ordinal
            .checked_add(self.range.count() - 1)
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_input("occurrence batch ordinal overflows")
            })?;
        Ok(())
    }
}

impl<'de> Deserialize<'de> for OccurrenceBatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = OccurrenceBatchWire::deserialize(deserializer)?;
        let batch = Self {
            schema: wire.schema,
            activation_hash: wire.activation_hash,
            batch_sequence: wire.batch_sequence,
            emission: wire.emission,
            first_schedule_ordinal: wire.first_schedule_ordinal,
            range: wire.range,
        };
        batch.validate_structure().map_err(de::Error::custom)?;
        Ok(batch)
    }
}

fn emission_parts(emission: &ScheduleEmission) -> (OccurrenceBatchEmission, &OccurrenceRange) {
    match emission {
        ScheduleEmission::Individual(range) => (OccurrenceBatchEmission::Individual, range),
        ScheduleEmission::Coalesced(range) => (OccurrenceBatchEmission::Coalesced, range),
    }
}

/// Persisted version of an individual tick identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SemanticScheduleTickSchema {
    #[serde(rename = "karma.semantic-schedule-tick.v1")]
    V1,
}

/// One intended boundary identified independently of the host wake or batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SemanticScheduleTickWire")]
pub struct SemanticScheduleTick {
    pub schema: SemanticScheduleTickSchema,
    pub activation_hash: CanonicalHash,
    pub schedule_ordinal: u64,
    pub intended_at: TimestampMs,
}

#[derive(Deserialize)]
struct SemanticScheduleTickWire {
    schema: SemanticScheduleTickSchema,
    activation_hash: CanonicalHash,
    schedule_ordinal: u64,
    intended_at: TimestampMs,
}

impl SemanticScheduleTick {
    pub fn new(
        activation_hash: CanonicalHash,
        schedule_ordinal: u64,
        intended_at: TimestampMs,
    ) -> Result<Self, KarmaBoundaryError> {
        Ok(Self {
            schema: SemanticScheduleTickSchema::V1,
            activation_hash,
            schedule_ordinal,
            intended_at,
        })
    }

    pub fn tick_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(SEMANTIC_SCHEDULE_TICK_HASH_DOMAIN, self)
    }
}

impl TryFrom<SemanticScheduleTickWire> for SemanticScheduleTick {
    type Error = String;

    fn try_from(wire: SemanticScheduleTickWire) -> Result<Self, Self::Error> {
        if wire.schema != SemanticScheduleTickSchema::V1 {
            return Err("unsupported semantic schedule tick schema".into());
        }
        Self::new(
            wire.activation_hash,
            wire.schedule_ordinal,
            wire.intended_at,
        )
        .map_err(|error| error.to_string())
    }
}

/// Fenced record of an elapsed schedule advance and its emitted batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElapsedScheduleOccurrence {
    pub schema: ElapsedScheduleOccurrenceSchema,
    pub activation_hash: CanonicalHash,
    pub sequence: u64,
    pub claimed_cursor_revision: u64,
    pub lease_fencing_token: u64,
    pub observed_at: TimestampMs,
    pub batch: OccurrenceBatch,
    pub advance: ScheduleAdvance,
}

#[derive(Deserialize)]
struct ElapsedScheduleOccurrenceWire {
    schema: ElapsedScheduleOccurrenceSchema,
    activation_hash: CanonicalHash,
    sequence: u64,
    claimed_cursor_revision: u64,
    lease_fencing_token: u64,
    observed_at: TimestampMs,
    batch: OccurrenceBatch,
    advance: ScheduleAdvance,
}

impl ElapsedScheduleOccurrence {
    pub fn new(
        activation_hash: CanonicalHash,
        sequence: u64,
        claimed_cursor_revision: u64,
        lease_fencing_token: u64,
        observed_at: TimestampMs,
        batch: OccurrenceBatch,
        advance: ScheduleAdvance,
    ) -> Result<Self, KarmaBoundaryError> {
        if sequence == 0 || claimed_cursor_revision == 0 || lease_fencing_token == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "schedule occurrence sequence, cursor revision, and fencing token must be positive",
            ));
        }
        let Some(emission) = advance
            .emission
            .as_ref()
            .filter(|_| advance.observed_at == observed_at)
        else {
            return Err(KarmaBoundaryError::invalid_input(
                "schedule occurrence requires an emitted advance at the same observed time",
            ));
        };
        batch.validate_envelope(&activation_hash, sequence, emission)?;
        Ok(Self {
            schema: ElapsedScheduleOccurrenceSchema::V1,
            activation_hash,
            sequence,
            claimed_cursor_revision,
            lease_fencing_token,
            observed_at,
            batch,
            advance,
        })
    }

    pub fn occurrence_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(ELAPSED_SCHEDULE_OCCURRENCE_HASH_DOMAIN, self)
    }
}

impl<'de> Deserialize<'de> for ElapsedScheduleOccurrence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ElapsedScheduleOccurrenceWire::deserialize(deserializer)?;
        if wire.schema != ElapsedScheduleOccurrenceSchema::V1 {
            return Err(de::Error::custom(
                "unsupported elapsed schedule occurrence schema",
            ));
        }
        Self::new(
            wire.activation_hash,
            wire.sequence,
            wire.claimed_cursor_revision,
            wire.lease_fencing_token,
            wire.observed_at,
            wire.batch,
            wire.advance,
        )
        .map_err(de::Error::custom)
    }
}
