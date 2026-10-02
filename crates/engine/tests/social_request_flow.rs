use engine::{Engine, EngineError, actions::Action, social::Network};
use nucleus::social::{
    Command, PostDraft, PostState, PublicRequest, RequestDecision, ServiceSettings, Snippet,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

#[path = "social_request_flow/late_delivery.rs"]
mod late_delivery;

struct Hosts {
    nodes: BTreeMap<String, Arc<Engine>>,
    offline: Mutex<BTreeSet<String>>,
}

#[async_trait::async_trait]
impl Network for Hosts {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        if self.offline.lock().unwrap().contains(destination) {
            return Err(EngineError::Consequence(
                "Selected mailbox is offline".into(),
            ));
        }
        self.nodes[destination]
            .social_public_request(
                "request-flow",
                destination,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await
    }
}

async fn command(engine: &Engine, request: Command) -> Value {
    Box::pin(engine.act(Action::Social { request }, None))
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn person(secret: u8) -> (Arc<Engine>, tempfile::TempDir) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let dir = tempfile::tempdir().unwrap();
    engine.set_sealing_keyring_path(dir.path().join("sealing.json"));
    let root = dir.path().join("root.key");
    std::fs::write(&root, [secret; 32]).unwrap();
    engine.set_root_key_path(root);
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let signer = engine.operational_key_for(&organ).await.unwrap();
    engine.set_signer(signer.clone()).await.unwrap();
    engine.set_organ_signer(signer).await.unwrap();
    (engine, dir)
}

async fn publication(engine: &Engine) {
    engine.social_reconcile_private_admissions().await.unwrap();
    for _ in 0..4 {
        engine.social_publish_once().await.unwrap();
    }
}

async fn due(engine: &Engine) {
    store::sqlx::query("UPDATE social_private_destination SET next_attempt=0")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE social_pickup_work SET next_attempt=0")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE social_message_work SET next_attempt=0")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE social_publication_job SET next_attempt=0")
        .execute(&engine.store.pool)
        .await
        .unwrap();
}

async fn copy(source: &Engine, target: &Engine, organ: &str) {
    for _ in 0..4 {
        let vector = store::sync_ops::version_vector_for_organ(&target.store.pool, organ)
            .await
            .unwrap();
        let page = source.export_sync_page(organ, &vector, 2000).await.unwrap();
        if page.batch.ops.is_empty() {
            break;
        }
        target.receive_sync_batch(organ, &page.batch).await.unwrap();
    }
}

