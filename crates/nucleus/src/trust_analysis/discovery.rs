use super::{
    Identity,
    serving::{Artifact, MAX_ARTIFACT_BYTES, SourceKind},
};
use crate::{
    karma::{CanonicalHash, canonical_hash},
    transfer::{
        outcome::Role,
        receipt::{ApprovalRole, Measurement},
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_HEADERS: usize = 8;
pub const MAX_QUESTION_BYTES: usize = 12288;
pub const MAX_CURSOR_BYTES: usize = 2048;
pub const PAGE_WORK: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Question {
    Activity {
        subject: Identity,
        role: Role,
        measurements: Vec<Measurement>,
        from: Option<i64>,
        until: Option<i64>,
    },
    ClaimSeries {
        issuer: Identity,
        claim: String,
    },
    Contacts {
        subject: Identity,
    },
    ReceiptSource {
        origin: String,
        occurrence: CanonicalHash,
    },
}

impl Question {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Activity {
                subject,
                measurements,
                from,
                until,
                ..
            } => {
                subject.validate()?;
                if measurements.len() > 32
                    || from.zip(*until).is_some_and(|(a, b)| a >= b)
                    || [*from, *until]
                        .into_iter()
                        .flatten()
                        .any(|at| chrono::DateTime::from_timestamp(at, 0).is_none())
                {
                    return Err(
                        "Choose up to 32 exact measurements and a valid discovery period".into(),
                    );
                }
                let mut seen = BTreeSet::new();
                for measurement in measurements {
                    if !seen.insert(measurement.id()?) {
                        return Err("Discovery measurements must not repeat".into());
                    }
                }
            }
            Self::ClaimSeries { issuer, claim } => {
                issuer.validate()?;
                if !crate::valid_uid(claim, "r") {
                    return Err("Choose a public claim-series identity".into());
                }
            }
            Self::Contacts { subject } => subject.validate()?,
            Self::ReceiptSource { origin, .. } => {
                if !crate::valid_uid(origin, "r") {
                    return Err("Choose a public receipt origin".into());
                }
            }
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_QUESTION_BYTES {
            return Err("Discovery question exceeds 12 KiB".into());
        }
        Ok(())
    }

    pub fn hash(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.evidence-discovery.question.v1", self).map_err(|e| e.to_string())
    }

    pub fn matches(&self, artifact: &Artifact) -> bool {
        match (self, artifact) {
            (Self::ClaimSeries { issuer, claim }, Artifact::Claim(document)) => {
                &document.statement.issuer == issuer && &document.statement.claim == claim
            }
            (Self::Contacts { subject }, Artifact::Contact(document)) => {
                &document.statement.issuer == subject || &document.statement.contact == subject
            }
            (Self::ReceiptSource { origin, occurrence }, Artifact::Receipt(document)) => {
                document.public_bundle().is_some_and(|bundle| {
                    &bundle.publication.scope.source.origin == origin
                        && &bundle.publication.scope.source.occurrence == occurrence
                })
            }
            (
                Self::Activity {
                    subject,
                    role,
                    measurements,
                    from,
                    until,
                },
                Artifact::Claim(document),
            ) => {
                let descriptor = &document.statement.descriptor;
                &descriptor.subject == subject
                    && &descriptor.role == role
                    && (measurements.is_empty() || measurements.contains(&descriptor.measurement))
                    && !from.zip(descriptor.until).is_some_and(|(a, b)| a >= b)
                    && !until.zip(descriptor.from).is_some_and(|(a, b)| a <= b)
            }
            (
                Self::Activity {
                    subject,
                    role,
                    measurements,
                    from,
                    until,
                },
                Artifact::Receipt(document),
            ) => document.public_bundle().is_some_and(|bundle| {
                let scope = &bundle.publication.scope;
                let party_role = match role {
                    Role::Giver => ApprovalRole::Giver,
                    Role::Receiver => ApprovalRole::Receiver,
                };
                let party = match subject {
                    Identity::Organ(uid) => uid == scope.organ(party_role),
                    Identity::Person(uid) => scope
                        .public_persons
                        .iter()
                        .any(|party| &party.person == uid && party.role == party_role),
                };
                party
                    && (measurements.is_empty()
                        || scope
                            .measurements
                            .iter()
                            .any(|measurement| measurements.contains(measurement)))
                    && (from.is_none() && until.is_none()
                        || bundle.publication.events.iter().any(|event| {
                            from.is_none_or(|at| event.event.recorded_at >= at)
                                && until.is_none_or(|at| event.event.recorded_at < at)
                        }))
            }),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub question: Question,
    pub issued_at: i64,
    pub deadline: i64,
    pub after: Option<String>,
}

impl Request {
    pub fn validate(&self, now: i64) -> Result<(), String> {
        self.question.validate()?;
        if !crate::valid_uid(&self.id, "ask")
            || self.issued_at <= 0
            || self.issued_at > now.saturating_add(5)
            || !(1..=15).contains(&self.deadline.saturating_sub(self.issued_at))
            || self.deadline <= now
            || self.deadline > now.saturating_add(20)
            || self
                .after
                .as_ref()
                .is_some_and(|cursor| cursor.is_empty() || cursor.len() > MAX_CURSOR_BYTES)
        {
            return Err(
                "Choose a fresh discovery question with a deadline of at most 15 seconds".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub hash: CanonicalHash,
    pub kind: SourceKind,
    pub bytes: u32,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: String,
    pub question: CanonicalHash,
    pub headers: Vec<Header>,
    pub next: Option<String>,
    pub partial: bool,
}

impl Page {
    pub fn validate(&self, request: &Request, now: i64) -> Result<(), String> {
        request.validate(now)?;
        if self.id != request.id
            || self.question != request.question.hash()?
            || self.headers.len() > MAX_HEADERS
            || self
                .headers
                .iter()
                .map(|header| &header.hash)
                .collect::<BTreeSet<_>>()
                .len()
                != self.headers.len()
            || self.headers.iter().any(|header| {
                header.bytes == 0
                    || header.bytes as usize > MAX_ARTIFACT_BYTES
                    || header.expires_at <= now
            })
            || self
                .next
                .as_ref()
                .is_some_and(|cursor| cursor.is_empty() || cursor.len() > MAX_CURSOR_BYTES)
        {
            return Err("Discovery answer differs from its bounded original question".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            id: crate::new_uid("ask"),
            question: Question::Contacts {
                subject: Identity::Person(crate::new_uid("r")),
            },
            issued_at: 1000,
            deadline: 1015,
            after: None,
        }
    }

    #[test]
    fn questions_exclude_reader_policy_and_bound_freshness_and_continuation() {
        let request = request();
        request.validate(1000).unwrap();
        assert!(request.validate(1015).is_err());
        assert!(request.validate(994).is_err());
        let mut changed = request.clone();
        changed.deadline += 1;
        assert!(changed.validate(1000).is_err());
        changed = request.clone();
        changed.after = Some("x".repeat(MAX_CURSOR_BYTES + 1));
        assert!(changed.validate(1000).is_err());
        let mut value = serde_json::to_value(&request).unwrap();
        value["weights"] = serde_json::json!({"giver":"100"});
        assert!(serde_json::from_value::<Request>(value).is_err());
        let mut value = serde_json::to_value(&request.question).unwrap();
        value["private_person"] = serde_json::json!(crate::new_uid("r"));
        assert!(serde_json::from_value::<Question>(value).is_err());
        let invalid = Question::Activity {
            subject: Identity::Person(crate::new_uid("r")),
            role: Role::Giver,
            measurements: Vec::new(),
            from: Some(1001),
            until: Some(1000),
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn headers_are_bounded_hints_bound_to_the_original_question() {
        let request = request();
        let mut page = Page {
            id: request.id.clone(),
            question: request.question.hash().unwrap(),
            headers: Vec::new(),
            next: None,
            partial: false,
        };
        page.validate(&request, 1000).unwrap();
        page.headers.push(Header {
            hash: page.question.clone(),
            kind: SourceKind::Contact,
            bytes: 320,
            expires_at: 1020,
        });
        page.validate(&request, 1000).unwrap();
        let mut other = request.clone();
        other.id = crate::new_uid("ask");
        assert!(page.validate(&other, 1000).is_err());
        other = request.clone();
        other.question = Question::Contacts {
            subject: Identity::Organ(crate::new_uid("r")),
        };
        assert!(page.validate(&other, 1000).is_err());
        page.headers.push(page.headers[0].clone());
        assert!(page.validate(&request, 1000).is_err());
        page.headers.pop();
        page.headers[0].bytes = MAX_ARTIFACT_BYTES as u32 + 1;
        assert!(page.validate(&request, 1000).is_err());
        page.headers[0].bytes = 320;
        page.headers[0].expires_at = 1000;
        assert!(page.validate(&request, 1000).is_err());
    }

    #[test]
    fn disclosed_contacts_do_not_turn_persons_into_organs_with_the_same_identifier() {
        let uid = crate::new_uid("r");
        let artifact = Artifact::Contact(super::super::contacts::Document {
            statement: super::super::contacts::Statement {
                issuer: Identity::Person(uid.clone()),
                contact: Identity::Organ(crate::new_uid("r")),
                revision: 1,
                issued_at: 1000,
                expires_at: 2000,
                withdrawn: false,
                audience: super::super::contacts::Audience::Public,
            },
            key_id: String::new(),
            public_key: String::new(),
            signature: String::new(),
        });
        assert!(
            Question::Contacts {
                subject: Identity::Person(uid.clone())
            }
            .matches(&artifact)
        );
        assert!(
            !Question::Contacts {
                subject: Identity::Organ(uid)
            }
            .matches(&artifact)
        );
    }
}
