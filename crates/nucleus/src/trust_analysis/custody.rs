//! Custody policy, signed custody agreements, and control commands.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Filter;
use crate::karma::CanonicalHash;

mod agreement;
pub mod forwarding;

pub use agreement::{Acknowledgment, Offer};

pub const NAMESPACE: &str = "lince.evidence-custody";
pub const MAX_STORAGE_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ITEMS: u32 = 4096;
pub const CONTROL_SECONDS: i64 = 30 * 86400;
pub const UPLOAD_SECONDS: u64 = 15;
pub const MAX_UPLOADS: usize = 4;

const MAX_RETENTION_SECONDS: u32 = 365 * 86_400;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub enabled: bool,
    #[serde(default)]
    pub forwarding: forwarding::Policy,
    pub senders: Filter,
    pub readers: Filter,
    pub storage_bytes: u64,
    pub items: u32,
    pub retention_seconds: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            forwarding: Default::default(),
            senders: Filter::default(),
            readers: Filter::default(),
            storage_bytes: 8 * 1024 * 1024,
            items: 256,
            retention_seconds: MAX_RETENTION_SECONDS,
        }
    }
}

impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        self.senders.validate()?;
        self.readers.validate()?;
        self.forwarding.validate()?;
        if self.storage_bytes == 0
            || self.storage_bytes > MAX_STORAGE_BYTES
            || self.items == 0
            || self.items > MAX_ITEMS
            || !(1..=MAX_RETENTION_SECONDS).contains(&self.retention_seconds)
        {
            return Err(
                "Choose bounded custody storage, item count and retention up to one year".into(),
            );
        }
        if self.enabled && (self.senders.include.is_empty() || self.readers.include.is_empty()) {
            return Err(
                "Choose explicit sender and reader include conditions before enabling custody"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    InspectOriginal {
        record: String,
    },
    Outbox {
        before: Option<i64>,
    },
    CancelDelivery {
        record: String,
        expected_offer: CanonicalHash,
    },
    InspectPolicy,
    SetPolicy {
        policy: Policy,
        expected_version: u64,
    },
    Inventory {
        before: Option<i64>,
    },
    Cleanup,
}

#[cfg(test)]
mod tests;
