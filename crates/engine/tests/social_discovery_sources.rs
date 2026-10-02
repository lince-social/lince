use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    Engine, EngineError,
    social::{Network, document_hash, signing_bytes},
    trust::Signer,
};
use nucleus::social::{
    Command, PUBLICATION_NAMESPACE, PostDraft, PostState, PublicRequest, Search, ServiceSettings,
    Snippet,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[path = "social_discovery_sources/boundaries.rs"]
mod boundaries;

struct Sources {
    pages: Mutex<BTreeMap<String, Value>>,
    known: Mutex<Vec<Vec<String>>>,
}

#[async_trait::async_trait]
impl Network for Sources {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        if let PublicRequest::Search { known, .. } = request {
            self.known.lock().unwrap().push(known);
        }
        self.pages
            .lock()
            .unwrap()
            .get(destination)
            .cloned()
            .ok_or_else(|| EngineError::Consequence("Source unavailable".into()))
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

async fn publish(author: &Engine, destinations: Vec<String>) -> (String, Snippet, Signer) {
    let saved = command(
        author,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Bicycle help".into(),
                text: "Original public offer".into(),
                redistribute: true,
                destinations,
                ..Default::default()
            },
        },
    )
    .await;
    let record = saved["record"].as_str().unwrap().to_owned();
    let document = transition(author, &record, PostState::Active).await;
    let state = store::records::get_extension(&author.store.pool, &record, PUBLICATION_NAMESPACE)
        .await
        .unwrap()
        .unwrap();
    let secret: [u8; 32] = B64
        .decode(state["secret"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let signer = Signer::from_bytes("", "social", secret);
    assert_eq!(signer.public_key_b64(), document.signing_key);
    (record, document, signer)
}

async fn transition(author: &Engine, record: &str, state: PostState) -> Snippet {
    let preview = command(
        author,
        Command::Preview {
            record: record.into(),
            state,
        },
    )
    .await;
    let document: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    command(
        author,
        Command::Publish {
            record: record.into(),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: document.clone(),
        },
    )
    .await;
    document
}

fn variant(document: &Snippet, signer: &Signer) -> Snippet {
    let mut variant = document.clone();
    variant.text = "A different claim at the same public revision".into();
    variant.signature = signer.sign_bytes(&signing_bytes("snippet", &variant).unwrap());
    engine::social::validate_snippet(&variant, nucleus::execution::now().timestamp()).unwrap();
    variant
}

fn page(document: &Snippet) -> Value {
    json!({"results":[{"document":document,"hash":document_hash("snippet",document).unwrap(),"source":"untrusted remote origin label"}],"updates":[]})
}

async fn search(client: &Engine, services: Vec<String>) -> Value {
    command(
        client,
        Command::Search {
            query: Search {
                text: "bicycle".into(),
                ..Default::default()
            },
            services,
        },
    )
    .await
}

#[tokio::test]
async fn signed_equivocation_hides_both_arrival_orders_survives_reopen_and_requires_newer_revision()
{
    let clock = nucleus::execution::Execution::new([220; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            for reverse in [false, true] {
                let author = Engine::open_memory().await.unwrap();
                let client = Engine::open_memory().await.unwrap();
                let a = iroh::SecretKey::from_bytes(&[221; 32]).public().to_string();
                let b = iroh::SecretKey::from_bytes(&[222; 32]).public().to_string();
                let (record, original, signer) = publish(&author, vec![a.clone(), b.clone()]).await;
                let changed = variant(&original, &signer);
                let (first, second) = if reverse {
                    (&changed, &original)
                } else {
                    (&original, &changed)
                };
                let network = Arc::new(Sources {
                    pages: Mutex::new(BTreeMap::from([
                        (a.clone(), page(first)),
                        (b.clone(), page(second)),
                    ])),
                    known: Mutex::new(Vec::new()),
                });
                client.attach_social_network(network.clone());
                let found = search(&client, vec![a.clone()]).await;
                assert_eq!(found["results"].as_array().unwrap().len(), 1);
                assert_eq!(found["results"][0]["sources"][0]["source"], a);
                assert_eq!(found["results"][0]["source"], a);
                let mut forged = second.clone();
                forged.signature = "forged signature".into();
                network
                    .pages
                    .lock()
                    .unwrap()
                    .insert(b.clone(), page(&forged));
                let refused = search(&client, vec![b.clone()]).await;
                assert_eq!(refused["failures"].as_array().unwrap().len(), 1);
                assert_eq!(refused["results"].as_array().unwrap().len(), 1);
                assert_eq!(refused["conflicts"].as_array().unwrap().len(), 0);
                network
                    .pages
                    .lock()
                    .unwrap()
                    .insert(b.clone(), page(second));
                let hidden = search(&client, vec![b.clone()]).await;
                assert!(hidden["results"].as_array().unwrap().is_empty());
                assert_eq!(hidden["conflicts"][0]["post"], original.id);
                assert_eq!(hidden["conflicts"][0]["evidence_limited"], false);
                assert_eq!(
                    store::sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM social_search WHERE id=?"
                    )
                    .bind(&original.id)
                    .fetch_one(&client.store.pool)
                    .await
                    .unwrap(),
                    0
                );
                assert!(
                    search(&client, vec![a.clone()]).await["results"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
                command(
                    &client,
                    Command::ConfigureServices {
                        settings: ServiceSettings {
                            directory: true,
                            ..Default::default()
                        },
                    },
                )
                .await;
                let public = client
                    .social_public_request(
                        "reader",
                        &a,
                        PublicRequest::Search {
                            query: Search {
                                text: "bicycle".into(),
                                ..Default::default()
                            },
                            known: vec![format!(
                                "{}:{}",
                                original.id,
                                document_hash("snippet", second).unwrap()
                            )],
                        },
                        clock.now().timestamp(),
                    )
                    .await
                    .unwrap();
                assert!(public["results"].as_array().unwrap().is_empty());
                assert!(public["updates"].as_array().unwrap().is_empty());
                let reopened = Engine::new(client.store.clone()).await.unwrap();
                let retained = search(&reopened, vec![]).await;
                assert!(retained["results"].as_array().unwrap().is_empty());
                assert_eq!(retained["conflicts"].as_array().unwrap().len(), 1);
                command(
                    &author,
                    Command::SaveDraft {
                        record: Some(record.clone()),
                        source: None,
                        draft: PostDraft {
                            title: "Bicycle help".into(),
                            text: "Author's newer resolved offer".into(),
                            redistribute: true,
                            destinations: vec![a.clone(), b.clone()],
                            ..Default::default()
                        },
                    },
                )
                .await;
                let resolved = transition(&author, &record, PostState::Active).await;
                assert_eq!(resolved.id, original.id);
                assert!(
                    resolved.revision.parse::<i64>().unwrap()
                        > original.revision.parse::<i64>().unwrap()
                );
                network
                    .pages
                    .lock()
                    .unwrap()
                    .insert(b.clone(), page(&resolved));
                let visible = search(&client, vec![b.clone()]).await;
                assert_eq!(
                    visible["results"][0]["hash"],
                    document_hash("snippet", &resolved).unwrap()
                );
                assert!(visible["conflicts"].as_array().unwrap().is_empty());
                assert_eq!(
                    store::sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM social_discovery_conflict"
                    )
                    .fetch_one(&client.store.pool)
                    .await
                    .unwrap(),
                    0
                );
                let stale = search(&client, vec![a.clone()]).await;
                assert_eq!(stale["results"][0]["document"]["text"], resolved.text);
                assert_eq!(
                    stale["results"][0]["document"]["expires_at"],
                    resolved.expires_at
                );
                assert_eq!(stale["results"][0]["sources"].as_array().unwrap().len(), 1);
                assert!(stale["conflicts"].as_array().unwrap().is_empty());
            }
        }))
        .await;
}

#[tokio::test]
async fn source_observations_are_bounded_ignore_remote_labels_and_never_renew_expiry() {
    let clock = nucleus::execution::Execution::new([223; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let author = Engine::open_memory().await.unwrap();
        let client = Engine::open_memory().await.unwrap();
        let endpoints:Vec<String> = (224..235).map(|secret|iroh::SecretKey::from_bytes(&[secret;32]).public().to_string()).collect();
        let (_,document,_) = publish(&author,vec![endpoints[0].clone()]).await;
        let network = Arc::new(Sources {pages:Mutex::new(endpoints.iter().map(|endpoint|(endpoint.clone(),page(&document))).collect()),known:Mutex::new(Vec::new())});
        client.attach_social_network(network.clone());
        for endpoint in &endpoints {
            clock.set_time(clock.now().timestamp_millis()+1000).unwrap();
            let found = search(&client,vec![endpoint.clone()]).await;
            assert_eq!(found["results"][0]["document"]["expires_at"],document.expires_at);
            assert_eq!(found["results"][0]["checked_at"],clock.now().timestamp());
            assert!(found["results"][0]["sources"].as_array().unwrap().len()<=8);
            assert!(!serde_json::to_string(&found).unwrap().contains("untrusted remote origin label"));
        }
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_discovery_source WHERE post=?").bind(&document.id).fetch_one(&client.store.pool).await.unwrap(),8);
        let mut tx = store::write_tx(&client.store.pool).await.unwrap();
        let ids:Vec<String> = (0..8184).map(|value|format!("quota-fixture-{value}")).collect();
        store::sqlx::query("INSERT INTO social_discovery_source(post,hash,source,checked_at) SELECT value,?,'quota fixture',1 FROM json_each(?)")
            .bind("a".repeat(64)).bind(serde_json::to_string(&ids).unwrap()).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let endpoint = iroh::SecretKey::from_bytes(&[235;32]).public().to_string();
        network.pages.lock().unwrap().insert(endpoint.clone(),page(&document));
        assert_eq!(search(&client,vec![endpoint]).await["results"][0]["document"]["expires_at"],document.expires_at);
        assert!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_discovery_source").fetch_one(&client.store.pool).await.unwrap()<=8192);
    })).await;
}

