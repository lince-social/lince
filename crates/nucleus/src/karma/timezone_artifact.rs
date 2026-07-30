use std::{collections::BTreeMap, num::NonZeroU64};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::{
    Cadence, CalendarSchedule, CivilDateTime, FoldPolicy, KarmaBoundaryError,
    LocalTimeResolution, TimeZoneId, TimeZoneProvider, TimestampMs, TzdbRevision, TzdbVersion,
    canonical_hash, canonical_json_bytes,
};

pub const TZDB_ARTIFACT_HASH_DOMAIN: &str = "karma.tzdb-artifact.v1";
pub const MAX_TZDB_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TZDB_ARTIFACT_ZONES: usize = 4_096;
pub const MAX_TZDB_SEGMENTS_PER_ZONE: usize = 100_000;
const MAX_ABSOLUTE_UTC_OFFSET_SECONDS: i32 = 86_400;
const MILLIS_PER_DAY: u64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TimeZoneArtifactSchema {
    #[serde(rename = "karma.tzdb-artifact.v1")]
    V1,
}

/// One half-open UTC interval with a fixed local offset. `None` at the first
/// start or final end represents complete coverage toward that infinity.
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

    fn validate(self) -> Result<(), KarmaBoundaryError> {
        if self.offset_seconds.unsigned_abs() > MAX_ABSOLUTE_UTC_OFFSET_SECONDS as u32 {
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

    fn contains_utc_millis(self, value: i128) -> bool {
        self.utc_start
            .is_none_or(|start| value >= i128::from(start.as_millis()))
            && self
                .utc_end
                .is_none_or(|end| value < i128::from(end.as_millis()))
    }

    fn local_start_millis(self) -> i128 {
        self.utc_start.map_or(i128::MIN, |start| {
            i128::from(start.as_millis()) + self.offset_millis()
        })
    }

    fn local_end_millis(self) -> i128 {
        self.utc_end.map_or(i128::MAX, |end| {
            i128::from(end.as_millis()) + self.offset_millis()
        })
    }

    fn offset_millis(self) -> i128 {
        i128::from(self.offset_seconds) * 1_000
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

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.segments.is_empty() || self.segments.len() > MAX_TZDB_SEGMENTS_PER_ZONE {
            return Err(KarmaBoundaryError::invalid_definition(format!(
                "timezone artifact zone needs 1..={MAX_TZDB_SEGMENTS_PER_ZONE} segments"
            )));
        }
        if self
            .segments
            .first()
            .and_then(|value| value.utc_start)
            .is_some()
            || self
                .segments
                .last()
                .and_then(|value| value.utc_end)
                .is_some()
        {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact zone must cover the complete UTC timeline",
            ));
        }
        for (index, segment) in self.segments.iter().copied().enumerate() {
            segment.validate()?;
            if index > 0 && segment.utc_start.is_none() {
                return Err(KarmaBoundaryError::invalid_definition(
                    "only the first timezone segment may omit utc_start",
                ));
            }
            if index + 1 < self.segments.len() && segment.utc_end.is_none() {
                return Err(KarmaBoundaryError::invalid_definition(
                    "only the final timezone segment may omit utc_end",
                ));
            }
        }
        for pair in self.segments.windows(2) {
            if pair[0].utc_end != pair[1].utc_start {
                return Err(KarmaBoundaryError::invalid_definition(
                    "timezone artifact UTC segments must be contiguous",
                ));
            }
        }
        self.validate_local_multiplicity()
    }

    fn validate_local_multiplicity(&self) -> Result<(), KarmaBoundaryError> {
        let mut active = 0_i32;
        let mut events = Vec::with_capacity(self.segments.len().saturating_mul(2));
        for segment in &self.segments {
            let start = segment.local_start_millis();
            let end = segment.local_end_millis();
            if start == i128::MIN {
                active += 1;
            } else {
                events.push((start, 1_i32));
            }
            if end != i128::MAX {
                events.push((end, -1_i32));
            }
        }
        events.sort_by_key(|(at, delta)| (*at, *delta));
        if active > 2 {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact maps one local instant to more than two UTC instants",
            ));
        }
        for (_, delta) in events {
            active += delta;
            if !(0..=2).contains(&active) {
                return Err(KarmaBoundaryError::invalid_definition(
                    "timezone artifact has invalid local-time interval multiplicity",
                ));
            }
        }
        if active != 1 {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact does not cover the final local-time horizon",
            ));
        }
        Ok(())
    }

    fn resolve_local(
        &self,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        let local_ms = i128::from(local.as_naive().and_utc().timestamp_millis());
        let mut candidates = Vec::with_capacity(2);
        for segment in &self.segments {
            let candidate = local_ms - segment.offset_millis();
            if segment.contains_utc_millis(candidate)
                && let Ok(candidate) = i64::try_from(candidate)
                && let Ok(candidate) = TimestampMs::from_millis(candidate)
            {
                candidates.push(candidate);
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        match candidates.as_slice() {
            [instant] => Ok(LocalTimeResolution::Unique { instant: *instant }),
            [first, second] => Ok(LocalTimeResolution::Fold {
                first: *first,
                second: *second,
            }),
            [] => self.resolve_gap(local_ms),
            _ => Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact maps one local instant to more than two UTC instants",
            )),
        }
    }

    fn resolve_gap(&self, local_ms: i128) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        let previous = self
            .segments
            .iter()
            .filter(|segment| segment.local_end_millis() <= local_ms)
            .max_by_key(|segment| segment.local_end_millis())
            .and_then(|segment| segment.utc_end);
        let next = self
            .segments
            .iter()
            .filter(|segment| segment.local_start_millis() > local_ms)
            .min_by_key(|segment| segment.local_start_millis())
            .and_then(|segment| segment.utc_start);
        let (Some(transition), Some(first_valid_after)) = (previous, next) else {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact has an unexplained local-time hole",
            ));
        };
        if transition != first_valid_after {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact local gap does not have one UTC transition boundary",
            ));
        }
        let before =
            TimestampMs::from_millis(transition.as_millis().checked_sub(1).ok_or_else(|| {
                KarmaBoundaryError::invalid_definition("timezone gap underflows")
            })?)?;
        Ok(LocalTimeResolution::Gap {
            before,
            first_valid_after,
        })
    }

    fn conservative_minimum_interval_ms(
        &self,
        schedule: &CalendarSchedule,
    ) -> Result<NonZeroU64, KarmaBoundaryError> {
        let local_minimum = minimum_local_interval_ms(&schedule.cadence)?;
        let minimum_offset = self
            .segments
            .iter()
            .map(|segment| segment.offset_seconds)
            .min()
            .expect("validated timezone has a segment");
        let maximum_offset = self
            .segments
            .iter()
            .map(|segment| segment.offset_seconds)
            .max()
            .expect("validated timezone has a segment");
        let offset_spread_ms = u64::try_from(maximum_offset - minimum_offset)
            .expect("ordered i32 offset difference is non-negative")
            .checked_mul(1_000)
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_definition("timezone offset spread overflowed")
            })?;
        let mut minimum = local_minimum.saturating_sub(offset_spread_ms).max(1);
        if schedule.fold == FoldPolicy::Both
            && let Some(fold_width) = self.minimum_fold_width_ms()?
        {
            minimum = minimum.min(fold_width.get());
        }
        NonZeroU64::new(minimum).ok_or_else(|| {
            KarmaBoundaryError::invalid_definition("timezone minimum interval became zero")
        })
    }

    fn minimum_fold_width_ms(&self) -> Result<Option<NonZeroU64>, KarmaBoundaryError> {
        let mut minimum = None::<u64>;
        for pair in self.segments.windows(2) {
            if pair[0].offset_seconds > pair[1].offset_seconds {
                let seconds = u64::try_from(pair[0].offset_seconds - pair[1].offset_seconds)
                    .expect("ordered offset difference is non-negative");
                let milliseconds = seconds.checked_mul(1_000).ok_or_else(|| {
                    KarmaBoundaryError::invalid_definition("timezone fold width overflowed")
                })?;
                minimum = Some(minimum.map_or(milliseconds, |value| value.min(milliseconds)));
            }
        }
        Ok(minimum.and_then(NonZeroU64::new))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZoneArtifact {
    schema: TimeZoneArtifactSchema,
    version: TzdbVersion,
    zones: BTreeMap<TimeZoneId, TimeZoneDefinition>,
}

#[derive(Serialize, Deserialize)]
struct TimeZoneArtifactWire {
    schema: TimeZoneArtifactSchema,
    version: TzdbVersion,
    zones: BTreeMap<TimeZoneId, TimeZoneDefinition>,
}

impl TimeZoneArtifact {
    pub fn new(
        version: TzdbVersion,
        zones: BTreeMap<TimeZoneId, TimeZoneDefinition>,
    ) -> Result<Self, KarmaBoundaryError> {
        if zones.is_empty() || zones.len() > MAX_TZDB_ARTIFACT_ZONES {
            return Err(KarmaBoundaryError::invalid_definition(format!(
                "timezone artifact needs 1..={MAX_TZDB_ARTIFACT_ZONES} zones"
            )));
        }
        for definition in zones.values() {
            definition.validate()?;
        }
        Ok(Self {
            schema: TimeZoneArtifactSchema::V1,
            version,
            zones,
        })
    }

    pub fn revision(&self) -> Result<TzdbRevision, KarmaBoundaryError> {
        Ok(TzdbRevision {
            version: self.version.clone(),
            digest: canonical_hash(TZDB_ARTIFACT_HASH_DOMAIN, self)?,
        })
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, KarmaBoundaryError> {
        canonical_json_bytes(self)
    }

    pub fn zones(&self) -> &BTreeMap<TimeZoneId, TimeZoneDefinition> {
        &self.zones
    }
}

impl Serialize for TimeZoneArtifact {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TimeZoneArtifactWire {
            schema: self.schema,
            version: self.version.clone(),
            zones: self.zones.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TimeZoneArtifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TimeZoneArtifactWire::deserialize(deserializer)?;
        if wire.schema != TimeZoneArtifactSchema::V1 {
            return Err(de::Error::custom("unsupported timezone artifact schema"));
        }
        Self::new(wire.version, wire.zones).map_err(de::Error::custom)
    }
}

/// Pure provider created only from canonical, content-addressed artifact bytes.
#[derive(Debug, Clone)]
pub struct ArtifactTimeZoneProvider {
    artifact: TimeZoneArtifact,
    revision: TzdbRevision,
}

impl ArtifactTimeZoneProvider {
    pub fn from_artifact(artifact: TimeZoneArtifact) -> Result<Self, KarmaBoundaryError> {
        let revision = artifact.revision()?;
        Ok(Self { artifact, revision })
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, KarmaBoundaryError> {
        if bytes.is_empty() || bytes.len() > MAX_TZDB_ARTIFACT_BYTES {
            return Err(KarmaBoundaryError::invalid_definition(format!(
                "timezone artifact byte length must be within 1..={MAX_TZDB_ARTIFACT_BYTES}"
            )));
        }
        let artifact: TimeZoneArtifact = serde_json::from_slice(bytes).map_err(|error| {
            KarmaBoundaryError::invalid_definition(format!(
                "timezone artifact JSON is invalid: {error}"
            ))
        })?;
        if artifact.canonical_bytes()? != bytes {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact bytes are not canonical JSON",
            ));
        }
        Self::from_artifact(artifact)
    }

    pub fn from_canonical_bytes_expected(
        bytes: &[u8],
        expected: &TzdbRevision,
    ) -> Result<Self, KarmaBoundaryError> {
        let provider = Self::from_canonical_bytes(bytes)?;
        if provider.revision() != expected {
            return Err(KarmaBoundaryError::invalid_definition(
                "timezone artifact revision does not match the expected content address",
            ));
        }
        Ok(provider)
    }

    pub fn artifact(&self) -> &TimeZoneArtifact {
        &self.artifact
    }
}

impl TimeZoneProvider for ArtifactTimeZoneProvider {
    fn revision(&self) -> &TzdbRevision {
        &self.revision
    }

    fn resolve_local(
        &self,
        timezone: &TimeZoneId,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        self.artifact
            .zones
            .get(timezone)
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_definition(format!(
                    "timezone artifact does not contain {}",
                    timezone.as_str()
                ))
            })?
            .resolve_local(local)
    }

    fn minimum_interval_ms(
        &self,
        schedule: &CalendarSchedule,
    ) -> Result<NonZeroU64, KarmaBoundaryError> {
        if &schedule.tzdb != self.revision() {
            return Err(KarmaBoundaryError::invalid_definition(
                "calendar schedule tzdb revision does not match provider",
            ));
        }
        self.artifact
            .zones
            .get(&schedule.timezone)
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_definition(format!(
                    "timezone artifact does not contain {}",
                    schedule.timezone.as_str()
                ))
            })?
            .conservative_minimum_interval_ms(schedule)
    }
}

