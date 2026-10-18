use super::*;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

struct NetworkFixture {
    source: String,
    helper: String,
    peers: BTreeMap<String, Arc<Engine>>,
    offline: AtomicBool,
    requests: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl Network for NetworkFixture {
    async fn request(&self, endpoint: &str, request: PublicRequest) -> Result<Value, EngineError> {
        self.requests.lock().unwrap().push(endpoint.into());
        if endpoint == self.helper && self.offline.load(Ordering::SeqCst) {
            return Err(EngineError::Consequence(
                "scripted optional-helper outage".into(),
            ));
        }
        self.peers
            .get(endpoint)
            .unwrap()
            .social_public_frame(
                &self.source,
                endpoint,
                &serde_json::to_vec(&request).unwrap(),
                nucleus::execution::now().timestamp(),
            )
            .await
    }
}

async fn selected(node: &Engine) -> Vec<String> {
    command(node, Command::Overview).await["servers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|choice| choice["query"] == true)
        .map(|choice| choice["endpoint"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn institute_preset_is_disabled_pins_recognized_identity_stays_optional_and_removal_survives_restart()
 {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("helper.sqlite").display()
    );
    let client = Engine::open(&url).await.unwrap();
    let mut peers = BTreeMap::new();
    let mut endpoints = Vec::new();
    for seed in [183, 184, 185] {
        let node = Arc::new(Engine::open_memory().await.unwrap());
        command(
            &node,
            Command::ConfigureServices {
                settings: ServiceSettings {
                    directory: true,
                    ..Default::default()
                },
            },
        )
        .await;
        let endpoint = iroh::SecretKey::from_bytes(&[seed; 32])
            .public()
            .to_string();
        endpoints.push(endpoint.clone());
        peers.insert(endpoint, node);
    }
    let helper_endpoint = endpoints[2].clone();
    let helper = peers[&helper_endpoint].clone();
    let network = Arc::new(NetworkFixture {
        source: iroh::SecretKey::from_bytes(&[186; 32]).public().to_string(),
        helper: helper_endpoint.clone(),
        peers,
        offline: AtomicBool::new(false),
        requests: Mutex::new(Vec::new()),
    });
    client.attach_social_network(network.clone());
    for endpoint in &endpoints[..2] {
        command(
            &client,
            Command::SaveServer {
                choice: ServerChoice {
                    endpoint: endpoint.clone(),
                    query: true,
                    ..Default::default()
                },
            },
        )
        .await;
    }
    command(
        &client,
        Command::Search {
            query: Default::default(),
            services: selected(&client).await,
        },
    )
    .await;
    assert_eq!(network.requests.lock().unwrap().len(), 2);
    let mut preset = ServerChoice::institute_helper();
    assert!(!preset.query && !preset.publication && !preset.mailbox);
    assert!(preset.endpoint.is_empty() && preset.organ.is_none());
    preset.endpoint = helper_endpoint.clone();
    command(
        &client,
        Command::SaveServer {
            choice: preset.clone(),
        },
    )
    .await;
    network.requests.lock().unwrap().clear();
    command(
        &client,
        Command::Search {
            query: Default::default(),
            services: vec![helper_endpoint.clone()],
        },
    )
    .await;
    assert!(network.requests.lock().unwrap().is_empty());
    preset.query = true;
    assert!(
        client
            .social_command(
                Command::SaveServer {
                    choice: preset.clone()
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    let organ = store::organs::local(&helper.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::organs::add_contact(&client.store.pool, &organ, None, "Chosen helper", "", 1)
        .await
        .unwrap();
    store::organs::set_trust(&client.store.pool, &organ, "known")
        .await
        .unwrap();
    store::organs::set_node_id(&client.store.pool, &organ, Some(&helper_endpoint))
        .await
        .unwrap();
    let root = engine::trust::Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    helper.publish_root_key(&root).await.unwrap();
    client.publish_root_key(&root).await.unwrap();
    let cell = store::cells::local(&helper.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let signer = engine::trust::Signer::generate(&organ, &engine::roster::cell_key_id(&cell));
    helper.set_organ_signer(signer.clone()).await.unwrap();
    let roster = helper
        .publish_roster(
            &root,
            vec![engine::roster::CellEntry {
                cell_uid: cell,
                node_id: helper_endpoint.clone(),
                label: "Chosen helper representative".into(),
                operational_key: signer.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
    client.adopt_roster(&roster).await.unwrap();
    preset.organ = Some(organ.clone());
    let mut wrong = preset.clone();
    wrong.endpoint = endpoints[0].clone();
    assert!(
        client
            .social_command(
                Command::SaveServer { choice: wrong },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    command(&client, Command::SaveServer { choice: preset }).await;
    network.requests.lock().unwrap().clear();
    command(
        &client,
        Command::Search {
            query: Default::default(),
            services: selected(&client).await,
        },
    )
    .await;
    assert_eq!(network.requests.lock().unwrap().len(), 3);
    network.offline.store(true, Ordering::SeqCst);
    let offline = command(
        &client,
        Command::Search {
            query: Default::default(),
            services: selected(&client).await,
        },
    )
    .await;
    assert!(!offline["failures"].as_array().unwrap().is_empty());
    command(
        &client,
        Command::RemoveServer {
            endpoint: helper_endpoint.clone(),
        },
    )
    .await;
    client.store.pool.close().await;
    drop(client);
    let restarted = Engine::open(&url).await.unwrap();
    restarted.attach_social_network(network.clone());
    network.requests.lock().unwrap().clear();
    let choices = selected(&restarted).await;
    assert_eq!(choices.len(), 2);
    command(
        &restarted,
        Command::Search {
            query: Default::default(),
            services: choices,
        },
    )
    .await;
    assert!(
        network
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|endpoint| endpoint != &helper_endpoint)
    );
    let policies: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM trust_query_record")
        .fetch_one(&restarted.store.pool)
        .await
        .unwrap();
    assert_eq!(policies, 0);
    assert!(!restarted.social_settings().await.unwrap().directory);
}
