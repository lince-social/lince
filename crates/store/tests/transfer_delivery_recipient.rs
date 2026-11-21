use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::transfer_delivery::{
    TRANSFER_ENVELOPE_VERSION as VERSION,
    TransferDeliveryMode::{self, Hosted, Replicated},
    TransferEnvelopeV1,
};
use serde_json::{Value, json};
use store::{
    Store, StoreError, sqlx,
    transfer_delivery::{
        NewRemoteReference, RemoteReferenceRow, ReplicaCommit, accept_hosted_snapshot,
        accept_replica_envelope, apply_remote_policy, create_remote_reference, enqueue_pull,
        pull_mark_completed, pull_mark_failed, pulls_due, remote_reference_by_identity,
    },
    transfer_outcomes::remote_on,
};

fn reference(mode: TransferDeliveryMode, policy: &Value) -> NewRemoteReference<'_> {
    NewRemoteReference {
        origin_organ_uid: "origin",
        transfer_uid: "transfer",
        delivery_policy_uid: "policy",
        recipient_person_uid: "recipient",
        recipient_organ_uid: "recipient-organ",
        mode,
        policy_revision: 1,
        hosted_url: None,
        policy_payload_hash: "policy-1",
        signed_policy_payload: policy,
        envelope_uid: None,
    }
}

fn envelope(
    reference: &RemoteReferenceRow,
    cursor: u64,
    now: DateTime<Utc>,
) -> Result<TransferEnvelopeV1, serde_json::Error> {
    let mut envelope = TransferEnvelopeV1 {
        version: VERSION,
        envelope_uid: format!("envelope-{cursor}"),
        origin_organ_uid: reference.origin_organ_uid.clone(),
        transfer_uid: reference.transfer_uid.clone(),
        transfer_revision: cursor,
        delivery_policy_uid: reference.delivery_policy_uid.clone(),
        delivery_policy_revision: reference.policy_revision,
        delivery_policy_state: "active".into(),
        cursor,
        recipient_person_uid: reference.recipient_person_uid.clone(),
        recipient_organ_uid: reference.recipient_organ_uid.clone(),
        mode: reference.mode,
        disclosure: Vec::new(),
        projection: json!({"head": format!("version {cursor}")}),
        events: Vec::new(),
        facts: Vec::new(),
        action_intents: Vec::new(),
        actor_keys: Vec::new(),
        created_at: now.to_rfc3339(),
        payload_hash: String::new(),
        origin_key_id: "fixture-key".into(),
        origin_signature: "fixture-signature".into(),
    };
    envelope.payload_hash = envelope.computed_payload_hash()?;
    Ok(envelope)
}

#[tokio::test]
async fn recipient_policy_preserves_identity_and_exact_signed_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let original = json!({"revision": 1});
    let created = create_remote_reference(
        pool,
        NewRemoteReference {
            hosted_url: Some("https://origin.invalid/first"),
            ..reference(Hosted, &original)
        },
        now,
    )
    .await?;
    let exact = create_remote_reference(
        pool,
        NewRemoteReference {
            hosted_url: Some("https://origin.invalid/replay"),
            ..reference(Hosted, &original)
        },
        now,
    )
    .await?;
    assert_eq!(created.uid, exact.uid);
    let hosted: String =
        sqlx::query_scalar("SELECT hosted_url FROM transfer_remote_reference WHERE uid = ?")
            .bind(&created.uid)
            .fetch_one(pool)
            .await?;
    assert_eq!(hosted, "https://origin.invalid/first");

    assert!(matches!(
        create_remote_reference(pool, reference(Hosted, &json!({"revision": 99})), now).await,
        Err(StoreError::Protocol(message)) if message.contains("changed contents")
    ));
    let found =
        remote_reference_by_identity(pool, "origin", "transfer", "recipient", "recipient-organ")
            .await?
            .expect("the stored recipient reference");
    assert_eq!(found.uid, created.uid);

    let changed = json!({"revision": 2});
    let second = apply_remote_policy(
        pool,
        &created.uid,
        1,
        Replicated,
        false,
        None,
        "policy-2",
        &changed,
        now,
    )
    .await?;
    assert_eq!((second.mode, second.policy_revision), (Replicated, 2));
    let revoked = apply_remote_policy(
        pool,
        &created.uid,
        2,
        Replicated,
        true,
        None,
        "policy-3",
        &json!({"revision": 3}),
        now,
    )
    .await?;
    assert_eq!(revoked.state, "revoked");

    let replay = apply_remote_policy(
        pool,
        &created.uid,
        1,
        Replicated,
        false,
        None,
        "policy-2",
        &changed,
        now,
    )
    .await?;
    assert_eq!(
        (replay.mode, replay.state.as_str(), replay.policy_revision),
        (Replicated, "revoked", 3)
    );
    assert!(matches!(
        apply_remote_policy(pool, &created.uid, 1, Replicated, false, None, "changed-proof", &changed, now).await,
        Err(StoreError::Protocol(message)) if message.contains("changed contents")
    ));
    assert!(matches!(
        apply_remote_policy(pool, &created.uid, 3, Replicated, false, None, "policy-4", &json!({}), now).await,
        Err(StoreError::Protocol(message)) if message.contains("stale or revoked")
    ));
    Ok(())
}

