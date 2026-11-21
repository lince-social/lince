use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::DateTime;
use nucleus::{
    RecordKind,
    transfer::{
        OccurrenceClaimRole::{Delivery, Receipt},
        TransferActivationEvidence, TransferBulkClaimReviewItem, TransferOccurrenceClaimEvidence,
        TransferRemainderPolicy, occurrence_application_formula_hash,
        transfer_bulk_claim_review_token,
    },
};
use serde_json::json;
use store::{
    exact::zero,
    records::{self, NewRecord},
    sqlx::{self, Error},
    transfers::{
        ActivateOccurrencesInput, BulkOccurrenceClaimCommit, BulkOccurrenceClaimInput,
        OccurrenceActivationCommit, OccurrenceActivationInput, OccurrenceApplicationFormulaCommit,
        OccurrenceApplicationFormulaInput, OccurrenceClaimCommit, OccurrenceClaimInput,
        OccurrenceSettlementCommit, OccurrenceSettlementCompensationCommit,
        OccurrenceSettlementCompensationInput, OccurrenceSettlementInput,
        ReviewedBulkOccurrenceClaim, activate_occurrences, compensate_occurrence_settlement,
        complete_occurrence_claims_bulk, occurrence, occurrence_application_events,
        occurrence_application_policy, occurrence_claim_event_for_request, occurrence_for_promise,
        occurrence_settlement_for_application_fact, occurrence_settlement_progress,
        occurrences_for_activation_request, occurrences_of, phase6_bulk_request_for_request,
        set_occurrence_application_formula, set_occurrence_claim, settle_occurrence,
        source_group_result, source_group_state_for_transfer,
    },
};

mod transfer_occurrence_fixture;

