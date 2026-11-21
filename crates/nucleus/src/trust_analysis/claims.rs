// non-lean-yet
// Scope: const NAMESPACE, const MAX_DOCUMENT_BYTES, mod commands, mod recipes.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 69, 70, 72, 416.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 157, 392.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 23, 31, 88, 101, 334.
// - [Clippy: clippy::manual_is_variant_and] manual implementation of `Option::is_none_or`. Sites
//   265, 278.
// - [Clippy: clippy::match_same_arms] these match arms have identical bodies. Sites 403, 407.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 50, 67, 139, 203, 212, 218, 290, 308, 317, 326, 353.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 16, 17,
//   18, 19, 23, 24, 25, 26, 31, 32, 33, 38, 39, 40, 41, 42, 43, 44, 45, 46, 50, 67, 88, 89, 90, 91,
//   92, 93, 94, 95, 101, 103, 104, 105, 109, 120, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131,
//   132, 133, 134, 135, 139, 203, 212, 218, 253, 254, 255, 256, 257, 290, 308, 317, 326, 334, 335,
//   336, 337, 338, 339, 340, 344, 345, 346, 347, 348, 349, 350, 353.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites checking_query:
//   String (67), compare: String (353), id: String (212, 326), parse: String (317), series: String
//   (203), signing_bytes: String (290), trusted_issuer_for_activity: String (218), validate: String
//   (50, 139, 308).

use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    Identity,
    contacts::{Audience, MAX_TERM_SECONDS},
    query::{Analysis, CounterpartyBasis, EvidenceLevel, Query},
};
use crate::{
    DecimalValue,
    karma::{CanonicalHash, canonical_hash, canonical_json_bytes},
    transfer::{outcome::Role, receipt::Measurement},
};

pub const NAMESPACE: &str = "lince.profile-claim";
pub const MAX_DOCUMENT_BYTES: usize = 32768;
pub mod commands;
pub mod recipes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Quantity,
    ReportingOccurrences,
    PositiveOccurrences,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    Exact,
    AtLeast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub subject: Identity,
    pub role: Role,
    pub measurement: Measurement,
    pub level: EvidenceLevel,
    pub metric: Metric,
    pub counterparty_basis: CounterpartyBasis,
    pub from: Option<i64>,
    pub until: Option<i64>,
}

impl Descriptor {
    pub fn validate(&self) -> Result<(), String> {
        self.subject.validate()?;
        self.measurement.validate()?;
        if self
            .from
            .zip(self.until)
            .is_some_and(|(from, until)| from >= until)
            || [self.from, self.until]
                .into_iter()
                .flatten()
                .any(|time| chrono::DateTime::from_timestamp(time, 0).is_none())
        {
            return Err("Choose a valid claim reporting period".into());
        }
        Ok(())
    }

