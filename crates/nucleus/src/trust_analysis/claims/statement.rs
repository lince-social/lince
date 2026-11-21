//! Signed claim statements and the reporting scope they describe.

use chrono::DateTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::{
    Identity,
    contacts::{Audience, MAX_TERM_SECONDS},
    query::{CounterpartyBasis, EvidenceLevel, Query},
};
use super::recipes;
use crate::{
    DecimalValue,
    karma::{CanonicalHash, canonical_hash},
    transfer::{outcome::Role, receipt::Measurement},
    valid_uid,
};

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
                .any(|time| DateTime::from_timestamp(time, 0).is_none())
        {
            return Err("Choose a valid claim reporting period".into());
        }
        Ok(())
    }

    /// Pin the reporting scope while keeping the reader's source policy.
    ///
    /// # Errors
    ///
    /// The descriptor or resulting policy violates its input limits.
    pub fn checking_query(&self, sources: &Query, as_of: i64) -> Result<Query, String> {
        self.validate()?;
        let until = as_of.saturating_add(1);
        let query = Query {
            subject: self.subject.clone(),
            role: self.role,
            counterparty_basis: self.counterparty_basis,
            measurements: vec![self.measurement.clone()],
            from: self.from,
            until: Some(self.until.unwrap_or(until).min(until)),
            levels: vec![self.level],
            endpoints: sources.endpoints.clone(),
            issuers: sources.issuers.clone(),
            counterparties: sources.counterparties.clone(),
            rely_on_all_visible: sources.rely_on_all_visible,
            contacts: sources.contacts.clone(),
            exclude_everywhere: sources.exclude_everywhere.clone(),
            weights: sources.weights.clone(),
            issuer_cap: sources.issuer_cap,
            first_contact_cap: sources.first_contact_cap,
        };
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
        if !valid_uid(&self.claim, "r")
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
                .any(|time| DateTime::from_timestamp(time, 0).is_none())
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
        if let Authorship::Generated {
            recipe,
            revision,
            state,
            permit,
            ..
        } = &self.authorship
        {
            if *revision == 0 {
                return Err("A generated claim must identify a reviewed recipe revision".into());
            }
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
