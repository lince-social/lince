//! Deactivating someone across an Organ's Cells (Ontology C3).
//!
//! Standing has to travel or it is not deactivation: turning someone off on the
//! laptop while the VPS Cell still takes their password is a checkbox, not a
//! decision. And it must travel to OUR CELLS ONLY — outbound because "who did
//! this Organ turn off, and when" is a statement about a person and nobody
//! else's business, inbound because a contact who could write this field could
//! lock an Organ out of its own Cell.
//!
//! The inbound half is tested by hand-assembling the batch our own filter would
//! never send, because a filter is our policy on our side and proves nothing
//! about what can arrive.

use engine::sync::Delivery;
use engine::Engine;
use engine::trust::Signer;

async fn cell() -> (Engine, String) {
    let engine = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&engine.store.pool, "http://cell.test")
        .await
        .expect("local organ")
        .uid;
    engine
        .set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (engine, organ)
}

/// Same fixture as `karma_sync.rs`, and for the same reason: enrolment (C3's
/// own remaining work) is what makes a real sibling, so until it exists the
/// local Organ uid is rewritten AFTER pairing. Pair as strangers, then become
/// siblings — an Organ is not its own contact.
async fn become_sibling_of(engine: &Engine, organ_uid: &str) {
    let existing = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .expect("local organ")
        .uid;
    let mut connection = engine.store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    for statement in [
        "DELETE FROM organ_contact WHERE record_uid = ?",
        "DELETE FROM record WHERE uid = ?",
    ] {
        store::sqlx::query(statement)
            .bind(organ_uid)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    for statement in [
        "UPDATE record SET organ_uid = ? WHERE organ_uid = ?",
        "UPDATE record SET uid = ? WHERE uid = ?",
        "UPDATE sync_op SET organ_uid = ? WHERE organ_uid = ?",
    ] {
        store::sqlx::query(statement)
            .bind(organ_uid)
            .bind(&existing)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    store::sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await
        .unwrap();
}

async fn pair(from: &Engine, from_organ: &str, to: &Engine, to_organ: &str) {
    let from_intro = from.introduction().await.unwrap();
    let to_intro = to.introduction().await.unwrap();
    to.adopt_introduction(&from_intro, 1).await.unwrap();
    from.adopt_introduction(&to_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&from.store.pool, to_organ, true, false)
        .await
        .unwrap();
    store::organs::set_sync_policy(&to.store.pool, from_organ, true, false)
        .await
        .unwrap();
}

/// A sibling catches up by PULLING the log — `organ_contact` cannot represent
/// one, because it is keyed by the contact's Organ uid and a sibling's is ours.
async fn deliver_to_sibling(from: &Engine, to: &Engine, organ: &str) {
    let (ops, _head) = from.ops_after(0, 1_000).await.unwrap();
    to.import_op_batch(&engine::sync::OpBatch {
        from_organ: organ.to_string(),
        ops,
    })
    .await
    .expect("import");
}

async fn deliver(from: &Engine, to: &Engine) {
    let target = to;
    from.drain_outbox(|_contact, root, batch| async move {
        match root {
            Some(root) => target.import_grant_batch(&root, &batch).await,
            None => target.import_op_batch(&batch).await,
        }
        .map_or_else(|e| Delivery::Failed(e.to_string()), |_| Delivery::Sent)
    })
    .await
    .expect("drain");
}

async fn person(engine: &Engine, slug: &str) -> String {
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some(slug),
            kind: nucleus::RecordKind::Person,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("person record")
    .uid
}

/// The point: deactivate here, and the Cell that holds the password agrees.
#[tokio::test]
async fn deactivating_someone_reaches_this_organs_other_cell() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;
    become_sibling_of(&b, &organ).await;

    let uid = person(&a, "maria").await;
    deliver_to_sibling(&a, &b, &organ).await;
    assert!(
        store::people::is_active(&b.store.pool, &uid).await.unwrap(),
        "precondition: she can act on both Cells to begin with"
    );

    store::people::deactivate(
        &a.store.pool,
        &uid,
        "2026-08-15T12:00:00Z",
        Some("moved out"),
    )
    .await
    .unwrap();
    deliver_to_sibling(&a, &b, &organ).await;

    assert!(
        !store::people::is_active(&b.store.pool, &uid).await.unwrap(),
        "the other Cell must stop accepting her too"
    );
    assert_eq!(
        store::people::standing(&b.store.pool, &uid)
            .await
            .unwrap()
            .and_then(|standing| standing.at),
        Some("2026-08-15T12:00:00Z".to_string()),
    );
}

/// People come back, and the return travels the same way. A reactivation that
/// did not cross would leave the VPS Cell refusing someone the laptop lets in —
/// worse than the original bug, because it looks like it worked.
#[tokio::test]
async fn reactivating_reaches_the_other_cell_too() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;
    become_sibling_of(&b, &organ).await;

    let uid = person(&a, "joao").await;
    store::people::deactivate(&a.store.pool, &uid, "2026-08-15T12:00:00Z", None)
        .await
        .unwrap();
    deliver_to_sibling(&a, &b, &organ).await;
    assert!(!store::people::is_active(&b.store.pool, &uid).await.unwrap());

    store::people::reactivate(&a.store.pool, &uid)
        .await
        .unwrap();
    deliver_to_sibling(&a, &b, &organ).await;

    assert!(
        store::people::is_active(&b.store.pool, &uid).await.unwrap(),
        "the other Cell must let him back in"
    );
}

