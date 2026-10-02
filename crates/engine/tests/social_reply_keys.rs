use engine::{
    Engine,
    actions::Action,
    pairing::EnrolmentInvite,
    roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
    trust::Signer,
};
use nucleus::social::{
    Command, PRIVATE_NAMESPACE, PostDraft,
    requests::{AUTHORITY_LIFETIME, CertifiedRoute, SESSION_AUTHORITY_NAMESPACE},
};
use serde_json::{Value, json};

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .act(Action::Social { request }, None)
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn copy(source: &Engine, target: &Engine, organ: &str) {
    let vector = store::sync_ops::version_vector_for_organ(&target.store.pool, organ)
        .await
        .unwrap();
    let page = source.export_sync_page(organ, &vector, 2000).await.unwrap();
    target.receive_sync_batch(organ, &page.batch).await.unwrap();
}

async fn device(engine: &Engine, organ: &str, label: &str, secret: u8) -> (CellEntry, Signer) {
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let key = Signer::from_bytes(organ, &engine::roster::cell_key_id(&cell.uid), [secret; 32]);
    let local_organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let local_key = Signer::from_bytes(&local_organ, &key.key_id, [secret; 32]);
    engine.set_organ_signer(local_key.clone()).await.unwrap();
    engine.set_signer(local_key).await.unwrap();
    (
        CellEntry {
            cell_uid: cell.uid,
            node_id: label.into(),
            label: label.into(),
            operational_key: key.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: full_capabilities(),
        },
        key,
    )
}

