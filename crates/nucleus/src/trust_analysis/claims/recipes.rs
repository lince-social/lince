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
            amount: DecimalValue::from_mantissa(0, 0).unwrap(),
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
        if serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_BYTES
        {
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
        if serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_BYTES
        {
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
