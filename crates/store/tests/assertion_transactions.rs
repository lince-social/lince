use nucleus::{DecimalValue, RecordKind};
use serde_json::{Value, json};
use store::assertions::{self, AssertionQuantity, AssertionRole, AssertionRow, NewAssertion};
use store::sqlx::Row;
use store::{Store, sqlx};

struct Fixture {
    store: Store,
    subject: String,
    object: String,
    actor: String,
    predicate: String,
    alternative: String,
    unit: String,
    other_unit: String,
}

async fn record(store: &Store, kind: RecordKind) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head: "Assertion transaction fixture",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

impl Fixture {
    async fn new() -> Self {
        Self::with_store(Store::open_memory().await.unwrap()).await
    }

    async fn with_store(store: Store) -> Self {
        let subject = record(&store, RecordKind::Plain).await;
        let object = record(&store, RecordKind::Plain).await;
        let actor = record(&store, RecordKind::Person).await;
        let predicate = store::concepts::create(&store.pool, "predicate", &[])
            .await
            .unwrap();
        let alternative = store::concepts::create(&store.pool, "alternative", &[])
            .await
            .unwrap();
        let unit = store::concepts::create(&store.pool, "unit", &[])
            .await
            .unwrap();
        let other_unit = store::concepts::create(&store.pool, "other-unit", &[])
            .await
            .unwrap();
        let cell = store::cells::local(&store.pool).await.unwrap().unwrap();
        let organ = store::organs::local(&store.pool).await.unwrap().unwrap();
        assert_eq!(cell.organ_uid, organ.uid);
        Self {
            store,
            subject,
            object,
            actor,
            predicate,
            alternative,
            unit,
            other_unit,
        }
    }

    fn new_assertion(&self) -> NewAssertion<'_> {
        NewAssertion {
            subject_uid: &self.subject,
            predicate_uid: &self.predicate,
            object_uid: None,
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: Some(&self.actor),
        }
    }

    async fn insert(&self, uid: &str, new: NewAssertion<'_>) -> AssertionRow {
        let mut tx = store::write_tx(&self.store.pool).await.unwrap();
        let row = assertions::insert_tx(&mut tx, uid, new).await.unwrap();
        tx.commit().await.unwrap();
        row
    }
}

fn decimal(scale: u8, mantissa: i128) -> DecimalValue {
    DecimalValue::from_mantissa(scale, mantissa).unwrap()
}

async fn assertion_ops(store: &Store) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sync_op WHERE tbl = 'record_assertion'")
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

async fn assert_payload(store: &Store, expected: &AssertionRow) {
    let stored = assertions::get(&store.pool, &expected.uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&stored, expected);
    let op = sqlx::query(
        "SELECT * FROM sync_op WHERE tbl = 'record_assertion' AND uid = ? AND kind = 'set'
          ORDER BY seq DESC LIMIT 1",
    )
    .bind(&expected.uid)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    let raw: String = op.get("value");
    let payload: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        payload,
        json!({
            "subject_uid": stored.subject_uid,
            "predicate_uid": stored.predicate_uid,
            "object_uid": stored.object_uid,
            "role": stored.role,
            "quantity_mantissa": stored.quantity.map(|quantity| quantity.mantissa().to_string()),
            "quantity_scale": stored.quantity.map(|quantity| quantity.scale()),
            "unit_uid": stored.unit_uid,
            "asserted_by": stored.asserted_by,
            "created_at": stored.created_at,
        })
    );
    let cell = store::cells::local(&store.pool).await.unwrap().unwrap();
    assert_eq!(op.get::<String, _>("actor_cell"), cell.uid);
    assert_eq!(op.get::<String, _>("organ_uid"), cell.organ_uid);
}

