use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

tokio::task_local! {
    static PAUSE: Arc<Pause>;
}

#[derive(Default)]
struct Pause {
    entered: Notify,
    release: Notify,
    after: usize,
    calls: AtomicUsize,
}

pub(super) async fn pause() {
    if let Ok(signals) = PAUSE.try_with(Arc::clone) {
        if signals.calls.fetch_add(1, Ordering::SeqCst) == signals.after {
            signals.entered.notify_one();
            signals.release.notified().await;
        }
    }
}

struct Answer(Snippet);

#[tokio::test]
async fn contact_reconstruction_rechecks_conversation_consent_and_device_authority() {
    use nucleus::social::requests::{
        ConversationParticipant, ConversationState, PARTICIPANTS_NAMESPACE, REVEAL_NAMESPACE,
    };

    for change in ["unchanged", "closed", "blocked", "deleted", "read-only"] {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let peer = Engine::open_memory().await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        for (name, node, secret) in [("local", engine.as_ref(), 208), ("peer", &peer, 209)] {
            let path = directory.path().join(format!("{name}.key"));
            std::fs::write(&path, [secret; 32]).unwrap();
            node.set_root_key_path(path);
            node.social_checked_root_signer().await.unwrap().unwrap();
        }
        let organ = store::organs::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let key = engine.operational_key_for(&organ).await.unwrap();
        engine.set_organ_signer(key.clone()).await.unwrap();
        let root = Signer::from_bytes(&organ, crate::roster::ROOT_KEY_ID, [208; 32]);
        engine
            .publish_roster(
                &root,
                vec![crate::roster::CellEntry {
                    cell_uid: cell,
                    node_id: iroh::SecretKey::from_bytes(&[212; 32]).public().to_string(),
                    label: "Owner".into(),
                    operational_key: key.public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: crate::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        let saved = peer
            .social_command(
                Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Reviewed friend".into(),
                        ..Default::default()
                    },
                    parents: vec![],
                    destinations: vec![],
                },
                None,
                nucleus::execution::now(),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let profile: Profile = serde_json::from_value(saved["profile"].clone()).unwrap();
        let mut records = Vec::new();
        for kind in [
            nucleus::RecordKind::Plain,
            nucleus::RecordKind::Conversation,
        ] {
            records.push(
                store::records::create(
                    &engine.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind,
                        head: "Private social context",
                        body: "",
                        quantity: store::exact::zero(),
                    },
                )
                .await
                .unwrap()
                .uid,
            );
        }
        let participant = ConversationParticipant {
            token: nucleus::new_uid("talk"),
            context: records[0].clone(),
            local_owner: Signer::from_bytes("", "social", [210; 32]).public_key_b64(),
            peer_owner: Signer::from_bytes("", "social", [211; 32]).public_key_b64(),
            alias: String::new(),
            routes: vec![],
            state: ConversationState::Accepted,
            incoming: true,
            started_at: profile.issued_at,
            local_accepted: true,
            peer_accepted: true,
            provisional_sent: 0,
        };
        let (signer, _) = peer.social_profile_signer().await.unwrap();
        let signature = signer.sign_bytes(
            &reveal::binding_bytes(
                &participant.token,
                &participant.peer_owner,
                &participant.local_owner,
                &profile,
            )
            .unwrap(),
        );
        let state = json!({"peer":{"profile":profile,"binding_signature":signature},"local_connect":true,"peer_connect":true,"connected":true});
        store::records::set_extension(
            &engine.store.pool,
            &records[1],
            PARTICIPANTS_NAMESPACE,
            &serde_json::to_value(&participant).unwrap(),
        )
        .await
        .unwrap();
        store::records::set_extension(&engine.store.pool, &records[1], REVEAL_NAMESPACE, &state)
            .await
            .unwrap();
        engine.social_require_local_write().await.unwrap();
        let signals = Arc::new(Pause::default());
        let worker = {
            let engine = engine.clone();
            let signals = signals.clone();
            tokio::spawn(async move {
                PAUSE
                    .scope(signals, engine.social_reconcile_revealed_contacts())
                    .await
            })
        };
        tokio::time::timeout(Duration::from_secs(10), signals.entered.notified())
            .await
            .unwrap();
        match change {
            "closed" => {
                let mut closed = participant.clone();
                closed.state = ConversationState::Closed;
                store::records::set_extension(
                    &engine.store.pool,
                    &records[1],
                    PARTICIPANTS_NAMESPACE,
                    &serde_json::to_value(closed).unwrap(),
                )
                .await
                .unwrap();
            }
            "blocked" => {
                let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
                admission::block_on(
                    &mut tx,
                    &participant.context,
                    &participant.peer_owner,
                    true,
                    nucleus::execution::now().timestamp(),
                )
                .await
                .unwrap();
                tx.commit().await.unwrap();
            }
            "deleted" => {
                store::sqlx::query("UPDATE record SET deleted_at=? WHERE uid=?")
                    .bind(nucleus::execution::now().to_rfc3339())
                    .bind(&records[1])
                    .execute(&engine.store.pool)
                    .await
                    .unwrap();
            }
            "read-only" => {
                let organ = store::organs::local(&engine.store.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .uid;
                let mut cells = engine
                    .roster_of(&organ)
                    .await
                    .unwrap()
                    .unwrap()
                    .roster
                    .cells;
                for cell in &mut cells {
                    cell.capabilities.clear();
                }
                let root = Signer::from_bytes(&organ, crate::roster::ROOT_KEY_ID, [208; 32]);
                engine.publish_roster(&root, cells).await.unwrap();
            }
            _ => {}
        }
        signals.release.notify_one();
        let result = worker.await.unwrap().unwrap();
        assert_eq!(result, usize::from(change == "unchanged"), "{change}");
        assert_eq!(
            store::organs::contacts(&engine.store.pool)
                .await
                .unwrap()
                .len(),
            usize::from(change == "unchanged"),
            "{change}"
        );
        assert_eq!(
            store::records::get_extension(&engine.store.pool, &records[1], REVEAL_NAMESPACE)
                .await
                .unwrap()
                .unwrap(),
            state
        );
    }
}

#[async_trait::async_trait]
impl Network for Answer {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let PublicRequest::AskContacts { document } = request else {
            panic!("Expected contact query");
        };
        Ok(
            json!({"service":destination,"reply":nucleus::social::ask::Reply {
                id: document.id, documents: vec![self.0.clone()], partial: false,
            }}),
        )
    }
}

#[tokio::test]
async fn contact_results_recheck_actor_permission_after_the_last_answer_check() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let peer = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&peer.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let endpoint = iroh::SecretKey::from_bytes(&[206; 32]).public().to_string();
    store::organs::add_contact(&engine.store.pool, &organ, None, "Contact", "", 1)
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, &organ, "known")
        .await
        .unwrap();
    store::organs::set_node_id(&engine.store.pool, &organ, Some(&endpoint))
        .await
        .unwrap();
    for command in [
        Command::ConfigureAsk { enabled: true },
        Command::SetAskContact {
            choice: nucleus::social::ask::ContactConsent {
                organ: organ.clone(),
                ask: true,
                answer: false,
                forward: false,
            },
        },
    ] {
        engine
            .social_command(command, None, nucleus::execution::now())
            .await
            .unwrap();
    }
    let role = store::auth::ensure_role(&engine.store.pool, "query-editor")
        .await
        .unwrap();
    for (subject, action) in [("organ", "update"), ("record", "read"), ("view", "stream")] {
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    let actor =
        store::auth::create_person_login(&engine.store.pool, "Editor", "query", "hash", role)
            .await
            .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&actor), &organ)
        .await
        .unwrap();
    let (document, _) = qualification::publication(9, &endpoint);
    let network = Arc::new(Answer(document.clone()));
    engine.attach_social_network(network.clone());
    engine
        .social_command(
            Command::StartAsk {
                query: Search {
                    text: "Bicycle".into(),
                    ..Default::default()
                },
                contacts: vec![organ],
            },
            Some(&actor),
            nucleus::execution::now(),
        )
        .await
        .unwrap();
    let signals = Arc::new(Pause {
        after: 1,
        ..Default::default()
    });
    let worker = {
        let engine = engine.clone();
        let signals = signals.clone();
        tokio::spawn(async move { PAUSE.scope(signals, engine.social_ask_once()).await })
    };
    tokio::time::timeout(Duration::from_secs(10), signals.entered.notified())
        .await
        .unwrap();
    store::sqlx::query("DELETE FROM role_permission WHERE role_id=?")
        .bind(role)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    signals.release.notify_one();
    assert_eq!(worker.await.unwrap().unwrap(), 0);
    let result: (String, String) =
        store::sqlx::query_as("SELECT state,results FROM social_ask_query")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    assert_eq!(result, ("pending".into(), "[]".into()));
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_document WHERE id=?")
            .bind(&document.id)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn social_actions_recheck_the_original_actor_before_writing() {
    for command in [
        Command::ConfigureAsk { enabled: true },
        Command::ConfigureGossip { enabled: true },
        Command::ConfigureSubscriptions { enabled: true },
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
        Command::RotateProfileAuthority,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Must remain private".into(),
                ..Default::default()
            },
        },
    ] {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("root.key");
        std::fs::write(&root, [205; 32]).unwrap();
        engine.set_root_key_path(root);
        engine.set_sealing_keyring_path(directory.path().join("sealing.json"));
        engine.social_checked_root_signer().await.unwrap().unwrap();
        let role = store::auth::ensure_role(&engine.store.pool, "social-editor")
            .await
            .unwrap();
        for (subject, action) in [
            ("organ", "update"),
            ("record", "update"),
            ("record", "read"),
            ("view", "stream"),
        ] {
            let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        let actor =
            store::auth::create_person_login(&engine.store.pool, "Editor", "editor", "hash", role)
                .await
                .unwrap();
        let before: Vec<(String, String, String)> = store::sqlx::query_as(
            "SELECT record_uid,namespace,fds FROM record_extension ORDER BY record_uid,namespace",
        )
        .fetch_all(&engine.store.pool)
        .await
        .unwrap();
        let signals = Arc::new(Pause::default());
        let worker = {
            let engine = engine.clone();
            let signals = signals.clone();
            let actor = actor.clone();
            tokio::spawn(async move {
                PAUSE
                    .scope(signals, async {
                        engine
                            .social_command(command, Some(&actor), nucleus::execution::now())
                            .await
                    })
                    .await
            })
        };
        tokio::time::timeout(Duration::from_secs(10), signals.entered.notified())
            .await
            .unwrap();
        store::sqlx::query("DELETE FROM role_permission WHERE role_id=?")
            .bind(role)
            .execute(&engine.store.pool)
            .await
            .unwrap();
        signals.release.notify_one();
        let result = tokio::time::timeout(Duration::from_secs(10), worker)
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(result, Err(EngineError::Forbidden(_))),
            "{result:?}"
        );
        let after: Vec<(String, String, String)> = store::sqlx::query_as(
            "SELECT record_uid,namespace,fds FROM record_extension ORDER BY record_uid,namespace",
        )
        .fetch_all(&engine.store.pool)
        .await
        .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_private_outbox")
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_publication_job")
                .fetch_one(&engine.store.pool)
                .await
                .unwrap(),
            0
        );
    }
}
