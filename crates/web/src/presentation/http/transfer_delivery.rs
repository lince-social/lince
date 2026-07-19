//! Recipient-specific Transfer HTTP transport.
//!
//! This boundary deliberately does not accept `engine::sync::Package`. Every
//! body is bound to an Organ signature and Transfer replicas stay in their
//! isolated read model.

use std::time::Duration;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use chrono::{DateTime, Utc};
use nucleus::transfer_delivery::{
    SignedOrganRequestV1, TransferApplicationAttestationV1, TransferDeliveryMode,
    TransferDeliveryPolicyEventV1, TransferEnvelopeV1, TransferRemoteCommandV1,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;

use crate::CellApiState;

const ENVELOPE_PATH: &str = "/organ/transfers/envelopes";
const PULL_PATH: &str = "/organ/transfers/pull";
const PULL_RESULT_PATH: &str = "/organ/transfers/pull-result";
const RECEIPT_PATH: &str = "/organ/transfers/receipts";
const COMMAND_PATH: &str = "/organ/transfers/commands";
const COMMAND_RESULT_PATH: &str = "/organ/transfers/command-results";
const POLICY_PATH: &str = "/organ/transfers/policy-events";
const ATTESTATION_PATH: &str = "/organ/transfers/application-attestations";

type HttpError = (StatusCode, String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Authenticated<T> {
    auth: SignedOrganRequestV1,
    body: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PullRequest {
    transfer_uid: String,
    recipient_person_uid: String,
    after_cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PackageReceipt {
    request_id: String,
    envelope_uid: String,
    transfer_uid: String,
    recipient_person_uid: String,
    cursor: u64,
    kind: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CommandResult {
    command_uid: String,
    accepted: bool,
    authoritative_revision: u64,
    #[serde(default)]
    fact_refs: Vec<FactRef>,
    #[serde(default)]
    warnings: Vec<String>,
    created: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FactRef {
    uid: String,
    hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PullResult {
    policy: TransferDeliveryPolicyEventV1,
    envelope: Option<TransferEnvelopeV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeliveryPush {
    policy: TransferDeliveryPolicyEventV1,
    envelope: TransferEnvelopeV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApplicationAttestationRequest {
    handoff_uid: String,
    request_id: String,
    attestation: TransferApplicationAttestationV1,
}

pub(crate) async fn receive_envelope(
    State(state): State<CellApiState>,
    Json(wire): Json<Authenticated<DeliveryPush>>,
) -> Result<impl IntoResponse, HttpError> {
    let now = Utc::now();
    verify_wire(&state, &wire, ENVELOPE_PATH, now).await?;
    if wire.auth.sender_organ_uid != wire.body.envelope.origin_organ_uid
        || wire.auth.recipient_organ_uid != wire.body.envelope.recipient_organ_uid
        || wire.body.policy.origin_organ_uid != wire.body.envelope.origin_organ_uid
        || wire.body.policy.recipient_organ_uid != wire.body.envelope.recipient_organ_uid
        || wire.body.policy.delivery_policy_uid != wire.body.envelope.delivery_policy_uid
        || wire.body.policy.policy_revision != wire.body.envelope.delivery_policy_revision
    {
        return Err(forbidden(
            "Organ request and Transfer envelope identities differ",
        ));
    }
    accept_policy_event(&state, &wire.body.policy, now).await?;
    accept_envelope(&state, &wire.body.envelope, now).await?;

    let receipt = PackageReceipt {
        request_id: format!(
            "transfer-package-received:{}:{}",
            wire.body.envelope.recipient_organ_uid, wire.body.envelope.envelope_uid
        ),
        envelope_uid: wire.body.envelope.envelope_uid.clone(),
        transfer_uid: wire.body.envelope.transfer_uid.clone(),
        recipient_person_uid: wire.body.envelope.recipient_person_uid.clone(),
        cursor: wire.body.envelope.cursor,
        kind: "received".into(),
        // Stable across an exact envelope retry, so the signed receipt proof is stable too.
        created_at: wire.body.envelope.created_at.clone(),
    };
    signed_response(
        &state,
        RECEIPT_PATH,
        &wire.body.envelope.origin_organ_uid,
        receipt,
        now,
    )
    .await
}

pub(crate) async fn pull_envelope(
    State(state): State<CellApiState>,
    Json(wire): Json<Authenticated<PullRequest>>,
) -> Result<impl IntoResponse, HttpError> {
    let now = Utc::now();
    verify_wire(&state, &wire, PULL_PATH, now).await?;
    let delivery: Option<String> = store::sqlx::query_scalar(
        "SELECT uid FROM transfer_delivery_policy
         WHERE transfer_uid = ? AND recipient_person_uid = ? AND recipient_organ_uid = ?
           AND origin_organ_uid = ?",
    )
    .bind(&wire.body.transfer_uid)
    .bind(&wire.body.recipient_person_uid)
    .bind(&wire.auth.sender_organ_uid)
    .bind(&wire.auth.recipient_organ_uid)
    .fetch_optional(&state.store.pool)
    .await
    .map_err(internal)?;
    let delivery_uid = delivery.ok_or_else(|| {
        (
            StatusCode::GONE,
            "Transfer delivery is absent or revoked".into(),
        )
    })?;

    let policy = store::transfer_delivery::policy(&state.store.pool, &delivery_uid)
        .await
        .map_err(internal)?
        .ok_or_else(|| (StatusCode::GONE, "Transfer delivery is absent".into()))?;
    let policy_kind = if policy.state == "revoked" {
        "revoked"
    } else if policy.revision == 1 {
        "reference"
    } else {
        "mode_changed"
    };
    let policy_event = state
        .engine
        .sign_transfer_delivery_policy_event(&policy, policy_kind, now)
        .await
        .map_err(engine_error)?;
    let mut payload: Option<String> = if policy.state == "active" {
        store::sqlx::query_scalar(
            "SELECT payload FROM transfer_delivery_outbox
         WHERE delivery_uid = ? AND cursor > ? AND status != 'cancelled'
         ORDER BY cursor DESC LIMIT 1",
        )
        .bind(&delivery_uid)
        .bind(wire.body.after_cursor as i64)
        .fetch_optional(&state.store.pool)
        .await
        .map_err(internal)?
    } else {
        None
    };
    if payload.is_none() && policy.state == "active" {
        let transfer_revision: i64 =
            store::sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid = ?")
                .bind(&wire.body.transfer_uid)
                .fetch_one(&state.store.pool)
                .await
                .map_err(internal)?;
        let request_id = format!(
            "transfer-pull:{}:{}:{}",
            delivery_uid, policy.revision, transfer_revision
        );
        let _ = state
            .engine
            .enqueue_transfer_delivery(&wire.body.transfer_uid, &delivery_uid, &request_id, now)
            .await
            .map_err(engine_error)?;
        payload = store::sqlx::query_scalar(
            "SELECT payload FROM transfer_delivery_outbox
             WHERE delivery_uid = ? AND cursor > ? AND status != 'cancelled'
             ORDER BY cursor DESC LIMIT 1",
        )
        .bind(&delivery_uid)
        .bind(wire.body.after_cursor as i64)
        .fetch_optional(&state.store.pool)
        .await
        .map_err(internal)?;
    }
    let envelope = payload
        .map(|payload| serde_json::from_str::<TransferEnvelopeV1>(&payload))
        .transpose()
        .map_err(|error| internal(error.to_string()))?;
    signed_response(
        &state,
        PULL_RESULT_PATH,
        &wire.auth.sender_organ_uid,
        PullResult {
            policy: policy_event,
            envelope,
        },
        now,
    )
    .await
}

pub(crate) async fn receive_policy_event(
    State(state): State<CellApiState>,
    Json(wire): Json<Authenticated<TransferDeliveryPolicyEventV1>>,
) -> Result<impl IntoResponse, HttpError> {
    let now = Utc::now();
    verify_wire(&state, &wire, POLICY_PATH, now).await?;
    if wire.auth.sender_organ_uid != wire.body.origin_organ_uid
        || wire.auth.recipient_organ_uid != wire.body.recipient_organ_uid
    {
        return Err(forbidden(
            "Organ request and Transfer policy identities differ",
        ));
    }
    let reference = accept_policy_event(&state, &wire.body, now).await?;
    Ok(Json(json!({
        "reference_uid": reference.uid,
        "policy_revision": reference.policy_revision,
        "state": reference.state,
    })))
}

pub(crate) async fn receive_application_attestation(
    State(state): State<CellApiState>,
    Json(wire): Json<Authenticated<ApplicationAttestationRequest>>,
) -> Result<impl IntoResponse, HttpError> {
    let now = Utc::now();
    verify_wire(&state, &wire, ATTESTATION_PATH, now).await?;
    if wire.auth.sender_organ_uid != wire.body.attestation.participant_organ_uid
        || wire.auth.recipient_organ_uid != wire.body.attestation.origin_organ_uid
    {
        return Err(forbidden(
            "Organ request and application attestation identities differ",
        ));
    }
    let handoff = state
        .engine
        .accept_transfer_application_attestation(
            &wire.body.handoff_uid,
            &wire.body.attestation,
            &wire.body.request_id,
            now,
        )
        .await
        .map_err(engine_error)?;
    Ok(Json(json!({
        "handoff_uid": handoff.uid,
        "state": handoff.state.as_str(),
        "attestation_uid": handoff.attestation_uid,
    })))
}

pub(crate) async fn receive_receipt(
    State(state): State<CellApiState>,
    Json(wire): Json<Authenticated<PackageReceipt>>,
) -> Result<impl IntoResponse, HttpError> {
    let now = Utc::now();
    verify_wire(&state, &wire, RECEIPT_PATH, now).await?;
    if wire.body.kind != "received" && wire.body.kind != "seen" {
        return Err(bad_request("Unknown package receipt kind"));
    }
    let delivery_uid: Option<String> = store::sqlx::query_scalar(
        "SELECT uid FROM transfer_delivery_policy
         WHERE transfer_uid = ? AND recipient_person_uid = ? AND recipient_organ_uid = ?
           AND origin_organ_uid = ?",
    )
    .bind(&wire.body.transfer_uid)
    .bind(&wire.body.recipient_person_uid)
    .bind(&wire.auth.sender_organ_uid)
    .bind(&wire.auth.recipient_organ_uid)
    .fetch_optional(&state.store.pool)
    .await
    .map_err(internal)?;
    let delivery_uid = delivery_uid.ok_or_else(|| forbidden("Receipt has no delivery policy"))?;
    let signed_payload = serde_json::to_value(&wire.body).map_err(internal)?;
    let uid = store::transfer_delivery::record_package_receipt(
        &state.store.pool,
        store::transfer_delivery::NewPackageReceipt {
            delivery_uid: &delivery_uid,
            envelope_uid: &wire.body.envelope_uid,
            cursor: wire.body.cursor,
            kind: &wire.body.kind,
            actor_organ_uid: &wire.auth.sender_organ_uid,
            payload_hash: &wire.auth.body_hash,
            key_id: &wire.auth.key_id,
            signature: &wire.auth.signature,
            signed_payload: &signed_payload,
            local_fact_uid: None,
            request_id: &wire.body.request_id,
        },
        now,
    )
    .await
    .map_err(internal)?;
    Ok(Json(json!({ "receipt_uid": uid })))
}

pub(crate) async fn receive_command(
    State(state): State<CellApiState>,
    Json(wire): Json<Authenticated<TransferRemoteCommandV1>>,
) -> Result<impl IntoResponse, HttpError> {
    let now = Utc::now();
    verify_wire(&state, &wire, COMMAND_PATH, now).await?;
    if wire.auth.sender_organ_uid != wire.body.sender_organ_uid
        || wire.auth.recipient_organ_uid != wire.body.origin_organ_uid
    {
        return Err(forbidden(
            "Organ request and remote command identities differ",
        ));
    }
    let accepted = state
        .engine
        .accept_transfer_remote_command(&wire.body, now)
        .await;
    let mut value = match accepted {
        Ok(value) => value,
        Err(error) => store::sqlx::query_scalar::<_, String>(
            "SELECT result_payload FROM transfer_remote_command WHERE command_uid = ?",
        )
        .bind(&wire.body.command_uid)
        .fetch_optional(&state.store.pool)
        .await
        .map_err(internal)?
        .and_then(|payload| serde_json::from_str(&payload).ok())
        .ok_or_else(|| engine_error(error))?,
    };
    value
        .as_object_mut()
        .ok_or_else(|| internal("remote command result is not an object"))?
        .insert(
            "command_uid".into(),
            serde_json::Value::String(wire.body.command_uid.clone()),
        );
    let result: CommandResult = serde_json::from_value(value).map_err(internal)?;
    signed_response(
        &state,
        COMMAND_RESULT_PATH,
        &wire.body.sender_organ_uid,
        result,
        now,
    )
    .await
}

pub(crate) fn spawn_worker(state: CellApiState) {
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                tracing::error!(%error, "cannot start Transfer delivery worker");
                return;
            }
        };
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = enqueue_periodic_pulls(&state).await {
                tracing::warn!(%error, "cannot schedule periodic Transfer pulls");
            }
            if let Err(error) = drain_envelopes(&state, &client).await {
                tracing::warn!(%error, "Transfer envelope drain failed");
            }
            if let Err(error) = drain_commands(&state, &client).await {
                tracing::warn!(%error, "Transfer command drain failed");
            }
            if let Err(error) = drain_pulls(&state, &client).await {
                tracing::warn!(%error, "Transfer pull drain failed");
            }
        }
    });
}

async fn enqueue_periodic_pulls(state: &CellApiState) -> Result<(), String> {
    let reference_uids: Vec<String> = store::sqlx::query_scalar(
        "SELECT uid FROM transfer_remote_reference WHERE state = 'active' ORDER BY uid",
    )
    .fetch_all(&state.store.pool)
    .await
    .map_err(|error| error.to_string())?;
    let minute = Utc::now().timestamp() / 60;
    for reference_uid in reference_uids {
        store::transfer_delivery::enqueue_pull(
            &state.store.pool,
            &reference_uid,
            &format!("periodic-transfer-pull:{reference_uid}:{minute}"),
            Utc::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn drain_envelopes(state: &CellApiState, client: &reqwest::Client) -> Result<(), String> {
    for row in store::transfer_delivery::outbox_due(&state.store.pool, Utc::now(), 32)
        .await
        .map_err(|error| error.to_string())?
    {
        let attempt = push_envelope(state, client, &row).await;
        match attempt {
            Ok(cursor) => store::transfer_delivery::outbox_mark_sent(
                &state.store.pool,
                &row.uid,
                cursor,
                Utc::now(),
            )
            .await
            .map_err(|error| error.to_string())?,
            Err(error) => {
                store::transfer_delivery::outbox_mark_failed(
                    &state.store.pool,
                    &row.uid,
                    Utc::now(),
                    &error,
                    5,
                    3_600,
                )
                .await
                .map_err(|store_error| store_error.to_string())?;
            }
        }
    }
    Ok(())
}

async fn push_envelope(
    state: &CellApiState,
    client: &reqwest::Client,
    row: &store::transfer_delivery::DeliveryOutboxRow,
) -> Result<u64, String> {
    let policy = store::transfer_delivery::policy(&state.store.pool, &row.delivery_uid)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "delivery policy disappeared".to_string())?;
    if policy.state != "active" {
        return Err("delivery policy is revoked".into());
    }
    let contact = store::organs::contact(&state.store.pool, &policy.recipient_organ_uid)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "recipient Organ is not introduced".to_string())?;
    if contact.trust == "blocked" || !contact.sync_out {
        return Err("recipient Organ is blocked or outgoing delivery is disabled".into());
    }
    let envelope: TransferEnvelopeV1 = serde_json::from_str(&row.payload)
        .map_err(|error| format!("invalid queued envelope: {error}"))?;
    if envelope.recipient_organ_uid != contact.record_uid || envelope.mode != policy.mode {
        return Err("queued envelope no longer matches delivery policy".into());
    }
    let policy_kind = if policy.revision == 1 {
        "reference"
    } else {
        "mode_changed"
    };
    let policy_event = state
        .engine
        .sign_transfer_delivery_policy_event(&policy, policy_kind, Utc::now())
        .await
        .map_err(|error| error.to_string())?;
    let body = DeliveryPush {
        policy: policy_event,
        envelope,
    };
    let auth = state
        .engine
        .sign_organ_request(
            "POST",
            ENVELOPE_PATH,
            &body_bytes(&body).map_err(|error| error.to_string())?,
            &contact.record_uid,
            Utc::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!(
            "{}{ENVELOPE_PATH}",
            contact.base_url.trim_end_matches('/')
        ))
        .json(&Authenticated { auth, body })
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("recipient returned HTTP {}", response.status()));
    }
    let receipt: Authenticated<PackageReceipt> = response
        .json()
        .await
        .map_err(|error| format!("invalid recipient receipt: {error}"))?;
    verify_wire(state, &receipt, RECEIPT_PATH, Utc::now())
        .await
        .map_err(|(_, error)| error)?;
    if receipt.auth.sender_organ_uid != policy.recipient_organ_uid
        || receipt.body.envelope_uid != row.envelope_uid
        || receipt.body.cursor != row.cursor
        || receipt.body.kind != "received"
    {
        return Err("recipient receipt does not acknowledge the queued envelope".into());
    }
    let signed_payload = serde_json::to_value(&receipt.body).map_err(|error| error.to_string())?;
    store::transfer_delivery::record_package_receipt(
        &state.store.pool,
        store::transfer_delivery::NewPackageReceipt {
            delivery_uid: &policy.uid,
            envelope_uid: &receipt.body.envelope_uid,
            cursor: receipt.body.cursor,
            kind: &receipt.body.kind,
            actor_organ_uid: &receipt.auth.sender_organ_uid,
            payload_hash: &receipt.auth.body_hash,
            key_id: &receipt.auth.key_id,
            signature: &receipt.auth.signature,
            signed_payload: &signed_payload,
            local_fact_uid: None,
            request_id: &receipt.body.request_id,
        },
        Utc::now(),
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(receipt.body.cursor)
}

async fn drain_commands(state: &CellApiState, client: &reqwest::Client) -> Result<(), String> {
    for row in store::transfer_delivery::remote_commands_due(&state.store.pool, Utc::now(), 16)
        .await
        .map_err(|error| error.to_string())?
    {
        let result = push_command(state, client, &row).await;
        match result {
            Ok(result) => {
                let value = serde_json::to_value(&result).map_err(|error| error.to_string())?;
                if !result.accepted {
                    let cursor: i64 = store::sqlx::query_scalar(
                        "SELECT COALESCE(MAX(last_cursor), 0) FROM transfer_remote_reference
                         WHERE origin_organ_uid = ? AND transfer_uid = ?
                           AND recipient_person_uid = ?",
                    )
                    .bind(&row.origin_organ_uid)
                    .bind(&row.transfer_uid)
                    .bind(&row.actor_person_uid)
                    .fetch_one(&state.store.pool)
                    .await
                    .map_err(|error| error.to_string())?;
                    let reviewed = json!({
                        "command": serde_json::from_str::<serde_json::Value>(&row.payload)
                            .unwrap_or_else(|_| json!({ "payload_hash": row.payload_hash })),
                        "result": value.clone(),
                    });
                    store::transfer_delivery::record_remote_conflict(
                        &state.store.pool,
                        store::transfer_delivery::NewRemoteConflict {
                            origin_organ_uid: &row.origin_organ_uid,
                            transfer_uid: &row.transfer_uid,
                            recipient_person_uid: &row.actor_person_uid,
                            command_uid: Some(&row.command_uid),
                            request_id: Some(&row.request_id),
                            envelope_uid: None,
                            submitted_revision: row.expected_revision,
                            authoritative_revision: result.authoritative_revision,
                            authoritative_cursor: cursor as u64,
                            code: result.code.as_deref().unwrap_or("remote_action_rejected"),
                            reviewed_payload: &reviewed,
                        },
                        Utc::now(),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                }
                store::transfer_delivery::finish_remote_command(
                    &state.store.pool,
                    &row.command_uid,
                    result.accepted,
                    result.authoritative_revision,
                    &value,
                    result.code.as_deref(),
                    result.message.as_deref(),
                    Utc::now(),
                )
                .await
                .map_err(|error| error.to_string())?;
            }
            Err(error) => {
                store::transfer_delivery::remote_command_mark_failed(
                    &state.store.pool,
                    &row.command_uid,
                    Utc::now(),
                    "transport_failed",
                    &error,
                    5,
                    3_600,
                )
                .await
                .map_err(|store_error| store_error.to_string())?;
            }
        }
    }
    Ok(())
}

async fn drain_pulls(state: &CellApiState, client: &reqwest::Client) -> Result<(), String> {
    for row in store::transfer_delivery::pulls_due(&state.store.pool, Utc::now(), 16)
        .await
        .map_err(|error| error.to_string())?
    {
        let attempt = pull_reference(state, client, &row).await;
        match attempt {
            Ok(cursor) => store::transfer_delivery::pull_mark_completed(
                &state.store.pool,
                &row.uid,
                cursor,
                Utc::now(),
            )
            .await
            .map_err(|error| error.to_string())?,
            Err(error) => {
                store::transfer_delivery::pull_mark_failed(
                    &state.store.pool,
                    &row.uid,
                    Utc::now(),
                    &error,
                    5,
                    3_600,
                )
                .await
                .map_err(|store_error| store_error.to_string())?;
            }
        }
    }
    Ok(())
}

async fn pull_reference(
    state: &CellApiState,
    client: &reqwest::Client,
    row: &store::transfer_delivery::PullRequestRow,
) -> Result<u64, String> {
    let reference: Option<(String, String, String, String, Option<String>)> =
        store::sqlx::query_as(
            "SELECT origin_organ_uid, transfer_uid, recipient_person_uid,
                    recipient_organ_uid, hosted_url
             FROM transfer_remote_reference WHERE uid = ? AND state = 'active'",
        )
        .bind(&row.reference_uid)
        .fetch_optional(&state.store.pool)
        .await
        .map_err(|error| error.to_string())?;
    let (origin, transfer, person, local_organ, hosted_url) =
        reference.ok_or_else(|| "remote Transfer reference is absent or revoked".to_string())?;
    let contact = store::organs::contact(&state.store.pool, &origin)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "origin Organ is not introduced".to_string())?;
    if contact.trust == "blocked" || !contact.sync_out {
        return Err("origin Organ is blocked or outgoing delivery is disabled".into());
    }
    let request = PullRequest {
        transfer_uid: transfer,
        recipient_person_uid: person,
        after_cursor: row.after_cursor,
    };
    let auth = state
        .engine
        .sign_organ_request(
            "POST",
            PULL_PATH,
            &body_bytes(&request).map_err(|error| error.to_string())?,
            &origin,
            Utc::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!(
            "{}{PULL_PATH}",
            hosted_url
                .as_deref()
                .unwrap_or(&contact.base_url)
                .trim_end_matches('/')
        ))
        .json(&Authenticated {
            auth,
            body: request,
        })
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("origin returned HTTP {}", response.status()));
    }
    let wire: Authenticated<PullResult> = response
        .json()
        .await
        .map_err(|error| format!("invalid signed pull result: {error}"))?;
    verify_wire(state, &wire, PULL_RESULT_PATH, Utc::now())
        .await
        .map_err(|(_, error)| error)?;
    if wire.auth.sender_organ_uid != origin || wire.auth.recipient_organ_uid != local_organ {
        return Err("pull result Organ identity mismatch".into());
    }
    accept_policy_event(state, &wire.body.policy, Utc::now())
        .await
        .map_err(|(_, error)| error)?;
    let Some(envelope) = wire.body.envelope else {
        return Ok(row.after_cursor);
    };
    accept_envelope(state, &envelope, Utc::now())
        .await
        .map_err(|(_, error)| error)?;
    send_received_receipt(state, client, &contact.base_url, &envelope).await?;
    Ok(envelope.cursor)
}

async fn send_received_receipt(
    state: &CellApiState,
    client: &reqwest::Client,
    origin_base_url: &str,
    envelope: &TransferEnvelopeV1,
) -> Result<(), String> {
    let receipt = PackageReceipt {
        request_id: format!(
            "transfer-package-received:{}:{}",
            envelope.recipient_organ_uid, envelope.envelope_uid
        ),
        envelope_uid: envelope.envelope_uid.clone(),
        transfer_uid: envelope.transfer_uid.clone(),
        recipient_person_uid: envelope.recipient_person_uid.clone(),
        cursor: envelope.cursor,
        kind: "received".into(),
        created_at: envelope.created_at.clone(),
    };
    let auth = state
        .engine
        .sign_organ_request(
            "POST",
            RECEIPT_PATH,
            &body_bytes(&receipt).map_err(|error| error.to_string())?,
            &envelope.origin_organ_uid,
            Utc::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!(
            "{}{RECEIPT_PATH}",
            origin_base_url.trim_end_matches('/')
        ))
        .json(&Authenticated {
            auth,
            body: receipt,
        })
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "origin rejected package receipt with HTTP {}",
            response.status()
        ))
    }
}

