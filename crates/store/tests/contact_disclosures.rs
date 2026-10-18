use nucleus::trust_analysis::{
    Identity,
    contacts::{Audience, Document, NAMESPACE, Statement},
};
use serde_json::json;
use store::{Store, contact_disclosures as contacts};

async fn fixture() -> (Store, String, Document) {
    let store = Store::open_memory().await.unwrap();
    store::organs::ensure_local(&store.pool, "http://contact.test")
        .await
        .unwrap();
    let person = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Proof owner",
            body: "",
            quantity: store::exact::integer(1),
        },
    )
    .await
    .unwrap()
    .uid;
    let now = nucleus::execution::now().timestamp();
    let document = Document {
        statement: Statement {
            issuer: Identity::Person(person.clone()),
            contact: Identity::Organ(nucleus::new_uid("r")),
            revision: 1,
            issued_at: now,
            expires_at: now + 3600,
            withdrawn: false,
            audience: Audience::Public,
        },
        key_id: "codec-fixture".into(),
        public_key: format!("{}=", "A".repeat(43)),
        signature: format!("{}==", "A".repeat(86)),
    };
    (store, person, document)
}

fn input(person: &str, document: &Document, id: &str) -> contacts::Input {
    contacts::Input {
        record: nucleus::new_uid("r"),
        person: person.into(),
        request_id: id.into(),
        request: json!({"kind":"fixture","document":document}),
        document: document.clone(),
        expected_revision: None,
        authorization_intent: None,
    }
}

async fn save(store: &Store, input: contacts::Input) -> contacts::Commit {
    let (committed, tx) = contacts::save(
        store::write_tx(&store.pool).await.unwrap(),
        input,
        nucleus::execution::now(),
        |_| Some("private-audit-codec-fixture".into()),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    committed
}

async fn counts(store: &Store) -> (i64, i64, i64, i64) {
    store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM contact_disclosure_record),(SELECT COUNT(*) FROM contact_disclosure_request)")
        .fetch_one(&store.pool).await.unwrap()
}