#[tokio::test]
async fn snapshot_modes_preserve_policy_gates_evidence_and_stale_history()
-> Result<(), Box<dyn std::error::Error>> {
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    for mode in [Hosted, Replicated] {
        let store = Store::open_memory().await?;
        let pool = &store.pool;
        let reference = create_remote_reference(pool, reference(mode, &json!({})), now).await?;
        let current = envelope(&reference, 2, now)?;
        let accept = async |envelope: &TransferEnvelopeV1| match mode {
            Hosted => accept_hosted_snapshot(pool, &reference.uid, envelope, now).await,
            Replicated => accept_replica_envelope(pool, &reference.uid, envelope, now).await,
        };
        assert!(matches!(
            accept(&current).await?,
            ReplicaCommit::Applied(row) if row.last_cursor == 2 && row.last_transfer_revision == 2
        ));
        assert!(matches!(
            accept(&current).await?,
            ReplicaCommit::Replayed(row) if row.last_cursor == 2
        ));
        let mut changed = current.clone();
        changed.projection = json!({"head": "changed replay"});
        changed.payload_hash = changed.computed_payload_hash()?;
        assert!(matches!(
            accept(&changed).await,
            Err(StoreError::Protocol(message)) if message.contains("changed contents")
        ));
        changed.envelope_uid = "different-envelope".into();
        changed.payload_hash = changed.computed_payload_hash()?;
        assert!(matches!(
            accept(&changed).await,
            Err(StoreError::Protocol(message)) if message.contains("cursor")
        ));
        let older = envelope(&reference, 1, now)?;
        assert!(matches!(
            accept(&older).await?,
            ReplicaCommit::Replayed(row) if row.last_cursor == 2
        ));
        let mut wrong_policy = envelope(&reference, 3, now)?;
        wrong_policy.delivery_policy_revision += 1;
        wrong_policy.payload_hash = wrong_policy.computed_payload_hash()?;
        assert!(matches!(
            accept(&wrong_policy).await,
            Err(StoreError::Protocol(message)) if message.contains("does not match")
        ));

        let received: Vec<String> =
            sqlx::query_scalar("SELECT payload FROM transfer_outcome_evidence")
                .fetch_all(pool)
                .await?;
        assert_eq!(received, [serde_json::to_string(&current)?]);
        let history: Vec<String> =
            sqlx::query_scalar("SELECT payload FROM transfer_replica_envelope ORDER BY cursor")
                .fetch_all(pool)
                .await?;
        match mode {
            Hosted => assert!(history.is_empty()),
            Replicated => assert_eq!(
                history,
                [
                    serde_json::to_string(&older)?,
                    serde_json::to_string(&current)?
                ]
            ),
        }
    }
    Ok(())
}