/// The shortest local separation two consecutive occurrences of this rule can
/// have, in milliseconds.
///
/// Deliberately a lower bound, never an average. It exists so the dispatcher can
/// refuse a schedule it could not keep up with, and a bound that is too large
/// would admit exactly the schedule that overloads it.
///
/// A month contributes 28 days, its shortest possible length. Landing subtracts
/// six days: rolling each occurrence forward to an allowed weekday moves it by
/// nought to six days, so it can pull two neighbours at most six days closer
/// together — never more, because the roll is bounded by the week it starts in.
fn minimum_local_interval_ms(cadence: &Cadence) -> Result<u64, KarmaBoundaryError> {
    let overflow =
        || KarmaBoundaryError::invalid_definition("calendar cadence overflowed");
    let months = u64::from(cadence.every.calendar_months().ok_or_else(overflow)?);
    let fixed = u64::try_from(cadence.every.fixed_milliseconds().ok_or_else(overflow)?)
        .map_err(|_| KarmaBoundaryError::invalid_definition("cadence step is negative"))?;
    let calendar = months
        .checked_mul(28)
        .and_then(|days| days.checked_mul(MILLIS_PER_DAY))
        .ok_or_else(overflow)?;
    let step = calendar.checked_add(fixed).ok_or_else(overflow)?;
    let step = if cadence.land_on.is_some() {
        step.saturating_sub(6 * MILLIS_PER_DAY)
    } else {
        step
    };
    // A one-shot has no second occurrence to be separated from, and a zero here
    // would read as "infinitely fast" to a dispatcher sizing its budget.
    Ok(step.max(1))
}