#[tokio::test]
async fn settlement_compensation_and_source_group_preserve_the_fact_chain()
-> Result<(), Box<dyn std::error::Error>> {
    let store = transfer_occurrence_fixture::activated_transfer().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let sign = |_: &str| Some("signature".into());
    let record = records::create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Local settlement quantity",
            body: "",
            quantity: zero(),
        },
    )
    .await?;
    sqlx::raw_sql(
        "UPDATE promise SET party_uid = 'receiver', state = 'active' WHERE uid = 'promise';
         UPDATE promise SET state = 'withdrawn' WHERE uid = 'second-promise';
         UPDATE transfer_occurrence SET delivery_claimed = 1, receipt_claimed = 1
             WHERE uid = 'occurrence';
         INSERT INTO record (uid, kind, organ_uid, created_at, updated_at) VALUES
             ('sibling-z', 'transfer', 'organ', 'fixture', 'fixture'),
             ('sibling-a', 'transfer', 'organ', 'fixture', 'fixture');
         INSERT INTO transfer (record_uid, revision, satiation) VALUES
             ('sibling-z', 1, 'first_completes'), ('sibling-a', 1, 'first_completes');",
    )
    .execute(pool)
    .await?;
    sqlx::query("UPDATE transfer SET source_uid = ?, satiation = 'first_completes'")
        .bind(&record.uid)
        .execute(pool)
        .await?;

    let first = OccurrenceSettlementInput {
        expected_effects_hash: None,
        occurrence_uid: "occurrence".into(),
        idempotency_key: "settle-part".into(),
        actor_person_uid: "receiver".into(),
        canonical_quantity: 0.25,
        local_record_uid: record.uid.clone(),
        local_delta: 0.25,
        local_cumulative_after: 0.25,
        application_formula_hash: occurrence_application_formula_hash("incoming()"),
        application_formula: "incoming()".into(),
        application_formula_version: 0,
        remainder_policy: TransferRemainderPolicy::Visible,
        authorization_intent_uid: None,
    };
    assert!(!format!("{first:?}").contains("incoming()"));
    let OccurrenceSettlementCommit::Committed(partial) =
        settle_occurrence(pool, first.clone(), now, sign).await?
    else {
        panic!("the first slice should commit");
    };
    assert!(!format!("{partial:?}").contains("incoming()"));
    assert_eq!(
        partial.application_fact.prev_hash,
        partial.evidence_fact.hash
    );
    assert!(partial.progress.partially_settled);
    assert!(partial.satiation_facts.is_empty());
    assert_eq!(partial.progress.remaining_quantity, 0.75);
    let OccurrenceSettlementCommit::Replayed(replay) =
        settle_occurrence(pool, first.clone(), now, |_| None).await?
    else {
        panic!("the same settlement request should replay");
    };
    assert_eq!(replay.application_fact, partial.application_fact);
    let by_fact = occurrence_settlement_for_application_fact(pool, &partial.application_fact.uid)
        .await?
        .expect("slice for application fact");
    assert_eq!(by_fact.uid, partial.slice.uid);

    let correction = OccurrenceSettlementCompensationInput {
        settlement_uid: partial.slice.uid.clone(),
        idempotency_key: "correct-part".into(),
        actor_person_uid: "receiver".into(),
        authorization_intent_uid: None,
    };
    let OccurrenceSettlementCompensationCommit::Committed(compensated) =
        compensate_occurrence_settlement(pool, correction.clone(), now, sign).await?
    else {
        panic!("the original application should be compensated");
    };
    assert_eq!(compensated.fact.prev_hash, partial.application_fact.hash);
    assert_eq!(compensated.fact.delta.to_f64(), -0.25);
    let quantity = records::quantity(pool, &record.uid)
        .await?
        .expect("local Record");
    assert!(quantity.is_zero());
    let progress = occurrence_settlement_progress(pool, "occurrence")
        .await?
        .expect("public settlement progress");
    assert_eq!(progress.settled_quantity, 0.25);
    assert!(matches!(
        compensate_occurrence_settlement(pool, correction, now, |_| None).await?,
        OccurrenceSettlementCompensationCommit::Replayed(replay) if replay.fact == compensated.fact
    ));

    let final_slice = OccurrenceSettlementInput {
        idempotency_key: "settle-rest".into(),
        canonical_quantity: 0.75,
        local_delta: 0.75,
        local_cumulative_after: 1.0,
        ..first
    };
    let signatures = AtomicUsize::new(0);
    let failed = settle_occurrence(pool, final_slice.clone(), now, |_| {
        (signatures.fetch_add(1, Ordering::Relaxed) != 2).then(|| "signature".into())
    })
    .await;
    assert!(matches!(
        failed,
        Err(Error::Protocol(message)) if message.contains("source-group result requires")
    ));
    assert!(source_group_result(pool, &record.uid).await?.is_none());
    let progress = occurrence_settlement_progress(pool, "occurrence")
        .await?
        .expect("public settlement progress");
    assert_eq!(
        progress.settled_quantity, 0.25,
        "failed signing rolls back the final slice"
    );
    let OccurrenceSettlementCommit::Committed(complete) =
        settle_occurrence(pool, final_slice, now, sign).await?
    else {
        panic!("the remaining canonical quantity should settle");
    };
    assert!(complete.progress.settled);
    assert_eq!(complete.evidence_fact.prev_hash, compensated.fact.hash);
    let [winner, first_sibling, second_sibling] = complete.satiation_facts.as_slice() else {
        panic!("the source group should write one winner and two sibling facts");
    };
    assert_eq!(winner.prev_hash, complete.application_fact.hash);
    assert_eq!(first_sibling.prev_hash, winner.hash);
    assert_eq!(second_sibling.prev_hash, first_sibling.hash);
    assert_eq!(first_sibling.record_uid, "sibling-a");
    assert_eq!(second_sibling.record_uid, "sibling-z");
    let result = source_group_result(pool, &record.uid)
        .await?
        .expect("source group");
    assert_eq!(result.fact_uid, winner.uid);
    let state = source_group_state_for_transfer(pool, "sibling-z")
        .await?
        .expect("sibling group state");
    assert!(state.satiated);
    assert_eq!(
        state.loser.expect("losing sibling evidence").fact_uid,
        second_sibling.uid
    );
    Ok(())
}

