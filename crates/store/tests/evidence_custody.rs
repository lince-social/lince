use nucleus::{
    RecordKind,
    trust_analysis::{
        Condition, Filter, Identity,
        contacts::{Audience, Document, Statement},
        custody::{Acknowledgment, Offer, Policy},
        serving::Artifact,
    },
};
use store::{Store, evidence_custody as custody};
#[path = "evidence_custody/forwarding.rs"]
mod forwarding;

struct Fixture {
    store: Store,
    owner: String,
    artifact: Artifact,
    offer: Offer,
    ack: Acknowledgment,
    now: i64,
}

async fn fixture(store: Store) -> Fixture {
    let local = store::organs::ensure_local(&store.pool, "http://custody.test")
        .await
        .unwrap();
    let mut ids = Vec::new();
    for (head, kind) in [
        ("Operator", RecordKind::Person),
        ("Sender", RecordKind::Organ),
    ] {
        ids.push(
            store::records::create(
                &store.pool,
                store::records::NewRecord {
                    slug: None,
                    kind,
                    head,
                    body: "",
                    quantity: store::exact::integer(1),
                },
            )
            .await
            .unwrap()
            .uid,
        );
    }
    let owner = ids[0].clone();
    let now = 10_000_000;
    let artifact = Artifact::Contact(Document {
        statement: Statement {
            issuer: Identity::Organ(ids[1].clone()),
            contact: Identity::Person(owner.clone()),
            revision: 1,
            issued_at: now - 1,
            expires_at: now + 7200,
            withdrawn: false,
            audience: Audience::Public,
        },
        key_id: "root".into(),
        public_key: format!("{}=", "A".repeat(43)),
        signature: format!("{}==", "A".repeat(86)),
    });
    let filter = Filter {
        include: vec![Condition {
            known: Some(true),
            ..Default::default()
        }],
        exclude: vec![],
    };
    let offer = Offer {
        nonce: nucleus::new_uid("cust"),
        sender: ids[1].clone(),
        custodian: local.uid,
        artifact: artifact.hash().unwrap(),
        source_kind: artifact.kind(),
        bytes: artifact.bytes().unwrap().len() as u32,
        retained_until: now + 300,
        deliver_by: now + 10,
        readers: filter.clone(),
        redistribute: true,
        key_id: "root".into(),
        public_key: "sender key".into(),
        signature: "protocol fixture".into(),
    };
    let ack = acknowledgment(&offer, now);
    let policy = Policy {
        enabled: true,
        senders: filter.clone(),
        readers: filter,
        items: 1,
        retention_seconds: 3600,
        ..Default::default()
    };
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    custody::set_policy_on(
        &mut tx,
        &owner,
        &policy,
        0,
        now,
        |_| Some("audit fixture".into()),
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    Fixture {
        store,
        owner,
        artifact,
        offer,
        ack,
        now,
    }
}

fn acknowledgment(offer: &Offer, now: i64) -> Acknowledgment {
    Acknowledgment {
        offer: offer.hash().unwrap(),
        artifact: offer.artifact.clone(),
        sender: offer.sender.clone(),
        custodian: offer.custodian.clone(),
        bytes: offer.bytes,
        retained_until: offer.retained_until,
        stored_at: now,
        key_id: "root".into(),
        public_key: "custodian key".into(),
        signature: "protocol acknowledgment fixture".into(),
    }
}

async fn retain(
    f: &Fixture,
    offer: &Offer,
    artifact: &Artifact,
    ack: &Acknowledgment,
) -> Result<Acknowledgment, store::StoreError> {
    let mut tx = store::write_tx(&f.store.pool).await?;
    let stored = custody::retain_on(
        &mut tx,
        custody::Retain {
            person: &f.owner,
            expected_policy: 1,
            offer,
            artifact,
            acknowledgment: ack,
            now: f.now,
        },
        |_| Some("audit fixture".into()),
    )
    .await?;
    tx.commit().await?;
    Ok(stored)
}

async fn count(store: &Store, table: &str) -> i64 {
    store::sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn retention_commits_exact_bytes_once_survives_reopen_and_never_puts_originals_in_facts() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("custody.sqlite").display()
    );
    let mut f = fixture(Store::open_durable(&url).await.unwrap()).await;
    let first = retain(&f, &f.offer, &f.artifact, &f.ack).await.unwrap();
    assert_eq!(first, f.ack);
    let facts = count(&f.store, "fact").await;
    assert_eq!(
        retain(&f, &f.offer, &f.artifact, &f.ack).await.unwrap(),
        first
    );
    assert_eq!(count(&f.store, "fact").await, facts);
    assert_eq!(count(&f.store, "evidence_custody_payload").await, 1);
    assert_eq!(count(&f.store, "evidence_custody_commitment").await, 1);
    let payloads: Vec<String> =
        store::sqlx::query_scalar("SELECT payload FROM fact WHERE payload IS NOT NULL")
            .fetch_all(&f.store.pool)
            .await
            .unwrap();
    assert!(
        payloads
            .iter()
            .all(|payload| !payload.contains("\"statement\""))
    );
    f.store.pool.close().await;
    f.store = Store::open_durable(&url).await.unwrap();
    let body: Vec<u8> = store::sqlx::query_scalar(
        "SELECT body FROM evidence_custody_payload WHERE artifact_hash=?",
    )
    .bind(f.offer.artifact.as_str())
    .fetch_one(&f.store.pool)
    .await
    .unwrap();
    assert_eq!(body, f.artifact.bytes().unwrap());
    let mut tx = f.store.pool.begin().await.unwrap();
    assert_eq!(
        custody::acknowledgment_on(&mut tx, &f.offer).await.unwrap(),
        Some(first)
    );
}