async fn push_command(
    state: &CellApiState,
    client: &reqwest::Client,
    row: &store::transfer_delivery::RemoteCommandRow,
) -> Result<CommandResult, String> {
    let contact = store::organs::contact(&state.store.pool, &row.origin_organ_uid)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "origin Organ is not introduced".to_string())?;
    if contact.trust == "blocked" || !contact.sync_out {
        return Err("origin Organ is blocked or outgoing delivery is disabled".into());
    }
    let command: TransferRemoteCommandV1 = serde_json::from_str(&row.payload)
        .map_err(|error| format!("invalid queued remote command: {error}"))?;
    let auth = state
        .engine
        .sign_organ_request(
            "POST",
            COMMAND_PATH,
            &body_bytes(&command).map_err(|error| error.to_string())?,
            &row.origin_organ_uid,
            Utc::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!(
            "{}{COMMAND_PATH}",
            contact.base_url.trim_end_matches('/')
        ))
        .json(&Authenticated {
            auth,
            body: command,
        })
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("origin returned HTTP {}", response.status()));
    }
    let wire: Authenticated<CommandResult> = response
        .json()
        .await
        .map_err(|error| format!("invalid origin command result: {error}"))?;
    verify_wire(state, &wire, COMMAND_RESULT_PATH, Utc::now())
        .await
        .map_err(|(_, error)| error)?;
    if wire.auth.sender_organ_uid != row.origin_organ_uid
        || wire.body.command_uid != row.command_uid
    {
        return Err("origin command result identity mismatch".into());
    }
    Ok(wire.body)
}

