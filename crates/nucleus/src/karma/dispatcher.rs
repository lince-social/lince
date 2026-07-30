use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    num::{NonZeroU32, NonZeroUsize},
};

use serde::{Deserialize, Deserializer, Serialize, de};

use super::{
    CalendarSchedule, CanonicalHash, DurationMs, ElapsedSchedule, KarmaBoundaryError,
    OverloadPolicy, RationalRate, ScheduleAdvance, ScheduleEmission, TimeZoneProvider, TimerPolicy,
    TimestampMs, canonical_hash,
};

pub const ELAPSED_SCHEDULE_OCCURRENCE_HASH_DOMAIN: &str = "karma.elapsed-schedule-occurrence.v1";
pub const OCCURRENCE_BATCH_HASH_DOMAIN: &str = "karma.occurrence-batch.v1";
pub const SEMANTIC_SCHEDULE_TICK_HASH_DOMAIN: &str = "karma.semantic-schedule-tick.v1";
pub const MAX_OCCURRENCE_BATCH_PAGE_TICKS: u32 = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScheduleCursorLifecycle {
    Armed,
    Leased,
    Paused,
    Superseded,
    Failed,
}

impl ScheduleCursorLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Armed => "armed",
            Self::Leased => "leased",
            Self::Paused => "paused",
            Self::Superseded => "superseded",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "armed" => Self::Armed,
            "leased" => Self::Leased,
            "paused" => Self::Paused,
            "superseded" => Self::Superseded,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ElapsedScheduleOccurrenceSchema {
    #[serde(rename = "karma.elapsed-schedule-occurrence.v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OccurrenceBatchSchema {
    #[serde(rename = "karma.occurrence-batch.v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OccurrenceBatchEmission {
    Individual,
    Coalesced,
}

/// Compact, lossless representation of elapsed boundaries emitted by one
/// fenced cursor completion. `first_schedule_ordinal` is relative to the
/// immutable schedule anchor, so semantic tick identity does not depend on how
/// host wakes happened to divide the same lattice into batches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OccurrenceBatch {
    pub schema: OccurrenceBatchSchema,
    pub activation_hash: CanonicalHash,
    pub batch_sequence: u64,
    pub emission: OccurrenceBatchEmission,
    pub first_schedule_ordinal: u64,
    pub range: super::OccurrenceRange,
}

