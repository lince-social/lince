use std::sync::Arc;

use base64::Engine as _;
use ed25519_dalek::{Signer as _, SigningKey};
use engine::{Engine, roster::CellEntry, seal, trust::Signer};

fn sealed(organ: &str, keys: &[seal::SealingKey]) -> seal::SealedBundle {
    seal::seal(
        &seal::MailedBatch {
            root: None,
            batch: engine::sync::OpBatch {
                from_organ: "sender".into(),
                ops: vec![],
            },
        },
        "sender-cell",
        organ,
        keys,
        &SigningKey::from_bytes(&[17; 32]),
    )
    .unwrap()
}

fn entry(name: &str) -> CellEntry {
    let (_, key) = seal::generate(name, 1, "2099-01-01T00:00:00Z");
    CellEntry {
        cell_uid: name.into(),
        node_id: format!("node-{name}"),
        label: name.into(),
        operational_key: "unused".into(),
        sealing_key: Some(key),
        front_door: false,
        capabilities: engine::roster::full_capabilities(),
    }
}

async fn recipient() -> (Engine, String, Signer) {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    engine.publish_root_key(&root).await.unwrap();
    (engine, organ, root)
}

#[test]
fn envelope_identity_expiry_and_sender_are_authenticated_and_retries_are_stable() {
    let (_, key) = seal::generate("phone", 1, "2099-01-01T00:00:00Z");
    let original = sealed("recipient", &[key]);
    assert!(seal::validate_envelope(&original, nucleus::execution::now().timestamp()).is_ok());
    assert_eq!(
        seal::delivery_id(&original),
        seal::delivery_id(&original.clone())
    );
    for mutation in 0..5 {
        let mut changed = original.clone();
        match mutation {
            0 => changed.uid = nucleus::new_uid("r"),
            1 => changed.expires_at -= 1,
            2 => changed.created_at -= 1,
            3 => {
                changed.sender_key = base64::engine::general_purpose::STANDARD
                    .encode(SigningKey::from_bytes(&[18; 32]).verifying_key().as_bytes())
            }
            _ => changed.to_organ = "another-recipient".into(),
        }
        assert!(seal::validate_envelope(&changed, nucleus::execution::now().timestamp()).is_err());
    }
}

#[test]
fn signed_malformed_encryption_fields_and_unsafe_recipient_keys_are_rejected() {
    let (_, key) = seal::generate("phone", 1, "2099-01-01T00:00:00Z");
    let original = sealed("recipient", &[key.clone()]);
    for mutation in 0..5 {
        let mut changed = original.clone();
        match mutation {
            0 => changed.recipients.push(changed.recipients[0].clone()),
            1 => changed.recipients[0].nonce = "invalid".into(),
            2 => changed.ephemeral = "invalid".into(),
            3 => changed.to_organ = "x".repeat(129),
            _ => changed.ciphertext = "invalid".into(),
        }
        changed.signature = base64::engine::general_purpose::STANDARD.encode(
            SigningKey::from_bytes(&[17; 32])
                .sign(&seal::transcript(&changed))
                .to_bytes(),
        );
        assert!(seal::validate_envelope(&changed, nucleus::execution::now().timestamp()).is_err());
    }
    let mut unsafe_key = key;
    unsafe_key.public = base64::engine::general_purpose::STANDARD.encode([0; 32]);
    assert!(
        seal::seal(
            &seal::MailedBatch {
                root: None,
                batch: engine::sync::OpBatch {
                    from_organ: "sender".into(),
                    ops: vec![]
                }
            },
            "sender-cell",
            "recipient",
            &[unsafe_key],
            &SigningKey::from_bytes(&[17; 32]),
        )
        .is_err()
    );
}

#[test]
fn locally_saved_mail_has_a_bounded_recovery_window_after_host_expiry() {
    let (_, key) = seal::generate("phone", 1, "2099-01-01T00:00:00Z");
    let mail = sealed("recipient", &[key]);
    assert!(seal::validate_envelope(&mail, mail.expires_at).is_err());
    assert!(seal::validate_recoverable(&mail, mail.expires_at + 1).is_ok());
    assert!(
        seal::validate_recoverable(&mail, mail.expires_at + seal::GRACE_DAYS * 86_400 + 1).is_err()
    );
}

