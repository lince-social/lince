//! Publication approvals and consent to disclose personal identities.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Scope, digest, signature_shape};
use crate::karma::CanonicalHash;

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