#[derive(Deserialize)]
struct OccurrenceBatchWire {
    schema: OccurrenceBatchSchema,
    activation_hash: CanonicalHash,
    batch_sequence: u64,
    emission: OccurrenceBatchEmission,
    first_schedule_ordinal: u64,
    range: super::OccurrenceRange,
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
        let delta = u64::try_from(range.first().as_millis() - schedule.anchor().as_millis())
            .expect("ordered timestamp difference is non-negative");
        if delta % schedule.interval_ms() != 0 {
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

    /// Reconstruct one individually emitted tick without allocating the whole
    /// range. Coalesced batches deliberately have one aggregate occurrence and
    /// cannot be expanded through this method.
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

    /// Expand at most one deterministic work page. A caller must persist its
    /// next batch ordinal between pages; requesting an unbounded allocation is
    /// rejected even if the compact range itself contains billions of ticks.
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

fn emission_parts(
    emission: &ScheduleEmission,
) -> (OccurrenceBatchEmission, &super::OccurrenceRange) {
    match emission {
        ScheduleEmission::Individual(range) => (OccurrenceBatchEmission::Individual, range),
        ScheduleEmission::Coalesced(range) => (OccurrenceBatchEmission::Coalesced, range),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SemanticScheduleTickSchema {
    #[serde(rename = "karma.semantic-schedule-tick.v1")]
    V1,
}

/// Segmentation-independent identity for one individually emitted elapsed
/// boundary. Retries and different host wake grouping reproduce the same hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

impl<'de> Deserialize<'de> for SemanticScheduleTick {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SemanticScheduleTickWire::deserialize(deserializer)?;
        if wire.schema != SemanticScheduleTickSchema::V1 {
            return Err(de::Error::custom(
                "unsupported semantic schedule tick schema",
            ));
        }
        Self::new(
            wire.activation_hash,
            wire.schedule_ordinal,
            wire.intended_at,
        )
        .map_err(de::Error::custom)
    }
}

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
        let emission = advance.emission.as_ref().ok_or_else(|| {
            KarmaBoundaryError::invalid_input(
                "schedule occurrence requires an emitted advance at the same observed time",
            )
        })?;
        if advance.observed_at != observed_at {
            return Err(KarmaBoundaryError::invalid_input(
                "schedule occurrence requires an emitted advance at the same observed time",
            ));
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadlineEntry {
    activation_hash: CanonicalHash,
    cursor_revision: u64,
    next_intended_at: TimestampMs,
    required_resolution_ms: NonZeroU32,
    max_lateness_ms: u32,
    coalesce_window_ms: u32,
    overload: OverloadPolicy,
}

#[derive(Deserialize)]
struct DeadlineEntryWire {
    activation_hash: CanonicalHash,
    cursor_revision: u64,
    next_intended_at: TimestampMs,
    required_resolution_ms: NonZeroU32,
    max_lateness_ms: u32,
    coalesce_window_ms: u32,
    overload: OverloadPolicy,
}

impl DeadlineEntry {
    pub fn new(
        activation_hash: CanonicalHash,
        cursor_revision: u64,
        next_intended_at: TimestampMs,
        timer: TimerPolicy,
        overload: OverloadPolicy,
    ) -> Result<Self, KarmaBoundaryError> {
        let required_resolution_ms = NonZeroU32::new(timer.required_resolution_ms())
            .expect("TimerPolicy guarantees non-zero resolution");
        let entry = Self {
            activation_hash,
            cursor_revision,
            next_intended_at,
            required_resolution_ms,
            max_lateness_ms: timer.max_lateness_ms(),
            coalesce_window_ms: timer.coalesce_window_ms(),
            overload,
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn activation_hash(&self) -> &CanonicalHash {
        &self.activation_hash
    }

    pub const fn cursor_revision(&self) -> u64 {
        self.cursor_revision
    }

    pub const fn next_intended_at(&self) -> TimestampMs {
        self.next_intended_at
    }

    pub const fn required_resolution_ms(&self) -> NonZeroU32 {
        self.required_resolution_ms
    }

    pub const fn max_lateness_ms(&self) -> u32 {
        self.max_lateness_ms
    }

    pub const fn coalesce_window_ms(&self) -> u32 {
        self.coalesce_window_ms
    }

    pub const fn overload_policy(&self) -> OverloadPolicy {
        self.overload
    }

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.cursor_revision == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "deadline cursor revision must be positive",
            ));
        }
        if self.coalesce_window_ms > self.max_lateness_ms {
            return Err(KarmaBoundaryError::invalid_input(
                "deadline coalescing window cannot exceed maximum lateness",
            ));
        }
        self.next_intended_at
            .checked_add(super::DurationMs::new(i64::from(self.coalesce_window_ms)))
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_input("deadline arm window overflows timestamp")
            })?;
        Ok(())
    }
}

impl<'de> Deserialize<'de> for DeadlineEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DeadlineEntryWire::deserialize(deserializer)?;
        let entry = Self {
            activation_hash: wire.activation_hash,
            cursor_revision: wire.cursor_revision,
            next_intended_at: wire.next_intended_at,
            required_resolution_ms: wire.required_resolution_ms,
            max_lateness_ms: wire.max_lateness_ms,
            coalesce_window_ms: wire.coalesce_window_ms,
            overload: wire.overload,
        };
        entry.validate().map_err(de::Error::custom)?;
        Ok(entry)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostTimerCapabilities {
    supported_resolution_ms: BTreeSet<NonZeroU32>,
    maximum_armed_entries: NonZeroUsize,
}

#[derive(Deserialize)]
struct HostTimerCapabilitiesWire {
    supported_resolution_ms: BTreeSet<NonZeroU32>,
    maximum_armed_entries: NonZeroUsize,
}

impl HostTimerCapabilities {
    pub fn new(
        supported_resolution_ms: impl IntoIterator<Item = NonZeroU32>,
        maximum_armed_entries: NonZeroUsize,
    ) -> Result<Self, KarmaBoundaryError> {
        let supported_resolution_ms = supported_resolution_ms.into_iter().collect::<BTreeSet<_>>();
        if supported_resolution_ms.is_empty() {
            return Err(KarmaBoundaryError::invalid_input(
                "host timer must expose at least one supported resolution",
            ));
        }
        Ok(Self {
            supported_resolution_ms,
            maximum_armed_entries,
        })
    }

    pub fn supported_resolutions(&self) -> &BTreeSet<NonZeroU32> {
        &self.supported_resolution_ms
    }

    pub const fn maximum_armed_entries(&self) -> NonZeroUsize {
        self.maximum_armed_entries
    }
}

impl<'de> Deserialize<'de> for HostTimerCapabilities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = HostTimerCapabilitiesWire::deserialize(deserializer)?;
        Self::new(wire.supported_resolution_ms, wire.maximum_armed_entries)
            .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatcherResourceGrant {
    finest_resolution_ms: NonZeroU32,
    maximum_lanes: NonZeroUsize,
    maximum_entries: NonZeroUsize,
    allow_bounded_degradation: bool,
}

