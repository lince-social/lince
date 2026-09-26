use std::sync::Arc;

use engine::{
    Engine,
    trust::Signer,
    wire::{Reach, Wire},
};

async fn cell(seed: u8) -> (Arc<Engine>, Arc<Wire>, String, Signer) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let signer = Signer::generate(&organ, "group-test");
    engine.set_signer(signer.clone()).await.unwrap();
    let wire = Arc::new(
        Wire::bind(
            engine.clone(),
            iroh::SecretKey::from_bytes(&[seed; 32]),
            Reach::Local,
        )
        .await
        .unwrap(),
    );
    wire.serve_enrolment();
    (engine, wire, organ, signer)
}

async fn know(engine: &Engine, wire: &Wire, signer: &Signer) {
    store::organs::add_contact(&engine.store.pool, &signer.actor_uid, None, "Peer", "", 1)
        .await
        .unwrap();
    store::organs::set_node_id(
        &engine.store.pool,
        &signer.actor_uid,
        Some(&wire.node_id().to_string()),
    )
    .await
    .unwrap();
    engine::trust::adopt_key(
        &engine.store,
        &signer.actor_uid,
        &signer.key_id,
        &signer.public_key_b64(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn introduction_opens_only_a_fresh_root_after_each_organ_accepts() {
    let (a, aw, ao, ak) = cell(81).await;
    let (b, bw, bo, bk) = cell(82).await;
    let (c, cw, co, ck) = cell(83).await;
    know(&a, &bw, &bk).await;
    know(&b, &aw, &ak).await;
    know(&b, &cw, &ck).await;
    know(&c, &bw, &bk).await;
    let (old_root, old_thread) = b
        .start_conversation(&ao, "Private conversation")
        .await
        .unwrap();
    let secret = b
        .send_message(&old_thread, "Private", "Do not introduce this message")
        .await
        .unwrap();
    assert_eq!(
        b.call_context(&old_thread, None)
            .await
            .unwrap()
            .current_organs,
        vec![ao.clone()]
    );
    let invitation = b
        .propose_group(&old_thread, "New group", &[ao.clone(), co.clone()], None)
        .await
        .unwrap();
    let root = &invitation.membership.root;
    assert_ne!(root, &old_root);
    let encoded = serde_json::to_string(&invitation).unwrap();
    assert!(!encoded.contains(&old_root));
    assert!(!encoded.contains(&old_thread));
    assert!(!encoded.contains(&secret));
    assert!(
        !store::replica::is_accepted(&b.store.pool, root, &ao)
            .await
            .unwrap()
    );
    a.receive_group(&bo, &invitation).await.unwrap();
    c.receive_group(&bo, &invitation).await.unwrap();
    assert!(
        store::organs::contact(&a.store.pool, &co)
            .await
            .unwrap()
            .is_none()
    );
    a.accept_group(root).await.unwrap();
    assert!(
        !store::replica::is_accepted(&a.store.pool, root, &bo)
            .await
            .unwrap()
    );
    let accepted = b.admit_group_organ(&ao, root).await.unwrap();
    a.receive_group(&bo, &accepted).await.unwrap();
    assert!(
        store::replica::is_accepted(&a.store.pool, root, &bo)
            .await
            .unwrap()
    );
    assert!(
        !store::replica::is_accepted(&a.store.pool, root, &co)
            .await
            .unwrap()
    );
    c.accept_group(root).await.unwrap();
    let accepted = b.admit_group_organ(&co, root).await.unwrap();
    a.receive_group(&bo, &accepted).await.unwrap();
    c.receive_group(&bo, &accepted).await.unwrap();
    for (engine, peers) in [(&a, [&bo, &co]), (&b, [&ao, &co]), (&c, [&ao, &bo])] {
        for peer in peers {
            assert!(
                store::replica::is_accepted(&engine.store.pool, root, peer)
                    .await
                    .unwrap()
            );
        }
    }
    let introduced = store::organs::contact(&a.store.pool, &co)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(introduced.trust, "unknown");
    assert!(!introduced.sync_in && !introduced.sync_out);
    let rows = store::sync_ops::ops_missing_from_vector_in_root(&b.store.pool, root, &[], 2000)
        .await
        .unwrap();
    let batch = engine::sync::OpBatch {
        from_organ: bo.clone(),
        ops: b.hydrate_ops(rows).await.unwrap(),
    };
    a.import_grant_batch(root, &batch).await.unwrap();
    c.import_grant_batch(root, &batch).await.unwrap();
    assert!(
        store::records::get(&c.store.pool, &accepted.membership.thread)
            .await
            .unwrap()
            .is_some()
    );
    let message = c
        .send_message(&accepted.membership.thread, "Hello", "Only the new group")
        .await
        .unwrap();
    let rows = store::sync_ops::ops_missing_from_vector_in_root(&c.store.pool, root, &[], 2000)
        .await
        .unwrap()
        .into_iter()
        .filter(|op| op.organ_uid == co)
        .collect();
    let batch = engine::sync::OpBatch {
        from_organ: co.clone(),
        ops: c.hydrate_ops(rows).await.unwrap(),
    };
    a.import_grant_batch(root, &batch).await.unwrap();
    assert_eq!(
        store::records::get(&a.store.pool, &message)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Only the new group"
    );
    assert!(
        !store::replica::is_accepted(&c.store.pool, &old_root, &bo)
            .await
            .unwrap()
    );
    assert!(
        store::records::get(&c.store.pool, &secret)
            .await
            .unwrap()
            .is_none()
    );
    b.remove_group_organ(root, &co, None).await.unwrap();
    let removed = b.group(root).await.unwrap().unwrap();
    a.receive_group(&bo, &removed).await.unwrap();
    c.receive_group(&bo, &removed).await.unwrap();
    assert!(
        !store::replica::is_accepted(&a.store.pool, root, &co)
            .await
            .unwrap()
    );
    assert!(
        !store::replica::is_accepted(&c.store.pool, root, &ao)
            .await
            .unwrap()
    );
    assert!(a.receive_group(&bo, &accepted).await.is_err());
    assert!(b.admit_group_organ(&co, root).await.is_err());
}

#[tokio::test]
async fn forged_or_misaddressed_invitations_never_create_grants() {
    let (a, aw, ao, ak) = cell(84).await;
    let (b, bw, bo, bk) = cell(85).await;
    know(&a, &bw, &bk).await;
    know(&b, &aw, &ak).await;
    let (_, thread) = b.start_conversation(&ao, "Private").await.unwrap();
    let original = b
        .propose_group(&thread, "Group", &[ao], None)
        .await
        .unwrap();
    let mut forged = original.clone();
    forged.membership.title = "Changed by another Organ".into();
    assert!(a.receive_group(&bo, &forged).await.is_err());
    assert!(a.receive_group("wrong-organ", &original).await.is_err());
    assert!(a.group(&original.membership.root).await.unwrap().is_none());
    let mut expired = original.clone();
    expired.membership.invitation_expires = 1;
    let mut bytes = b"lince/conversation-membership/1\0".to_vec();
    bytes.extend(serde_json::to_vec(&expired.membership).unwrap());
    expired.signature = bk.sign_bytes(&bytes);
    assert!(a.receive_group(&bo, &expired).await.is_err());
    store::organs::set_trust(&a.store.pool, &bo, "blocked")
        .await
        .unwrap();
    assert!(a.receive_group(&bo, &original).await.is_err());
}

#[tokio::test]
async fn calls_cross_authenticated_organ_connections_without_a_shared_login() {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let (a, aw, ao, ak) = cell(95).await;
        let (b, bw, bo, bk) = cell(96).await;
        know(&a, &bw, &bk).await;
        know(&b, &aw, &ak).await;
        aw.remember_addr(bw.endpoint().addr());
        bw.remember_addr(aw.endpoint().addr());
        let serve_a = aw.clone();
        let serve_b = bw.clone();
        let a_task = tokio::spawn(async move { serve_a.serve().await });
        let b_task = tokio::spawn(async move { serve_b.serve().await });
        let (_, original) = b.start_conversation(&ao, "Original").await.unwrap();
        let signed = b
            .propose_group(&original, "Group", &[ao.clone()], None)
            .await
            .unwrap();
        let root = &signed.membership.root;
        a.receive_group(&bo, &signed).await.unwrap();
        a.accept_group(root).await.unwrap();
        let accepted = b.admit_group_organ(&ao, root).await.unwrap();
        a.receive_group(&bo, &accepted).await.unwrap();
        let rows = store::sync_ops::ops_missing_from_vector_in_root(&b.store.pool, root, &[], 2000)
            .await
            .unwrap();
        a.import_grant_batch(
            root,
            &engine::sync::OpBatch {
                from_organ: bo.clone(),
                ops: b.hydrate_ops(rows).await.unwrap(),
            },
        )
        .await
        .unwrap();
        let person = store::records::create(
            &a.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Alice",
                body: "",
                quantity: store::exact::one(),
            },
        )
        .await
        .unwrap()
        .uid;
        let role = store::auth::ensure_role(&a.store.pool, "caller")
            .await
            .unwrap();
        store::auth::compare_and_set_role(&a.store.pool, &person, Some(role), 0)
            .await
            .unwrap();
        let read = store::auth::ensure_permission(&a.store.pool, "record", "read")
            .await
            .unwrap();
        store::auth::grant(&a.store.pool, role, read).await.unwrap();
        store::role_policies::set(
            &a.store.pool,
            role,
            &serde_json::json!({"read":{"all":[]},"grants":[]}),
            0,
        )
        .await
        .unwrap();
        a.set_group_person(root, &person, true, None).await.unwrap();
        let started = a
            .call_for_session(
                signed.membership.thread.clone(),
                Some(person.clone()),
                None,
                "alice-device",
                engine::calls::Operation::Start,
            )
            .await
            .unwrap();
        assert_eq!(started.participants[0].identity.organ, ao);
        assert_eq!(started.participants[0].identity.person, person);
        assert!(
            store::logins::person_for_organ(&b.store.pool, &ao)
                .await
                .unwrap()
                .is_none()
        );
        b.remove_group_organ(root, &ao, None).await.unwrap();
        assert!(
            a.call_for_session(
                signed.membership.thread.clone(),
                Some(person),
                None,
                "alice-device",
                engine::calls::Operation::Poll {
                    call: started.call.unwrap()
                }
            )
            .await
            .is_err()
        );
        b.sweep_calls().await.unwrap();
        a_task.abort();
        b_task.abort();
    })
    .await
    .expect("authenticated call signaling timed out");
}

