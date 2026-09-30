use std::time::Duration;

use chrono::{DateTime, Utc};
use nucleus::transfer_delivery::{
    SignedOrganRequestV1, TransferApplicationAttestationV1, TransferDeliveryMode,
    TransferDeliveryPolicyEventV1, TransferEnvelopeV1, TransferPackageReceiptV1,
    TransferRemoteCommandV1,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;

use crate::CellRuntime;

const ENVELOPE_PATH: &str = "/organ/transfers/envelopes";
const PULL_PATH: &str = "/organ/transfers/pull";
const PULL_RESULT_PATH: &str = "/organ/transfers/pull-result";
const RECEIPT_PATH: &str = "/organ/transfers/receipts";
const COMMAND_PATH: &str = "/organ/transfers/commands";
const COMMAND_RESULT_PATH: &str = "/organ/transfers/command-results";
const POLICY_PATH: &str = "/organ/transfers/policy-events";
const ATTESTATION_PATH: &str = "/organ/transfers/application-attestations";
const ATTESTATION_RESULT_PATH: &str = "/organ/transfers/application-attestation-results";

type CellError = (Refusal, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    BadRequest,
    Forbidden,
    NotFound,
    Conflict,
    Gone,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Authenticated<T> {
    pub auth: SignedOrganRequestV1,
    pub body: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    transfer_uid: String,
    recipient_person_uid: String,
    after_cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub command_uid: String,
    pub accepted: bool,
    pub authoritative_revision: u64,
    #[serde(default)]
    fact_refs: Vec<FactRef>,
    #[serde(default)]
    warnings: Vec<String>,
    pub created: Option<String>,
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FactRef {
    uid: String,
    hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResult {
    policy: TransferDeliveryPolicyEventV1,
    envelope: Option<TransferEnvelopeV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryPush {
    pub policy: TransferDeliveryPolicyEventV1,
    pub envelope: TransferEnvelopeV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationAttestationRequest {
    handoff_uid: String,
    request_id: String,
    attestation: TransferApplicationAttestationV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationAttestationResult {
    handoff_uid: String,
    attestation_uid: String,
    state: String,
}

pub async fn receive_envelope(
    state: &CellRuntime,
    wire: Authenticated<DeliveryPush>,
) -> Result<serde_json::Value, CellError> {
    let now = nucleus::execution::now();
    verify_wire(state, &wire, ENVELOPE_PATH, now).await?;
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
    accept_policy_event(state, &wire.body.policy, now).await?;
    accept_envelope(state, &wire.body.envelope, now).await?;

    let receipt = state
        .engine
        .sign_transfer_package_receipt(TransferPackageReceiptV1 {
            version: nucleus::transfer_delivery::TRANSFER_ENVELOPE_VERSION,
            request_id: format!(
                "transfer-package-received:{}:{}",
                wire.body.envelope.recipient_organ_uid, wire.body.envelope.envelope_uid
            ),
            envelope_uid: wire.body.envelope.envelope_uid.clone(),
            transfer_uid: wire.body.envelope.transfer_uid.clone(),
            origin_organ_uid: wire.body.envelope.origin_organ_uid.clone(),
            recipient_person_uid: wire.body.envelope.recipient_person_uid.clone(),
            recipient_organ_uid: wire.body.envelope.recipient_organ_uid.clone(),
            cursor: wire.body.envelope.cursor,
            kind: "received".into(),
            created_at: wire.body.envelope.created_at.clone(),
            key_id: String::new(),
            signature: String::new(),
        })
        .await
        .map_err(engine_error)?;
    signed_response(
        &state,
        RECEIPT_PATH,
        &wire.body.envelope.origin_organ_uid,
        receipt,
        now,
    )
    .await
}

pub async fn pull_envelope(
    state: &CellRuntime,
    wire: Authenticated<PullRequest>,
) -> Result<serde_json::Value, CellError> {
    let now = nucleus::execution::now();
    verify_wire(state, &wire, PULL_PATH, now).await?;
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
            Refusal::Gone,
            "Transfer delivery is absent or revoked".into(),
        )
    })?;

    let policy = store::transfer_delivery::policy(&state.store.pool, &delivery_uid)
        .await
        .map_err(internal)?
        .ok_or_else(|| (Refusal::Gone, "Transfer delivery is absent".into()))?;
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
        let request_id = format!("transfer-pull:{}", nucleus::fact::sha256_hex(format!("{}:{}:{}", delivery_uid, policy.revision, wire.auth.nonce).as_bytes()));
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

pub async fn receive_policy_event(
    state: &CellRuntime,
    wire: Authenticated<TransferDeliveryPolicyEventV1>,
) -> Result<serde_json::Value, CellError> {
    let now = nucleus::execution::now();
    verify_wire(state, &wire, POLICY_PATH, now).await?;
    if wire.auth.sender_organ_uid != wire.body.origin_organ_uid
        || wire.auth.recipient_organ_uid != wire.body.recipient_organ_uid
    {
        return Err(forbidden(
            "Organ request and Transfer policy identities differ",
        ));
    }
    let reference = accept_policy_event(state, &wire.body, now).await?;
    Ok(json!({
        "reference_uid": reference.uid,
        "policy_revision": reference.policy_revision,
        "state": reference.state,
    }))
}

pub async fn receive_application_attestation(
    state: &CellRuntime,
    wire: Authenticated<ApplicationAttestationRequest>,
) -> Result<serde_json::Value, CellError> {
    let now = nucleus::execution::now();
    verify_wire(state, &wire, ATTESTATION_PATH, now).await?;
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
    signed_response(
        &state,
        ATTESTATION_RESULT_PATH,
        &wire.auth.sender_organ_uid,
        ApplicationAttestationResult {
            handoff_uid: handoff.uid,
            state: handoff.state.as_str().into(),
            attestation_uid: handoff
                .attestation_uid
                .filter(|uid| !uid.is_empty())
                .ok_or_else(|| internal("Accepted handoff is missing its attestation"))?,
        },
        now,
    )
    .await
}

pub async fn receive_receipt(
    state: &CellRuntime,
    wire: Authenticated<TransferPackageReceiptV1>,
) -> Result<serde_json::Value, CellError> {
    let now = nucleus::execution::now();
    verify_wire(state, &wire, RECEIPT_PATH, now).await?;
    state
        .engine
        .verify_transfer_package_receipt(&wire.body)
        .await
        .map_err(engine_error)?;
    if wire.auth.sender_organ_uid != wire.body.recipient_organ_uid
        || wire.auth.recipient_organ_uid != wire.body.origin_organ_uid
    {
        return Err(forbidden(
            "Organ request and package receipt identities differ",
        ));
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
    let envelope_matches: bool = store::sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM transfer_delivery_outbox
             WHERE delivery_uid = ? AND envelope_uid = ? AND cursor = ?
         )",
    )
    .bind(&delivery_uid)
    .bind(&wire.body.envelope_uid)
    .bind(wire.body.cursor as i64)
    .fetch_one(&state.store.pool)
    .await
    .map_err(internal)?;
    if !envelope_matches {
        return Err(forbidden(
            "Package receipt does not identify an envelope sent under this policy",
        ));
    }
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
            key_id: &wire.body.key_id,
            signature: &wire.body.signature,
            signed_payload: &signed_payload,
            local_fact_uid: None,
            request_id: &wire.body.request_id,
        },
        now,
    )
    .await
    .map_err(internal)?;
    Ok(json!({ "receipt_uid": uid }))
}

pub async fn receive_command(
    state: &CellRuntime,
    wire: Authenticated<TransferRemoteCommandV1>,
) -> Result<serde_json::Value, CellError> {
    let now = nucleus::execution::now();
    verify_wire(state, &wire, COMMAND_PATH, now).await?;
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
        .map(|payload| serde_json::from_str(&payload))
        .transpose()
        .map_err(internal)?
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

pub fn spawn_worker(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = enqueue_periodic_pulls(&state).await {
                tracing::warn!(%error, "cannot schedule periodic Transfer pulls");
            }
            if let Err(error) = drain_envelopes(&state).await {
                tracing::warn!(%error, "Transfer envelope drain failed");
            }
            if let Err(error) = drain_commands(&state).await {
                tracing::warn!(%error, "Transfer command drain failed");
            }
            if let Err(error) = drain_pulls(&state).await {
                tracing::warn!(%error, "Transfer pull drain failed");
            }
            if let Err(error) = drain_application_attestations(&state).await {
                tracing::warn!(%error, "Transfer application attestation drain failed");
            }
        }
    })
}