#[tokio::test]
async fn anonymous_post_editing_is_leased_across_devices_and_owner_rotates_after_removal() {
    use nucleus::social::{PUBLICATION_NAMESPACE, PostState, Snippet};
    let owner = Engine::open_memory().await.unwrap();
    let second = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    owner.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [202; 32]).unwrap();
    owner.set_root_key_path(root_path);
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [202; 32]);
    let (first, _) = device(&owner, &organ, "owner", 203).await;
    let (next, next_key) = device(&second, &organ, "second", 204).await;
    owner.publish_root_key(&root).await.unwrap();
    let roster = owner
        .publish_roster(&root, vec![first.clone(), next])
        .await
        .unwrap();
    second
        .join_organ(
            &EnrolmentInvite {
                node_id: "owner".into(),
                organ_uid: organ.clone(),
                root_key: root.public_key_b64(),
                token: "anonymous-editing".into(),
                addrs: vec![],
            },
            &roster,
            next_key.clone(),
        )
        .await
        .unwrap();
    second.set_signer(next_key).await.unwrap();
    let saved = command(
        &owner,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Bicycle help".into(),
                ..Default::default()
            },
        },
    )
    .await;
    let record = saved["record"].as_str().unwrap().to_owned();
    let reviewed = command(
        &owner,
        Command::Preview {
            record: record.clone(),
            state: PostState::Active,
        },
    )
    .await;
    let original: Snippet = serde_json::from_value(reviewed["document"].clone()).unwrap();
    command(
        &owner,
        Command::Publish {
            record: record.clone(),
            preview_hash: reviewed["preview_hash"].as_str().unwrap().into(),
            document: original.clone(),
        },
    )
    .await;
    copy(&owner, &second, &organ).await;
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_device_state")
            .fetch_one(&second.store.pool)
            .await
            .unwrap(),
        0
    );
    let other = command(
        &second,
        Command::Preview {
            record: record.clone(),
            state: PostState::Active,
        },
    )
    .await;
    assert_eq!(other["document"]["id"], original.id);
    assert_eq!(other["document"]["signing_key"], original.signing_key);
    let waiting = command(
        &second,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Another anonymous need".into(),
                ..Default::default()
            },
        },
    )
    .await;
    let waiting_record = waiting["record"].as_str().unwrap().to_owned();
    assert!(
        second
            .act(
                Action::Social {
                    request: Command::Preview {
                        record: waiting_record.clone(),
                        state: PostState::Active
                    }
                },
                None
            )
            .await
            .is_err()
    );
    copy(&second, &owner, &organ).await;
    command(
        &owner,
        Command::Preview {
            record: waiting_record.clone(),
            state: PostState::Active,
        },
    )
    .await;
    copy(&owner, &second, &organ).await;
    command(
        &second,
        Command::Preview {
            record: waiting_record,
            state: PostState::Active,
        },
    )
    .await;
    let revoked = owner.publish_roster(&root, vec![first]).await.unwrap();
    second.adopt_roster(&revoked).await.unwrap();
    assert!(
        second
            .act(
                Action::Social {
                    request: Command::Preview {
                        record: record.clone(),
                        state: PostState::Active
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let changed = command(
        &owner,
        Command::Preview {
            record: record.clone(),
            state: PostState::Withdrawn,
        },
    )
    .await;
    assert_eq!(changed["document"]["id"], original.id);
    assert_eq!(changed["document"]["anonymous"]["generation"], "2");
    assert_ne!(changed["document"]["signing_key"], original.signing_key);
    let state = store::records::get_extension(&owner.store.pool, &record, PUBLICATION_NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state["anonymous_authority"]["owner_key"],
        original.anonymous.unwrap().owner_key
    );
}

#[tokio::test]
async fn new_device_uses_own_sync_for_authorization_and_never_copies_live_keys() {
    let owner = Engine::open_memory().await.unwrap();
    let second = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    owner.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
    second.set_sealing_keyring_path(directory.path().join("second/sealing.json"));
    let root_path = directory.path().join("root.key");
    std::fs::write(&root_path, [150; 32]).unwrap();
    owner.set_root_key_path(root_path);
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [150; 32]);
    let (first_member, _) = device(&owner, &organ, "owner", 151).await;
    let (second_member, second_key) = device(&second, &organ, "second", 152).await;
    owner.publish_root_key(&root).await.unwrap();
    let roster = owner
        .publish_roster(&root, vec![first_member.clone(), second_member.clone()])
        .await
        .unwrap();
    second
        .join_organ(
            &EnrolmentInvite {
                node_id: "owner".into(),
                organ_uid: organ.clone(),
                root_key: root.public_key_b64(),
                token: "private-replies".into(),
                addrs: vec![],
            },
            &roster,
            second_key.clone(),
        )
        .await
        .unwrap();
    second.set_signer(second_key).await.unwrap();
    let saved = command(
        &owner,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Bicycle help".into(),
                text: "A private introduction is welcome".into(),
                ..Default::default()
            },
        },
    )
    .await;
    let context = saved["record"].as_str().unwrap().to_owned();
    let hosts = vec![iroh::SecretKey::from_bytes(&[153; 32]).public().to_string()];
    let first = command(
        &owner,
        Command::PrepareReplyKeys {
            record: context.clone(),
            services: hosts,
        },
    )
    .await;
    assert_eq!(first["reply_keys"], "ready");
    let first_route: CertifiedRoute = serde_json::from_value(first["route"].clone()).unwrap();
    assert_eq!(
        first_route.control.expires_at - first_route.control.issued_at,
        AUTHORITY_LIFETIME
    );
    assert_ne!(first_route.control.owner_key, root.public_key_b64());
    let public = serde_json::to_string(&first_route).unwrap();
    for private in [
        &organ,
        &context,
        &first_member.cell_uid,
        &second_member.cell_uid,
    ] {
        assert!(!public.contains(private));
    }
    copy(&owner, &second, &organ).await;
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_device_state")
            .fetch_one(&second.store.pool)
            .await
            .unwrap(),
        0
    );
    assert!(
        store::records::get_extension(&second.store.pool, &context, PRIVATE_NAMESPACE)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        second.social_refresh_reply_authorizations().await.unwrap(),
        0
    );
    let waiting = command(
        &second,
        Command::ReplyKeyStatus {
            record: context.clone(),
        },
    )
    .await;
    assert_eq!(waiting["reply_keys"], "waiting-for-owner");
    let second_account =
        store::social::device_state(&second.store.pool, &format!("account:{context}"))
            .await
            .unwrap()
            .unwrap();
    copy(&second, &owner, &organ).await;
    assert_eq!(
        owner.social_refresh_reply_authorizations().await.unwrap(),
        1
    );
    copy(&owner, &second, &organ).await;
    assert_eq!(
        second.social_refresh_reply_authorizations().await.unwrap(),
        1
    );
    let ready = command(
        &second,
        Command::ReplyKeyStatus {
            record: context.clone(),
        },
    )
    .await;
    let second_route: CertifiedRoute = serde_json::from_value(ready["route"].clone()).unwrap();
    assert_eq!(
        second_route.control.owner_key,
        first_route.control.owner_key
    );
    assert_ne!(
        second_route.route.identity_key,
        first_route.route.identity_key
    );
    assert_ne!(second_route.route.mailbox, first_route.route.mailbox);
    assert_ne!(second_route.route.pickup_key, first_route.route.pickup_key);
    assert_eq!(
        second_account,
        store::social::device_state(&second.store.pool, &format!("account:{context}"))
            .await
            .unwrap()
            .unwrap()
    );
    copy(&second, &owner, &organ).await;
    let before = store::social::device_state(&owner.store.pool, &format!("authority:{context}"))
        .await
        .unwrap()
        .unwrap();
    owner.social_refresh_reply_authorizations().await.unwrap();
    assert_eq!(
        before,
        store::social::device_state(&owner.store.pool, &format!("authority:{context}"))
            .await
            .unwrap()
            .unwrap()
    );
    let roster = owner
        .publish_roster(&root, vec![first_member])
        .await
        .unwrap();
    owner.social_refresh_reply_authorizations().await.unwrap();
    let renewed = command(
        &owner,
        Command::ReplyKeyStatus {
            record: context.clone(),
        },
    )
    .await;
    assert_eq!(renewed["route"]["control"]["generation"], "2");
    second.adopt_roster(&roster).await.unwrap();
    assert!(second.social_refresh_reply_authorizations().await.is_err());
    let current =
        store::records::get_extension(&owner.store.pool, &context, SESSION_AUTHORITY_NAMESPACE)
            .await
            .unwrap()
            .unwrap();
    assert!(current[format!("authorized_{}", second_member.cell_uid)].is_null());
    let wallet_before_reset =
        store::social::device_state(&owner.store.pool, &format!("authority:{context}"))
            .await
            .unwrap()
            .unwrap();
    let old_storage_key = owner.social_storage_key().await.unwrap();
    command(&owner, Command::ResetPrivateSessions).await;
    assert_eq!(old_storage_key, owner.social_storage_key().await.unwrap());
    assert_eq!(
        wallet_before_reset,
        store::social::device_state(&owner.store.pool, &format!("authority:{context}"))
            .await
            .unwrap()
            .unwrap()
    );
    assert!(
        store::social::device_state(&owner.store.pool, &format!("account:{context}"))
            .await
            .unwrap()
            .is_none()
    );
    owner.social_refresh_reply_authorizations().await.unwrap();
    let recovered = command(&owner, Command::ReplyKeyStatus { record: context }).await;
    assert_eq!(recovered["reply_keys"], "ready");
    assert_eq!(
        recovered["route"]["control"]["owner_key"],
        first["route"]["control"]["owner_key"]
    );
    assert_ne!(
        recovered["route"]["route"]["identity_key"],
        first["route"]["route"]["identity_key"]
    );
    assert_eq!(recovered["route"]["control"]["generation"], "3");
}