#[tokio::test]
async fn activation_requires_signed_terms_and_replays_the_exact_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let store = transfer_occurrence_fixture::pending_transfer().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let sign = |_: &str| Some("signature".into());
    sqlx::raw_sql(
        "UPDATE transfer SET agreement_type = 'full' WHERE record_uid = 'transfer';
         UPDATE promise SET party_uid = 'receiver', state = 'agreed' WHERE transfer_uid = 'transfer';
         INSERT INTO promise
             (uid, record_uid, delta, transfer_uid, party_uid, state, created_at, updated_at)
             VALUES ('opposite-promise', 'transfer', -1, 'transfer', 'giver', 'agreed', 'fixture', 'fixture');
         INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at, revision) VALUES
             ('agreed-giver', 'transfer', 'giver-party', 2, 'fixture', 1),
             ('agreed-receiver', 'transfer', 'receiver-party', 2, 'fixture', 1);",
    )
    .execute(pool)
    .await?;
    let promises = [
        ("promise", 1),
        ("second-promise", 1),
        ("opposite-promise", -1),
    ]
    .map(|(uid, delta)| {
        json!({"uid": uid, "revision": 1, "delta": delta, "reserve_from": "active", "state": "agreed"})
    });
    let evidence = json!({
        "action": "publish-transfer",
        "terms": {
            "revision": 1, "transfer": {
                "uid": "transfer", "head": "Transfer", "agreement_type": "full",
                "settlement": "individual", "visibility": "hidden", "require_confirmation": false
            },
            "parties": [], "invitations": [], "promises": promises
        }
    });
    sqlx::query(
        "INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, cause_kind,
                          prev_hash, hash, payload)
         VALUES ('revision-fact', 'transfer', '0', 0, '2026-01-01T00:00:00Z', 'user_edit',
                 '', 'revision-hash', ?)",
    )
    .bind(evidence.to_string())
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_revision (transfer_uid, revision, fact_uid, created_at)
         VALUES ('transfer', 1, 'revision-fact', 'fixture')",
    )
    .execute(pool)
    .await?;

    let mut input = ActivateOccurrencesInput {
        transfer_uid: "transfer".into(),
        expected_revision: 1,
        idempotency_key: "activate".into(),
        actor_person_uid: "receiver".into(),
        occurrences: ["promise", "second-promise"]
            .map(|promise_uid| OccurrenceActivationInput {
                promise_uid: promise_uid.into(),
                opposite_promise_uid: (promise_uid == "promise").then(|| "opposite-promise".into()),
                giver_person_uid: "giver".into(),
                receiver_person_uid: "receiver".into(),
            })
            .into(),
        original_authority_records: Vec::new(),
        authorization_intent_uid: None,
    };
    let stale = ActivateOccurrencesInput {
        expected_revision: 2,
        ..input.clone()
    };
    assert!(matches!(
        activate_occurrences(pool, stale, now, sign).await?,
        OccurrenceActivationCommit::Stale {
            current_revision: 1,
            ..
        }
    ));
    let unsigned = activate_occurrences(pool, input.clone(), now, sign).await;
    assert!(
        matches!(unsigned, Err(Error::Protocol(message)) if message.contains("signed Transfer"))
    );
    sqlx::query("UPDATE fact SET signature = 'signature' WHERE uid = 'revision-fact'")
        .execute(pool)
        .await?;

    let mut invalid = input.clone();
    invalid
        .occurrences
        .last_mut()
        .expect("second path")
        .giver_person_uid = "outsider".into();
    assert!(matches!(
        activate_occurrences(pool, invalid, now, sign).await,
        Err(Error::Protocol(message)) if message.contains("accepted Transfer parties")
    ));
    let paths: i64 = sqlx::query_scalar("SELECT count(*) FROM transfer_exchange_path")
        .fetch_one(pool)
        .await?;
    assert_eq!(paths, 0, "a failed second path rolls back the first");
    assert!(
        occurrences_for_activation_request(pool, "activate")
            .await?
            .is_none()
    );

    let OccurrenceActivationCommit::Committed(committed) =
        activate_occurrences(pool, input.clone(), now, sign).await?
    else {
        panic!("the signed and agreed paths should activate");
    };
    let signed: TransferActivationEvidence = serde_json::from_str(
        committed
            .fact
            .payload
            .as_deref()
            .expect("activation evidence"),
    )?;
    let rows = occurrences_of(pool, "transfer").await?;
    assert_eq!(rows.len(), 2);
    for (row, snapshot) in rows.iter().zip(&signed.occurrences) {
        assert_eq!(row.uid, snapshot.uid);
        assert_eq!(row.promise_uid, snapshot.promise_uid);
        assert_eq!(
            row.opposite_promise_uid.as_deref(),
            (row.promise_uid == "promise").then_some("opposite-promise")
        );
        let by_promise = occurrence_for_promise(pool, &row.promise_uid)
            .await?
            .expect("activated promise occurrence");
        assert_eq!(by_promise.uid, row.uid);
    }
    assert_eq!(signed.occurrences.len(), rows.len());

    input.occurrences.reverse();
    let OccurrenceActivationCommit::Replayed(replay) =
        activate_occurrences(pool, input.clone(), now, |_| None).await?
    else {
        panic!("reordering the same paths should replay the request");
    };
    assert_eq!(replay.fact, committed.fact);
    let [first, second] = input.occurrences.as_mut_slice() else {
        panic!("the request should have two paths");
    };
    *second = first.clone();
    assert!(matches!(
        activate_occurrences(pool, input, now, sign).await,
        Err(Error::Protocol(message)) if message.contains("different targets")
    ));
    Ok(())
}

