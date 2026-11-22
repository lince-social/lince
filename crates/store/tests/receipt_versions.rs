use chrono::TimeDelta;
use nucleus::{
    RecordKind,
    execution::now,
    karma::canonical_hash,
    new_uid,
    transfer::receipt::{
        Approval, ApprovalRole, Command, Document, PartyAttribution, Review, Scope, Source, Stage,
    },
    trust_analysis::contacts::Audience,
};
use store::{
    Store,
    exact::integer,
    organs::ensure_local,
    records::{NewRecord, create},
    transfer_receipts::{self, Input},
    write_tx,
};

#[tokio::test]
async fn a_receipt_request_replays_the_original_version_after_an_append()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    ensure_local(pool, "http://receipt.test").await?;
    let person = create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Person,
            head: "Receipt author",
            body: "",
            quantity: integer(1),
        },
    )
    .await?
    .uid;
    let at = now();
    let record = new_uid("r");
    let origin = new_uid("r");
    let commitment = canonical_hash("lince.store.test-receipt.v1", &"agreement")?;
    let mut review = Review {
        person_approvals: Vec::new(),
        scope: Scope {
            audience: Audience::Public,
            upfront_terms: None,
            public_persons: Vec::new(),
            source: Source {
                occurrence: Source::occurrence_id(&origin, &new_uid("toc"))?,
                origin,
                activated_at: (at - TimeDelta::seconds(60)).timestamp(),
                agreement_revision: 1,
                agreement_commitment: commitment.clone(),
                activation_commitment: commitment,
            },
            party_attribution: PartyAttribution::LaterOrganEndorsement,
            original_authorities: Vec::new(),
            giver: new_uid("r"),
            receiver: new_uid("r"),
            measurements: Vec::new(),
            stages: vec![Stage::Settled],
            agreed_quantity: None,
            canonical_unit: None,
            issued_at: at.timestamp(),
            expires_at: (at + TimeDelta::hours(1)).timestamp(),
        },
        approvals: Vec::new(),
        authorities: Vec::new(),
    };
    let original = Document::Review(review.clone());
    let input = |id: &str, document: Document, expected| Input {
        record: record.clone(),
        person: person.clone(),
        request_id: id.into(),
        command: Command::Import {
            document: serde_json::to_string(&document).unwrap(),
        },
        document,
        expected,
        local_source: None,
        authorization_intent: None,
    };
    let sign = |_: &str| Some("fixture-signature".into());
    let (first, tx) = transfer_receipts::save(
        write_tx(pool).await?,
        input("first", original.clone(), None),
        at,
        sign,
    )
    .await?;
    tx.commit().await?;
    assert!(first.created);
    assert_eq!(first.saved.version, 1);
    assert_eq!(
        first.saved.fact.payload.as_deref(),
        Some(serde_json::to_string(&original)?.as_str())
    );
    let (duplicate, tx) = transfer_receipts::save(
        write_tx(pool).await?,
        input("duplicate", original.clone(), Some((record.clone(), 1))),
        at,
        |_| None,
    )
    .await?;
    tx.commit().await?;
    assert!(!duplicate.created);
    assert_eq!(duplicate.saved.fact.uid, first.saved.fact.uid);

    review.approvals.push(Approval {
        role: ApprovalRole::Origin,
        issued_at: at.timestamp(),
        key_id: "authority".into(),
        signature: "a".repeat(88),
    });
    let revised = Document::Review(review);
    let (second, tx) = transfer_receipts::save(
        write_tx(pool).await?,
        input("append", revised.clone(), Some((record.clone(), 1))),
        at,
        sign,
    )
    .await?;
    tx.commit().await?;
    assert!(second.created);
    assert_eq!(second.saved.version, 2);
    assert_eq!(
        transfer_receipts::get(pool, &record).await?.document,
        revised
    );

    let command = input("first", original.clone(), None).command;
    let replayed = transfer_receipts::replay(pool, &person, "first", &command)
        .await?
        .expect("saved request must be replayable");
    assert_eq!(replayed.version, 1);
    assert_eq!(replayed.document, original);
    assert_eq!(replayed.fact.uid, first.saved.fact.uid);
    assert!(
        transfer_receipts::replay(pool, &person, "first", &Command::List)
            .await
            .is_err()
    );
    assert_eq!(
        transfer_receipts::versions(pool, &record, None)
            .await?
            .len(),
        2
    );
    let Document::Review(mut rollback) = original else {
        unreachable!("the fixture started as a receipt review")
    };
    rollback.scope.expires_at += 1;
    let mut rejected = input("unsigned", Document::Review(rollback), None);
    rejected.record = new_uid("r");
    let rejected_record = rejected.record.clone();
    assert!(
        transfer_receipts::save(write_tx(pool).await?, rejected, at, |_| None)
            .await
            .is_err()
    );
    let exists: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=?)")
        .bind(&rejected_record)
        .fetch_one(pool)
        .await?;
    assert!(!exists);
    Ok(())
}

#[tokio::test]
async fn delivery_inbox_breaks_equal_timestamp_ties_by_record_id()
-> Result<(), Box<dyn std::error::Error>> {
    use nucleus::transfer::receipt_delivery::MIME;
    use store::{
        artifact_deliveries::{InboxPosition, inbox_page},
        sqlx,
    };

    let store = Store::open_memory().await?;
    let pool = &store.pool;
    ensure_local(pool, "http://receipt.test").await?;
    let created_at = now().to_rfc3339();
    for label in ["First", "Second", "Third"] {
        let message = create(
            pool,
            NewRecord {
                slug: None,
                kind: RecordKind::Message,
                head: label,
                body: "",
                quantity: integer(0),
            },
        )
        .await?;
        sqlx::query("UPDATE record SET created_at=? WHERE uid=?")
            .bind(&created_at)
            .bind(&message.uid)
            .execute(pool)
            .await?;
        sqlx::query(
            "INSERT INTO record_extension(record_uid,namespace,fds) VALUES(?,'lince.message-content',?)",
        )
        .bind(&message.uid).bind(serde_json::json!({"parts":[{"mime_type":MIME}]}).to_string())
        .execute(pool).await?;
    }
    let page = inbox_page(pool, None).await?;
    let [first, second, third] = page.as_slice() else {
        unreachable!("the fixture stores three receipts");
    };
    assert!(first.message > second.message && second.message > third.message);
    assert!(
        page.iter()
            .all(|entry| !entry.outgoing && entry.created_at == created_at)
    );
    let before = InboxPosition {
        created_at,
        message: second.message.clone(),
    };
    let rest = inbox_page(pool, Some(&before)).await?;
    assert_eq!(
        rest.into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>()
            .as_slice(),
        std::slice::from_ref(&third.message)
    );
    Ok(())
}
