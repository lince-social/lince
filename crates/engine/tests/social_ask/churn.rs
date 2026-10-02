use super::*;
use nucleus::social::ask::Reply;
use std::time::{Duration, Instant};

struct DelayedAnswer {
    document: Snippet,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    destinations: Mutex<Vec<String>>,
}

struct NoRequest;

#[async_trait::async_trait]
impl Network for NoRequest {
    async fn request(&self, _: &str, _: PublicRequest) -> Result<Value, EngineError> {
        panic!("A stale contact endpoint must not receive a query");
    }
}

#[tokio::test]
async fn unfinished_forwarded_question_reopens_with_local_partial_answers_without_new_work() {
    let clock = nucleus::execution::Execution::new([217; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let author = Engine::open_memory().await.unwrap();
            let onward = Engine::open_memory().await.unwrap();
            let (_, document) = publish(&author, "Bicycle answer after restart", true, true).await;
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("unfinished.sqlite");
            let host = Arc::new(Engine::open(path.to_str().unwrap()).await.unwrap());
            let source = iroh::SecretKey::from_bytes(&[211; 32]).public().to_string();
            let endpoint = iroh::SecretKey::from_bytes(&[212; 32]).public().to_string();
            let child = iroh::SecretKey::from_bytes(&[213; 32]).public().to_string();
            let source_organ = known(&host, &author, &source).await;
            known(&host, &onward, &child).await;
            command(&host, Command::ConfigureAsk { enabled: true }).await;
            command(
                &host,
                Command::SetGossipContact {
                    choice: nucleus::social::gossip::ContactConsent {
                        organ: source_organ.clone(),
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
            let network = Arc::new(DelayedAnswer {
                document: document.clone(),
                entered: Default::default(),
                release: Default::default(),
                destinations: Default::default(),
            });
            host.attach_social_network(network.clone());
            let request = Request {
                id: nucleus::new_uid("ask"),
                query: Search {
                    text: "Bicycle".into(),
                    ..Default::default()
                },
                issued_at: clock.now().timestamp(),
                deadline: clock.now().timestamp() + 30,
                work: 12,
                bytes: 192 * 1024,
                results: 50,
                depth: 2,
            };
            let worker = {
                let host = host.clone();
                let clock = clock.clone();
                let source = source.clone();
                let endpoint = endpoint.clone();
                let request = request.clone();
                tokio::spawn(async move {
                    clock
                        .scope(async {
                            host.social_public_request(
                                &source,
                                &endpoint,
                                PublicRequest::AskContacts {
                                    document: Box::new(request),
                                },
                                nucleus::execution::now().timestamp(),
                            )
                            .await
                        })
                        .await
                })
            };
            tokio::time::timeout(Duration::from_secs(2), network.entered.notified())
                .await
                .unwrap();
            worker.abort();
            assert!(worker.await.unwrap_err().is_cancelled());
            let reserved: (i64, i64, Option<String>) = store::sqlx::query_as(
                "SELECT deadline,reserved,reply FROM social_ask_seen WHERE id=?",
            )
            .bind(&request.id)
            .fetch_one(&host.store.pool)
            .await
            .unwrap();
            assert_eq!(reserved.0, request.deadline);
            assert_eq!(reserved.1, i64::from(request.bytes) + 1024);
            assert!(reserved.2.is_none());
            assert_eq!(*network.destinations.lock().unwrap(), vec![child]);
            host.store.pool.close().await;
            drop(host);
            let reopened = Engine::open(path.to_str().unwrap()).await.unwrap();
            let no_request = Arc::new(NoRequest);
            reopened.attach_social_network(no_request.clone());
            let mut first = None;
            for _ in 0..2 {
                let reply = reopened
                    .social_public_request(
                        &source,
                        &endpoint,
                        PublicRequest::AskContacts {
                            document: Box::new(request.clone()),
                        },
                        clock.now().timestamp(),
                    )
                    .await
                    .unwrap();
                assert_eq!(reply["reply"]["partial"], true);
                assert_eq!(reply["reply"]["documents"].as_array().unwrap().len(), 1);
                assert_eq!(reply["reply"]["documents"][0]["id"], document.id);
                assert_eq!(reply["reply"]["id"], request.id);
                if let Some(first) = &first {
                    assert_eq!(&reply, first);
                } else {
                    first = Some(reply);
                }
            }
            assert_eq!(
                store::sqlx::query_as::<_, (i64, i64, Option<String>)>(
                    "SELECT deadline,reserved,reply FROM social_ask_seen WHERE id=?"
                )
                .bind(&request.id)
                .fetch_one(&reopened.store.pool)
                .await
                .unwrap(),
                reserved
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_ask_seen")
                    .fetch_one(&reopened.store.pool)
                    .await
                    .unwrap(),
                1
            );
            store::organs::set_trust(&reopened.store.pool, &source_organ, "blocked")
                .await
                .unwrap();
            assert!(
                reopened
                    .social_public_request(
                        &source,
                        &endpoint,
                        PublicRequest::AskContacts {
                            document: Box::new(request.clone())
                        },
                        clock.now().timestamp()
                    )
                    .await
                    .is_err()
            );
            reopened.store.pool.close().await;
        }))
        .await;
}

#[async_trait::async_trait]
impl Network for DelayedAnswer {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let PublicRequest::AskContacts { document } = request else {
            panic!("Only contact search is expected");
        };
        self.destinations.lock().unwrap().push(destination.into());
        self.entered.notify_one();
        self.release.notified().await;
        Ok(serde_json::json!({
            "service":destination,
            "reply":Reply {
                id:document.id,
                documents:vec![self.document.clone()],
                partial:false,
            }
        }))
    }
}

#[tokio::test]
async fn late_contact_answers_require_the_original_pin_and_current_consent() {
    let execution = nucleus::execution::Execution::new([216; 32], 1_790_899_200_000).unwrap();
    execution
        .scope(Box::pin(async {
            let author = Engine::open_memory().await.unwrap();
            let alternative = Engine::open_memory().await.unwrap();
            let (_, document) = publish(&author, "Late bicycle answer", true, true).await;
            let endpoint = iroh::SecretKey::from_bytes(&[217; 32]).public().to_string();
            let replacement = iroh::SecretKey::from_bytes(&[218; 32]).public().to_string();
            for change in ["endpoint", "reassigned", "consent", "blocked", "cancelled"] {
                let origin = Arc::new(Engine::open_memory().await.unwrap());
                let selected = known(&origin, &author, &endpoint).await;
                let other = known(&origin, &alternative, &replacement).await;
                command(&origin, Command::ConfigureAsk { enabled: true }).await;
                let network = Arc::new(DelayedAnswer {
                    document: document.clone(),
                    entered: Default::default(),
                    release: Default::default(),
                    destinations: Default::default(),
                });
                origin.attach_social_network(network.clone());
                let opened = command(
                    &origin,
                    Command::StartAsk {
                        query: Search {
                            text: "bicycle".into(),
                            ..Default::default()
                        },
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
                    let clock = execution.clone();
                    tokio::spawn(async move { clock.scope(engine.social_ask_once()).await })
                };
                tokio::time::timeout(Duration::from_secs(2), network.entered.notified())
                    .await
                    .unwrap();
                match change {
                    "endpoint" => store::organs::set_node_id(
                        &origin.store.pool,
                        &selected,
                        Some(&iroh::SecretKey::from_bytes(&[219; 32]).public().to_string()),
                    )
                    .await
                    .unwrap(),
                    "reassigned" => {
                        store::organs::set_node_id(&origin.store.pool, &selected, None)
                            .await
                            .unwrap();
                        store::organs::set_node_id(&origin.store.pool, &other, Some(&endpoint))
                            .await
                            .unwrap();
                    }
                    "consent" => {
                        command(
                            &origin,
                            Command::SetAskContact {
                                choice: ContactConsent {
                                    organ: selected,
                                    ask: false,
                                    answer: true,
                                    forward: true,
                                },
                            },
                        )
                        .await;
                    }
                    "blocked" => store::organs::set_trust(&origin.store.pool, &selected, "blocked")
                        .await
                        .unwrap(),
                    "cancelled" => {
                        command(&origin, Command::CancelAsk { id: id.clone() }).await;
                    }
                    _ => unreachable!(),
                }
                network.release.notify_one();
                let finished = tokio::time::timeout(Duration::from_secs(3), worker)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert_eq!(finished, usize::from(change != "cancelled"), "{change}");
                let (state, results, error): (String, String, Option<String>) =
                    store::sqlx::query_as(
                        "SELECT state,results,error FROM social_ask_query WHERE id=?",
                    )
                    .bind(&id)
                    .fetch_one(&origin.store.pool)
                    .await
                    .unwrap();
                assert_eq!(results, "[]", "A {change} answer entered local history");
                assert_eq!(
                    state,
                    if change == "cancelled" {
                        "cancelled"
                    } else {
                        "completed"
                    }
                );
                assert!(change == "cancelled" || error.unwrap().contains("Partial contact search"));
                assert_eq!(
                    store::sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM social_document WHERE id=?"
                    )
                    .bind(&document.id)
                    .fetch_one(&origin.store.pool)
                    .await
                    .unwrap(),
                    0,
                    "{change}"
                );
                assert_eq!(
                    *network.destinations.lock().unwrap(),
                    vec![endpoint.clone()]
                );
            }
        }))
        .await;
}

#[tokio::test]
async fn contact_selection_stays_bounded_amid_five_hundred_stored_contacts_and_endpoint_churn() {
    let origin = Engine::open_memory().await.unwrap();
    let network = Arc::new(NoRequest);
    origin.attach_social_network(network.clone());
    let mut selected = Vec::new();
    for index in 0u32..500 {
        let organ = nucleus::new_uid("r");
        let mut bytes = [0; 32];
        bytes[..4].copy_from_slice(&index.to_le_bytes());
        let endpoint = iroh::SecretKey::from_bytes(&bytes).public().to_string();
        store::organs::add_contact(&origin.store.pool, &organ, None, "Contact", "", 1)
            .await
            .unwrap();
        store::organs::set_trust(&origin.store.pool, &organ, "known")
            .await
            .unwrap();
        store::organs::set_node_id(&origin.store.pool, &organ, Some(&endpoint))
            .await
            .unwrap();
        if index < 3 {
            selected.push(organ);
        }
    }
    for organ in &selected {
        command(
            &origin,
            Command::SetAskContact {
                choice: ContactConsent {
                    organ: organ.clone(),
                    ask: true,
                    answer: false,
                    forward: false,
                },
            },
        )
        .await;
    }
    command(&origin, Command::ConfigureAsk { enabled: true }).await;
    let started = Instant::now();
    let opened = command(
        &origin,
        Command::StartAsk {
            query: Search::default(),
            contacts: selected.clone(),
        },
    )
    .await;
    let elapsed = started.elapsed();
    let id = opened["asks"]["queries"][0]["id"].as_str().unwrap();
    let saved: (String, String, i64) =
        store::sqlx::query_as("SELECT request,peers,deadline FROM social_ask_query WHERE id=?")
            .bind(id)
            .fetch_one(&origin.store.pool)
            .await
            .unwrap();
    let peers: Vec<(String, String)> = serde_json::from_str(&saved.1).unwrap();
    assert_eq!(peers.len(), 3);
    assert!(peers.iter().all(|(organ, _)| selected.contains(organ)));
    assert!(saved.1.len() <= 2048);
    assert!(
        elapsed < Duration::from_secs(1),
        "Selection took {elapsed:?}"
    );
    for organ in &selected {
        let replacement = iroh::SecretKey::generate().public().to_string();
        store::organs::set_node_id(&origin.store.pool, organ, Some(&replacement))
            .await
            .unwrap();
    }
    let finished = origin.social_ask_once().await.unwrap();
    assert_eq!(
        finished,
        1,
        "{}",
        command(&origin, Command::AskStatus).await
    );
    let current: (String, String, i64, String, String) = store::sqlx::query_as(
        "SELECT request,peers,deadline,state,results FROM social_ask_query WHERE id=?",
    )
    .bind(id)
    .fetch_one(&origin.store.pool)
    .await
    .unwrap();
    assert_eq!((current.0, current.1, current.2), saved);
    assert_eq!(current.3, "completed");
    assert_eq!(current.4, "[]");
    eprintln!("contact-selection stored_contacts=500 selected=3 elapsed={elapsed:?}");
}
