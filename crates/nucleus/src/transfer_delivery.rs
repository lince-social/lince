use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::Fact;

pub const TRANSFER_ENVELOPE_VERSION: u16 = 1;
const ENVELOPE_DOMAIN: &str = "lince.transfer-envelope.v1";
const APPLICATION_ATTESTATION_DOMAIN: &str = "lince.transfer-application-attestation.v1";
const APPLICATION_HANDOFF_DOMAIN: &str = "lince.transfer-application-handoff.v1";
const ORGAN_REQUEST_DOMAIN: &str = "lince.organ-transfer-request.v1";
const DELIVERY_POLICY_EVENT_DOMAIN: &str = "lince.transfer-delivery-policy-event.v1";
const PACKAGE_RECEIPT_DOMAIN: &str = "lince.transfer-package-receipt.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDeliveryMode {
    Hosted,
    Replicated,
}

impl TransferDeliveryMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hosted => "hosted",
            Self::Replicated => "replicated",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "hosted" => Some(Self::Hosted),
            "replicated" => Some(Self::Replicated),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDisclosureEntry {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferActorKey {
    pub actor_person_uid: String,
    pub key_id: String,
    pub public_key_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferActionIntentEvidence {
    pub intent_uid: String,
    pub actor_person_uid: String,
    pub key_id: String,
    pub session_id: String,
    pub session_challenge: String,
    pub sequence: u64,
    pub message_id: String,
    pub action_base64: String,
    pub signature: String,
    pub fact_uids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferEnvelopeV1 {
    pub version: u16,
    pub envelope_uid: String,
    pub origin_organ_uid: String,
    pub transfer_uid: String,
    pub transfer_revision: u64,
    pub delivery_policy_uid: String,
    pub delivery_policy_revision: u64,
    pub delivery_policy_state: String,
    pub cursor: u64,
    pub recipient_person_uid: String,
    pub recipient_organ_uid: String,
    pub mode: TransferDeliveryMode,
    pub disclosure: Vec<TransferDisclosureEntry>,
    pub projection: Value,
    #[serde(default)]
    pub events: Vec<Value>,
    #[serde(default)]
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub action_intents: Vec<TransferActionIntentEvidence>,
    #[serde(default)]
    pub actor_keys: Vec<TransferActorKey>,
    pub created_at: String,
    pub payload_hash: String,
    pub origin_key_id: String,
    pub origin_signature: String,
}

#[derive(Serialize)]
struct TransferEnvelopeHashInput<'a> {
    version: u16,
    envelope_uid: &'a str,
    origin_organ_uid: &'a str,
    transfer_uid: &'a str,
    transfer_revision: u64,
    delivery_policy_uid: &'a str,
    delivery_policy_revision: u64,
    delivery_policy_state: &'a str,
    cursor: u64,
    recipient_person_uid: &'a str,
    recipient_organ_uid: &'a str,
    mode: TransferDeliveryMode,
    disclosure: &'a [TransferDisclosureEntry],
    projection: &'a Value,
    events: &'a [Value],
    facts: &'a [Fact],
    action_intents: &'a [TransferActionIntentEvidence],
    actor_keys: &'a [TransferActorKey],
    created_at: &'a str,
    origin_key_id: &'a str,
}

impl TransferEnvelopeV1 {
    pub fn computed_payload_hash(&self) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(&TransferEnvelopeHashInput {
            version: self.version,
            envelope_uid: &self.envelope_uid,
            origin_organ_uid: &self.origin_organ_uid,
            transfer_uid: &self.transfer_uid,
            transfer_revision: self.transfer_revision,
            delivery_policy_uid: &self.delivery_policy_uid,
            delivery_policy_revision: self.delivery_policy_revision,
            delivery_policy_state: &self.delivery_policy_state,
            cursor: self.cursor,
            recipient_person_uid: &self.recipient_person_uid,
            recipient_organ_uid: &self.recipient_organ_uid,
            mode: self.mode,
            disclosure: &self.disclosure,
            projection: &self.projection,
            events: &self.events,
            facts: &self.facts,
            action_intents: &self.action_intents,
            actor_keys: &self.actor_keys,
            created_at: &self.created_at,
            origin_key_id: &self.origin_key_id,
        })?;
        Ok(hex_sha256(&bytes))
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        format!("{ENVELOPE_DOMAIN}\n{}", self.payload_hash).into_bytes()
    }

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != TRANSFER_ENVELOPE_VERSION {
            return Err("unsupported Transfer envelope version".into());
        }
        for (label, value) in [
            ("envelope uid", self.envelope_uid.as_str()),
            ("origin Organ uid", self.origin_organ_uid.as_str()),
            ("Transfer uid", self.transfer_uid.as_str()),
            ("delivery policy uid", self.delivery_policy_uid.as_str()),
            ("recipient Person uid", self.recipient_person_uid.as_str()),
            ("recipient Organ uid", self.recipient_organ_uid.as_str()),
            ("origin key id", self.origin_key_id.as_str()),
            ("origin signature", self.origin_signature.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 512 {
                return Err(format!("{label} has an invalid length"));
            }
        }
        if self.transfer_revision == 0
            || self.delivery_policy_revision == 0
            || self.cursor == 0
            || self.delivery_policy_state != "active"
        {
            return Err(
                "Transfer envelope revisions/cursor must be positive and policy active".into(),
            );
        }
        if self.disclosure.iter().any(|entry| {
            entry.path.trim().is_empty()
                || entry.path.len() > 1_024
                || entry.reason.trim().is_empty()
                || entry.reason.len() > 512
        }) {
            return Err("Transfer envelope disclosure entry is invalid".into());
        }
        let computed = self
            .computed_payload_hash()
            .map_err(|error| format!("cannot hash Transfer envelope: {error}"))?;
        if computed != self.payload_hash {
            return Err("Transfer envelope payload hash does not match its contents".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDeliveryPolicyEventV1 {
    pub version: u16,
    pub event_uid: String,
    pub delivery_policy_uid: String,
    pub policy_revision: u64,
    pub kind: String,
    pub origin_organ_uid: String,
    pub transfer_uid: String,
    pub recipient_person_uid: String,
    pub recipient_organ_uid: String,
    pub mode: TransferDeliveryMode,
    pub state: String,
    pub created_at: String,
    pub origin_key_id: String,
    pub payload_hash: String,
    pub origin_signature: String,
}

#[derive(Serialize)]
struct TransferDeliveryPolicyEventHashInput<'a> {
    version: u16,
    event_uid: &'a str,
    delivery_policy_uid: &'a str,
    policy_revision: u64,
    kind: &'a str,
    origin_organ_uid: &'a str,
    transfer_uid: &'a str,
    recipient_person_uid: &'a str,
    recipient_organ_uid: &'a str,
    mode: TransferDeliveryMode,
    state: &'a str,
    created_at: &'a str,
    origin_key_id: &'a str,
}

impl TransferDeliveryPolicyEventV1 {
    pub fn computed_payload_hash(&self) -> Result<String, serde_json::Error> {
        Ok(hex_sha256(&serde_json::to_vec(
            &TransferDeliveryPolicyEventHashInput {
                version: self.version,
                event_uid: &self.event_uid,
                delivery_policy_uid: &self.delivery_policy_uid,
                policy_revision: self.policy_revision,
                kind: &self.kind,
                origin_organ_uid: &self.origin_organ_uid,
                transfer_uid: &self.transfer_uid,
                recipient_person_uid: &self.recipient_person_uid,
                recipient_organ_uid: &self.recipient_organ_uid,
                mode: self.mode,
                state: &self.state,
                created_at: &self.created_at,
                origin_key_id: &self.origin_key_id,
            },
        )?))
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        format!("{DELIVERY_POLICY_EVENT_DOMAIN}\n{}", self.payload_hash).into_bytes()
    }

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != TRANSFER_ENVELOPE_VERSION
            || self.policy_revision == 0
            || !matches!(self.kind.as_str(), "reference" | "mode_changed" | "revoked")
            || !matches!(self.state.as_str(), "active" | "revoked")
            || (self.kind == "revoked") != (self.state == "revoked")
        {
            return Err("Transfer delivery policy event type or revision is invalid".into());
        }
        for (label, value) in [
            ("event uid", self.event_uid.as_str()),
            ("delivery policy uid", self.delivery_policy_uid.as_str()),
            ("origin Organ uid", self.origin_organ_uid.as_str()),
            ("Transfer uid", self.transfer_uid.as_str()),
            ("recipient Person uid", self.recipient_person_uid.as_str()),
            ("recipient Organ uid", self.recipient_organ_uid.as_str()),
            ("created at", self.created_at.as_str()),
            ("origin key id", self.origin_key_id.as_str()),
            ("origin signature", self.origin_signature.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 1_024 {
                return Err(format!("{label} has an invalid length"));
            }
        }
        if self
            .computed_payload_hash()
            .map_err(|error| format!("cannot hash Transfer policy event: {error}"))?
            != self.payload_hash
        {
            return Err("Transfer delivery policy event hash does not match its contents".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferApplicationHandoffState {
    Pending,
    Accepted,
    Rejected,
    Compensated,
}

impl TransferApplicationHandoffState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Compensated => "compensated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferApplicationHandoffV1 {
    pub version: u16,
    pub handoff_uid: String,
    pub origin_organ_uid: String,
    pub participant_organ_uid: String,
    pub participant_person_uid: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub source_promise_uid: String,
    pub settlement_slice_uid: String,
    pub origin_revision: u64,
    pub canonical_quantity: f64,
    pub canonical_unit_uid: Option<String>,
    pub canonical_cumulative_before: f64,
    pub canonical_cumulative_after: f64,
    pub canonical_remaining_after: f64,
    pub application_direction: i8,
    pub canonical_slice_hash: String,
    pub created_at: String,
    pub key_id: String,
    pub signature: String,
}

impl TransferApplicationHandoffV1 {
    pub fn signing_bytes(&self) -> Vec<u8> {
        format!(
            "{APPLICATION_HANDOFF_DOMAIN}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.version,
            self.handoff_uid,
            self.origin_organ_uid,
            self.participant_organ_uid,
            self.participant_person_uid,
            self.transfer_uid,
            self.occurrence_uid,
            self.source_promise_uid,
            self.settlement_slice_uid,
            self.origin_revision,
            self.canonical_quantity,
            self.canonical_unit_uid.as_deref().unwrap_or(""),
            self.canonical_cumulative_before,
            self.canonical_cumulative_after,
            self.canonical_remaining_after,
            self.application_direction,
            self.canonical_slice_hash,
            self.created_at,
            self.key_id,
        )
        .into_bytes()
    }

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != TRANSFER_ENVELOPE_VERSION
            || self.origin_revision == 0
            || !self.canonical_quantity.is_finite()
            || self.canonical_quantity <= 0.0
            || !self.canonical_cumulative_before.is_finite()
            || !self.canonical_cumulative_after.is_finite()
            || !self.canonical_remaining_after.is_finite()
            || self.canonical_cumulative_after <= self.canonical_cumulative_before
            || (self.canonical_cumulative_after
                - self.canonical_cumulative_before
                - self.canonical_quantity)
                .abs()
                > 1e-9
            || self.canonical_remaining_after < 0.0
            || !matches!(self.application_direction, -1 | 1)
        {
            return Err("application handoff quantities or revision are invalid".into());
        }
        for (label, value) in [
            ("handoff uid", self.handoff_uid.as_str()),
            ("origin Organ uid", self.origin_organ_uid.as_str()),
            ("participant Organ uid", self.participant_organ_uid.as_str()),
            (
                "participant Person uid",
                self.participant_person_uid.as_str(),
            ),
            ("Transfer uid", self.transfer_uid.as_str()),
            ("occurrence uid", self.occurrence_uid.as_str()),
            ("source promise uid", self.source_promise_uid.as_str()),
            ("settlement slice uid", self.settlement_slice_uid.as_str()),
            ("canonical slice hash", self.canonical_slice_hash.as_str()),
            ("created at", self.created_at.as_str()),
            ("key id", self.key_id.as_str()),
            ("signature", self.signature.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 1_024 {
                return Err(format!("{label} has an invalid length"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferApplicationAttestationV1 {
    pub version: u16,
    pub attestation_uid: String,
    pub origin_organ_uid: String,
    pub participant_organ_uid: String,
    pub participant_person_uid: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub settlement_slice_uid: String,
    pub origin_revision: u64,
    pub canonical_slice_hash: String,
    pub formula_commitment: String,
    pub formula_version: String,
    pub application_fact_uid: String,
    pub applied_at: String,
    pub key_id: String,
    pub signature: String,
}

impl TransferApplicationAttestationV1 {
    pub fn signing_bytes(&self) -> Vec<u8> {
        format!(
            "{APPLICATION_ATTESTATION_DOMAIN}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.version,
            self.attestation_uid,
            self.origin_organ_uid,
            self.participant_organ_uid,
            self.participant_person_uid,
            self.transfer_uid,
            self.occurrence_uid,
            self.settlement_slice_uid,
            self.origin_revision,
            self.canonical_slice_hash,
            self.formula_commitment,
            self.formula_version,
            self.application_fact_uid,
            self.applied_at,
            self.key_id,
        )
        .into_bytes()
    }

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != TRANSFER_ENVELOPE_VERSION || self.origin_revision == 0 {
            return Err("application attestation version or revision is invalid".into());
        }
        for (label, value) in [
            ("attestation uid", self.attestation_uid.as_str()),
            ("origin Organ uid", self.origin_organ_uid.as_str()),
            ("participant Organ uid", self.participant_organ_uid.as_str()),
            (
                "participant Person uid",
                self.participant_person_uid.as_str(),
            ),
            ("Transfer uid", self.transfer_uid.as_str()),
            ("occurrence uid", self.occurrence_uid.as_str()),
            ("settlement slice uid", self.settlement_slice_uid.as_str()),
            ("canonical slice hash", self.canonical_slice_hash.as_str()),
            ("formula commitment", self.formula_commitment.as_str()),
            ("formula version", self.formula_version.as_str()),
            ("application Fact uid", self.application_fact_uid.as_str()),
            ("applied at", self.applied_at.as_str()),
            ("key id", self.key_id.as_str()),
            ("signature", self.signature.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 1_024 {
                return Err(format!("{label} has an invalid length"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferPackageReceiptV1 {
    pub version: u16,
    pub request_id: String,
    pub envelope_uid: String,
    pub transfer_uid: String,
    pub origin_organ_uid: String,
    pub recipient_person_uid: String,
    pub recipient_organ_uid: String,
    pub cursor: u64,
    pub kind: String,
    pub created_at: String,
    pub key_id: String,
    pub signature: String,
}

impl TransferPackageReceiptV1 {
    pub fn signing_bytes(&self) -> Vec<u8> {
        format!(
            "{PACKAGE_RECEIPT_DOMAIN}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.version,
            self.request_id,
            self.envelope_uid,
            self.transfer_uid,
            self.origin_organ_uid,
            self.recipient_person_uid,
            self.recipient_organ_uid,
            self.cursor,
            self.kind,
            self.created_at,
            self.key_id,
        )
        .into_bytes()
    }

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != TRANSFER_ENVELOPE_VERSION
            || self.cursor == 0
            || !matches!(self.kind.as_str(), "received" | "seen")
        {
            return Err("package receipt version, cursor, or kind is invalid".into());
        }
        for (label, value) in [
            ("request id", self.request_id.as_str()),
            ("envelope uid", self.envelope_uid.as_str()),
            ("Transfer uid", self.transfer_uid.as_str()),
            ("origin Organ uid", self.origin_organ_uid.as_str()),
            ("recipient Person uid", self.recipient_person_uid.as_str()),
            ("recipient Organ uid", self.recipient_organ_uid.as_str()),
            ("created at", self.created_at.as_str()),
            ("key id", self.key_id.as_str()),
            ("signature", self.signature.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 1_024 {
                return Err(format!("{label} has an invalid length"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRemoteCommandV1 {
    pub version: u16,
    pub command_uid: String,
    pub request_id: String,
    pub origin_organ_uid: String,
    pub sender_organ_uid: String,
    pub transfer_uid: String,
    pub expected_revision: Option<u64>,
    pub actor_person_uid: String,
    pub key_id: String,
    pub public_key_base64: String,
    pub session_id: String,
    pub session_challenge: String,
    pub sequence: u64,
    pub message_id: String,
    pub action_base64: String,
    pub created_at: String,
    pub signature: String,
}

impl TransferRemoteCommandV1 {
    pub fn signing_bytes(&self) -> Vec<u8> {
        crate::action_intent::signing_bytes(
            &self.session_id,
            &self.session_challenge,
            self.sequence,
            &self.message_id,
            &self.action_base64,
        )
    }

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != TRANSFER_ENVELOPE_VERSION {
            return Err("unsupported remote Transfer command version".into());
        }
        for (label, value) in [
            ("command uid", self.command_uid.as_str()),
            ("request id", self.request_id.as_str()),
            ("origin Organ uid", self.origin_organ_uid.as_str()),
            ("sender Organ uid", self.sender_organ_uid.as_str()),
            ("Transfer uid", self.transfer_uid.as_str()),
            ("actor Person uid", self.actor_person_uid.as_str()),
            ("key id", self.key_id.as_str()),
            ("public key", self.public_key_base64.as_str()),
            ("session id", self.session_id.as_str()),
            ("session challenge", self.session_challenge.as_str()),
            ("message id", self.message_id.as_str()),
            ("Action payload", self.action_base64.as_str()),
            ("created at", self.created_at.as_str()),
            ("signature", self.signature.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 1_398_104 {
                return Err(format!("{label} has an invalid length"));
            }
        }
        if self.sequence == 0 {
            return Err("remote Transfer command sequence must be positive".into());
        }
        Ok(())
    }

    pub fn payload_hash(&self) -> String {
        hex_sha256(&self.signing_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedOrganRequestV1 {
    pub version: u16,
    pub sender_organ_uid: String,
    pub recipient_organ_uid: String,
    pub method: String,
    pub path: String,
    pub body_hash: String,
    pub timestamp: String,
    pub nonce: String,
    pub key_id: String,
    pub signature: String,
}

impl SignedOrganRequestV1 {
    pub fn signing_bytes(&self) -> Vec<u8> {
        format!(
            "{ORGAN_REQUEST_DOMAIN}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.version,
            self.sender_organ_uid,
            self.recipient_organ_uid,
            self.method,
            self.path,
            self.body_hash,
            self.timestamp,
            self.nonce,
            self.key_id,
        )
        .into_bytes()
    }

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != TRANSFER_ENVELOPE_VERSION {
            return Err("unsupported signed Organ request version".into());
        }
        if !matches!(self.method.as_str(), "GET" | "POST") || !self.path.starts_with('/') {
            return Err("signed Organ request method or path is invalid".into());
        }
        for (label, value) in [
            ("sender Organ uid", self.sender_organ_uid.as_str()),
            ("recipient Organ uid", self.recipient_organ_uid.as_str()),
            ("body hash", self.body_hash.as_str()),
            ("timestamp", self.timestamp.as_str()),
            ("nonce", self.nonce.as_str()),
            ("key id", self.key_id.as_str()),
            ("signature", self.signature.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 1_024 {
                return Err(format!("{label} has an invalid length"));
            }
        }
        Ok(())
    }

    pub fn request_hash(&self) -> String {
        hex_sha256(&self.signing_bytes())
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}
