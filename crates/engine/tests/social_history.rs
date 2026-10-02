use engine::{
    Engine,
    actions::Action,
    pairing::EnrolmentInvite,
    roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
    trust::Signer,
};
use nucleus::social::{Command, PRIVATE_NAMESPACE, PROFILE_NAMESPACE, ProfileFields};
use serde_json::json;

async fn copy(source: &Engine, target: &Engine, organ: &str) {
    let vector = store::sync_ops::version_vector_for_organ(&target.store.pool, organ)
        .await
        .unwrap();
    let page = source.export_sync_page(organ, &vector, 2000).await.unwrap();
    target.receive_sync_batch(organ, &page.batch).await.unwrap();
    assert_eq!(
        target.receive_sync_batch(organ, &page.batch).await.unwrap(),
        0
    );
}

#[tokio::test]
async fn retained_history_and_private_mapping_reuse_own_sync_on_existing_and_new_devices() {
    let source = Engine::open_memory().await.unwrap();
    let second = Engine::open_memory().await.unwrap();
    let third = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&source.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [91; 32]);
    let mut keys = Vec::new();
    let mut members = Vec::new();
    for (engine, label) in [(&source, "source"), (&second, "second"), (&third, "third")] {
        let key = engine.operational_key_for(&organ).await.unwrap();
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap();
        members.push(CellEntry {
            cell_uid: cell.uid,
            node_id: label.into(),
            label: label.into(),
            operational_key: key.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: full_capabilities(),
        });
        keys.push(key);
    }
    source.set_organ_signer(keys[0].clone()).await.unwrap();
    source.set_signer(keys[0].clone()).await.unwrap();
    source.publish_root_key(&root).await.unwrap();
    let roster = source
        .publish_roster(&root, members[..2].to_vec())
        .await
        .unwrap();
    let invitation = EnrolmentInvite {
        node_id: "source".into(),
        organ_uid: organ.clone(),
        root_key: root.public_key_b64(),
        token: "own-social-history".into(),
        addrs: Vec::new(),
    };
    second
        .join_organ(&invitation, &roster, keys[1].clone())
        .await
        .unwrap();
    second.set_signer(keys[1].clone()).await.unwrap();
    let contact = nucleus::new_uid("r");
    store::organs::add_contact(&source.store.pool, &contact, None, "Contact", "", 1)
        .await
        .unwrap();
    let (conversation, thread) = source
        .start_conversation(&contact, "Bicycle conversation")
        .await
        .unwrap();
    let message = source
        .send_message(&thread, "Hello", "Retained private conversation")
        .await
        .unwrap();
    let mapping = json!({"participant":"pseudonymous-peer","accepted":true,"reveal":false,"request":"private-request"});
    store::records::set_extension(
        &source.store.pool,
        &conversation,
        "lince.social.participants",
        &mapping,
    )
    .await
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [91; 32]).unwrap();
    source.set_root_key_path(root_path);
    source.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
    let profile = source
        .act(
            Action::Social {
                request: Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Workshop".into(),
                        ..Default::default()
                    },
                    parents: vec![],
                    destinations: vec![],
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    copy(&source, &second, &organ).await;
    let roster = source.publish_roster(&root, members).await.unwrap();
    third
        .join_organ(&invitation, &roster, keys[2].clone())
        .await
        .unwrap();
    third.set_signer(keys[2].clone()).await.unwrap();
    copy(&source, &third, &organ).await;
    for target in [&second, &third] {
        for uid in [&conversation, &thread, &message] {
            let row = store::records::get(&target.store.pool, uid)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(row.uid, *uid);
        }
        assert_eq!(
            store::records::get(&target.store.pool, &message)
                .await
                .unwrap()
                .unwrap()
                .body,
            "Retained private conversation"
        );
        assert_eq!(
            store::records::get_extension(
                &target.store.pool,
                &conversation,
                "lince.social.participants"
            )
            .await
            .unwrap()
            .unwrap(),
            mapping
        );
        assert_eq!(
            store::records::get_extension(&target.store.pool, &organ, PROFILE_NAMESPACE)
                .await
                .unwrap()
                .unwrap()["published"],
            profile["profile"]
        );
        assert!(
            store::records::get_extension(&target.store.pool, &organ, PRIVATE_NAMESPACE)
                .await
                .unwrap()
                .unwrap()["profile_signer"]["authority"]
                .is_object()
        );
    }
    let hash = profile["hash"].as_str().unwrap().to_owned();
    let revised = second
        .act(
            Action::Social {
                request: Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Shared updated workshop".into(),
                        ..Default::default()
                    },
                    parents: vec![hash],
                    destinations: vec![],
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(revised["profile"]["authority"]["organ"], organ);
    assert_eq!(
        revised["profile"]["authority"]["editor_key"],
        profile["profile"]["authority"]["editor_key"]
    );
    copy(&second, &source, &organ).await;
    assert_eq!(
        store::records::get_extension(&source.store.pool, &organ, PROFILE_NAMESPACE)
            .await
            .unwrap()
            .unwrap()["published"],
        revised["profile"]
    );
    assert!(
        store::records::get(&source.store.pool, &conversation)
            .await
            .unwrap()
            .is_some()
    );
    let peer = Signer::from_bytes("", "social", [92; 32]).public_key_b64();
    let window = nucleus::execution::now().timestamp();
    store::records::set_extension(
        &source.store.pool,
        &conversation,
        nucleus::social::requests::BLOCK_NAMESPACE,
        &json!({peer.clone(): {"blocked":true,"window":window}}),
    )
    .await
    .unwrap();
    store::records::mark_deleted(&source.store.pool, &conversation)
        .await
        .unwrap();
    copy(&source, &third, &organ).await;
    assert_eq!(
        source
            .act(
                Action::Social {
                    request: Command::Requests { after: None }
                },
                None
            )
            .await
            .unwrap()
            .data
            .unwrap()["blocks"],
        third
            .act(
                Action::Social {
                    request: Command::Requests { after: None }
                },
                None
            )
            .await
            .unwrap()
            .data
            .unwrap()["blocks"]
    );
    source
        .act(
            Action::Social {
                request: Command::UnblockParticipant {
                    context: conversation.clone(),
                    peer: peer.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    copy(&source, &third, &organ).await;
    let decisions =
        store::records::get_extension(&third.store.pool, &organ, "lince.social.retained-blocks")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        decisions[format!("{conversation}:{peer}")]["blocked"],
        false
    );
    assert!(
        decisions[format!("{conversation}:{peer}")]["window"]
            .as_i64()
            .unwrap()
            > window
    );
    assert!(
        third
            .act(
                Action::Social {
                    request: Command::Requests { after: None }
                },
                None
            )
            .await
            .unwrap()
            .data
            .unwrap()["blocks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        store::records::get(&third.store.pool, &conversation)
            .await
            .unwrap()
            .is_none()
    );
}