#[tokio::test]
async fn quota_failed_write_private_original_and_excess_retention_never_commit_an_acknowledgment() {
    let f = fixture(Store::open_memory().await.unwrap()).await;
    let facts = count(&f.store, "fact").await;
    store::sqlx::query("CREATE TRIGGER fixture_custody_write_failure BEFORE INSERT ON evidence_custody_commitment BEGIN SELECT RAISE(ABORT,'simulated storage failure'); END").execute(&f.store.pool).await.unwrap();
    assert!(retain(&f, &f.offer, &f.artifact, &f.ack).await.is_err());
    assert_eq!(count(&f.store, "fact").await, facts);
    assert_eq!(count(&f.store, "evidence_custody_payload").await, 0);
    store::sqlx::query("DROP TRIGGER fixture_custody_write_failure")
        .execute(&f.store.pool)
        .await
        .unwrap();
    let mut overlong = f.offer.clone();
    overlong.retained_until = f.now + 3601;
    assert!(
        retain(
            &f,
            &overlong,
            &f.artifact,
            &acknowledgment(&overlong, f.now)
        )
        .await
        .is_err()
    );
    let mut private = f.artifact.clone();
    if let Artifact::Contact(document) = &mut private {
        document.statement.audience = Audience::Selected {
            viewers: vec![Identity::Person(f.owner.clone())],
        };
    }
    let mut private_offer = f.offer.clone();
    private_offer.artifact = private.hash().unwrap();
    private_offer.bytes = private.bytes().unwrap().len() as u32;
    assert!(
        retain(
            &f,
            &private_offer,
            &private,
            &acknowledgment(&private_offer, f.now)
        )
        .await
        .is_err()
    );
    retain(&f, &f.offer, &f.artifact, &f.ack).await.unwrap();
    let mut second = f.offer.clone();
    second.nonce = nucleus::new_uid("cust");
    second.retained_until += 60;
    assert!(
        retain(&f, &second, &f.artifact, &acknowledgment(&second, f.now))
            .await
            .is_err()
    );
    assert_eq!(count(&f.store, "evidence_custody_commitment").await, 1);
    assert_eq!(count(&f.store, "evidence_custody_payload").await, 1);
    let expiry: i64 =
        store::sqlx::query_scalar("SELECT retained_until FROM evidence_custody_commitment")
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
    assert_eq!(expiry, f.offer.retained_until);
}

#[tokio::test]
async fn byte_quota_refuses_a_first_copy_without_retention_or_audit_side_effects() {
    let f = fixture(Store::open_memory().await.unwrap()).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut state = custody::policy_on(&mut tx, &f.owner).await.unwrap();
    state.policy.storage_bytes = u64::from(f.offer.bytes) - 1;
    custody::set_policy_on(
        &mut tx,
        &f.owner,
        &state.policy,
        1,
        f.now,
        |_| Some("audit fixture".into()),
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let facts = count(&f.store, "fact").await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        custody::retain_on(
            &mut tx,
            custody::Retain {
                person: &f.owner,
                expected_policy: 2,
                offer: &f.offer,
                artifact: &f.artifact,
                acknowledgment: &f.ack,
                now: f.now
            },
            |_| Some("audit fixture".into())
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(count(&f.store, "fact").await, facts);
    assert_eq!(count(&f.store, "evidence_custody_payload").await, 0);
    assert_eq!(count(&f.store, "evidence_custody_commitment").await, 0);
}

#[tokio::test]
async fn expired_custody_cleanup_keeps_minimal_controls_then_removes_only_cache_metadata() {
    let f = fixture(Store::open_memory().await.unwrap()).await;
    retain(&f, &f.offer, &f.artifact, &f.ack).await.unwrap();
    let records = count(&f.store, "record").await;
    let facts = count(&f.store, "fact").await;
    let mut interrupted = store::write_tx(&f.store.pool).await.unwrap();
    custody::cleanup_on(&mut interrupted, f.offer.retained_until)
        .await
        .unwrap();
    interrupted.rollback().await.unwrap();
    assert_eq!(count(&f.store, "evidence_custody_payload").await, 1);
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert_eq!(
        custody::cleanup_on(&mut tx, f.offer.retained_until)
            .await
            .unwrap(),
        (1, 0)
    );
    assert_eq!(
        custody::cleanup_on(&mut tx, f.offer.retained_until)
            .await
            .unwrap(),
        (0, 0)
    );
    tx.commit().await.unwrap();
    assert_eq!(count(&f.store, "evidence_custody_commitment").await, 1);
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert_eq!(
        custody::cleanup_on(
            &mut tx,
            f.offer.retained_until + nucleus::trust_analysis::custody::CONTROL_SECONDS
        )
        .await
        .unwrap(),
        (0, 1)
    );
    tx.commit().await.unwrap();
    assert_eq!(count(&f.store, "record").await, records);
    assert_eq!(count(&f.store, "fact").await, facts);
    assert_eq!(count(&f.store, "evidence_custody_payload").await, 0);
}
