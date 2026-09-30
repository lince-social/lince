use engine::{Engine, sync::OpBatch, trust::Signer};
use nucleus::{Cause, CauseKind, NewFact, RecordKind};

#[tokio::test]
async fn relayed_facts_keep_original_signatures_and_correction_evidence() {
    let source = Engine::open_memory().await.unwrap();
    let relay = Engine::open_memory().await.unwrap();
    let recipient = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&source.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let signer = Signer::generate(&organ, "source-key");
    source.set_signer(signer.clone()).await.unwrap();
    let introduction = source.introduction().await.unwrap();
    relay.adopt_introduction(&introduction, 1).await.unwrap();
    recipient
        .adopt_introduction(&introduction, 1)
        .await
        .unwrap();
    let record = store::records::create(
        &source.store.pool,
        store::records::NewRecord {
            slug: Some("relayed-stock"),
            kind: RecordKind::Plain,
            head: "Relayed stock",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let credit = source
        .append(
            NewFact::quantity(
                record.clone(),
                nucleus::DecimalValue::parse_canonical(2, "1.50").unwrap(),
                Cause::user_edit(),
            ),
            chrono::Utc::now(),
        )
        .await
        .unwrap()
        .remove(0);
    let correction = source
        .append(
            NewFact::quantity(
                record.clone(),
                nucleus::DecimalValue::parse_canonical(2, "-0.50").unwrap(),
                Cause {
                    kind: CauseKind::Compensation,
                    uid: Some(credit.uid.clone()),
                },
            ),
            chrono::Utc::now(),
        )
        .await
        .unwrap()
        .remove(0);
    let batch = OpBatch {
        from_organ: organ.clone(),
        ops: source.ops_after(0, 10_000).await.unwrap().0,
    };
    relay.import_op_batch(&batch).await.unwrap();
    for original in [&credit, &correction] {
        let retained = store::facts::get(&relay.store.pool, &original.uid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained.hash, original.hash);
        assert_eq!(retained.signature, original.signature);
        assert_eq!(retained.cause, original.cause);
        assert!(nucleus::fact::verify_chain_step(&retained));
        assert!(
            engine::trust::verify_fact(&relay.store, &retained)
                .await
                .unwrap()
        );
    }
    let ledger = store::facts::for_record(&relay.store.pool, &record, 10)
        .await
        .unwrap();
    let imported: Vec<_> = ledger
        .iter()
        .filter(|fact| fact.uid == credit.uid || fact.uid == correction.uid)
        .collect();
    assert_eq!(imported.len(), 2);
    assert!(imported.iter().all(|fact| fact.cause.kind == CauseKind::Sync));
    assert!(imported.iter().all(|fact| fact.signature.is_none()));
    assert!(ledger.iter().all(nucleus::fact::verify_chain_step));
    let rows = store::sync_ops::after(&relay.store.pool, &organ, 0, 10_000)
        .await
        .unwrap();
    let relayed = OpBatch {
        from_organ: organ,
        ops: relay.hydrate_ops(rows).await.unwrap(),
    };
    recipient.import_op_batch(&relayed).await.unwrap();
    assert_eq!(recipient.import_op_batch(&relayed).await.unwrap(), 0);
    assert_eq!(
        store::organs::quarantine_count(&recipient.store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        store::records::quantity(&recipient.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .to_f64(),
        1.0
    );
    let final_correction = store::facts::get(&recipient.store.pool, &correction.uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_correction.hash, correction.hash);
    assert_eq!(
        final_correction.cause.uid.as_deref(),
        Some(credit.uid.as_str())
    );
    assert!(
        engine::trust::verify_fact(&recipient.store, &final_correction)
            .await
            .unwrap()
    );
    let mut conflict = relayed.clone();
    let op = conflict
        .ops
        .iter_mut()
        .find(|op| op.uid == credit.uid && op.kind == "fact")
        .unwrap();
    op.fact.as_mut().unwrap().delta = store::exact::integer(99);
    recipient.import_op_batch(&conflict).await.unwrap();
    assert_eq!(
        store::records::quantity(&recipient.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .to_f64(),
        1.0
    );
    assert_eq!(
        store::facts::get(&recipient.store.pool, &credit.uid)
            .await
            .unwrap()
            .unwrap()
            .hash,
        credit.hash
    );
    let mut contradictory = relayed.clone();
    let op = contradictory
        .ops
        .iter_mut()
        .find(|op| op.uid == credit.uid && op.kind == "fact")
        .unwrap();
    let mut changed = nucleus::fact::seal(
        NewFact {
            uid: Some(credit.uid.clone()),
            record_uid: record.clone(),
            delta: store::exact::integer(99),
            at: Some(credit.at),
            actor_uid: credit.actor_uid.clone(),
            cause: credit.cause.clone(),
            payload: credit.payload.clone(),
        },
        &credit.prev_hash,
        credit.at,
    );
    changed.signature = Some(signer.sign_bytes(changed.hash.as_bytes()));
    op.fact = Some(changed);
    let error = recipient.import_op_batch(&contradictory).await.unwrap_err();
    assert!(
        error.to_string().contains("different original evidence"),
        "{error}"
    );
    assert_eq!(
        store::records::quantity(&recipient.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .to_f64(),
        1.0
    );
    assert!(
        store::sqlx::query("DELETE FROM fact_origin WHERE fact_uid = ?")
            .bind(&credit.uid)
            .execute(&recipient.store.pool)
            .await
            .is_err()
    );
    assert_eq!(
        store::facts::delete_by_uids(&recipient.store.pool, std::slice::from_ref(&credit.uid))
            .await
            .unwrap(),
        1
    );
    let retained: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM fact_origin WHERE fact_uid = ?")
            .bind(&credit.uid)
            .fetch_one(&recipient.store.pool)
            .await
            .unwrap();
    assert_eq!(retained, 0);
}
