// non-lean-yet
// Scope: const NAMESPACE, const MAX_BYTES, mod commands, struct Delegate.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 35, 36, 37, 39,
//   46.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 85, 86, 144.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 212, 222.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 32, 79, 123, 141, 158, 163, 190, 204.
// - [Clippy: clippy::wildcard_imports] Use explicit imports while preserving framework and test
//   conventions. Sites 1.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 3, 4,
//   5, 9, 10, 11, 12, 13, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 32, 67, 68, 69, 70, 71, 72,
//   73, 74, 75, 79, 123, 131, 132, 133, 134, 135, 136, 137, 141, 158, 163, 184, 185, 186, 190, 204,
//   212, 213, 214, 215, 216, 217, 222, 223, 224, 225, 229.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites id: String
//   (123), matches: String (163), signing_bytes: String (141), validate: String (32, 79, 158, 204),
//   validate_unsigned: String (190).

use super::*;

pub const NAMESPACE: &str = "lince.profile-claim-recipe";
pub const MAX_BYTES: usize = 98304;
pub mod commands;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delegate {
    pub organ: String,
    pub cell: String,
    pub key_id: String,
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Terms {
    pub issuer: Identity,
    pub claim: String,
    pub title: String,
    pub descriptor: Descriptor,
    pub comparator: Comparator,
    pub audience: Audience,
    pub issued_at: i64,
    pub expires_at: i64,
    pub lifetime_seconds: u32,
    pub delegate: Delegate,
}

impl Terms {
    pub fn validate(&self) -> Result<(), String> {
        let statement = Statement {
            category: Default::default(),
            title: self.title.clone(),
            issuer: self.issuer.clone(),
            claim: self.claim.clone(),
            revision: 1,
            descriptor: self.descriptor.clone(),
            comparator: self.comparator,
            amount: DecimalValue::from_integer(0),
            as_of: self.issued_at,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            withdrawn: false,
            audience: self.audience.clone(),
            authorship: Authorship::Manual,
        };
        statement.validate()?;
        if !crate::valid_uid(&self.delegate.organ, "r")
            || !crate::valid_uid(&self.delegate.cell, "r")
            || self.lifetime_seconds < 60
            || i64::from(self.lifetime_seconds) > self.expires_at - self.issued_at
        {
            return Err(
                "Choose a signing Cell and an output lifetime within the reviewed permit term"
                    .into(),
            );
        }
        validate_key(&self.delegate.key_id, &self.delegate.public_key)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub starting_claim_revision: u64,
    pub recipe: String,
    pub revision: u64,
    pub controller: String,
    pub terms: Terms,
    pub inputs: Query,
    pub start_enabled: bool,
    pub refresh_seconds: u32,
}

impl Definition {
    pub fn validate(&self) -> Result<(), String> {
        self.terms.validate()?;
        self.inputs.validate()?;
        if !crate::valid_uid(&self.recipe, "r")
            || !crate::valid_uid(&self.controller, "r")
            || self.revision == 0
            || self.revision > i64::MAX as u64
            || self.starting_claim_revision > i64::MAX as u64
            || !(30..=86400).contains(&self.refresh_seconds)
        {
            return Err("Choose a controller, recipe revision and local refresh interval of 30–86400 seconds".into());
        }
        let mut pinned = self
            .terms
            .descriptor
            .checking_query(&self.inputs, self.terms.issued_at)?;
        pinned.until = self.terms.descriptor.until;
        if pinned != self.inputs {
            return Err("The recipe input activity, role, measurement and period must match its reviewed output".into());
        }
        if self.inputs.contacts.is_some()
            || self.inputs.endpoints != super::super::Filter::default()
            || [&self.inputs.issuers, &self.inputs.counterparties]
                .into_iter()
                .any(|filter| {
                    filter
                        .include
                        .iter()
                        .chain(&filter.exclude)
                        .any(|condition| {
                            condition.known.is_some()
                                || condition.lower.is_some()
                                || condition.upper.is_some()
                        })
                })
        {
            return Err("Published automatic summaries use approved public receipts and explicit identities, without private contact paths, known/proximity criteria or network endpoints".into());
        }
        if crate::json::encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES {
            return Err("Publication recipe exceeds 96 KiB".into());
        }
        Ok(())
    }

    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.profile-claim.recipe.v1", self).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Permit {
    pub recipe: CanonicalHash,
    pub revision: u64,
    pub terms: Terms,
    pub key_id: String,
    pub public_key: String,
    pub signature: String,
}

impl Permit {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.terms.validate()?;
        validate_key(&self.key_id, &self.public_key)?;
        if self.revision == 0 || self.revision > i64::MAX as u64 {
            return Err("Choose a reviewed permit revision".into());
        }
        canonical_json_bytes(&(
            "lince.profile-claim.recipe-permit.signature.v1",
            &self.recipe,
            self.revision,
            &self.terms,
            &self.key_id,
            &self.public_key,
        ))
        .map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.signing_bytes()?;
        validate_signature(&self.signature)
    }

    pub fn matches(&self, statement: &Statement) -> Result<(), String> {
        self.validate()?;
        let terms = &self.terms;
        if terms.issuer != statement.issuer
            || terms.claim != statement.claim
            || terms.title != statement.title
            || terms.descriptor != statement.descriptor
            || terms.comparator != statement.comparator
            || terms.audience != statement.audience
            || statement.issued_at < terms.issued_at
            || statement.expires_at > terms.expires_at
            || statement.expires_at - statement.issued_at > i64::from(terms.lifetime_seconds)
        {
            return Err("The generated claim exceeds its reviewed signing permit".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub definition: Definition,
    pub permit: Permit,
}

impl Recipe {
    pub fn validate_unsigned(&self) -> Result<(), String> {
        if self.definition.id()? != self.permit.recipe
            || self.definition.revision != self.permit.revision
            || self.definition.terms != self.permit.terms
        {
            return Err("The permit must bind this exact recipe and output scope".into());
        }
        self.permit.signing_bytes()?;
        if crate::json::encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES {
            return Err("Signed recipe exceeds 96 KiB".into());
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.validate_unsigned()?;
        self.permit.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationState {
    Available,
    Unknown,
    ArithmeticUnavailable,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Enabled,
    Paused,
    Revoked,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Paused => "paused",
            Self::Revoked => "revoked",
        }
    }
}
