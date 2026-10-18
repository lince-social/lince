use crate::karma::{CanonicalHash, canonical_hash, canonical_json_bytes};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "lince.identity.organ-root";
pub const MAX_ROOTS: u64 = 64;
pub const MAX_PROOF_BYTES: usize = 16384;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub organ: String,
    pub public_key: String,
    pub nonce: String,
    pub issued_at: i64,
    pub signature: String,
}

impl Proof {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        if self.public_key.len() != 44 {
            return Err("Use a canonical Ed25519 public key".into());
        }
        let key = STANDARD
            .decode(&self.public_key)
            .map_err(|_| "Use a canonical Ed25519 public key")?;
        if !crate::valid_uid(&self.organ, "r")
            || !crate::valid_uid(&self.nonce, "r")
            || key.len() != 32
            || STANDARD.encode(key) != self.public_key
            || self.issued_at <= 0
            || chrono::DateTime::from_timestamp(self.issued_at, 0).is_none()
        {
            return Err(
                "Choose an Organ, canonical public key and valid key-control evidence time".into(),
            );
        }
        canonical_json_bytes(&(
            "lince.identity.organ-root-possession.v1",
            &self.organ,
            &self.public_key,
            &self.nonce,
            self.issued_at,
        ))
        .map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.signing_bytes()?;
        if self.signature.len() != 88 {
            return Err("Use a canonical key-control signature".into());
        }
        let signature = STANDARD
            .decode(&self.signature)
            .map_err(|_| "Use a canonical key-control signature")?;
        if signature.len() != 64 || STANDARD.encode(signature) != self.signature {
            return Err("Use a canonical key-control signature".into());
        }
        Ok(())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PROOF_BYTES {
            return Err("Organ key-control proof exceeds 16 KiB".into());
        }
        let proof: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        proof.validate()?;
        Ok(proof)
    }

    pub fn fingerprint(&self) -> Result<CanonicalHash, String> {
        fingerprint(&self.organ, &self.public_key)
    }
}

pub fn fingerprint(organ: &str, public_key: &str) -> Result<CanonicalHash, String> {
    canonical_hash("lince.identity.organ-root-key.v1", &(organ, public_key))
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyStatus {
    pub public_key: String,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub original: Option<KeyStatus>,
    pub current: Option<KeyStatus>,
    pub revision: u64,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub person: String,
    pub proof: Proof,
    pub profile_revision: i64,
    pub previous: State,
    pub reviewed_at: i64,
}

impl Review {
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        self.proof.validate()?;
        if !crate::valid_uid(&self.person, "r")
            || self.profile_revision < 0
            || self.previous.revision >= MAX_ROOTS
            || self.reviewed_at < self.proof.issued_at
            || self.previous.original.is_none()
        {
            return Err(
                "Review an existing Organ identity and its replacement root before accepting"
                    .into(),
            );
        }
        canonical_hash("lince.identity.organ-root-review.v1", self)
            .map_err(|error| error.to_string())
    }
}