async fn accept_envelope(
    state: &CellApiState,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
) -> Result<(), HttpError> {
    state
        .engine
        .verify_transfer_envelope(envelope)
        .await
        .map_err(engine_error)?;
    let reference = store::transfer_delivery::remote_reference_by_identity(
        &state.store.pool,
        &envelope.origin_organ_uid,
        &envelope.transfer_uid,
        &envelope.recipient_person_uid,
        &envelope.recipient_organ_uid,
    )
    .await
    .map_err(internal)?;
    if reference.as_ref().is_some_and(|row| row.state == "revoked") {
        return Err((StatusCode::GONE, "Transfer delivery was revoked".into()));
    }
    let reference = reference.ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            "Transfer envelope arrived without its signed delivery policy".into(),
        )
    })?;
    if reference.mode != envelope.mode {
        return Err((
            StatusCode::CONFLICT,
            "Envelope mode differs from the last signed delivery policy".into(),
        ));
    }
    match envelope.mode {
        TransferDeliveryMode::Hosted => {
            store::transfer_delivery::accept_hosted_snapshot(
                &state.store.pool,
                &reference.uid,
                envelope,
                now,
            )
            .await
            .map_err(internal)?;
        }
        TransferDeliveryMode::Replicated => {
            store::transfer_delivery::accept_replica_envelope(
                &state.store.pool,
                &reference.uid,
                envelope,
                now,
            )
            .await
            .map_err(internal)?;
        }
    }
    Ok(())
}

