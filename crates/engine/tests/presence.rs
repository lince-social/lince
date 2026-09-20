use engine::{Engine, actions::Action, presence::Cursor, trust::Signer};
use nucleus::RecordKind;

async fn organ() -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    let uid = store::organs::ensure_local(&engine.store.pool, "http://local")
        .await
        .unwrap()
        .uid;
    engine
        .set_signer(Signer::generate(&uid, "key"))
        .await
        .unwrap();
    (engine, uid)
}

async fn pair(a: &Engine, b: &Engine) {
    let ai = a.introduction().await.unwrap();
    let bi = b.introduction().await.unwrap();
    a.adopt_introduction(&bi, 1).await.unwrap();
    b.adopt_introduction(&ai, 1).await.unwrap();
    store::organs::set_trust(&a.store.pool, &bi.organ_uid, "known")
        .await
        .unwrap();
    store::organs::set_trust(&b.store.pool, &ai.organ_uid, "known")
        .await
        .unwrap();
    store::organs::set_sync_policy(&a.store.pool, &bi.organ_uid, true, true)
        .await
        .unwrap();
    store::organs::set_sync_policy(&b.store.pool, &ai.organ_uid, true, true)
        .await
        .unwrap();
}

#[tokio::test]
async fn owner_relays_cursors_between_unconnected_participants_without_persisting_them() {
    let (a, ao) = organ().await;
    let (b, bo) = organ().await;
    let (c, co) = organ().await;
    pair(&a, &b).await;
    pair(&a, &c).await;
    let uid = a
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Title".into(),
                body: "Olá".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    a.drain_outbox(|contact, _, batch| {
        let engine = if contact.record_uid == bo { &b } else { &c };
        async move {
            engine.import_op_batch(&batch).await.unwrap();
            engine::sync::Delivery::Sent
        }
    })
    .await
    .unwrap();
    let before = store::sync_ops::max_seq(&a.store.pool).await.unwrap();
    b.presence.join(&uid, "same-session");
    c.presence.join(&uid, "same-session");
    b.presence.cursor(
        &uid,
        Cursor {
            session: "same-session".into(),
            person: None,
            organ: None,
            property: "body".into(),
            anchor: "anchor".into(),
            focus: "focus".into(),
        },
    );
    let records = vec![uid.clone()];
    let sent = b.presence_for(&ao, &records).await.unwrap();
    assert_eq!(
        sent.len(),
        1,
        "record: {:?}; contact: {:?}",
        store::records::get(&b.store.pool, &uid).await.unwrap(),
        store::organs::contact(&b.store.pool, &ao).await.unwrap()
    );
    a.exchange_presence(&bo, &records, sent).await.unwrap();
    let relayed = a.exchange_presence(&co, &records, vec![]).await.unwrap();
    assert_eq!(relayed.len(), 1);
    c.receive_presence(&ao, relayed.clone()).await.unwrap();
    let visible = c.presence.cursors(&uid);
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].organ.as_deref(), Some(bo.as_str()));
    assert_eq!(visible[0].session, format!("{bo}/same-session"));
    assert_eq!(
        store::sync_ops::max_seq(&a.store.pool).await.unwrap(),
        before
    );
    let mut forged = relayed;
    forged[0].origin = co.clone();
    a.exchange_presence(&bo, &records, forged).await.unwrap();
    assert!(a.presence.cursors(&uid).is_empty());
    a.exchange_presence(&bo, &records, b.presence_for(&ao, &records).await.unwrap())
        .await
        .unwrap();
    store::visibility::set_hidden_from_organ(&a.store.pool, &co, &uid, true)
        .await
        .unwrap();
    assert!(a.presence_for(&co, &records).await.unwrap().is_empty());
    store::visibility::set_hidden_from_organ(&a.store.pool, &co, &uid, false)
        .await
        .unwrap();
    store::organs::set_contact_scope(&a.store.pool, &co, Some(&["head".into()]))
        .await
        .unwrap();
    assert!(a.presence_for(&co, &records).await.unwrap().is_empty());
    b.presence.leave_cursor(None, "same-session");
    a.exchange_presence(&bo, &[], vec![]).await.unwrap();
    assert!(a.presence.cursors(&uid).is_empty());
    c.receive_presence(&ao, vec![]).await.unwrap();
    assert!(c.presence.cursors(&uid).is_empty());
}

#[tokio::test]
async fn unknown_contacts_need_an_accepted_grant_and_revocation_hides_presence() {
    let (owner, _) = organ().await;
    let (guest, guest_uid) = organ().await;
    owner
        .adopt_introduction(&guest.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    let uid = owner
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Private".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    owner.presence.cursor(
        &uid,
        Cursor {
            session: "owner".into(),
            person: None,
            organ: None,
            property: "head".into(),
            anchor: "a".into(),
            focus: "b".into(),
        },
    );
    let records = vec![uid.clone()];
    assert!(
        owner
            .presence_for(&guest_uid, &records)
            .await
            .unwrap()
            .is_empty()
    );
    store::replica::make_own_root(&owner.store.pool, &uid)
        .await
        .unwrap();
    store::replica::offer(&owner.store.pool, &uid, &guest_uid)
        .await
        .unwrap();
    assert!(
        owner
            .presence_for(&guest_uid, &records)
            .await
            .unwrap()
            .is_empty()
    );
    store::replica::accept(&owner.store.pool, &uid, &guest_uid)
        .await
        .unwrap();
    assert_eq!(
        owner
            .presence_for(&guest_uid, &records)
            .await
            .unwrap()
            .len(),
        1
    );
    store::replica::revoke(&owner.store.pool, &uid, &guest_uid)
        .await
        .unwrap();
    assert!(
        owner
            .presence_for(&guest_uid, &records)
            .await
            .unwrap()
            .is_empty()
    );
}