#[tokio::test]
async fn mail_addressed_to_another_organ_is_never_imported() {
    let (engine, _organ, _root) = recipient().await;
    let (_, key) = seal::generate("phone", 1, "2099-01-01T00:00:00Z");
    let mail = sealed("another-organ", &[key]);
    assert!(matches!(
        engine.open_mailed(&mail).await,
        Err(engine::EngineError::Forbidden(_))
    ));
}

async fn sender_for(
    owner: &Engine,
    organ: &str,
    root: &Signer,
) -> (tempfile::TempDir, Engine, String) {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("sender.db").display());
    let sender = Engine::open(&url).await.unwrap();
    let local = store::organs::local(&sender.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    sender
        .set_organ_signer(Signer::generate(&local, "organ"))
        .await
        .unwrap();
    store::organs::add_contact(&sender.store.pool, organ, None, "Recipient", "", 1)
        .await
        .unwrap();
    store::organs::set_trust(&sender.store.pool, organ, "known")
        .await
        .unwrap();
    engine::trust::adopt_key(
        &sender.store,
        organ,
        engine::roster::ROOT_KEY_ID,
        &root.public_key_b64(),
    )
    .await
    .unwrap();
    sender
        .adopt_roster(&owner.roster_of(organ).await.unwrap().unwrap())
        .await
        .unwrap();
    (directory, sender, url)
}