async fn accept_policy_event(
    state: &CellApiState,
    event: &TransferDeliveryPolicyEventV1,
    now: DateTime<Utc>,
) -> Result<store::transfer_delivery::RemoteReferenceRow, HttpError> {
    state
        .engine
        .verify_transfer_delivery_policy_event(event)
        .await
        .map_err(engine_error)?;
    let signed = serde_json::to_value(event).map_err(internal)?;
    let existing = store::transfer_delivery::remote_reference_by_identity(
        &state.store.pool,
        &event.origin_organ_uid,
        &event.transfer_uid,
        &event.recipient_person_uid,
        &event.recipient_organ_uid,
    )
    .await
    .map_err(internal)?;
    let Some(reference) = existing else {
        if event.kind != "reference" || event.state != "active" {
            return Err((
                StatusCode::CONFLICT,
                "First Transfer delivery policy event must be an active reference".into(),
            ));
        }
        let contact = store::organs::contact(&state.store.pool, &event.origin_organ_uid)
            .await
            .map_err(internal)?
            .ok_or_else(|| forbidden("Transfer origin is not introduced"))?;
        return store::transfer_delivery::create_remote_reference(
            &state.store.pool,
            store::transfer_delivery::NewRemoteReference {
                origin_organ_uid: &event.origin_organ_uid,
                transfer_uid: &event.transfer_uid,
                delivery_policy_uid: &event.delivery_policy_uid,
                recipient_person_uid: &event.recipient_person_uid,
                recipient_organ_uid: &event.recipient_organ_uid,
                mode: event.mode,
                policy_revision: event.policy_revision,
                hosted_url: Some(&contact.base_url),
                policy_payload_hash: &event.payload_hash,
                signed_policy_payload: &signed,
                envelope_uid: None,
            },
            now,
        )
        .await
        .map_err(internal);
    };
    if reference.delivery_policy_uid != event.delivery_policy_uid {
        return Err((
            StatusCode::CONFLICT,
            "Transfer delivery policy identity changed".into(),
        ));
    }
    if event.policy_revision == reference.policy_revision {
        let stored: Option<(String, String)> = store::sqlx::query_as(
            "SELECT payload_hash, signed_payload FROM transfer_remote_policy_event
             WHERE reference_uid = ? AND policy_revision = ?",
        )
        .bind(&reference.uid)
        .bind(event.policy_revision as i64)
        .fetch_optional(&state.store.pool)
        .await
        .map_err(internal)?;
        let submitted = serde_json::to_string(event).map_err(internal)?;
        if stored.as_ref().is_some_and(|(payload_hash, payload)| {
            payload_hash == &event.payload_hash && payload == &submitted
        }) && event.mode == reference.mode
            && event.state == reference.state
        {
            return Ok(reference);
        }
        return Err((
            StatusCode::CONFLICT,
            "Transfer delivery policy revision replay changed".into(),
        ));
    }
    if event.policy_revision != reference.policy_revision + 1 {
        return Err((
            StatusCode::CONFLICT,
            "Transfer delivery policy revision has a gap".into(),
        ));
    }
    store::transfer_delivery::apply_remote_policy(
        &state.store.pool,
        &reference.uid,
        reference.policy_revision,
        event.mode,
        event.state == "revoked",
        None,
        &event.payload_hash,
        &signed,
        now,
    )
    .await
    .map_err(internal)
}