async fn enqueue_periodic_pulls(state: &CellRuntime) -> Result<(), String> {
    let reference_uids: Vec<String> = store::sqlx::query_scalar(
        "SELECT uid FROM transfer_remote_reference r WHERE state = 'active' AND NOT EXISTS (SELECT 1 FROM transfer_sync_owner o WHERE o.table_name = 'transfer_remote_reference' AND o.row_key = json_array(r.uid) AND o.cell_uid != (SELECT uid FROM record WHERE slug = 'local-cell')) ORDER BY uid",
    )
    .fetch_all(&state.store.pool)
    .await
    .map_err(|error| error.to_string())?;
    let minute = nucleus::execution::now().timestamp() / 60;
    for reference_uid in reference_uids {
        store::transfer_delivery::enqueue_pull(
            &state.store.pool,
            &reference_uid,
            &format!("periodic-transfer-pull:{reference_uid}:{minute}"),
            nucleus::execution::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn drain_envelopes(state: &CellRuntime) -> Result<(), String> {
    for row in store::transfer_delivery::outbox_due(&state.store.pool, nucleus::execution::now(), 32)
        .await
        .map_err(|error| error.to_string())?
    {
        if !executor_runs_here(state, &row).await? {
            continue;
        }
        let attempt = push_envelope(state, &row).await;
        match attempt {
            Ok(cursor) => store::transfer_delivery::outbox_mark_sent(
                &state.store.pool,
                &row.uid,
                cursor,
                nucleus::execution::now(),
            )
            .await
            .map_err(|error| error.to_string())?,
            Err(error) => {
                store::transfer_delivery::outbox_mark_failed(
                    &state.store.pool,
                    &row.uid,
                    nucleus::execution::now(),
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

async fn executor_runs_here(
    state: &CellRuntime,
    row: &store::transfer_delivery::DeliveryOutboxRow,
) -> Result<bool, String> {
    let Some(policy) = store::transfer_delivery::policy(&state.store.pool, &row.delivery_uid)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(true);
    };
    store::executor::runs_here(&state.store.pool, &policy.transfer_uid)
        .await
        .map_err(|error| error.to_string())
}

async fn push_envelope(
    state: &CellRuntime,
    row: &store::transfer_delivery::DeliveryOutboxRow,
) -> Result<u64, String> {
    let (recipient, request) = prepare_envelope(state, row).await?;
    let receipt = post_to_peer(state, &recipient, engine::wire::TransferVerb::Envelope, &request).await?;
    acknowledge_envelope(state, row, receipt).await
}

pub async fn prepare_envelope(
    state: &CellRuntime,
    row: &store::transfer_delivery::DeliveryOutboxRow,
) -> Result<(String, Authenticated<DeliveryPush>), String> {
    if !executor_runs_here(state, row).await? {
        return Err("this Cell is not the Transfer executor".into());
    }
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
    if contact.trust != "known" || !contact.sync_out {
        return Err(
            "recipient Organ is not a known contact, or outgoing delivery is disabled".into(),
        );
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
        .sign_transfer_delivery_policy_event(&policy, policy_kind, nucleus::execution::now())
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
            nucleus::execution::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((contact.record_uid, Authenticated { auth, body }))
}

pub async fn acknowledge_envelope(
    state: &CellRuntime,
    row: &store::transfer_delivery::DeliveryOutboxRow,
    receipt: Authenticated<TransferPackageReceiptV1>,
) -> Result<u64, String> {
    let policy = store::transfer_delivery::policy(&state.store.pool, &row.delivery_uid).await
        .map_err(|error| error.to_string())?.ok_or("delivery policy disappeared")?;
    verify_wire(state, &receipt, RECEIPT_PATH, nucleus::execution::now())
        .await
        .map_err(|(_, error)| error)?;
    state
        .engine
        .verify_transfer_package_receipt(&receipt.body)
        .await
        .map_err(|error| error.to_string())?;
    if receipt.auth.sender_organ_uid != policy.recipient_organ_uid
        || receipt.auth.recipient_organ_uid != policy.origin_organ_uid
        || receipt.body.recipient_organ_uid != policy.recipient_organ_uid
        || receipt.body.origin_organ_uid != policy.origin_organ_uid
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
            key_id: &receipt.body.key_id,
            signature: &receipt.body.signature,
            signed_payload: &signed_payload,
            local_fact_uid: None,
            request_id: &receipt.body.request_id,
        },
        nucleus::execution::now(),
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(receipt.body.cursor)
}

async fn drain_commands(state: &CellRuntime) -> Result<(), String> {
    for row in store::transfer_delivery::remote_commands_due(&state.store.pool, nucleus::execution::now(), 16)
        .await
        .map_err(|error| error.to_string())?
    {
        let result = push_command(state, &row).await;
        match result {
            Ok(_) => {}
            Err(error) => {
                store::transfer_delivery::remote_command_mark_failed(
                    &state.store.pool,
                    &row.command_uid,
                    nucleus::execution::now(),
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

async fn drain_application_attestations(state: &CellRuntime) -> Result<(), String> {
    for row in
        store::transfer_delivery::application_attestations_due(&state.store.pool, nucleus::execution::now(), 16)
            .await
            .map_err(|error| error.to_string())?
    {
        let result = push_application_attestation(state, &row).await;
        match result {
            Ok(()) => {}
            Err(error) => store::transfer_delivery::application_attestation_mark_failed(
                &state.store.pool,
                &row.attestation_uid,
                nucleus::execution::now(),
                &error,
            )
            .await
            .map_err(|store_error| store_error.to_string())?,
        }
    }
    Ok(())
}

pub async fn prepare_application_attestation(
    state: &CellRuntime,
    row: &store::transfer_delivery::AttestationOutboxRow,
) -> Result<Authenticated<ApplicationAttestationRequest>, String> {
    let contact = store::organs::contact(&state.store.pool, &row.origin_organ_uid)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "origin Organ is not introduced".to_string())?;
    if contact.trust != "known" || !contact.sync_out {
        return Err("origin Organ is not a known contact, or outgoing delivery is disabled".into());
    }
    let attestation: TransferApplicationAttestationV1 = serde_json::from_str(&row.payload)
        .map_err(|error| format!("invalid queued application attestation: {error}"))?;
    let body = ApplicationAttestationRequest {
        handoff_uid: row.handoff_uid.clone(),
        request_id: format!("accept-transfer-application:{}", row.attestation_uid),
        attestation,
    };
    let auth = state
        .engine
        .sign_organ_request(
            "POST",
            ATTESTATION_PATH,
            &body_bytes(&body).map_err(|error| error.to_string())?,
            &row.origin_organ_uid,
            nucleus::execution::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(Authenticated { auth, body })
}

pub async fn receive_application_attestation_result(
    state: &CellRuntime,
    wire: Authenticated<ApplicationAttestationResult>,
) -> Result<(), CellError> {
    verify_wire(state, &wire, ATTESTATION_RESULT_PATH, nucleus::execution::now()).await?;
    let binding: Option<(String, String)> = store::sqlx::query_as(
        "SELECT origin_organ_uid, handoff_uid FROM transfer_application_attestation_outbox WHERE attestation_uid = ?",
    ).bind(&wire.body.attestation_uid).fetch_optional(&state.store.pool).await.map_err(internal)?;
    let (origin, handoff) = binding.ok_or_else(|| forbidden("unknown application attestation result"))?;
    if wire.auth.sender_organ_uid != origin || wire.body.handoff_uid != handoff || wire.body.state != "accepted" {
        return Err(forbidden("application attestation result identity mismatch"));
    }
    store::transfer_delivery::application_attestation_mark_sent(
        &state.store.pool, &wire.body.attestation_uid, nucleus::execution::now(),
    ).await.map_err(internal)
}

async fn push_application_attestation(
    state: &CellRuntime,
    row: &store::transfer_delivery::AttestationOutboxRow,
) -> Result<(), String> {
    let request = prepare_application_attestation(state, row).await?;
    let wire: Authenticated<ApplicationAttestationResult> = post_to_peer(
        state, &row.origin_organ_uid, engine::wire::TransferVerb::ApplicationAttestation, &request,
    ).await?;
    if wire.body.attestation_uid != row.attestation_uid {
        return Err("application attestation result identity mismatch".into());
    }
    receive_application_attestation_result(state, wire).await.map_err(|(_, error)| error)
}

async fn drain_pulls(state: &CellRuntime) -> Result<(), String> {
    for row in store::transfer_delivery::pulls_due(&state.store.pool, nucleus::execution::now(), 16)
        .await
        .map_err(|error| error.to_string())?
    {
        let attempt = pull_reference(state, &row).await;
        match attempt {
            Ok(cursor) => store::transfer_delivery::pull_mark_completed(
                &state.store.pool,
                &row.uid,
                cursor,
                nucleus::execution::now(),
            )
            .await
            .map_err(|error| error.to_string())?,
            Err(error) => {
                store::transfer_delivery::pull_mark_failed(
                    &state.store.pool,
                    &row.uid,
                    nucleus::execution::now(),
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

pub async fn prepare_pull(
    state: &CellRuntime,
    row: &store::transfer_delivery::PullRequestRow,
) -> Result<(String, Authenticated<PullRequest>), String> {
    let reference: Option<(String, String, String, String)> = store::sqlx::query_as(
        "SELECT origin_organ_uid, transfer_uid, recipient_person_uid,
                    recipient_organ_uid
             FROM transfer_remote_reference WHERE uid = ? AND state = 'active'",
    )
    .bind(&row.reference_uid)
    .fetch_optional(&state.store.pool)
    .await
    .map_err(|error| error.to_string())?;
    let (origin, transfer, person, _) =
        reference.ok_or_else(|| "remote Transfer reference is absent or revoked".to_string())?;
    let contact = store::organs::contact(&state.store.pool, &origin)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "origin Organ is not introduced".to_string())?;
    if contact.trust != "known" || !contact.sync_out {
        return Err("origin Organ is not a known contact, or outgoing delivery is disabled".into());
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
            nucleus::execution::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((
        origin,
        Authenticated {
            auth,
            body: request,
        },
    ))
}

pub async fn receive_pull_result(
    state: &CellRuntime,
    reference_uid: &str,
    after_cursor: u64,
    wire: Authenticated<PullResult>,
) -> Result<(u64, Option<Authenticated<TransferPackageReceiptV1>>), String> {
    let (origin, transfer, person, local_organ): (String, String, String, String) = store::sqlx::query_as(
        "SELECT origin_organ_uid, transfer_uid, recipient_person_uid, recipient_organ_uid FROM transfer_remote_reference WHERE uid = ?",
    ).bind(reference_uid).fetch_one(&state.store.pool).await.map_err(|error| error.to_string())?;
    verify_wire(state, &wire, PULL_RESULT_PATH, nucleus::execution::now())
        .await
        .map_err(|(_, error)| error)?;
    if wire.auth.sender_organ_uid != origin
        || wire.auth.recipient_organ_uid != local_organ
        || wire.body.policy.origin_organ_uid != origin
        || wire.body.policy.recipient_organ_uid != local_organ
        || wire.body.policy.transfer_uid != transfer
        || wire.body.policy.recipient_person_uid != person
    {
        return Err("pull result does not match the requested Transfer and recipient".into());
    }
    if let Some(envelope) = &wire.body.envelope {
        if envelope.transfer_uid != transfer
            || envelope.recipient_person_uid != person
            || envelope.delivery_policy_uid != wire.body.policy.delivery_policy_uid
            || envelope.delivery_policy_revision != wire.body.policy.policy_revision
        {
            return Err("pull result envelope does not match its policy".into());
        }
    }
    accept_policy_event(state, &wire.body.policy, nucleus::execution::now())
        .await
        .map_err(|(_, error)| error)?;
    let Some(envelope) = wire.body.envelope else {
        return Ok((after_cursor, None));
    };
    accept_envelope(state, &envelope, nucleus::execution::now())
        .await
        .map_err(|(_, error)| error)?;
    let receipt = prepare_received_receipt(state, &envelope).await?;
    Ok((envelope.cursor, Some(receipt)))
}

async fn pull_reference(
    state: &CellRuntime,
    row: &store::transfer_delivery::PullRequestRow,
) -> Result<u64, String> {
    let (origin, request) = prepare_pull(state, row).await?;
    let wire = post_to_peer(state, &origin, engine::wire::TransferVerb::Pull, &request).await?;
    let (cursor, receipt) =
        receive_pull_result(state, &row.reference_uid, row.after_cursor, wire).await?;
    if let Some(receipt) = receipt {
        let _: serde_json::Value = post_to_peer(
            state,
            &origin,
            engine::wire::TransferVerb::Receipt,
            &receipt,
        )
        .await?;
    }
    Ok(cursor)
}

async fn prepare_received_receipt(
    state: &CellRuntime,
    envelope: &TransferEnvelopeV1,
) -> Result<Authenticated<TransferPackageReceiptV1>, String> {
    let receipt = state
        .engine
        .sign_transfer_package_receipt(TransferPackageReceiptV1 {
            version: nucleus::transfer_delivery::TRANSFER_ENVELOPE_VERSION,
            request_id: format!(
                "transfer-package-received:{}:{}",
                envelope.recipient_organ_uid, envelope.envelope_uid
            ),
            envelope_uid: envelope.envelope_uid.clone(),
            transfer_uid: envelope.transfer_uid.clone(),
            origin_organ_uid: envelope.origin_organ_uid.clone(),
            recipient_person_uid: envelope.recipient_person_uid.clone(),
            recipient_organ_uid: envelope.recipient_organ_uid.clone(),
            cursor: envelope.cursor,
            kind: "received".into(),
            created_at: envelope.created_at.clone(),
            key_id: String::new(),
            signature: String::new(),
        })
        .await
        .map_err(|error| error.to_string())?;
    let auth = state
        .engine
        .sign_organ_request(
            "POST",
            RECEIPT_PATH,
            &body_bytes(&receipt).map_err(|error| error.to_string())?,
            &envelope.origin_organ_uid,
            nucleus::execution::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(Authenticated {
        auth,
        body: receipt,
    })
}

pub async fn prepare_command(
    state: &CellRuntime,
    row: &store::transfer_delivery::RemoteCommandRow,
) -> Result<Authenticated<TransferRemoteCommandV1>, String> {
    let contact = store::organs::contact(&state.store.pool, &row.origin_organ_uid)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "origin Organ is not introduced".to_string())?;
    if contact.trust != "known" || !contact.sync_out {
        return Err("origin Organ is not a known contact, or outgoing delivery is disabled".into());
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
            nucleus::execution::now(),
        )
        .await
        .map_err(|error| error.to_string())?;
    if !state.engine.prepare_karma_transfer_command_dispatch(row).await.map_err(|error| error.to_string())? {
        return Err("The Rule's remote command was cancelled before dispatch".into());
    }
    Ok(Authenticated { auth, body: command })
}

pub async fn receive_command_result(
    state: &CellRuntime,
    wire: Authenticated<CommandResult>,
) -> Result<CommandResult, CellError> {
    verify_wire(state, &wire, COMMAND_RESULT_PATH, nucleus::execution::now()).await?;
    let row = store::transfer_delivery::remote_command(&state.store.pool, &wire.body.command_uid)
        .await.map_err(internal)?.ok_or_else(|| forbidden("unknown remote command result"))?;
    if row.direction != "outgoing" || wire.auth.sender_organ_uid != row.origin_organ_uid {
        return Err(forbidden("origin command result identity mismatch"));
    }
    let result = wire.body;
    persist_command_result(state, &row, &result).await.map_err(internal)?;
    Ok(result)
}

async fn persist_command_result(
    state: &CellRuntime,
    row: &store::transfer_delivery::RemoteCommandRow,
    result: &CommandResult,
) -> Result<(), String> {
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
            nucleus::execution::now(),
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
        nucleus::execution::now(),
    )
    .await
    .map_err(|error| error.to_string())?;
    if store::karma_commands::get(&state.store.pool, &row.command_uid).await.map_err(|error| error.to_string())?.is_some() {
        state.engine.refresh_karma_transfer_command_outcomes().await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn push_command(
    state: &CellRuntime,
    row: &store::transfer_delivery::RemoteCommandRow,
) -> Result<CommandResult, String> {
    let command = prepare_command(state, row).await?;
    let wire: Authenticated<CommandResult> = post_to_peer(
        state, &row.origin_organ_uid, engine::wire::TransferVerb::Command, &command,
    ).await?;
    if wire.body.command_uid != row.command_uid {
        return Err("origin command result identity mismatch".into());
    }
    receive_command_result(state, wire).await.map_err(|(_, error)| error)
}

async fn accept_envelope(
    state: &CellRuntime,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
) -> Result<(), CellError> {
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
        return Err((Refusal::Gone, "Transfer delivery was revoked".into()));
    }
    let reference = reference.ok_or_else(|| {
        (
            Refusal::Conflict,
            "Transfer envelope arrived without its signed delivery policy".into(),
        )
    })?;
    if reference.mode != envelope.mode {
        return Err((
            Refusal::Conflict,
            "Envelope mode differs from the last signed delivery policy".into(),
        ));
    }
    let before = state.engine.read_karma_transfer_state(&envelope.transfer_uid, None).await.ok();
    let accepted = match envelope.mode {
        TransferDeliveryMode::Hosted => {
            store::transfer_delivery::accept_hosted_snapshot(
                &state.store.pool,
                &reference.uid,
                envelope,
                now,
            )
            .await
            .map_err(internal)?
        }
        TransferDeliveryMode::Replicated => {
            store::transfer_delivery::accept_replica_envelope(
                &state.store.pool,
                &reference.uid,
                envelope,
                now,
            )
            .await
            .map_err(internal)?
        }
    };
    let (accepted, applied) = match accepted {
        store::transfer_delivery::ReplicaCommit::Applied(reference) => (reference, true),
        store::transfer_delivery::ReplicaCommit::Replayed(reference) => (reference, false),
    };
    if envelope.cursor < accepted.last_cursor {
        return Ok(());
    }
    for value in envelope
        .projection
        .get("application_handoffs")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let handoff: nucleus::transfer_delivery::TransferApplicationHandoffV1 =
            serde_json::from_value(value.clone()).map_err(internal)?;
        state
            .engine
            .verify_transfer_application_handoff(&handoff)
            .await
            .map_err(engine_error)?;
        if handoff.origin_organ_uid != envelope.origin_organ_uid
            || handoff.participant_organ_uid != envelope.recipient_organ_uid
            || handoff.participant_person_uid != envelope.recipient_person_uid
            || handoff.transfer_uid != envelope.transfer_uid
        {
            return Err(forbidden(
                "Application handoff and Transfer envelope identities differ",
            ));
        }
        store::transfer_delivery::accept_remote_application_handoff(
            &state.store.pool,
            store::transfer_delivery::NewRemoteApplicationHandoff {
                uid: &handoff.handoff_uid,
                reference_uid: &reference.uid,
                origin_organ_uid: &handoff.origin_organ_uid,
                participant_organ_uid: &handoff.participant_organ_uid,
                participant_person_uid: &handoff.participant_person_uid,
                transfer_uid: &handoff.transfer_uid,
                occurrence_uid: &handoff.occurrence_uid,
                source_promise_uid: &handoff.source_promise_uid,
                settlement_slice_uid: &handoff.settlement_slice_uid,
                origin_revision: handoff.origin_revision,
                canonical_quantity: handoff.canonical_quantity,
                canonical_unit_uid: handoff.canonical_unit_uid.as_deref(),
                canonical_cumulative_before: handoff.canonical_cumulative_before,
                canonical_cumulative_after: handoff.canonical_cumulative_after,
                canonical_remaining_after: handoff.canonical_remaining_after,
                application_direction: handoff.application_direction,
                canonical_slice_hash: &handoff.canonical_slice_hash,
                envelope_uid: &envelope.envelope_uid,
                envelope_payload_hash: &envelope.payload_hash,
                origin_created_at: &handoff.created_at,
            },
            now,
        )
        .await
        .map_err(internal)?;
    }
    let after = if applied { state.engine.read_karma_transfer_state(&envelope.transfer_uid, None).await.ok() } else { None };
    if applied && after.is_some() && before != after {
        let parent = envelope.projection["agreement"]["history"].as_array().into_iter().flatten().rev().find(|event| {
            let Some(person) = event["person"].as_str() else { return false; };
            let current = after.as_ref().and_then(|state| state.participants.get(person));
            let original = before.as_ref().and_then(|state| state.participants.get(person));
            current != original && current.is_some_and(|state| state.guard.change_uid.as_deref().is_some_and(|uid| Some(uid) == event["uid"].as_str()))
        }).and_then(|event| event["fact"].as_str());
        state.engine.observe_karma_transfer_projection(&envelope.transfer_uid, &envelope.envelope_uid, parent, now).await.map_err(engine_error)?;
    }
    Ok(())
}

async fn accept_policy_event(
    state: &CellRuntime,
    event: &TransferDeliveryPolicyEventV1,
    now: DateTime<Utc>,
) -> Result<store::transfer_delivery::RemoteReferenceRow, CellError> {
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
                Refusal::Conflict,
                "First Transfer delivery policy event must be an active reference".into(),
            ));
        }
        store::organs::contact(&state.store.pool, &event.origin_organ_uid)
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
                hosted_url: None,
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
            Refusal::Conflict,
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
            Refusal::Conflict,
            "Transfer delivery policy revision replay changed".into(),
        ));
    }
    if event.policy_revision != reference.policy_revision + 1 {
        return Err((
            Refusal::Conflict,
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

async fn post_to_peer<B, R>(
    state: &CellRuntime,
    contact_organ: &str,
    verb: engine::wire::TransferVerb,
    body: &Authenticated<B>,
) -> Result<R, String>
where
    B: Serialize,
    R: DeserializeOwned,
{
    let wire =
        state.wire.read().await.clone().ok_or_else(|| {
            "this Cell has no iroh endpoint, so nothing can be delivered".to_string()
        })?;
    let payload = serde_json::to_value(body).map_err(|error| error.to_string())?;
    let reply = wire
        .transfer_post(contact_organ, verb, payload)
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_value(reply).map_err(|error| format!("invalid peer reply: {error}"))
}

async fn verify_wire<T: Serialize>(
    state: &CellRuntime,
    wire: &Authenticated<T>,
    path: &str,
    now: DateTime<Utc>,
) -> Result<(), CellError> {
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
    state: &CellRuntime,
    path: &str,
    recipient_organ_uid: &str,
    body: T,
    now: DateTime<Utc>,
) -> Result<serde_json::Value, CellError> {
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
    serde_json::to_value(Authenticated { auth, body }).map_err(internal)
}

fn body_bytes(value: &impl Serialize) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}

fn forbidden(message: impl Into<String>) -> CellError {
    (Refusal::Forbidden, message.into())
}

fn internal(error: impl ToString) -> CellError {
    (Refusal::Internal, error.to_string())
}

fn engine_error(error: engine::EngineError) -> CellError {
    let status = match error {
        engine::EngineError::Forbidden(_) => Refusal::Forbidden,
        engine::EngineError::Conflict { .. } => Refusal::Conflict,
        engine::EngineError::UnknownRecord(_) => Refusal::NotFound,
        _ => Refusal::BadRequest,
    };
    (status, error.to_string())
}

pub struct TransferPeerHandler {
    state: CellRuntime,
}

impl TransferPeerHandler {
    pub fn new(state: CellRuntime) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl engine::wire::TransferPeer for TransferPeerHandler {
    async fn handle(
        &self,
        verb: engine::wire::TransferVerb,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        use engine::wire::TransferVerb;
        fn parse<T: DeserializeOwned>(body: serde_json::Value) -> Result<T, String> {
            serde_json::from_value(body)
                .map_err(|error| format!("malformed Transfer body: {error}"))
        }
        let state = &self.state;
        match verb {
            TransferVerb::Envelope => receive_envelope(state, parse(body)?).await,
            TransferVerb::Pull => pull_envelope(state, parse(body)?).await,
            TransferVerb::Receipt => receive_receipt(state, parse(body)?).await,
            TransferVerb::Command => receive_command(state, parse(body)?).await,
            TransferVerb::PolicyEvent => receive_policy_event(state, parse(body)?).await,
            TransferVerb::ApplicationAttestation => {
                receive_application_attestation(state, parse(body)?).await
            }
        }
        .map_err(|(_, message)| message)
    }
}