async fn enrol(source: &Engine, secret: u8) -> (Arc<Engine>, tempfile::TempDir) {
    use engine::{
        pairing::EnrolmentInvite,
        roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
        trust::Signer,
    };
    let organ = store::organs::local(&source.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [secret; 32]);
    let second = Arc::new(Engine::open_memory().await.unwrap());
    let directory = tempfile::tempdir().unwrap();
    second.set_sealing_keyring_path(directory.path().join("sealing.json"));
    let mut members = Vec::new();
    let mut keys = Vec::new();
    for (device, label, node_secret) in [(source, "Owner", 248), (&*second, "Second device", 249)] {
        let cell = store::cells::local(&device.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let key = device.operational_key_for(&organ).await.unwrap();
        members.push(CellEntry {
            cell_uid: cell,
            node_id: iroh::SecretKey::from_bytes(&[node_secret; 32])
                .public()
                .to_string(),
            label: label.into(),
            operational_key: key.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: full_capabilities(),
        });
        keys.push(key);
    }
    let roster = source.publish_roster(&root, members.clone()).await.unwrap();
    second
        .join_organ(
            &EnrolmentInvite {
                node_id: members[0].node_id.clone(),
                organ_uid: organ,
                root_key: root.public_key_b64(),
                token: "social-device-history".into(),
                addrs: Vec::new(),
            },
            &roster,
            keys[1].clone(),
        )
        .await
        .unwrap();
    second.set_signer(keys[1].clone()).await.unwrap();
    (second, directory)
}

async fn exchange(sender: &Engine, receiver: &Engine) {
    due(sender).await;
    sender.social_reconcile_private_admissions().await.unwrap();
    sender.social_prepare_messages_once().await.unwrap();
    publication(sender).await;
    for _ in 0..8 {
        sender.social_send_private_once().await.unwrap();
    }
    for _ in 0..4 {
        due(receiver).await;
        receiver.social_collect_private_once().await.unwrap();
    }
}

async fn message_rule(engine: &Engine) -> (String, String, String, String, i64) {
    use nucleus::karma::{Cadence, Carry, Consequence, Consequences, Gate};
    let message: String = store::sqlx::query_scalar("SELECT uid FROM record WHERE kind='message' AND body='Concurrent independent device receipt'").fetch_one(&engine.store.pool).await.unwrap();
    let event: (String, String, i64) = store::sqlx::query_as(
        "SELECT event,fact,issued_at FROM social_message_event WHERE record_uid=?",
    )
    .bind(&message)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    let counter = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "Message effect counter",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::executor::designate(&engine.store.pool, &counter, Some(&cell))
        .await
        .unwrap();
    let condition = format!("@{message}");
    let now = nucleus::execution::now();
    store::recurrence::create(
        &engine.store.pool,
        store::recurrence::NewRecurrence {
            record_uid: &counter,
            consequences: Consequences::new(vec![Consequence::AddQuantity {
                delta: Some(nucleus::DecimalValue::parse_inferred("1").unwrap()),
            }])
            .unwrap(),
            condition: Some(store::recurrence::RuleCondition {
                source: condition.clone(),
                bindings: store::karma_bindings::resolve(&engine.store.pool, &condition, &[])
                    .await
                    .unwrap(),
                gate: Gate::parse(">0").unwrap(),
                carry: Carry::parse("one").unwrap(),
            }),
            note: None,
            cadence: Cadence::every_days(1),
            anchor_at: now - chrono::TimeDelta::minutes(1),
            request_id: "social-message-effect-replay",
            actor_uid: None,
        },
        now,
    )
    .await
    .unwrap();
    engine.notify_karma_deadline_change();
    (counter, message, event.0, event.1, event.2)
}

async fn replay_message_effect(engine: &Engine, retained: (String, String, String, String, i64)) {
    let (counter, message, event, fact, issued_at) = retained;
    assert_eq!(
        store::records::quantity(&engine.store.pool, &counter)
            .await
            .unwrap()
            .unwrap()
            .to_string(),
        "1"
    );
    store::sqlx::query(
        "INSERT INTO social_message_event(event,record_uid,fact,issued_at) VALUES(?,?,?,?)",
    )
    .bind(&event)
    .bind(message)
    .bind(fact)
    .bind(issued_at)
    .execute(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(
        engine.social_process_message_events_once().await.unwrap(),
        1
    );
    assert_eq!(
        store::records::quantity(&engine.store.pool, &counter)
            .await
            .unwrap()
            .unwrap()
            .to_string(),
        "1"
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM karma_rule_application WHERE event_id=? AND status='applied'"
        )
        .bind(event)
        .fetch_one(&engine.store.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fact WHERE record_uid=?")
            .bind(counter)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        1
    );
}

async fn refuse_delivery(
    alice: &Engine,
    bob: &Engine,
    alice_root: &str,
    bob_root: &str,
    hosts: &Hosts,
) {
    use nucleus::social::requests::{DELIVERY_NAMESPACE, PARTICIPANTS_NAMESPACE};
    let thread: String = store::sqlx::query_scalar(
        "SELECT uid FROM record WHERE replica_root=? AND kind='thread' AND deleted_at IS NULL",
    )
    .bind(bob_root)
    .fetch_one(&bob.store.pool)
    .await
    .unwrap();
    let saved = bob
        .act(
            Action::CreateMessage {
                thread,
                body: "Deliberately refused delivery keeps its local history".into(),
                author: None,
                state: nucleus::MessageState::Finished,
                content: vec![],
                parent: None,
                references: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let message = saved["message"].as_str().unwrap();
    due(bob).await;
    bob.social_prepare_messages_once().await.unwrap();
    for _ in 0..4 {
        bob.social_send_private_once().await.unwrap();
    }
    let original =
        store::records::get_extension(&alice.store.pool, alice_root, PARTICIPANTS_NAMESPACE)
            .await
            .unwrap()
            .unwrap();
    let mut unavailable = original.clone();
    unavailable["state"] = serde_json::json!("closed");
    store::sqlx::query("UPDATE record_extension SET fds=? WHERE record_uid=? AND namespace=?")
        .bind(serde_json::to_string(&unavailable).unwrap())
        .bind(alice_root)
        .bind(PARTICIPANTS_NAMESPACE)
        .execute(&alice.store.pool)
        .await
        .unwrap();
    due(alice).await;
    assert_eq!(alice.social_collect_private_once().await.unwrap(), 0);
    let requests = command(alice, Command::Requests { after: None }).await;
    let failures = requests["receive_failures"].as_array().unwrap();
    assert_eq!(failures.len(), 2);
    for failure in failures {
        command(
            alice,
            Command::DiscardPrivate {
                context: failure["context"].as_str().unwrap().into(),
                service: failure["service"].as_str().unwrap().into(),
                envelope: failure["envelope"].as_str().unwrap().into(),
            },
        )
        .await;
    }
    due(alice).await;
    assert_eq!(alice.social_collect_private_once().await.unwrap(), 0);
    store::sqlx::query("UPDATE record_extension SET fds=? WHERE record_uid=? AND namespace=?")
        .bind(serde_json::to_string(&original).unwrap())
        .bind(alice_root)
        .bind(PARTICIPANTS_NAMESPACE)
        .execute(&alice.store.pool)
        .await
        .unwrap();
    let mut retained = Vec::new();
    for failure in failures {
        let host = &hosts.nodes[failure["service"].as_str().unwrap()];
        let envelope = failure["envelope"].as_str().unwrap();
        let receipt: String =
            store::sqlx::query_scalar("SELECT receipt FROM social_service_completed WHERE id=?")
                .bind(envelope)
                .fetch_one(&host.store.pool)
                .await
                .unwrap();
        let mut forged: Value = serde_json::from_str(&receipt).unwrap();
        forged["content_hash"] = serde_json::json!("0".repeat(64));
        store::sqlx::query("UPDATE social_service_completed SET receipt=? WHERE id=?")
            .bind(serde_json::to_string(&forged).unwrap())
            .bind(envelope)
            .execute(&host.store.pool)
            .await
            .unwrap();
        retained.push((host, envelope, receipt));
    }
    due(bob).await;
    for _ in 0..4 {
        bob.social_send_private_once().await.unwrap();
    }
    let state = store::records::get_extension(&bob.store.pool, message, DELIVERY_NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(state["stage"], "recipient-refused");
    for (host, envelope, receipt) in retained {
        store::sqlx::query("UPDATE social_service_completed SET receipt=? WHERE id=?")
            .bind(receipt)
            .bind(envelope)
            .execute(&host.store.pool)
            .await
            .unwrap();
    }
    due(bob).await;
    for _ in 0..4 {
        bob.social_send_private_once().await.unwrap();
    }
    let state = store::records::get_extension(&bob.store.pool, message, DELIVERY_NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state["stage"], "recipient-refused");
    assert_eq!(state["receipt"]["stage"], "recipient-refused");
    let pending:i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_private_outbox WHERE record_uid=? AND state IN ('pending','stored','held')").bind(message).fetch_one(&bob.store.pool).await.unwrap();
    assert_eq!(pending, 0);
    assert_eq!(
        store::records::get(&bob.store.pool, message)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Deliberately refused delivery keeps its local history"
    );
    assert!(
        bob.social_command(
            Command::ResumePrivate {
                message: message.into()
            },
            None,
            nucleus::execution::now()
        )
        .await
        .is_err()
    );
}

async fn reveal_connect_and_archive(
    alice: &Engine,
    bob: &Engine,
    second: &Engine,
    alice_root: &str,
    bob_root: &str,
    organ: &str,
) {
    for (person, name) in [(alice, "Alice"), (bob, "Bob")] {
        command(
            person,
            Command::SaveProfile {
                fields: nucleus::social::ProfileFields {
                    name: name.into(),
                    ..Default::default()
                },
                parents: vec![],
                destinations: vec![],
            },
        )
        .await;
    }
    command(
        alice,
        Command::RevealProfile {
            conversation: alice_root.into(),
        },
    )
    .await;
    exchange(alice, bob).await;
    command(
        bob,
        Command::RevealProfile {
            conversation: bob_root.into(),
        },
    )
    .await;
    exchange(bob, alice).await;
    let state = command(alice, Command::Requests { after: None }).await;
    assert_eq!(state["requests"][0]["verification"]["can_connect"], true);
    let proof = &state["requests"][0]["reveal"]["local"];
    let participant = &state["requests"][0]["state"];
    let doc: nucleus::social::Profile = serde_json::from_value(proof["profile"].clone()).unwrap();
    let token = participant["token"].as_str().unwrap();
    let author = participant["local_owner"].as_str().unwrap();
    let recipient = participant["peer_owner"].as_str().unwrap();
    let signature = proof["binding_signature"].as_str().unwrap();
    let now = nucleus::execution::now().timestamp();
    engine::social::validate_private_profile_binding(
        token, author, recipient, &doc, signature, now,
    )
    .unwrap();
    assert!(
        engine::social::validate_private_profile_binding(
            token,
            author,
            recipient,
            &doc,
            &doc.signature,
            now
        )
        .is_err()
    );
    assert!(
        engine::social::validate_private_profile_binding(
            &nucleus::new_uid("talk"),
            author,
            recipient,
            &doc,
            signature,
            now
        )
        .is_err()
    );
    assert!(
        engine::social::validate_private_profile_binding(
            token, recipient, author, &doc, signature, now
        )
        .is_err()
    );
    assert!(
        engine::social::validate_private_profile_binding(
            token,
            author,
            recipient,
            &doc,
            signature,
            doc.expires_at + 1
        )
        .is_err()
    );
    assert_eq!(
        state["requests"][0]["reveal"]["peer"]["profile"]["fields"]["name"],
        "Bob"
    );
    assert_eq!(state["requests"][0]["reveal"]["connected"], false);
    command(
        alice,
        Command::ConnectParticipant {
            conversation: alice_root.into(),
        },
    )
    .await;
    exchange(alice, bob).await;
    assert_eq!(
        command(bob, Command::Requests { after: None }).await["requests"][0]["reveal"]["connected"],
        false
    );
    command(
        bob,
        Command::ConnectParticipant {
            conversation: bob_root.into(),
        },
    )
    .await;
    exchange(bob, alice).await;
    for person in [alice, bob] {
        let visible = command(person, Command::Requests { after: None }).await;
        assert_eq!(
            visible["requests"][0]["verification"]["contact_present"],
            true
        );
        let contact = store::organs::contacts(&person.store.pool).await.unwrap();
        assert_eq!(contact.len(), 1);
        assert_eq!(contact[0].trust, "known");
        assert!(!contact[0].sync_out && !contact[0].sync_in);
        assert_eq!(contact[0].scope_fields, Some(vec![]));
        assert!(contact[0].node_id.is_none());
        assert!(
            person
                .export_sync_page(&contact[0].record_uid, &[], 2000)
                .await
                .unwrap()
                .batch
                .ops
                .is_empty()
        );
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM replica_grant")
                .fetch_one(&person.store.pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM social_document WHERE kind='profile' AND id=?"
            )
            .bind(&contact[0].record_uid)
            .fetch_one(&person.store.pool)
            .await
            .unwrap(),
            0
        );
    }
    copy(alice, second, organ).await;
    assert_eq!(
        second.social_reconcile_revealed_contacts().await.unwrap(),
        1
    );
    assert_eq!(
        store::organs::contacts(&second.store.pool)
            .await
            .unwrap()
            .len(),
        1
    );
    let bob_uid = store::organs::local(&bob.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let bob_cell = store::cells::local(&bob.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let batch = engine::sync::OpBatch {
        from_organ: bob_uid.clone(),
        ops: vec![engine::sync::WireOp {
            tbl: "record".into(),
            uid: organ.into(),
            field: "".into(),
            kind: "tombstone".into(),
            value: None,
            hlc: nucleus::execution::now().timestamp_millis(),
            actor_cell: bob_cell,
            organ_uid: bob_uid,
            fact: None,
        }],
    };
    assert!(alice.import_op_batch(&batch).await.is_err());
    assert!(
        store::records::get(&alice.store.pool, organ)
            .await
            .unwrap()
            .is_some()
    );
    command(
        alice,
        Command::DecideRequest {
            conversation: alice_root.into(),
            decision: RequestDecision::Block,
        },
    )
    .await;
    exchange(alice, bob).await;
    due(alice).await;
    for _ in 0..8 {
        alice.social_send_private_once().await.unwrap();
    }
    command(
        alice,
        Command::ArchiveRequest {
            record: alice_root.into(),
        },
    )
    .await;
    let retained = command(alice, Command::Requests { after: None }).await;
    assert!(retained["requests"].as_array().unwrap().is_empty());
    assert_eq!(retained["blocks"].as_array().unwrap().len(), 1);
    let block = &retained["blocks"][0];
    command(
        alice,
        Command::UnblockParticipant {
            context: block["context"].as_str().unwrap().into(),
            peer: block["peer"].as_str().unwrap().into(),
        },
    )
    .await;
    assert!(
        command(alice, Command::Requests { after: None }).await["blocks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        bob.act(
            Action::DeleteConversation {
                conversation: bob_root.into()
            },
            None
        )
        .await
        .is_err()
    );
    assert!(
        bob.act(
            Action::DeleteRecord {
                target: bob_root.into()
            },
            None
        )
        .await
        .is_err()
    );
    let participant = store::records::get_extension(
        &bob.store.pool,
        bob_root,
        nucleus::social::requests::PARTICIPANTS_NAMESPACE,
    )
    .await
    .unwrap()
    .unwrap();
    let context = participant["context"].as_str().unwrap();
    assert!(
        bob.act(
            Action::Social {
                request: Command::ArchiveRequest {
                    record: bob_root.into()
                }
            },
            None
        )
        .await
        .is_err()
    );
    let expired = nucleus::execution::now().timestamp() - 1;
    store::sqlx::query("UPDATE social_private_outbox SET expires_at=? WHERE record_uid IN (SELECT uid FROM record WHERE replica_root=?)").bind(expired).bind(bob_root).execute(&bob.store.pool).await.unwrap();
    store::sqlx::query("UPDATE social_message_work SET expires_at=? WHERE conversation=?")
        .bind(expired)
        .bind(bob_root)
        .execute(&bob.store.pool)
        .await
        .unwrap();
    command(
        bob,
        Command::ArchiveRequest {
            record: bob_root.into(),
        },
    )
    .await;
    assert!(
        store::records::get(&bob.store.pool, bob_root)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::records::get(&bob.store.pool, context)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        store::records::get_extension(
            &bob.store.pool,
            context,
            nucleus::social::requests::REQUEST_DRAFT_NAMESPACE
        )
        .await
        .unwrap()
        .unwrap()["archived"],
        true
    );
    assert!(
        bob.act(
            Action::Social {
                request: Command::ResumeRequest {
                    record: context.into()
                }
            },
            None
        )
        .await
        .is_err()
    );
    let key = bob.social_storage_key().await.unwrap();
    let id = format!("account:{context}");
    let (body, version) = store::social::device_state(&bob.store.pool, &id)
        .await
        .unwrap()
        .unwrap();
    let mut account: engine::social::session::AccountState =
        engine::social::session::open_local(&id, &body, &key).unwrap();
    let previous = account.route.prekey.clone();
    account.expires_at = nucleus::execution::now().timestamp() - 1;
    let sealed = engine::social::session::seal_local(&id, &account, &key).unwrap();
    let mut tx = store::write_tx(&bob.store.pool).await.unwrap();
    store::social::put_device_state_on(
        &mut tx,
        &id,
        "account",
        context,
        &sealed,
        Some(version),
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(bob.social_refresh_reply_authorizations().await.unwrap(), 0);
    assert_eq!(
        command(
            bob,
            Command::ReplyKeyStatus {
                record: context.into()
            }
        )
        .await["reply_keys"],
        "dormant"
    );
    let preserved: engine::social::session::AccountState = engine::social::session::open_local(
        &id,
        &store::social::device_state(&bob.store.pool, &id)
            .await
            .unwrap()
            .unwrap()
            .0,
        &key,
    )
    .unwrap();
    assert_eq!(preserved.route.prekey, previous);
    command(
        bob,
        Command::PrepareReplyKeys {
            record: context.into(),
            services: account.route.services.clone(),
        },
    )
    .await;
    let (body, _) = store::social::device_state(&bob.store.pool, &id)
        .await
        .unwrap()
        .unwrap();
    let renewed: engine::social::session::AccountState =
        engine::social::session::open_local(&id, &body, &key).unwrap();
    assert_ne!(renewed.route.prekey, previous);
    assert!(
        command(bob, Command::Requests { after: None }).await["requests"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn requests_deliver_offline_with_exact_retries_acceptance_and_retained_history_after_ending()
{
    Box::pin(request_scenario()).await;
}

async fn request_scenario() {
    let mut nodes = BTreeMap::new();
    for secret in [241, 242] {
        let host = Arc::new(Engine::open_memory().await.unwrap());
        command(
            &host,
            Command::ConfigureServices {
                settings: ServiceSettings {
                    directory: true,
                    mailbox: true,
                    ..Default::default()
                },
            },
        )
        .await;
        nodes.insert(
            iroh::SecretKey::from_bytes(&[secret; 32])
                .public()
                .to_string(),
            host,
        );
    }
    let services: Vec<String> = nodes.keys().cloned().collect();
    let hosts = Arc::new(Hosts {
        nodes,
        offline: Mutex::new(BTreeSet::new()),
    });
    let network: Arc<dyn Network> = hosts.clone();
    let (alice, _alice_dir) = person(243).await;
    let (bob, _bob_dir) = person(244).await;
    alice.attach_social_network(network.clone());
    bob.attach_social_network(network);
    let saved = command(
        &alice,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Can help with bicycle repairs".into(),
                destinations: services.clone(),
                ..Default::default()
            },
        },
    )
    .await;
    let post_context = saved["record"].as_str().unwrap().to_owned();
    command(
        &alice,
        Command::PrepareReplyKeys {
            record: post_context.clone(),
            services: services.clone(),
        },
    )
    .await;
    let reviewed = command(
        &alice,
        Command::Preview {
            record: post_context.clone(),
            state: PostState::Active,
        },
    )
    .await;
    let post: Snippet = serde_json::from_value(reviewed["document"].clone()).unwrap();
    command(
        &alice,
        Command::Publish {
            record: post_context.clone(),
            preview_hash: reviewed["preview_hash"].as_str().unwrap().into(),
            document: post.clone(),
        },
    )
    .await;
    publication(&alice).await;
    let draft = command(
        &bob,
        Command::OpenRequest {
            post: Box::new(post),
            text: "My bicycle needs help".into(),
            alias: "Neighbor".into(),
            services: services.clone(),
        },
    )
    .await;
    let context = draft["record"].as_str().unwrap().to_owned();
    assert_eq!(bob.social_prepare_messages_once().await.unwrap(), 1);
    bob.social_reconcile_private_admissions().await.unwrap();
    publication(&bob).await;
    assert_eq!(bob.social_prepare_messages_once().await.unwrap(), 1);
    let cipher: Vec<(String, String)> =
        store::sqlx::query_as("SELECT id,body FROM social_private_outbox ORDER BY id")
            .fetch_all(&bob.store.pool)
            .await
            .unwrap();
    due(&bob).await;
    bob.social_prepare_messages_once().await.unwrap();
    let retried: Vec<(String, String)> =
        store::sqlx::query_as("SELECT id,body FROM social_private_outbox ORDER BY id")
            .fetch_all(&bob.store.pool)
            .await
            .unwrap();
    assert_eq!(cipher, retried);
    hosts.offline.lock().unwrap().insert(services[1].clone());
    assert_eq!(bob.social_send_private_once().await.unwrap(), 1);
    let requests = command(&bob, Command::Requests { after: None }).await;
    assert_eq!(
        requests["requests"][0]["messages"][0]["delivery"]["stage"],
        "mailbox-stored"
    );
    due(&bob).await;
    bob.social_send_private_once().await.unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_service_envelope")
            .fetch_one(&hosts.nodes[&services[0]].store.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(alice.social_collect_private_once().await.unwrap(), 1);
    let request = command(&alice, Command::Requests { after: None }).await["requests"][0].clone();
    let alice_root = request["record"].as_str().unwrap().to_owned();
    let bob_root = requests["requests"][0]["record"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(request["state"]["alias"], "Neighbor");
    assert_eq!(request["messages"][0]["body"], "My bicycle needs help");
    assert!(
        alice
            .act(
                Action::EditRecordText {
                    target: request["messages"][0]["uid"].as_str().unwrap().into(),
                    head: None,
                    body: Some("Changed".into())
                },
                None
            )
            .await
            .is_err()
    );
    alice.social_reconcile_private_admissions().await.unwrap();
    publication(&alice).await;
    for number in 0..3 {
        command(
            &bob,
            Command::SendPrivate {
                conversation: bob_root.clone(),
                text: format!("Provisional reply {number}"),
            },
        )
        .await;
    }
    assert!(
        bob.act(
            Action::Social {
                request: Command::SendPrivate {
                    conversation: bob_root.clone(),
                    text: "Too many provisional texts".into()
                }
            },
            None
        )
        .await
        .is_err()
    );
    due(&bob).await;
    bob.social_prepare_messages_once().await.unwrap();
    bob.social_send_private_once().await.unwrap();
    command(
        &alice,
        Command::DecideRequest {
            conversation: alice_root.clone(),
            decision: RequestDecision::Accept,
        },
    )
    .await;
    publication(&alice).await;
    assert_eq!(
        alice.social_prepare_messages_once().await.unwrap(),
        1,
        "{:?}",
        command(&alice, Command::Requests { after: None }).await
    );
    assert_eq!(
        alice.social_send_private_once().await.unwrap(),
        1,
        "{:?}",
        store::sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT state,error FROM social_private_outbox"
        )
        .fetch_all(&alice.store.pool)
        .await
        .unwrap()
    );
    due(&bob).await;
    assert_eq!(bob.social_collect_private_once().await.unwrap(), 1);
    assert_eq!(
        command(&bob, Command::Requests { after: None }).await["requests"][0]["state"]["state"],
        "accepted"
    );
    hosts.offline.lock().unwrap().clear();
    due(&alice).await;
    due(&bob).await;
    publication(&alice).await;
    publication(&bob).await;
    bob.social_send_private_once().await.unwrap();
    alice.social_collect_private_once().await.unwrap();
    due(&bob).await;
    bob.social_send_private_once().await.unwrap();
    let deliveries = command(&bob, Command::Requests { after: None }).await;
    assert!(
        deliveries["requests"][0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["delivery"]["stage"] == "recipient-durable")
    );
    let ending = command(
        &alice,
        Command::Preview {
            record: post_context.clone(),
            state: PostState::Withdrawn,
        },
    )
    .await;
    command(
        &alice,
        Command::Publish {
            record: post_context,
            preview_hash: ending["preview_hash"].as_str().unwrap().into(),
            document: serde_json::from_value(ending["document"].clone()).unwrap(),
        },
    )
    .await;
    publication(&alice).await;
    command(
        &bob,
        Command::SendPrivate {
            conversation: bob_root,
            text: "Our accepted conversation continues".into(),
        },
    )
    .await;
    bob.social_prepare_messages_once().await.unwrap();
    bob.social_send_private_once().await.unwrap();
    due(&alice).await;
    assert_eq!(alice.social_collect_private_once().await.unwrap(), 1);
    let final_request =
        command(&alice, Command::Requests { after: None }).await["requests"][0].clone();
    assert_eq!(final_request["record"], alice_root);
    assert_eq!(final_request["messages"].as_array().unwrap().len(), 6);
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM replica_grant")
            .fetch_one(&alice.store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM record WHERE kind='thread' AND quantity_mantissa='1'"
        )
        .fetch_one(&alice.store.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_private_seen")
            .fetch_one(&alice.store.pool)
            .await
            .unwrap(),
        5
    );
    assert!(!serde_json::to_string(&cipher).unwrap().contains(&context));
    let (second, _second_dir) = enrol(&alice, 243).await;
    let organ = store::organs::local(&alice.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    copy(&alice, &second, &organ).await;
    assert_eq!(
        command(&second, Command::Requests { after: None }).await["requests"][0]["record"],
        alice_root
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_device_state WHERE kind IN ('account','session')"
        )
        .fetch_one(&second.store.pool)
        .await
        .unwrap(),
        0
    );
    let preparing = command(
        &second,
        Command::PrepareReplyKeys {
            record: final_request["state"]["context"].as_str().unwrap().into(),
            services: services.clone(),
        },
    )
    .await;
    assert_eq!(preparing["reply_keys"], "waiting-for-owner");
    copy(&second, &alice, &organ).await;
    assert_eq!(
        alice.social_refresh_reply_authorizations().await.unwrap(),
        1
    );
    copy(&alice, &second, &organ).await;
    let network: Arc<dyn Network> = hosts.clone();
    second.attach_social_network(network);
    second.social_refresh_reply_authorizations().await.unwrap();
    alice.social_reconcile_private_admissions().await.unwrap();
    second.social_reconcile_private_admissions().await.unwrap();
    publication(&alice).await;
    publication(&second).await;
    let second_authority = command(
        &second,
        Command::ReplyKeyStatus {
            record: final_request["state"]["context"].as_str().unwrap().into(),
        },
    )
    .await;
    assert_eq!(second_authority["reply_keys"], "ready");
    assert_eq!(bob.social_refresh_private_routes_once().await.unwrap(), 2);
    let next = command(
        &bob,
        Command::SendPrivate {
            conversation: deliveries["requests"][0]["record"].as_str().unwrap().into(),
            text: "History before fresh-device reception".into(),
        },
    )
    .await;
    due(&bob).await;
    bob.social_prepare_messages_once().await.unwrap();
    for _ in 0..4 {
        bob.social_send_private_once().await.unwrap();
    }
    due(&alice).await;
    assert_eq!(alice.social_collect_private_once().await.unwrap(), 1);
    copy(&alice, &second, &organ).await;
    due(&second).await;
    assert_eq!(second.social_collect_private_once().await.unwrap(), 0);
    let second_history = command(&second, Command::Requests { after: None }).await;
    assert!(
        second_history["requests"][0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["body"] == "History before fresh-device reception")
    );
    assert!(next["message"].is_string());
    command(
        &bob,
        Command::SendPrivate {
            conversation: deliveries["requests"][0]["record"].as_str().unwrap().into(),
            text: "Concurrent independent device receipt".into(),
        },
    )
    .await;
    due(&bob).await;
    bob.social_prepare_messages_once().await.unwrap();
    for _ in 0..4 {
        bob.social_send_private_once().await.unwrap();
    }
    due(&alice).await;
    due(&second).await;
    let (one, two) = tokio::join!(
        Box::pin(alice.social_collect_private_once()),
        Box::pin(second.social_collect_private_once())
    );
    let pickup_errors: Vec<Option<String>> =
        store::sqlx::query_scalar("SELECT error FROM social_pickup_work")
            .fetch_all(&second.store.pool)
            .await
            .unwrap();
    let send_errors: Vec<(String, String, Option<String>)> =
        store::sqlx::query_as("SELECT recipient_owner,state,error FROM social_private_outbox")
            .fetch_all(&bob.store.pool)
            .await
            .unwrap();
    let first_errors: Vec<Option<String>> =
        store::sqlx::query_scalar("SELECT error FROM social_pickup_work")
            .fetch_all(&alice.store.pool)
            .await
            .unwrap();
    let destination_errors: Vec<(String, String, Option<String>)> =
        store::sqlx::query_as("SELECT service,state,error FROM social_private_destination")
            .fetch_all(&bob.store.pool)
            .await
            .unwrap();
    assert_eq!(
        one.unwrap(),
        1,
        "pickup={first_errors:?}, send={send_errors:?}, destinations={destination_errors:?}"
    );
    assert_eq!(
        two.unwrap(),
        1,
        "pickup={pickup_errors:?}, send={send_errors:?}"
    );
    copy(&alice, &second, &organ).await;
    copy(&second, &alice, &organ).await;
    let retained_event = message_rule(&alice).await;
    copy(&alice, &second, &organ).await;
    for device in [&*alice, &*second] {
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM record WHERE kind='message' AND body='Concurrent independent device receipt' AND quantity_mantissa='1'").fetch_one(&device.store.pool).await.unwrap(),1);
        assert!(device.social_process_message_events_once().await.unwrap() > 0);
        while device.social_process_message_events_once().await.unwrap() > 0 {}
        assert_eq!(
            device.social_process_message_events_once().await.unwrap(),
            0
        );
    }
    Box::pin(replay_message_effect(&alice, retained_event)).await;
    Box::pin(refuse_delivery(
        &alice,
        &bob,
        &alice_root,
        deliveries["requests"][0]["record"].as_str().unwrap(),
        &hosts,
    ))
    .await;
    Box::pin(reveal_connect_and_archive(
        &alice,
        &bob,
        &second,
        &alice_root,
        deliveries["requests"][0]["record"].as_str().unwrap(),
        &organ,
    ))
    .await;
}
