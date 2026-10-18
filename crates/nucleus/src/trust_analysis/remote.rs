use super::{Identity, commands::ProfileVersion, discovery::Question, query::Query};
use crate::karma::{CanonicalHash, canonical_hash};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_PEERS: usize = 32;
pub const MAX_JOBS: usize = 8;
pub const MAX_REVIEWS: usize = 32;
pub const LEASE_SECONDS: u64 = 5;
pub const DEADLINE_SECONDS: u64 = 15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Series {
    pub issuer: Identity,
    pub claim: String,
    #[serde(default)]
    pub snapshot: Option<super::claims::Document>,
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
    pub fn questions(&self) -> Result<Vec<Question>, String> {
        self.query.validate()?;
        if self.query.exclude_everywhere.contains(&self.query.subject)
            || self
                .linked_claims
                .iter()
                .any(|series| self.query.exclude_everywhere.contains(&series.issuer))
            || self.receipt_sources.iter().any(|source| {
                self.query
                    .exclude_everywhere
                    .contains(&Identity::Organ(source.origin.clone()))
            })
            || self
                .contact_subjects
                .iter()
                .any(|identity| self.query.exclude_everywhere.contains(identity))
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
                if document.statement.issuer != series.issuer || document.statement.claim != series.claim {
                    return Err("The reviewed claim snapshot differs from its requested series".into());
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
        for source in &self.receipt_sources {
            questions.push(Question::ReceiptSource {
                origin: source.origin.clone(),
                occurrence: source.occurrence.clone(),
            });
        }
        for subject in &self.contact_subjects {
            questions.push(Question::Contacts {
                subject: subject.clone(),
            });
        }
        let mut seen = BTreeSet::new();
        for question in &questions {
            question.validate()?;
            if !seen.insert(question.hash()?) {
                return Err("Discovery questions must not repeat".into());
            }
        }
        Ok(questions)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub organ: String,
    pub endpoint: String,
    pub label: String,
    pub profile_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub nonce: String,
    pub person: String,
    pub draft: Draft,
    pub questions: Vec<Question>,
    pub endpoints: Vec<Endpoint>,
    pub profiles: Vec<ProfileVersion>,
    pub reviewed_at: i64,
}

impl Review {
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        if !crate::valid_uid(&self.nonce, "ask")
            || !crate::valid_uid(&self.person, "r")
            || self.reviewed_at <= 0
            || self.endpoints.is_empty()
            || self.endpoints.len() > MAX_PEERS
            || self.profiles.len() > 70
            || self
                .profiles
                .iter()
                .any(|profile| profile.identity.validate().is_err() || profile.revision < 0)
            || self.questions != self.draft.questions()?
            || self.endpoints.iter().any(|peer| {
                !crate::valid_uid(&peer.organ, "r")
                    || peer.endpoint.len() != 64
                    || !peer
                        .endpoint
                        .bytes()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                    || peer.profile_revision < 0
                    || peer.label.len() > 640
            })
            || self
                .endpoints
                .iter()
                .map(|peer| &peer.organ)
                .collect::<BTreeSet<_>>()
                .len()
                != self.endpoints.len()
            || self
                .endpoints
                .iter()
                .map(|peer| &peer.endpoint)
                .collect::<BTreeSet<_>>()
                .len()
                != self.endpoints.len()
        {
            return Err("Review the exact questions, reader and distinct pinned recipients".into());
        }
        canonical_hash("lince.trust-remote.review.v1", self).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Preview {
        draft: Draft,
    },
    Start {
        review: Box<Review>,
        expected_review: CanonicalHash,
    },
    Poll {
        job: String,
    },
    Verify {
        job: String,
    },
    Cancel {
        job: String,
    },
    Inspect {
        job: String,
        artifact: CanonicalHash,
    },
    Forget {
        job: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Queued,
    Running,
    Completed,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    Accepted,
    UnknownIdentity,
    WithheldAuthority,
    Incomplete,
    Invalid,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review() -> Review {
        let draft = Draft {
            query: Query::for_subject(Identity::Organ(crate::new_uid("r"))),
            as_organ: false,
            linked_claims: Vec::new(),
            include_contacts: true,
            receipt_sources: Vec::new(),
            contact_subjects: Vec::new(),
        };
        Review {
            nonce: crate::new_uid("ask"),
            person: crate::new_uid("r"),
            questions: draft.questions().unwrap(),
            draft,
            endpoints: vec![Endpoint {
                organ: crate::new_uid("r"),
                endpoint: "a".repeat(64),
                label: "Selected source".into(),
                profile_revision: 0,
            }],
            profiles: Vec::new(),
            reviewed_at: 1000,
        }
    }

    #[test]
    fn outgoing_questions_do_not_contain_reader_policy_but_the_review_binds_it() {
        let original = review();
        let mut changed = original.clone();
        changed.draft.query.weights.giver = crate::DecimalValue::parse_inferred("10").unwrap();
        assert_eq!(changed.draft.questions().unwrap(), original.questions);
        assert_ne!(changed.hash().unwrap(), original.hash().unwrap());
        let question = serde_json::to_string(&original.questions).unwrap();
        assert!(!question.contains(&original.person));
        assert!(!question.contains("weights"));
        assert!(!question.contains("endpoints"));
        changed.draft.as_organ = true;
        assert_ne!(changed.hash().unwrap(), original.hash().unwrap());
    }

    #[test]
    fn review_rejects_duplicate_recipients_and_edited_public_questions() {
        let original = review();
        original.hash().unwrap();
        let mut changed = original.clone();
        changed.endpoints.push(changed.endpoints[0].clone());
        assert!(changed.hash().is_err());
        changed = original.clone();
        changed.questions.clear();
        assert!(changed.hash().is_err());
        changed = original.clone();
        changed.endpoints[0].endpoint = "unreviewed-address".into();
        assert!(changed.hash().is_err());
        changed = original.clone();
        changed.draft.linked_claims = (0..5)
            .map(|_| Series {
                issuer: Identity::Person(crate::new_uid("r")),
                claim: crate::new_uid("r"),
                snapshot: None,
            })
            .collect();
        assert!(changed.hash().is_err());
    }

    #[test]
    fn additional_public_context_needs_an_exact_new_question_review_and_keeps_private_anchors_local()
     {
        let original = review();
        let mut expanded = original.clone();
        expanded.draft.receipt_sources.push(ReceiptSource {
            origin: crate::new_uid("r"),
            occurrence: canonical_hash("lince.test.source.v1", &"public occurrence").unwrap(),
        });
        expanded
            .draft
            .contact_subjects
            .push(Identity::Organ(crate::new_uid("r")));
        assert!(expanded.hash().is_err());
        expanded.questions = expanded.draft.questions().unwrap();
        assert_ne!(expanded.hash().unwrap(), original.hash().unwrap());
        assert_eq!(expanded.questions.len(), original.questions.len() + 2);
        let encoded = serde_json::to_string(&expanded.questions).unwrap();
        assert!(!encoded.contains(&original.person));
        expanded
            .draft
            .query
            .exclude_everywhere
            .push(expanded.draft.contact_subjects[0].clone());
        assert!(expanded.draft.questions().is_err());
        expanded.draft.query.exclude_everywhere.clear();
        let subject = expanded.draft.contact_subjects[0].clone();
        expanded.draft.contact_subjects.push(subject);
        assert!(expanded.draft.questions().is_err());
    }
}
