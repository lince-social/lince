use nucleus::trust_analysis::{
    Identity,
    claims::{self, Authorship, Comparator, Descriptor, Document, Metric, Statement},
    contacts::Audience,
    query::{CounterpartyBasis, EvidenceLevel},
};
use store::{Store, profile_claims as saved};

async fn fixture() -> (Store, String, Document) {
    let store = Store::open_memory().await.unwrap();
    store::organs::ensure_local(&store.pool, "http://claim.test")
        .await
        .unwrap();
    let person = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Claim holder",
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
            category: Default::default(),
            title: "Food contribution".into(),
            issuer: Identity::Person(person.clone()),
            claim: nucleus::new_uid("r"),
            revision: 1,
            descriptor: Descriptor {
                subject: Identity::Person(person.clone()),
                role: nucleus::transfer::outcome::Role::Giver,
                measurement: nucleus::transfer::receipt::Measurement {
                    meaning: nucleus::vocabulary::reference::MeaningReference {
                        vocabulary_uid: nucleus::new_uid("g"),
                        concept_uid: nucleus::new_uid("c"),
                        revision: 1,
                        definition_hash: "a".repeat(64),
                    },
                    unit: None,
                    object_commitment: None,
                },
                level: EvidenceLevel::NeedLinkedReceiver,
                metric: Metric::Quantity,
                counterparty_basis: CounterpartyBasis::Organ,
                from: None,
                until: None,
            },
            comparator: Comparator::AtLeast,
            amount: nucleus::DecimalValue::parse_inferred("100").unwrap(),
            as_of: now,
            issued_at: now,
            expires_at: now + 3600,
            withdrawn: false,
            audience: Audience::Public,
            authorship: Authorship::Manual,
        },
        key_id: "codec-fixture".into(),
        public_key: format!("{}=", "A".repeat(43)),
        signature: format!("{}==", "A".repeat(86)),
    };
    (store, person, document)
}

fn input(person: &str, document: &Document, id: &str, expected: Option<u64>) -> saved::Input {
    saved::Input {
        generation_recipe: None,
        record: nucleus::new_uid("r"),
        person: person.into(),
        request_id: id.into(),
        request: serde_json::json!({"document":document}),
        document: document.clone(),
        expected_revision: expected,
        authorization_intent: None,
    }
}

async fn save(store: &Store, input: saved::Input) -> saved::Commit {
    let (result, tx) = saved::save(
        store::write_tx(&store.pool).await.unwrap(),
        input,
        nucleus::execution::now(),
        |_| Some("private-fixture-audit".into()),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    result
}

async fn counts(store: &Store) -> (i64, i64, i64) {
    store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM profile_claim_record)").fetch_one(&store.pool).await.unwrap()
}

#[tokio::test]
async fn signed_claim_storage_is_private_immutable_zero_quantity_and_idempotent() {
    let (store, person, document) = fixture().await;
    let first = save(&store, input(&person, &document, "claim:first", Some(0))).await;
    assert!(first.created);
    assert!(first.saved.fact.delta.is_zero());
    let policy = store::data_visibility::policy(
        &store.pool,
        &first.saved.record,
        nucleus::visibility::Data::Record,
    )
    .await
    .unwrap()
    .unwrap();
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
    let before = counts(&store).await;
    assert!(
        !save(&store, input(&person, &document, "claim:first", Some(0)))
            .await
            .created
    );
    assert!(
        !save(&store, input(&person, &document, "claim:copy", None))
            .await
            .created
    );
    assert_eq!(before, counts(&store).await);
    assert!(
        store::records::set_extension(
            &store.pool,
            &first.saved.record,
            claims::NAMESPACE,
            &serde_json::json!({})
        )
        .await
        .is_err()
    );
    assert!(
        store::records::delete_extension(&store.pool, &first.saved.record, claims::NAMESPACE)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE profile_claim_record SET revision=4 WHERE record_uid=?")
            .bind(&first.saved.record)
            .execute(&store.pool)
            .await
            .is_err()
    );
    let mut changed = input(&person, &document, "claim:first", Some(0));
    changed.request["changed"] = serde_json::json!(true);
    assert!(
        saved::save(
            store::write_tx(&store.pool).await.unwrap(),
            changed,
            nucleus::execution::now(),
            |_| Some("audit".into())
        )
        .await
        .is_err()
    );
    assert_eq!(before, counts(&store).await);
}

#[tokio::test]
async fn hidden_withdrawals_and_forks_cannot_resurrect_older_claims_and_unsigned_saves_roll_back() {
    let (store, person, document) = fixture().await;
    save(&store, input(&person, &document, "claim:first", Some(0))).await;
    let mut withdrawal = document.clone();
    withdrawal.statement.revision = 2;
    withdrawal.statement.withdrawn = true;
    withdrawal.statement.amount = store::exact::zero();
    let withdrawn = save(
        &store,
        input(&person, &withdrawal, "claim:withdraw", Some(1)),
    )
    .await;
    store::sqlx::query("UPDATE record SET deleted_at=? WHERE uid=?")
        .bind(nucleus::execution::now().to_rfc3339())
        .bind(&withdrawn.saved.record)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        !saved::current_on(&mut connection, &person, &document.statement)
            .await
            .unwrap()
    );
    assert_eq!(
        saved::latest_on(&mut connection, &person, &document.statement)
            .await
            .unwrap(),
        2
    );
    drop(connection);
    let mut fork = withdrawal.clone();
    fork.statement.title = "Competing current statement".into();
    save(&store, input(&person, &fork, "claim:fork", None)).await;
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        !saved::current_on(&mut connection, &person, &fork.statement)
            .await
            .unwrap()
    );
    drop(connection);
    let before = counts(&store).await;
    let mut unsigned = document.clone();
    unsigned.statement.claim = nucleus::new_uid("r");
    assert!(
        saved::save(
            store::write_tx(&store.pool).await.unwrap(),
            input(&person, &unsigned, "claim:unsigned", Some(0)),
            nucleus::execution::now(),
            |_| None
        )
        .await
        .is_err()
    );
    assert_eq!(before, counts(&store).await);
}
