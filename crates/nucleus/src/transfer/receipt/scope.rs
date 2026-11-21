//! Receipt scope, source provenance, and measurements.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ApprovalRole, MAX_MEASURES, OriginalAuthority, Participation, PublicPerson, digest};
use crate::{
    karma::CanonicalHash, transfer::public_history::Terms, trust_analysis::contacts::Audience,
    valid_uid, vocabulary::reference::MeaningReference,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeRequest {
    #[serde(default = "use_upfront_terms")]
    pub use_upfront_terms: bool,
    #[serde(default = "public_audience")]
    pub audience: Audience,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_person_roles: Vec<ApprovalRole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_measurements: Vec<Measurement>,
    pub occurrence: String,
    pub expected_revision: u64,
    pub outcome_records: Vec<String>,
    pub stages: Vec<Stage>,
    pub publish_agreed_quantity: bool,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub meaning: MeaningReference,
    pub unit: Option<MeaningReference>,
    pub object_commitment: Option<CanonicalHash>,
}

impl Measurement {
    pub fn validate(&self) -> Result<(), String> {
        self.meaning.validate().map_err(str::to_owned)?;
        if let Some(unit) = &self.unit {
            unit.validate().map_err(str::to_owned)?;
        }
        Ok(())
    }

    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        digest("lince.transfer.receipt.measure.v1", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedFloat {
    pub ieee754_binary64: String,
}

impl RecordedFloat {
    pub fn new(value: f64) -> Result<Self, String> {
        check_quantity(value)?;
        Ok(Self {
            ieee754_binary64: format!("{:016x}", value.to_bits()),
        })
    }

    pub fn value(&self) -> Result<f64, String> {
        let bits = u64::from_str_radix(&self.ieee754_binary64, 16)
            .map_err(|_| "Invalid recorded binary64 quantity")?;
        let value = f64::from_bits(bits);
        check_quantity(value)?;
        if self.ieee754_binary64.len() != 16
            || !self
                .ieee754_binary64
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err("Recorded binary64 quantity must use 16 lowercase hex digits".into());
        }
        Ok(value)
    }
}

fn check_quantity(value: f64) -> Result<(), String> {
    if !value.is_finite() || value.is_sign_negative() {
        return Err("Canonical quantities must be finite and nonnegative".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub origin: String,
    pub occurrence: CanonicalHash,
    pub activated_at: i64,
    pub agreement_revision: u64,
    pub agreement_commitment: CanonicalHash,
    pub activation_commitment: CanonicalHash,
}

impl Source {
    pub fn fact_commitment(hash: &str) -> Result<CanonicalHash, String> {
        digest("lince.transfer.receipt.fact.v1", &hash)
    }

    pub fn occurrence_id(origin: &str, occurrence: &str) -> Result<CanonicalHash, String> {
        if !valid_uid(origin, "r") || !valid_uid(occurrence, "toc") {
            return Err("Choose a canonical origin Organ and resource occurrence".into());
        }
        digest("lince.transfer.receipt.source.v1", &(origin, occurrence))
    }

    pub fn validate(&self) -> Result<(), String> {
        if !valid_uid(&self.origin, "r") || self.agreement_revision == 0 || self.activated_at <= 0 {
            return Err("Receipt source requires an origin Organ and agreement revision".into());
        }
        Ok(())
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Need,
    Giver,
    Receiver,
    Settled,
    SettlementStatus,
    Cancelled,
    Compensated,
    Dispute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PartyAttribution {
    LaterOrganEndorsement,
    OriginalOrganAuthority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub audience: Audience,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upfront_terms: Option<Terms>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_persons: Vec<PublicPerson>,
    pub source: Source,
    pub party_attribution: PartyAttribution,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_authorities: Vec<OriginalAuthority>,
    pub giver: String,
    pub receiver: String,
    pub measurements: Vec<Measurement>,
    pub stages: Vec<Stage>,
    pub agreed_quantity: Option<RecordedFloat>,
    pub canonical_unit: Option<MeaningReference>,
    pub issued_at: i64,
    pub expires_at: i64,
}

impl Scope {
    pub fn validate(&self) -> Result<(), String> {
        self.audience.validate()?;
        if self
            .upfront_terms
            .as_ref()
            .is_some_and(|terms| !terms.permits(self))
        {
            return Err("The receipt exceeds its signed upfront public-history terms".into());
        }
        self.source.validate()?;
        let mut person_roles = BTreeSet::new();
        let mut persons = BTreeSet::new();
        if self.public_persons.len() > 2
            || self.public_persons.iter().any(|target| {
                target.role == ApprovalRole::Origin
                    || !valid_uid(&target.person, "r")
                    || !person_roles.insert(target.role)
                    || !persons.insert(&target.person)
            })
        {
            return Err(
                "Public Person attribution requires distinct original giver/receiver identities"
                    .into(),
            );
        }
        if !valid_uid(&self.giver, "r")
            || !valid_uid(&self.receiver, "r")
            || self.issued_at <= 0
            || self.source.activated_at > self.issued_at
            || self.expires_at <= self.issued_at
            || chrono::DateTime::from_timestamp(self.expires_at, 0).is_none()
            || self.measurements.len() > MAX_MEASURES
            || self.stages.is_empty()
            || self.stages.len() > 8
        {
            return Err("Invalid public receipt parties, scope or retention term".into());
        }
        let mut stages = self.stages.iter();
        while let Some(stage) = stages.next() {
            if stages.as_slice().contains(stage) {
                return Err("Receipt stages must not repeat".into());
            }
        }
        let mut measures = BTreeSet::new();
        for measure in &self.measurements {
            if !measures.insert(measure.id()?) {
                return Err("Receipt measurements must not repeat".into());
            }
        }
        if self
            .stages
            .iter()
            .any(|stage| matches!(stage, Stage::Need | Stage::Giver | Stage::Receiver))
            && self.measurements.is_empty()
        {
            return Err("Outcome publication requires reviewed measurements".into());
        }
        if let Some(quantity) = &self.agreed_quantity {
            quantity.value()?;
        }
        if let Some(unit) = &self.canonical_unit {
            unit.validate().map_err(str::to_owned)?;
        }
        match self.party_attribution {
            PartyAttribution::LaterOrganEndorsement if !self.original_authorities.is_empty() => {
                return Err("Later endorsement cannot claim original Organ authority".into());
            }
            PartyAttribution::OriginalOrganAuthority => {
                if self.original_authorities.len() != 3 {
                    return Err(
                        "Original Organ participation requires all three role proofs".into(),
                    );
                }
                let mut roles = BTreeSet::new();
                let mut parties = BTreeSet::new();
                for proof in &self.original_authorities {
                    proof.validate(self)?;
                    if let Some(terms) = &self.upfront_terms
                        && proof
                            .authority
                            .participation
                            .as_ref()
                            .and_then(|participation| participation.public_history.as_ref())
                            != Some(&terms.id()?)
                    {
                        return Err("The upfront manifest differs from the original signed participation grant".into());
                    }
                    if !roles.insert(proof.role) {
                        return Err("Original Organ participation repeats a role".into());
                    }
                    if let Some(party) = &proof
                        .authority
                        .participation
                        .as_ref()
                        .ok_or("Original participation proof has no agreement scope")?
                        .party_commitment
                        && !parties.insert(party)
                    {
                        return Err("Original giver and receiver must be distinct parties".into());
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        self.validated_id()
    }

    pub(super) fn validated_id(&self) -> Result<CanonicalHash, String> {
        digest("lince.transfer.receipt.scope.v1", self)
    }

    pub fn organ(&self, role: ApprovalRole) -> &str {
        match role {
            ApprovalRole::Origin => &self.source.origin,
            ApprovalRole::Giver => &self.giver,
            ApprovalRole::Receiver => &self.receiver,
        }
    }

    pub fn upfront_roles(&self) -> BTreeSet<ApprovalRole> {
        let Some(terms) = self
            .upfront_terms
            .as_ref()
            .filter(|terms| terms.permits(self))
        else {
            return BTreeSet::new();
        };
        if self.party_attribution != PartyAttribution::OriginalOrganAuthority {
            return BTreeSet::new();
        }
        let Ok(commitment) = terms.id() else {
            return BTreeSet::new();
        };
        self.original_authorities
            .iter()
            .filter(|proof| {
                proof.role != ApprovalRole::Origin
                    && proof
                        .authority
                        .participation
                        .as_ref()
                        .and_then(|participation| participation.public_history.as_ref())
                        == Some(&commitment)
            })
            .map(|proof| proof.role)
            .collect()
    }

    pub fn opens_original_party(
        &self,
        role: ApprovalRole,
        transfer: &str,
        person: &str,
    ) -> Result<bool, String> {
        if self.party_attribution != PartyAttribution::OriginalOrganAuthority
            || role == ApprovalRole::Origin
        {
            return Ok(false);
        }
        let commitment = Participation::party_commitment(
            &self.source.origin,
            transfer,
            self.source.agreement_revision,
            person,
        )?;
        Ok(self
            .original_authorities
            .iter()
            .find(|proof| proof.role == role)
            .and_then(|proof| proof.authority.participation.as_ref())
            .is_some_and(|terms| terms.party_commitment.as_ref() == Some(&commitment)))
    }
}

fn public_audience() -> Audience {
    Audience::Public
}

fn use_upfront_terms() -> bool {
    true
}
