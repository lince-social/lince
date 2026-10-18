use super::{Filter, Identity, claims, contacts};
use crate::{
    karma::{CanonicalHash, canonical_hash},
    transfer::receipt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "lince.evidence-serving";
pub const CHUNK_BYTES: usize = 65536;
pub const MAX_ARTIFACT_BYTES: usize = 800000;
pub const MAX_STORAGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PERMISSIONS: usize = 256;

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
    pub fn validate(&self, now: i64) -> Result<(), String> {
        match self {
            Self::Receipt(document) => document.validate(now)?,
            Self::Contact(document) => {
                contacts::Document::parse(
                    &serde_json::to_vec(document).map_err(|e| e.to_string())?,
                )?;
            }
            Self::Claim(document) => {
                claims::Document::parse(&serde_json::to_vec(document).map_err(|e| e.to_string())?)?;
            }
        };
        if self.expires_at()? <= now || self.bytes()?.len() > MAX_ARTIFACT_BYTES {
            return Err("Choose unexpired bounded original evidence".into());
        }
        Ok(())
    }
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        crate::karma::canonical_json_bytes(self).map_err(|e| e.to_string())
    }
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        canonical_hash("lince.evidence-serving.artifact.v1", self).map_err(|e| e.to_string())
    }
    pub fn allows(&self, organ: &str) -> bool {
        self.allows_reader(&Identity::Organ(organ.into()))
    }
    pub fn allows_reader(&self, reader: &Identity) -> bool {
        match self {
            Self::Receipt(document) => document.public_bundle().is_some_and(|bundle| bundle.publication.scope.audience.allows(reader, &Identity::Organ(bundle.publication.scope.source.origin.clone()))),
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
        let artifact: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        artifact.validate(now)?;
        Ok(artifact)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub record: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub source: Source,
    #[serde(default, skip_serializing_if = "Scope::is_fixed")]
    pub scope: Scope,
    #[serde(default)]
    pub as_organ: bool,
    pub audience: Filter,
    pub lifetime_seconds: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    #[default]
    Fixed,
    Recipe,
}

impl Scope {
    pub fn is_fixed(&self) -> bool {
        *self == Self::Fixed
    }
}

impl Request {
    pub fn validate(&self) -> Result<(), String> {
        self.audience.validate()?;
        if serde_json::to_vec(&self.audience)
            .map_err(|e| e.to_string())?
            .len()
            > crate::visibility::MAX_POLICY_BYTES
        {
            return Err("Serving audience exceeds its 12 KiB policy limit".into());
        }
        if !crate::valid_uid(&self.source.record, "r")
            || (self.scope == Scope::Recipe && self.source.kind != SourceKind::Claim)
            || !(1..=31622400).contains(&self.lifetime_seconds)
            || self
                .audience
                .include
                .iter()
                .chain(&self.audience.exclude)
                .flat_map(|condition| &condition.identities)
                .any(|identity| !matches!(identity, Identity::Organ(_)))
        {
            return Err("Choose retained evidence, Organ audience conditions and a term of at most 366 days".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub person: String,
    pub previous: Option<String>,
    pub request: Request,
    pub artifact: Artifact,
    pub source_version: u64,
    pub source_revision: i64,
    pub reviewed_at: i64,
    pub expires_at: i64,
}

impl Review {
    pub fn term_boundary(&self) -> Result<i64, String> {
        match (self.request.scope, &self.artifact) {
            (Scope::Fixed, artifact) => artifact.expires_at(),
            (Scope::Recipe, Artifact::Claim(document)) => match &document.statement.authorship {
                claims::Authorship::Generated { permit, .. } => Ok(permit.terms.expires_at),
                _ => Err("Recipe sharing requires an original generated claim and permit".into()),
            },
            _ => Err("Recipe sharing is only available for generated claims".into()),
        }
    }

    pub fn allows_current(&self, artifact: &Artifact, version: u64, revision: i64) -> bool {
        if self.source_version != version {
            return false;
        }
        match (self.request.scope, &self.artifact, artifact) {
            (Scope::Fixed, original, current) => {
                self.source_revision == revision && original == current
            }
            (Scope::Recipe, Artifact::Claim(original), Artifact::Claim(current)) => {
                match (
                    &original.statement.authorship,
                    &current.statement.authorship,
                ) {
                    (
                        claims::Authorship::Generated {
                            recipe,
                            revision,
                            permit,
                            ..
                        },
                        claims::Authorship::Generated {
                            recipe: current_recipe,
                            revision: current_revision,
                            permit: current_permit,
                            ..
                        },
                    ) => {
                        recipe == current_recipe
                            && revision == current_revision
                            && permit == current_permit
                            && permit.matches(&original.statement).is_ok()
                            && permit.matches(&current.statement).is_ok()
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub fn hash(&self) -> Result<CanonicalHash, String> {
        self.request.validate()?;
        self.artifact.validate(self.reviewed_at)?;
        if !crate::valid_uid(&self.person, "r")
            || self
                .previous
                .as_ref()
                .is_some_and(|record| !crate::valid_uid(record, "r"))
            || self.source_version == 0
            || self.source_version > i64::MAX as u64
            || self.source_revision < 0
            || self.reviewed_at <= 0
            || self.request.source.kind != self.artifact.kind()
            || self.expires_at
                != self.term_boundary()?.min(
                    self.reviewed_at
                        .saturating_add(i64::from(self.request.lifetime_seconds)),
                )
        {
            return Err("Review the exact original evidence, audience and bounded term".into());
        }
        canonical_hash("lince.evidence-serving.review.v1", self).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Preview {
        request: Request,
    },
    Publish {
        review: Box<Review>,
        expected_review: CanonicalHash,
    },
    List {
        before: Option<String>,
    },
    Inspect {
        record: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Chunk {
    pub hash: CanonicalHash,
    pub offset: u32,
    pub total: u32,
    pub bytes: String,
}

impl Chunk {
    pub fn decode(&self, hash: &CanonicalHash, offset: u32) -> Result<Vec<u8>, String> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        if &self.hash != hash
            || self.offset != offset
            || self.total == 0
            || self.total as usize > MAX_ARTIFACT_BYTES
            || offset as usize % CHUNK_BYTES != 0
            || offset >= self.total
            || self.bytes.len() > CHUNK_BYTES.div_ceil(3) * 4
        {
            return Err("Invalid evidence chunk binding or byte limits".into());
        }
        let bytes = STANDARD
            .decode(&self.bytes)
            .map_err(|_| "Invalid evidence chunk encoding")?;
        if bytes.len() != (self.total - offset).min(CHUNK_BYTES as u32) as usize
            || STANDARD.encode(&bytes) != self.bytes
        {
            return Err("Incomplete evidence chunk".into());
        }
        Ok(bytes)
    }
}

pub fn assemble(hash: &CanonicalHash, chunks: &[Chunk], now: i64) -> Result<Artifact, String> {
    if chunks.is_empty() || chunks.len() > MAX_ARTIFACT_BYTES.div_ceil(CHUNK_BYTES) {
        return Err("Choose a bounded complete sequence of evidence chunks".into());
    }
    let total = chunks[0].total;
    let mut bytes = Vec::new();
    for chunk in chunks {
        if chunk.total != total {
            return Err("Evidence changed size during retrieval".into());
        }
        bytes.extend(chunk.decode(hash, bytes.len() as u32)?);
    }
    if bytes.len() != total as usize {
        return Err("Evidence retrieval is incomplete".into());
    }
    let artifact = Artifact::parse(&bytes, now)?;
    if artifact.hash()? != *hash {
        return Err("Retrieved evidence differs from its requested content hash".into());
    }
    Ok(artifact)
}
