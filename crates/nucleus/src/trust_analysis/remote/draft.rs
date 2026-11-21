//! Build public discovery questions from a reviewed reader draft.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::{Identity, claims, discovery::Question, query::Query};
use crate::karma::CanonicalHash;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Series {
    pub issuer: Identity,
    pub claim: String,
    #[serde(default)]
    pub snapshot: Option<claims::Document>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub query: Query,
    pub as_organ: bool,
    pub linked_claims: Vec<Series>,
    pub include_contacts: bool,
    #[serde(default)]
    pub receipt_sources: Vec<ReceiptSource>,
    #[serde(default)]
    pub contact_subjects: Vec<Identity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReceiptSource {
    pub origin: String,
    pub occurrence: CanonicalHash,
}

impl Draft {
    /// Build only the public questions covered by this reader draft.
    ///
    /// # Errors
    ///
    /// The policy or a question is invalid, excluded, duplicated, or over budget.
    pub fn questions(&self) -> Result<Vec<Question>, String> {
        self.query.validate()?;
        let excluded = &self.query.exclude_everywhere;
        if excluded.contains(&self.query.subject)
            || self
                .linked_claims
                .iter()
                .any(|series| excluded.contains(&series.issuer))
            || self.receipt_sources.iter().any(|source| {
                excluded.iter().any(
                    |identity| matches!(identity, Identity::Organ(uid) if uid == &source.origin),
                )
            })
            || self
                .contact_subjects
                .iter()
                .any(|identity| excluded.contains(identity))
        {
            return Err("A reviewed question cannot name an identity excluded everywhere".into());
        }
        if self.linked_claims.len() > 4
            || self.receipt_sources.len() > 32
            || self.contact_subjects.len() > 32
        {
            return Err(
                "Choose at most four claim series and 32 public receipt sources/contact subjects"
                    .into(),
            );
        }
        let mut questions = vec![Question::Activity {
            subject: self.query.subject.clone(),
            role: self.query.role,
            measurements: self.query.measurements.clone(),
            from: self.query.from,
            until: self.query.until,
        }];
        for series in &self.linked_claims {
            if let Some(document) = &series.snapshot {
                document.validate()?;
                if document.statement.issuer != series.issuer
                    || document.statement.claim != series.claim
                {
                    return Err(
                        "The reviewed claim snapshot differs from its requested series".into(),
                    );
                }
            }
            questions.push(Question::ClaimSeries {
                issuer: series.issuer.clone(),
                claim: series.claim.clone(),
            });
        }
        if self.include_contacts {
            questions.push(Question::Contacts {
                subject: self.query.subject.clone(),
            });
        }
        questions.extend(
            self.receipt_sources
                .iter()
                .map(|source| Question::ReceiptSource {
                    origin: source.origin.clone(),
                    occurrence: source.occurrence.clone(),
                }),
        );
        questions.extend(
            self.contact_subjects
                .iter()
                .cloned()
                .map(|subject| Question::Contacts { subject }),
        );
        let mut seen = BTreeSet::new();
        for question in &questions {
            if !seen.insert(question.hash()?) {
                return Err("Discovery questions must not repeat".into());
            }
        }
        Ok(questions)
    }
}