#[tokio::test]
async fn foreign_contexts_and_a_wrong_owner_key_cannot_initialize_reply_authority() {
    let engine = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    engine.set_sealing_keyring_path(directory.path().join("sealing.json"));
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    device(&engine, &organ, "owner", 154).await;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [155; 32]);
    engine.publish_root_key(&root).await.unwrap();
    let path = directory.path().join("wrong-root.key");
    std::fs::write(&path, [156; 32]).unwrap();
    engine.set_root_key_path(path);
    let saved = command(
        &engine,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Test".into(),
                ..Default::default()
            },
        },
    )
    .await;
    let hosts = vec![iroh::SecretKey::from_bytes(&[157; 32]).public().to_string()];
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::PrepareReplyKeys {
                        record: saved["record"].as_str().unwrap().into(),
                        services: hosts.clone()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::Social {
                    request: Command::PrepareReplyKeys {
                        record: nucleus::new_uid("r"),
                        services: hosts
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_device_state")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        0
    );
}

#[test]
fn private_local_state_is_authenticated_to_the_exact_device_context() {
    let key = [158; 32];
    let body =
        engine::social::session::seal_local("account:one", &json!({"secret":"private"}), &key)
            .unwrap();
    assert_eq!(
        engine::social::session::open_local::<Value>("account:one", &body, &key).unwrap(),
        json!({"secret":"private"})
    );
    assert!(engine::social::session::open_local::<Value>("account:two", &body, &key).is_err());
    assert!(
        engine::social::session::open_local::<Value>("account:one", &body, &[159; 32]).is_err()
    );
}