struct Directories {
    hosts: BTreeMap<String, Arc<Engine>>,
    known: Mutex<Vec<Vec<String>>>,
}

#[async_trait::async_trait]
impl Network for Directories {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        if let PublicRequest::Search { known, .. } = &request {
            self.known.lock().unwrap().push(known.clone());
        }
        self.hosts
            .get(destination)
            .unwrap()
            .social_public_request(
                "discovery-client",
                destination,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await
    }
}

#[tokio::test]
async fn hidden_posts_still_refresh_signed_withdrawals_from_actual_directories() {
    let clock = nucleus::execution::Execution::new([236; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let author = Engine::open_memory().await.unwrap();
            let client = Engine::open_memory().await.unwrap();
            let mut hosts = BTreeMap::new();
            for secret in [237, 238] {
                let host = Arc::new(Engine::open_memory().await.unwrap());
                command(
                    &host,
                    Command::ConfigureServices {
                        settings: ServiceSettings {
                            directory: true,
                            ..Default::default()
                        },
                    },
                )
                .await;
                hosts.insert(
                    iroh::SecretKey::from_bytes(&[secret; 32])
                        .public()
                        .to_string(),
                    host,
                );
            }
            let endpoints: Vec<String> = hosts.keys().cloned().collect();
            let publisher = iroh::SecretKey::from_bytes(&[247; 32]).public().to_string();
            let (record, document, signer) = publish(&author, endpoints.clone()).await;
            let changed = variant(&document, &signer);
            for (endpoint, document) in [(&endpoints[0], &document), (&endpoints[1], &changed)] {
                hosts[endpoint]
                    .social_public_request(
                        &publisher,
                        endpoint,
                        PublicRequest::PublishSnippet {
                            document: document.clone(),
                        },
                        clock.now().timestamp(),
                    )
                    .await
                    .unwrap();
            }
            let network = Arc::new(Directories {
                hosts,
                known: Mutex::new(Vec::new()),
            });
            client.attach_social_network(network.clone());
            assert_eq!(
                search(&client, endpoints.clone()).await["conflicts"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            let withdrawal = transition(&author, &record, PostState::Withdrawn).await;
            network.hosts[&endpoints[0]]
                .social_public_request(
                    &publisher,
                    &endpoints[0],
                    PublicRequest::PublishSnippet {
                        document: withdrawal.clone(),
                    },
                    clock.now().timestamp(),
                )
                .await
                .unwrap();
            let ended = search(&client, vec![endpoints[0].clone()]).await;
            assert!(ended["results"].as_array().unwrap().is_empty());
            assert!(ended["conflicts"].as_array().unwrap().is_empty());
            assert!(
                network
                    .known
                    .lock()
                    .unwrap()
                    .last()
                    .unwrap()
                    .iter()
                    .any(|value| value.starts_with(&format!("{}:", document.id)))
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, String>(
                    "SELECT state FROM social_document WHERE kind='snippet' AND id=?"
                )
                .bind(&document.id)
                .fetch_one(&client.store.pool)
                .await
                .unwrap(),
                "withdrawn"
            );
            let stale = search(&client, vec![endpoints[1].clone()]).await;
            assert!(stale["results"].as_array().unwrap().is_empty());
            assert!(stale["conflicts"].as_array().unwrap().is_empty());
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_ended_post WHERE id=?"
                )
                .bind(&document.id)
                .fetch_one(&client.store.pool)
                .await
                .unwrap(),
                1
            );
        }))
        .await;
}
