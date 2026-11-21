// non-lean-yet
// Scope: const NAMESPACE, const DOMAIN, struct SourceRequest, struct RecordedClock.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 44.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 9, 10,
//   14, 15, 16, 17, 18, 19, 24, 25, 26, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 44.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites source_key:
//   String (44).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    outcome::{Content, Request},
    receipt::Source,
};
use crate::karma::{CanonicalHash, canonical_hash};

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
