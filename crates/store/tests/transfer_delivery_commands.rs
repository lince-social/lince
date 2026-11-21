use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{
    Cause, NewFact,
    fact::seal,
    simulation::RuleOccurrence,
    transfer_delivery::{TRANSFER_ENVELOPE_VERSION, TransferRemoteCommandV1},
};
use serde_json::{Value, json};
use store::{
    Store, StoreError, facts,
    karma_commands::{self, Origin},
    sqlx::{self, Sqlite, Transaction},
    transfer_delivery::{
        NewPackageReceipt, NewRemoteConflict, RemoteCommandCommit, finish_remote_command,
        link_pending_remote_command_fact, persist_remote_command,
        persist_remote_command_with_karma, record_package_receipt, record_remote_conflict,
        remote_command, remote_command_for_fact, remote_command_mark_failed,
        remote_command_mark_sent, remote_commands_due,
    },
};

fn time() -> Result<DateTime<Utc>, chrono::ParseError> {
    Ok(DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc())
}

fn command(uid: &str, request: &str, now: DateTime<Utc>) -> TransferRemoteCommandV1 {
    TransferRemoteCommandV1 {
        version: TRANSFER_ENVELOPE_VERSION,
        command_uid: uid.into(),
        request_id: request.into(),
        origin_organ_uid: "origin".into(),
        sender_organ_uid: "sender".into(),
        transfer_uid: "transfer".into(),
        expected_revision: Some(4),
        actor_person_uid: "actor".into(),
        key_id: "fixture-key".into(),
        public_key_base64: "fixture-public-key".into(),
        session_id: "session".into(),
        session_challenge: "challenge".into(),
        sequence: 1,
        message_id: "message".into(),
        action_base64: "fixture-action".into(),
        created_at: now.to_rfc3339(),
        signature: "fixture-signature".into(),
    }
}

