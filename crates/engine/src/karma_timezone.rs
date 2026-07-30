use std::{path::Path, sync::Arc};

use nucleus::karma::{
    ArtifactTimeZoneProvider, MAX_TZDB_ARTIFACT_BYTES, TimeZoneProvider, TzdbRevision,
};

use crate::EngineError;

/// Load a pinned timezone artifact at process/configuration startup. The file
/// is bounded before allocation, must contain canonical JSON, and must hash to
/// the exact revision named by the Frequency/runtime configuration.
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
