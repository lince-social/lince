use engine::{Engine, EngineError, social::Network};
use nucleus::social::{
    Command, PostDraft, PostState, PublicRequest, Search, Snippet,
    ask::{ContactConsent, Request},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[path = "social_ask/churn.rs"]
mod churn;

struct ContactNetwork {
    source: String,
    peers: Arc<BTreeMap<String, Arc<Engine>>>,
    traffic: Arc<Mutex<Vec<Value>>>,
    pause: AtomicBool,
    entered: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl Network for ContactNetwork {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        self.traffic
            .lock()
            .unwrap()
            .push(serde_json::to_value(&request).unwrap());
        if self.pause.load(Ordering::SeqCst) {
            self.entered.notify_one();
            std::future::pending::<()>().await;
        }
        self.peers[destination]
            .social_public_request(
                &self.source,
                destination,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await
    }
}

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .social_command(request, None, nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn known(engine: &Engine, peer: &Engine, endpoint: &str) -> String {
    let organ = store::organs::local(&peer.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::organs::add_contact(&engine.store.pool, &organ, None, "Search contact", "", 1)
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, &organ, "known")
        .await
        .unwrap();
    store::organs::set_node_id(&engine.store.pool, &organ, Some(endpoint))
        .await
        .unwrap();
    command(
        engine,
        Command::SetAskContact {
            choice: ContactConsent {
                organ: organ.clone(),
                ask: true,
                answer: true,
                forward: true,
            },
        },
    )
    .await;
    organ
}

async fn publish(
    engine: &Engine,
    title: &str,
    redistribute: bool,
    hosted: bool,
) -> (String, Snippet) {
    let draft = command(
        engine,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: title.into(),
                redistribute,
                destinations: if hosted {
                    vec![iroh::SecretKey::from_bytes(&[219; 32]).public().to_string()]
                } else {
                    vec![]
                },
                ..Default::default()
            },
        },
    )
    .await;
    let record = draft["record"].as_str().unwrap().to_owned();
    let document = transition(engine, &record, PostState::Active).await;
    (record, document)
}

async fn transition(engine: &Engine, record: &str, state: PostState) -> Snippet {
    let preview = command(
        engine,
        Command::Preview {
            record: record.into(),
            state,
        },
    )
    .await;
    let document: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    command(
        engine,
        Command::Publish {
            record: record.into(),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: document.clone(),
        },
    )
    .await;
    document
}

#[tokio::test]
async fn contact_queries_bound_a_cycle_and_hide_local_only_and_nonredistributable_posts() {
    Box::pin(async {
        let mut peers = BTreeMap::new();
        for key in 220..224 {
            peers.insert(
                iroh::SecretKey::from_bytes(&[key; 32]).public().to_string(),
                Arc::new(Engine::open_memory().await.unwrap()),
            );
        }
        let peers = Arc::new(peers);
        let endpoints: Vec<_> = peers.keys().cloned().collect();
        let traffic = Arc::new(Mutex::new(Vec::new()));
        let mut networks = Vec::new();
        let mut selected = Vec::new();
        for (endpoint, engine) in peers.iter() {
            for (other, peer) in peers.iter() {
                if endpoint != other {
                    let uid = known(engine, peer, other).await;
                    if endpoint == &endpoints[0] {
                        selected.push(uid);
                    }
                }
            }
            command(engine, Command::ConfigureAsk { enabled: true }).await;
            publish(
                engine,
                &format!("Bicycle contribution {endpoint}"),
                true,
                true,
            )
            .await;
            publish(engine, "Bicycle local-only sentinel", true, false).await;
            publish(engine, "Bicycle non-redistribution sentinel", false, true).await;
            let network = Arc::new(ContactNetwork {
                source: endpoint.clone(),
                peers: peers.clone(),
                traffic: traffic.clone(),
                pause: AtomicBool::new(false),
                entered: tokio::sync::Notify::new(),
            });
            engine.attach_social_network(network.clone());
            networks.push(network);
        }
        let origin = &peers[&endpoints[0]];
        let opened = command(
            origin,
            Command::StartAsk {
                query: Search {
                    text: "Bicycle".into(),
                    ..Default::default()
                },
                contacts: selected,
            },
        )
        .await;
        assert!(!opened["results"].as_array().unwrap().is_empty());
        assert!(traffic.lock().unwrap().is_empty());
        assert_eq!(origin.social_ask_once().await.unwrap(), 1);
        let status = command(origin, Command::AskStatus).await;
        let saved = command(
            origin,
            Command::AskResults {
                id: status["asks"]["queries"][0]["id"].as_str().unwrap().into(),
            },
        )
        .await;
        assert_eq!(saved["results"], status["results"]);
        assert_eq!(status["asks"]["queries"][0]["state"], "completed");
        assert!(status["results"].as_array().unwrap().len() >= 2);
        assert!(status["results"].as_array().unwrap().iter().all(|row| {
            row["document"]["redistribute"] == true
                && !row["document"]["destinations"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        }));
        let traffic = traffic.lock().unwrap().clone();
        assert!(traffic.len() <= 11);
        let encoded = serde_json::to_string(&traffic).unwrap();
        assert!(!encoded.contains("sentinel"));
        for engine in peers.values() {
            assert!(
                !encoded.contains(
                    &store::organs::local(&engine.store.pool)
                        .await
                        .unwrap()
                        .unwrap()
                        .uid
                )
            );
        }
        for row in traffic {
            let request: PublicRequest = serde_json::from_value(row).unwrap();
            let PublicRequest::AskContacts { document } = request else {
                panic!("Only contact queries should be sent")
            };
            assert!(
                document.work <= 11
                    && document.depth < 2
                    && document.bytes <= 192 * 1024
                    && document.results <= 50
            );
        }
        command(origin, Command::ClearAsks).await;
        assert!(
            command(origin, Command::AskStatus).await["asks"]["queries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            !command(
                origin,
                Command::Search {
                    query: Search::default(),
                    services: vec![]
                }
            )
            .await["results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    })
    .await;
}

#[tokio::test]
async fn cancellation_interrupts_waiting_network_and_expiry_never_renews_a_saved_question() {
    let origin = Arc::new(Engine::open_memory().await.unwrap());
    let peer = Arc::new(Engine::open_memory().await.unwrap());
    let source = iroh::SecretKey::from_bytes(&[224; 32]).public().to_string();
    let endpoint = iroh::SecretKey::from_bytes(&[225; 32]).public().to_string();
    let selected = known(&origin, &peer, &endpoint).await;
    command(&origin, Command::ConfigureAsk { enabled: true }).await;
    let network = Arc::new(ContactNetwork {
        source,
        peers: Arc::new(BTreeMap::from([(endpoint, peer)])),
        traffic: Arc::new(Mutex::new(Vec::new())),
        pause: AtomicBool::new(true),
        entered: tokio::sync::Notify::new(),
    });
    origin.attach_social_network(network.clone());
    let opened = command(
        &origin,
        Command::StartAsk {
            query: Search::default(),
            contacts: vec![selected.clone()],
        },
    )
    .await;
    let id = opened["asks"]["queries"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let worker = {
        let engine = origin.clone();
        tokio::spawn(async move { engine.social_ask_once().await })
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        network.entered.notified(),
    )
    .await
    .unwrap();
    command(&origin, Command::CancelAsk { id: id.clone() }).await;
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        0
    );
    assert_eq!(
        command(&origin, Command::AskStatus).await["asks"]["queries"][0]["state"],
        "cancelled"
    );
    command(
        &origin,
        Command::StartAsk {
            query: Search {
                text: "Expired question".into(),
                ..Default::default()
            },
            contacts: vec![selected],
        },
    )
    .await;
    store::sqlx::query("UPDATE social_ask_query SET deadline=?,request=json_set(request,'$.deadline',?) WHERE state='pending'").bind(nucleus::execution::now().timestamp()-1).bind(nucleus::execution::now().timestamp()-1).execute(&origin.store.pool).await.unwrap();
    command(&origin, Command::ClearAsks).await;
    assert_eq!(origin.social_ask_once().await.unwrap(), 0);
    assert_eq!(network.traffic.lock().unwrap().len(), 1);
    assert!(
        command(&origin, Command::AskStatus).await["asks"]["queries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["state"] != "pending")
    );
}

#[tokio::test]
async fn saved_answers_survive_response_loss_and_restart_but_follow_blocks_and_withdrawals() {
    let author = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("query.sqlite");
    let host = Engine::open(path.to_str().unwrap()).await.unwrap();
    let source = iroh::SecretKey::from_bytes(&[226; 32]).public().to_string();
    let endpoint = iroh::SecretKey::from_bytes(&[227; 32]).public().to_string();
    let contact = known(&host, &author, &source).await;
    let (record, document) = publish(&author, "Bicycle help", true, true).await;
    command(
        &host,
        Command::SetGossipContact {
            choice: nucleus::social::gossip::ContactConsent {
                organ: contact.clone(),
                send: false,
                receive: true,
            },
        },
    )
    .await;
    command(&host, Command::ConfigureGossip { enabled: true }).await;
    host.social_public_request(
        &source,
        &endpoint,
        PublicRequest::GossipDeliver {
            payload: Box::new(nucleus::social::gossip::Payload::Snippet {
                document: Box::new(document.clone()),
            }),
        },
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap();
    let request = Request {
        id: nucleus::new_uid("ask"),
        query: Search {
            text: "Bicycle".into(),
            ..Default::default()
        },
        issued_at: nucleus::execution::now().timestamp(),
        deadline: nucleus::execution::now().timestamp() + 30,
        work: 1,
        bytes: 192 * 1024,
        results: 50,
        depth: 0,
    };
    let message = || PublicRequest::AskContacts {
        document: Box::new(request.clone()),
    };
    assert!(
        host.social_public_request(
            &source,
            &endpoint,
            message(),
            nucleus::execution::now().timestamp()
        )
        .await
        .is_err()
    );
    command(&host, Command::ConfigureAsk { enabled: true }).await;
    let first = host
        .social_public_request(
            &source,
            &endpoint,
            message(),
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(first["reply"]["documents"][0]["id"], document.id);
    drop(host);
    let restored = Engine::open(path.to_str().unwrap()).await.unwrap();
    let replay = restored
        .social_public_request(
            &source,
            &endpoint,
            message(),
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(first, replay);
    let mut conflicting = request.clone();
    conflicting.query.text = "Another question".into();
    assert!(
        restored
            .social_public_request(
                &source,
                &endpoint,
                PublicRequest::AskContacts {
                    document: Box::new(conflicting)
                },
                nucleus::execution::now().timestamp()
            )
            .await
            .is_err()
    );
    let ending = transition(&author, &record, PostState::Withdrawn).await;
    restored
        .social_public_request(
            &source,
            &endpoint,
            PublicRequest::GossipDeliver {
                payload: Box::new(nucleus::social::gossip::Payload::Snippet {
                    document: Box::new(ending),
                }),
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    let after = restored
        .social_public_request(
            &source,
            &endpoint,
            message(),
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert!(after["reply"]["documents"].as_array().unwrap().is_empty());
    store::organs::set_trust(&restored.store.pool, &contact, "blocked")
        .await
        .unwrap();
    assert!(
        restored
            .social_public_request(
                &source,
                &endpoint,
                message(),
                nucleus::execution::now().timestamp()
            )
            .await
            .is_err()
    );
    assert!(!restored.social_settings().await.unwrap().directory);
}

#[tokio::test]
async fn full_query_admission_and_local_history_limits_fail_before_onward_work() {
    let origin = Engine::open_memory().await.unwrap();
    let host = Engine::open_memory().await.unwrap();
    let source = iroh::SecretKey::from_bytes(&[228; 32]).public().to_string();
    let endpoint = iroh::SecretKey::from_bytes(&[229; 32]).public().to_string();
    known(&host, &origin, &source).await;
    let selected = known(&origin, &host, &endpoint).await;
    command(&host, Command::ConfigureAsk { enabled: true }).await;
    command(&origin, Command::ConfigureAsk { enabled: true }).await;
    let deadline = nucleus::execution::now().timestamp() + 30;
    store::sqlx::query("WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM numbers WHERE n<64) INSERT INTO social_ask_seen(id,binding,source,deadline,reserved,reply) SELECT printf('held-%d',n),'binding',?, ?,1024,'{}' FROM numbers").bind(&source).bind(deadline).execute(&host.store.pool).await.unwrap();
    let request = Request {
        id: nucleus::new_uid("ask"),
        query: Search::default(),
        issued_at: deadline - 30,
        deadline,
        work: 12,
        bytes: 192 * 1024,
        results: 50,
        depth: 2,
    };
    assert!(
        host.social_public_request(
            &source,
            &endpoint,
            PublicRequest::AskContacts {
                document: Box::new(request)
            },
            nucleus::execution::now().timestamp()
        )
        .await
        .is_err()
    );
    for _ in 0..4 {
        command(
            &origin,
            Command::StartAsk {
                query: Search::default(),
                contacts: vec![selected.clone()],
            },
        )
        .await;
    }
    assert!(
        origin
            .social_command(
                Command::StartAsk {
                    query: Search::default(),
                    contacts: vec![selected]
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_ask_query")
        .fetch_one(&origin.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 4);
    assert_eq!(
        command(&origin, Command::AskStatus).await["asks"]["queries"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[tokio::test]
async fn query_history_is_actor_scoped_and_revoked_actors_cannot_send_saved_questions() {
    let origin = Arc::new(Engine::open_memory().await.unwrap());
    let host = Arc::new(Engine::open_memory().await.unwrap());
    let endpoint = iroh::SecretKey::from_bytes(&[230; 32]).public().to_string();
    let selected = known(&origin, &host, &endpoint).await;
    command(&origin, Command::ConfigureAsk { enabled: true }).await;
    let role = store::auth::ensure_role(&origin.store.pool, "query-managers")
        .await
        .unwrap();
    let mut settings_permission = 0;
    for (subject, action) in [("organ", "update"), ("view", "stream"), ("record", "read")] {
        let permission = store::auth::ensure_permission(&origin.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&origin.store.pool, role, permission)
            .await
            .unwrap();
        if subject == "organ" {
            settings_permission = permission;
        }
    }
    let first =
        store::auth::create_person_login(&origin.store.pool, "First", "first-query", "hash", role)
            .await
            .unwrap();
    store::visibility::grant(&origin.store.pool, "actor", Some(&first), &selected)
        .await
        .unwrap();
    let second = store::auth::create_person_login(
        &origin.store.pool,
        "Second",
        "second-query",
        "hash",
        role,
    )
    .await
    .unwrap();
    let saved = origin
        .social_command(
            Command::StartAsk {
                query: Search {
                    text: "Private question sentinel".into(),
                    ..Default::default()
                },
                contacts: vec![selected],
            },
            Some(&first),
            nucleus::execution::now(),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let id = saved["asks"]["queries"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let other = origin
        .social_command(Command::AskStatus, Some(&second), nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap();
    assert!(other["asks"]["queries"].as_array().unwrap().is_empty());
    assert!(
        origin
            .social_command(
                Command::AskResults { id: id.clone() },
                Some(&second),
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    origin
        .social_command(
            Command::CancelAsk { id: id.clone() },
            Some(&second),
            nucleus::execution::now(),
        )
        .await
        .unwrap();
    let state: String = store::sqlx::query_scalar("SELECT state FROM social_ask_query WHERE id=?")
        .bind(&id)
        .fetch_one(&origin.store.pool)
        .await
        .unwrap();
    assert_eq!(state, "pending");
    let network = Arc::new(ContactNetwork {
        source: iroh::SecretKey::from_bytes(&[231; 32]).public().to_string(),
        peers: Arc::new(BTreeMap::from([(endpoint, host)])),
        traffic: Arc::new(Mutex::new(Vec::new())),
        pause: AtomicBool::new(false),
        entered: tokio::sync::Notify::new(),
    });
    origin.attach_social_network(network.clone());
    store::auth::revoke(&origin.store.pool, role, settings_permission)
        .await
        .unwrap();
    assert_eq!(origin.social_ask_once().await.unwrap(), 0);
    assert!(network.traffic.lock().unwrap().is_empty());
}