#[tokio::test]
async fn outgoing_command_preserves_rule_origin_replay_and_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = time()?;
    let pending = command("outgoing", "request", now);
    let origin = Origin {
        rule: "rule".into(),
        revision: 1,
        request: "rule-request".into(),
        occurrence: RuleOccurrence {
            rule_uid: "rule".into(),
            revision: 1,
            event_id: "event".into(),
            frequency: None,
            intended_at_ms: None,
        },
        evaluation: json!({"step": "dispatch"}),
    };
    assert!(
        persist_remote_command_with_karma(pool, "outgoing", &pending, now, Some(&origin))
            .await
            .is_err()
    );
    assert!(remote_command(pool, &pending.command_uid).await?.is_none());
    sqlx::query(
        "INSERT INTO recurrence
            (uid, record_uid, consequences_json, cadence_json, anchor_at, state, revision, created_at, updated_at)
         SELECT 'rule', uid, '[]', '{}', ?1, 'armed', 1, ?1, ?1
         FROM record WHERE slug = 'local-organ'",
    )
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    let RemoteCommandCommit::Applied(created) =
        persist_remote_command_with_karma(pool, "outgoing", &pending, now, Some(&origin)).await?
    else {
        panic!("a new command should be applied");
    };
    assert_eq!(created.payload, serde_json::to_string(&pending)?);
    assert_eq!(created.payload_hash, pending.payload_hash());
    assert!(matches!(
        persist_remote_command_with_karma(pool, "outgoing", &pending, now, Some(&origin)).await?,
        RemoteCommandCommit::Replayed(row) if row.command_uid == pending.command_uid
    ));
    let mut changed = origin.clone();
    changed.occurrence.event_id = "different-event".into();
    assert!(matches!(
        persist_remote_command_with_karma(pool, "outgoing", &pending, now, Some(&changed)).await,
        Err(StoreError::Protocol(message)) if message.contains("origin changed")
    ));
    let collision = command("new-command", "request", now);
    assert!(matches!(
        persist_remote_command(pool, "outgoing", &collision, now).await,
        Err(StoreError::Protocol(message)) if message.contains("request id changed")
    ));

    sqlx::query(
        "INSERT INTO transfer_sync_owner (table_name, row_key, cell_uid)
         VALUES ('transfer_remote_command', json_array(?), 'another-cell')
         ON CONFLICT (table_name, row_key) DO UPDATE SET cell_uid = excluded.cell_uid",
    )
    .bind(&pending.command_uid)
    .execute(pool)
    .await?;
    assert!(remote_commands_due(pool, now, 5).await?.is_empty());
    sqlx::query(
        "UPDATE transfer_sync_owner SET cell_uid = (SELECT uid FROM record WHERE slug = 'local-cell')
         WHERE table_name = 'transfer_remote_command' AND row_key = json_array(?)",
    )
    .bind(&pending.command_uid)
    .execute(pool)
    .await?;
    assert_eq!(remote_commands_due(pool, now, 5).await?.len(), 1);
    let next = now + TimeDelta::seconds(5);
    assert_eq!(
        remote_command_mark_failed(
            pool,
            &pending.command_uid,
            now,
            "offline",
            "peer unavailable",
            5,
            30
        )
        .await?,
        next.to_rfc3339()
    );
    assert!(remote_commands_due(pool, now, 5).await?.is_empty());
    let due = remote_commands_due(pool, next, 5).await?;
    let [retry] = due.as_slice() else {
        panic!("one retry should be due");
    };
    assert_eq!(
        (retry.attempts, retry.last_error_code.as_deref()),
        (1, Some("offline"))
    );
    remote_command_mark_sent(pool, &pending.command_uid, next).await?;
    let sent = remote_command(pool, &pending.command_uid)
        .await?
        .expect("stored command");
    assert_eq!(
        (sent.status.as_str(), sent.attempts, sent.last_error_code),
        ("sent", 2, None)
    );

    let result = json!({"accepted": true});
    finish_remote_command(
        pool,
        &pending.command_uid,
        true,
        5,
        &result,
        None,
        None,
        now,
    )
    .await?;
    finish_remote_command(
        pool,
        &pending.command_uid,
        true,
        5,
        &result,
        None,
        None,
        next,
    )
    .await?;
    assert!(matches!(
        finish_remote_command(pool, &pending.command_uid, true, 6, &result, None, None, next).await,
        Err(StoreError::Protocol(message)) if message.contains("result changed")
    ));

    let cancelled = command("cancelled", "another-request", now);
    persist_remote_command_with_karma(pool, "outgoing", &cancelled, now, Some(&origin)).await?;
    karma_commands::cancel(pool, &cancelled.command_uid, "cancelled by fixture").await?;
    assert!(remote_commands_due(pool, next, 5).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn incoming_fact_link_checks_actor_direction_and_terminal_state()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = time()?;
    let incoming = command("incoming", "incoming-request", now);
    let outgoing = command("outgoing", "outgoing-request", now);
    persist_remote_command(pool, "incoming", &incoming, now).await?;
    persist_remote_command(pool, "outgoing", &outgoing, now).await?;
    let record: String = sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-organ'")
        .fetch_one(pool)
        .await?;
    let mut tx = store::write_tx(pool).await?;
    let fact = seal(
        NewFact {
            actor_uid: Some(incoming.actor_person_uid.clone()),
            ..NewFact::quantity(record, store::exact::zero(), Cause::user_edit())
        },
        &facts::last_hash(&mut tx).await?,
        now,
    );
    facts::insert(&mut tx, &fact).await?;
    assert!(matches!(
        link_pending_remote_command_fact(&mut tx, &outgoing.command_uid, &fact).await,
        Err(StoreError::Protocol(message)) if message.contains("actor or state")
    ));
    let mut wrong_actor = fact.clone();
    wrong_actor.actor_uid = Some("different-actor".into());
    assert!(matches!(
        link_pending_remote_command_fact(&mut tx, &incoming.command_uid, &wrong_actor).await,
        Err(StoreError::Protocol(message)) if message.contains("actor or state")
    ));
    link_pending_remote_command_fact(&mut tx, &incoming.command_uid, &fact).await?;
    tx.commit().await?;
    finish_remote_command(
        pool,
        &incoming.command_uid,
        false,
        4,
        &json!({}),
        Some("refused"),
        None,
        now,
    )
    .await?;
    let linked = remote_command_for_fact(pool, &fact.uid)
        .await?
        .expect("stored Fact link");
    assert_eq!(
        (linked.command_uid, linked.status.as_str()),
        (incoming.command_uid.clone(), "rejected")
    );
    let mut tx = store::write_tx(pool).await?;
    assert!(matches!(
        link_pending_remote_command_fact(&mut tx, &incoming.command_uid, &fact).await,
        Err(StoreError::Protocol(message)) if message.contains("actor or state")
    ));
    tx.rollback().await?;
    Ok(())
}

fn receipt<'a>(request: &'a str, envelope: &'a str, payload: &'a Value) -> NewPackageReceipt<'a> {
    NewPackageReceipt {
        delivery_uid: "delivery",
        envelope_uid: envelope,
        cursor: 1,
        kind: "received",
        actor_organ_uid: "recipient-organ",
        payload_hash: "payload-hash",
        key_id: "fixture-key",
        signature: "fixture-signature",
        signed_payload: payload,
        local_fact_uid: None,
        request_id: request,
    }
}

