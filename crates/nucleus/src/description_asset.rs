use serde::{Deserialize, Serialize};

use crate::valid_uid;

/// Maximum decoded size of one Record description asset.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;

const REFERENCE_PREFIX: &str = "asset:";
const DIGEST_HEX_BYTES: usize = 64;

/// Format of a stored description asset.
#[expect(
    clippy::exhaustive_enums,
    reason = "callers match every supported asset format"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Native vector drawing.
    Drawing,
    /// PNG image.
    Png,
    /// WebP image.
    Webp,
}

impl Kind {
    /// Returns the stable format name used in storage and asset digests.
    pub fn name(self) -> &'static str {
        match self {
            Self::Drawing => "drawing",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}

/// Request to store or retrieve a description asset.
#[expect(
    clippy::exhaustive_enums,
    reason = "callers match every request variant"
)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    /// Stores encoded data for a Record's description.
    Put {
        /// Owning Record identifier.
        record: String,
        /// Format of the encoded asset.
        kind: Kind,
        /// Asset data as Base64.
        data_base64: String,
    },
    /// Retrieves an asset by its Record and digest.
    Get {
        /// Owning Record identifier.
        record: String,
        /// Lowercase hexadecimal asset digest.
        asset: String,
    },
}

/// Result of storing or retrieving a description asset.
#[expect(
    clippy::exhaustive_enums,
    reason = "callers match every response variant"
)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    /// Confirms storage and identifies the new or existing asset.
    Stored {
        /// Lowercase hexadecimal asset digest.
        asset: String,
    },
    /// Returns the asset format and encoded bytes.
    Data {
        /// Format of the returned asset.
        kind: Kind,
        /// Asset data as Base64.
        data_base64: String,
    },
}

/// Formats a Record identifier and asset digest into a reference.
pub fn reference(record: &str, asset: &str) -> String {
    format!("{REFERENCE_PREFIX}{record}/{asset}")
}

/// Borrows the Record identifier and lowercase hexadecimal digest from a valid reference.
pub fn parse_reference(source: &str) -> Option<(&str, &str)> {
    let (record, asset) = source.strip_prefix(REFERENCE_PREFIX)?.split_once('/')?;
    (valid_uid(record, "r")
        && asset.len() == DIGEST_HEX_BYTES
        && asset
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')))
    .then_some((record, asset))
}