/// Layer one: nothing leaves. A contact's feed carries the Person — they may
/// well know her — and carries no word about whether we turned her off.
///
/// Asserted on the BATCH WE SEND, not on what the peer ends up holding. The
/// obvious version of this test — deliver, then look at the receiver — passes
/// with the outbound filter deleted, because the receiver's own admissibility
/// gate refuses the op anyway. Two guards covering each other is exactly the
/// arrangement in which a test proves neither.
#[tokio::test]
async fn a_contacts_feed_never_carries_a_persons_standing() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;

    let uid = person(&a, "maria").await;
    store::people::deactivate(
        &a.store.pool,
        &uid,
        "2026-08-15T12:00:00Z",
        Some("moved out"),
    )
    .await
    .unwrap();

    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let collector = std::sync::Arc::clone(&sent);
    a.drain_outbox(move |_contact, _root, batch| {
        let collector = std::sync::Arc::clone(&collector);
        async move {
            collector.lock().unwrap().extend(batch.ops.iter().cloned());
            Delivery::Sent
        }
    })
    .await
    .expect("drain");

    let sent = sent.lock().unwrap();
    assert!(
        sent.iter().any(|op| op.uid == uid),
        "precondition: the Person herself does travel, so the absence below is the filter"
    );
    assert!(
        !sent
            .iter()
            .any(|op| store::people::is_standing_field(&op.field)),
        "our membership admin is nobody else's business: {:?}",
        sent.iter().map(|op| &op.field).collect::<Vec<_>>()
    );
}

/// Layer two, and the one that matters: a standing op that bypasses our own
/// outbound filter is REFUSED on arrival. Our filter is our policy on our side;
/// it says nothing about what a hostile peer can assemble by hand.
///
/// Unlike a Karma definition — which a contact may hold inertly, doing nothing —
/// this one cannot be allowed to land at all: materialising it would be a
/// contact locking us out of our own Cell.
#[tokio::test]
async fn a_contact_cannot_deactivate_one_of_our_people() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;

    // `b` knows this Person because we sync with them; the uid is not a secret.
    let uid = person(&a, "maria").await;
    deliver(&a, &b).await;

    // Hand-assembled: the shape our own `drain_outbox` refuses to send.
    let hostile = engine::sync::OpBatch {
        from_organ: b_organ.clone(),
        ops: vec![engine::sync::WireOp {
            tbl: "record_extension".into(),
            uid: uid.clone(),
            field: format!(
                "{}.{}",
                store::people::NAMESPACE,
                store::people::STANDING_KEY
            ),
            kind: "set".into(),
            value: Some(
                serde_json::json!({ "active": false, "at": "2026-08-15T12:00:00Z" }).to_string(),
            ),
            hlc: nucleus::hlc::next(),
            actor_cell: format!("{b_organ}-cell"),
            organ_uid: b_organ.clone(),
            fact: None,
        }],
    };
    a.import_op_batch(&hostile).await.expect("import returns");

    assert!(
        store::people::is_active(&a.store.pool, &uid).await.unwrap(),
        "a contact must not be able to close an account in our Organ"
    );
    assert_eq!(
        store::records::get_extension(&a.store.pool, &uid, store::people::NAMESPACE)
            .await
            .unwrap(),
        None,
        "and it must not even land — refused, not stored-and-ignored"
    );
}