#[tokio::test]
async fn outgoing_retries_reuse_saved_ciphertext_after_restart_without_resealing() {
    let (owner, organ, root) = recipient().await;
    owner
        .publish_roster(&root, vec![entry("phone")])
        .await
        .unwrap();
    let (_directory, sender, url) = sender_for(&owner, &organ, &root).await;
    let batch = engine::sync::OpBatch {
        from_organ: store::organs::local(&sender.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid,
        ops: vec![],
    };
    let first = sender
        .prepare_outgoing_mail(&organ, None, &batch)
        .await
        .unwrap();
    let second = sender
        .prepare_outgoing_mail(&organ, None, &batch)
        .await
        .unwrap();
    assert_eq!(first, second);
    sender.store.pool.close().await;
    let reopened = Engine::open(&url).await.unwrap();
    assert_eq!(
        first,
        reopened
            .prepare_outgoing_mail(&organ, None, &batch)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn restricted_carriers_receive_no_wrapped_private_mail_key() {
    let (owner, organ, root) = recipient().await;
    let mut carrier = entry("carrier");
    carrier.capabilities = engine::roster::relay_capabilities();
    owner
        .publish_roster(&root, vec![entry("phone"), carrier])
        .await
        .unwrap();
    let (_directory, sender, _url) = sender_for(&owner, &organ, &root).await;
    let batch = engine::sync::OpBatch {
        from_organ: store::organs::local(&sender.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid,
        ops: vec![],
    };
    let mail = sender.seal_batch_for(&organ, None, &batch).await.unwrap();
    assert_eq!(mail.recipients.len(), 1);
    assert!(mail.recipients[0].key_id.starts_with("x25519:cell:phone:"));
}

#[tokio::test]
async fn mailbox_copy_policy_rejects_impossible_counts_and_preserves_other_limits() {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::cells::ensure_local(&engine.store.pool, &organ, "Device")
        .await
        .unwrap();
    store::cells::set_config(
        &engine.store.pool,
        "lince.social",
        &serde_json::json!({"max_storage_bytes":1000000}),
    )
    .await
    .unwrap();
    for copies in [0, 3, 255] {
        assert!(
            engine
                .act(engine::actions::Action::MailboxSetCopies { copies }, None)
                .await
                .is_err()
        );
    }
    engine
        .act(
            engine::actions::Action::MailboxSetCopies { copies: 1 },
            None,
        )
        .await
        .unwrap();
    let config = store::cells::config(&engine.store.pool, "lince.social")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(config["mailbox_copies"], 1);
    assert_eq!(config["max_storage_bytes"], 1000000);
}

#[tokio::test]
async fn recovery_status_does_not_expose_private_parser_input() {
    let engine = Engine::open_memory().await.unwrap();
    let body = serde_json::json!({"v":"private recovery detail"}).to_string();
    store::mailbox::delivery::receive(
        &engine.store.pool,
        "bad",
        "carrier",
        &body,
        &seal::envelope_hash(&body),
        "2099-01-01T00:00:00Z",
    )
    .await
    .unwrap();
    assert_eq!(engine.process_recovered_mail().await.unwrap(), 0);
    let status = engine
        .act(engine::actions::Action::MailboxSavedStatus, None)
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(status["saved_mail"][0]["state"], "pending");
    assert!(!status.to_string().contains("private recovery detail"));
    assert!(status["saved_mail"][0].get("body").is_none());
}

#[tokio::test]
async fn two_servers_receive_the_same_envelope_and_partial_delivery_can_resume() {
    let (owner, organ, root) = recipient().await;
    owner
        .publish_roster(&root, vec![entry("phone")])
        .await
        .unwrap();
    let mut hosts = Vec::new();
    let mut points = Vec::new();
    let mut addresses = Vec::new();
    let mut serving = Vec::new();
    for index in 0..2 {
        let host = Arc::new(Engine::open_memory().await.unwrap());
        store::mailbox::register(
            &host.store.pool,
            &organ,
            &root.public_key_b64(),
            "Host",
            if index == 0 { 10000 } else { 0 },
        )
        .await
        .unwrap();
        let wire = engine::wire::Wire::bind_with_discovery(
            host.clone(),
            iroh::SecretKey::from_bytes(&[40 + index; 32]),
            engine::wire::Reach::Local,
            None,
            false,
        )
        .await
        .unwrap();
        let port = wire
            .endpoint()
            .bound_sockets()
            .into_iter()
            .next()
            .unwrap()
            .port();
        addresses.push(
            iroh::EndpointAddr::new(wire.node_id()).with_ip_addr(([127, 0, 0, 1], port).into()),
        );
        points.push(engine::roster::PickupPoint {
            organ_uid: format!("host-{index}"),
            node_id: wire.node_id().to_string(),
            label: "Host".into(),
        });
        hosts.push(host);
        serving.push(tokio::spawn(async move { wire.serve().await }));
    }
    owner.set_pickup_points(&root, points).await.unwrap();
    let (_directory, sender, _url) = sender_for(&owner, &organ, &root).await;
    let sender = Arc::new(sender);
    let wire = engine::wire::Wire::bind_with_discovery(
        sender.clone(),
        iroh::SecretKey::from_bytes(&[42; 32]),
        engine::wire::Reach::Local,
        None,
        false,
    )
    .await
    .unwrap();
    for address in addresses {
        wire.remember_addr(address);
    }
    let batch = engine::sync::OpBatch {
        from_organ: store::organs::local(&sender.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid,
        ops: vec![],
    };
    let first = wire.leave_mail(&organ, None, &batch).await.unwrap();
    let uid = match first {
        engine::wire::MailLeft::Left {
            uid,
            copies: 1,
            requested_copies: 2,
            ..
        } => uid,
        other => panic!("Expected a single accepted copy: {other:?}"),
    };
    assert_eq!(
        store::mailbox::waiting(&hosts[0].store.pool, &organ)
            .await
            .unwrap()
            .bundles,
        1
    );
    assert_eq!(
        store::mailbox::waiting(&hosts[1].store.pool, &organ)
            .await
            .unwrap()
            .bundles,
        0
    );
    store::mailbox::register(
        &hosts[1].store.pool,
        &organ,
        &root.public_key_b64(),
        "Host",
        10000,
    )
    .await
    .unwrap();
    store::mailbox::outbox::retry_recipient(&sender.store.pool, &organ)
        .await
        .unwrap();
    assert!(
        matches!(wire.leave_mail(&organ, None, &batch).await.unwrap(),
        engine::wire::MailLeft::Left {uid:ref completed,copies:2,requested_copies:2,..} if completed==&uid)
    );
    let first = store::mailbox::for_recipient(&hosts[0].store.pool, &organ, 10)
        .await
        .unwrap();
    let second = store::mailbox::for_recipient(&hosts[1].store.pool, &organ, 10)
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(first[0].uid, second[0].uid);
    assert_eq!(first[0].body, second[0].body);
    assert_eq!(
        store::mailbox::outbox::receipts(&sender.store.pool, &uid)
            .await
            .unwrap()
            .len(),
        2
    );
    for task in serving {
        task.abort();
    }
}

#[tokio::test]
async fn a_retry_with_different_json_formatting_does_not_consume_another_quota_slot() {
    let (engine, organ, _root) = recipient().await;
    let (_, key) = seal::generate("phone", 1, "2099-01-01T00:00:00Z");
    let mail = sealed(&organ, &[key]);
    let body = serde_json::to_string(&mail).unwrap();
    store::mailbox::register(
        &engine.store.pool,
        &organ,
        "root",
        "Recipient",
        body.len() as i64,
    )
    .await
    .unwrap();
    let first = engine.accept_bundle(&body, "sender-node").await.unwrap();
    let duplicate = engine
        .accept_bundle(&serde_json::to_string_pretty(&mail).unwrap(), "sender-node")
        .await
        .unwrap();
    assert_eq!(first, duplicate);
    assert_eq!(
        store::mailbox::waiting(&engine.store.pool, &organ)
            .await
            .unwrap()
            .bundles,
        1
    );
    let mut conflicting = mail.clone();
    conflicting.from_cell = "different-cell".into();
    conflicting.signature = base64::engine::general_purpose::STANDARD.encode(
        SigningKey::from_bytes(&[17; 32])
            .sign(&seal::transcript(&conflicting))
            .to_bytes(),
    );
    assert!(
        engine
            .accept_bundle(&serde_json::to_string(&conflicting).unwrap(), "sender-node")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn revoked_or_restricted_devices_cannot_collect_using_a_signed_old_roster() {
    let (owner, organ, root) = recipient().await;
    let carrier = Engine::open_memory().await.unwrap();
    store::mailbox::register(
        &carrier.store.pool,
        &organ,
        &root.public_key_b64(),
        "Recipient",
        10000,
    )
    .await
    .unwrap();
    let old = owner
        .publish_roster(&root, vec![entry("phone"), entry("laptop")])
        .await
        .unwrap();
    assert!(
        carrier
            .may_collect(&organ, "node-phone", &old)
            .await
            .unwrap()
    );
    let mut restricted = entry("carrier");
    restricted.capabilities = engine::roster::relay_capabilities();
    let current = owner
        .publish_roster(&root, vec![entry("laptop"), restricted])
        .await
        .unwrap();
    assert!(
        carrier
            .may_collect(&organ, "node-laptop", &current)
            .await
            .unwrap()
    );
    assert!(
        !carrier
            .may_collect(&organ, "node-phone", &old)
            .await
            .unwrap()
    );
    assert!(
        !carrier
            .may_collect(&organ, "node-carrier", &current)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn the_first_device_acknowledgement_does_not_delete_another_devices_mail() {
    let (owner, organ, root) = recipient().await;
    let carrier = Engine::open_memory().await.unwrap();
    let phone = entry("phone");
    let laptop = entry("laptop");
    let keys = vec![
        phone.sealing_key.clone().unwrap(),
        laptop.sealing_key.clone().unwrap(),
    ];
    let roster = owner
        .publish_roster(&root, vec![phone, laptop])
        .await
        .unwrap();
    store::mailbox::register(
        &carrier.store.pool,
        &organ,
        &root.public_key_b64(),
        "Recipient",
        10000,
    )
    .await
    .unwrap();
    let uid = carrier
        .accept_bundle(
            &serde_json::to_string(&sealed(&organ, &keys)).unwrap(),
            "sender-node",
        )
        .await
        .unwrap();
    assert!(
        carrier
            .may_collect(&organ, "node-phone", &roster)
            .await
            .unwrap()
    );
    assert_eq!(
        carrier
            .confirm_collected(&organ, "node-phone", &[uid.clone()])
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        carrier
            .bundles_for_device(&organ, "node-laptop", 50)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        carrier
            .confirm_collected(&organ, "node-laptop", &[uid])
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn a_known_revocation_releases_mail_already_saved_by_the_remaining_device() {
    let (owner, organ, root) = recipient().await;
    let carrier = Engine::open_memory().await.unwrap();
    let phone = entry("phone");
    let laptop = entry("laptop");
    let keys = vec![
        phone.sealing_key.clone().unwrap(),
        laptop.sealing_key.clone().unwrap(),
    ];
    let roster = owner
        .publish_roster(&root, vec![phone.clone(), laptop])
        .await
        .unwrap();
    store::mailbox::register(
        &carrier.store.pool,
        &organ,
        &root.public_key_b64(),
        "Recipient",
        10000,
    )
    .await
    .unwrap();
    let uid = carrier
        .accept_bundle(
            &serde_json::to_string(&sealed(&organ, &keys)).unwrap(),
            "sender-node",
        )
        .await
        .unwrap();
    assert!(
        carrier
            .may_collect(&organ, "node-phone", &roster)
            .await
            .unwrap()
    );
    assert_eq!(
        carrier
            .confirm_collected(&organ, "node-phone", &[uid])
            .await
            .unwrap(),
        0
    );
    let current = owner.publish_roster(&root, vec![phone]).await.unwrap();
    assert!(
        carrier
            .may_collect(&organ, "node-phone", &current)
            .await
            .unwrap()
    );
    assert_eq!(
        store::mailbox::waiting(&carrier.store.pool, &organ)
            .await
            .unwrap()
            .bundles,
        0
    );
    assert!(
        !carrier
            .may_collect(&organ, "node-laptop", &roster)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn wire_collection_commits_recoverable_ciphertext_before_server_acknowledgement() {
    let (owner, organ, root) = recipient().await;
    let carrier = Arc::new(Engine::open_memory().await.unwrap());
    let carrier_wire = engine::wire::Wire::bind(
        carrier.clone(),
        iroh::SecretKey::from_bytes(&[31; 32]),
        engine::wire::Reach::Local,
    )
    .await
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("recipient.db").display()
    );
    let client = Arc::new(
        Engine::new(store::Store::open(&url).await.unwrap())
            .await
            .unwrap(),
    );
    let wire = engine::wire::Wire::bind(
        client.clone(),
        iroh::SecretKey::from_bytes(&[32; 32]),
        engine::wire::Reach::Local,
    )
    .await
    .unwrap();
    let mut phone = entry("phone");
    phone.node_id = wire.node_id().to_string();
    let keys = vec![phone.sealing_key.clone().unwrap()];
    let roster = owner.publish_roster(&root, vec![phone]).await.unwrap();
    store::mailbox::register(
        &carrier.store.pool,
        &organ,
        &root.public_key_b64(),
        "Recipient",
        10000,
    )
    .await
    .unwrap();
    let body = serde_json::to_string(&sealed(&organ, &keys)).unwrap();
    let uid = carrier.accept_bundle(&body, "sender-node").await.unwrap();
    let port = carrier_wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .next()
        .unwrap()
        .port();
    let addr =
        iroh::EndpointAddr::new(carrier_wire.node_id()).with_ip_addr(([127, 0, 0, 1], port).into());
    let serving = tokio::spawn(async move { carrier_wire.serve().await });
    let received = wire.collect_mail(addr, &organ, &roster, 50).await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(
        store::mailbox::waiting(&carrier.store.pool, &organ)
            .await
            .unwrap()
            .bundles,
        0
    );
    client.store.pool.close().await;
    let reopened = store::Store::open_durable(&url).await.unwrap();
    assert_eq!(
        store::mailbox::delivery::pending(&reopened.pool)
            .await
            .unwrap(),
        vec![(uid, body)]
    );
    serving.abort();
}
