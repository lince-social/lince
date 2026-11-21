//! Durable calendar occurrence validation and canonical identity.

use serde::{Deserialize, Deserializer, Serialize, de};

use super::CalendarCatchUp;
use crate::{CanonicalHash, KarmaBoundaryError, TimestampMs, canonical_hash};

pub const CALENDAR_SCHEDULE_OCCURRENCE_HASH_DOMAIN: &str = "karma.calendar-schedule-occurrence.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CalendarScheduleOccurrenceSchema {
    #[serde(rename = "karma.calendar-schedule-occurrence.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarScheduleOccurrence {
    pub schema: CalendarScheduleOccurrenceSchema,
    pub activation_hash: CanonicalHash,
    pub sequence: u64,
    pub claimed_cursor_revision: u64,
    pub lease_fencing_token: u64,
    pub observed_at: TimestampMs,
    pub catch_up: CalendarCatchUp,
}

#[derive(Deserialize)]
struct CalendarScheduleOccurrenceWire {
    schema: CalendarScheduleOccurrenceSchema,
    activation_hash: CanonicalHash,
    sequence: u64,
    claimed_cursor_revision: u64,
    lease_fencing_token: u64,
    observed_at: TimestampMs,
    catch_up: CalendarCatchUp,
}

impl CalendarScheduleOccurrence {
    /// Records an emitted catch-up at the claimed observation time.
    ///
    /// # Errors
    ///
    /// Rejects missing emission, mismatched observation time, or zero claims.
    pub fn new(
        activation_hash: CanonicalHash,
        sequence: u64,
        claimed_cursor_revision: u64,
        lease_fencing_token: u64,
        observed_at: TimestampMs,
        catch_up: CalendarCatchUp,
    ) -> Result<Self, KarmaBoundaryError> {
        if sequence == 0 || claimed_cursor_revision == 0 || lease_fencing_token == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "calendar occurrence sequence, cursor revision, and fencing token must be positive",
            ));
        }
        if catch_up.observed_at != observed_at || catch_up.emission.is_none() {
            return Err(KarmaBoundaryError::invalid_input(
                "calendar occurrence requires an emitted catch-up at the same observed time",
            ));
        }
        Ok(Self {
            schema: CalendarScheduleOccurrenceSchema::V1,
            activation_hash,
            sequence,
            claimed_cursor_revision,
            lease_fencing_token,
            observed_at,
            catch_up,
        })
    }

    pub fn occurrence_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(CALENDAR_SCHEDULE_OCCURRENCE_HASH_DOMAIN, self)
    }
}

impl<'de> Deserialize<'de> for CalendarScheduleOccurrence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = CalendarScheduleOccurrenceWire::deserialize(deserializer)?;
        if wire.schema != CalendarScheduleOccurrenceSchema::V1 {
            return Err(de::Error::custom(
                "unsupported calendar schedule occurrence schema",
            ));
        }
        Self::new(
            wire.activation_hash,
            wire.sequence,
            wire.claimed_cursor_revision,
            wire.lease_fencing_token,
            wire.observed_at,
            wire.catch_up,
        )
        .map_err(de::Error::custom)
    }
}
