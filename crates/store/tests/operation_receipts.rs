use chrono::{DateTime, Utc};
use nucleus::RecordKind;
use serde_json::json;
use store::Store;
use store::operation_receipts::{NewOperationReceipt, OperationReceiptKey, ReceiptWrite};

async fn record(store: &Store, kind: RecordKind, head: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn scope(store: &Store) -> (String, String) {
    let organ = store::organs::local(&store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let person = record(store, RecordKind::Person, "Operator").await;
    (organ, person)
}

fn accepted_at() -> DateTime<Utc> {
    "2026-09-06T12:00:00Z".parse().unwrap()
}

#[tokio::test]
async fn the_real_migration_has_strict_immutable_receipt_tables() {
    let store = Store::open_memory().await.unwrap();
    for table in ["operation_receipt", "operation_receipt_record"] {
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(
                "SELECT strict FROM pragma_table_list WHERE name = ?",
            )
            .bind(table)
            .fetch_one(&store.pool)
            .await
            .unwrap(),
            1
        );
    }
    let triggers = store::sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_schema
          WHERE type = 'trigger' AND name LIKE 'operation_receipt_%immutable_%'
          ORDER BY name",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        triggers,
        vec![
            "operation_receipt_immutable_delete",
            "operation_receipt_immutable_update",
            "operation_receipt_record_immutable_delete",
            "operation_receipt_record_immutable_update",
        ]
    );
}

#[tokio::test]
async fn exact_retry_returns_the_immutable_outcome_and_changed_content_refuses() {
    let store = Store::open_memory().await.unwrap();
    let (organ, person) = scope(&store).await;
    let target = record(&store, RecordKind::Plain, "Target").await;
    let operation = nucleus::new_uid("op");
    let digest = [7; 32];
    let original = json!({"status": "accepted", "revision": 2});
    let affected = vec![target.clone()];
    let key = OperationReceiptKey {
        organ_uid: &organ,
        person_uid: &person,
        operation_uid: &operation,
    };
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let stored = store::operation_receipts::store_on(
        &mut tx,
        NewOperationReceipt {
            key,
            payload_digest: &digest,
            outcome: &original,
            affected_record_uids: &affected,
            accepted_at: accepted_at(),
        },
    )
    .await
    .unwrap();
    let ReceiptWrite::Stored(stored) = stored else {
        panic!("first write must store the receipt")
    };
    tx.commit().await.unwrap();
    assert!(
        store::sqlx::query(
            "UPDATE operation_receipt SET outcome = '{}' \
             WHERE organ_uid = ? AND person_uid = ? AND operation_uid = ?",
        )
        .bind(&organ)
        .bind(&person)
        .bind(&operation)
        .execute(&store.pool)
        .await
        .is_err()
    );
    assert!(
        store::sqlx::query(
            "DELETE FROM operation_receipt_record \
             WHERE organ_uid = ? AND person_uid = ? AND operation_uid = ?",
        )
        .bind(&organ)
        .bind(&person)
        .bind(&operation)
        .execute(&store.pool)
        .await
        .is_err()
    );

    let replacement = json!({"status": "must not replace"});
    let no_targets = Vec::new();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let replayed = store::operation_receipts::store_on(
        &mut tx,
        NewOperationReceipt {
            key,
            payload_digest: &digest,
            outcome: &replacement,
            affected_record_uids: &no_targets,
            accepted_at: "2030-01-01T00:00:00Z".parse().unwrap(),
        },
    )
    .await
    .unwrap();
    assert_eq!(replayed, ReceiptWrite::Existing(stored.clone()));
    assert_eq!(
        store::operation_receipts::get_on(&mut tx, key)
            .await
            .unwrap(),
        Some(stored.clone())
    );
    tx.commit().await.unwrap();

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::operation_receipts::store_on(
            &mut tx,
            NewOperationReceipt {
                key,
                payload_digest: &[8; 32],
                outcome: &original,
                affected_record_uids: &affected,
                accepted_at: accepted_at(),
            },
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("different content")
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn receipt_identity_is_scoped_by_both_organ_and_person() {
    let store = Store::open_memory().await.unwrap();
    let (first_organ, first_person) = scope(&store).await;
    let second_organ = record(&store, RecordKind::Organ, "Other Organ").await;
    let second_person = record(&store, RecordKind::Person, "Other Person").await;
    let operation = nucleus::new_uid("op");
    let outcome = json!({"ok": true});
    let affected = Vec::new();
    let scopes = [
        (&first_organ, &first_person, [1; 32]),
        (&first_organ, &second_person, [2; 32]),
        (&second_organ, &first_person, [3; 32]),
    ];
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    for (organ, person, digest) in &scopes {
        let result = store::operation_receipts::store_on(
            &mut tx,
            NewOperationReceipt {
                key: OperationReceiptKey {
                    organ_uid: organ,
                    person_uid: person,
                    operation_uid: &operation,
                },
                payload_digest: digest,
                outcome: &outcome,
                affected_record_uids: &affected,
                accepted_at: accepted_at(),
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, ReceiptWrite::Stored(_)));
    }
    tx.commit().await.unwrap();

    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM operation_receipt WHERE operation_uid = ?",
        )
        .bind(&operation)
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        3
    );
}

