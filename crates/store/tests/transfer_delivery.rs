use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{
    Cause, Fact, NewFact,
    transfer_delivery::{
        SignedOrganRequestV1, TRANSFER_ENVELOPE_VERSION, TransferApplicationAttestationV1,
        TransferApplicationHandoffState::{Accepted, Compensated, Pending},
        TransferDeliveryMode, TransferEnvelopeV1,
    },
};
use store::{
    Store, StoreError, facts,
    sqlx::{self, SqlitePool},
    transfer_delivery::{
        ApplicationHandoffTransition, DeliveryPolicyTransition, EnqueueCommit,
        NewApplicationHandoff, NewDeliveryPolicy, PolicyCommit, change_policy_mode,
        consume_organ_request_nonce, create_application_handoff, create_policy, enqueue,
        outbox_due, outbox_mark_failed, outbox_mark_sent, prune_organ_request_nonces, retry_outbox,
        revoke_policy, store_application_attestation, transition_application_handoff,
    },
    transfers::{NewTransfer, create},
};
use tokio::time::timeout;

async fn fixture() -> Result<(Store, String, String), StoreError> {
    let store = Store::open_memory().await?;
    let organ = sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-organ'")
        .fetch_one(&store.pool)
        .await?;
    let transfer = create(
        &store.pool,
        NewTransfer {
            slug: None,
            head: "Outgoing Transfer",
            agreement_type: "full",
            agreement_pct: None,
            satiation: None,
            source_uid: None,
            reserve_default: None,
            require_confirmation: false,
        },
    )
    .await?;
    Ok((store, organ, transfer))
}

async fn evidence(
    pool: &SqlitePool,
    transfer: &str,
    now: DateTime<Utc>,
) -> Result<Fact, StoreError> {
    let mut tx = store::write_tx(pool).await?;
    let previous = facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            actor_uid: Some("actor".into()),
            ..NewFact::quantity(transfer, store::exact::zero(), Cause::user_edit())
        },
        &previous,
        now,
    );
    fact.signature = Some("fixture-signature".into());
    facts::insert(&mut tx, &fact).await?;
    tx.commit().await?;
    Ok(fact)
}

#[tokio::test]
async fn policy_create_change_and_revoke_work_with_one_connection()
-> Result<(), Box<dyn std::error::Error>> {
    let (store, origin, transfer) = fixture().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let create_fact = evidence(pool, &transfer, now).await?;
    let create = || NewDeliveryPolicy {
        transfer_uid: &transfer,
        origin_organ_uid: &origin,
        recipient_person_uid: "recipient",
        recipient_organ_uid: "recipient-organ",
        mode: TransferDeliveryMode::Hosted,
        actor_person_uid: "actor",
        fact_uid: &create_fact.uid,
        request_id: "create-policy",
    };
    let deadline = Duration::from_secs(3);
    let PolicyCommit::Applied(created) =
        timeout(deadline, create_policy(pool, create(), now)).await??
    else {
        panic!("a new request should create a policy");
    };
    assert!(matches!(
        create_policy(pool, create(), now).await?,
        PolicyCommit::Replayed(row) if row.uid == created.uid && row.revision == 1
    ));

    let change_fact = evidence(pool, &transfer, now).await?;
    let change = || DeliveryPolicyTransition {
        delivery_uid: &created.uid,
        expected_revision: 1,
        actor_person_uid: "actor",
        fact_uid: &change_fact.uid,
        request_id: "change-mode",
    };
    let replicated = TransferDeliveryMode::Replicated;
    assert!(matches!(
        timeout(deadline, change_policy_mode(pool, change(), replicated, now)).await??,
        PolicyCommit::Applied(row) if row.mode == replicated && row.revision == 2
    ));
    assert!(matches!(
        change_policy_mode(pool, change(), replicated, now).await?,
        PolicyCommit::Replayed(row) if row.mode == replicated && row.revision == 2
    ));
    assert!(matches!(
        change_policy_mode(pool, change(), TransferDeliveryMode::Hosted, now).await,
        Err(StoreError::Protocol(message)) if message.contains("another transition")
    ));

    let revoke_fact = evidence(pool, &transfer, now).await?;
    let revoke = || DeliveryPolicyTransition {
        fact_uid: &revoke_fact.uid,
        expected_revision: 2,
        request_id: "revoke-policy",
        ..change()
    };
    let PolicyCommit::Applied(revoked) =
        timeout(deadline, revoke_policy(pool, revoke(), now)).await??
    else {
        panic!("a new request should revoke the policy");
    };
    assert_eq!((revoked.state.as_str(), revoked.revision), ("revoked", 3));
    Ok(())
}