#[tokio::test]
async fn receipt_replay_checks_request_identity_before_the_payload_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = time()?;
    let proof = json!({"receipt": "original"});
    let original = record_package_receipt(pool, receipt("request", "first", &proof), now).await?;
    assert_eq!(
        record_package_receipt(pool, receipt("request", "first", &proof), now).await?,
        original
    );
    assert_eq!(
        record_package_receipt(pool, receipt("alias-request", "first", &proof), now).await?,
        original
    );
    assert!(matches!(
        record_package_receipt(pool, receipt("alias-request", "first", &json!({"receipt":"changed"})), now).await,
        Err(StoreError::Protocol(message)) if message.contains("identity was replayed")
    ));
    record_package_receipt(pool, receipt("second-request", "second", &proof), now).await?;
    assert!(matches!(
        record_package_receipt(pool, receipt("second-request", "first", &proof), now).await,
        Err(StoreError::Protocol(message)) if message.contains("request id was replayed")
    ));
    let stored: Vec<String> =
        sqlx::query_scalar("SELECT signed_payload FROM transfer_delivery_receipt")
            .fetch_all(pool)
            .await?;
    assert_eq!(
        stored,
        [
            serde_json::to_string(&proof)?,
            serde_json::to_string(&proof)?
        ]
    );
    Ok(())
}

#[tokio::test]
async fn conflict_preserves_reviewed_evidence_and_request_uniqueness()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = time()?;
    let reviewed = json!({"provided_revision":4});
    let conflict = || NewRemoteConflict {
        origin_organ_uid: "origin",
        transfer_uid: "transfer",
        recipient_person_uid: "recipient",
        command_uid: Some("command"),
        request_id: Some("request"),
        envelope_uid: None,
        submitted_revision: Some(4),
        authoritative_revision: 5,
        authoritative_cursor: 2,
        code: "stale",
        reviewed_payload: &reviewed,
    };
    let uid = record_remote_conflict(pool, conflict(), now).await?;
    assert_eq!(record_remote_conflict(pool, conflict(), now).await?, uid);
    assert!(matches!(
        record_remote_conflict(pool, NewRemoteConflict { authoritative_cursor: 3, ..conflict() }, now).await,
        Err(StoreError::Protocol(message)) if message.contains("changed contents")
    ));
    assert!(matches!(
        record_remote_conflict(pool, NewRemoteConflict { command_uid: None, request_id: None, ..conflict() }, now).await,
        Err(StoreError::Protocol(message)) if message.contains("requires a command")
    ));
    let request_only = || NewRemoteConflict {
        command_uid: None,
        request_id: Some("request-only"),
        ..conflict()
    };
    record_remote_conflict(pool, request_only(), now).await?;
    assert!(matches!(
        record_remote_conflict(pool, request_only(), now).await,
        Err(StoreError::Database(error)) if error.is_unique_violation()
    ));
    let stored: String =
        sqlx::query_scalar("SELECT reviewed_payload FROM transfer_remote_conflict WHERE uid = ?")
            .bind(&uid)
            .fetch_one(pool)
            .await?;
    assert_eq!(stored, serde_json::to_string(&reviewed)?);
    Ok(())
}

async fn release_writer(writer: Transaction<'_, Sqlite>) -> Result<(), StoreError> {
    tokio::time::sleep(Duration::from_millis(150)).await;
    writer.rollback().await
}

#[tokio::test]
async fn concurrent_identical_results_finish_once() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let url = format!(
        "sqlite://{}",
        directory.path().join("command.sqlite").display()
    );
    let store = Store::open_durable(&url).await?;
    let pool = &store.pool;
    let now = time()?;
    let command = command("command", "request", now);
    persist_remote_command(pool, "outgoing", &command, now).await?;
    let result = json!({"accepted": true});
    let submit = || {
        finish_remote_command(
            pool,
            &command.command_uid,
            true,
            5,
            &result,
            None,
            None,
            now,
        )
    };
    let writer = pool.begin_with("BEGIN IMMEDIATE").await?;
    let (first, second, released) = tokio::join!(submit(), submit(), release_writer(writer));
    released?;
    first?;
    second?;
    let saved = remote_command(pool, &command.command_uid)
        .await?
        .expect("saved command");
    assert_eq!(saved.status, "accepted");
    assert_eq!(
        saved.result_payload.as_deref(),
        Some(r#"{"accepted":true}"#)
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_identical_receipts_keep_one_proof() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let url = format!(
        "sqlite://{}",
        directory.path().join("receipt.sqlite").display()
    );
    let store = Store::open_durable(&url).await?;
    let pool = &store.pool;
    let now = time()?;
    let proof = json!({"acknowledged": true});
    let submit = || record_package_receipt(pool, receipt("request", "envelope", &proof), now);
    let writer = pool.begin_with("BEGIN IMMEDIATE").await?;
    let (first, second, released) = tokio::join!(submit(), submit(), release_writer(writer));
    released?;
    assert_eq!(first?, second?);
    Ok(())
}
