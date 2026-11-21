//! Wire documents for receipt authority, review, publication, and proof fragments.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    Approval, Authority, MAX_BUNDLE_BYTES, MAX_BYTES, PersonApproval, Scope, SignedEvent, digest,
    merge,
    view::{self, Validation},
};
use crate::{karma::CanonicalHash, performance::Span};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub person_approvals: Vec<PersonApproval>,
    pub scope: Scope,
    pub approvals: Vec<Approval>,
    pub events: Vec<SignedEvent>,
}

impl Publication {
    pub(in crate::transfer) fn evidence_time(&self) -> i64 {
        self.events
            .iter()
            .map(|signed| signed.event.issued_at)
            .chain(self.approvals.iter().map(|approval| approval.issued_at))
            .chain(
                self.person_approvals
                    .iter()
                    .map(|approval| approval.issued_at),
            )
            .max()
            .unwrap_or(self.scope.issued_at)
    }

    pub fn parse(bytes: &[u8], now: i64) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Public receipt exceeds its byte budget".into());
        }
        let publication: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        publication.validate(now)?;
        Ok(publication)
    }

    pub fn validate(&self, now: i64) -> Result<(), String> {
        view::Publication::from(self).validate_mode(now, Validation::Complete)
    }

    pub fn validate_fragment(&self, now: i64) -> Result<(), String> {
        view::Publication::from(self).validate_mode(now, Validation::Fragment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub publication: Publication,
    pub authorities: Vec<Authority>,
}

impl Bundle {
    pub fn parse(bytes: &[u8], now: i64) -> Result<Self, String> {
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err("Public receipt bundle exceeds its byte budget".into());
        }
        let bundle: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        bundle.validate(now)?;
        Ok(bundle)
    }

    pub fn validate(&self, now: i64) -> Result<(), String> {
        view::Bundle::from(self).validate_mode(now, Validation::Complete)
    }

    pub fn validate_fragment(&self, now: i64) -> Result<(), String> {
        view::Bundle::from(self).validate_mode(now, Validation::Fragment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub person_approvals: Vec<PersonApproval>,
    pub scope: Scope,
    pub approvals: Vec<Approval>,
    pub authorities: Vec<Authority>,
}

impl Review {
    pub fn as_bundle(&self) -> Bundle {
        self.clone().into_bundle()
    }

    pub fn into_bundle(self) -> Bundle {
        Bundle {
            publication: Publication {
                person_approvals: self.person_approvals,
                scope: self.scope,
                approvals: self.approvals,
                events: Vec::new(),
            },
            authorities: self.authorities,
        }
    }

    pub fn validate(&self, now: i64) -> Result<(), String> {
        let _span = Span::new("receipt-review-validation");
        view::Bundle::from(self).validate_mode(now, Validation::Review)
    }

    pub fn is_complete(&self) -> bool {
        self.approvals
            .iter()
            .map(|approval| approval.role)
            .chain(self.scope.upfront_roles())
            .collect::<BTreeSet<_>>()
            .len()
            == 3
            && self.person_approvals.len() == self.scope.public_persons.len()
    }

    pub fn parse(bytes: &[u8], now: i64) -> Result<Self, String> {
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err("Receipt review exceeds its byte budget".into());
        }
        let review: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        review.validate(now)?;
        Ok(review)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Document {
    Authority(Authority),
    Review(Review),
    Publication(Bundle),
    ProofFragment(Bundle),
}

impl Document {
    pub fn public_bundle(&self) -> Option<&Bundle> {
        match self {
            Self::Publication(bundle) | Self::ProofFragment(bundle) => Some(bundle),
            _ => None,
        }
    }

    pub fn parse(bytes: &[u8], now: i64) -> Result<Self, String> {
        if bytes.len() > MAX_BUNDLE_BYTES + 64 {
            return Err("Receipt document exceeds its byte budget".into());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        document.validate(now)?;
        Ok(document)
    }

    pub fn merge(&self, incoming: &Self, now: i64) -> Result<Self, String> {
        merge::documents(self, incoming, now)
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Authority(_) => "authority",
            Self::Review(_) => "review",
            Self::Publication(_) | Self::ProofFragment(_) => "publication",
        }
    }

    pub fn hash(&self) -> Result<CanonicalHash, String> {
        digest("lince.transfer.receipt.document.v1", self)
    }

    pub fn scope(&self) -> Option<&Scope> {
        match self {
            Self::Review(review) => Some(&review.scope),
            Self::Publication(bundle) | Self::ProofFragment(bundle) => {
                Some(&bundle.publication.scope)
            }
            Self::Authority(_) => None,
        }
    }

    pub fn identity(&self) -> Result<CanonicalHash, String> {
        if matches!(self, Self::ProofFragment(_)) {
            return self.hash();
        }
        match self.scope() {
            Some(scope) => scope.id(),
            None => self.hash(),
        }
    }

    pub fn expires_at(&self) -> i64 {
        match self {
            Self::Authority(authority) => authority.expires_at,
            Self::Review(review) => review.scope.expires_at,
            Self::Publication(bundle) | Self::ProofFragment(bundle) => {
                bundle.publication.scope.expires_at
            }
        }
    }

    pub fn validate(&self, now: i64) -> Result<(), String> {
        match self {
            Self::Authority(authority) => authority.validate(now),
            Self::Review(review) => review.validate(now),
            Self::Publication(bundle) => bundle.validate(now),
            Self::ProofFragment(bundle) => bundle.validate_fragment(now),
        }
    }
}