impl DispatcherResourceGrant {
    pub const fn new(
        finest_resolution_ms: NonZeroU32,
        maximum_lanes: NonZeroUsize,
        maximum_entries: NonZeroUsize,
        allow_bounded_degradation: bool,
    ) -> Self {
        Self {
            finest_resolution_ms,
            maximum_lanes,
            maximum_entries,
            allow_bounded_degradation,
        }
    }

    pub const fn finest_resolution_ms(&self) -> NonZeroU32 {
        self.finest_resolution_ms
    }
}

/// Conservative work reachable from one semantic schedule tick. Counts are
/// upper bounds, not observed averages; at least one write accounts for cursor
/// advancement/occurrence evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScheduleWorkloadUpperBounds {
    evaluator_fuel_per_tick: u64,
    writes_per_tick: NonZeroU32,
    effects_per_tick: u32,
    trace_bytes_per_tick: u64,
}

#[derive(Deserialize)]
struct ScheduleWorkloadUpperBoundsWire {
    evaluator_fuel_per_tick: u64,
    writes_per_tick: NonZeroU32,
    effects_per_tick: u32,
    trace_bytes_per_tick: u64,
}

impl ScheduleWorkloadUpperBounds {
    pub const fn new(
        evaluator_fuel_per_tick: u64,
        writes_per_tick: NonZeroU32,
        effects_per_tick: u32,
        trace_bytes_per_tick: u64,
    ) -> Self {
        Self {
            evaluator_fuel_per_tick,
            writes_per_tick,
            effects_per_tick,
            trace_bytes_per_tick,
        }
    }

    pub const fn evaluator_fuel_per_tick(self) -> u64 {
        self.evaluator_fuel_per_tick
    }

    pub const fn writes_per_tick(self) -> NonZeroU32 {
        self.writes_per_tick
    }

    pub const fn effects_per_tick(self) -> u32 {
        self.effects_per_tick
    }

    pub const fn trace_bytes_per_tick(self) -> u64 {
        self.trace_bytes_per_tick
    }
}

