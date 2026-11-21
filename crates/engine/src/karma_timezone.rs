// non-lean-yet
// Scope: fn load_time_zone_artifact, const UTC_TZDB_VERSION, const UTC_TIME_ZONE_IDS, fn
//   bundled_time_zone_provider.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 61.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 16.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 16.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 11, 45, 54.
// - [Clippy probe: clippy::needless_pass_by_value] Borrow the input after checking the caller and
//   framework contract (Clippy suggests &). Sites 73.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   11, 41, 43, 45, 54.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use nucleus::karma::{
    ArtifactTimeZoneProvider, BundledTimeZoneProvider, KarmaBoundaryError, MAX_TZDB_ARTIFACT_BYTES,
    TimeZoneArtifact, TimeZoneDefinition, TimeZoneId, TimeZoneProvider, TzdbRevision, TzdbVersion,
    UtcOffsetSegment,
};

use crate::EngineError;

pub fn load_time_zone_artifact(
    path: &Path,
    expected: &TzdbRevision,
) -> Result<Arc<dyn TimeZoneProvider>, EngineError> {
    let metadata = std::fs::metadata(path)?;
    let byte_length = usize::try_from(metadata.len()).map_err(|_| EngineError::Conflict {
        code: "karma_tzdb_artifact_too_large",
        message: "timezone artifact length does not fit this host".to_string(),
    })?;
    if byte_length == 0 || byte_length > MAX_TZDB_ARTIFACT_BYTES {
        return Err(EngineError::Conflict {
            code: "karma_tzdb_artifact_too_large",
            message: format!("timezone artifact must contain 1..={MAX_TZDB_ARTIFACT_BYTES} bytes"),
        });
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() != byte_length {
        return Err(EngineError::Conflict {
            code: "karma_tzdb_artifact_changed",
            message: "timezone artifact changed while it was being loaded".to_string(),
        });
    }
    let provider = ArtifactTimeZoneProvider::from_canonical_bytes_expected(&bytes, expected)
        .map_err(|error| EngineError::Conflict {
            code: "karma_tzdb_artifact_invalid",
            message: error.to_string(),
        })?;
    Ok(Arc::new(provider))
}

pub const UTC_TZDB_VERSION: &str = "lince-utc.1";

pub const UTC_TIME_ZONE_IDS: [&str; 4] = ["Etc/GMT", "Etc/UTC", "GMT", "UTC"];

pub fn bundled_time_zone_provider() -> Result<Arc<dyn TimeZoneProvider>, EngineError> {
    Ok(Arc::new(BundledTimeZoneProvider::new().map_err(
        |error| EngineError::Conflict {
            code: "karma_tzdb_bundled_invalid",
            message: error.to_string(),
        },
    )?))
}

pub fn utc_time_zone_provider() -> Result<Arc<dyn TimeZoneProvider>, EngineError> {
    let fixed = TimeZoneDefinition::new(vec![
        UtcOffsetSegment::new(None, None, 0).map_err(utc_boundary)?,
    ])
    .map_err(utc_boundary)?;
    let mut zones = BTreeMap::new();
    for name in UTC_TIME_ZONE_IDS {
        zones.insert(TimeZoneId::new(name).map_err(utc_boundary)?, fixed.clone());
    }
    let artifact = TimeZoneArtifact::new(
        TzdbVersion::new(UTC_TZDB_VERSION).map_err(utc_boundary)?,
        zones,
    )
    .map_err(utc_boundary)?;
    Ok(Arc::new(
        ArtifactTimeZoneProvider::from_artifact(artifact).map_err(utc_boundary)?,
    ))
}

fn utc_boundary(error: KarmaBoundaryError) -> EngineError {
    EngineError::Conflict {
        code: "karma_tzdb_utc_invalid",
        message: error.to_string(),
    }
}
