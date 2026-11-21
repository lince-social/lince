//! Canonical timezone artifacts, their revisions, and validated zone data.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    KarmaBoundaryError, TimeZoneId, TzdbRevision, TzdbVersion, canonical::hash_canonical_bytes,
    canonical_json_bytes,
};

mod provider;
mod zone;

pub use provider::ArtifactTimeZoneProvider;
pub use zone::{TimeZoneDefinition, UtcOffsetSegment};

pub const TZDB_ARTIFACT_HASH_DOMAIN: &str = "karma.tzdb-artifact.v1";
pub const MAX_TZDB_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TZDB_ARTIFACT_ZONES: usize = 4_096;
pub const MAX_TZDB_SEGMENTS_PER_ZONE: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TimeZoneArtifactSchema {
    #[serde(rename = "karma.tzdb-artifact.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename = "TimeZoneArtifactWire")]
pub struct TimeZoneArtifact {
    schema: TimeZoneArtifactSchema,
    version: TzdbVersion,
    zones: BTreeMap<TimeZoneId, TimeZoneDefinition>,
}

#[derive(Deserialize)]
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
        Ok(self.revision_from_bytes(&self.canonical_bytes()?))
    }

    fn revision_from_bytes(&self, bytes: &[u8]) -> TzdbRevision {
        TzdbRevision {
            version: self.version.clone(),
            digest: hash_canonical_bytes(TZDB_ARTIFACT_HASH_DOMAIN, bytes),
        }
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, KarmaBoundaryError> {
        canonical_json_bytes(self)
    }

    pub fn zones(&self) -> &BTreeMap<TimeZoneId, TimeZoneDefinition> {
        &self.zones
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
