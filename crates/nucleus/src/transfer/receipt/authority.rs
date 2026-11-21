//! Signing authorities and proof of original participation in a Transfer.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ApprovalRole, MAX_AUTHORITIES, MAX_AUTHORITY_SECONDS, Scope, Source, digest, signature_shape,
};
use crate::{
    karma::CanonicalHash,
    social::RootSuccession,
    transfer::{TransferRevisionEvidence, public_history::Terms},
    valid_uid,
};

/// Sort the selected approval Record IDs for consistent original-participation requests.
///
/// # Errors
///
/// The selection exceeds the authority limit, contains an invalid ID, or repeats an ID.
pub fn original_authority_records(values: &[String]) -> Result<Vec<String>, String> {
    const INVALID: &str = "Select at most 64 distinct original participation approval Records";
    if values.len() > MAX_AUTHORITIES || values.iter().any(|uid| !valid_uid(uid, "r")) {
        return Err(INVALID.into());
    }
    let mut records = values.to_vec();
    records.sort_unstable();
    if records.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(INVALID.into());
    }
    Ok(records)
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
    pub public_history_terms: Option<Terms>,
    pub transfer: String,
    pub party: Option<String>,
    pub terms: Participation,
}

impl ParticipationRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.terms.validate()?;
        if self
            .public_history_terms
            .as_ref()
            .map(Terms::id)
            .transpose()?
            != self.terms.public_history
        {
            return Err("Review the exact public-history manifest before granting its future publication rights".into());
        }
        if !valid_uid(&self.transfer, "r")
            || self.party.as_ref().is_some_and(|uid| !valid_uid(uid, "r"))
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
        if !valid_uid(origin, "r")
            || !valid_uid(transfer, "r")
            || !valid_uid(person, "r")
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
        if !valid_uid(&self.organ, "r")
            || !valid_uid(&self.origin, "r")
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
        revision: &TransferRevisionEvidence,
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
            public_history: revision
                .terms
                .transfer
                .public_history
                .as_ref()
                .map(Terms::id)
                .transpose()?,
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
        if !valid_uid(&self.organ, "r")
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
