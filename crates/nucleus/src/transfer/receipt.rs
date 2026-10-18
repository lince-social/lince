use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    DecimalValue,
    karma::{CanonicalHash, canonical_hash},
    social::RootSuccession,
    vocabulary::reference::MeaningReference,
};

use super::outcome::Role;

pub const MAX_BYTES: usize = 524_288;
pub const MAX_EVENTS: usize = 256;
pub const MAX_MEASURES: usize = 32;
pub const MAX_AUTHORITIES: usize = 64;
pub const MAX_AUTHORITY_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const MAX_BUNDLE_BYTES: usize = MAX_BYTES + 262_144;
pub const NAMESPACE: &str = "lince.transfer-receipt";

pub fn original_authority_records(values: &[String]) -> Result<Vec<String>, String> {
    let mut records = values.to_vec();
    records.sort();
    if records.len() > MAX_AUTHORITIES
        || records.iter().any(|uid| !crate::valid_uid(uid, "r"))
        || records.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err("Select at most 64 distinct original participation approval Records".into());
    }
    Ok(records)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeRequest {
    #[serde(default = "use_upfront_terms")]
    pub use_upfront_terms: bool,
    #[serde(default = "public_audience")]
    pub audience: crate::trust_analysis::contacts::Audience,
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

fn digest<T: Serialize>(domain: &str, value: &T) -> Result<CanonicalHash, String> {
    canonical_hash(domain, value).map_err(|error| error.to_string())
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
        if !value.is_finite() || value.is_sign_negative() {
            return Err("Canonical quantities must be finite and nonnegative".into());
        }
        Ok(Self {
            ieee754_binary64: format!("{:016x}", value.to_bits()),
        })
    }

    pub fn value(&self) -> Result<f64, String> {
        let bits = u64::from_str_radix(&self.ieee754_binary64, 16)
            .map_err(|_| "Invalid recorded binary64 quantity")?;
        let value = f64::from_bits(bits);
        if Self::new(value)? != *self {
            return Err("Recorded binary64 quantity must use 16 lowercase hex digits".into());
        }
        Ok(value)
    }
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
        if !crate::valid_uid(origin, "r") || !crate::valid_uid(occurrence, "toc") {
            return Err("Choose a canonical origin Organ and resource occurrence".into());
        }
        digest("lince.transfer.receipt.source.v1", &(origin, occurrence))
    }

    pub fn validate(&self) -> Result<(), String> {
        if !crate::valid_uid(&self.origin, "r")
            || self.agreement_revision == 0
            || self.activated_at <= 0
        {
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
    pub audience: crate::trust_analysis::contacts::Audience,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upfront_terms: Option<super::public_history::Terms>,
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
        if self.upfront_terms.as_ref().is_some_and(|terms| !terms.permits(self)) {
            return Err("The receipt exceeds its signed upfront public-history terms".into());
        }
        self.source.validate()?;
        let mut person_roles = BTreeSet::new();
        let mut persons = BTreeSet::new();
        if self.public_persons.len() > 2
            || self.public_persons.iter().any(|target| {
                target.role == ApprovalRole::Origin
                    || !crate::valid_uid(&target.person, "r")
                    || !person_roles.insert(target.role)
                    || !persons.insert(&target.person)
            })
        {
            return Err(
                "Public Person attribution requires distinct original giver/receiver identities"
                    .into(),
            );
        }
        if !crate::valid_uid(&self.giver, "r")
            || !crate::valid_uid(&self.receiver, "r")
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
        if self.stages.iter().copied().collect::<BTreeSet<_>>().len() != self.stages.len() {
            return Err("Receipt stages must not repeat".into());
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
                    if let Some(terms) = &self.upfront_terms {
                        if proof.authority.participation.as_ref().and_then(|participation| participation.public_history.as_ref()) != Some(&terms.id()?) {
                            return Err("The upfront manifest differs from the original signed participation grant".into());
                        }
                    }
                    if !roles.insert(proof.role) {
                        return Err("Original Organ participation repeats a role".into());
                    }
                    if let Some(party) = &proof
                        .authority
                        .participation
                        .as_ref()
                        .unwrap()
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
        let Some(terms) = self.upfront_terms.as_ref().filter(|terms| terms.permits(self)) else { return BTreeSet::new() };
        if self.party_attribution != PartyAttribution::OriginalOrganAuthority { return BTreeSet::new(); }
        let Ok(commitment) = terms.id() else { return BTreeSet::new() };
        self.original_authorities.iter().filter(|proof| proof.role != ApprovalRole::Origin && proof.authority.participation.as_ref().and_then(|participation| participation.public_history.as_ref()) == Some(&commitment)).map(|proof| proof.role).collect()
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

fn public_audience() -> crate::trust_analysis::contacts::Audience {
    crate::trust_analysis::contacts::Audience::Public
}

fn use_upfront_terms() -> bool { true }

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRole {
    Origin,
    Giver,
    Receiver,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub role: ApprovalRole,
    pub issued_at: i64,
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicPerson {
    pub role: ApprovalRole,
    pub person: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PersonApproval {
    pub scope: CanonicalHash,
    pub role: ApprovalRole,
    pub person: String,
    pub issued_at: i64,
    pub key_id: String,
    pub signature: String,
}

impl PersonApproval {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        Ok(digest(
            "lince.transfer.receipt.person-consent.v1",
            &(
                &self.scope,
                self.role,
                &self.person,
                self.issued_at,
                &self.key_id,
            ),
        )?
        .as_str()
        .as_bytes()
        .to_vec())
    }

    pub fn validate(&self, scope: &Scope, now: i64) -> Result<(), String> {
        signature_shape(&self.key_id, &self.signature)?;
        if self.scope != scope.id()?
            || !scope
                .public_persons
                .iter()
                .any(|target| target.role == self.role && target.person == self.person)
            || self.issued_at < scope.issued_at
            || self.issued_at >= scope.expires_at
            || self.issued_at > now.saturating_add(300)
        {
            return Err(
                "Person consent differs from the reviewed identity, role, scope or term".into(),
            );
        }
        Ok(())
    }
}

impl Approval {
    pub fn signing_bytes(&self, scope: &Scope) -> Result<Vec<u8>, String> {
        Ok(digest(
            "lince.transfer.receipt.approval.v1",
            &(scope.id()?, self.role, self.issued_at, &self.key_id),
        )?
        .as_str()
        .as_bytes()
        .to_vec())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    Need {
        measurement: Measurement,
        requested: DecimalValue,
        need_commitment: CanonicalHash,
    },
    Statement {
        role: Role,
        measurement: Measurement,
        quantity: DecimalValue,
        need: Option<CanonicalHash>,
        withdrawn: bool,
    },
    Settled {
        quantity: RecordedFloat,
    },
    SettlementStatus {
        quantity: RecordedFloat,
        remaining_after: RecordedFloat,
    },
    Cancelled {
        quantity: DecimalValue,
        revision: u64,
    },
    Compensated {
        role: Role,
        settlement: CanonicalHash,
        application: CanonicalHash,
    },
    Dispute {
        role: Role,
        active: bool,
    },
}

impl Content {
    pub fn stage(&self) -> Stage {
        match self {
            Self::Need { .. } => Stage::Need,
            Self::Statement {
                role: Role::Giver, ..
            } => Stage::Giver,
            Self::Statement {
                role: Role::Receiver,
                ..
            } => Stage::Receiver,
            Self::Settled { .. } => Stage::Settled,
            Self::SettlementStatus { .. } => Stage::SettlementStatus,
            Self::Cancelled { .. } => Stage::Cancelled,
            Self::Compensated { .. } => Stage::Compensated,
            Self::Dispute { .. } => Stage::Dispute,
        }
    }

    pub fn role(&self) -> ApprovalRole {
        match self {
            Self::Need { .. }
            | Self::Statement {
                role: Role::Receiver,
                ..
            }
            | Self::Dispute {
                role: Role::Receiver,
                ..
            } => ApprovalRole::Receiver,
            Self::Statement {
                role: Role::Giver, ..
            }
            | Self::Dispute {
                role: Role::Giver, ..
            } => ApprovalRole::Giver,
            Self::Compensated {
                role: Role::Receiver,
                ..
            } => ApprovalRole::Receiver,
            Self::Compensated {
                role: Role::Giver, ..
            } => ApprovalRole::Giver,
            Self::Settled { .. } | Self::SettlementStatus { .. } | Self::Cancelled { .. } => {
                ApprovalRole::Origin
            }
        }
    }

    pub fn measurement(&self) -> Option<&Measurement> {
        match self {
            Self::Need { measurement, .. } | Self::Statement { measurement, .. } => {
                Some(measurement)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub scope: CanonicalHash,
    pub source_commitment: CanonicalHash,
    pub recorded_at: i64,
    pub issued_at: i64,
    pub supersedes: Option<CanonicalHash>,
    pub content: Content,
}

impl Event {
    pub fn id(&self) -> Result<CanonicalHash, String> {
        digest("lince.transfer.receipt.event.v1", self)
    }

    pub fn slot(&self, scope: &Scope) -> Result<CanonicalHash, String> {
        if let Content::Compensated {
            role, settlement, ..
        } = &self.content
        {
            return digest(
                "lince.transfer.receipt.compensation-slot.v1",
                &(
                    &scope.source.origin,
                    &scope.source.occurrence,
                    role,
                    settlement,
                ),
            );
        }
        digest(
            "lince.transfer.receipt.slot.v1",
            &(
                &scope.source.origin,
                &scope.source.occurrence,
                self.content.stage(),
                self.content.role(),
                self.content.measurement(),
            ),
        )
    }

    pub(super) fn validate(&self, scope: &Scope, now: i64) -> Result<(), String> {
        if self.scope != scope.id()?
            || self.recorded_at <= 0
            || (matches!(
                self.content,
                Content::Settled { .. }
                    | Content::SettlementStatus { .. }
                    | Content::Cancelled { .. }
                    | Content::Compensated { .. }
                    | Content::Dispute { .. }
            ) && (self.recorded_at < scope.source.activated_at
                || self.recorded_at > self.issued_at))
            || self.issued_at < scope.issued_at
            || self.issued_at >= scope.expires_at
            || self.issued_at > now.saturating_add(300)
            || !scope.stages.contains(&self.content.stage())
        {
            return Err("Receipt event is outside its approved scope or term".into());
        }
        if let Some(measurement) = self.content.measurement()
            && !scope.measurements.contains(measurement)
        {
            return Err("Receipt event uses an unapproved measurement".into());
        }
        match &self.content {
            Content::Need { requested, .. } if !requested.is_positive() => {
                Err("A public Need binding requires a positive requested amount".into())
            }
            Content::Statement {
                quantity,
                withdrawn,
                ..
            } if quantity.is_negative() || (*withdrawn && !quantity.is_zero()) => {
                Err("Use a nonnegative outcome; withdrawal requires zero".into())
            }
            Content::Settled { quantity } => quantity.value().map(|_| ()),
            Content::SettlementStatus {
                quantity,
                remaining_after,
            } => {
                quantity.value()?;
                remaining_after.value().map(|_| ())
            }
            Content::Cancelled { quantity, revision }
                if !quantity.is_positive() || *revision <= scope.source.agreement_revision =>
            {
                Err("Cancellation requires a positive amount and a later agreed revision".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SignedEvent {
    pub event: Event,
    pub key_id: String,
    pub signature: String,
}

impl SignedEvent {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        Ok(digest(
            "lince.transfer.receipt.signed-event.v1",
            &(&self.event, &self.key_id),
        )?
        .as_str()
        .as_bytes()
        .to_vec())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub person_approvals: Vec<PersonApproval>,
    pub scope: Scope,
    pub approvals: Vec<Approval>,
    pub events: Vec<SignedEvent>,
}

fn signature_shape(key_id: &str, signature: &str) -> Result<(), String> {
    if key_id.is_empty() || key_id.len() > 160 || signature.len() != 88 {
        return Err("Invalid receipt signing key or signature length".into());
    }
    Ok(())
}

impl Publication {
    pub fn parse(bytes: &[u8], now: i64) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Public receipt exceeds its byte budget".into());
        }
        let publication: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        publication.validate(now)?;
        Ok(publication)
    }

    pub fn validate(&self, now: i64) -> Result<(), String> {
        self.validate_approvals(now, true)
    }

    fn validate_approvals(&self, now: i64, complete: bool) -> Result<(), String> {
        self.validate_mode(now, complete, true)
    }

    pub fn validate_fragment(&self, now: i64) -> Result<(), String> {
        self.validate_mode(now, true, false)
    }

    fn validate_mode(&self, now: i64, complete: bool, dependencies: bool) -> Result<(), String> {
        self.scope.validate()?;
        if self.scope.issued_at > now.saturating_add(300)
            || self.approvals.len() > 3
            || (!complete && !self.events.is_empty())
            || self.events.len() > MAX_EVENTS
            || serde_json::to_vec(self)
                .map_err(|error| error.to_string())?
                .len()
                > MAX_BYTES
        {
            return Err("Invalid public receipt approval, time or resource budget".into());
        }
        let mut roles = BTreeSet::new();
        let mut latest_approval = self.scope.issued_at;
        let mut persons = BTreeSet::new();
        if self.person_approvals.len() > self.scope.public_persons.len() {
            return Err("Person consent includes an unrequested identity".into());
        }
        if complete && self.person_approvals.len() != self.scope.public_persons.len() {
            return Err(
                "Public Person identities require their own explicit consent signatures".into(),
            );
        }
        for approval in &self.person_approvals {
            approval.validate(&self.scope, now)?;
            if !persons.insert(approval.role) {
                return Err("Person consent must not repeat a role".into());
            }
            latest_approval = latest_approval.max(approval.issued_at);
        }
        for approval in &self.approvals {
            signature_shape(&approval.key_id, &approval.signature)?;
            if !roles.insert(approval.role)
                || approval.issued_at < self.scope.issued_at
                || approval.issued_at >= self.scope.expires_at
                || approval.issued_at > now.saturating_add(300)
            {
                return Err(
                    "Receipt requires distinct origin, giver and receiver approvals".into(),
                );
            }
            latest_approval = latest_approval.max(approval.issued_at);
        }
        roles.extend(self.scope.upfront_roles());
        if complete && roles.len() != 3 {
            return Err("Receipt requires origin attestation and each party's exact-scope or signed upfront publication consent".into());
        }
        let mut events = BTreeMap::new();
        for signed in &self.events {
            signature_shape(&signed.key_id, &signed.signature)?;
            signed.event.validate(&self.scope, now)?;
            if signed.event.issued_at < latest_approval {
                return Err("Public events must follow approval of their scope".into());
            }
            if events.insert(signed.event.id()?, &signed.event).is_some() {
                return Err("A receipt bundle must not repeat an event".into());
            }
        }
        let mut roots = BTreeSet::new();
        for (id, event) in &events {
            let slot = event.slot(&self.scope)?;
            if let Some(parent) = &event.supersedes {
                if parent == id {
                    return Err("Receipt correction changes its source, role or measurement".into());
                }
                if let Some(previous) = events.get(parent) {
                    if previous.slot(&self.scope)? != slot || previous.issued_at > event.issued_at {
                        return Err(
                            "Receipt correction changes its source, role or measurement".into()
                        );
                    }
                } else if dependencies {
                    return Err("Receipt correction is missing its original".into());
                }
            } else if !roots.insert(slot) && dependencies {
                return Err("Repeated outcome roots cannot multiply contribution history".into());
            }
            if let Content::Statement {
                measurement,
                need: Some(need),
                ..
            } = &event.content
            {
                if let Some(binding) = events.get(need) {
                    if !matches!(&binding.content, Content::Need { measurement: bound, .. } if bound == measurement)
                        || binding.issued_at > event.issued_at
                    {
                        return Err(
                            "Receipt confirmation does not match its receiver-authored Need".into(),
                        );
                    }
                } else if dependencies {
                    return Err("Receipt statement is missing its Need proof".into());
                }
            }
            let mut cursor = *event;
            let mut visited = BTreeSet::new();
            while let Some(parent) = &cursor.supersedes {
                if !visited.insert(parent) {
                    return Err("Receipt correction history contains a loop".into());
                }
                if let Some(previous) = events.get(parent) {
                    cursor = previous;
                } else if dependencies {
                    return Err("Receipt correction history is incomplete".into());
                } else {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub organ: String,
    pub roster_version: i64,
    pub root_key: String,
    pub signing_key: String,
    pub key_id: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub successions: Vec<RootSuccession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participation: Option<Participation>,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Participation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_history: Option<CanonicalHash>,
    pub organ: String,
    pub origin: String,
    pub agreement_revision: u64,
    pub agreement_commitment: CanonicalHash,
    pub party_commitment: Option<CanonicalHash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParticipationRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_history_terms: Option<super::public_history::Terms>,
    pub transfer: String,
    pub party: Option<String>,
    pub terms: Participation,
}

impl ParticipationRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.terms.validate()?;
        if self.public_history_terms.as_ref().map(super::public_history::Terms::id).transpose()? != self.terms.public_history {
            return Err("Review the exact public-history manifest before granting its future publication rights".into());
        }
        if !crate::valid_uid(&self.transfer, "r")
            || self
                .party
                .as_ref()
                .is_some_and(|uid| !crate::valid_uid(uid, "r"))
        {
            return Err(
                "Private participation request requires canonical Record references".into(),
            );
        }
        let commitment = self
            .party
            .as_ref()
            .map(|party| {
                Participation::party_commitment(
                    &self.terms.origin,
                    &self.transfer,
                    self.terms.agreement_revision,
                    party,
                )
            })
            .transpose()?;
        if commitment != self.terms.party_commitment {
            return Err("Private participation request does not open its party commitment".into());
        }
        Ok(())
    }
}

impl Participation {
    pub fn party_commitment(
        origin: &str,
        transfer: &str,
        revision: u64,
        person: &str,
    ) -> Result<CanonicalHash, String> {
        if !crate::valid_uid(origin, "r")
            || !crate::valid_uid(transfer, "r")
            || !crate::valid_uid(person, "r")
            || revision == 0
        {
            return Err(
                "Original party opening requires canonical source and Person references".into(),
            );
        }
        digest(
            "lince.transfer.participation.party.v1",
            &(origin, transfer, revision, person),
        )
    }

    pub fn validate(&self) -> Result<(), String> {
        if !crate::valid_uid(&self.organ, "r")
            || !crate::valid_uid(&self.origin, "r")
            || self.agreement_revision == 0
            || (self.party_commitment.is_none() && self.organ != self.origin)
        {
            return Err(
                "Original participation requires exact Organ and agreement references".into(),
            );
        }
        Ok(())
    }

    pub fn from_revision(
        revision: &super::TransferRevisionEvidence,
        fact_hash: &str,
        person: Option<&str>,
    ) -> Result<Self, String> {
        let origin = revision
            .terms
            .transfer
            .origin_organ_uid
            .as_ref()
            .ok_or("Original signed source Organ is unavailable")?;
        let (organ, party_commitment) = if let Some(person) = person {
            let party = revision
                .terms
                .parties
                .iter()
                .find(|party| party.person_uid == person)
                .ok_or("Person is absent from the original signed agreement")?;
            (
                party
                    .organ_uid
                    .clone()
                    .ok_or("Original signed party Organ is unavailable")?,
                Some(Self::party_commitment(
                    origin,
                    &revision.terms.transfer.uid,
                    revision.terms.revision,
                    person,
                )?),
            )
        } else {
            (origin.clone(), None)
        };
        let value = Self {
            public_history: revision.terms.transfer.public_history.as_ref().map(super::public_history::Terms::id).transpose()?,
            organ,
            origin: origin.clone(),
            agreement_revision: revision.terms.revision,
            agreement_commitment: Source::fact_commitment(fact_hash)?,
            party_commitment,
        };
        value.validate()?;
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OriginalAuthority {
    pub role: ApprovalRole,
    pub authority: Authority,
}

impl OriginalAuthority {
    pub fn validate(&self, scope: &Scope) -> Result<(), String> {
        let authority = &self.authority;
        authority.validate(scope.source.activated_at)?;
        let terms = authority
            .participation
            .as_ref()
            .ok_or("Original participation proof has no agreement scope")?;
        if authority.organ != scope.organ(self.role)
            || terms.origin != scope.source.origin
            || terms.agreement_revision != scope.source.agreement_revision
            || terms.agreement_commitment != scope.source.agreement_commitment
            || terms.party_commitment.is_none() != (self.role == ApprovalRole::Origin)
            || authority.issued_at > scope.source.activated_at
            || authority.expires_at <= scope.source.activated_at
        {
            return Err("Original participation proof differs from the activated agreement".into());
        }
        Ok(())
    }
}

impl Authority {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let base = digest(
            "lince.transfer.receipt.authority.v1",
            &(
                &self.organ,
                self.roster_version,
                &self.root_key,
                &self.signing_key,
                &self.key_id,
                self.issued_at,
                self.expires_at,
                &self.successions,
            ),
        )?;
        let hash = if let Some(terms) = &self.participation {
            digest("lince.transfer.participation.authority.v1", &(base, terms))?
        } else {
            base
        };
        Ok(hash.as_str().as_bytes().to_vec())
    }

    pub fn validate(&self, now: i64) -> Result<(), String> {
        signature_shape(&self.key_id, &self.signature)?;
        if !crate::valid_uid(&self.organ, "r")
            || self.roster_version < 1
            || self.root_key.len() != 44
            || self.signing_key.len() != 44
            || self.issued_at <= 0
            || self.issued_at > now.saturating_add(300)
            || self.expires_at <= self.issued_at
            || self.expires_at.saturating_sub(self.issued_at) > MAX_AUTHORITY_SECONDS
            || self.successions.len() > 8
        {
            return Err("Invalid public receipt signing authority".into());
        }
        for change in &self.successions {
            if change.old_key.len() != 44
                || change.new_key.len() != 44
                || change.signature.len() != 88
                || change.created_at.len() > 64
            {
                return Err("Invalid public receipt identity succession".into());
            }
        }
        if let Some(terms) = &self.participation {
            terms.validate()?;
            if terms.organ != self.organ {
                return Err("Participation approval belongs to a different Organ".into());
            }
        }
        Ok(())
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
        self.validate_approvals(now, true)
    }

    fn validate_approvals(&self, now: i64, complete: bool) -> Result<(), String> {
        self.validate_mode(now, complete, true)
    }

    pub fn validate_fragment(&self, now: i64) -> Result<(), String> {
        self.validate_mode(now, true, false)
    }

    fn validate_mode(&self, now: i64, complete: bool, dependencies: bool) -> Result<(), String> {
        self.publication
            .validate_mode(now, complete, dependencies)?;
        if self.authorities.len() + self.publication.scope.original_authorities.len()
            > MAX_AUTHORITIES
            || serde_json::to_vec(self)
                .map_err(|error| error.to_string())?
                .len()
                > MAX_BUNDLE_BYTES
        {
            return Err("Public receipt authority or byte budget exceeded".into());
        }
        let mut bindings = BTreeMap::new();
        let scope = &self.publication.scope;
        for authority in &self.authorities {
            authority.validate(now)?;
            if authority.participation.is_some() {
                return Err(
                    "Participation approval cannot authorize receipt publication signatures".into(),
                );
            }
            if ![&scope.source.origin, &scope.giver, &scope.receiver].contains(&&authority.organ) {
                return Err("Receipt includes an unrelated signing authority".into());
            }
            if bindings
                .insert(
                    (&authority.organ, &authority.key_id),
                    &authority.signing_key,
                )
                .is_some_and(|key| key != &authority.signing_key)
            {
                return Err("Receipt signing key identifier has conflicting bindings".into());
            }
        }
        Ok(())
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
        Bundle {
            publication: Publication {
                person_approvals: self.person_approvals.clone(),
                scope: self.scope.clone(),
                approvals: self.approvals.clone(),
                events: Vec::new(),
            },
            authorities: self.authorities.clone(),
        }
    }

    pub fn validate(&self, now: i64) -> Result<(), String> {
        self.as_bundle().validate_approvals(now, false)
    }

    pub fn is_complete(&self) -> bool {
        self.approvals.iter().map(|approval| approval.role).chain(self.scope.upfront_roles()).collect::<BTreeSet<_>>().len() == 3 && self.person_approvals.len() == self.scope.public_persons.len()
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
        if self == incoming {
            self.validate(now)?;
            return Ok(self.clone());
        }
        fn merge_review(current: &Review, incoming: &Review) -> Result<Review, String> {
            if current.scope != incoming.scope {
                return Err("Changed receipt terms require a new scope and new approvals".into());
            }
            let mut merged = current.clone();
            for approval in &incoming.person_approvals {
                if let Some(previous) = merged
                    .person_approvals
                    .iter()
                    .find(|item| item.role == approval.role)
                {
                    if previous != approval {
                        return Err(
                            "Conflicting personal consent requires a separate review".into()
                        );
                    }
                } else {
                    merged.person_approvals.push(approval.clone());
                }
            }
            merged
                .person_approvals
                .sort_by_key(|approval| approval.role);
            for approval in &incoming.approvals {
                if let Some(previous) = merged
                    .approvals
                    .iter()
                    .find(|item| item.role == approval.role)
                {
                    if previous != approval {
                        return Err("Conflicting approvals must be reviewed separately".into());
                    }
                } else {
                    merged.approvals.push(approval.clone());
                }
            }
            merged.approvals.sort_by_key(|approval| approval.role);
            for authority in &incoming.authorities {
                if !merged.authorities.contains(authority) {
                    merged.authorities.push(authority.clone());
                }
            }
            Ok(merged)
        }
        let merged = match (self, incoming) {
            (Self::Authority(current), Self::Authority(incoming)) if current == incoming => {
                self.clone()
            }
            (Self::Review(current), Self::Review(incoming)) => {
                Self::Review(merge_review(current, incoming)?)
            }
            (Self::Publication(current), Self::Publication(incoming)) => {
                let review = merge_review(
                    &Review {
                        person_approvals: current.publication.person_approvals.clone(),
                        scope: current.publication.scope.clone(),
                        approvals: current.publication.approvals.clone(),
                        authorities: current.authorities.clone(),
                    },
                    &Review {
                        person_approvals: incoming.publication.person_approvals.clone(),
                        scope: incoming.publication.scope.clone(),
                        approvals: incoming.publication.approvals.clone(),
                        authorities: incoming.authorities.clone(),
                    },
                )?;
                let mut bundle = review.as_bundle();
                bundle.publication.events = current.publication.events.clone();
                for signed in &incoming.publication.events {
                    if let Some(previous) = bundle
                        .publication
                        .events
                        .iter()
                        .find(|item| item.event == signed.event)
                    {
                        if previous != signed {
                            return Err("An event has conflicting signature envelopes".into());
                        }
                    } else {
                        bundle.publication.events.push(signed.clone());
                    }
                }
                Self::Publication(bundle)
            }
            _ => return Err("Receipt document identity or kind changed".into()),
        };
        merged.validate(now)?;
        Ok(merged)
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Authority(_) => "authority",
            Self::Review(_) => "review",
            Self::Publication(_) => "publication",
            Self::ProofFragment(_) => "publication",
        }
    }

    pub fn hash(&self) -> Result<CanonicalHash, String> {
        digest("lince.transfer.receipt.document.v1", self)
    }

    pub fn scope(&self) -> Option<&Scope> {
        match self {
            Self::Review(review) => Some(&review.scope),
            Self::Publication(bundle) => Some(&bundle.publication.scope),
            Self::ProofFragment(bundle) => Some(&bundle.publication.scope),
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
            Self::Publication(bundle) => bundle.publication.scope.expires_at,
            Self::ProofFragment(bundle) => bundle.publication.scope.expires_at,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventSource {
    Snapshot {
        record: String,
    },
    Outcome {
        record: String,
    },
    Settlement {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
    SettlementStatus {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
    Cancellation {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
    Compensation {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
    Dispute {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventRequest {
    pub record: String,
    pub expected_version: u64,
    pub authority_record: String,
    pub source: EventSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    ApprovePerson {
        record: String,
        expected_version: u64,
        approval: PersonApproval,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        private_transfer: Option<String>,
    },
    History {
        origin: String,
        occurrence: CanonicalHash,
        #[serde(default)]
        before: Option<String>,
    },
    InspectVersion {
        record: String,
        version: u64,
    },
    ExportVersion {
        record: String,
        version: u64,
    },
    Versions {
        record: String,
        #[serde(default)]
        before: Option<String>,
    },
    Proof {
        record: String,
        version: u64,
        #[serde(default)]
        before: Option<String>,
    },
    Delivery {
        request: super::receipt_delivery::Command,
    },
    RecordSource {
        request: super::receipt_source::SourceRequest,
    },
    InspectSource {
        record: String,
    },
    SourceOptions {
        receipt: String,
    },
    IssueAuthority {
        cell: String,
    },
    PrepareParticipation {
        transfer: String,
        expected_revision: u64,
        party: Option<String>,
    },
    AuthorizeParticipation {
        cell: String,
        request: String,
    },
    Prepare {
        scope: ScopeRequest,
    },
    Import {
        document: String,
    },
    PreviewEvent {
        request: EventRequest,
    },
    PublishEvent {
        request: EventRequest,
        reviewed_at: i64,
        expected_projection: CanonicalHash,
    },
    Approve {
        record: String,
        expected_version: u64,
        role: ApprovalRole,
        authority_record: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        private_transfer: Option<String>,
    },
    Inspect {
        record: String,
    },
    Export {
        record: String,
    },
    List,
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::ApprovePerson { .. } => "record:update",
            Self::Delivery { request } => request.permission(),
            Self::RecordSource { .. } => "transfer:update",
            Self::InspectSource { .. } | Self::SourceOptions { .. } => "record:read",
            Self::IssueAuthority { .. } | Self::AuthorizeParticipation { .. } => "organ:update",
            Self::Prepare { .. } | Self::PrepareParticipation { .. } => "transfer:read",
            Self::PreviewEvent { request }
                if matches!(request.source, EventSource::Snapshot { .. }) =>
            {
                "record:read"
            }
            Self::PreviewEvent { .. } => "transfer:read",
            Self::Import { .. } => "record:create",
            Self::Approve { .. } | Self::PublishEvent { .. } => "transfer:update",
            Self::Inspect { .. }
            | Self::Export { .. }
            | Self::List
            | Self::History { .. }
            | Self::InspectVersion { .. }
            | Self::ExportVersion { .. }
            | Self::Versions { .. }
            | Self::Proof { .. } => "record:read",
        }
    }

    pub fn is_mutation(&self) -> bool {
        if let Self::Delivery { request } = self {
            return request.is_mutation();
        }
        !matches!(
            self,
            Self::Inspect { .. }
                | Self::InspectVersion { .. }
                | Self::ExportVersion { .. }
                | Self::Versions { .. }
                | Self::Proof { .. }
                | Self::History { .. }
                | Self::Export { .. }
                | Self::List
                | Self::PrepareParticipation { .. }
                | Self::PreviewEvent { .. }
                | Self::InspectSource { .. }
                | Self::SourceOptions { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(label: &str) -> CanonicalHash {
        digest("lince.transfer.receipt.test.v1", &label).unwrap()
    }

    fn measurement() -> Measurement {
        Measurement {
            meaning: MeaningReference {
                vocabulary_uid: crate::new_uid("g"),
                concept_uid: crate::new_uid("c"),
                revision: 1,
                definition_hash: "a".repeat(64),
            },
            unit: None,
            object_commitment: None,
        }
    }

    fn publication() -> Publication {
        let scope = Scope {
        audience: crate::trust_analysis::contacts::Audience::Public,
        upfront_terms: None,
            public_persons: Vec::new(),
            party_attribution: PartyAttribution::LaterOrganEndorsement,
            original_authorities: Vec::new(),
            source: Source {
                origin: crate::new_uid("r"),
                occurrence: hash("occurrence"),
                activated_at: 900,
                agreement_revision: 1,
                agreement_commitment: hash("agreement"),
                activation_commitment: hash("activation"),
            },
            giver: crate::new_uid("r"),
            receiver: crate::new_uid("r"),
            measurements: vec![measurement()],
            stages: vec![
                Stage::Need,
                Stage::Giver,
                Stage::Receiver,
                Stage::Settled,
                Stage::Dispute,
            ],
            agreed_quantity: Some(RecordedFloat::new(10.0).unwrap()),
            canonical_unit: None,
            issued_at: 1000,
            expires_at: 2000,
        };
        Publication {
            person_approvals: Vec::new(),
            scope,
            approvals: [
                ApprovalRole::Origin,
                ApprovalRole::Giver,
                ApprovalRole::Receiver,
            ]
            .into_iter()
            .map(|role| Approval {
                role,
                issued_at: 1000,
                key_id: "test".into(),
                signature: "a".repeat(88),
            })
            .collect(),
            events: Vec::new(),
        }
    }

    fn statement(publication: &Publication, amount: &str, time: i64) -> SignedEvent {
        SignedEvent {
            event: Event {
                scope: publication.scope.id().unwrap(),
                source_commitment: hash(&format!("statement:{amount}:{time}")),
                recorded_at: 950,
                issued_at: time,
                supersedes: None,
                content: Content::Statement {
                    role: Role::Receiver,
                    measurement: publication.scope.measurements[0].clone(),
                    quantity: DecimalValue::parse_inferred(amount).unwrap(),
                    need: None,
                    withdrawn: false,
                },
            },
            key_id: "test".into(),
            signature: "a".repeat(88),
        }
    }

    #[test]
    fn lifecycle_requires_fresh_stages_and_keeps_compensated_slices_in_distinct_slots() {
        let mut publication = publication();
        let mut event = statement(&publication, "4", 1500);
        event.event.recorded_at = 1200;
        event.event.content = Content::SettlementStatus {
            quantity: RecordedFloat::new(4.0).unwrap(),
            remaining_after: RecordedFloat::new(6.0).unwrap(),
        };
        assert!(event.event.validate(&publication.scope, 1600).is_err());
        let earlier_scope = publication.scope.id().unwrap();
        publication.scope.stages.extend([
            Stage::SettlementStatus,
            Stage::Cancelled,
            Stage::Compensated,
        ]);
        let scope = &publication.scope;
        assert_ne!(earlier_scope, scope.id().unwrap());
        event.event.scope = scope.id().unwrap();
        event.event.validate(scope, 1600).unwrap();
        if let Content::SettlementStatus {
            remaining_after, ..
        } = &mut event.event.content
        {
            remaining_after.ieee754_binary64 = "7ff0000000000000".into();
        }
        assert!(event.event.validate(scope, 1600).is_err());
        event.event.content = Content::Cancelled {
            quantity: DecimalValue::parse_inferred("6").unwrap(),
            revision: 2,
        };
        event.event.validate(scope, 1600).unwrap();
        if let Content::Cancelled { revision, .. } = &mut event.event.content {
            *revision = 1;
        }
        assert!(event.event.validate(scope, 1600).is_err());
        event.event.content = Content::Compensated {
            role: Role::Giver,
            settlement: hash("first-slice"),
            application: hash("application"),
        };
        let first = event.event.slot(scope).unwrap();
        if let Content::Compensated { settlement, .. } = &mut event.event.content {
            *settlement = hash("second-slice");
        }
        assert_ne!(first, event.event.slot(scope).unwrap());
        event.event.recorded_at = 899;
        assert!(event.event.validate(scope, 1600).is_err());
    }

    #[test]
    fn naming_a_person_requires_separate_exact_scope_consent_before_publication() {
        let mut publication = publication();
        let original = publication.scope.id().unwrap();
        let person = crate::new_uid("r");
        publication.scope.public_persons.push(PublicPerson {
            role: ApprovalRole::Receiver,
            person: person.clone(),
        });
        assert_ne!(original, publication.scope.id().unwrap());
        let review = Review {
            scope: publication.scope.clone(),
            approvals: publication.approvals.clone(),
            person_approvals: Vec::new(),
            authorities: Vec::new(),
        };
        review.validate(1600).unwrap();
        assert!(!review.is_complete());
        assert!(publication.validate(1600).is_err());
        let approval = PersonApproval {
            scope: publication.scope.id().unwrap(),
            role: ApprovalRole::Receiver,
            person,
            issued_at: 1500,
            key_id: "person-key".into(),
            signature: "a".repeat(88),
        };
        publication.person_approvals.push(approval.clone());
        publication.validate(1600).unwrap();
        assert!(crate::transfer::receipt_history::project(&publication).is_ok());
        assert!(crate::transfer::receipt_fragment::references(&publication).is_ok());
        for change in 0..4 {
            let mut changed = approval.clone();
            match change {
                0 => changed.scope = original.clone(),
                1 => changed.role = ApprovalRole::Giver,
                2 => changed.person = crate::new_uid("r"),
                _ => changed.issued_at = publication.scope.expires_at,
            }
            assert!(changed.validate(&publication.scope, 1600).is_err());
        }
        let mut early = statement(&publication, "12", 1010);
        publication.events.push(early.clone());
        assert!(publication.validate(1600).is_err());
        early.event.issued_at = 1510;
        publication.events = vec![early];
        publication.validate(1600).unwrap();
    }

    #[test]
    fn personal_consents_cannot_repeat_roles_or_replace_an_existing_signature() {
        let mut scope = publication().scope;
        scope.public_persons = vec![PublicPerson {
            role: ApprovalRole::Receiver,
            person: crate::new_uid("r"),
        }];
        let approval = PersonApproval {
            scope: scope.id().unwrap(),
            role: ApprovalRole::Receiver,
            person: scope.public_persons[0].person.clone(),
            issued_at: 1010,
            key_id: "personal".into(),
            signature: "a".repeat(88),
        };
        let review = Review {
            scope: scope.clone(),
            approvals: Vec::new(),
            person_approvals: vec![approval.clone()],
            authorities: Vec::new(),
        };
        let original = Document::Review(review.clone());
        let mut duplicate = review.clone();
        duplicate.person_approvals.push(approval.clone());
        assert!(duplicate.validate(1100).is_err());
        let mut changed = review;
        changed.person_approvals[0].issued_at += 1;
        assert!(original.merge(&Document::Review(changed), 1100).is_err());
        scope.public_persons.push(scope.public_persons[0].clone());
        assert!(scope.validate().is_err());
        scope.public_persons.truncate(1);
        scope.public_persons[0].role = ApprovalRole::Origin;
        assert!(scope.validate().is_err());
    }

    #[test]
    fn reported_source_times_do_not_assume_the_origins_clock() {
        let mut publication = publication();
        let mut reported = statement(&publication, "12", 1010);
        reported.event.recorded_at = publication.scope.source.activated_at - 30;
        publication.events.push(reported);
        publication.validate(1100).unwrap();
        publication.events[0].event.recorded_at = 1200;
        publication.validate(1100).unwrap();
        publication.events[0].event.recorded_at = 0;
        assert!(publication.validate(1100).is_err());
    }

    #[test]
    fn private_participation_requests_open_the_exact_source_and_party() {
        let source = crate::new_uid("r");
        let transfer = crate::new_uid("r");
        let person = crate::new_uid("r");
        let terms = Participation {
            public_history: None,
            organ: source.clone(),
            origin: source.clone(),
            agreement_revision: 1,
            agreement_commitment: hash("agreement"),
            party_commitment: Some(
                digest(
                    "lince.transfer.participation.party.v1",
                    &(&source, &transfer, 1u64, &person),
                )
                .unwrap(),
            ),
        };
        let request = ParticipationRequest {
            public_history_terms: None,
            transfer,
            party: Some(person),
            terms,
        };
        request.validate().unwrap();
        let mut changed = request.clone();
        changed.party = Some(crate::new_uid("r"));
        assert!(changed.validate().is_err());
        let mut changed = request.clone();
        changed.transfer = crate::new_uid("r");
        assert!(changed.validate().is_err());
        let mut changed = request;
        changed.terms.agreement_revision += 1;
        assert!(changed.validate().is_err());
    }

    #[test]
    fn original_authority_requires_complete_roles_and_cannot_authorize_publication_signatures() {
        let mut scope = publication().scope;
        scope.party_attribution = PartyAttribution::OriginalOrganAuthority;
        assert!(scope.validate().is_err());
        for role in [
            ApprovalRole::Origin,
            ApprovalRole::Giver,
            ApprovalRole::Receiver,
        ] {
            let organ = scope.organ(role).to_owned();
            let authority = Authority {
                organ: organ.clone(),
                roster_version: 1,
                root_key: "A".repeat(44),
                signing_key: "B".repeat(44),
                key_id: format!("ed25519:cell:{}:v1", crate::new_uid("r")),
                issued_at: 800,
                expires_at: 1800,
                successions: Vec::new(),
                signature: "A".repeat(88),
                participation: Some(Participation {
                    public_history: None,
                    organ,
                    origin: scope.source.origin.clone(),
                    agreement_revision: scope.source.agreement_revision,
                    agreement_commitment: scope.source.agreement_commitment.clone(),
                    party_commitment: if role == ApprovalRole::Origin {
                        None
                    } else {
                        Some(hash(&format!("{role:?}")))
                    },
                }),
            };
            let mut generic = authority.clone();
            generic.participation = None;
            assert_ne!(
                authority.signing_bytes().unwrap(),
                generic.signing_bytes().unwrap()
            );
            scope
                .original_authorities
                .push(OriginalAuthority { role, authority });
        }
        scope.validate().unwrap();
        let mut late = scope.clone();
        late.original_authorities[0].authority.issued_at = scope.source.activated_at + 1;
        assert!(late.validate().is_err());
        let mut incomplete = scope.clone();
        incomplete.original_authorities.pop();
        assert!(incomplete.validate().is_err());
        let mut wrong = scope.clone();
        wrong.original_authorities[1]
            .authority
            .participation
            .as_mut()
            .unwrap()
            .agreement_revision += 1;
        assert!(wrong.validate().is_err());
        let mut later = scope.clone();
        later.party_attribution = PartyAttribution::LaterOrganEndorsement;
        assert!(later.validate().is_err());
        let approval = scope.original_authorities[0].authority.clone();
        let review = Review {
            person_approvals: Vec::new(),
            scope,
            approvals: vec![],
            authorities: vec![approval],
        };
        assert!(review.validate(1300).is_err());
    }

    #[test]
    fn recorded_float_preserves_the_actual_bits_without_claiming_decimal_precision() {
        for value in [0.0, 0.1, 10.0, f64::MAX, f64::MIN_POSITIVE] {
            let quantity = RecordedFloat::new(value).unwrap();
            assert_eq!(quantity.value().unwrap().to_bits(), value.to_bits());
            assert!(
                !serde_json::to_string(&quantity)
                    .unwrap()
                    .contains("decimal")
            );
        }
        for value in [f64::NAN, f64::INFINITY, -0.0, -1.0] {
            assert!(RecordedFloat::new(value).is_err());
        }
        for bits in [
            "0",
            "3FB999999999999A",
            "7ff0000000000000",
            "8000000000000000",
        ] {
            assert!(
                RecordedFloat {
                    ieee754_binary64: bits.into()
                }
                .value()
                .is_err()
            );
        }
    }

    #[test]
    fn corrections_preserve_branches_but_reject_duplicate_roots_and_cross_role_rewrites() {
        let mut receipt = publication();
        let original = statement(&receipt, "12", 1100);
        let parent = original.event.id().unwrap();
        let mut correction = statement(&receipt, "11", 1200);
        correction.event.supersedes = Some(parent.clone());
        let mut competing = statement(&receipt, "13", 1200);
        competing.event.supersedes = Some(parent);
        receipt.events = vec![competing.clone(), original.clone(), correction];
        receipt.validate(1300).unwrap();
        receipt.events.push(statement(&receipt, "14", 1250));
        assert!(receipt.validate(1300).is_err());
        receipt.events.pop();
        if let Content::Statement { role, .. } = &mut receipt.events[0].event.content {
            *role = Role::Giver;
        }
        assert!(receipt.validate(1300).is_err());
        receipt.events = vec![competing];
        assert!(receipt.validate(1300).is_err());
    }

    #[test]
    fn need_proof_is_exact_and_received_quantity_may_exceed_requested() {
        let mut receipt = publication();
        let mut need = statement(&receipt, "10", 1050);
        need.event.content = Content::Need {
            measurement: receipt.scope.measurements[0].clone(),
            requested: DecimalValue::parse_inferred("10").unwrap(),
            need_commitment: hash("private-need"),
        };
        let mut received = statement(&receipt, "12.000000000000000001", 1100);
        if let Content::Statement { need: binding, .. } = &mut received.event.content {
            *binding = Some(need.event.id().unwrap());
        }
        receipt.events = vec![received, need];
        receipt.validate(1300).unwrap();
        receipt.events.pop();
        assert!(receipt.validate(1300).is_err());
    }

    #[test]
    fn projection_cannot_expand_scope_or_reinterpret_withdrawal() {
        let mut receipt = publication();
        receipt.scope.stages = vec![Stage::Giver];
        receipt.events = vec![statement(&receipt, "12", 1100)];
        assert!(receipt.validate(1300).is_err());
        receipt.scope.stages.push(Stage::Receiver);
        receipt.events[0].event.scope = receipt.scope.id().unwrap();
        receipt.validate(1300).unwrap();
        if let Content::Statement { withdrawn, .. } = &mut receipt.events[0].event.content {
            *withdrawn = true;
        }
        assert!(receipt.validate(1300).is_err());
        if let Content::Statement { quantity, .. } = &mut receipt.events[0].event.content {
            *quantity = DecimalValue::parse_inferred("0").unwrap();
        }
        receipt.validate(1300).unwrap();
    }

    #[test]
    fn parsing_rejects_private_extra_fields_oversize_and_unapproved_time() {
        let mut receipt = publication();
        let mut value = serde_json::to_value(&receipt).unwrap();
        value["scope"]["private_need"] = serde_json::json!(crate::new_uid("r"));
        assert!(Publication::parse(&serde_json::to_vec(&value).unwrap(), 1300).is_err());
        assert!(Publication::parse(&vec![b' '; MAX_BYTES + 1], 1300).is_err());
        receipt.approvals[2].issued_at = 1200;
        receipt.events = vec![statement(&receipt, "12", 1100)];
        assert!(receipt.validate(1300).is_err());
        receipt.events[0].event.issued_at = 1250;
        receipt.validate(1300).unwrap();
        receipt.events[0].event.issued_at = 2000;
        assert!(receipt.validate(2000).is_err());
    }

    #[test]
    fn replica_and_scope_changes_keep_the_same_source_slot() {
        let receipt = publication();
        let original = statement(&receipt, "12", 1100);
        let mut changed = receipt.clone();
        changed.scope.expires_at += 1000;
        let changed_event = statement(&changed, "12", 1100);
        assert_eq!(
            original.event.slot(&receipt.scope).unwrap(),
            changed_event.event.slot(&changed.scope).unwrap()
        );
        assert_ne!(
            original.event.id().unwrap(),
            changed_event.event.id().unwrap()
        );
        let origin = crate::new_uid("r");
        let occurrence = crate::new_uid("toc");
        let source = Source::occurrence_id(&origin, &occurrence).unwrap();
        assert_eq!(source, Source::occurrence_id(&origin, &occurrence).unwrap());
        assert_ne!(
            source,
            Source::occurrence_id(&crate::new_uid("r"), &occurrence).unwrap()
        );
    }

    #[test]
    fn review_approvals_merge_without_authorizing_an_incomplete_publication() {
        let publication = publication();
        let review = Review {
            person_approvals: Vec::new(),
            scope: publication.scope.clone(),
            approvals: Vec::new(),
            authorities: Vec::new(),
        };
        review.validate(1300).unwrap();
        assert!(review.as_bundle().validate(1300).is_err());
        let first = Document::Review(review.clone());
        let mut approved = review;
        approved.approvals.push(publication.approvals[0].clone());
        let second = Document::Review(approved.clone());
        let merged = first.merge(&second, 1300).unwrap();
        assert_eq!(merged, second);
        assert_eq!(merged.identity().unwrap(), first.identity().unwrap());
        assert_ne!(merged.hash().unwrap(), first.hash().unwrap());
        assert!(approved.as_bundle().validate(1300).is_err());
        approved.approvals.push(approved.approvals[0].clone());
        assert!(approved.validate(1300).is_err());
        let mut changed = publication.scope;
        changed.expires_at += 1;
        assert!(
            first
                .merge(
                    &Document::Review(Review {
                        person_approvals: Vec::new(),
                        scope: changed,
                        approvals: Vec::new(),
                        authorities: Vec::new()
                    }),
                    1300
                )
                .is_err()
        );
        let mut value = serde_json::to_value(&first).unwrap();
        value["value"]["private_address"] = serde_json::json!("secret");
        assert!(Document::parse(&serde_json::to_vec(&value).unwrap(), 1300).is_err());
        assert!(Document::parse(&vec![b' '; MAX_BUNDLE_BYTES + 65], 1300).is_err());
    }

    #[test]
    fn reimporting_an_identical_document_keeps_its_original_approval_order_and_hash() {
        let mut publication = publication();
        publication.approvals.reverse();
        let review = Document::Review(Review {
            person_approvals: Vec::new(),
            scope: publication.scope.clone(),
            approvals: publication.approvals.clone(),
            authorities: Vec::new(),
        });
        let bundle = Document::Publication(Bundle {
            publication,
            authorities: Vec::new(),
        });
        for document in [review, bundle] {
            let original = serde_json::to_vec(&document).unwrap();
            let imported = Document::parse(&original, 1300).unwrap();
            let merged = imported.merge(&imported, 1300).unwrap();
            assert_eq!(merged.hash().unwrap(), imported.hash().unwrap());
            assert_eq!(serde_json::to_vec(&merged).unwrap(), original);
        }
    }
}
