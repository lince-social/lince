//! Pinned timezone resolution and conservative calendar interval attestation.

use std::num::NonZeroU64;

use super::{MAX_TZDB_ARTIFACT_BYTES, TimeZoneArtifact, TimeZoneDefinition};
use crate::{
    Cadence, CalendarSchedule, CivilDateTime, FoldPolicy, KarmaBoundaryError, LocalTimeResolution,
    TimeZoneId, TimeZoneProvider, TzdbRevision,
};

const MILLIS_PER_DAY: u64 = 86_400_000;

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

    /// Loads a canonical artifact and derives its content-addressed revision.
    ///
    /// # Errors
    ///
    /// Rejects malformed artifacts, noncanonical JSON, and bytes outside the size limit.
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
        let revision = artifact.revision_from_bytes(bytes);
        Ok(Self { artifact, revision })
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

    fn zone(&self, timezone: &TimeZoneId) -> Result<&TimeZoneDefinition, KarmaBoundaryError> {
        self.artifact.zones.get(timezone).ok_or_else(|| {
            KarmaBoundaryError::invalid_definition(format!(
                "timezone artifact does not contain {}",
                timezone.as_str()
            ))
        })
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
        self.zone(timezone)?.resolve_local(local)
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
        let zone = self.zone(&schedule.timezone)?;
        conservative_minimum(zone, schedule)
    }
}

fn conservative_minimum(
    zone: &TimeZoneDefinition,
    schedule: &CalendarSchedule,
) -> Result<NonZeroU64, KarmaBoundaryError> {
    let local_minimum = minimum_local_interval_ms(&schedule.cadence)?;
    let mut offsets = zone.segments.iter().map(|segment| segment.offset_seconds);
    let first = offsets.next().ok_or_else(|| {
        KarmaBoundaryError::invalid_definition("timezone artifact zone has no segments")
    })?;
    let (mut minimum, mut maximum, mut previous) = (first, first, first);
    let mut fold_limit = u64::MAX;
    for offset in offsets {
        minimum = minimum.min(offset);
        maximum = maximum.max(offset);
        if schedule.fold == FoldPolicy::Both && previous > offset {
            fold_limit = fold_limit.min(u64::from(previous.abs_diff(offset)) * 1_000);
        }
        previous = offset;
    }
    let spread = u64::from(maximum.abs_diff(minimum)) * 1_000;
    let minimum = local_minimum.saturating_sub(spread).max(1).min(fold_limit);
    NonZeroU64::new(minimum).ok_or_else(|| {
        KarmaBoundaryError::invalid_definition("timezone minimum interval became zero")
    })
}

fn minimum_local_interval_ms(cadence: &Cadence) -> Result<u64, KarmaBoundaryError> {
    let overflow = || KarmaBoundaryError::invalid_definition("calendar cadence overflowed");
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
    Ok(step.max(1))
}
