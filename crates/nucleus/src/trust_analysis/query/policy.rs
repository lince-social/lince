//! Wire query policy, defaults, and validation.

use std::collections::BTreeSet;

use chrono::DateTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::{Filter, Identity, MAX_IDENTITIES, paths::Policy};
use crate::{
    DecimalValue,
    json::encoded_len,
    karma::{CanonicalHash, canonical_hash},
    transfer::{outcome::Role, receipt::Measurement},
};

pub const NAMESPACE: &str = "lince.trust-query";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    NeedLinkedReceiver,
    Receiver,
    Giver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CounterpartyBasis {
    Organ,
    AttributedPerson,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Weights {
    pub need_linked_receiver: DecimalValue,
    pub receiver: DecimalValue,
    pub giver: DecimalValue,
    pub distance_decay: DecimalValue,
}

impl Default for Weights {
    fn default() -> Self {
        let tenth = |mantissa| {
            DecimalValue::from_mantissa(1, mantissa).expect("one decimal place is supported")
        };
        Self {
            need_linked_receiver: DecimalValue::from_integer(1),
            receiver: tenth(5),
            giver: tenth(1),
            distance_decay: tenth(5),
        }
    }
}

impl Weights {
    fn validate(&self) -> Result<(), String> {
        for weight in [self.need_linked_receiver, self.receiver, self.giver] {
            if weight.is_negative()
                || weight
                    .exact_numeric_cmp(DecimalValue::from_integer(100))
                    .is_gt()
            {
                return Err("Choose evidence weights from 0 to 100".into());
            }
        }
        if self.distance_decay.is_negative()
            || self
                .distance_decay
                .exact_numeric_cmp(DecimalValue::from_integer(1))
                .is_gt()
        {
            return Err("Choose a distance decay from 0 to 1".into());
        }
        Ok(())
    }

    pub(super) fn evidence(&self, level: EvidenceLevel) -> DecimalValue {
        match level {
            EvidenceLevel::NeedLinkedReceiver => self.need_linked_receiver,
            EvidenceLevel::Receiver => self.receiver,
            EvidenceLevel::Giver => self.giver,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub subject: Identity,
    pub role: Role,
    pub counterparty_basis: CounterpartyBasis,
    pub measurements: Vec<Measurement>,
    pub from: Option<i64>,
    pub until: Option<i64>,
    pub levels: Vec<EvidenceLevel>,
    pub endpoints: Filter,
    pub issuers: Filter,
    pub counterparties: Filter,
    pub rely_on_all_visible: bool,
    pub contacts: Option<Policy>,
    pub exclude_everywhere: Vec<Identity>,
    pub weights: Weights,
    pub issuer_cap: Option<DecimalValue>,
    pub first_contact_cap: Option<DecimalValue>,
}

impl Query {
    pub fn for_subject(subject: Identity) -> Self {
        Self {
            subject,
            role: Role::Giver,
            counterparty_basis: CounterpartyBasis::Organ,
            measurements: Vec::new(),
            from: None,
            until: None,
            levels: vec![EvidenceLevel::NeedLinkedReceiver],
            endpoints: Filter::default(),
            issuers: Filter::default(),
            counterparties: Filter::default(),
            rely_on_all_visible: false,
            contacts: None,
            exclude_everywhere: Vec::new(),
            weights: Weights::default(),
            issuer_cap: None,
            first_contact_cap: None,
        }
    }

    /// Check filter, measurement, cap, and encoding limits before evaluation.
    ///
    /// # Errors
    ///
    /// A policy input violates its limits or cannot be encoded.
    pub fn validate(&self) -> Result<(), String> {
        self.subject.validate()?;
        self.endpoints.validate()?;
        self.issuers.validate()?;
        self.counterparties.validate()?;
        self.weights.validate()?;
        if self.measurements.len() > 32
            || self.levels.is_empty()
            || self.levels.len() > 3
            || self.levels.iter().collect::<BTreeSet<_>>().len() != self.levels.len()
            || self.exclude_everywhere.len() > MAX_IDENTITIES
            || self.from.zip(self.until).is_some_and(|(a, b)| a >= b)
            || [self.from, self.until]
                .into_iter()
                .flatten()
                .any(|time| DateTime::from_timestamp(time, 0).is_none())
        {
            return Err(
                "Choose distinct evidence levels, at most 32 measurements and a valid period"
                    .into(),
            );
        }
        for measurement in &self.measurements {
            measurement.validate()?;
        }
        for identity in &self.exclude_everywhere {
            identity.validate()?;
        }
        if let Some(policy) = &self.contacts {
            policy.validate()?;
        } else if self.first_contact_cap.is_some() {
            return Err("A first-contact cap requires an explicit contact-path policy".into());
        }
        if [self.issuer_cap, self.first_contact_cap]
            .into_iter()
            .flatten()
            .any(DecimalValue::is_negative)
        {
            return Err(
                "Influence caps must be nonnegative indices in each measurement basis".into(),
            );
        }
        if encoded_len(self).map_err(|error| error.to_string())? > 65536 {
            return Err("Trust query exceeds 64 KiB".into());
        }
        Ok(())
    }

    /// Hash the validated query policy.
    ///
    /// # Errors
    ///
    /// The policy is invalid or cannot be encoded.
    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.trust-query.policy.v1", self).map_err(|error| error.to_string())
    }
}
