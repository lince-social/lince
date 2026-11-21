//! Remote evidence questions and the public artifacts they can match.

use std::collections::BTreeSet;

use chrono::DateTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    super::{Identity, serving::Artifact},
    MAX_QUESTION_BYTES,
};
use crate::{
    json::encoded_len,
    karma::{CanonicalHash, canonical_hash},
    transfer::{
        outcome::Role,
        receipt::{ApprovalRole, Measurement},
    },
    valid_uid,
};

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
    /// Check that the question contains bounded public criteria.
    ///
    /// # Errors
    ///
    /// An identity, time range, or measurement is invalid or the question exceeds its budget.
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
                        .any(|at| DateTime::from_timestamp(at, 0).is_none())
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
                if !valid_uid(claim, "r") {
                    return Err("Choose a public claim-series identity".into());
                }
            }
            Self::Contacts { subject } => subject.validate()?,
            Self::ReceiptSource { origin, .. } => {
                if !valid_uid(origin, "r") {
                    return Err("Choose a public receipt origin".into());
                }
            }
        }
        if encoded_len(self).map_err(|error| error.to_string())? > MAX_QUESTION_BYTES {
            return Err("Discovery question exceeds 12 KiB".into());
        }
        Ok(())
    }

    pub fn hash(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        self.validated_hash()
    }

    pub(super) fn validated_hash(&self) -> Result<CanonicalHash, String> {
        canonical_hash("lince.evidence-discovery.question.v1", self)
            .map_err(|error| error.to_string())
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
                    && from.zip(descriptor.until).is_none_or(|(a, b)| a < b)
                    && until.zip(descriptor.from).is_none_or(|(a, b)| a > b)
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
