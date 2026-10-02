use engine::{Engine, EngineError, social::Network};
use nucleus::social::{
    Command, PostDraft, PostState, PublicRequest, ServerChoice, ServiceSettings,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

struct DescriptorHost {
    engine: Arc<Engine>,
    change: Mutex<Option<(&'static str, Value)>>,
}

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .social_command(request, None, nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap()
}

#[tokio::test]
async fn remembered_servers_keep_roles_separate_and_bound_atomic_edits_without_network_side_effects()
 {
    let engine = Engine::open_memory().await.unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[161; 32]).public().to_string();
    let choice = ServerChoice {
        endpoint: endpoint.clone(),
        label: "Independent mailbox".into(),
        operator: "Operator A".into(),
        mailbox: true,
        ..Default::default()
    };
    let result = command(
        &engine,
        Command::SaveServer {
            choice: choice.clone(),
        },
    )
    .await;
    assert_eq!(result["servers"][0]["mailbox"], true);
    assert_eq!(result["servers"][0]["query"], false);
    assert_eq!(result["servers"][0]["publication"], false);
    let overview = command(&engine, Command::Overview).await;
    assert_eq!(overview["servers"], result["servers"]);
    assert!(!engine.social_settings().await.unwrap().mailbox);
    let mut changed = choice.clone();
    changed.query = true;
    let edited = command(&engine, Command::SaveServer { choice: changed }).await;
    assert_eq!(edited["servers"].as_array().unwrap().len(), 1);
    assert_eq!(edited["servers"][0]["query"], true);
    let mut invalid = choice.clone();
    invalid.label = "x".repeat(81);
    assert!(
        engine
            .social_command(
                Command::SaveServer { choice: invalid },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    assert_eq!(
        command(&engine, Command::Overview).await["servers"],
        edited["servers"]
    );
    for id in 162..169 {
        command(
            &engine,
            Command::SaveServer {
                choice: ServerChoice {
                    endpoint: iroh::SecretKey::from_bytes(&[id; 32]).public().to_string(),
                    mailbox: true,
                    ..Default::default()
                },
            },
        )
        .await;
    }
    assert!(
        engine
            .social_command(
                Command::SaveServer {
                    choice: ServerChoice {
                        endpoint: iroh::SecretKey::from_bytes(&[169; 32]).public().to_string(),
                        mailbox: true,
                        ..Default::default()
                    }
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    for id in 169..177 {
        command(
            &engine,
            Command::SaveServer {
                choice: ServerChoice {
                    endpoint: iroh::SecretKey::from_bytes(&[id; 32]).public().to_string(),
                    ..Default::default()
                },
            },
        )
        .await;
    }
    assert_eq!(
        command(&engine, Command::Overview).await["servers"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    assert!(
        engine
            .social_command(
                Command::SaveServer {
                    choice: ServerChoice {
                        endpoint: iroh::SecretKey::from_bytes(&[177; 32]).public().to_string(),
                        ..Default::default()
                    }
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    let removed = command(&engine, Command::RemoveServer { endpoint }).await;
    assert_eq!(removed["servers"].as_array().unwrap().len(), 15);
    let jobs: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_publication_job")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
    assert!(nucleus::social::private_sync_field(
        "record_extension",
        "lince.social.servers"
    ));
}

struct SearchHost(Mutex<Value>);

#[async_trait::async_trait]
impl Network for SearchHost {
    async fn request(&self, _: &str, _: PublicRequest) -> Result<Value, EngineError> {
        Ok(self.0.lock().unwrap().clone())
    }
}

#[tokio::test]
async fn discovery_rejects_conflicting_hashes_and_malformed_controls_and_labels_partial_refresh() {
    let engine = Engine::open_memory().await.unwrap();
    let draft = command(
        &engine,
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
    let preview = command(
        &engine,
        Command::Preview {
            record: draft["record"].as_str().unwrap().into(),
            state: PostState::Active,
        },
    )
    .await;
    let row = json!({"document":preview["document"],"hash":preview["preview_hash"]});
    let endpoint = iroh::SecretKey::from_bytes(&[181; 32]).public().to_string();
    let transport = Arc::new(SearchHost(Mutex::new(
        json!({"results":[row],"updates":[]}),
    )));
    let network: Arc<dyn Network> = transport.clone();
    engine.attach_social_network(network);
    let search = || Command::Search {
        query: Default::default(),
        services: vec![endpoint.clone()],
    };
    transport.0.lock().unwrap()["results"][0]["hash"] = json!("0".repeat(64));
    let rejected = command(&engine, search()).await;
    assert_eq!(rejected["failures"].as_array().unwrap().len(), 1);
    assert!(rejected["results"].as_array().unwrap().is_empty());
    for key in ["authorities", "posting_authorities", "updates"] {
        *transport.0.lock().unwrap() = json!({"results":[row],"updates":[]});
        transport.0.lock().unwrap()[key] = json!({"malformed":true});
        assert_eq!(
            command(&engine, search()).await["failures"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    *transport.0.lock().unwrap() = json!({"results":[row],"updates":[],"refresh_incomplete":true});
    let partial = command(&engine, search()).await;
    assert_eq!(partial["results"].as_array().unwrap().len(), 1);
    assert!(
        partial["failures"][0]["error"]
            .as_str()
            .unwrap()
            .contains("partial page")
    );
}

#[async_trait::async_trait]
impl Network for DescriptorHost {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let mut result = self
            .engine
            .social_public_request(
                destination,
                destination,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await?;
        if let Some((key, value)) = &*self.change.lock().unwrap() {
            result["descriptor"][*key] = value.clone();
        }
        Ok(result)
    }
}

#[tokio::test]
async fn inspection_verifies_the_pinned_endpoint_and_effective_roles_without_enabling_any_service()
{
    let host = Arc::new(Engine::open_memory().await.unwrap());
    let client = Engine::open_memory().await.unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[142; 32]).public().to_string();
    let transport = Arc::new(DescriptorHost {
        engine: host.clone(),
        change: Mutex::new(None),
    });
    let network: Arc<dyn Network> = transport.clone();
    client.attach_social_network(network);
    let command = || Command::InspectService {
        endpoint: endpoint.clone(),
    };
    let first = client
        .social_command(command(), None, nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(first["descriptor"]["endpoint"], endpoint);
    assert_eq!(first["descriptor"]["roles"], json!([]));
    assert!(!host.social_settings().await.unwrap().mailbox);
    assert!(!client.social_settings().await.unwrap().directory);
    host.social_command(
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                mailbox: true,
                relay: true,
                gossip: true,
                contact: "operator@example.test".into(),
                policy: "Independent voluntary service".into(),
                ..Default::default()
            },
        },
        None,
        nucleus::execution::now(),
    )
    .await
    .unwrap();
    let active = client
        .social_command(command(), None, nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(
        active["descriptor"]["roles"],
        json!(["directory", "mailbox", "reports"])
    );
    assert_eq!(active["descriptor"]["settings"]["relay"], false);
    assert_eq!(active["descriptor"]["settings"]["gossip"], false);
    assert_eq!(
        active["descriptor"]["settings"]["contact"],
        "operator@example.test"
    );
    assert_eq!(active["descriptor"]["mailbox_retention_days"], 30);
    for (key, value) in [
        (
            "endpoint",
            json!(iroh::SecretKey::from_bytes(&[143; 32]).public().to_string()),
        ),
        ("protocol", json!("lince.social-service.2")),
        ("roles", json!(["directory", "mailbox", "relay"])),
        (
            "expires_at",
            json!(nucleus::execution::now().timestamp() - 1),
        ),
        ("frame_bytes", json!(u32::MAX)),
        ("mailbox_retention_days", json!(1000)),
    ] {
        *transport.change.lock().unwrap() = Some((key, value));
        assert!(
            client
                .social_command(command(), None, nucleus::execution::now())
                .await
                .is_err()
        );
    }
    let jobs: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_publication_job")
        .fetch_one(&client.store.pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
    let invalid = ServiceSettings {
        policy: "x".repeat(1001),
        ..Default::default()
    };
    assert!(
        host.social_command(
            Command::ConfigureServices { settings: invalid },
            None,
            nucleus::execution::now()
        )
        .await
        .is_err()
    );
    assert!(host.social_settings().await.unwrap().directory);
}