#[tokio::test]
async fn single_and_bulk_claims_preserve_roles_order_and_atomicity()
-> Result<(), Box<dyn std::error::Error>> {
    let store = transfer_occurrence_fixture::activated_transfer().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let sign = |_: &str| Some("signature".into());
    let mut single = OccurrenceClaimInput {
        occurrence_uid: "occurrence".into(),
        idempotency_key: "single".into(),
        actor_person_uid: "receiver".into(),
        role: Delivery,
        asserted: true,
        authorization_intent_uid: None,
    };
    assert!(matches!(
        set_occurrence_claim(pool, single.clone(), now, sign).await,
        Err(Error::Protocol(_))
    ));
    single.role = Receipt;
    let OccurrenceClaimCommit::Committed(received) =
        set_occurrence_claim(pool, single, now, sign).await?
    else {
        panic!("the receiver's claim should commit");
    };
    assert!(received.occurrence.receipt_claimed);
    assert!(!received.occurrence.delivery_claimed);

    let review = [("second-occurrence", false), ("occurrence", true)].map(
        |(occurrence_uid, receipt_claimed)| TransferBulkClaimReviewItem {
            occurrence_uid: occurrence_uid.into(),
            transfer_uid: "transfer".into(),
            transfer_revision: 1,
            role: Delivery,
            delivery_claimed: false,
            receipt_claimed,
        },
    );
    let request = |review: &[TransferBulkClaimReviewItem]| BulkOccurrenceClaimInput {
        idempotency_key: "bulk".into(),
        actor_person_uid: "giver".into(),
        review_token: transfer_bulk_claim_review_token("giver", review),
        items: review
            .iter()
            .map(|item| ReviewedBulkOccurrenceClaim {
                occurrence_uid: item.occurrence_uid.clone(),
                transfer_uid: item.transfer_uid.clone(),
                expected_revision: item.transfer_revision,
                role: item.role,
                expected_delivery_claimed: item.delivery_claimed,
                expected_receipt_claimed: item.receipt_claimed,
            })
            .collect(),
        authorization_intent_uid: None,
    };
    let [duplicate, _] = &review;
    let rejected = complete_occurrence_claims_bulk(
        pool,
        request(&[duplicate.clone(), duplicate.clone()]),
        now,
        sign,
    )
    .await?;
    assert!(matches!(
        rejected,
        BulkOccurrenceClaimCommit::Rejected(failures)
            if matches!(failures.as_slice(), [failure] if failure.code == "duplicate_item")
    ));

    let bulk = request(&review);
    let signed = AtomicUsize::new(0);
    let failed = complete_occurrence_claims_bulk(pool, bulk.clone(), now, |_| {
        (signed.fetch_add(1, Ordering::Relaxed) == 0).then(|| "signature".into())
    })
    .await;
    assert!(matches!(failed, Err(Error::Protocol(_))));
    assert!(
        phase6_bulk_request_for_request(pool, "bulk")
            .await?
            .is_none()
    );
    assert!(
        occurrence_claim_event_for_request(pool, "bulk/0")
            .await?
            .is_none()
    );

    let BulkOccurrenceClaimCommit::Committed(committed) =
        complete_occurrence_claims_bulk(pool, bulk.clone(), now, sign).await?
    else {
        panic!("the same review should commit after rollback");
    };
    let [first, second] = committed.facts.as_slice() else {
        panic!("each occurrence should have a claim fact");
    };
    assert_eq!(first.prev_hash, received.fact.hash);
    assert_eq!(second.prev_hash, first.hash);
    for (ordinal, (fact, occurrence_uid)) in [(first, "occurrence"), (second, "second-occurrence")]
        .into_iter()
        .enumerate()
    {
        let evidence: TransferOccurrenceClaimEvidence =
            serde_json::from_str(fact.payload.as_deref().expect("claim evidence"))?;
        assert_eq!(evidence.occurrence_uid, occurrence_uid);
        assert_eq!(evidence.idempotency_key, format!("bulk/{ordinal}"));
        assert!(
            occurrence(pool, occurrence_uid)
                .await?
                .expect("claimed occurrence")
                .delivery_claimed
        );
    }
    let BulkOccurrenceClaimCommit::Replayed(replay) =
        complete_occurrence_claims_bulk(pool, bulk, now, sign).await?
    else {
        panic!("the matching bulk claim should replay");
    };
    assert_eq!(replay.uid, committed.uid);
    assert_eq!(replay.facts, committed.facts);
    Ok(())
}

