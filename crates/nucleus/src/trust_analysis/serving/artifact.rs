//! Original evidence artifacts, their audience, and canonical bytes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    super::{Identity, claims, contacts},
    MAX_ARTIFACT_BYTES,
};
use crate::{
    json::encoded_len,
    karma::{CanonicalHash, canonical_hash, canonical_json_bytes},
    transfer::receipt,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Receipt,
    Contact,
    Claim,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receipt => "receipt",
            Self::Contact => "contact",
            Self::Claim => "claim",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "document",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Artifact {
    Receipt(Box<receipt::Document>),
    Contact(contacts::Document),
    Claim(claims::Document),
}

impl Artifact {
    pub fn kind(&self) -> SourceKind {
        match self {
            Self::Receipt(_) => SourceKind::Receipt,
            Self::Contact(_) => SourceKind::Contact,
            Self::Claim(_) => SourceKind::Claim,
        }
    }
    pub fn expires_at(&self) -> Result<i64, String> {
        match self {
            Self::Receipt(document) => document
                .public_bundle()
                .map(|bundle| bundle.publication.scope.expires_at)
                .ok_or_else(|| {
                    "Private receipt reviews and authority documents cannot be served".into()
                }),
            Self::Contact(document) => Ok(document.statement.expires_at),
            Self::Claim(document) => Ok(document.statement.expires_at),
        }
    }
    /// Validate an original document and its serving lifetime and budget.
    ///
    /// # Errors
    ///
    /// The artifact is invalid, private to its original review, expired, or oversized.
    pub fn validate(&self, now: i64) -> Result<(), String> {
        self.validated_bytes(now).map(drop)
    }

    pub(in crate::trust_analysis) fn validated_bytes(&self, now: i64) -> Result<Vec<u8>, String> {
        match self {
            Self::Receipt(document) => document.validate(now)?,
            Self::Contact(document) => {
                check_size(
                    document,
                    contacts::MAX_DOCUMENT_BYTES,
                    "Contact disclosure exceeds 32 KiB",
                )?;
                document.validate()?;
            }
            Self::Claim(document) => {
                check_size(
                    document,
                    claims::MAX_DOCUMENT_BYTES,
                    "Profile claim exceeds 32 KiB",
                )?;
                document.validate()?;
            }
        }
        let invalid = "Choose unexpired bounded original evidence";
        if self.expires_at()? <= now {
            return Err(invalid.into());
        }
        let bytes = self.bytes()?;
        if bytes.len() > MAX_ARTIFACT_BYTES {
            return Err(invalid.into());
        }
        Ok(bytes)
    }
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        canonical_json_bytes(self).map_err(|error| error.to_string())
    }
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        canonical_hash("lince.evidence-serving.artifact.v1", self)
            .map_err(|error| error.to_string())
    }
    pub fn allows(&self, organ: &str) -> bool {
        self.allows_reader(&Identity::Organ(organ.into()))
    }
    pub fn allows_reader(&self, reader: &Identity) -> bool {
        match self {
            Self::Receipt(document) => document.public_bundle().is_some_and(|bundle| {
                bundle.publication.scope.audience.allows(
                    reader,
                    &Identity::Organ(bundle.publication.scope.source.origin.clone()),
                )
            }),
            Self::Contact(document) => document
                .statement
                .audience
                .allows(reader, &document.statement.issuer),
            Self::Claim(document) => document
                .statement
                .audience
                .allows(reader, &document.statement.issuer),
        }
    }
    pub fn parse(bytes: &[u8], now: i64) -> Result<Self, String> {
        if bytes.len() > MAX_ARTIFACT_BYTES {
            return Err("Evidence artifact exceeds its byte limit".into());
        }
        let artifact: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        artifact.validate(now)?;
        Ok(artifact)
    }
}

fn check_size(document: &impl Serialize, limit: usize, message: &str) -> Result<(), String> {
    if encoded_len(document).map_err(|error| error.to_string())? > limit {
        return Err(message.into());
    }
    Ok(())
}