impl<'de> Deserialize<'de> for ScheduleWorkloadUpperBounds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ScheduleWorkloadUpperBoundsWire::deserialize(deserializer)?;
        Ok(Self::new(
            wire.evaluator_fuel_per_tick,
            wire.writes_per_tick,
            wire.effects_per_tick,
            wire.trace_bytes_per_tick,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerCalibration {
    fire_cpu_ns_per_wake: NonZeroU32,
}

impl SchedulerCalibration {
    pub const fn new(fire_cpu_ns_per_wake: NonZeroU32) -> Self {
        Self {
            fire_cpu_ns_per_wake,
        }
    }

    pub const fn fire_cpu_ns_per_wake(self) -> NonZeroU32 {
        self.fire_cpu_ns_per_wake
    }
}

/// Exact upper-bound demand for one schedule. Derived rates are calculated
/// from reduced rationals and checked multiplication; no float participates in
/// admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScheduleDemand {
    semantic_ticks_per_second: RationalRate,
    timer_wakes_per_second: RationalRate,
    required_resolution_ms: NonZeroU32,
    workload: ScheduleWorkloadUpperBounds,
    calibration: SchedulerCalibration,
}

#[derive(Deserialize)]
struct ScheduleDemandWire {
    semantic_ticks_per_second: RationalRate,
    timer_wakes_per_second: RationalRate,
    required_resolution_ms: NonZeroU32,
    workload: ScheduleWorkloadUpperBounds,
    calibration: SchedulerCalibration,
}

impl ScheduleDemand {
    pub fn for_elapsed(
        schedule: &ElapsedSchedule,
        workload: ScheduleWorkloadUpperBounds,
        calibration: SchedulerCalibration,
    ) -> Result<Self, KarmaBoundaryError> {
        let semantic_ticks_per_second = schedule.semantic_rate();
        Self::new(
            semantic_ticks_per_second,
            semantic_ticks_per_second,
            NonZeroU32::new(schedule.timer().required_resolution_ms())
                .expect("TimerPolicy validates non-zero resolution"),
            workload,
            calibration,
        )
    }

    pub fn for_calendar(
        schedule: &CalendarSchedule,
        provider: &dyn TimeZoneProvider,
        workload: ScheduleWorkloadUpperBounds,
        calibration: SchedulerCalibration,
    ) -> Result<Self, KarmaBoundaryError> {
        if provider.revision() != &schedule.tzdb {
            return Err(KarmaBoundaryError::invalid_definition(
                "calendar demand provider revision does not match schedule",
            ));
        }
        let minimum_interval_ms = provider.minimum_interval_ms(schedule)?;
        let semantic_ticks_per_second = RationalRate::new(1_000, minimum_interval_ms.get())?;
        Self::new(
            semantic_ticks_per_second,
            semantic_ticks_per_second,
            NonZeroU32::new(schedule.timer.required_resolution_ms())
                .expect("TimerPolicy validates non-zero resolution"),
            workload,
            calibration,
        )
    }

    fn new(
        semantic_ticks_per_second: RationalRate,
        timer_wakes_per_second: RationalRate,
        required_resolution_ms: NonZeroU32,
        workload: ScheduleWorkloadUpperBounds,
        calibration: SchedulerCalibration,
    ) -> Result<Self, KarmaBoundaryError> {
        if timer_wakes_per_second > semantic_ticks_per_second {
            return Err(KarmaBoundaryError::invalid_input(
                "timer wake rate cannot exceed semantic schedule rate",
            ));
        }
        let demand = Self {
            semantic_ticks_per_second,
            timer_wakes_per_second,
            required_resolution_ms,
            workload,
            calibration,
        };
        demand.derived_rates()?;
        Ok(demand)
    }

    pub const fn semantic_ticks_per_second(self) -> RationalRate {
        self.semantic_ticks_per_second
    }

    pub const fn timer_wakes_per_second(self) -> RationalRate {
        self.timer_wakes_per_second
    }

    pub const fn required_resolution_ms(self) -> NonZeroU32 {
        self.required_resolution_ms
    }

    pub const fn workload(self) -> ScheduleWorkloadUpperBounds {
        self.workload
    }

    pub const fn calibration(self) -> SchedulerCalibration {
        self.calibration
    }

    pub fn derived_rates(self) -> Result<ScheduleDemandRates, KarmaBoundaryError> {
        Ok(ScheduleDemandRates {
            semantic_ticks_per_second: self.semantic_ticks_per_second,
            timer_wakes_per_second: self.timer_wakes_per_second,
            scheduler_cpu_ns_per_second: checked_rate_product(
                self.timer_wakes_per_second,
                u64::from(self.calibration.fire_cpu_ns_per_wake.get()),
                "scheduler CPU rate",
            )?,
            evaluator_fuel_per_second: checked_rate_product(
                self.semantic_ticks_per_second,
                self.workload.evaluator_fuel_per_tick,
                "evaluator fuel rate",
            )?,
            writes_per_second: checked_rate_product(
                self.semantic_ticks_per_second,
                u64::from(self.workload.writes_per_tick.get()),
                "write rate",
            )?,
            effects_per_second: checked_rate_product(
                self.semantic_ticks_per_second,
                u64::from(self.workload.effects_per_tick),
                "effect rate",
            )?,
            trace_bytes_per_second: checked_rate_product(
                self.semantic_ticks_per_second,
                self.workload.trace_bytes_per_tick,
                "trace byte rate",
            )?,
        })
    }
}

impl<'de> Deserialize<'de> for ScheduleDemand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ScheduleDemandWire::deserialize(deserializer)?;
        Self::new(
            wire.semantic_ticks_per_second,
            wire.timer_wakes_per_second,
            wire.required_resolution_ms,
            wire.workload,
            wire.calibration,
        )
        .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleDemandRates {
    pub semantic_ticks_per_second: RationalRate,
    pub timer_wakes_per_second: RationalRate,
    pub scheduler_cpu_ns_per_second: RationalRate,
    pub evaluator_fuel_per_second: RationalRate,
    pub writes_per_second: RationalRate,
    pub effects_per_second: RationalRate,
    pub trace_bytes_per_second: RationalRate,
}

impl ScheduleDemandRates {
    pub fn zero() -> Self {
        let zero = RationalRate::new(0, 1).expect("one is a valid denominator");
        Self {
            semantic_ticks_per_second: zero,
            timer_wakes_per_second: zero,
            scheduler_cpu_ns_per_second: zero,
            evaluator_fuel_per_second: zero,
            writes_per_second: zero,
            effects_per_second: zero,
            trace_bytes_per_second: zero,
        }
    }

    pub fn checked_add(self, demand: ScheduleDemand) -> Result<Self, KarmaBoundaryError> {
        let rhs = demand.derived_rates()?;
        Ok(Self {
            semantic_ticks_per_second: checked_rate_sum(
                self.semantic_ticks_per_second,
                rhs.semantic_ticks_per_second,
                "semantic tick rate",
            )?,
            timer_wakes_per_second: checked_rate_sum(
                self.timer_wakes_per_second,
                rhs.timer_wakes_per_second,
                "timer wake rate",
            )?,
            scheduler_cpu_ns_per_second: checked_rate_sum(
                self.scheduler_cpu_ns_per_second,
                rhs.scheduler_cpu_ns_per_second,
                "scheduler CPU rate",
            )?,
            evaluator_fuel_per_second: checked_rate_sum(
                self.evaluator_fuel_per_second,
                rhs.evaluator_fuel_per_second,
                "evaluator fuel rate",
            )?,
            writes_per_second: checked_rate_sum(
                self.writes_per_second,
                rhs.writes_per_second,
                "write rate",
            )?,
            effects_per_second: checked_rate_sum(
                self.effects_per_second,
                rhs.effects_per_second,
                "effect rate",
            )?,
            trace_bytes_per_second: checked_rate_sum(
                self.trace_bytes_per_second,
                rhs.trace_bytes_per_second,
                "trace byte rate",
            )?,
        })
    }

    pub fn exceeded_resources(self, capacity: ScheduleDemandCapacity) -> Vec<DemandResource> {
        let checks = [
            (
                DemandResource::SemanticTicks,
                self.semantic_ticks_per_second,
                capacity.semantic_ticks_per_second,
            ),
            (
                DemandResource::TimerWakes,
                self.timer_wakes_per_second,
                capacity.timer_wakes_per_second,
            ),
            (
                DemandResource::SchedulerCpu,
                self.scheduler_cpu_ns_per_second,
                capacity.scheduler_cpu_ns_per_second,
            ),
            (
                DemandResource::EvaluatorFuel,
                self.evaluator_fuel_per_second,
                capacity.evaluator_fuel_per_second,
            ),
            (
                DemandResource::Writes,
                self.writes_per_second,
                capacity.writes_per_second,
            ),
            (
                DemandResource::Effects,
                self.effects_per_second,
                capacity.effects_per_second,
            ),
            (
                DemandResource::TraceBytes,
                self.trace_bytes_per_second,
                capacity.trace_bytes_per_second,
            ),
        ];
        checks
            .into_iter()
            .filter_map(|(resource, demand, limit)| (demand > limit).then_some(resource))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleDemandCapacity {
    pub semantic_ticks_per_second: RationalRate,
    pub timer_wakes_per_second: RationalRate,
    pub scheduler_cpu_ns_per_second: RationalRate,
    pub evaluator_fuel_per_second: RationalRate,
    pub writes_per_second: RationalRate,
    pub effects_per_second: RationalRate,
    pub trace_bytes_per_second: RationalRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DemandResource {
    SemanticTicks,
    TimerWakes,
    SchedulerCpu,
    EvaluatorFuel,
    Writes,
    Effects,
    TraceBytes,
}

fn checked_rate_product(
    rate: RationalRate,
    multiplier: u64,
    label: &str,
) -> Result<RationalRate, KarmaBoundaryError> {
    rate.checked_mul_u64(multiplier)
        .ok_or_else(|| KarmaBoundaryError::invalid_input(format!("{label} overflowed")))
}

fn checked_rate_sum(
    left: RationalRate,
    right: RationalRate,
    label: &str,
) -> Result<RationalRate, KarmaBoundaryError> {
    left.checked_add(right)
        .ok_or_else(|| KarmaBoundaryError::invalid_input(format!("{label} overflowed")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeadlineRejectionReason {
    DuplicateActivation,
    UnsupportedResolution,
    EntryLimit,
    LaneLimit,
    DegradationExceedsLateness,
    SemanticRateLimit,
    WakeRateLimit,
    SchedulerCpuLimit,
    EvaluatorFuelLimit,
    WriteRateLimit,
    EffectRateLimit,
    TraceRateLimit,
    DemandArithmeticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum DeadlineAdmission {
    Admitted {
        activation_hash: CanonicalHash,
        lane_resolution_ms: NonZeroU32,
        degraded: bool,
    },
    Rejected {
        activation_hash: CanonicalHash,
        reason: DeadlineRejectionReason,
    },
    Paused {
        activation_hash: CanonicalHash,
        reason: DeadlineRejectionReason,
    },
}

#[derive(Debug, Clone)]
pub struct DeadlinePlan {
    pub admissions: Vec<DeadlineAdmission>,
    pub index: DeadlineIndex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DemandedDeadline {
    pub entry: DeadlineEntry,
    pub demand: ScheduleDemand,
}

impl DemandedDeadline {
    pub fn new(entry: DeadlineEntry, demand: ScheduleDemand) -> Result<Self, KarmaBoundaryError> {
        if entry.required_resolution_ms() != demand.required_resolution_ms() {
            return Err(KarmaBoundaryError::invalid_input(
                "deadline and demand required resolutions disagree",
            ));
        }
        Ok(Self { entry, demand })
    }
}

#[derive(Deserialize)]
struct DemandedDeadlineWire {
    entry: DeadlineEntry,
    demand: ScheduleDemand,
}

impl<'de> Deserialize<'de> for DemandedDeadline {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DemandedDeadlineWire::deserialize(deserializer)?;
        Self::new(wire.entry, wire.demand).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone)]
pub struct DemandedDeadlinePlan {
    pub admissions: Vec<DeadlineAdmission>,
    pub index: DeadlineIndex,
    pub admitted_demand: ScheduleDemandRates,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlinePlanError {
    pub activation_hash: CanonicalHash,
    pub reason: DeadlineRejectionReason,
}

impl std::fmt::Display for DeadlinePlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "deadline {} could not be admitted: {:?}",
            self.activation_hash.as_str(),
            self.reason
        )
    }
}

impl std::error::Error for DeadlinePlanError {}

pub fn plan_deadlines(
    entries: impl IntoIterator<Item = DeadlineEntry>,
    host: &HostTimerCapabilities,
    grant: &DispatcherResourceGrant,
) -> Result<DeadlinePlan, DeadlinePlanError> {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.next_intended_at
            .cmp(&right.next_intended_at)
            .then_with(|| left.activation_hash.cmp(&right.activation_hash))
            .then_with(|| left.cursor_revision.cmp(&right.cursor_revision))
    });
    let mut seen = BTreeSet::new();
    let mut admissions = Vec::with_capacity(entries.len());
    let mut index = DeadlineIndex::default();
    let entry_limit = host
        .maximum_armed_entries
        .get()
        .min(grant.maximum_entries.get());

    for entry in entries {
        if !seen.insert(entry.activation_hash.clone()) {
            return Err(DeadlinePlanError {
                activation_hash: entry.activation_hash,
                reason: DeadlineRejectionReason::DuplicateActivation,
            });
        }
        if index.len() >= entry_limit {
            admissions.push(overload_admission(
                &entry,
                DeadlineRejectionReason::EntryLimit,
            )?);
            continue;
        }
        let direct = host
            .supported_resolution_ms
            .iter()
            .copied()
            .filter(|resolution| {
                resolution.get() >= grant.finest_resolution_ms.get()
                    && resolution.get() <= entry.required_resolution_ms.get()
            })
            .max();
        let (mut lane_resolution_ms, degraded) = if let Some(resolution) = direct {
            (resolution, false)
        } else if entry.overload == OverloadPolicy::DegradeWithinGrant
            && grant.allow_bounded_degradation
        {
            let degraded = host
                .supported_resolution_ms
                .iter()
                .copied()
                .filter(|resolution| resolution.get() >= grant.finest_resolution_ms.get())
                .min();
            let Some(resolution) = degraded else {
                admissions.push(overload_admission(
                    &entry,
                    DeadlineRejectionReason::UnsupportedResolution,
                )?);
                continue;
            };
            let excess = resolution
                .get()
                .saturating_sub(entry.required_resolution_ms.get());
            if excess > entry.max_lateness_ms {
                admissions.push(overload_admission(
                    &entry,
                    DeadlineRejectionReason::DegradationExceedsLateness,
                )?);
                continue;
            }
            (resolution, true)
        } else {
            admissions.push(overload_admission(
                &entry,
                DeadlineRejectionReason::UnsupportedResolution,
            )?);
            continue;
        };

        if !index.lanes.contains_key(&lane_resolution_ms)
            && index.lane_count() >= grant.maximum_lanes.get()
        {
            let compatible_existing = index
                .lanes
                .keys()
                .copied()
                .filter(|resolution| {
                    resolution.get() >= grant.finest_resolution_ms.get()
                        && resolution.get() <= entry.required_resolution_ms.get()
                })
                .max();
            if let Some(existing) = compatible_existing {
                lane_resolution_ms = existing;
            } else {
                admissions.push(overload_admission(
                    &entry,
                    DeadlineRejectionReason::LaneLimit,
                )?);
                continue;
            }
        }
        let activation_hash = entry.activation_hash.clone();
        index.upsert(ArmedDeadline {
            entry,
            lane_resolution_ms,
            degraded,
        });
        admissions.push(DeadlineAdmission::Admitted {
            activation_hash,
            lane_resolution_ms,
            degraded,
        });
    }
    Ok(DeadlinePlan { admissions, index })
}

pub fn plan_demanded_deadlines(
    entries: impl IntoIterator<Item = DemandedDeadline>,
    host: &HostTimerCapabilities,
    grant: &DispatcherResourceGrant,
    capacity: ScheduleDemandCapacity,
) -> Result<DemandedDeadlinePlan, DeadlinePlanError> {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.entry
            .next_intended_at
            .cmp(&right.entry.next_intended_at)
            .then_with(|| left.entry.activation_hash.cmp(&right.entry.activation_hash))
            .then_with(|| left.entry.cursor_revision.cmp(&right.entry.cursor_revision))
    });
    let mut seen = BTreeSet::new();
    let mut admissions = Vec::with_capacity(entries.len());
    let mut index = DeadlineIndex::default();
    let mut admitted_demand = ScheduleDemandRates::zero();
    let entry_limit = host
        .maximum_armed_entries
        .get()
        .min(grant.maximum_entries.get());

    for demanded in entries {
        let entry = demanded.entry;
        if !seen.insert(entry.activation_hash.clone()) {
            return Err(DeadlinePlanError {
                activation_hash: entry.activation_hash,
                reason: DeadlineRejectionReason::DuplicateActivation,
            });
        }
        if index.len() >= entry_limit {
            admissions.push(overload_admission(
                &entry,
                DeadlineRejectionReason::EntryLimit,
            )?);
            continue;
        }
        let direct = host
            .supported_resolution_ms
            .iter()
            .copied()
            .filter(|resolution| {
                resolution.get() >= grant.finest_resolution_ms.get()
                    && resolution.get() <= entry.required_resolution_ms.get()
            })
            .max();
        let (mut lane_resolution_ms, degraded) = if let Some(resolution) = direct {
            (resolution, false)
        } else if entry.overload == OverloadPolicy::DegradeWithinGrant
            && grant.allow_bounded_degradation
        {
            let degraded = host
                .supported_resolution_ms
                .iter()
                .copied()
                .filter(|resolution| resolution.get() >= grant.finest_resolution_ms.get())
                .min();
            let Some(resolution) = degraded else {
                admissions.push(overload_admission(
                    &entry,
                    DeadlineRejectionReason::UnsupportedResolution,
                )?);
                continue;
            };
            let excess = resolution
                .get()
                .saturating_sub(entry.required_resolution_ms.get());
            if excess > entry.max_lateness_ms {
                admissions.push(overload_admission(
                    &entry,
                    DeadlineRejectionReason::DegradationExceedsLateness,
                )?);
                continue;
            }
            (resolution, true)
        } else {
            admissions.push(overload_admission(
                &entry,
                DeadlineRejectionReason::UnsupportedResolution,
            )?);
            continue;
        };

        if !index.lanes.contains_key(&lane_resolution_ms)
            && index.lane_count() >= grant.maximum_lanes.get()
        {
            let compatible_existing = index
                .lanes
                .keys()
                .copied()
                .filter(|resolution| {
                    resolution.get() >= grant.finest_resolution_ms.get()
                        && resolution.get() <= entry.required_resolution_ms.get()
                })
                .max();
            if let Some(existing) = compatible_existing {
                lane_resolution_ms = existing;
            } else {
                admissions.push(overload_admission(
                    &entry,
                    DeadlineRejectionReason::LaneLimit,
                )?);
                continue;
            }
        }

        let candidate_demand = match admitted_demand.checked_add(demanded.demand) {
            Ok(candidate) => candidate,
            Err(_) => {
                admissions.push(overload_admission(
                    &entry,
                    DeadlineRejectionReason::DemandArithmeticOverflow,
                )?);
                continue;
            }
        };
        if let Some(resource) = candidate_demand
            .exceeded_resources(capacity)
            .into_iter()
            .next()
        {
            admissions.push(overload_admission(
                &entry,
                demand_rejection_reason(resource),
            )?);
            continue;
        }

        let activation_hash = entry.activation_hash.clone();
        index.upsert(ArmedDeadline {
            entry,
            lane_resolution_ms,
            degraded,
        });
        admitted_demand = candidate_demand;
        admissions.push(DeadlineAdmission::Admitted {
            activation_hash,
            lane_resolution_ms,
            degraded,
        });
    }
    Ok(DemandedDeadlinePlan {
        admissions,
        index,
        admitted_demand,
    })
}

const fn demand_rejection_reason(resource: DemandResource) -> DeadlineRejectionReason {
    match resource {
        DemandResource::SemanticTicks => DeadlineRejectionReason::SemanticRateLimit,
        DemandResource::TimerWakes => DeadlineRejectionReason::WakeRateLimit,
        DemandResource::SchedulerCpu => DeadlineRejectionReason::SchedulerCpuLimit,
        DemandResource::EvaluatorFuel => DeadlineRejectionReason::EvaluatorFuelLimit,
        DemandResource::Writes => DeadlineRejectionReason::WriteRateLimit,
        DemandResource::Effects => DeadlineRejectionReason::EffectRateLimit,
        DemandResource::TraceBytes => DeadlineRejectionReason::TraceRateLimit,
    }
}

fn overload_admission(
    entry: &DeadlineEntry,
    reason: DeadlineRejectionReason,
) -> Result<DeadlineAdmission, DeadlinePlanError> {
    match entry.overload {
        OverloadPolicy::RejectActivation => Err(DeadlinePlanError {
            activation_hash: entry.activation_hash.clone(),
            reason,
        }),
        OverloadPolicy::PauseAndAsk | OverloadPolicy::DegradeWithinGrant => {
            Ok(DeadlineAdmission::Paused {
                activation_hash: entry.activation_hash.clone(),
                reason,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmedDeadline {
    pub entry: DeadlineEntry,
    pub lane_resolution_ms: NonZeroU32,
    pub degraded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DeadlineKey {
    next_intended_at: TimestampMs,
    lane_resolution_ms: NonZeroU32,
    activation_hash: CanonicalHash,
    cursor_revision: u64,
}

impl From<&ArmedDeadline> for DeadlineKey {
    fn from(deadline: &ArmedDeadline) -> Self {
        Self {
            next_intended_at: deadline.entry.next_intended_at,
            lane_resolution_ms: deadline.lane_resolution_ms,
            activation_hash: deadline.entry.activation_hash.clone(),
            cursor_revision: deadline.entry.cursor_revision,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeadlineIndex {
    heap: BinaryHeap<Reverse<DeadlineKey>>,
    active: BTreeMap<CanonicalHash, ArmedDeadline>,
    lanes: BTreeMap<NonZeroU32, BTreeSet<CanonicalHash>>,
}

impl DeadlineIndex {
    pub fn upsert(&mut self, deadline: ArmedDeadline) -> Option<ArmedDeadline> {
        let hash = deadline.entry.activation_hash.clone();
        let previous = self.active.insert(hash.clone(), deadline.clone());
        if let Some(previous) = &previous {
            self.remove_lane_member(previous.lane_resolution_ms, &hash);
        }
        self.lanes
            .entry(deadline.lane_resolution_ms)
            .or_default()
            .insert(hash);
        self.heap.push(Reverse(DeadlineKey::from(&deadline)));
        previous
    }

    pub fn remove(&mut self, activation_hash: &CanonicalHash) -> Option<ArmedDeadline> {
        let previous = self.active.remove(activation_hash)?;
        self.remove_lane_member(previous.lane_resolution_ms, activation_hash);
        Some(previous)
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    pub fn lane_sizes(&self) -> BTreeMap<NonZeroU32, usize> {
        self.lanes
            .iter()
            .map(|(resolution, members)| (*resolution, members.len()))
            .collect()
    }

    pub fn next_host_wake(&mut self) -> Option<TimestampMs> {
        self.discard_stale_head();
        self.heap.peek().map(|Reverse(key)| key.next_intended_at)
    }

    pub fn pop_due(&mut self, observed_at: TimestampMs) -> DispatchBatch {
        let mut due = Vec::new();
        let mut stale_discarded = 0_u64;
        loop {
            let Some(Reverse(key)) = self.heap.peek() else {
                break;
            };
            if key.next_intended_at > observed_at {
                break;
            }
            let Reverse(key) = self.heap.pop().expect("peeked deadline exists");
            let current = self.active.get(&key.activation_hash);
            let matches_current =
                current.is_some_and(|deadline| DeadlineKey::from(deadline) == key);
            if !matches_current {
                stale_discarded += 1;
                continue;
            }
            let deadline = self
                .active
                .remove(&key.activation_hash)
                .expect("matching active deadline exists");
            self.remove_lane_member(deadline.lane_resolution_ms, &key.activation_hash);
            due.push(deadline);
        }
        DispatchBatch {
            observed_at,
            due,
            stale_discarded,
        }
    }

    fn discard_stale_head(&mut self) {
        loop {
            let stale = self.heap.peek().is_some_and(|Reverse(key)| {
                self.active
                    .get(&key.activation_hash)
                    .is_none_or(|deadline| DeadlineKey::from(deadline) != *key)
            });
            if !stale {
                break;
            }
            self.heap.pop();
        }
    }

    fn remove_lane_member(
        &mut self,
        lane_resolution_ms: NonZeroU32,
        activation_hash: &CanonicalHash,
    ) {
        let empty = self
            .lanes
            .get_mut(&lane_resolution_ms)
            .is_some_and(|members| {
                members.remove(activation_hash);
                members.is_empty()
            });
        if empty {
            self.lanes.remove(&lane_resolution_ms);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchBatch {
    pub observed_at: TimestampMs,
    pub due: Vec<ArmedDeadline>,
    pub stale_discarded: u64,
}