#[tokio::test]
async fn signed_contact_storage_is_private_zero_quantity_and_strictly_idempotent() {
    let (store, person, document) = fixture().await;
    let mut first = input(&person, &document, "contact:first");
    first.expected_revision = Some(0);
    let committed = save(&store, first).await;
    assert!(committed.created);
    let record = committed.saved.record;
    let saved = contacts::get(&store.pool, &record).await.unwrap();
    assert_eq!(saved.document, document);
    assert!(saved.fact.delta.is_zero());
    assert_eq!(saved.fact.actor_uid.as_deref(), Some(person.as_str()));
    assert!(
        store::records::get(&store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .is_zero()
    );
    let policy =
        store::data_visibility::policy(&store.pool, &record, nucleus::visibility::Data::Record)
            .await
            .unwrap()
            .unwrap();
    assert!(policy.controller_uid.is_empty());
    assert_eq!(policy.policy.include.len(), 1);
    assert_eq!(policy.policy.include[0].persons, vec![person.clone()]);
    let organ = store::organs::local(&store.pool).await.unwrap().unwrap();
    assert!(
        policy
            .policy
            .decide_principal(&organ.uid, Some(&person), Some(0))
            .allowed
    );
    assert!(
        !policy
            .policy
            .decide_principal(&organ.uid, None, Some(0))
            .allowed
    );
    assert!(
        !policy
            .policy
            .decide_principal(&organ.uid, Some(&nucleus::new_uid("r")), Some(0))
            .allowed
    );
    let baseline = counts(&store).await;
    let mut replay = input(&person, &document, "contact:first");
    replay.expected_revision = Some(0);
    let replay = save(&store, replay).await;
    assert!(!replay.created);
    assert_eq!(replay.saved.record, record);
    assert_eq!(counts(&store).await, baseline);
    let duplicate = save(&store, input(&person, &document, "contact:duplicate")).await;
    assert!(!duplicate.created);
    assert_eq!(duplicate.saved.record, record);
    let after = counts(&store).await;
    assert_eq!(
        (after.0, after.1, after.2),
        (baseline.0, baseline.1, baseline.2)
    );
    assert_eq!(after.3, baseline.3 + 1);
    let mut different = input(&person, &document, "contact:first");
    different.request["changed"] = json!(true);
    assert!(
        contacts::save(
            store::write_tx(&store.pool).await.unwrap(),
            different,
            nucleus::execution::now(),
            |_| Some("fixture".into())
        )
        .await
        .is_err()
    );
    assert_eq!(counts(&store).await, after);
    let changed_expected = input(&person, &document, "contact:first");
    assert!(
        contacts::save(
            store::write_tx(&store.pool).await.unwrap(),
            changed_expected,
            nucleus::execution::now(),
            |_| Some("fixture".into())
        )
        .await
        .is_err()
    );
    let mut changed_evidence = input(&person, &document, "contact:first");
    changed_evidence.expected_revision = Some(0);
    changed_evidence.document.statement.expires_at += 1;
    assert!(
        contacts::save(
            store::write_tx(&store.pool).await.unwrap(),
            changed_evidence,
            nucleus::execution::now(),
            |_| Some("fixture".into())
        )
        .await
        .is_err()
    );
    assert_eq!(counts(&store).await, after);
}

#[tokio::test]
async fn contact_revisions_keep_originals_withdrawals_and_conflicts_without_generic_rewriting() {
    let (store, person, document) = fixture().await;
    let first = save(&store, input(&person, &document, "contact:original"))
        .await
        .saved;
    let mut withdrawn = document.clone();
    withdrawn.statement.revision = 2;
    withdrawn.statement.withdrawn = true;
    let mut stale = input(&person, &withdrawn, "contact:stale");
    stale.expected_revision = Some(0);
    let baseline = counts(&store).await;
    assert!(
        contacts::save(
            store::write_tx(&store.pool).await.unwrap(),
            stale,
            nucleus::execution::now(),
            |_| Some("fixture".into())
        )
        .await
        .is_err()
    );
    assert_eq!(counts(&store).await, baseline);
    let mut current = input(&person, &withdrawn, "contact:withdraw");
    current.expected_revision = Some(1);
    let current = save(&store, current).await.saved;
    assert_eq!(
        contacts::get(&store.pool, &first.record)
            .await
            .unwrap()
            .document,
        document
    );
    assert_eq!(
        contacts::current_candidates(&store.pool, &person)
            .await
            .unwrap(),
        vec![current.record.clone()]
    );
    assert!(
        store::records::set_extension(&store.pool, &first.record, NAMESPACE, &json!(withdrawn))
            .await
            .is_err()
    );
    assert!(
        store::records::delete_extension(&store.pool, &first.record, NAMESPACE)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE contact_disclosure_record SET withdrawn=0 WHERE record_uid=?")
            .bind(&current.record)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query(
            "UPDATE contact_disclosure_request SET request_json='{}' WHERE record_uid=?"
        )
        .bind(&current.record)
        .execute(&store.pool)
        .await
        .is_err()
    );
    store::records::mark_deleted(&store.pool, &current.record)
        .await
        .unwrap();
    assert_eq!(
        contacts::current_candidates(&store.pool, &person)
            .await
            .unwrap(),
        vec![current.record.clone()]
    );
    assert!(
        contacts::get(&store.pool, &current.record)
            .await
            .unwrap()
            .document
            .statement
            .withdrawn
    );
    let mut conflicting = withdrawn.clone();
    conflicting.statement.withdrawn = false;
    let conflicting = save(&store, input(&person, &conflicting, "contact:conflict"))
        .await
        .saved;
    let candidates = contacts::current_candidates(&store.pool, &person)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 2);
    assert!(candidates.contains(&current.record));
    assert!(candidates.contains(&conflicting.record));
}

#[tokio::test]
async fn a_missing_private_authorization_rolls_back_records_facts_and_disclosure_indexes() {
    let (store, person, document) = fixture().await;
    let baseline = counts(&store).await;
    assert!(
        contacts::save(
            store::write_tx(&store.pool).await.unwrap(),
            input(&person, &document, "contact:unsigned-audit"),
            nucleus::execution::now(),
            |_| None
        )
        .await
        .is_err()
    );
    assert_eq!(counts(&store).await, baseline);
    let ordinary = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "Unverified ordinary Record",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    assert!(
        store::records::set_extension(&store.pool, &ordinary.uid, NAMESPACE, &json!(document))
            .await
            .is_err()
    );
    assert!(contacts::get(&store.pool, &ordinary.uid).await.is_err());
}
