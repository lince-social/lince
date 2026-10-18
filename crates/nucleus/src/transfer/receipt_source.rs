use super::{
    outcome::{Content, Request},
    receipt::Source,
};
use crate::karma::{CanonicalHash, canonical_hash};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "lince.transfer-receipt-source";
pub const DOMAIN: &str = "lince.transfer-receipt-source.v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRequest {
    pub receipt: String,
    pub expected_version: u64,
    pub private_transfer: Option<String>,
    pub outcome: Request,
    pub public_need: Option<CanonicalHash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedClock {
    pub organ: String,
    pub cell: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub domain: String,
    pub record: String,
    pub source: Source,
    pub author: String,
    pub clock: RecordedClock,
    pub private_transfer: Option<String>,
    pub request_id: String,
    pub content: Content,
    pub measurement: super::receipt::Measurement,
    pub public_need: Option<CanonicalHash>,
}

pub fn source_key(source: &Source) -> Result<CanonicalHash, String> {
    source.validate()?;
    canonical_hash("lince.transfer.receipt.private-source.v1", source)
        .map_err(|error| error.to_string())
}
