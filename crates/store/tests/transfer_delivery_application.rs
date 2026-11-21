use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{
    RecordKind,
    execution::Execution,
    transfer::application::{EffectMode, PrivateEffect},
    transfer_delivery::{TRANSFER_ENVELOPE_VERSION, TransferApplicationAttestationV1},
};
use store::{
    Store, StoreError, exact, facts,
    records::{self, NewRecord},
    sqlx::{self, SqlitePool},
    transfer_accounting::{
        Binding, PolicyInput, amount, calculate, compensate, effective, set_policy,
    },
    transfer_delivery::{
        NewApplicationHandoff, NewLocalTransferApplication, NewRemoteApplicationHandoff,
        RemoteApplicationHandoffRow, accept_remote_application_handoff,
        application_attestation_mark_failed, application_attestation_mark_sent,
        application_attestations_due, application_effect_handoff, application_handoff_detail,
        apply_transfer_locally, create_application_handoff, enqueue_application_attestation,
        local_application_for_request, remote_application_handoff, store_application_attestation,
        store_application_handoff_detail,
    },
    transfer_effects,
};

mod transfer_occurrence_fixture;

fn time() -> Result<DateTime<Utc>, chrono::ParseError> {
    Ok(DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc())
}

fn sign(_: &str) -> Option<String> {
    Some("fixture-signature".into())
}

async fn local_stock(pool: &SqlitePool) -> Result<(String, String), StoreError> {
    let organ = sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-organ'")
        .fetch_one(pool)
        .await?;
    let stock = records::create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Stock",
            body: "",
            quantity: exact::zero(),
        },
    )
    .await?;
    Ok((organ, stock.uid))
}

async fn reviewed_application(
    pool: &SqlitePool,
    handoff: &RemoteApplicationHandoffRow,
    stock: &str,
) -> Result<NewLocalTransferApplication, StoreError> {
    let binding = Binding {
        transfer: &handoff.transfer_uid,
        exchange: "exchange",
        occurrence: Some(&handoff.occurrence_uid),
        person: &handoff.participant_person_uid,
        record: Some(stock),
        unit: None,
        outgoing: false,
    };
    let current = effective(pool, &binding).await?;
    let (delta, after) = calculate(&current.formula, amount(1.0)?, exact::zero())?;
    Ok(NewLocalTransferApplication {
        expected_effects_hash: None,
        exchange_uid: binding.exchange.into(),
        handoff_uid: handoff.uid.clone(),
        participant_person_uid: handoff.participant_person_uid.clone(),
        local_record_uid: stock.into(),
        local_delta: delta.to_f64(),
        local_cumulative_before: 0.0,
        local_cumulative_after: after.to_f64(),
        application_formula: current.formula,
        application_formula_hash: current.formula_hash,
        application_formula_version: current.version,
        authorization_intent_uid: None,
        request_id: "apply-slice".into(),
    })
}

#[tokio::test]
async fn origin_allocation_and_local_application_preserve_exact_review_and_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let store = transfer_occurrence_fixture::activated_transfer().await?;
    let pool = &store.pool;
    let now = time()?;
    let (organ, stock) = local_stock(pool).await?;
    sqlx::query("UPDATE promise SET party_uid = 'receiver' WHERE uid = 'promise'")
        .execute(pool)
        .await?;
    let origin = create_application_handoff(
        pool,
        NewApplicationHandoff {
            origin_organ_uid: &organ,
            participant_organ_uid: &organ,
            participant_person_uid: "receiver",
            transfer_uid: "transfer",
            occurrence_uid: "occurrence",
            settlement_slice_uid: "slice",
            origin_revision: 1,
            canonical_slice_hash: "canonical-hash",
            request_id: "origin-handoff",
            fact_uid: None,
        },
        now,
    )
    .await?;
    let detail = async |remaining| {
        store_application_handoff_detail(
            pool,
            &origin.uid,
            "promise",
            1.0,
            None,
            0.0,
            1.0,
            remaining,
            1,
            None,
            now,
        )
        .await
    };
    assert!(
        matches!(detail(0.5).await, Err(StoreError::Protocol(message)) if message.contains("allocation changed"))
    );
    assert_eq!(detail(0.0).await?.canonical_cumulative_after, 1.0);
    assert_eq!(detail(0.0).await?.handoff_uid, origin.uid);
    assert!(
        matches!(detail(0.5).await, Err(StoreError::Protocol(message)) if message.contains("changed during replay"))
    );
    let handoff = application_effect_handoff(pool, &origin.uid)
        .await?
        .expect("local handoff view");
    assert!(handoff.reference_uid.is_empty());

    let first = reviewed_application(pool, &handoff, &stock).await?;
    assert!(matches!(
        apply_transfer_locally(pool, first, now, |_| None).await,
        Err(StoreError::Protocol(message)) if message.contains("requires a Person signer")
    ));
    assert!(
        local_application_for_request(pool, "apply-slice")
            .await?
            .is_none()
    );
    let applied = apply_transfer_locally(
        pool,
        reviewed_application(pool, &handoff, &stock).await?,
        now,
        sign,
    )
    .await?;
    let replayed = apply_transfer_locally(
        pool,
        reviewed_application(pool, &handoff, &stock).await?,
        now,
        |_| None,
    )
    .await?;
    assert!(!applied.replayed);
    assert!(replayed.replayed);
    assert_eq!(applied.fact, replayed.fact);
    let stored = application_handoff_detail(pool, &origin.uid)
        .await?
        .expect("saved detail");
    assert_eq!(
        stored.origin_acceptance_fact_uid.as_deref(),
        Some(applied.fact.uid.as_str())
    );
    let state: String = sqlx::query_scalar("SELECT state FROM promise WHERE uid = 'promise'")
        .fetch_one(pool)
        .await?;
    assert_eq!(state, "kept");
    assert_eq!(
        records::quantity(pool, &stock)
            .await?
            .expect("stock")
            .to_f64(),
        1.0
    );
    Ok(())
}