async fn verify_wire<T: Serialize>(
    state: &CellApiState,
    wire: &Authenticated<T>,
    path: &str,
    now: DateTime<Utc>,
) -> Result<(), HttpError> {
    state
        .engine
        .verify_organ_request(
            &wire.auth,
            "POST",
            path,
            &body_bytes(&wire.body).map_err(internal)?,
            now,
        )
        .await
        .map_err(engine_error)
}

async fn signed_response<T: Serialize + DeserializeOwned>(
    state: &CellApiState,
    path: &str,
    recipient_organ_uid: &str,
    body: T,
    now: DateTime<Utc>,
) -> Result<Json<Authenticated<T>>, HttpError> {
    let auth = state
        .engine
        .sign_organ_request(
            "POST",
            path,
            &body_bytes(&body).map_err(internal)?,
            recipient_organ_uid,
            now,
        )
        .await
        .map_err(engine_error)?;
    Ok(Json(Authenticated { auth, body }))
}

fn body_bytes(value: &impl Serialize) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}

fn bad_request(message: impl Into<String>) -> HttpError {
    (StatusCode::BAD_REQUEST, message.into())
}

fn forbidden(message: impl Into<String>) -> HttpError {
    (StatusCode::FORBIDDEN, message.into())
}

fn internal(error: impl ToString) -> HttpError {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn engine_error(error: engine::EngineError) -> HttpError {
    let status = match error {
        engine::EngineError::Forbidden(_) => StatusCode::FORBIDDEN,
        engine::EngineError::Conflict { .. } => StatusCode::CONFLICT,
        engine::EngineError::UnknownRecord(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, error.to_string())
}
