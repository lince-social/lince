use engine::{Engine, EngineError, social::Network};
use nucleus::social::{
    Command, PostDraft, PostState, PublicRequest, Snippet,
    gossip::{ContactConsent, Payload},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

struct ContactNetwork {
    source: String,
    peers: Arc<BTreeMap<String, Arc<Engine>>>,
    traffic: Arc<Mutex<Vec<String>>>,
    lose: AtomicBool,
}

#[async_trait::async_trait]
impl Network for ContactNetwork {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let delivery = matches!(&request, PublicRequest::GossipDeliver { .. });
        self.traffic
            .lock()
            .unwrap()
            .push(serde_json::to_string(&request).unwrap());
        let response = self
            .peers
            .get(destination)
            .unwrap()
            .social_public_request(
                &self.source,
                destination,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await?;
        if delivery && self.lose.swap(false, Ordering::SeqCst) {
            return Err(EngineError::Consequence(
                "Lost response after durable public import".into(),
            ));
        }
        Ok(response)
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

async fn known(engine: &Engine, peer: &Engine, endpoint: &str) {
    let uid = store::organs::local(&peer.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::organs::add_contact(&engine.store.pool, &uid, None, "Known contact", "", 1)
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, &uid, "known")
        .await
        .unwrap();
    store::organs::set_node_id(&engine.store.pool, &uid, Some(endpoint))
        .await
        .unwrap();
    command(
        engine,
        Command::SetGossipContact {
            choice: ContactConsent {
                organ: uid,
                send: true,
                receive: true,
            },
        },
    )
    .await;
}

#[tokio::test]
async fn public_directory_results_do_not_export_the_private_gossip_contact_path() {
    let author = Engine::open_memory().await.unwrap();
    let receiver = Engine::open_memory().await.unwrap();
    let contact_endpoint = iroh::SecretKey::from_bytes(&[185; 32]).public().to_string();
    let directory_endpoint = iroh::SecretKey::from_bytes(&[186; 32]).public().to_string();
    known(&receiver, &author, &contact_endpoint).await;
    command(&receiver, Command::ConfigureGossip { enabled: true }).await;
    command(
        &receiver,
        Command::ConfigureServices {
            settings: nucleus::social::ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
    )
    .await;
    let (_, document) = publish(&author, "Bicycle help through a contact", true).await;
    receiver
        .social_public_request(
            &contact_endpoint,
            &directory_endpoint,
            PublicRequest::GossipDeliver {
                payload: Box::new(Payload::Snippet {
                    document: Box::new(document.clone()),
                }),
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    let local = command(
        &receiver,
        Command::Search {
            query: nucleus::social::Search {
                text: "bicycle".into(),
                ..Default::default()
            },
            services: vec![],
        },
    )
    .await;
    assert!(
        serde_json::to_string(&local)
            .unwrap()
            .contains(&contact_endpoint)
    );
    let public = receiver
        .social_public_request(
            "unknown-directory-reader",
            &directory_endpoint,
            PublicRequest::Search {
                query: nucleus::social::Search {
                    text: "bicycle".into(),
                    ..Default::default()
                },
                known: vec![],
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(public["results"][0]["document"]["id"], document.id);
    let bytes = serde_json::to_string(&public).unwrap();
    assert!(
        !bytes.contains(&contact_endpoint),
        "A directory exported its private incoming contact path"
    );
    let contact = store::organs::local(&author.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    assert!(!bytes.contains(&contact));
}

async fn publish(engine: &Engine, title: &str, redistribute: bool) -> (String, Snippet) {
    let saved = command(
        engine,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: title.into(),
                redistribute,
                destinations: vec![iroh::SecretKey::from_bytes(&[189; 32]).public().to_string()],
                ..Default::default()
            },
        },
    )
    .await;
    let uid = saved["record"].as_str().unwrap().to_owned();
    let document = transition(engine, &uid, PostState::Active).await;
    (uid, document)
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
async fn host_removal_cancels_queued_public_gossip_but_still_forwards_signed_withdrawal() {
    let author = Arc::new(Engine::open_memory().await.unwrap());
    let receiver = Arc::new(Engine::open_memory().await.unwrap());
    let source = iroh::SecretKey::from_bytes(&[176; 32]).public().to_string();
    let destination = iroh::SecretKey::from_bytes(&[177; 32]).public().to_string();
    known(&author, &receiver, &destination).await;
    known(&receiver, &author, &source).await;
    command(&author, Command::ConfigureGossip { enabled: true }).await;
    command(&receiver, Command::ConfigureGossip { enabled: true }).await;
    let (record, document) = publish(&author, "Host removed public listing", true).await;
    assert!(author.social_gossip_once().await.is_err());
    let pending: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM social_gossip_forward WHERE state='pending'",
    )
    .fetch_one(&author.store.pool)
    .await
    .unwrap();
    assert!(pending > 0);
    command(
        &author,
        Command::RemoveListing {
            post: document.id.clone(),
            reason: "Local operator choice".into(),
        },
    )
    .await;
    let peers = Arc::new(BTreeMap::from([(destination, receiver.clone())]));
    let traffic = Arc::new(Mutex::new(Vec::new()));
    let network = Arc::new(ContactNetwork {
        source,
        peers,
        traffic: traffic.clone(),
        lose: AtomicBool::new(false),
    });
    author.attach_social_network(network.clone());
    for _ in 0..3 {
        author.social_gossip_once().await.unwrap();
    }
    let received: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM social_document WHERE kind='snippet' AND id=?",
    )
    .bind(&document.id)
    .fetch_one(&receiver.store.pool)
    .await
    .unwrap();
    assert_eq!(received, 0);
    let ending = transition(&author, &record, PostState::Withdrawn).await;
    for _ in 0..3 {
        author.social_gossip_once().await.unwrap();
    }
    let state: String = store::sqlx::query_scalar(
        "SELECT state FROM social_document WHERE kind='snippet' AND id=?",
    )
    .bind(&ending.id)
    .fetch_one(&receiver.store.pool)
    .await
    .unwrap();
    assert_eq!(state, "withdrawn");
    for bytes in traffic.lock().unwrap().iter() {
        let request: PublicRequest = serde_json::from_str(bytes).unwrap();
        if let PublicRequest::GossipDeliver { payload } = request
            && let Payload::Snippet { document } = *payload
        {
            assert_eq!(document.state, PostState::Withdrawn);
        }
    }
}

#[tokio::test]
async fn signed_conflict_cancels_already_queued_snippets_without_stopping_withdrawals() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};

    let author = Arc::new(Engine::open_memory().await.unwrap());
    let receiver = Arc::new(Engine::open_memory().await.unwrap());
    let source = iroh::SecretKey::from_bytes(&[174; 32]).public().to_string();
    let destination = iroh::SecretKey::from_bytes(&[175; 32]).public().to_string();
    known(&author, &receiver, &destination).await;
    known(&receiver, &author, &source).await;
    command(&author, Command::ConfigureGossip { enabled: true }).await;
    command(&receiver, Command::ConfigureGossip { enabled: true }).await;
    let (record, document) = publish(&author, "Signed conflict with queued gossip", true).await;
    assert!(author.social_gossip_once().await.is_err());
    assert!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_gossip_forward WHERE state='pending'"
        )
        .fetch_one(&author.store.pool)
        .await
        .unwrap()
            > 0
    );
    let state = store::records::get_extension(
        &author.store.pool,
        &record,
        nucleus::social::PUBLICATION_NAMESPACE,
    )
    .await
    .unwrap()
    .unwrap();
    let secret: [u8; 32] = B64
        .decode(state["secret"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let signer = engine::trust::Signer::from_bytes("", "social", secret);
    let mut conflicting = document.clone();
    conflicting.text = "Contradictory signed copy of the same revision".into();
    conflicting.signature =
        signer.sign_bytes(&engine::social::signing_bytes("snippet", &conflicting).unwrap());
    author
        .social_public_request(
            &destination,
            &source,
            PublicRequest::GossipDeliver {
                payload: Box::new(Payload::Snippet {
                    document: Box::new(conflicting),
                }),
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, String>(
            "SELECT state FROM social_document WHERE kind='snippet' AND id=?"
        )
        .bind(&document.id)
        .fetch_one(&author.store.pool)
        .await
        .unwrap(),
        "conflict"
    );
    let peers = Arc::new(BTreeMap::from([(destination, receiver.clone())]));
    let traffic = Arc::new(Mutex::new(Vec::new()));
    let network = Arc::new(ContactNetwork {
        source,
        peers,
        traffic: traffic.clone(),
        lose: AtomicBool::new(false),
    });
    author.attach_social_network(network.clone());
    for _ in 0..3 {
        author.social_gossip_once().await.unwrap();
    }
    let snippet_hash = engine::social::document_hash("snippet", &document).unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_gossip_forward WHERE hash=? AND state='pending'"
        )
        .bind(&snippet_hash)
        .fetch_one(&author.store.pool)
        .await
        .unwrap(),
        0
    );
    let ending = transition(&author, &record, PostState::Withdrawn).await;
    for _ in 0..3 {
        author.social_gossip_once().await.unwrap();
    }
    assert_eq!(
        store::sqlx::query_scalar::<_, String>(
            "SELECT state FROM social_document WHERE kind='snippet' AND id=?"
        )
        .bind(&ending.id)
        .fetch_one(&receiver.store.pool)
        .await
        .unwrap(),
        "withdrawn"
    );
    for bytes in traffic.lock().unwrap().iter() {
        let request: PublicRequest = serde_json::from_str(bytes).unwrap();
        if let PublicRequest::GossipDeliver { payload } = request
            && let Payload::Snippet { document } = *payload
        {
            assert_eq!(document.state, PostState::Withdrawn);
        }
    }
}

#[tokio::test]
async fn contact_gossip_cycles_retry_exact_bytes_bound_fanout_and_propagate_withdrawals() {
    Box::pin(async {
        let mut peers=BTreeMap::new();
        for id in 190..195 {peers.insert(iroh::SecretKey::from_bytes(&[id;32]).public().to_string(),Arc::new(Engine::open_memory().await.unwrap()));}
        let peers=Arc::new(peers);
        let endpoints:Vec<_>=peers.keys().cloned().collect();
        let traffic=Arc::new(Mutex::new(Vec::new()));
        let mut networks=Vec::new();
        for (endpoint,engine) in peers.iter() {
            for (other,person) in peers.iter() {if other!=endpoint {known(engine,person,other).await;}}
            command(engine,Command::ConfigureGossip {enabled:true}).await;
            let network=Arc::new(ContactNetwork {source:endpoint.clone(),peers:peers.clone(),traffic:traffic.clone(),lose:AtomicBool::new(endpoint==&endpoints[0])});
            engine.attach_social_network(network.clone());
            networks.push(network);
        }
        let author=&peers[&endpoints[0]];
        let (record,document)=publish(author,"Bicycle help",true).await;
        let (_,private_to_directory)=publish(author,"No redistribution",false).await;
        let mut changed:PostDraft=serde_json::from_value(command(author,Command::Overview).await["posts"].as_array().unwrap().iter().find(|p|p["record"]==record).unwrap()["draft"].clone()).unwrap();
        changed.redistribute=false;
        assert!(author.social_command(Command::SaveDraft {record:Some(record.clone()),source:None,draft:changed},None,nucleus::execution::now()).await.is_err());
        for _ in 0..12 {for engine in peers.values() {
            store::sqlx::query("UPDATE social_gossip_forward SET next_attempt=0 WHERE state='pending'").execute(&engine.store.pool).await.unwrap();
            engine.social_gossip_once().await.unwrap();
        }}
        let mut holders=0;
        for engine in peers.values() {
            let count:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM social_document WHERE kind='snippet' AND id=? AND state='active'").bind(&document.id).fetch_one(&engine.store.pool).await.unwrap();
            holders+=count;
            let fanout:Vec<i64>=store::sqlx::query_scalar("SELECT COUNT(*) FROM social_gossip_forward GROUP BY hash").fetch_all(&engine.store.pool).await.unwrap();
            assert!(fanout.iter().all(|n|*n<=3));
            if !Arc::ptr_eq(engine,author) {
                let hidden:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM social_document WHERE id=?").bind(&private_to_directory.id).fetch_one(&engine.store.pool).await.unwrap();
                assert_eq!(hidden,0);
            }
        }
        assert!(holders>=4);
        let bytes=traffic.lock().unwrap().join("\n");
        assert!(!bytes.contains(&record));
        for engine in peers.values() {assert!(!bytes.contains(&store::organs::local(&engine.store.pool).await.unwrap().unwrap().uid));}
        let duplicates:Vec<i64>=store::sqlx::query_scalar("SELECT COUNT(*) FROM social_gossip_forward WHERE state='accepted' GROUP BY hash").fetch_all(&author.store.pool).await.unwrap();
        assert!(duplicates.iter().all(|n|*n<=3));
        let ending=transition(author,&record,PostState::Withdrawn).await;
        for _ in 0..12 {for engine in peers.values() {engine.social_gossip_once().await.unwrap();}}
        for engine in peers.values() {
            let active:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM social_document WHERE kind='snippet' AND id=? AND state='active'").bind(&document.id).fetch_one(&engine.store.pool).await.unwrap();
            assert_eq!(active,0);
        }
        let fresh=Engine::open_memory().await.unwrap();
        let endpoint=iroh::SecretKey::from_bytes(&[195;32]).public().to_string();
        known(&fresh,author,&endpoints[0]).await;
        command(&fresh,Command::ConfigureGossip {enabled:true}).await;
        for doc in [ending,document] {
            fresh.social_public_request(&endpoints[0],&endpoint,PublicRequest::GossipDeliver {payload:Box::new(Payload::Snippet {document:Box::new(doc)})},nucleus::execution::now().timestamp()).await.unwrap();
        }
        let result=command(&fresh,Command::Search {query:Default::default(),services:vec![]}).await;
        assert!(result["results"].as_array().unwrap().is_empty());
        let uid=store::organs::local(&author.store.pool).await.unwrap().unwrap().uid;
        store::organs::set_trust(&fresh.store.pool,&uid,"blocked").await.unwrap();
        assert!(fresh.social_public_request(&endpoints[0],&endpoint,PublicRequest::GossipOffer {offer:nucleus::social::gossip::Offer {post:nucleus::new_uid("post"),hash:"0".repeat(64),expires_at:nucleus::execution::now().timestamp()+30}},nucleus::execution::now().timestamp()).await.is_err());
        drop(networks);
    }).await;
}

#[tokio::test]
async fn gossip_is_disabled_by_default_and_preserves_control_reserve_when_active_admission_is_full()
{
    let author = Engine::open_memory().await.unwrap();
    let host = Engine::open_memory().await.unwrap();
    let source = iroh::SecretKey::from_bytes(&[196; 32]).public().to_string();
    let endpoint = iroh::SecretKey::from_bytes(&[197; 32]).public().to_string();
    let (record, document) = publish(&author, "Public help", true).await;
    let request = || PublicRequest::GossipDeliver {
        payload: Box::new(Payload::Snippet {
            document: Box::new(document.clone()),
        }),
    };
    assert!(
        host.social_public_request(
            &source,
            &endpoint,
            request(),
            nucleus::execution::now().timestamp()
        )
        .await
        .is_err()
    );
    known(&host, &author, &source).await;
    assert!(
        host.social_public_request(
            &source,
            &endpoint,
            request(),
            nucleus::execution::now().timestamp()
        )
        .await
        .is_err()
    );
    command(&host, Command::ConfigureGossip { enabled: true }).await;
    let now = nucleus::execution::now().timestamp();
    store::sqlx::query("WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM numbers WHERE n<27000) INSERT INTO social_gossip_seen(hash,expires_at,control) SELECT printf('%064x',n),?,0 FROM numbers").bind(now+86400).execute(&host.store.pool).await.unwrap();
    assert!(
        host.social_public_request(&source, &endpoint, request(), now)
            .await
            .is_err()
    );
    let ending = transition(&author, &record, PostState::Withdrawn).await;
    host.social_public_request(
        &source,
        &endpoint,
        PublicRequest::GossipDeliver {
            payload: Box::new(Payload::Snippet {
                document: Box::new(ending),
            }),
        },
        now,
    )
    .await
    .unwrap();
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_gossip_seen")
        .fetch_one(&host.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 27001);
    assert!(!host.social_settings().await.unwrap().directory);
}

#[tokio::test]
async fn gossip_known_revocations_and_deduplication_survive_a_host_restart() {
    let author = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let key_path = directory.path().join("root.key");
    std::fs::write(&key_path, [198; 32]).unwrap();
    author.set_root_key_path(key_path);
    command(
        &author,
        Command::SaveProfile {
            fields: nucleus::social::ProfileFields {
                name: "Public author".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![],
        },
    )
    .await;
    let saved = command(
        &author,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Identified contribution".into(),
                mode: nucleus::social::AuthorMode::Identified,
                redistribute: true,
                destinations: vec![iroh::SecretKey::from_bytes(&[199; 32]).public().to_string()],
                ..Default::default()
            },
        },
    )
    .await;
    let document = transition(
        &author,
        saved["record"].as_str().unwrap(),
        PostState::Active,
    )
    .await;
    let path = directory.path().join("host.sqlite");
    let host = Engine::open(path.to_str().unwrap()).await.unwrap();
    let source = iroh::SecretKey::from_bytes(&[200; 32]).public().to_string();
    let endpoint = iroh::SecretKey::from_bytes(&[201; 32]).public().to_string();
    known(&host, &author, &source).await;
    command(&host, Command::ConfigureGossip { enabled: true }).await;
    let old = || PublicRequest::GossipDeliver {
        payload: Box::new(Payload::Snippet {
            document: Box::new(document.clone()),
        }),
    };
    let now = nucleus::execution::now().timestamp();
    host.social_public_request(&source, &endpoint, old(), now)
        .await
        .unwrap();
    command(&author, Command::RotateProfileAuthority).await;
    let organ = store::organs::local(&author.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let private = store::records::get_extension(
        &author.store.pool,
        &organ,
        nucleus::social::PRIVATE_NAMESPACE,
    )
    .await
    .unwrap()
    .unwrap();
    let authority: nucleus::social::Delegation =
        serde_json::from_value(private["profile_signer"]["authority"].clone()).unwrap();
    assert_ne!(
        authority.generation,
        document.profile.as_ref().unwrap().generation
    );
    let control = Payload::ProfileAuthority {
        proof: Box::new(document.clone()),
        authority: Box::new(authority),
    };
    let receipt = host
        .social_public_request(
            &source,
            &endpoint,
            PublicRequest::GossipDeliver {
                payload: Box::new(control.clone()),
            },
            now,
        )
        .await
        .unwrap();
    assert!(
        host.social_public_request(&source, &endpoint, old(), now)
            .await
            .is_err()
    );
    drop(host);
    let restored = Engine::open(path.to_str().unwrap()).await.unwrap();
    let replay = restored
        .social_public_request(
            &source,
            &endpoint,
            PublicRequest::GossipDeliver {
                payload: Box::new(control),
            },
            now,
        )
        .await
        .unwrap();
    assert_eq!(replay, receipt);
    assert!(
        restored
            .social_public_request(&source, &endpoint, old(), now)
            .await
            .is_err()
    );
    let active: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM social_document WHERE id=? AND state='active'",
    )
    .bind(&document.id)
    .fetch_one(&restored.store.pool)
    .await
    .unwrap();
    assert_eq!(active, 0);
    let retained: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_gossip_seen")
        .fetch_one(&restored.store.pool)
        .await
        .unwrap();
    assert_eq!(retained, 2);
}

#[tokio::test]
async fn real_contact_transport_forwards_queries_and_withdrawals_without_private_replica_grants() {
    Box::pin(async {
        let mut engines = Vec::new();
        let mut wires = Vec::new();
        let mut tasks = Vec::new();
        for key in 232..235 {
            let engine = Arc::new(Engine::open_memory().await.unwrap());
            let wire = Arc::new(
                engine::wire::Wire::bind_with_discovery(
                    engine.clone(),
                    iroh::SecretKey::from_bytes(&[key; 32]),
                    engine::wire::Reach::Local,
                    None,
                    false,
                )
                .await
                .unwrap(),
            );
            engine.attach_social_network(wire.clone());
            let server = wire.clone();
            tasks.push(tokio::spawn(async move { server.serve().await }));
            engines.push(engine);
            wires.push(wire);
        }
        for (index, engine) in engines.iter().enumerate() {
            for (other, peer) in engines.iter().enumerate() {
                if index != other {
                    let endpoint = wires[other].node_id().to_string();
                    known(engine, peer, &endpoint).await;
                    let organ = store::organs::local(&peer.store.pool)
                        .await
                        .unwrap()
                        .unwrap()
                        .uid;
                    command(
                        engine,
                        Command::SetAskContact {
                            choice: nucleus::social::ask::ContactConsent {
                                organ,
                                ask: true,
                                answer: true,
                                forward: true,
                            },
                        },
                    )
                    .await;
                    let port = wires[other]
                        .endpoint()
                        .bound_sockets()
                        .into_iter()
                        .next()
                        .unwrap()
                        .port();
                    let addr = iroh::EndpointAddr::new(wires[other].node_id())
                        .with_ip_addr(([127, 0, 0, 1], port).into());
                    wires[index].remember_addr(addr.clone());
                    let connection = wires[index]
                        .endpoint()
                        .connect(addr, engine::wire::ALPN_SOCIAL)
                        .await
                        .unwrap();
                    let (mut send, mut recv) = connection.open_bi().await.unwrap();
                    send.write_all(&serde_json::to_vec(&PublicRequest::DescribeService).unwrap())
                        .await
                        .unwrap();
                    send.finish().unwrap();
                    let bytes = recv
                        .read_to_end(nucleus::social::MAX_FRAME_BYTES)
                        .await
                        .unwrap();
                    let response: Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(response["data"]["descriptor"]["endpoint"], endpoint);
                    connection.close(0u32.into(), b"contact route reviewed");
                }
            }
            command(engine, Command::ConfigureGossip { enabled: true }).await;
            command(engine, Command::ConfigureAsk { enabled: true }).await;
        }
        let (record, document) = publish(
            &engines[0],
            "Bicycle help over authenticated contact transport",
            true,
        )
        .await;
        publish(
            &engines[1],
            "Bicycle contribution from another contact",
            true,
        )
        .await;
        for _ in 0..6 {
            for engine in &engines {
                engine.social_gossip_once().await.unwrap();
            }
        }
        for engine in &engines {
            let rows = command(
                engine,
                Command::Search {
                    query: nucleus::social::Search {
                        text: "Bicycle".into(),
                        ..Default::default()
                    },
                    services: vec![],
                },
            )
            .await;
            assert_eq!(rows["results"].as_array().unwrap().len(), 2);
            assert!(!serde_json::to_string(&rows).unwrap().contains(&record));
            assert!(!engine.social_settings().await.unwrap().directory);
        }
        let peer = store::organs::local(&engines[1].store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        command(
            &engines[0],
            Command::StartAsk {
                query: nucleus::social::Search {
                    text: "Bicycle".into(),
                    ..Default::default()
                },
                contacts: vec![peer],
            },
        )
        .await;
        assert_eq!(engines[0].social_ask_once().await.unwrap(), 1);
        let queried = command(&engines[0], Command::AskStatus).await;
        assert_eq!(queried["results"].as_array().unwrap().len(), 2);
        transition(&engines[0], &record, PostState::Withdrawn).await;
        for _ in 0..6 {
            for engine in &engines {
                engine.social_gossip_once().await.unwrap();
            }
        }
        for engine in &engines {
            let active: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM social_document WHERE id=? AND state='active'",
            )
            .bind(&document.id)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            assert_eq!(active, 0);
            let grants: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM replica_grant")
                .fetch_one(&engine.store.pool)
                .await
                .unwrap();
            assert_eq!(grants, 0);
        }
        let filtered = command(&engines[0], Command::AskStatus).await;
        assert_eq!(filtered["results"].as_array().unwrap().len(), 1);
        for task in tasks {
            task.abort();
        }
        for wire in wires {
            wire.endpoint().close().await;
        }
    })
    .await;
}