fn incoming<'a>(organ: &'a str, at: &'a str) -> NewRemoteApplicationHandoff<'a> {
    NewRemoteApplicationHandoff {
        uid: "incoming",
        reference_uid: "reference",
        origin_organ_uid: "origin",
        participant_organ_uid: organ,
        participant_person_uid: "receiver",
        transfer_uid: "remote-transfer",
        occurrence_uid: "remote-occurrence",
        source_promise_uid: "promise",
        settlement_slice_uid: "slice",
        origin_revision: 1,
        canonical_quantity: 1.0,
        canonical_unit_uid: None,
        canonical_cumulative_before: 0.0,
        canonical_cumulative_after: 1.0,
        canonical_remaining_after: 0.0,
        application_direction: 1,
        canonical_slice_hash: "canonical-hash",
        envelope_uid: "envelope",
        envelope_payload_hash: "envelope-hash",
        origin_created_at: at,
    }
}

async fn seed_reference(pool: &SqlitePool, organ: &str, at: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO transfer_remote_reference
            (uid, origin_organ_uid, transfer_uid, delivery_policy_uid, recipient_person_uid,
             recipient_organ_uid, last_envelope_uid, last_payload_hash, created_at, updated_at)
         VALUES ('reference', 'origin', 'remote-transfer', 'policy', 'receiver',
                 ?1, 'envelope', 'envelope-hash', ?2, ?2)",
    )
    .bind(organ)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn incoming_handoff_checks_current_snapshot_and_exact_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = time()?;
    let at = now.to_rfc3339();
    let (organ, stock) = local_stock(pool).await?;
    seed_reference(pool, &organ, &at).await?;
    assert!(matches!(
        accept_remote_application_handoff(pool, NewRemoteApplicationHandoff {
            envelope_payload_hash: "wrong-hash", ..incoming(&organ, &at)
        }, now).await,
        Err(StoreError::Protocol(message)) if message.contains("verified reference")
    ));
    assert!(
        remote_application_handoff(pool, "incoming")
            .await?
            .is_none()
    );
    let saved = accept_remote_application_handoff(pool, incoming(&organ, &at), now).await?;
    assert_eq!(
        accept_remote_application_handoff(pool, incoming(&organ, &at), now)
            .await?
            .uid,
        saved.uid
    );
    assert!(matches!(
        accept_remote_application_handoff(pool, NewRemoteApplicationHandoff {
            canonical_remaining_after: 1.0, ..incoming(&organ, &at)
        }, now).await,
        Err(StoreError::Protocol(message)) if message.contains("changed contents")
    ));
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at)
         VALUES ('receiver', 'person', ?1, ?2, ?2)",
    )
    .bind(&organ)
    .bind(&at)
    .execute(pool)
    .await?;
    let (_, secondary) = local_stock(pool).await?;
    set_policy(
        pool,
        PolicyInput {
            transfer: saved.transfer_uid.clone(),
            exchange: "exchange".into(),
            person: "receiver".into(),
            effects: vec![
                PrivateEffect {
                    record: stock.clone(),
                    formula: "incoming()".into(),
                    mode: EffectMode::Quantity,
                },
                PrivateEffect {
                    record: secondary.clone(),
                    formula: "incoming() * 2".into(),
                    mode: EffectMode::Quantity,
                },
            ],
            expected_version: 0,
            request_id: "configure-effects".into(),
        },
        now,
        "fixture-key",
        "fixture-public-key",
        sign,
    )
    .await?;
    let binding = Binding {
        transfer: &saved.transfer_uid,
        exchange: "exchange",
        occurrence: Some(&saved.occurrence_uid),
        person: "receiver",
        record: Some(&stock),
        unit: None,
        outgoing: false,
    };
    sqlx::query(
        "CREATE TABLE simulation_assumed_transfer_effect (
            transfer_uid TEXT, exchange_uid TEXT, occurrence_uid TEXT, person_uid TEXT,
            record_uid TEXT, local_mantissa TEXT, local_scale INTEGER
         )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO simulation_assumed_transfer_effect VALUES
            ('remote-transfer', 'exchange', 'remote-occurrence', 'receiver', ?1, '25', 2),
            ('remote-transfer', 'exchange', NULL, 'receiver', ?2, '5', 1)",
    )
    .bind(&stock)
    .bind(&secondary)
    .execute(pool)
    .await?;
    let simulation = Execution::new([7; 32], now.timestamp_millis())?;
    let assumed = simulation
        .scope(transfer_effects::quote(pool, &binding, amount(1.0)?))
        .await?
        .expect("simulation effect group");
    assert_eq!(
        assumed
            .effects
            .iter()
            .map(|effect| effect.delta.to_f64())
            .collect::<Vec<_>>(),
        [0.75, 1.5]
    );
    let group = transfer_effects::quote(pool, &binding, amount(1.0)?)
        .await?
        .expect("reviewed effect group");
    assert_eq!(
        group
            .effects
            .iter()
            .map(|effect| effect.delta.to_f64())
            .collect::<Vec<_>>(),
        [1.0, 2.0]
    );
    let input = async || -> Result<_, StoreError> {
        Ok(NewLocalTransferApplication {
            expected_effects_hash: Some(group.hash.clone()),
            ..reviewed_application(pool, &saved, &stock).await?
        })
    };
    let applied = apply_transfer_locally(pool, input().await?, now, sign).await?;
    let replay = apply_transfer_locally(pool, input().await?, now, |_| None).await?;
    assert!(replay.replayed);
    assert_eq!(replay.fact, applied.fact);
    let children = transfer_effects::facts(pool, &applied.fact.uid).await?;
    let [child] = children.as_slice() else {
        panic!("exactly one secondary Fact should be recorded");
    };
    let mut canonical = child.clone();
    canonical.signature = Some("origin-signature".into());
    let mut tx = store::write_tx(pool).await?;
    facts::retain_origin(&mut tx, &canonical, "origin", "origin-cell").await?;
    tx.commit().await?;
    assert_eq!(
        transfer_effects::facts(pool, &applied.fact.uid).await?,
        [canonical]
    );
    let recorded = transfer_effects::recorded(pool, &applied.fact.uid).await?;
    let [primary_entry, secondary_entry] = recorded.as_slice() else {
        panic!("both saved effects should be recorded");
    };
    assert_eq!(primary_entry["fact"], applied.fact.uid);
    assert_eq!(secondary_entry["fact"], child.uid);
    let touched = transfer_effects::records(pool, &applied.fact.uid).await?;
    assert_eq!(touched.len(), 2);
    assert!(touched.contains(&stock) && touched.contains(&secondary));
    assert!(transfer_effects::protected(pool, &child.uid).await?);
    assert_eq!(
        records::quantity(pool, &secondary)
            .await?
            .expect("secondary stock")
            .to_f64(),
        2.0
    );
    let current = remote_application_handoff(pool, "incoming")
        .await?
        .expect("saved handoff");
    assert_eq!(current.state, "applied");
    assert_eq!(
        current.local_application_uid.as_deref(),
        Some(applied.application.uid.as_str())
    );

    sqlx::query("UPDATE record SET deleted_at = ? WHERE uid = ?")
        .bind(&at)
        .bind(&secondary)
        .execute(pool)
        .await?;
    assert!(matches!(
        compensate(pool, &applied.application.uid, "receiver", "correct", now, sign).await,
        Err(StoreError::Protocol(message)) if message.contains("restore every original private Record")
    ));
    assert_eq!(
        records::quantity(pool, &stock)
            .await?
            .expect("primary stock")
            .to_f64(),
        1.0
    );
    assert!(
        transfer_effects::correction_facts(pool, &applied.fact.uid)
            .await?
            .is_empty()
    );

    sqlx::query("UPDATE record SET deleted_at = NULL WHERE uid = ?")
        .bind(&secondary)
        .execute(pool)
        .await?;
    let (correction, replayed) = compensate(
        pool,
        &applied.application.uid,
        "receiver",
        "correct",
        now,
        sign,
    )
    .await?;
    assert!(!replayed);
    let corrections = transfer_effects::correction_facts(pool, &applied.fact.uid).await?;
    let [secondary_correction] = corrections.as_slice() else {
        panic!("exactly one secondary Fact should be compensated");
    };
    assert_eq!(
        secondary_correction.cause.uid.as_deref(),
        Some(child.uid.as_str())
    );
    assert_eq!(secondary_correction.delta.to_f64(), -2.0);
    assert!(transfer_effects::protected(pool, &correction.uid).await?);
    assert!(transfer_effects::protected(pool, &secondary_correction.uid).await?);
    for record in [&stock, &secondary] {
        assert!(
            records::quantity(pool, record)
                .await?
                .expect("compensated stock")
                .is_zero()
        );
    }
    assert_eq!(
        compensate(
            pool,
            &applied.application.uid,
            "receiver",
            "correct",
            now,
            |_| None
        )
        .await?,
        (correction, true)
    );
    Ok(())
}