#[tokio::test]
async fn formula_replay_keeps_the_original_evidence_and_current_private_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let store = transfer_occurrence_fixture::activated_transfer().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let sign = |_: &str| Some("signature".into());
    let mut request = OccurrenceApplicationFormulaInput {
        occurrence_uid: "occurrence".into(),
        idempotency_key: "first".into(),
        actor_person_uid: "giver".into(),
        formula: "  private formula one  ".into(),
        authorization_intent_uid: None,
    };
    assert!(!format!("{request:?}").contains("private formula one"));
    assert!(matches!(
        set_occurrence_application_formula(pool, request.clone(), now, sign).await,
        Err(Error::Protocol(_))
    ));
    request.actor_person_uid = "receiver".into();
    let OccurrenceApplicationFormulaCommit::Committed(first) =
        set_occurrence_application_formula(pool, request.clone(), now, sign).await?
    else {
        panic!("the receiver's formula should commit");
    };
    assert_eq!(first.policy.formula, "private formula one");
    assert!(!format!("{first:?}").contains("private formula one"));
    assert!(
        !first
            .fact
            .payload
            .as_deref()
            .expect("formula evidence")
            .contains("private formula one")
    );
    assert!(
        occurrence_application_policy(pool, "occurrence", "giver")
            .await?
            .is_none()
    );

    let next = OccurrenceApplicationFormulaInput {
        idempotency_key: "second".into(),
        formula: "private formula two".into(),
        ..request.clone()
    };
    set_occurrence_application_formula(pool, next, now, sign).await?;
    let OccurrenceApplicationFormulaCommit::Replayed(replay) =
        set_occurrence_application_formula(pool, request, now, sign).await?
    else {
        panic!("the original formula should replay");
    };
    assert_eq!(replay.event.uid, first.event.uid);
    assert_eq!(replay.event.version, 1);
    assert_eq!(replay.policy.version, 2);
    assert_eq!(replay.policy.formula, "private formula two");
    assert_eq!(
        occurrence_application_events(pool, "occurrence")
            .await?
            .len(),
        2
    );
    Ok(())
}