    pub fn checking_query(&self, sources: &Query, as_of: i64) -> Result<Query, String> {
        self.validate()?;
        let mut query = sources.clone();
        query.subject = self.subject.clone();
        query.role = self.role;
        query.measurements = vec![self.measurement.clone()];
        query.levels = vec![self.level];
        query.counterparty_basis = self.counterparty_basis;
        query.from = self.from;
        query.until = Some(
            self.until
                .unwrap_or(as_of.saturating_add(1))
                .min(as_of.saturating_add(1)),
        );
        query.validate()?;
        Ok(query)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Authorship {
    Manual,
    Generated {
        recipe: CanonicalHash,
        revision: u64,
        partial: bool,
        state: recipes::EvaluationState,
        permit: Box<recipes::Permit>,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    #[default]
    Experience,
    Credential,
    Recommendation,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Experience => "Experience claim",
            Self::Credential => "Credential",
            Self::Recommendation => "Activity recommendation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Statement {
    #[serde(default)]
    pub category: Category,
    pub title: String,
    pub issuer: Identity,
    pub claim: String,
    pub revision: u64,
    pub descriptor: Descriptor,
    pub comparator: Comparator,
    pub amount: DecimalValue,
    pub as_of: i64,
    pub issued_at: i64,
    pub expires_at: i64,
    pub withdrawn: bool,
    pub audience: Audience,
    pub authorship: Authorship,
}

impl Statement {
    pub fn validate(&self) -> Result<(), String> {
        self.issuer.validate()?;
        self.descriptor.validate()?;
        if self.category == Category::Recommendation && self.issuer == self.descriptor.subject {
            return Err("Recommendations require a different issuer and subject".into());
        }
        if self.category != Category::Experience && !matches!(self.authorship, Authorship::Manual) {
            return Err(
                "Credentials and recommendations require an explicitly reviewed issuer statement"
                    .into(),
            );
        }
        self.audience.validate()?;
        if !crate::valid_uid(&self.claim, "r")
            || self.title.trim().is_empty()
            || self.title.len() > 256
            || self.title.chars().any(char::is_control)
            || self.revision == 0
            || self.revision > i64::MAX as u64
            || self.amount.is_negative()
            || self.as_of <= 0
            || self.as_of > self.issued_at
            || self.issued_at <= 0
            || self.expires_at <= self.issued_at
            || self.expires_at.saturating_sub(self.issued_at) > MAX_TERM_SECONDS
            || [self.as_of, self.issued_at, self.expires_at]
                .into_iter()
                .any(|time| chrono::DateTime::from_timestamp(time, 0).is_none())
            || self.descriptor.from.is_some_and(|from| from > self.as_of)
            || self
                .descriptor
                .until
                .is_some_and(|until| until > self.as_of.saturating_add(1))
            || (self.withdrawn && !self.amount.is_zero())
        {
            return Err("Choose a public claim identifier, positive revision, bounded term and nonnegative amount as of its declared period".into());
        }
        if self.descriptor.metric != Metric::Quantity && self.amount.rescale(0).is_none() {
            return Err("Occurrence counts must be nonnegative whole numbers".into());
        }
        if matches!(&self.authorship, Authorship::Generated { revision: 0, .. }) {
            return Err("A generated claim must identify a reviewed recipe revision".into());
        }
        if let Authorship::Generated {
            recipe,
            revision,
            state,
            permit,
            ..
        } = &self.authorship
        {
            permit.matches(self)?;
            if recipe != &permit.recipe
                || revision != &permit.revision
                || self.withdrawn != (*state != recipes::EvaluationState::Available)
            {
                return Err(
                    "Generated provenance and availability must match the reviewed permit".into(),
                );
            }
        }
        Ok(())
    }

    pub fn series(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash(
            "lince.profile-claim.series.v1",
            &(&self.issuer, &self.claim),
        )
        .map_err(|error| error.to_string())
    }

    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.profile-claim.statement.v1", self).map_err(|error| error.to_string())
    }
}

pub fn trusted_issuer_for_activity(
    statement: &Statement,
    sources: &Query,
    context: &super::IdentityContext,
    at: i64,
) -> Result<bool, String> {
    statement.validate()?;
    sources.validate()?;
    let explicit = sources.issuers.include.iter().any(|rule| {
        !rule.identities.is_empty()
            || rule.known.is_some()
            || rule.lower.is_some()
            || rule.upper.is_some()
    });
    Ok(statement.category != Category::Experience
        && !statement.withdrawn
        && statement.issued_at <= at
        && at < statement.expires_at
        && sources.subject == statement.descriptor.subject
        && sources.role == statement.descriptor.role
        && sources
            .measurements
            .contains(&statement.descriptor.measurement)
        && context.readable
        && !context.blocked
        && (sources.rely_on_all_visible || explicit)
        && sources.issuers.allows(&statement.issuer, context)
        && !sources.exclude_everywhere.contains(&statement.issuer)
        && !sources
            .exclude_everywhere
            .contains(&statement.descriptor.subject))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub statement: Statement,
    pub key_id: String,
    pub public_key: String,
    pub signature: String,
}

fn validate_key(key_id: &str, public_key: &str) -> Result<(), String> {
    if key_id.is_empty()
        || key_id.len() > 128
        || key_id.chars().any(char::is_control)
        || public_key.len() != 44
        || STANDARD
            .decode(public_key)
            .ok()
            .filter(|bytes| bytes.len() == 32 && STANDARD.encode(bytes) == public_key)
            .is_none()
    {
        return Err("Choose a canonical independently authorized signing key".into());
    }
    Ok(())
}

fn validate_signature(signature: &str) -> Result<(), String> {
    if signature.len() != 88
        || STANDARD
            .decode(signature)
            .ok()
            .filter(|bytes| bytes.len() == 64 && STANDARD.encode(bytes) == signature)
            .is_none()
    {
        return Err("Use a canonical profile-claim signature".into());
    }
    Ok(())
}

impl Document {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.statement.validate()?;
        validate_key(&self.key_id, &self.public_key)?;
        if let Authorship::Generated { permit, .. } = &self.statement.authorship
            && (self.key_id != permit.terms.delegate.key_id
                || self.public_key != permit.terms.delegate.public_key)
        {
            return Err("Sign a generated claim only with its reviewed publishing key".into());
        }
        canonical_json_bytes(&(
            "lince.profile-claim.signature.v1",
            &self.statement,
            &self.key_id,
            &self.public_key,
        ))
        .map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.signing_bytes()?;
        validate_signature(&self.signature)?;
        if crate::json::encoded_len(self).map_err(|error| error.to_string())? > MAX_DOCUMENT_BYTES {
            return Err("Profile claim exceeds 32 KiB".into());
        }
        Ok(())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err("Profile claim exceeds 32 KiB".into());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        document.validate()?;
        Ok(document)
    }

    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.profile-claim.document.v1", self).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportState {
    Unknown,
    PartialSupport,
    MatchesVisibleScope,
    AtLeastSupportedInVisibleScope,
    VisibleEvidenceExceedsClaim,
    ArithmeticUnavailable,
}

#[derive(Debug, Serialize)]
pub struct Support {
    pub state: SupportState,
    pub observed: Option<DecimalValue>,
    pub claimed: DecimalValue,
    pub partial: bool,
    pub observed_only: bool,
    pub policy: CanonicalHash,
}

pub fn compare(
    statement: &Statement,
    query: &Query,
    analysis: &Analysis,
) -> Result<Support, String> {
    statement.validate()?;
    if statement.category != Category::Experience {
        return Err(
            "Credentials and recommendations are issuer assertions, not delivery totals".into(),
        );
    }
    if statement.withdrawn {
        return Err("A withdrawn claim cannot be checked as a current statement".into());
    }
    let expected = statement
        .descriptor
        .checking_query(query, statement.as_of)?;
    if expected != *query || analysis.policy != query.id()? {
        return Err("Check the claim's exact meaning, unit, role, stage and period under the reader's source policy".into());
    }
    let group = analysis.groups.iter().find(|group| {
        group.measurement == statement.descriptor.measurement
            && group.level == statement.descriptor.level
    });
    let observed = match statement.descriptor.metric {
        Metric::Quantity => group.and_then(|group| group.raw_quantity),
        Metric::ReportingOccurrences | Metric::PositiveOccurrences => {
            let sources = analysis
                .rows
                .iter()
                .filter(|row| {
                    row.selected
                        && row.measurement == statement.descriptor.measurement
                        && row.level == statement.descriptor.level
                        && (statement.descriptor.metric != Metric::PositiveOccurrences
                            || row.quantity.is_positive())
                })
                .map(|row| &row.source)
                .collect::<BTreeSet<_>>();
            group.map(|_| DecimalValue::from_integer(sources.len() as i128))
        }
    };
    let state = match observed {
        None if group.is_some() => SupportState::ArithmeticUnavailable,
        None => SupportState::Unknown,
        Some(value) => match value.exact_numeric_cmp(statement.amount) {
            std::cmp::Ordering::Less => SupportState::PartialSupport,
            std::cmp::Ordering::Equal if statement.comparator == Comparator::Exact => {
                SupportState::MatchesVisibleScope
            }
            std::cmp::Ordering::Equal => SupportState::AtLeastSupportedInVisibleScope,
            std::cmp::Ordering::Greater if statement.comparator == Comparator::Exact => {
                SupportState::VisibleEvidenceExceedsClaim
            }
            std::cmp::Ordering::Greater => SupportState::AtLeastSupportedInVisibleScope,
        },
    };
    Ok(Support {
        state,
        observed,
        claimed: statement.amount,
        partial: analysis.partial,
        observed_only: true,
        policy: analysis.policy.clone(),
    })
}

#[cfg(test)]
mod tests;
