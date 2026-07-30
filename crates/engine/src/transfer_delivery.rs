//! Origin-authoritative cross-Cell Transfer delivery.
//!
//! This module deliberately does not use generic Sync. Transfer envelopes are
//! recipient-specific, Organ-signed, and retain Person-authored proof without
//! inserting remote Facts into the local Ledger chain.

use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use nucleus::transfer_delivery::{
    SignedOrganRequestV1, TRANSFER_ENVELOPE_VERSION, TransferApplicationAttestationV1,
    TransferApplicationHandoffState, TransferApplicationHandoffV1, TransferDeliveryMode,
    TransferDeliveryPolicyEventV1, TransferDisclosureEntry, TransferEnvelopeV1,
    TransferPackageReceiptV1, TransferRemoteCommandV1,
};
use sha2::{Digest, Sha256};

use crate::{Engine, EngineError};

impl Engine {
    pub async fn sign_transfer_application_handoff(
        &self,
        mut handoff: TransferApplicationHandoffV1,
    ) -> Result<TransferApplicationHandoffV1, EngineError> {
        let signer = self.organ_signer.lock().await.clone().ok_or_else(|| {
            delivery_conflict(
                "organ_signer_unavailable",
                "local Organ signer is unavailable",
            )
        })?;
        if handoff.origin_organ_uid != signer.actor_uid {
            return Err(delivery_conflict(
                "transfer_application_handoff_origin_mismatch",
                "application handoff origin does not match the local Organ",
            ));
        }
        handoff.version = TRANSFER_ENVELOPE_VERSION;
        handoff.key_id = signer.key_id.clone();
        handoff.signature.clear();
        handoff.signature = signer.sign_bytes(&handoff.signing_bytes());
        handoff.validate_shape().map_err(|message| {
            delivery_conflict("transfer_application_handoff_invalid", message)
        })?;
        Ok(handoff)
    }

    pub async fn verify_transfer_application_handoff(
        &self,
        handoff: &TransferApplicationHandoffV1,
    ) -> Result<(), EngineError> {
        handoff.validate_shape().map_err(|message| {
            delivery_conflict("transfer_application_handoff_invalid", message)
        })?;
        let key = identity_public_key(self, &handoff.origin_organ_uid, &handoff.key_id).await?;
        verify_signature(&key, &handoff.signature, &handoff.signing_bytes()).map_err(|_| {
            delivery_conflict(
                "transfer_application_handoff_signature_invalid",
                "application handoff origin signature is invalid",
            )
        })
    }

    pub async fn sign_transfer_package_receipt(
        &self,
        mut receipt: TransferPackageReceiptV1,
    ) -> Result<TransferPackageReceiptV1, EngineError> {
        let signer = self.organ_signer.lock().await.clone().ok_or_else(|| {
            delivery_conflict(
                "organ_signer_unavailable",
                "local Organ signer is unavailable",
            )
        })?;
        if receipt.recipient_organ_uid != signer.actor_uid {
            return Err(delivery_conflict(
                "transfer_package_receipt_signer_mismatch",
                "package receipt recipient does not match the local Organ signer",
            ));
        }
        receipt.version = TRANSFER_ENVELOPE_VERSION;
        receipt.key_id = signer.key_id.clone();
        receipt.signature.clear();
        receipt.signature = signer.sign_bytes(&receipt.signing_bytes());
        receipt
            .validate_shape()
            .map_err(|message| delivery_conflict("transfer_package_receipt_invalid", message))?;
        Ok(receipt)
    }

    pub async fn verify_transfer_package_receipt(
        &self,
        receipt: &TransferPackageReceiptV1,
    ) -> Result<(), EngineError> {
        receipt
            .validate_shape()
            .map_err(|message| delivery_conflict("transfer_package_receipt_invalid", message))?;
        let public_key =
            identity_public_key(self, &receipt.recipient_organ_uid, &receipt.key_id).await?;
        verify_signature(&public_key, &receipt.signature, &receipt.signing_bytes()).map_err(|_| {
            delivery_conflict(
                "transfer_package_receipt_signature_invalid",
                "package receipt Organ signature is invalid",
            )
        })
    }

    /// Reject canonical Transfer work anywhere except the Cell that created it.
    pub async fn require_transfer_origin_authority(
        &self,
        transfer_uid: &str,
    ) -> Result<String, EngineError> {
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
        let record = store::records::get(&self.store.pool, transfer_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(transfer_uid.into()))?;
        if record.kind != nucleus::RecordKind::Transfer.as_str() {
            return Err(EngineError::Consequence(
                "Transfer delivery target is not a Transfer".into(),
            ));
        }
        if record.organ_uid.as_deref() != Some(local.uid.as_str()) {
            return Err(EngineError::Conflict {
                code: "transfer_origin_authority_required",
                message: "canonical Transfer Actions must be submitted to the origin Cell".into(),
            });
        }
        Ok(local.uid)
    }

    /// Finish a recipient-redacted envelope with the dedicated local Organ key.
    pub async fn sign_transfer_envelope(
        &self,
        mut envelope: TransferEnvelopeV1,
    ) -> Result<TransferEnvelopeV1, EngineError> {
        let signer =
            self.organ_signer
                .lock()
                .await
                .clone()
                .ok_or_else(|| EngineError::Conflict {
                    code: "organ_signer_unavailable",
                    message: "the local Cell has no Organ transport signing key".into(),
                })?;
        if envelope.origin_organ_uid != signer.actor_uid {
            return Err(EngineError::Conflict {
                code: "transfer_envelope_origin_mismatch",
                message: "the envelope origin does not match the local Organ signer".into(),
            });
        }
        envelope.version = TRANSFER_ENVELOPE_VERSION;
        envelope.origin_key_id = signer.key_id.clone();
        envelope.origin_signature.clear();
        envelope.payload_hash = envelope
            .computed_payload_hash()
            .map_err(EngineError::Json)?;
        envelope.origin_signature = signer.sign_bytes(&envelope.signing_bytes());
        Ok(envelope)
    }