#[tokio::test]
async fn outgoing_envelopes_preserve_replay_retry_and_acknowledgement()
-> Result<(), Box<dyn std::error::Error>> {
    let (store, origin, transfer) = fixture().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    sqlx::query(
        "INSERT INTO transfer_delivery_policy
             (uid, transfer_uid, origin_organ_uid, recipient_person_uid,
              recipient_organ_uid, mode, created_at, updated_at)
         VALUES ('delivery', ?, ?, 'recipient', 'recipient-organ', 'replicated', ?3, ?3)",
    )
    .bind(&transfer)
    .bind(&origin)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    let mut envelope = TransferEnvelopeV1 {
        version: TRANSFER_ENVELOPE_VERSION,
        envelope_uid: "envelope".into(),
        origin_organ_uid: origin,
        transfer_uid: transfer,
        transfer_revision: 1,
        delivery_policy_uid: "delivery".into(),
        delivery_policy_revision: 1,
        delivery_policy_state: "active".into(),
        cursor: 2,
        recipient_person_uid: "recipient".into(),
        recipient_organ_uid: "recipient-organ".into(),
        mode: TransferDeliveryMode::Replicated,
        disclosure: Vec::new(),
        projection: serde_json::json!({"head": "Original"}),
        events: Vec::new(),
        facts: Vec::new(),
        action_intents: Vec::new(),
        actor_keys: Vec::new(),
        created_at: now.to_rfc3339(),
        payload_hash: String::new(),
        origin_key_id: "key".into(),
        origin_signature: "fixture-signature".into(),
    };
    envelope.payload_hash = envelope.computed_payload_hash()?;
    let EnqueueCommit::Applied(queued) = enqueue(pool, "delivery", &envelope, now).await? else {
        panic!("a new envelope should be queued");
    };
    assert!(matches!(
        enqueue(pool, "delivery", &envelope, now).await?,
        EnqueueCommit::Replayed(row) if row.uid == queued.uid
    ));
    envelope.projection = serde_json::json!({"head": "Changed"});
    envelope.payload_hash = envelope.computed_payload_hash()?;
    assert!(matches!(
        enqueue(pool, "delivery", &envelope, now).await,
        Err(StoreError::Protocol(message)) if message.contains("changed contents")
    ));
    assert_eq!(outbox_due(pool, now, 1).await?.len(), 1);
    outbox_mark_failed(pool, &queued.uid, now, "offline", 5, 30).await?;
    assert!(outbox_due(pool, now, 1).await?.is_empty());
    let retry = retry_outbox(pool, "delivery", "retry", now).await?;
    assert_eq!((retry.status.as_str(), retry.attempts), ("queued", 1));
    assert_eq!(
        retry_outbox(pool, "delivery", "retry", now).await?.uid,
        queued.uid
    );
    assert!(matches!(
        outbox_mark_sent(pool, &queued.uid, 1, now).await,
        Err(StoreError::Protocol(message)) if message.contains("below")
    ));
    outbox_mark_sent(pool, &queued.uid, 2, now).await?;
    outbox_mark_sent(pool, &queued.uid, 2, now).await?;
    assert!(matches!(
        outbox_mark_sent(pool, &queued.uid, 3, now).await,
        Err(StoreError::Protocol(message)) if message.contains("replay")
    ));
    assert!(outbox_due(pool, now, 1).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn nonce_replay_is_scoped_to_the_sender_and_pruned_strictly_before_cutoff()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let mut request = SignedOrganRequestV1 {
        version: TRANSFER_ENVELOPE_VERSION,
        sender_organ_uid: "sender".into(),
        recipient_organ_uid: "recipient-organ".into(),
        method: "GET".into(),
        path: "/transfer".into(),
        body_hash: "empty-body".into(),
        timestamp: now.to_rfc3339(),
        nonce: "nonce".into(),
        key_id: "key".into(),
        signature: "fixture-signature".into(),
    };
    consume_organ_request_nonce(pool, &request, now).await?;
    request.path = "/changed".into();
    assert!(matches!(
        consume_organ_request_nonce(pool, &request, now).await,
        Err(StoreError::Protocol(message)) if message.contains("already consumed")
    ));
    request.sender_organ_uid = "another-sender".into();
    consume_organ_request_nonce(pool, &request, now).await?;
    assert_eq!(prune_organ_request_nonces(pool, now).await?, 0);
    assert_eq!(
        prune_organ_request_nonces(pool, now + TimeDelta::seconds(1)).await?,
        2
    );
    consume_organ_request_nonce(pool, &request, now).await?;
    Ok(())
}

#[tokio::test]
async fn handoff_acceptance_and_compensation_work_with_one_connection()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let create = || NewApplicationHandoff {
        origin_organ_uid: "origin-organ",
        participant_organ_uid: "participant-organ",
        participant_person_uid: "participant",
        transfer_uid: "transfer",
        occurrence_uid: "occurrence",
        settlement_slice_uid: "slice",
        origin_revision: 1,
        canonical_slice_hash: "canonical-hash",
        request_id: "start-handoff",
        fact_uid: None,
    };
    let handoff = create_application_handoff(pool, create(), now).await?;
    assert_eq!(handoff.state, Pending);
    let attestation = TransferApplicationAttestationV1 {
        version: TRANSFER_ENVELOPE_VERSION,
        attestation_uid: "attestation".into(),
        origin_organ_uid: handoff.origin_organ_uid,
        participant_organ_uid: handoff.participant_organ_uid,
        participant_person_uid: handoff.participant_person_uid,
        transfer_uid: handoff.transfer_uid,
        occurrence_uid: handoff.occurrence_uid,
        settlement_slice_uid: handoff.settlement_slice_uid,
        origin_revision: handoff.origin_revision,
        canonical_slice_hash: handoff.canonical_slice_hash,
        formula_commitment: "formula-hash".into(),
        formula_version: "1".into(),
        application_fact_uid: "application-fact".into(),
        applied_at: now.to_rfc3339(),
        key_id: "key".into(),
        signature: "fixture-signature".into(),
    };
    let acceptance = || ApplicationHandoffTransition {
        handoff_uid: &handoff.uid,
        expected_state: Pending,
        to_state: Accepted,
        attestation_uid: Some(&attestation.attestation_uid),
        fact_uid: None,
        reason_code: None,
        request_id: "accept-handoff",
    };
    assert!(matches!(
        transition_application_handoff(
            pool,
            ApplicationHandoffTransition { attestation_uid: None, ..acceptance() },
            now,
        )
        .await,
        Err(StoreError::Protocol(message)) if message.contains("invalid")
    ));
    store_application_attestation(pool, &attestation, now).await?;
    let deadline = Duration::from_secs(3);
    let accepted = timeout(
        deadline,
        transition_application_handoff(pool, acceptance(), now),
    )
    .await??;
    assert_eq!(accepted.state, Accepted);
    assert_eq!(accepted.attestation_uid.as_deref(), Some("attestation"));
    assert_eq!(
        transition_application_handoff(pool, acceptance(), now)
            .await?
            .state,
        Accepted
    );
    assert_eq!(
        create_application_handoff(pool, create(), now).await?.state,
        Accepted
    );
    let compensation = ApplicationHandoffTransition {
        expected_state: Accepted,
        to_state: Compensated,
        attestation_uid: None,
        request_id: "compensate-handoff",
        ..acceptance()
    };
    let compensated = timeout(
        deadline,
        transition_application_handoff(pool, compensation, now),
    )
    .await??;
    assert_eq!(compensated.state, Compensated);
    assert_eq!(compensated.attestation_uid.as_deref(), Some("attestation"));
    Ok(())
}