#[tokio::test]
async fn assertion_transactions_preselected_unary_binary_and_exact_payloads() {
    let f = Fixture::new().await;
    let unary_uid = nucleus::new_uid("a");
    let binary_uid = nucleus::new_uid("a");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let unary = assertions::insert_tx(&mut tx, &unary_uid, f.new_assertion())
        .await
        .unwrap();
    let mut new = f.new_assertion();
    new.object_uid = Some(&f.object);
    new.quantity = Some(decimal(9, i128::MAX));
    new.unit_uid = Some(&f.unit);
    let binary = assertions::insert_tx(&mut tx, &binary_uid, new)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(unary.uid, unary_uid);
    assert_eq!(binary.uid, binary_uid);
    assert_payload(&f.store, &unary).await;
    assert_payload(&f.store, &binary).await;
    assert_eq!(assertion_ops(&f.store).await, 2);
}

#[tokio::test]
async fn assertion_transactions_strict_uid_tuple_and_retired_uid_refusal() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let original = f.insert(&uid, f.new_assertion()).await;
    for variant in 0..5 {
        let mut new = f.new_assertion();
        let proposed_uid = if variant == 0 {
            uid.clone()
        } else {
            nucleus::new_uid("a")
        };
        match variant {
            0 => new.predicate_uid = &f.alternative,
            1 => new.role = AssertionRole::Identity,
            2 => new.quantity = Some(decimal(2, 123)),
            3 => new.unit_uid = Some(&f.unit),
            _ => {}
        }
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        assert!(
            assertions::insert_tx(&mut tx, &proposed_uid, new)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    assert_payload(&f.store, &original).await;
    assert_eq!(assertion_ops(&f.store).await, 1);
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::retract_tx(&mut tx, &uid, Some(&f.actor))
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::insert_tx(&mut tx, &uid, f.new_assertion())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let replacement_uid = nucleus::new_uid("a");
    assertions::insert_tx(&mut tx, &replacement_uid, f.new_assertion())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        assertions::get(&f.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .retracted_at
            .is_some()
    );
    assert!(
        assertions::get(&f.store.pool, &replacement_uid)
            .await
            .unwrap()
            .unwrap()
            .retracted_at
            .is_none()
    );
}

#[tokio::test]
async fn assertion_transactions_duplicate_binary_tuple_is_strict() {
    let f = Fixture::new().await;
    let mut original = f.new_assertion();
    original.object_uid = Some(&f.object);
    f.insert(&nucleus::new_uid("a"), original).await;
    let mut new = f.new_assertion();
    new.object_uid = Some(&f.object);
    new.quantity = Some(decimal(0, 4));
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::insert_tx(&mut tx, &nucleus::new_uid("a"), new)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(assertion_ops(&f.store).await, 1);
}

#[tokio::test]
async fn assertion_transactions_identity_rejects_every_illegal_field() {
    let f = Fixture::new().await;
    for field in 0..3 {
        let mut new = f.new_assertion();
        new.role = AssertionRole::Identity;
        match field {
            0 => new.object_uid = Some(&f.object),
            1 => new.quantity = Some(decimal(0, 0)),
            _ => new.unit_uid = Some(&f.unit),
        }
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        assert!(
            assertions::insert_tx(&mut tx, &nucleus::new_uid("a"), new)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    assert_eq!(assertion_ops(&f.store).await, 0);
}

#[tokio::test]
async fn assertion_transactions_missing_and_noncanonical_references_refuse() {
    let f = Fixture::new().await;
    for field in 0..6 {
        for malformed in [false, true] {
            let prefix = match field {
                0 => "a",
                2 | 4 => "c",
                _ => "r",
            };
            let bad = if malformed {
                "not-a-uid".into()
            } else {
                nucleus::new_uid(prefix)
            };
            let valid_uid = nucleus::new_uid("a");
            let mut new = f.new_assertion();
            let uid = match field {
                0 => &bad,
                1 => {
                    new.subject_uid = &bad;
                    &valid_uid
                }
                2 => {
                    new.predicate_uid = &bad;
                    &valid_uid
                }
                3 => {
                    new.object_uid = Some(&bad);
                    &valid_uid
                }
                4 => {
                    new.unit_uid = Some(&bad);
                    &valid_uid
                }
                _ => {
                    new.asserted_by = Some(&bad);
                    &valid_uid
                }
            };
            let mut tx = store::write_tx(&f.store.pool).await.unwrap();
            let result = assertions::insert_tx(&mut tx, uid, new).await;
            if field == 0 && !malformed {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err(), "field {field}, malformed {malformed}");
            }
            tx.rollback().await.unwrap();
        }
    }
    assert_eq!(assertion_ops(&f.store).await, 0);
}

#[tokio::test]
async fn assertion_transactions_deleted_record_references_refuse() {
    let f = Fixture::new().await;
    for deleted in [&f.subject, &f.object, &f.actor] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query("UPDATE record SET deleted_at = 'deleted' WHERE uid = ?")
            .bind(deleted)
            .execute(&mut *tx)
            .await
            .unwrap();
        let mut new = f.new_assertion();
        new.object_uid = Some(&f.object);
        assert!(
            assertions::insert_tx(&mut tx, &nucleus::new_uid("a"), new)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    assert_eq!(assertion_ops(&f.store).await, 0);
}

#[tokio::test]
async fn assertion_transactions_private_root_matrix_and_log_scope() {
    let f = Fixture::new().await;
    let root_a = record(&f.store, RecordKind::Plain).await;
    let root_b = record(&f.store, RecordKind::Rule).await;
    for subject_root in [None, Some(&root_a), Some(&root_b)] {
        for object_root in [None, Some(&root_a), Some(&root_b)] {
            let mut tx = store::write_tx(&f.store.pool).await.unwrap();
            for (uid, root) in [(&f.subject, subject_root), (&f.object, object_root)] {
                sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
                    .bind(root)
                    .bind(uid)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            let mut new = f.new_assertion();
            new.object_uid = Some(&f.object);
            let uid = nucleus::new_uid("a");
            let result = assertions::insert_tx(&mut tx, &uid, new).await;
            let permitted = object_root.is_none() || object_root == subject_root;
            assert_eq!(result.is_ok(), permitted);
            if permitted {
                let logged_root: Option<String> = sqlx::query_scalar(
                    "SELECT replica_root FROM sync_op WHERE tbl = 'record_assertion' AND uid = ?",
                )
                .bind(&uid)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
                assert_eq!(logged_root.as_ref(), subject_root);
            }
            tx.rollback().await.unwrap();
        }
    }
}

#[tokio::test]
async fn assertion_transactions_invalid_root_lineage_refuses() {
    let f = Fixture::new().await;
    let root = record(&f.store, RecordKind::Plain).await;
    let other = record(&f.store, RecordKind::Plain).await;
    for variant in 0..5 {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let root_uid = if variant == 0 {
            nucleus::new_uid("r")
        } else {
            root.clone()
        };
        sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
            .bind(&root_uid)
            .bind(&f.subject)
            .execute(&mut *tx)
            .await
            .unwrap();
        match variant {
            1 => {
                sqlx::query("UPDATE record SET deleted_at = 'deleted' WHERE uid = ?")
                    .bind(&root)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            2 => {
                sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
                    .bind(&other)
                    .bind(&root)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            3 => {
                sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
                    .bind(&f.subject)
                    .bind(&root)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            4 => {
                sqlx::query("UPDATE record SET replica_root = uid WHERE uid = ?")
                    .bind(&root)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            _ => {}
        }
        let result =
            assertions::insert_tx(&mut tx, &nucleus::new_uid("a"), f.new_assertion()).await;
        assert_eq!(result.is_ok(), variant == 4);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn assertion_transactions_exact_quantity_unit_replacement_preserves_identity_and_provenance()
{
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let original = f.insert(&uid, f.new_assertion()).await;
    for (quantity, unit) in [
        (Some(decimal(9, i128::MIN)), Some(f.unit.as_str())),
        (Some(decimal(2, 100)), Some(f.other_unit.as_str())),
        (Some(decimal(3, 1000)), None),
        (None, Some(f.unit.as_str())),
        (None, None),
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let changed = assertions::set_quantity_tx(
            &mut tx,
            &uid,
            AssertionQuantity {
                quantity,
                unit_uid: unit,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(changed.uid, original.uid);
        assert_eq!(changed.subject_uid, original.subject_uid);
        assert_eq!(changed.predicate_uid, original.predicate_uid);
        assert_eq!(changed.object_uid, original.object_uid);
        assert_eq!(changed.asserted_by, original.asserted_by);
        assert_eq!(changed.created_at, original.created_at);
        assert_eq!(changed.quantity, quantity);
        assert_eq!(changed.unit_uid.as_deref(), unit);
        assert_payload(&f.store, &changed).await;
    }
    let before = assertion_ops(&f.store).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assertions::set_quantity_tx(
        &mut tx,
        &uid,
        AssertionQuantity {
            quantity: None,
            unit_uid: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(assertion_ops(&f.store).await, before);
}

#[tokio::test]
async fn assertion_transactions_explicit_identity_replacement_and_retraction() {
    let f = Fixture::new().await;
    let first_uid = nucleus::new_uid("a");
    let mut first = f.new_assertion();
    first.role = AssertionRole::Identity;
    f.insert(&first_uid, first).await;
    let next_uid = nucleus::new_uid("a");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::retract_tx(&mut tx, &first_uid, Some(&f.actor))
            .await
            .unwrap()
    );
    let mut next = f.new_assertion();
    next.role = AssertionRole::Identity;
    next.predicate_uid = &f.alternative;
    let next = assertions::insert_tx(&mut tx, &next_uid, next)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_payload(&f.store, &next).await;
    let first = assertions::get(&f.store.pool, &first_uid)
        .await
        .unwrap()
        .unwrap();
    assert!(first.retracted_at.is_some());
    assert_eq!(first.retracted_by.as_deref(), Some(f.actor.as_str()));
    assert_eq!(first.predicate_uid, f.predicate);
    let ops: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT uid, kind, value FROM sync_op WHERE tbl = 'record_assertion' ORDER BY seq",
    )
    .fetch_all(&f.store.pool)
    .await
    .unwrap();
    assert_eq!(ops.len(), 3);
    assert_eq!(ops[1], (first_uid.clone(), "tombstone".into(), None));
    assert_eq!(ops[2].0, next_uid);
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        !assertions::retract_tx(&mut tx, &first_uid, Some(&f.actor))
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!(assertion_ops(&f.store).await, 3);
}

#[tokio::test]
async fn assertion_transactions_promotion_clears_quantity_and_preserves_original_author_time() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let mut new = f.new_assertion();
    new.quantity = Some(decimal(2, 125));
    new.unit_uid = Some(&f.unit);
    let original = f.insert(&uid, new).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let promoted = assertions::promote_identity_tx(&mut tx, &uid)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(promoted.role, "identity");
    assert_eq!(promoted.quantity, None);
    assert_eq!(promoted.unit_uid, None);
    assert_eq!(promoted.asserted_by, original.asserted_by);
    assert_eq!(promoted.created_at, original.created_at);
    assert_payload(&f.store, &promoted).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert_eq!(
        assertions::promote_identity_tx(&mut tx, &uid)
            .await
            .unwrap(),
        promoted
    );
    tx.commit().await.unwrap();
    assert_eq!(assertion_ops(&f.store).await, 2);
}

#[tokio::test]
async fn assertion_transactions_legacy_identity_promotion_uses_matching_payload() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let mut new = f.new_assertion();
    new.quantity = Some(decimal(4, 500));
    new.unit_uid = Some(&f.unit);
    let original = f.insert(&uid, new).await;
    let organ = store::organs::local(&f.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    assert_eq!(
        assertions::set_identity(&f.store.pool, &f.subject, Some(&f.predicate), Some(&organ))
            .await
            .unwrap(),
        Some(uid.clone())
    );
    let current = assertions::get(&f.store.pool, &uid).await.unwrap().unwrap();
    assert_eq!(current.quantity, None);
    assert_eq!(current.unit_uid, None);
    assert_eq!(current.asserted_by, original.asserted_by);
    assert_eq!(current.created_at, original.created_at);
    assert_payload(&f.store, &current).await;
}

#[tokio::test]
async fn assertion_transactions_legacy_assert_remains_ensure_like() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let original = f.insert(&uid, f.new_assertion()).await;
    let mut requested = f.new_assertion();
    requested.quantity = Some(decimal(4, 500));
    requested.unit_uid = Some(&f.unit);
    assert_eq!(
        assertions::assert(&f.store.pool, requested).await.unwrap(),
        uid
    );
    assert_payload(&f.store, &original).await;
    assert_eq!(assertion_ops(&f.store).await, 1);
}

#[tokio::test]
async fn assertion_transactions_missing_retracted_and_invalid_mutation_targets_refuse() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let mut new = f.new_assertion();
    new.object_uid = Some(&f.object);
    f.insert(&uid, new).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::promote_identity_tx(&mut tx, &uid)
            .await
            .is_err()
    );
    assert!(
        assertions::retract_tx(&mut tx, &uid, Some("invalid"))
            .await
            .is_err()
    );
    assert!(
        assertions::retract_tx(&mut tx, &uid, Some(&nucleus::new_uid("r")))
            .await
            .is_err()
    );
    assertions::retract_tx(&mut tx, &uid, Some(&f.actor))
        .await
        .unwrap();
    for target in [uid, nucleus::new_uid("a"), "invalid".into()] {
        assert!(
            assertions::promote_identity_tx(&mut tx, &target)
                .await
                .is_err()
        );
        assert!(
            assertions::set_quantity_tx(
                &mut tx,
                &target,
                AssertionQuantity {
                    quantity: Some(decimal(0, 1)),
                    unit_uid: Some(&f.unit),
                }
            )
            .await
            .is_err()
        );
    }
    assert!(
        assertions::retract_tx(&mut tx, &nucleus::new_uid("a"), None)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(assertion_ops(&f.store).await, 1);
}

#[tokio::test]
async fn assertion_transactions_identity_quantity_and_invalid_unit_changes_refuse() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let mut new = f.new_assertion();
    new.role = AssertionRole::Identity;
    let original = f.insert(&uid, new).await;
    let missing_unit = nucleus::new_uid("c");
    for quantity in [
        AssertionQuantity {
            quantity: Some(decimal(0, 0)),
            unit_uid: None,
        },
        AssertionQuantity {
            quantity: None,
            unit_uid: Some(&f.unit),
        },
        AssertionQuantity {
            quantity: None,
            unit_uid: Some(&missing_unit),
        },
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        assert!(
            assertions::set_quantity_tx(&mut tx, &uid, quantity)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    let ordinary_uid = nucleus::new_uid("a");
    let mut ordinary = f.new_assertion();
    ordinary.predicate_uid = &f.alternative;
    f.insert(&ordinary_uid, ordinary).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::set_quantity_tx(
            &mut tx,
            &ordinary_uid,
            AssertionQuantity {
                quantity: Some(decimal(0, 1)),
                unit_uid: Some(&missing_unit),
            }
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert_payload(&f.store, &original).await;
}

#[tokio::test]
async fn assertion_transactions_compound_rollback_includes_rows_logs_and_outbox() {
    let f = Fixture::new().await;
    let contact = record(&f.store, RecordKind::Organ).await;
    store::organs::add_contact(
        &f.store.pool,
        &contact,
        None,
        "Assertion transaction peer",
        "https://assertion.invalid",
        1,
    )
    .await
    .unwrap();
    store::organs::set_sync_policy(&f.store.pool, &contact, true, false)
        .await
        .unwrap();
    let first_uid = nucleus::new_uid("a");
    let original = f.insert(&first_uid, f.new_assertion()).await;
    let baseline_ops = assertion_ops(&f.store).await;
    let baseline_outbox: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT uid, kind, seq FROM sync_outbox WHERE tbl = 'record_assertion' ORDER BY seq",
    )
    .fetch_all(&f.store.pool)
    .await
    .unwrap();
    assert_eq!(baseline_outbox.len(), 1);
    let second_uid = nucleus::new_uid("a");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assertions::set_quantity_tx(
        &mut tx,
        &first_uid,
        AssertionQuantity {
            quantity: Some(decimal(2, 765)),
            unit_uid: Some(&f.unit),
        },
    )
    .await
    .unwrap();
    assertions::promote_identity_tx(&mut tx, &first_uid)
        .await
        .unwrap();
    assertions::retract_tx(&mut tx, &first_uid, Some(&f.actor))
        .await
        .unwrap();
    let mut second = f.new_assertion();
    second.role = AssertionRole::Identity;
    second.predicate_uid = &f.alternative;
    assertions::insert_tx(&mut tx, &second_uid, second)
        .await
        .unwrap();
    let in_tx: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_op WHERE tbl = 'record_assertion'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(in_tx, baseline_ops + 4);
    tx.rollback().await.unwrap();
    assert_payload(&f.store, &original).await;
    assert!(
        assertions::get(&f.store.pool, &second_uid)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(assertion_ops(&f.store).await, baseline_ops);
    let outbox: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT uid, kind, seq FROM sync_outbox WHERE tbl = 'record_assertion' ORDER BY seq",
    )
    .fetch_all(&f.store.pool)
    .await
    .unwrap();
    assert_eq!(outbox, baseline_outbox);
}

#[tokio::test]
async fn assertion_transactions_late_refusal_rolls_back_earlier_mutation() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let original = f.insert(&uid, f.new_assertion()).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assertions::retract_tx(&mut tx, &uid, Some(&f.actor))
        .await
        .unwrap();
    let mut bad = f.new_assertion();
    bad.predicate_uid = "missing";
    assert!(
        assertions::insert_tx(&mut tx, &nucleus::new_uid("a"), bad)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_payload(&f.store, &original).await;
    assert_eq!(assertion_ops(&f.store).await, 1);
}

#[tokio::test]
async fn assertion_transactions_sync_failure_rolls_back_insert_and_quantity() {
    let f = Fixture::new().await;
    let existing_uid = nucleus::new_uid("a");
    let original = f.insert(&existing_uid, f.new_assertion()).await;
    sqlx::query(
        "CREATE TRIGGER assertion_transactions_refuse_sync BEFORE INSERT ON sync_op
          WHEN NEW.tbl = 'record_assertion' BEGIN SELECT RAISE(ABORT, 'test sync failure'); END",
    )
    .execute(&f.store.pool)
    .await
    .unwrap();
    let new_uid = nucleus::new_uid("a");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut new = f.new_assertion();
    new.object_uid = Some(&f.object);
    assert!(assertions::insert_tx(&mut tx, &new_uid, new).await.is_err());
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::set_quantity_tx(
            &mut tx,
            &existing_uid,
            AssertionQuantity {
                quantity: Some(decimal(1, 17)),
                unit_uid: None,
            }
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(
        assertions::get(&f.store.pool, &new_uid)
            .await
            .unwrap()
            .is_none()
    );
    assert_payload(&f.store, &original).await;
    assert_eq!(assertion_ops(&f.store).await, 1);
}

#[tokio::test]
async fn assertion_transactions_uncommitted_changes_are_invisible_on_second_connection() {
    let path = std::env::temp_dir().join(format!(
        "assertion-transactions-{}.sqlite",
        nucleus::new_uid("r")
    ));
    let store = Store::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let f = Fixture::with_store(store).await;
    let first_uid = nucleus::new_uid("a");
    let first = f.insert(&first_uid, f.new_assertion()).await;
    let next_uid = nucleus::new_uid("a");
    let mut observer = f.store.pool.acquire().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assertions::retract_tx(&mut tx, &first_uid, Some(&f.actor))
        .await
        .unwrap();
    assertions::insert_tx(&mut tx, &next_uid, f.new_assertion())
        .await
        .unwrap();
    let observed: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT uid, retracted_at FROM record_assertion ORDER BY uid")
            .fetch_all(&mut *observer)
            .await
            .unwrap();
    assert_eq!(observed, vec![(first.uid, None)]);
    let ops: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_op WHERE tbl = 'record_assertion'")
            .fetch_one(&mut *observer)
            .await
            .unwrap();
    assert_eq!(ops, 1);
    tx.commit().await.unwrap();
    let active: Vec<String> =
        sqlx::query_scalar("SELECT uid FROM record_assertion WHERE retracted_at IS NULL")
            .fetch_all(&mut *observer)
            .await
            .unwrap();
    assert_eq!(active, vec![next_uid]);
    let ops: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_op WHERE tbl = 'record_assertion'")
            .fetch_one(&mut *observer)
            .await
            .unwrap();
    assert_eq!(ops, 3);
    drop(observer);
    f.store.pool.close().await;
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn assertion_transactions_current_reference_deletion_refuses_update_and_retract() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let mut new = f.new_assertion();
    new.object_uid = Some(&f.object);
    f.insert(&uid, new).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("UPDATE record SET deleted_at = 'deleted' WHERE uid = ?")
        .bind(&f.object)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        assertions::set_quantity_tx(
            &mut tx,
            &uid,
            AssertionQuantity {
                quantity: Some(decimal(0, 1)),
                unit_uid: None,
            }
        )
        .await
        .is_err()
    );
    assert!(
        assertions::retract_tx(&mut tx, &uid, Some(&f.actor))
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(assertion_ops(&f.store).await, 1);
}

#[tokio::test]
async fn assertion_transactions_corrupt_role_and_quantity_refuse_without_panic() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    f.insert(&uid, f.new_assertion()).await;
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&f.store.pool)
        .await
        .unwrap();
    for statement in [
        "UPDATE record_assertion SET role = 'unknown' WHERE uid = ?",
        "UPDATE record_assertion SET quantity_mantissa = '17' WHERE uid = ?",
        "UPDATE record_assertion SET quantity_mantissa = 'not-a-number', quantity_scale = 1 WHERE uid = ?",
        "UPDATE record_assertion SET quantity_mantissa = '1', quantity_scale = 50 WHERE uid = ?",
        "UPDATE record_assertion SET quantity_mantissa = '01', quantity_scale = 1 WHERE uid = ?",
        "UPDATE record_assertion SET quantity_mantissa = '-0', quantity_scale = 1 WHERE uid = ?",
        "UPDATE record_assertion SET asserted_by = CAST(asserted_by AS BLOB) WHERE uid = ?",
        "UPDATE record_assertion SET created_at = zeroblob(1000000) WHERE uid = ?",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query(statement)
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            assertions::promote_identity_tx(&mut tx, &uid)
                .await
                .is_err()
        );
        assert!(
            assertions::retract_tx(&mut tx, &uid, Some(&f.actor))
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(assertion_ops(&f.store).await, 1);
}

#[tokio::test]
async fn assertion_transactions_identity_conflict_never_displaces_unlisted_assertion() {
    let f = Fixture::new().await;
    let first_uid = nucleus::new_uid("a");
    let mut first = f.new_assertion();
    first.role = AssertionRole::Identity;
    let first = f.insert(&first_uid, first).await;
    let ordinary_uid = nucleus::new_uid("a");
    let mut ordinary = f.new_assertion();
    ordinary.predicate_uid = &f.alternative;
    let ordinary = f.insert(&ordinary_uid, ordinary).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::promote_identity_tx(&mut tx, &ordinary_uid)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assertions::retract_tx(&mut tx, &ordinary_uid, Some(&f.actor))
        .await
        .unwrap();
    let mut conflicting = f.new_assertion();
    conflicting.role = AssertionRole::Identity;
    conflicting.predicate_uid = &f.alternative;
    assert!(
        assertions::insert_tx(&mut tx, &nucleus::new_uid("a"), conflicting)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_payload(&f.store, &first).await;
    assert_payload(&f.store, &ordinary).await;
    assert_eq!(assertion_ops(&f.store).await, 2);
}

#[tokio::test]
async fn assertion_transactions_references_created_in_same_transaction_are_visible() {
    let f = Fixture::new().await;
    let new_subject = nucleus::new_uid("r");
    let new_predicate = nucleus::new_uid("c");
    let uid = nucleus::new_uid("a");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO record (uid, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         SELECT ?, 'plain', '', '', '0', 0, organ_uid, created_at, updated_at
           FROM record WHERE uid = ?",
    )
    .bind(&new_subject)
    .bind(&f.subject)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO concept (uid, canonical_name, created_at) VALUES (?, ?, ?)")
        .bind(&new_predicate)
        .bind("same-transaction-concept")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await
        .unwrap();
    let mut new = f.new_assertion();
    new.subject_uid = &new_subject;
    new.predicate_uid = &new_predicate;
    let inserted = assertions::insert_tx(&mut tx, &uid, new).await.unwrap();
    tx.commit().await.unwrap();
    assert_payload(&f.store, &inserted).await;
}

#[tokio::test]
async fn assertion_transactions_historical_author_deletion_does_not_rewrite_provenance() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let original = f.insert(&uid, f.new_assertion()).await;
    sqlx::query("UPDATE record SET deleted_at = 'deleted' WHERE uid = ?")
        .bind(&f.actor)
        .execute(&f.store.pool)
        .await
        .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let changed = assertions::set_quantity_tx(
        &mut tx,
        &uid,
        AssertionQuantity {
            quantity: Some(decimal(1, 3)),
            unit_uid: Some(&f.unit),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(changed.asserted_by, original.asserted_by);
    assert_eq!(changed.created_at, original.created_at);
    assert_payload(&f.store, &changed).await;
    let organ = store::organs::local(&f.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::retract_tx(&mut tx, &uid, Some(&f.actor))
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        assertions::retract_tx(&mut tx, &uid, Some(&organ))
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    let current = assertions::get(&f.store.pool, &uid).await.unwrap().unwrap();
    assert_eq!(current.asserted_by, original.asserted_by);
    assert_eq!(current.retracted_by, Some(organ));
}

#[tokio::test]
async fn assertion_transactions_missing_or_deleted_local_identity_cannot_silently_skip_log() {
    let f = Fixture::new().await;
    let cell = store::cells::local(&f.store.pool).await.unwrap().unwrap();
    for variant in 0..3 {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        match variant {
            0 => {
                sqlx::query("UPDATE record SET slug = NULL WHERE uid = ?")
                    .bind(&cell.uid)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            1 => {
                sqlx::query("UPDATE record SET deleted_at = 'deleted' WHERE uid = ?")
                    .bind(&cell.uid)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query("UPDATE record SET deleted_at = 'deleted' WHERE uid = ?")
                    .bind(&cell.organ_uid)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
        }
        assert!(
            assertions::insert_tx(&mut tx, &nucleus::new_uid("a"), f.new_assertion())
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    assert_eq!(assertion_ops(&f.store).await, 0);
    let uid = nucleus::new_uid("a");
    let inserted = f.insert(&uid, f.new_assertion()).await;
    assert_payload(&f.store, &inserted).await;
}