    pub async fn sign_transfer_delivery_policy_event(
        &self,
        policy: &store::transfer_delivery::DeliveryPolicyRow,
        kind: &str,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> Result<TransferDeliveryPolicyEventV1, EngineError> {
        self.require_transfer_origin_authority(&policy.transfer_uid)
            .await?;
        let signer = self.organ_signer.lock().await.clone().ok_or_else(|| {
            delivery_conflict(
                "organ_signer_unavailable",
                "local Organ signer is unavailable",
            )
        })?;
        if policy.origin_organ_uid != signer.actor_uid {
            return Err(delivery_conflict(
                "transfer_delivery_policy_origin_mismatch",
                "delivery policy does not belong to the local Organ signer",
            ));
        }
        let mut event = TransferDeliveryPolicyEventV1 {
            version: TRANSFER_ENVELOPE_VERSION,
            event_uid: format!("tdpe:{}:{}", policy.uid, policy.revision),
            delivery_policy_uid: policy.uid.clone(),
            policy_revision: policy.revision,
            kind: kind.into(),
            origin_organ_uid: policy.origin_organ_uid.clone(),
            transfer_uid: policy.transfer_uid.clone(),
            recipient_person_uid: policy.recipient_person_uid.clone(),
            recipient_organ_uid: policy.recipient_organ_uid.clone(),
            mode: policy.mode,
            state: policy.state.clone(),
            created_at: policy.updated_at.clone(),
            origin_key_id: signer.key_id.clone(),
            payload_hash: String::new(),
            origin_signature: String::new(),
        };
        event.payload_hash = event.computed_payload_hash().map_err(EngineError::Json)?;
        event.origin_signature = signer.sign_bytes(&event.signing_bytes());
        event.validate_shape().map_err(|message| {
            delivery_conflict("transfer_delivery_policy_event_invalid", message)
        })?;
        Ok(event)
    }

    pub async fn verify_transfer_delivery_policy_event(
        &self,
        event: &TransferDeliveryPolicyEventV1,
    ) -> Result<(), EngineError> {
        event.validate_shape().map_err(|message| {
            delivery_conflict("transfer_delivery_policy_event_invalid", message)
        })?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
        if event.recipient_organ_uid != local.uid {
            return Err(delivery_conflict(
                "transfer_delivery_policy_wrong_recipient",
                "delivery policy event names a different recipient Organ",
            ));
        }
        let contact = store::organs::contact(&self.store.pool, &event.origin_organ_uid)
            .await?
            .ok_or_else(|| {
                delivery_conflict(
                    "transfer_delivery_policy_origin_unknown",
                    "delivery policy origin Organ is unknown",
                )
            })?;
        if contact.trust == "blocked" || !contact.sync_in {
            return Err(EngineError::Forbidden(
                "delivery policy origin is blocked".into(),
            ));
        }
        let public_key =
            identity_public_key(self, &event.origin_organ_uid, &event.origin_key_id).await?;
        verify_signature(&public_key, &event.origin_signature, &event.signing_bytes()).map_err(
            |_| {
                delivery_conflict(
                    "transfer_delivery_policy_signature_invalid",
                    "delivery policy Organ signature is invalid",
                )
            },
        )?;
        Ok(())
    }

    /// Verify transport authority and every included Person proof before an
    /// isolated hosted/replica store is allowed to retain an envelope.
    pub async fn verify_transfer_envelope(
        &self,
        envelope: &TransferEnvelopeV1,
    ) -> Result<(), EngineError> {
        envelope
            .validate_shape()
            .map_err(|message| delivery_conflict("transfer_envelope_invalid", message))?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
        if envelope.recipient_organ_uid != local.uid {
            return Err(delivery_conflict(
                "transfer_envelope_wrong_recipient",
                "Transfer envelope names a different recipient Organ",
            ));
        }
        let contact = store::organs::contact(&self.store.pool, &envelope.origin_organ_uid)
            .await?
            .ok_or_else(|| {
                delivery_conflict(
                    "transfer_envelope_origin_unknown",
                    "Transfer envelope origin has not been introduced",
                )
            })?;
        if contact.trust == "blocked" || !contact.sync_in {
            return Err(EngineError::Forbidden(
                "Transfer envelope origin is blocked or not enabled for incoming delivery".into(),
            ));
        }
        let origin_key: Option<String> = store::sqlx::query_scalar(
            "SELECT public_key FROM identity_key WHERE actor_uid = ? AND key_id = ?",
        )
        .bind(&envelope.origin_organ_uid)
        .bind(&envelope.origin_key_id)
        .fetch_optional(&self.store.pool)
        .await?;
        let origin_key = origin_key.ok_or_else(|| {
            delivery_conflict(
                "transfer_envelope_origin_key_unknown",
                "Transfer envelope origin key is not published locally",
            )
        })?;
        verify_signature(
            &origin_key,
            &envelope.origin_signature,
            &envelope.signing_bytes(),
        )
        .map_err(|_| {
            delivery_conflict(
                "transfer_envelope_signature_invalid",
                "Transfer envelope Organ signature is invalid",
            )
        })?;

        if envelope
            .projection
            .get("uid")
            .and_then(serde_json::Value::as_str)
            != Some(envelope.transfer_uid.as_str())
        {
            return Err(delivery_conflict(
                "transfer_envelope_projection_mismatch",
                "Transfer envelope projection names a different Transfer",
            ));
        }
        if !envelope.facts.is_empty() || !envelope.action_intents.is_empty() {
            return Err(delivery_conflict(
                "transfer_envelope_raw_proof_forbidden",
                "raw Fact payloads and Action bytes are not accepted by the redacted delivery format",
            ));
        }
        verify_envelope_person_proof(envelope)?;
        Ok(())
    }

    pub async fn create_transfer_delivery_policy(
        &self,
        transfer_uid: &str,
        recipient_person_uid: &str,
        recipient_organ_uid: &str,
        actor_person_uid: &str,
        fact_uid: &str,
        request_id: &str,
        mode: TransferDeliveryMode,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<store::transfer_delivery::DeliveryPolicyRow, EngineError> {
        let origin_organ_uid = self.require_transfer_origin_authority(transfer_uid).await?;
        require_kind(self, recipient_person_uid, nucleus::RecordKind::Person).await?;
        require_kind(self, recipient_organ_uid, nucleus::RecordKind::Organ).await?;
        let is_party =
            store::transfers::party_for_actor(&self.store.pool, transfer_uid, recipient_person_uid)
                .await?
                .is_some();
        if !is_party {
            return Err(delivery_conflict(
                "transfer_delivery_recipient_not_eligible",
                "recipient must be an active Transfer party",
            ));
        }
        let contact = store::organs::contact(&self.store.pool, recipient_organ_uid)
            .await?
            .ok_or_else(|| {
                delivery_conflict(
                    "transfer_delivery_organ_not_introduced",
                    "recipient Organ must be introduced before delivery is configured",
                )
            })?;
        if contact.trust == "blocked" || !contact.sync_out {
            return Err(EngineError::Forbidden(
                "recipient Organ is blocked or not enabled for outgoing delivery".into(),
            ));
        }
        let commit = store::transfer_delivery::create_policy(
            &self.store.pool,
            store::transfer_delivery::NewDeliveryPolicy {
                transfer_uid,
                origin_organ_uid: &origin_organ_uid,
                recipient_person_uid,
                recipient_organ_uid,
                mode,
                actor_person_uid,
                fact_uid,
                request_id,
            },
            now,
        )
        .await?;
        Ok(policy_from_commit(commit))
    }

    pub async fn change_transfer_delivery_mode(
        &self,
        transfer_uid: &str,
        delivery_uid: &str,
        expected_revision: u64,
        actor_person_uid: &str,
        fact_uid: &str,
        request_id: &str,
        mode: TransferDeliveryMode,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<store::transfer_delivery::DeliveryPolicyRow, EngineError> {
        self.require_delivery_policy(transfer_uid, delivery_uid)
            .await?;
        Ok(policy_from_commit(
            store::transfer_delivery::change_policy_mode(
                &self.store.pool,
                store::transfer_delivery::DeliveryPolicyTransition {
                    delivery_uid,
                    expected_revision,
                    actor_person_uid,
                    fact_uid,
                    request_id,
                },
                mode,
                now,
            )
            .await?,
        ))
    }

    pub async fn revoke_transfer_delivery_policy(
        &self,
        transfer_uid: &str,
        delivery_uid: &str,
        expected_revision: u64,
        actor_person_uid: &str,
        fact_uid: &str,
        request_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<store::transfer_delivery::DeliveryPolicyRow, EngineError> {
        self.require_delivery_policy(transfer_uid, delivery_uid)
            .await?;
        Ok(policy_from_commit(
            store::transfer_delivery::revoke_policy(
                &self.store.pool,
                store::transfer_delivery::DeliveryPolicyTransition {
                    delivery_uid,
                    expected_revision,
                    actor_person_uid,
                    fact_uid,
                    request_id,
                },
                now,
            )
            .await?,
        ))
    }

    /// Build and durably enqueue one current recipient projection. Replaying
    /// the same request id returns the original outbox row without rebuilding
    /// bytes with a different timestamp.
    pub async fn enqueue_transfer_delivery(
        &self,
        transfer_uid: &str,
        delivery_uid: &str,
        request_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<String, EngineError> {
        let envelope_uid = format!("te:{request_id}");
        if let Some(existing) = store::sqlx::query_scalar::<_, String>(
            "SELECT uid FROM transfer_delivery_outbox WHERE envelope_uid = ?",
        )
        .bind(&envelope_uid)
        .fetch_optional(&self.store.pool)
        .await?
        {
            return Ok(existing);
        }
        let policy = self
            .require_delivery_policy(transfer_uid, delivery_uid)
            .await?;
        if policy.state != "active" {
            return Err(delivery_conflict(
                "transfer_delivery_revoked",
                "revoked delivery cannot enqueue future envelopes",
            ));
        }
        let transfer = store::transfers::get(&self.store.pool, transfer_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(transfer_uid.into()))?;
        let cursor = store::sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(cursor), 0) + 1 FROM transfer_delivery_outbox WHERE delivery_uid = ?",
        )
        .bind(delivery_uid)
        .fetch_one(&self.store.pool)
        .await? as u64;
        let mut projection = protein::transfer_delivery_projection(
            &self.store,
            transfer_uid,
            &policy.recipient_person_uid,
            &policy.recipient_organ_uid,
        )
        .await
        .map_err(|error| {
            delivery_conflict("transfer_delivery_projection_failed", error.to_string())
        })?;
        let handoffs = store::sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                String,
                i64,
                String,
                String,
                f64,
                Option<String>,
                f64,
                f64,
                f64,
                i64,
                String,
            ),
        >(
            "SELECT h.uid, h.origin_organ_uid, h.participant_organ_uid,
                    h.participant_person_uid, h.occurrence_uid, d.source_promise_uid,
                    h.origin_revision, h.settlement_slice_uid, h.canonical_slice_hash,
                    d.canonical_quantity, d.canonical_unit_uid,
                    d.canonical_cumulative_before, d.canonical_cumulative_after,
                    d.canonical_remaining_after, d.application_direction, d.created_at
             FROM transfer_application_handoff h
             JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
             WHERE h.transfer_uid = ? AND h.participant_person_uid = ?
               AND h.participant_organ_uid = ? AND h.state = 'pending'
             ORDER BY d.created_at, h.uid",
        )
        .bind(transfer_uid)
        .bind(&policy.recipient_person_uid)
        .bind(&policy.recipient_organ_uid)
        .fetch_all(&self.store.pool)
        .await?;
        let mut signed_handoffs = Vec::with_capacity(handoffs.len());
        for row in handoffs {
            signed_handoffs.push(
                self.sign_transfer_application_handoff(TransferApplicationHandoffV1 {
                    version: TRANSFER_ENVELOPE_VERSION,
                    handoff_uid: row.0,
                    origin_organ_uid: row.1,
                    participant_organ_uid: row.2,
                    participant_person_uid: row.3,
                    transfer_uid: transfer_uid.into(),
                    occurrence_uid: row.4,
                    source_promise_uid: row.5,
                    settlement_slice_uid: row.7,
                    origin_revision: row.6 as u64,
                    canonical_quantity: row.9,
                    canonical_unit_uid: row.10,
                    canonical_cumulative_before: row.11,
                    canonical_cumulative_after: row.12,
                    canonical_remaining_after: row.13,
                    application_direction: row.14 as i8,
                    canonical_slice_hash: row.8,
                    created_at: row.15,
                    key_id: String::new(),
                    signature: String::new(),
                })
                .await?,
            );
        }
        if let Some(object) = projection.as_object_mut() {
            object.insert(
                "application_handoffs".into(),
                serde_json::to_value(signed_handoffs).map_err(EngineError::Json)?,
            );
        }
        let disclosure = projection
            .as_object()
            .into_iter()
            .flat_map(|object| object.keys())
            .map(|key| TransferDisclosureEntry {
                path: key.clone(),
                reason: "recipient Transfer projection".into(),
            })
            .collect();
        let envelope = self
            .sign_transfer_envelope(TransferEnvelopeV1 {
                version: TRANSFER_ENVELOPE_VERSION,
                envelope_uid,
                origin_organ_uid: policy.origin_organ_uid.clone(),
                transfer_uid: transfer_uid.into(),
                transfer_revision: transfer.revision as u64,
                delivery_policy_uid: policy.uid.clone(),
                delivery_policy_revision: policy.revision,
                delivery_policy_state: policy.state.clone(),
                cursor,
                recipient_person_uid: policy.recipient_person_uid.clone(),
                recipient_organ_uid: policy.recipient_organ_uid.clone(),
                mode: policy.mode,
                disclosure,
                projection,
                events: Vec::new(),
                facts: Vec::new(),
                action_intents: Vec::new(),
                actor_keys: Vec::new(),
                created_at: now.to_rfc3339(),
                payload_hash: String::new(),
                origin_key_id: String::new(),
                origin_signature: String::new(),
            })
            .await?;
        let commit =
            store::transfer_delivery::enqueue(&self.store.pool, delivery_uid, &envelope, now)
                .await?;
        Ok(match commit {
            store::transfer_delivery::EnqueueCommit::Applied(row)
            | store::transfer_delivery::EnqueueCommit::Replayed(row) => row.uid,
        })
    }

    pub async fn retry_transfer_delivery(
        &self,
        transfer_uid: &str,
        delivery_uid: &str,
        request_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<String, EngineError> {
        self.require_delivery_policy(transfer_uid, delivery_uid)
            .await?;
        Ok(
            store::transfer_delivery::retry_outbox(&self.store.pool, delivery_uid, request_id, now)
                .await?
                .uid,
        )
    }

    async fn require_delivery_policy(
        &self,
        transfer_uid: &str,
        delivery_uid: &str,
    ) -> Result<store::transfer_delivery::DeliveryPolicyRow, EngineError> {
        self.require_transfer_origin_authority(transfer_uid).await?;
        let policy = store::transfer_delivery::policy(&self.store.pool, delivery_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(delivery_uid.into()))?;
        if policy.transfer_uid != transfer_uid {
            return Err(delivery_conflict(
                "transfer_delivery_target_mismatch",
                "delivery policy belongs to another Transfer",
            ));
        }
        Ok(policy)
    }

    pub async fn sign_organ_request(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        recipient_organ_uid: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<SignedOrganRequestV1, EngineError> {
        let signer = self.organ_signer.lock().await.clone().ok_or_else(|| {
            delivery_conflict(
                "organ_signer_unavailable",
                "local Organ signer is unavailable",
            )
        })?;
        let mut request = SignedOrganRequestV1 {
            version: TRANSFER_ENVELOPE_VERSION,
            sender_organ_uid: signer.actor_uid.clone(),
            recipient_organ_uid: recipient_organ_uid.into(),
            method: method.into(),
            path: path.into(),
            body_hash: hex_sha256(body),
            timestamp: now.to_rfc3339(),
            nonce: uuid::Uuid::new_v4().to_string(),
            key_id: signer.key_id.clone(),
            signature: String::new(),
        };
        request.signature = signer.sign_bytes(&request.signing_bytes());
        request
            .validate_shape()
            .map_err(|message| delivery_conflict("organ_request_invalid", message))?;
        Ok(request)
    }

    pub async fn verify_organ_request(
        &self,
        request: &SignedOrganRequestV1,
        method: &str,
        path: &str,
        body: &[u8],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), EngineError> {
        request
            .validate_shape()
            .map_err(|message| delivery_conflict("organ_request_invalid", message))?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
        if request.recipient_organ_uid != local.uid
            || request.method != method
            || request.path != path
            || request.body_hash != hex_sha256(body)
        {
            return Err(delivery_conflict(
                "organ_request_binding_mismatch",
                "signed Organ request does not bind this recipient, route, or body",
            ));
        }
        let timestamp = chrono::DateTime::parse_from_rfc3339(&request.timestamp)
            .map_err(|_| {
                delivery_conflict("organ_request_time_invalid", "request timestamp is invalid")
            })?
            .with_timezone(&chrono::Utc);
        if (now - timestamp).num_seconds().abs() > 300 {
            return Err(delivery_conflict(
                "organ_request_expired",
                "signed Organ request is outside the five minute acceptance window",
            ));
        }
        let contact = store::organs::contact(&self.store.pool, &request.sender_organ_uid)
            .await?
            .ok_or_else(|| {
                delivery_conflict("organ_request_sender_unknown", "sender Organ is unknown")
            })?;
        if contact.trust == "blocked" || !contact.sync_in {
            return Err(EngineError::Forbidden(
                "sender Organ is blocked or incoming delivery is disabled".into(),
            ));
        }
        let public_key =
            identity_public_key(self, &request.sender_organ_uid, &request.key_id).await?;
        verify_signature(&public_key, &request.signature, &request.signing_bytes()).map_err(
            |_| {
                delivery_conflict(
                    "organ_request_signature_invalid",
                    "Organ request signature is invalid",
                )
            },
        )?;
        store::transfer_delivery::consume_organ_request_nonce(&self.store.pool, request, now)
            .await?;
        Ok(())
    }

    pub async fn verify_transfer_remote_command(
        &self,
        command: &TransferRemoteCommandV1,
    ) -> Result<crate::actions::Action, EngineError> {
        command
            .validate_shape()
            .map_err(|message| delivery_conflict("transfer_remote_command_invalid", message))?;
        let local = self
            .require_transfer_origin_authority(&command.transfer_uid)
            .await?;
        if command.origin_organ_uid != local {
            return Err(delivery_conflict(
                "transfer_remote_command_wrong_origin",
                "remote command names a different canonical origin Organ",
            ));
        }
        let contact = store::organs::contact(&self.store.pool, &command.sender_organ_uid)
            .await?
            .ok_or_else(|| {
                delivery_conflict(
                    "transfer_remote_command_sender_unknown",
                    "sender Organ is unknown",
                )
            })?;
        if contact.trust == "blocked" || !contact.sync_in {
            return Err(EngineError::Forbidden(
                "remote command sender is blocked".into(),
            ));
        }
        let actor = store::records::get(&self.store.pool, &command.actor_person_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(command.actor_person_uid.clone()))?;
        if actor.kind != nucleus::RecordKind::Person.as_str()
            || actor.organ_uid.as_deref() != Some(command.sender_organ_uid.as_str())
        {
            return Err(delivery_conflict(
                "transfer_remote_command_actor_binding_invalid",
                "command actor Person is not bound to the sender Organ",
            ));
        }
        let decoded_key = B64.decode(&command.public_key_base64).map_err(|_| {
            delivery_conflict(
                "transfer_remote_command_key_invalid",
                "remote command public key is invalid",
            )
        })?;
        let canonical_key: [u8; 32] = decoded_key.try_into().map_err(|_| {
            delivery_conflict(
                "transfer_remote_command_key_invalid",
                "remote command public key is invalid",
            )
        })?;
        if B64.encode(canonical_key) != command.public_key_base64 {
            return Err(delivery_conflict(
                "transfer_remote_command_key_invalid",
                "remote command public key is not canonical base64",
            ));
        }
        let existing: Option<String> = store::sqlx::query_scalar(
            "SELECT public_key FROM identity_key WHERE actor_uid = ? AND key_id = ?",
        )
        .bind(&command.actor_person_uid)
        .bind(&command.key_id)
        .fetch_optional(&self.store.pool)
        .await?;
        if existing
            .as_deref()
            .is_some_and(|key| key != command.public_key_base64)
        {
            return Err(delivery_conflict(
                "transfer_remote_command_key_conflict",
                "remote command key id is already bound to different material",
            ));
        }
        let public_key = existing.unwrap_or_else(|| command.public_key_base64.clone());
        verify_signature(&public_key, &command.signature, &command.signing_bytes()).map_err(
            |_| {
                delivery_conflict(
                    "transfer_remote_command_signature_invalid",
                    "remote command signature is invalid",
                )
            },
        )?;
        store::sqlx::query(
            "INSERT OR IGNORE INTO identity_key (actor_uid, key_id, public_key) VALUES (?, ?, ?)",
        )
        .bind(&command.actor_person_uid)
        .bind(&command.key_id)
        .bind(&public_key)
        .execute(&self.store.pool)
        .await?;
        let bytes = B64.decode(&command.action_base64).map_err(|_| {
            delivery_conflict(
                "transfer_remote_command_action_invalid",
                "Action payload is not base64",
            )
        })?;
        let action: crate::actions::Action =
            serde_json::from_slice(&bytes).map_err(EngineError::Json)?;
        let targets = self.canonical_transfer_action_targets(&action).await?;
        if targets.len() != 1 || targets[0] != command.transfer_uid {
            return Err(delivery_conflict(
                "transfer_remote_command_target_mismatch",
                "remote Action must target exactly the command Transfer",
            ));
        }
        let (submitted_person, request_id, expected_revision) = remote_action_binding(&action)
            .ok_or_else(|| {
                delivery_conflict(
                    "transfer_remote_command_action_unsupported",
                    "remote Action lacks an explicit Person and stable request binding",
                )
            })?;
        let submitted_person = store::records::resolve(&self.store.pool, submitted_person)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(submitted_person.into()))?;
        if submitted_person.uid != command.actor_person_uid
            || request_id != command.request_id
            || expected_revision != command.expected_revision
        {
            return Err(delivery_conflict(
                "transfer_remote_command_authorship_mismatch",
                "remote command actor, request id, or revision differs from its Action",
            ));
        }
        Ok(action)
    }

    /// Verify, deduplicate and execute a Person-signed command without
    /// entering trusted-local mode or substituting the origin Cell's identity.
    pub async fn accept_transfer_remote_command(
        &self,
        command: &TransferRemoteCommandV1,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<serde_json::Value, EngineError> {
        let action = self.verify_transfer_remote_command(command).await?;
        let persisted = store::transfer_delivery::persist_remote_command(
            &self.store.pool,
            "incoming",
            command,
            now,
        )
        .await?;
        if let store::transfer_delivery::RemoteCommandCommit::Replayed(row) = &persisted
            && let Some(payload) = row.result_payload.as_deref()
        {
            return serde_json::from_str(payload).map_err(EngineError::Json);
        }
        let intent_uid = self.ensure_remote_action_intent(command, now).await?;
        let result = self
            .act_at_with_authorship(
                action,
                None,
                now,
                Some(crate::actions::VerifiedActionAuthorship {
                    person_uid: command.actor_person_uid.clone(),
                    intent_uid: intent_uid.clone(),
                }),
            )
            .await;
        let revision = store::transfers::get(&self.store.pool, &command.transfer_uid)
            .await?
            .map(|transfer| transfer.revision as u64)
            .unwrap_or_default();
        match result {
            Ok(outcome) => {
                let fact_uids = outcome
                    .facts
                    .iter()
                    .filter(|fact| {
                        fact.actor_uid.as_deref() == Some(command.actor_person_uid.as_str())
                    })
                    .map(|fact| fact.uid.clone())
                    .collect::<Vec<_>>();
                store::action_intents::mark_committed(
                    &self.store.pool,
                    &intent_uid,
                    &fact_uids,
                    now,
                )
                .await?;
                let fact_refs = outcome
                    .facts
                    .iter()
                    .map(|fact| {
                        serde_json::json!({
                            "uid": fact.uid,
                            "hash": fact.hash,
                        })
                    })
                    .collect::<Vec<_>>();
                let payload = serde_json::json!({
                    "command_uid": command.command_uid,
                    "accepted": true,
                    "created": outcome.created,
                    "fact_refs": fact_refs,
                    "warnings": outcome.warnings,
                    "authoritative_revision": revision,
                });
                store::transfer_delivery::finish_remote_command(
                    &self.store.pool,
                    &command.command_uid,
                    true,
                    revision,
                    &payload,
                    None,
                    None,
                    now,
                )
                .await?;
                Ok(payload)
            }
            Err(error) => {
                store::action_intents::mark_failed(
                    &self.store.pool,
                    &intent_uid,
                    error.code(),
                    &error.to_string(),
                    now,
                )
                .await?;
                let payload = serde_json::json!({
                    "command_uid": command.command_uid,
                    "accepted": false,
                    "code": error.code(),
                    "message": error.to_string(),
                    "authoritative_revision": revision,
                });
                store::transfer_delivery::finish_remote_command(
                    &self.store.pool,
                    &command.command_uid,
                    false,
                    revision,
                    &payload,
                    error.code(),
                    Some(&error.to_string()),
                    now,
                )
                .await?;
                Ok(payload)
            }
        }
    }

    async fn ensure_remote_action_intent(
        &self,
        command: &TransferRemoteCommandV1,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<String, EngineError> {
        if let Some(row) = store::sqlx::query_as::<
            _,
            (
                String,
                String,
                i64,
                String,
                String,
                String,
                String,
                String,
                String,
            ),
        >(
            "SELECT uid, session_challenge, sequence, message_id, actor_person_uid,
                    key_id, action_base64, signature, status
             FROM signed_action_intent WHERE session_id = ? AND message_id = ?",
        )
        .bind(&command.session_id)
        .bind(&command.message_id)
        .fetch_optional(&self.store.pool)
        .await?
        {
            if row.1 != command.session_challenge
                || row.2 != command.sequence as i64
                || row.3 != command.message_id
                || row.4 != command.actor_person_uid
                || row.5 != command.key_id
                || row.6 != command.action_base64
                || row.7 != command.signature
                || row.8 == "failed"
            {
                return Err(delivery_conflict(
                    "transfer_remote_intent_replay_conflict",
                    "remote Action intent identity changed or previously failed",
                ));
            }
            return Ok(row.0);
        }
        Ok(store::action_intents::insert_pending(
            &self.store.pool,
            store::action_intents::NewSignedActionIntent {
                session_id: &command.session_id,
                session_challenge: &command.session_challenge,
                sequence: command.sequence,
                message_id: &command.message_id,
                actor_person_uid: &command.actor_person_uid,
                key_id: &command.key_id,
                action_base64: &command.action_base64,
                signature: &command.signature,
            },
            now,
        )
        .await?
        .uid)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn begin_remote_transfer_settlement(
        &self,
        transfer_uid: &str,
        occurrence_uid: &str,
        actor_person_uid: &str,
        expected_revision: u64,
        expected_remaining: f64,
        canonical_quantity: f64,
        request_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<store::transfer_delivery::ApplicationHandoffRow, EngineError> {
        let origin_organ_uid = self.require_transfer_origin_authority(transfer_uid).await?;
        let revision: i64 =
            store::sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid = ?")
                .bind(transfer_uid)
                .fetch_one(&self.store.pool)
                .await?;
        if revision as u64 != expected_revision {
            return Err(delivery_conflict(
                "transfer_revision_conflict",
                "Transfer changed after settlement review",
            ));
        }
        let replay_handoff = if let Some(uid) = store::sqlx::query_scalar::<_, String>(
            "SELECT handoff_uid FROM transfer_application_handoff_event WHERE request_id = ?",
        )
        .bind(request_id)
        .fetch_optional(&self.store.pool)
        .await?
        {
            let handoff = store::transfer_delivery::application_handoff(&self.store.pool, &uid)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(uid))?;
            if store::transfer_delivery::application_handoff_detail(&self.store.pool, &handoff.uid)
                .await?
                .is_some()
            {
                if let Some(delivery_uid) = store::sqlx::query_scalar::<_, String>(
                    "SELECT uid FROM transfer_delivery_policy
                     WHERE transfer_uid = ? AND recipient_person_uid = ?
                       AND recipient_organ_uid = ? AND state = 'active'",
                )
                .bind(&handoff.transfer_uid)
                .bind(&handoff.participant_person_uid)
                .bind(&handoff.participant_organ_uid)
                .fetch_optional(&self.store.pool)
                .await?
                {
                    self.enqueue_transfer_delivery(
                        &handoff.transfer_uid,
                        &delivery_uid,
                        &format!("remote-settlement-handoff:{}", handoff.uid),
                        now,
                    )
                    .await?;
                }
                return Ok(handoff);
            }
            Some(handoff)
        } else {
            None
        };
        let occurrence = store::sqlx::query_as::<
            _,
            (
                String,
                String,
                f64,
                Option<String>,
                String,
                String,
                i64,
                i64,
                i64,
                String,
                f64,
                String,
            ),
        >(
            "SELECT o.transfer_uid, o.promise_uid, o.quantity, o.unit_uid,
                    o.giver_person_uid, o.receiver_person_uid, o.delivery_claimed,
                    o.receipt_claimed, o.disputed, p.party_uid, p.delta, p.state
             FROM transfer_occurrence o JOIN promise p ON p.uid = o.promise_uid
             WHERE o.uid = ?",
        )
        .bind(occurrence_uid)
        .fetch_optional(&self.store.pool)
        .await?
        .ok_or_else(|| EngineError::UnknownRecord(occurrence_uid.into()))?;
        if occurrence.0 != transfer_uid
            || occurrence.9 != actor_person_uid
            || occurrence.11 != "active"
            || occurrence.6 == 0
            || occurrence.7 == 0
            || occurrence.8 != 0
            || !canonical_quantity.is_finite()
            || canonical_quantity <= 0.0
        {
            return Err(delivery_conflict(
                "transfer_remote_settlement_not_ready",
                "remote settlement requires the active source owner, both confirmations, and no dispute",
            ));
        }
        let participant_organ_uid: String = store::sqlx::query_scalar(
            "SELECT organ_uid FROM record WHERE uid = ? AND kind = 'person'",
        )
        .bind(actor_person_uid)
        .fetch_one(&self.store.pool)
        .await?;
        let delivery_uid: Option<String> = store::sqlx::query_scalar(
            "SELECT uid FROM transfer_delivery_policy
             WHERE transfer_uid = ? AND recipient_person_uid = ? AND recipient_organ_uid = ?
               AND state = 'active'",
        )
        .bind(transfer_uid)
        .bind(actor_person_uid)
        .bind(&participant_organ_uid)
        .fetch_optional(&self.store.pool)
        .await?;
        let delivery_uid = delivery_uid.ok_or_else(|| {
            delivery_conflict(
                "transfer_remote_settlement_delivery_missing",
                "participant requires an active cross-Cell delivery policy",
            )
        })?;
        if participant_organ_uid == origin_organ_uid {
            return Err(delivery_conflict(
                "transfer_remote_settlement_delivery_missing",
                "participant requires an active cross-Cell delivery policy",
            ));
        }
        let local_sum: f64 = store::sqlx::query_scalar(
            "SELECT COALESCE(SUM(canonical_quantity), 0.0)
             FROM transfer_occurrence_settlement_slice WHERE occurrence_uid = ?",
        )
        .bind(occurrence_uid)
        .fetch_one(&self.store.pool)
        .await?;
        let remote_sum: f64 = store::sqlx::query_scalar(
            "SELECT COALESCE(SUM(d.canonical_quantity), 0.0)
             FROM transfer_application_handoff h
             JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
             WHERE h.occurrence_uid = ? AND h.state IN ('pending', 'accepted')",
        )
        .bind(occurrence_uid)
        .fetch_one(&self.store.pool)
        .await?;
        let cumulative_before = local_sum + remote_sum;
        let remaining_before = (occurrence.2 - cumulative_before).max(0.0);
        if (remaining_before - expected_remaining).abs() > 1e-9
            || canonical_quantity > remaining_before
        {
            return Err(delivery_conflict(
                "transfer_settlement_remaining_stale",
                "reviewed remaining quantity is stale",
            ));
        }
        let cumulative_after = cumulative_before + canonical_quantity;
        let remaining_after = occurrence.2 - cumulative_after;
        let computed_slice_hash = hex_sha256(
            &serde_json::to_vec(&serde_json::json!({
                "transfer": transfer_uid,
                "occurrence": occurrence_uid,
                "promise": occurrence.1,
                "participant": actor_person_uid,
                "quantity": canonical_quantity,
                "unit": occurrence.3,
                "before": cumulative_before,
                "after": cumulative_after,
                "remaining": remaining_after,
                "application_direction": if occurrence.10 < 0.0 { -1 } else { 1 },
                "revision": expected_revision,
            }))
            .map_err(EngineError::Json)?,
        );
        let handoff = if let Some(handoff) = replay_handoff {
            if handoff.canonical_slice_hash != computed_slice_hash {
                return Err(delivery_conflict(
                    "transfer_remote_settlement_replay_conflict",
                    "settlement handoff request was replayed with changed canonical values",
                ));
            }
            handoff
        } else {
            let settlement_slice_uid = nucleus::new_uid("trss");
            store::transfer_delivery::create_application_handoff(
                &self.store.pool,
                store::transfer_delivery::NewApplicationHandoff {
                    origin_organ_uid: &origin_organ_uid,
                    participant_organ_uid: &participant_organ_uid,
                    participant_person_uid: actor_person_uid,
                    transfer_uid,
                    occurrence_uid,
                    settlement_slice_uid: &settlement_slice_uid,
                    origin_revision: expected_revision,
                    canonical_slice_hash: &computed_slice_hash,
                    request_id,
                    fact_uid: None,
                },
                now,
            )
            .await?
        };
        store::transfer_delivery::store_application_handoff_detail(
            &self.store.pool,
            &handoff.uid,
            &occurrence.1,
            canonical_quantity,
            occurrence.3.as_deref(),
            cumulative_before,
            cumulative_after,
            remaining_after,
            if occurrence.10 < 0.0 { -1 } else { 1 },
            None,
            now,
        )
        .await?;
        self.enqueue_transfer_delivery(
            transfer_uid,
            &delivery_uid,
            &format!("remote-settlement-handoff:{}", handoff.uid),
            now,
        )
        .await?;
        Ok(handoff)
    }

    pub async fn accept_transfer_application_attestation(
        &self,
        handoff_uid: &str,
        attestation: &TransferApplicationAttestationV1,
        request_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<store::transfer_delivery::ApplicationHandoffRow, EngineError> {
        attestation.validate_shape().map_err(|message| {
            delivery_conflict("transfer_application_attestation_invalid", message)
        })?;
        let handoff = store::sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                String,
                i64,
                String,
                String,
                Option<String>,
            ),
        >(
            "SELECT origin_organ_uid, participant_organ_uid, participant_person_uid,
                    transfer_uid, occurrence_uid, settlement_slice_uid, origin_revision,
                    canonical_slice_hash, state, attestation_uid
             FROM transfer_application_handoff WHERE uid = ?",
        )
        .bind(handoff_uid)
        .fetch_optional(&self.store.pool)
        .await?
        .ok_or_else(|| EngineError::UnknownRecord(handoff_uid.into()))?;
        let local = self.require_transfer_origin_authority(&handoff.3).await?;
        if handoff.8 == "accepted" && handoff.9.as_deref() == Some(&attestation.attestation_uid) {
            let stored: Option<String> = store::sqlx::query_scalar(
                "SELECT payload FROM transfer_application_attestation WHERE uid = ?",
            )
            .bind(&attestation.attestation_uid)
            .fetch_optional(&self.store.pool)
            .await?;
            if stored.as_deref()
                != Some(
                    serde_json::to_string(attestation)
                        .map_err(EngineError::Json)?
                        .as_str(),
                )
            {
                return Err(delivery_conflict(
                    "transfer_application_attestation_replay_conflict",
                    "accepted attestation was replayed with changed signed contents",
                ));
            }
            return store::transfer_delivery::application_handoff(&self.store.pool, handoff_uid)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(handoff_uid.into()));
        }
        if handoff.8 != "pending"
            || handoff.0 != local
            || attestation.origin_organ_uid != handoff.0
            || attestation.participant_organ_uid != handoff.1
            || attestation.participant_person_uid != handoff.2
            || attestation.transfer_uid != handoff.3
            || attestation.occurrence_uid != handoff.4
            || attestation.settlement_slice_uid != handoff.5
            || attestation.origin_revision != handoff.6 as u64
            || attestation.canonical_slice_hash != handoff.7
        {
            return Err(delivery_conflict(
                "transfer_application_attestation_binding_mismatch",
                "attestation does not match the pending canonical handoff",
            ));
        }
        let person = store::records::get(&self.store.pool, &attestation.participant_person_uid)
            .await?
            .ok_or_else(|| {
                EngineError::UnknownRecord(attestation.participant_person_uid.clone())
            })?;
        if person.kind != nucleus::RecordKind::Person.as_str()
            || person.organ_uid.as_deref() != Some(attestation.participant_organ_uid.as_str())
        {
            return Err(delivery_conflict(
                "transfer_application_attestation_actor_binding_invalid",
                "participant Person is not bound to the attesting Organ",
            ));
        }
        let contact = store::organs::contact(&self.store.pool, &attestation.participant_organ_uid)
            .await?
            .ok_or_else(|| {
                delivery_conflict(
                    "transfer_application_attestation_organ_unknown",
                    "participant Organ is unknown",
                )
            })?;
        if contact.trust == "blocked" || !contact.sync_in {
            return Err(EngineError::Forbidden(
                "participant Organ is blocked".into(),
            ));
        }
        let public_key = identity_public_key(
            self,
            &attestation.participant_person_uid,
            &attestation.key_id,
        )
        .await?;
        verify_signature(
            &public_key,
            &attestation.signature,
            &attestation.signing_bytes(),
        )
        .map_err(|_| {
            delivery_conflict(
                "transfer_application_attestation_signature_invalid",
                "application attestation Person signature is invalid",
            )
        })?;
        store::transfer_delivery::store_application_attestation(&self.store.pool, attestation, now)
            .await?;
        let organ_signer = self.organ_signer.lock().await.clone().ok_or_else(|| {
            delivery_conflict(
                "organ_signer_unavailable",
                "local Organ signer is unavailable",
            )
        })?;
        let canonical_fact = crate::append::append_one(
            &self.store,
            nucleus::NewFact {
                uid: Some(format!("taf:{}", attestation.attestation_uid)),
                record_uid: attestation.transfer_uid.clone(),
                delta: nucleus::fact::zero_delta(),
                at: None,
                actor_uid: Some(attestation.origin_organ_uid.clone()),
                cause: nucleus::Cause::settlement(attestation.settlement_slice_uid.clone()),
                payload: Some(
                    serde_json::json!({
                        "action": "accept-remote-transfer-application",
                        "handoff_uid": handoff_uid,
                        "attestation_uid": attestation.attestation_uid,
                        "participant_person_uid": attestation.participant_person_uid,
                        "occurrence_uid": attestation.occurrence_uid,
                        "settlement_slice_uid": attestation.settlement_slice_uid,
                        "origin_revision": attestation.origin_revision,
                        "canonical_slice_hash": attestation.canonical_slice_hash,
                        "formula_commitment": attestation.formula_commitment,
                        "formula_version": attestation.formula_version,
                    })
                    .to_string(),
                ),
            },
            now,
            Some(&organ_signer),
        )
        .await?;
        let fact_uid = if let Some(fact) = canonical_fact {
            let uid = fact.uid.clone();
            let _ = self.observe_committed_fact(fact, now).await?;
            uid
        } else {
            format!("taf:{}", attestation.attestation_uid)
        };
        store::sqlx::query(
            "UPDATE transfer_application_handoff_detail
             SET origin_acceptance_fact_uid = COALESCE(origin_acceptance_fact_uid, ?)
             WHERE handoff_uid = ?",
        )
        .bind(&fact_uid)
        .bind(handoff_uid)
        .execute(&self.store.pool)
        .await?;
        let accepted = store::transfer_delivery::transition_application_handoff(
            &self.store.pool,
            store::transfer_delivery::ApplicationHandoffTransition {
                handoff_uid,
                expected_state: TransferApplicationHandoffState::Pending,
                to_state: TransferApplicationHandoffState::Accepted,
                attestation_uid: Some(&attestation.attestation_uid),
                fact_uid: Some(&fact_uid),
                reason_code: None,
                request_id,
            },
            now,
        )
        .await?;
        if let Some(detail) =
            store::transfer_delivery::application_handoff_detail(&self.store.pool, handoff_uid)
                .await?
            && detail.canonical_remaining_after == 0.0
        {
            store::sqlx::query("UPDATE promise SET state = 'kept', updated_at = ? WHERE uid = ?")
                .bind(now.to_rfc3339())
                .bind(detail.source_promise_uid)
                .execute(&self.store.pool)
                .await?;
        }
        if let Some(delivery_uid) = store::sqlx::query_scalar::<_, String>(
            "SELECT uid FROM transfer_delivery_policy
             WHERE transfer_uid = ? AND recipient_person_uid = ? AND recipient_organ_uid = ?
               AND state = 'active'",
        )
        .bind(&accepted.transfer_uid)
        .bind(&accepted.participant_person_uid)
        .bind(&accepted.participant_organ_uid)
        .fetch_optional(&self.store.pool)
        .await?
        {
            self.enqueue_transfer_delivery(
                &accepted.transfer_uid,
                &delivery_uid,
                &format!("remote-settlement-accepted:{}", accepted.uid),
                now,
            )
            .await?;
        }
        Ok(accepted)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn queue_remote_transfer_action(
        &self,
        action: &crate::actions::Action,
        actor_person_uid: &str,
        key_id: &str,
        session_id: &str,
        session_challenge: &str,
        sequence: u64,
        message_id: &str,
        action_base64: &str,
        signature: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<crate::actions::ActionOutcome>, EngineError> {
        let Some(transfer_uid) = remote_action_transfer_token(action) else {
            return Ok(None);
        };
        if store::transfers::get(&self.store.pool, transfer_uid)
            .await?
            .is_some()
        {
            return Ok(None);
        }
        let Some((submitted_person, request_id, expected_revision)) = remote_action_binding(action)
        else {
            return Ok(None);
        };
        let submitted_person = store::records::resolve(&self.store.pool, submitted_person)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(submitted_person.into()))?;
        if submitted_person.uid != actor_person_uid {
            return Err(delivery_conflict(
                "transfer_remote_command_authorship_mismatch",
                "remote Action Person differs from its signed session",
            ));
        }
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
        let reference = store::sqlx::query_as::<_, (String, String)>(
            "SELECT origin_organ_uid, uid FROM transfer_remote_reference
             WHERE transfer_uid = ? AND recipient_person_uid = ? AND recipient_organ_uid = ?
               AND state = 'active'",
        )
        .bind(transfer_uid)
        .bind(actor_person_uid)
        .bind(&local.uid)
        .fetch_optional(&self.store.pool)
        .await?;
        let Some((origin_organ_uid, _reference_uid)) = reference else {
            return Ok(None);
        };
        let public_key = identity_public_key(self, actor_person_uid, key_id).await?;
        let command = TransferRemoteCommandV1 {
            version: TRANSFER_ENVELOPE_VERSION,
            command_uid: format!("trc:{session_id}:{message_id}"),
            request_id: request_id.into(),
            origin_organ_uid,
            sender_organ_uid: local.uid,
            transfer_uid: transfer_uid.into(),
            expected_revision,
            actor_person_uid: actor_person_uid.into(),
            key_id: key_id.into(),
            public_key_base64: public_key,
            session_id: session_id.into(),
            session_challenge: session_challenge.into(),
            sequence,
            message_id: message_id.into(),
            action_base64: action_base64.into(),
            created_at: now.to_rfc3339(),
            signature: signature.into(),
        };
        command
            .validate_shape()
            .map_err(|message| delivery_conflict("transfer_remote_command_invalid", message))?;
        let commit = store::transfer_delivery::persist_remote_command(
            &self.store.pool,
            "outgoing",
            &command,
            now,
        )
        .await?;
        let row = match commit {
            store::transfer_delivery::RemoteCommandCommit::Applied(row)
            | store::transfer_delivery::RemoteCommandCommit::Replayed(row) => row,
        };
        Ok(Some(crate::actions::ActionOutcome {
            created: Some(row.command_uid),
            facts: Vec::new(),
            warnings: Vec::new(),
        }))
    }
}

pub(crate) fn remote_action_binding(
    action: &crate::actions::Action,
) -> Option<(&str, &str, Option<u64>)> {
    use crate::actions::Action;
    match action {
        Action::ConfigureTransferDelivery {
            request_id,
            person: Some(person),
            ..
        }
        | Action::EnqueueTransferDelivery {
            request_id,
            person: Some(person),
            ..
        }
        | Action::RetryTransferDelivery {
            request_id,
            person: Some(person),
            ..
        }
        | Action::CreateTransferThread {
            request_id, person, ..
        }
        | Action::CreateTransferMessage {
            request_id, person, ..
        } => Some((person, request_id, None)),
        Action::BeginRemoteTransferSettlement {
            expected_revision,
            request_id,
            person,
            ..
        } => Some((person, request_id, Some(*expected_revision))),
        Action::AcceptTransferInvitation {
            expected_revision,
            request_id,
            person: Some(person),
            transfer: Some(_),
            ..
        } => Some((person, request_id, Some(*expected_revision))),
        Action::RejectTransferInvitation {
            request_id,
            person: Some(person),
            transfer: Some(_),
            ..
        } => Some((person, request_id, None)),
        Action::ReopenTransferPromise {
            expected_revision,
            request_id,
            person: Some(person),
            ..
        }
        | Action::CounterofferTransfer {
            expected_revision,
            request_id,
            person: Some(person),
            ..
        }
        | Action::ClaimOpenTransferPromise {
            expected_revision,
            request_id,
            person: Some(person),
            ..
        }
        | Action::SetTransferAgreementLevel {
            expected_revision,
            request_id,
            person: Some(person),
            ..
        }
        | Action::ActivateTransferOccurrence {
            expected_revision,
            request_id,
            person: Some(person),
            ..
        }
        | Action::SetTransferDeliveryMode {
            expected_revision,
            request_id,
            person: Some(person),
            ..
        }
        | Action::RevokeTransferDelivery {
            expected_revision,
            request_id,
            person: Some(person),
            ..
        } => Some((person, request_id, Some(*expected_revision))),
        Action::SetTransferOccurrenceClaim {
            request_id,
            person: Some(person),
            ..
        }
        | Action::CompleteTransferOccurrenceClaimsBulk {
            request_id,
            person: Some(person),
            ..
        }
        | Action::SetTransferOccurrenceDispute {
            request_id,
            person: Some(person),
            ..
        } => Some((person, request_id, None)),
        _ => None,
    }
}

fn remote_action_transfer_token(action: &crate::actions::Action) -> Option<&str> {
    use crate::actions::Action;
    match action {
        Action::ReopenTransferPromise { transfer, .. }
        | Action::CounterofferTransfer { transfer, .. }
        | Action::ClaimOpenTransferPromise { transfer, .. }
        | Action::SetTransferAgreementLevel { transfer, .. }
        | Action::ActivateTransferOccurrence { transfer, .. }
        | Action::ConfigureTransferDelivery { transfer, .. }
        | Action::SetTransferDeliveryMode { transfer, .. }
        | Action::EnqueueTransferDelivery { transfer, .. }
        | Action::RetryTransferDelivery { transfer, .. }
        | Action::RevokeTransferDelivery { transfer, .. }
        | Action::CreateTransferThread { transfer, .. }
        | Action::CreateTransferMessage { transfer, .. } => Some(transfer),
        Action::BeginRemoteTransferSettlement { transfer, .. } => Some(transfer),
        Action::AcceptTransferInvitation {
            transfer: Some(transfer),
            ..
        }
        | Action::RejectTransferInvitation {
            transfer: Some(transfer),
            ..
        } => Some(transfer),
        _ => None,
    }
}

async fn identity_public_key(
    engine: &Engine,
    actor_uid: &str,
    key_id: &str,
) -> Result<String, EngineError> {
    store::sqlx::query_scalar(
        "SELECT public_key FROM identity_key WHERE actor_uid = ? AND key_id = ?",
    )
    .bind(actor_uid)
    .bind(key_id)
    .fetch_optional(&engine.store.pool)
    .await?
    .ok_or_else(|| {
        delivery_conflict(
            "identity_key_unknown",
            "signing key is not published locally",
        )
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn require_kind(
    engine: &Engine,
    uid: &str,
    kind: nucleus::RecordKind,
) -> Result<(), EngineError> {
    let row = store::records::get(&engine.store.pool, uid)
        .await?
        .ok_or_else(|| EngineError::UnknownRecord(uid.into()))?;
    if row.kind != kind.as_str() {
        return Err(EngineError::Consequence(format!(
            "delivery identity `{uid}` is not a {}",
            kind.as_str()
        )));
    }
    Ok(())
}

fn policy_from_commit(
    commit: store::transfer_delivery::PolicyCommit,
) -> store::transfer_delivery::DeliveryPolicyRow {
    match commit {
        store::transfer_delivery::PolicyCommit::Applied(row)
        | store::transfer_delivery::PolicyCommit::Replayed(row) => row,
    }
}

fn verify_envelope_person_proof(envelope: &TransferEnvelopeV1) -> Result<(), EngineError> {
    let mut keys = HashMap::<(&str, &str), &str>::new();
    for key in &envelope.actor_keys {
        match keys.insert(
            (key.actor_person_uid.as_str(), key.key_id.as_str()),
            key.public_key_base64.as_str(),
        ) {
            Some(existing) if existing != key.public_key_base64 => {
                return Err(delivery_conflict(
                    "transfer_envelope_actor_key_conflict",
                    "Transfer envelope repeats an actor key id with different material",
                ));
            }
            _ => {}
        }
    }
    let intents = envelope
        .action_intents
        .iter()
        .flat_map(|intent| {
            intent
                .fact_uids
                .iter()
                .map(move |fact_uid| (fact_uid.as_str(), intent))
        })
        .collect::<HashMap<_, _>>();
    for fact in &envelope.facts {
        if fact.record_uid != envelope.transfer_uid
            || !nucleus::fact::verify_chain_step(fact)
            || !envelope
                .disclosure
                .iter()
                .any(|entry| entry.path == format!("facts.{}", fact.uid))
        {
            return Err(delivery_conflict(
                "transfer_envelope_fact_invalid",
                "Transfer envelope contains an undisclosed, foreign, or malformed Fact",
            ));
        }
        let Some(actor) = fact.actor_uid.as_deref() else {
            if fact.signature.is_some() {
                return Err(delivery_conflict(
                    "transfer_envelope_fact_proof_invalid",
                    "an actorless Transfer Fact cannot carry a Person signature",
                ));
            }
            continue;
        };
        let direct_valid = fact.signature.as_deref().is_some_and(|signature| {
            keys.iter().any(|((key_actor, _), public_key)| {
                *key_actor == actor
                    && verify_signature(public_key, signature, fact.hash.as_bytes()).is_ok()
            })
        });
        let intent_valid = intents.get(fact.uid.as_str()).is_some_and(|intent| {
            intent.actor_person_uid == actor
                && keys
                    .get(&(actor, intent.key_id.as_str()))
                    .is_some_and(|public_key| {
                        let bytes = nucleus::action_intent::signing_bytes(
                            &intent.session_id,
                            &intent.session_challenge,
                            intent.sequence,
                            &intent.message_id,
                            &intent.action_base64,
                        );
                        verify_signature(public_key, &intent.signature, &bytes).is_ok()
                    })
        });
        if !direct_valid && !intent_valid {
            return Err(delivery_conflict(
                "transfer_envelope_fact_proof_invalid",
                "Transfer envelope Fact has no valid Person signature or Action intent",
            ));
        }
    }
    Ok(())
}

fn verify_signature(public_key: &str, signature: &str, bytes: &[u8]) -> Result<(), ()> {
    let public_key = B64.decode(public_key).map_err(|_| ())?;
    let public_key: [u8; 32] = public_key.try_into().map_err(|_| ())?;
    let public_key = VerifyingKey::from_bytes(&public_key).map_err(|_| ())?;
    let signature = B64.decode(signature).map_err(|_| ())?;
    let signature = Signature::from_slice(&signature).map_err(|_| ())?;
    public_key.verify(bytes, &signature).map_err(|_| ())
}

fn delivery_conflict(code: &'static str, message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code,
        message: message.into(),
    }
}