#[tokio::test]
async fn invalid_scope_outcome_and_affected_records_are_refused() {
    let store = Store::open_memory().await.unwrap();
    let (organ, person) = scope(&store).await;
    let plain = record(&store, RecordKind::Plain, "Not an actor").await;
    let deleted = record(&store, RecordKind::Person, "Deleted").await;
    store::records::mark_deleted(&store.pool, &deleted)
        .await
        .unwrap();
    let target = record(&store, RecordKind::Plain, "Target").await;
    let digest = [4; 32];
    let valid_outcome = json!({});
    let no_targets = Vec::new();
    let operation = nucleus::new_uid("op");
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    for key in [
        OperationReceiptKey {
            organ_uid: &plain,
            person_uid: &person,
            operation_uid: &operation,
        },
        OperationReceiptKey {
            organ_uid: &organ,
            person_uid: &plain,
            operation_uid: &operation,
        },
        OperationReceiptKey {
            organ_uid: &organ,
            person_uid: &deleted,
            operation_uid: &operation,
        },
        OperationReceiptKey {
            organ_uid: &organ,
            person_uid: &person,
            operation_uid: "not-an-operation",
        },
    ] {
        assert!(
            store::operation_receipts::store_on(
                &mut tx,
                NewOperationReceipt {
                    key,
                    payload_digest: &digest,
                    outcome: &valid_outcome,
                    affected_record_uids: &no_targets,
                    accepted_at: accepted_at(),
                },
            )
            .await
            .is_err()
        );
    }

    let invalid_operation = nucleus::new_uid("op");
    for invalid in [json!(null), json!([]), json!("outcome")] {
        assert!(
            store::operation_receipts::store_on(
                &mut tx,
                NewOperationReceipt {
                    key: OperationReceiptKey {
                        organ_uid: &organ,
                        person_uid: &person,
                        operation_uid: &invalid_operation,
                    },
                    payload_digest: &digest,
                    outcome: &invalid,
                    affected_record_uids: &no_targets,
                    accepted_at: accepted_at(),
                },
            )
            .await
            .is_err()
        );
    }
    let oversized_outcome =
        json!({"text": "x".repeat(store::operation_receipts::MAX_OUTCOME_BYTES)});
    let oversized_operation = nucleus::new_uid("op");
    assert!(
        store::operation_receipts::store_on(
            &mut tx,
            NewOperationReceipt {
                key: OperationReceiptKey {
                    organ_uid: &organ,
                    person_uid: &person,
                    operation_uid: &oversized_operation,
                },
                payload_digest: &digest,
                outcome: &oversized_outcome,
                affected_record_uids: &no_targets,
                accepted_at: accepted_at(),
            },
        )
        .await
        .is_err()
    );

    let duplicate = vec![target.clone(), target.clone()];
    let missing = vec![nucleus::new_uid("r")];
    for affected in [&duplicate, &missing] {
        let operation = nucleus::new_uid("op");
        assert!(
            store::operation_receipts::store_on(
                &mut tx,
                NewOperationReceipt {
                    key: OperationReceiptKey {
                        organ_uid: &organ,
                        person_uid: &person,
                        operation_uid: &operation,
                    },
                    payload_digest: &digest,
                    outcome: &valid_outcome,
                    affected_record_uids: affected,
                    accepted_at: accepted_at(),
                },
            )
            .await
            .is_err()
        );
    }
    let too_many = vec![target; store::operation_receipts::MAX_AFFECTED_RECORDS + 1];
    let operation = nucleus::new_uid("op");
    assert!(
        store::operation_receipts::store_on(
            &mut tx,
            NewOperationReceipt {
                key: OperationReceiptKey {
                    organ_uid: &organ,
                    person_uid: &person,
                    operation_uid: &operation,
                },
                payload_digest: &digest,
                outcome: &valid_outcome,
                affected_record_uids: &too_many,
                accepted_at: accepted_at(),
            },
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn oversized_and_corrupt_stored_receipts_refuse_before_payload_reads() {
    let store = Store::open_memory().await.unwrap();
    let (organ, person) = scope(&store).await;
    let digest = [5_u8; 32];
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    let malformed = nucleus::new_uid("op");
    let oversized = nucleus::new_uid("op");
    let bad_digest = nucleus::new_uid("op");
    let bad_time = nucleus::new_uid("op");
    let corrupt_rows = vec![
        (
            malformed.clone(),
            digest.to_vec(),
            "{".to_owned(),
            "2026-09-06T12:00:00.000000000Z".to_owned(),
        ),
        (
            oversized.clone(),
            digest.to_vec(),
            format!(
                "{{\"text\":\"{}\"}}",
                "x".repeat(store::operation_receipts::MAX_OUTCOME_BYTES)
            ),
            "2026-09-06T12:00:00.000000000Z".to_owned(),
        ),
        (
            bad_digest.clone(),
            vec![1_u8; 31],
            "{}".to_owned(),
            "2026-09-06T12:00:00.000000000Z".to_owned(),
        ),
        (
            bad_time.clone(),
            digest.to_vec(),
            "{}".to_owned(),
            "2026-09-06T12:00:00Z".to_owned(),
        ),
    ];
    for (operation, stored_digest, outcome, time) in corrupt_rows {
        store::sqlx::query(
            "INSERT INTO operation_receipt
                (organ_uid, person_uid, operation_uid, payload_digest, outcome, accepted_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&organ)
        .bind(&person)
        .bind(operation)
        .bind(stored_digest)
        .bind(outcome)
        .bind(time)
        .execute(&store.pool)
        .await
        .unwrap();
    }
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();

    for operation in [&malformed, &oversized, &bad_digest, &bad_time] {
        let mut connection = store.pool.acquire().await.unwrap();
        assert!(
            store::operation_receipts::get_on(
                &mut connection,
                OperationReceiptKey {
                    organ_uid: &organ,
                    person_uid: &person,
                    operation_uid: operation,
                },
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn stored_affected_record_count_and_bytes_are_guarded_before_fetch() {
    let store = Store::open_memory().await.unwrap();
    let (organ, person) = scope(&store).await;
    let digest = [6_u8; 32];
    let outcome = json!({});
    let affected = Vec::new();
    let count_operation = nucleus::new_uid("op");
    let bytes_operation = nucleus::new_uid("op");
    let reference_operation = nucleus::new_uid("op");
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    for operation in [&count_operation, &bytes_operation, &reference_operation] {
        store::operation_receipts::store_on(
            &mut tx,
            NewOperationReceipt {
                key: OperationReceiptKey {
                    organ_uid: &organ,
                    person_uid: &person,
                    operation_uid: operation,
                },
                payload_digest: &digest,
                outcome: &outcome,
                affected_record_uids: &affected,
                accepted_at: accepted_at(),
            },
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    store::sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query(
        "WITH RECURSIVE number(value) AS (
             SELECT 1
             UNION ALL
             SELECT value + 1 FROM number WHERE value <= ?
         )
         INSERT INTO operation_receipt_record
             (organ_uid, person_uid, operation_uid, record_uid)
         SELECT ?, ?, ?, printf('r_%026d', value) FROM number",
    )
    .bind(i64::try_from(store::operation_receipts::MAX_AFFECTED_RECORDS).unwrap())
    .bind(&organ)
    .bind(&person)
    .bind(&count_operation)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "INSERT INTO operation_receipt_record
            (organ_uid, person_uid, operation_uid, record_uid)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&organ)
    .bind(&person)
    .bind(&reference_operation)
    .bind(nucleus::new_uid("r"))
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "INSERT INTO operation_receipt_record
            (organ_uid, person_uid, operation_uid, record_uid)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&organ)
    .bind(&person)
    .bind(&bytes_operation)
    .bind(format!("r_{}", "A".repeat(4096)))
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&store.pool)
        .await
        .unwrap();

    for operation in [&count_operation, &bytes_operation, &reference_operation] {
        let mut connection = store.pool.acquire().await.unwrap();
        assert!(
            store::operation_receipts::get_on(
                &mut connection,
                OperationReceiptKey {
                    organ_uid: &organ,
                    person_uid: &person,
                    operation_uid: operation,
                },
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn receipt_and_associated_write_roll_back_together() {
    let store = Store::open_memory().await.unwrap();
    let (organ, person) = scope(&store).await;
    let target = record(&store, RecordKind::Plain, "Before").await;
    let operation = nucleus::new_uid("op");
    let digest = [9; 32];
    let outcome = json!({"head": "After"});
    let affected = vec![target.clone()];
    let key = OperationReceiptKey {
        organ_uid: &organ,
        person_uid: &person,
        operation_uid: &operation,
    };
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE record SET head = 'After' WHERE uid = ?")
        .bind(&target)
        .execute(&mut *tx)
        .await
        .unwrap();
    store::operation_receipts::store_on(
        &mut tx,
        NewOperationReceipt {
            key,
            payload_digest: &digest,
            outcome: &outcome,
            affected_record_uids: &affected,
            accepted_at: accepted_at(),
        },
    )
    .await
    .unwrap();
    assert!(
        store::operation_receipts::get_on(&mut tx, key)
            .await
            .unwrap()
            .is_some()
    );
    tx.rollback().await.unwrap();

    assert_eq!(
        store::sqlx::query_scalar::<_, String>("SELECT head FROM record WHERE uid = ?")
            .bind(&target)
            .fetch_one(&store.pool)
            .await
            .unwrap(),
        "Before"
    );
    let mut connection = store.pool.acquire().await.unwrap();
    assert_eq!(
        store::operation_receipts::get_on(&mut connection, key)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn receipt_serializes_across_connections_and_survives_restart() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("operation-receipt"));
    std::fs::create_dir_all(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    let store = Store::open(&url).await.unwrap();
    let (organ, person) = scope(&store).await;
    let target = record(&store, RecordKind::Plain, "Persistent").await;
    let operation = nucleus::new_uid("op");
    let digest = [10; 32];
    let outcome = json!({"saved": true});
    let affected = vec![target];
    let key = OperationReceiptKey {
        organ_uid: &organ,
        person_uid: &person,
        operation_uid: &operation,
    };
    let mut second = store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA busy_timeout = 0")
        .execute(&mut *second)
        .await
        .unwrap();
    let mut first = store::write_tx(&store.pool).await.unwrap();
    let stored = store::operation_receipts::store_on(
        &mut first,
        NewOperationReceipt {
            key,
            payload_digest: &digest,
            outcome: &outcome,
            affected_record_uids: &affected,
            accepted_at: accepted_at(),
        },
    )
    .await
    .unwrap();
    let ReceiptWrite::Stored(stored) = stored else {
        panic!("first write must store the receipt")
    };
    let busy = store::sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *second)
        .await
        .unwrap_err();
    assert_eq!(
        busy.as_database_error().unwrap().code().as_deref(),
        Some("5")
    );
    first.commit().await.unwrap();
    drop(second);
    store.pool.close().await;

    let reopened = Store::open(&url).await.unwrap();
    let mut connection = reopened.pool.acquire().await.unwrap();
    assert_eq!(
        store::operation_receipts::get_on(&mut connection, key)
            .await
            .unwrap(),
        Some(stored)
    );
    drop(connection);
    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}