#[tokio::test]
async fn a_group_never_reassigns_its_coordinator_after_authorization_is_removed() {
    let (a, aw, ao, ak) = cell(97).await;
    let (b, bw, bo, bk) = cell(98).await;
    know(&b, &aw, &ak).await;
    know(&a, &bw, &bk).await;
    let (_, original) = b.start_conversation(&ao, "Original").await.unwrap();
    let group = b
        .propose_group(&original, "Group", &[ao], None)
        .await
        .unwrap();
    let thread = &group.membership.thread;
    let inspect = || {
        b.call_for_session(
            thread.clone(),
            None,
            None,
            "device",
            engine::calls::Operation::Inspect,
        )
    };
    inspect().await.unwrap();
    let mut coordinator = engine::roster::CellEntry {
        cell_uid: nucleus::new_uid("r"),
        node_id: bw.node_id().to_string(),
        label: "Coordinator".into(),
        operational_key: bk.public_key_b64(),
        sealing_key: None,
        front_door: true,
        capabilities: engine::roster::full_capabilities(),
    };
    b.publish_roster(&bk, vec![coordinator.clone()])
        .await
        .unwrap();
    inspect().await.unwrap();
    coordinator.capabilities.clear();
    b.publish_roster(&bk, vec![coordinator.clone()])
        .await
        .unwrap();
    assert!(inspect().await.is_err());
    coordinator.capabilities = engine::roster::full_capabilities();
    coordinator.node_id = aw.node_id().to_string();
    b.publish_roster(&bk, vec![coordinator]).await.unwrap();
    assert!(inspect().await.is_err());
    assert_eq!(
        b.group(&group.membership.root)
            .await
            .unwrap()
            .unwrap()
            .membership
            .owner,
        bo
    );
}
