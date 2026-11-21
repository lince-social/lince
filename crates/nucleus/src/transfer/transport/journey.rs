//! Transport journey events and participant progress.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_str, from_value};

use crate::valid_uid;

/// Domain written into signed Journey events.
pub const JOURNEY_DOMAIN: &str = "lince.transfer-journey.v1";
/// Time after the pickup window before an unstarted Journey releases capacity.
pub const PICKUP_GRACE_MILLIS: i64 = 15 * 60 * 1000;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Arrived,
    Boarded,
    Departed,
    Destination,
    Finished,
    Cancelled,
}

impl Step {
    /// Whether this step ends the participant's attendance.
    pub fn terminal(self) -> bool {
        matches!(self, Self::Finished | Self::Cancelled)
    }

    /// The caption for recording this step.
    pub fn caption(self) -> &'static str {
        match self {
            Self::Arrived => "Arrived at pickup",
            Self::Boarded => "Met / boarded",
            Self::Departed => "Departed",
            Self::Destination => "Reached destination",
            Self::Finished => "Finish my journey",
            Self::Cancelled => "Cancel my journey attendance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub domain: String,
    pub transfer: String,
    pub revision: u64,
    pub person: String,
    pub step: Step,
    pub request_id: String,
}

impl Event {
    /// Validate the event's domain, revision, identities, and replay key.
    ///
    /// # Errors
    ///
    /// Returns an error for an unrecognized domain, invalid identity or
    /// revision, or a missing, oversized, or non-printable replay key.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.domain != JOURNEY_DOMAIN
            || self.revision == 0
            || !valid_uid(&self.transfer, "r")
            || !valid_uid(&self.person, "r")
            || self.request_id.is_empty()
            || self.request_id.len() > 256
            || self.request_id.chars().any(char::is_control)
        {
            return Err("Choose the current journey, acting Person and stable request ID");
        }
        Ok(())
    }

    /// Read a valid Journey event from a bounded Fact payload.
    pub fn parse(payload: &str) -> Option<Self> {
        if payload.len() > 4096 {
            return None;
        }
        let mut value: Value = from_str(payload).ok()?;
        let event: Self = from_value(value.as_object_mut()?.remove("journey")?).ok()?;
        event.validate().ok()?;
        Some(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Progress {
    pub person: String,
    pub step: Option<Step>,
    pub fact: Option<String>,
    pub at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Journey {
    pub transfer: String,
    pub revision: u64,
    pub agreed: bool,
    pub participants: Vec<Progress>,
    pub started: bool,
    pub expired: bool,
    pub capacity_released: bool,
    pub all_finished: bool,
    pub pickup_grace_until: String,
}