#[tokio::test]
async fn sent_attestation_rolls_back_when_handoff_acceptance_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = time()?;
    let at = now.to_rfc3339();
    let (organ, stock) = local_stock(pool).await?;
    seed_reference(pool, &organ, &at).await?;
    let handoff = accept_remote_application_handoff(pool, incoming(&organ, &at), now).await?;
    let applied = apply_transfer_locally(
        pool,
        reviewed_application(pool, &handoff, &stock).await?,
        now,
        sign,
    )
    .await?;
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
        application_fact_uid: applied.fact.uid,
        applied_at: at,
        key_id: "key".into(),
        signature: "fixture-signature".into(),
    };
    let uid = store_application_attestation(pool, &attestation, now).await?;
    assert_eq!(
        store_application_attestation(pool, &attestation, now).await?,
        uid
    );
    let mut changed = attestation.clone();
    changed.signature = "different-signature".into();
    assert!(matches!(
        store_application_attestation(pool, &changed, now).await,
        Err(StoreError::Protocol(message)) if message.contains("changed contents")
    ));
    enqueue_application_attestation(
        pool,
        &handoff.uid,
        &handoff.reference_uid,
        &attestation.origin_organ_uid,
        &attestation,
        now,
    )
    .await?;
    application_attestation_mark_failed(pool, &uid, now, "offline").await?;
    assert!(application_attestations_due(pool, now, 1).await?.is_empty());

    sqlx::query(
        "CREATE TRIGGER fixture_reject_acceptance
         BEFORE UPDATE OF origin_state ON transfer_remote_application_handoff
         WHEN NEW.origin_state = 'accepted'
         BEGIN SELECT RAISE(ABORT, 'fixture acceptance error'); END",
    )
    .execute(pool)
    .await?;
    let retry = now + TimeDelta::seconds(5);
    assert!(
        application_attestation_mark_sent(pool, &uid, retry)
            .await
            .is_err()
    );
    let due = application_attestations_due(pool, retry, 1).await?;
    let [pending] = due.as_slice() else {
        panic!("the failed transaction should leave the attestation retryable");
    };
    assert_eq!(pending.payload, serde_json::to_string(&attestation)?);

    sqlx::query("DROP TRIGGER fixture_reject_acceptance")
        .execute(pool)
        .await?;
    application_attestation_mark_sent(pool, &uid, retry).await?;
    assert!(
        application_attestations_due(pool, retry, 1)
            .await?
            .is_empty()
    );
    let current = remote_application_handoff(pool, &handoff.uid)
        .await?
        .expect("saved handoff");
    assert_eq!(current.origin_state, "accepted");
    Ok(())
}

#[tokio::test]
async fn concurrent_applications_replay_the_same_signed_fact()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let url = format!(
        "sqlite://{}",
        directory.path().join("application.sqlite").display()
    );
    let store = Store::open(&url).await?;
    let pool = &store.pool;
    let now = time()?;
    let at = now.to_rfc3339();
    let (organ, stock) = local_stock(pool).await?;
    seed_reference(pool, &organ, &at).await?;
    let handoff = accept_remote_application_handoff(pool, incoming(&organ, &at), now).await?;
    let first = reviewed_application(pool, &handoff, &stock).await?;
    let second = reviewed_application(pool, &handoff, &stock).await?;
    let writer = pool.begin_with("BEGIN IMMEDIATE").await?;
    let release = async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        writer.rollback().await
    };
    let (first, second, released) = tokio::join!(
        apply_transfer_locally(pool, first, now, sign),
        apply_transfer_locally(pool, second, now, sign),
        release,
    );
    released?;
    let (first, second) = (first?, second?);
    assert_ne!(first.replayed, second.replayed);
    assert_eq!(first.fact, second.fact);
    assert_eq!(
        records::quantity(pool, &stock)
            .await?
            .expect("stock")
            .to_f64(),
        1.0
    );
    Ok(())
}