#[tokio::test]
async fn refresh_preserves_writing_cell_cursor_and_retry_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let reference = create_remote_reference(pool, reference(Hosted, &json!({})), now).await?;
    let uid = reference.uid.as_str();
    accept_hosted_snapshot(pool, uid, &envelope(&reference, 4, now)?, now).await?;

    sqlx::query("UPDATE transfer_sync_owner SET cell_uid = 'other-cell' WHERE table_name = 'transfer_remote_reference' AND row_key = json_array(?)")
        .bind(uid).execute(pool).await?;
    assert!(matches!(
        enqueue_pull(pool, uid, "refresh", now).await,
        Err(StoreError::Protocol(message)) if message.contains("writing Cell other-cell")
    ));
    sqlx::query("UPDATE transfer_sync_owner SET cell_uid = (SELECT uid FROM record WHERE slug = 'local-cell') WHERE table_name = 'transfer_remote_reference' AND row_key = json_array(?)")
        .bind(uid).execute(pool).await?;
    let pull = enqueue_pull(pool, uid, "refresh", now).await?;
    assert_eq!((pull.after_cursor, pull.status.as_str()), (4, "queued"));
    assert!(matches!(
        enqueue_pull(pool, "different-reference", "refresh", now).await,
        Err(StoreError::Protocol(message)) if message.contains("another reference")
    ));

    let scheduled = pull_mark_failed(pool, &pull.uid, now, "offline", 5, 30).await?;
    let next = now + TimeDelta::seconds(5);
    assert_eq!(scheduled, next.to_rfc3339());
    assert!(pulls_due(pool, now, 1).await?.is_empty());
    let due = pulls_due(pool, next, 1).await?;
    let [item] = due.as_slice() else {
        panic!("one refresh should be due");
    };
    assert_eq!((item.uid.as_str(), item.attempts), (pull.uid.as_str(), 1));
    assert!(matches!(
        pull_mark_completed(pool, &pull.uid, 3, next).await,
        Err(StoreError::Protocol(message)) if message.contains("starting cursor")
    ));
    pull_mark_completed(pool, &pull.uid, 4, next).await?;
    let replay = enqueue_pull(pool, uid, "refresh", next).await?;
    assert_eq!(
        (replay.uid, replay.after_cursor, replay.status.as_str()),
        (pull.uid, 4, "completed")
    );
    assert!(pulls_due(pool, next, 1).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn received_outcomes_preserve_freshness_and_authority_priority()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let organ: String = sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-organ'")
        .fetch_one(pool)
        .await?;
    let received = create_remote_reference(
        pool,
        NewRemoteReference {
            recipient_organ_uid: &organ,
            ..reference(Hosted, &json!({}))
        },
        now,
    )
    .await?;
    sqlx::raw_sql(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at)
             VALUES ('dependent', 'transfer', (SELECT uid FROM record WHERE slug = 'local-organ'), 'fixture', 'fixture');
         INSERT INTO transfer (record_uid, revision) VALUES ('dependent', 1);
         INSERT INTO transfer_party (uid, transfer_uid, actor_uid)
             VALUES ('party', 'dependent', 'recipient');",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_dependency (uid, transfer_uid, revision, scope, upstream_kind,
                                         upstream_uid, upstream_origin_uid, upstream_reference_uid)
         VALUES ('dependency', 'dependent', 1, 'transfer', 'transfer', ?, ?, ?)",
    )
    .bind(&received.transfer_uid)
    .bind(&received.origin_organ_uid)
    .bind(&received.uid)
    .execute(pool)
    .await?;
    let outcome = async |at| {
        let mut connection = pool.acquire().await?;
        remote_on(&mut connection, "dependency", at).await
    };
    assert_eq!(
        outcome(now).await?.unavailable,
        Some("upstream_evidence_unavailable")
    );

    let mut accepted = envelope(&received, 1, now)?;
    accepted.projection = json!({
        "outcome": {"agreed": true, "settled": false, "revision": 1},
        "detail": "private fixture",
    });
    accepted.payload_hash = accepted.computed_payload_hash()?;
    accept_hosted_snapshot(pool, &received.uid, &accepted, now).await?;
    let exact = outcome(now + TimeDelta::seconds(120)).await?;
    assert!(exact.agreed);
    assert!(!exact.settled);
    assert_eq!(exact.unavailable, None);
    assert_eq!(
        exact.evidence.get("hash"),
        Some(&json!(accepted.payload_hash))
    );
    assert!(exact.evidence.get("detail").is_none());
    assert_eq!(
        outcome(now + TimeDelta::seconds(121)).await?.unavailable,
        Some("upstream_evidence_stale")
    );
    assert_eq!(
        outcome(now - TimeDelta::seconds(30)).await?.unavailable,
        None
    );
    assert_eq!(
        outcome(now - TimeDelta::seconds(31)).await?.unavailable,
        Some("upstream_evidence_stale")
    );

    accepted.created_at = "invalid-time".into();
    accepted.envelope_uid = "invalid-time-envelope".into();
    accepted.cursor = 2;
    accepted.payload_hash = accepted.computed_payload_hash()?;
    accept_hosted_snapshot(pool, &received.uid, &accepted, now).await?;
    assert_eq!(
        outcome(now).await?.unavailable,
        Some("upstream_evidence_time_invalid")
    );
    apply_remote_policy(
        pool,
        &received.uid,
        1,
        Hosted,
        false,
        None,
        "version-2",
        &json!({}),
        now,
    )
    .await?;
    assert_eq!(
        outcome(now).await?.unavailable,
        Some("upstream_evidence_revision_changed")
    );
    apply_remote_policy(
        pool,
        &received.uid,
        2,
        Hosted,
        true,
        None,
        "version-3",
        &json!({}),
        now,
    )
    .await?;
    assert_eq!(
        outcome(now).await?.unavailable,
        Some("upstream_access_revoked")
    );
    sqlx::query("DELETE FROM transfer_party WHERE uid = 'party'")
        .execute(pool)
        .await?;
    assert_eq!(
        outcome(now).await?.unavailable,
        Some("upstream_evidence_unavailable")
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_refreshes_replay_one_stored_request() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let url = format!(
        "sqlite://{}",
        directory.path().join("refresh.sqlite").display()
    );
    let store = Store::open_durable(&url).await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let reference = create_remote_reference(pool, reference(Hosted, &json!({})), now).await?;
    let writer = pool.begin_with("BEGIN IMMEDIATE").await?;
    let refresh = || enqueue_pull(pool, &reference.uid, "concurrent-refresh", now);
    let release = async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        writer.rollback().await
    };
    let (first, second, released) = tokio::join!(refresh(), refresh(), release);
    released?;
    assert_eq!(first?.uid, second?.uid);
    assert_eq!(pulls_due(pool, now, 2).await?.len(), 1);
    Ok(())
}
