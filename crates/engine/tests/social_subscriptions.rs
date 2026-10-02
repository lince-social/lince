use chrono::Timelike;
use engine::{Engine, EngineError, social::Network};
use nucleus::social::{
    Command, PostDraft, PostState, PublicRequest, ServiceSettings, Snippet,
    subscriptions::Subscription,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .social_command(request, None, nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn publish(engine: &Engine, endpoint: &str) -> (String, Snippet) {
    let saved = command(
        engine,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Bicycle help".into(),
                text: "A small public contribution".into(),
                destinations: vec![endpoint.into()],
                ..Default::default()
            },
        },
    )
    .await;
    let record = saved["record"].as_str().unwrap().to_owned();
    let preview = command(
        engine,
        Command::Preview {
            record: record.clone(),
            state: PostState::Active,
        },
    )
    .await;
    let document: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    command(
        engine,
        Command::Publish {
            record: record.clone(),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: document.clone(),
        },
    )
    .await;
    (record, document)
}

async fn save(engine: &Engine, mut filter: Subscription) -> Subscription {
    if filter.id.is_empty() {
        filter.id = nucleus::new_uid("sub");
    }
    command(
        engine,
        Command::SaveSubscription {
            filter: Box::new(filter.clone()),
        },
    )
    .await;
    filter
}

fn filter() -> Subscription {
    Subscription {
        label: "Private bicycle search".into(),
        query: nucleus::social::Search {
            text: "bicycle".into(),
            ..Default::default()
        },
        enabled: true,
        notifications: true,
        quiet_start_hour: 0,
        quiet_end_hour: 0,
        ..Default::default()
    }
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

#[tokio::test]
async fn enrolled_devices_sync_private_filters_and_tombstones_without_enabling_fresh_device_queries()
 {
    use engine::{
        pairing::EnrolmentInvite,
        roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
        trust::Signer,
    };
    let owner = Engine::open_memory().await.unwrap();
    let second = Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.key");
    std::fs::write(&path, [156; 32]).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    owner.set_root_key_path(path);
    owner.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [156; 32]);
    let mut cells = Vec::new();
    let mut keys = Vec::new();
    for (engine, label) in [(&owner, "Owner"), (&second, "New device")] {
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let key = engine.operational_key_for(&organ).await.unwrap();
        cells.push(CellEntry {
            cell_uid: cell,
            node_id: label.into(),
            label: label.into(),
            operational_key: key.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: full_capabilities(),
        });
        keys.push(key);
    }
    owner.set_organ_signer(keys[0].clone()).await.unwrap();
    owner.set_signer(keys[0].clone()).await.unwrap();
    owner.publish_root_key(&root).await.unwrap();
    let roster = owner.publish_roster(&root, cells).await.unwrap();
    second
        .join_organ(
            &EnrolmentInvite {
                node_id: "Owner".into(),
                organ_uid: organ.clone(),
                root_key: root.public_key_b64(),
                token: "saved-searches".into(),
                addrs: vec![],
            },
            &roster,
            keys[1].clone(),
        )
        .await
        .unwrap();
    second.set_signer(keys[1].clone()).await.unwrap();
    let saved = save(&owner, filter()).await;
    command(&owner, Command::ConfigureSubscriptions { enabled: true }).await;
    copy(&owner, &second, &organ).await;
    let view = command(&second, Command::Subscriptions { after: None }).await;
    assert_eq!(view["saved_searches"][0]["filter"]["id"], saved.id);
    assert_eq!(view["device_enabled"], false);
    assert!(
        second
            .social_subscriptions_once(true)
            .await
            .unwrap()
            .is_empty()
    );
    let contact = store::organs::add_contact(
        &owner.store.pool,
        &nucleus::new_uid("r"),
        None,
        "Foreign contact",
        "",
        1,
    )
    .await
    .unwrap();
    store::organs::set_trust(&owner.store.pool, &contact, "known")
        .await
        .unwrap();
    let foreign = owner.export_sync_page(&contact, &[], 2000).await.unwrap();
    let bytes = serde_json::to_string(&foreign.batch).unwrap();
    assert!(!bytes.contains("lince.social.saved-searches"));
    assert!(!bytes.contains("lince.social.saved-search-device"));
    assert!(!bytes.contains("Private bicycle search"));
    command(&second, Command::RemoveSubscription { id: saved.id }).await;
    copy(&second, &owner, &organ).await;
    assert!(
        command(&owner, Command::Subscriptions { after: None }).await["saved_searches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

enum Change {
    DeviceOff,
    Remove(String),
    Revoke { role: i64, permission: i64 },
}

struct SearchNetwork {
    host: Arc<Engine>,
    client: Arc<Engine>,
    endpoint: String,
    source: String,
    calls: Arc<Mutex<Vec<String>>>,
    change: Mutex<Option<Change>>,
}

#[async_trait::async_trait]
impl Network for SearchNetwork {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        assert_eq!(destination, self.endpoint);
        self.calls
            .lock()
            .unwrap()
            .push(serde_json::to_string(&request).unwrap());
        let response = self
            .host
            .social_public_request(
                &self.source,
                &self.endpoint,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await?;
        let change = self.change.lock().unwrap().take();
        match change {
            Some(Change::DeviceOff) => {
                command(
                    &self.client,
                    Command::ConfigureSubscriptions { enabled: false },
                )
                .await;
            }
            Some(Change::Remove(id)) => {
                command(&self.client, Command::RemoveSubscription { id }).await;
            }
            Some(Change::Revoke { role, permission }) => {
                store::auth::revoke(&self.client.store.pool, role, permission)
                    .await
                    .unwrap();
            }
            None => {}
        }
        Ok(response)
    }
}

async fn network(
    client: Arc<Engine>,
    change: Option<Change>,
) -> (Arc<SearchNetwork>, String, Snippet) {
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
    let endpoint = iroh::SecretKey::from_bytes(&[151; 32]).public().to_string();
    let (record, document) = publish(&host, &endpoint).await;
    let network = Arc::new(SearchNetwork {
        host,
        client: client.clone(),
        endpoint,
        source: iroh::SecretKey::from_bytes(&[152; 32]).public().to_string(),
        calls: Arc::new(Mutex::new(Vec::new())),
        change: Mutex::new(change),
    });
    client.attach_social_network(network.clone());
    (network, record, document)
}

#[tokio::test]
async fn saved_filters_default_off_and_cached_notices_deduplicate_across_clearing_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("saved.sqlite").display()
    );
    let engine = Engine::open(&url).await.unwrap();
    engine.set_sealing_keyring_path(directory.path().join("keys/sealing.json"));
    let endpoint = iroh::SecretKey::from_bytes(&[153; 32]).public().to_string();
    let (_, document) = publish(&engine, &endpoint).await;
    let saved = save(&engine, filter()).await;
    assert_eq!(
        command(&engine, Command::Subscriptions { after: None }).await["device_enabled"],
        false
    );
    assert!(
        engine
            .social_subscriptions_once(false)
            .await
            .unwrap()
            .is_empty()
    );
    let seen: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_subscription_seen")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(seen, 0);
    command(&engine, Command::ConfigureSubscriptions { enabled: true }).await;
    assert_eq!(
        engine.social_subscriptions_once(false).await.unwrap().len(),
        1
    );
    let matches = command(
        &engine,
        Command::SubscriptionResults {
            id: saved.id.clone(),
        },
    )
    .await;
    assert_eq!(matches["results"].as_array().unwrap().len(), 1);
    assert_eq!(matches["results"][0]["document"]["id"], document.id);
    assert!(matches["source"].as_str().unwrap().contains("local cache"));
    command(
        &engine,
        Command::ClearSubscriptionMatches {
            id: saved.id.clone(),
        },
    )
    .await;
    assert!(
        command(
            &engine,
            Command::SubscriptionResults {
                id: saved.id.clone()
            }
        )
        .await["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    assert!(
        engine
            .social_subscriptions_once(false)
            .await
            .unwrap()
            .is_empty()
    );
    let seen: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_subscription_seen")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(seen, 1);
    store::sqlx::query("UPDATE social_subscription_job SET next_attempt=0")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        engine
            .social_subscriptions_once(false)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        command(&engine, Command::SubscriptionResults { id: saved.id }).await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let private: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM record WHERE kind IN ('message','transfer')",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(private, 0);
}

#[tokio::test]
async fn quiet_hours_defer_notices_and_hourly_searches_do_not_repeat_after_reconnect() {
    let start = nucleus::execution::now().timestamp();
    let clock = nucleus::execution::Execution::new([150; 32], start * 1000).unwrap();
    clock
        .scope(async {
            let client = Arc::new(Engine::open_memory().await.unwrap());
            let (network, record, _) = network(client.clone(), None).await;
            let mut choice = filter();
            choice.services = vec![network.endpoint.clone()];
            let hour = nucleus::execution::now()
                .with_timezone(&chrono::Local)
                .hour();
            choice.quiet_start_hour = hour;
            choice.quiet_end_hour = (hour + 1) % 24;
            let saved = save(&client, choice).await;
            command(&client, Command::ConfigureSubscriptions { enabled: true }).await;
            assert!(
                client
                    .social_subscriptions_once(false)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert!(network.calls.lock().unwrap().is_empty());
            assert!(
                client
                    .social_subscriptions_once(true)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(network.calls.lock().unwrap().len(), 1);
            assert!(
                client
                    .social_subscriptions_once(true)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(network.calls.lock().unwrap().len(), 1);
            assert_eq!(
                command(&client, Command::SubscriptionResults { id: saved.id }).await["results"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            clock.set_time((start + 3600) * 1000).unwrap();
            assert_eq!(
                client.social_subscriptions_once(true).await.unwrap().len(),
                1
            );
            assert_eq!(network.calls.lock().unwrap().len(), 2);
            assert!(
                client
                    .social_subscriptions_once(true)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(network.calls.lock().unwrap().len(), 2);
            let calls = network.calls.lock().unwrap().join("\n");
            assert!(!calls.contains(&record));
            for engine in [&client, &network.host] {
                assert!(
                    !calls.contains(
                        &store::organs::local(&engine.store.pool)
                            .await
                            .unwrap()
                            .unwrap()
                            .uid
                    )
                );
            }
        })
        .await;
}

#[tokio::test]
async fn opt_out_removal_and_actor_revocation_during_a_query_prevent_cache_and_match_commit() {
    for mode in 0..3 {
        let client = Arc::new(Engine::open_memory().await.unwrap());
        let id = nucleus::new_uid("sub");
        let mut choice = filter();
        choice.id = id.clone();
        let mut actor = None;
        let change = match mode {
            0 => Change::DeviceOff,
            1 => Change::Remove(id.clone()),
            _ => {
                let role = store::auth::ensure_role(&client.store.pool, "saved-search-editor")
                    .await
                    .unwrap();
                let permission =
                    store::auth::ensure_permission(&client.store.pool, "organ", "update")
                        .await
                        .unwrap();
                store::auth::grant(&client.store.pool, role, permission)
                    .await
                    .unwrap();
                actor = Some(
                    store::auth::create_person_login(
                        &client.store.pool,
                        "Search editor",
                        "editor",
                        "hash",
                        role,
                    )
                    .await
                    .unwrap(),
                );
                Change::Revoke { role, permission }
            }
        };
        let (network, _, document) = network(client.clone(), Some(change)).await;
        choice.services = vec![network.endpoint.clone()];
        client
            .social_command(
                Command::SaveSubscription {
                    filter: Box::new(choice),
                },
                actor.as_deref(),
                nucleus::execution::now(),
            )
            .await
            .unwrap();
        command(&client, Command::ConfigureSubscriptions { enabled: true }).await;
        assert!(
            client
                .social_subscriptions_once(true)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(network.calls.lock().unwrap().len(), 1);
        let cached: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM social_document WHERE kind='snippet' AND id=?",
        )
        .bind(document.id)
        .fetch_one(&client.store.pool)
        .await
        .unwrap();
        let matched: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM social_subscription_seen WHERE subscription=?",
        )
        .bind(id)
        .fetch_one(&client.store.pool)
        .await
        .unwrap();
        assert_eq!(cached, 0);
        assert_eq!(matched, 0);
    }
}

#[tokio::test]
async fn saved_filter_limits_and_seen_exhaustion_preserve_existing_tracking_and_allow_resolution() {
    let engine = Engine::open_memory().await.unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[154; 32]).public().to_string();
    publish(&engine, &endpoint).await;
    let first = save(&engine, filter()).await;
    let now = nucleus::execution::now().timestamp();
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    for index in 0..256 {
        store::sqlx::query("INSERT INTO social_subscription_seen(id,subscription,post,hash,expires_at,matched_at,notified) VALUES(?,?,?,'old hash',?,?,1)")
            .bind(format!("{index:064x}")).bind(&first.id).bind(nucleus::new_uid("post")).bind(now+86400).bind(now).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    command(&engine, Command::ConfigureSubscriptions { enabled: true }).await;
    assert!(
        engine
            .social_subscriptions_once(false)
            .await
            .unwrap()
            .is_empty()
    );
    let snapshot = command(&engine, Command::Subscriptions { after: None }).await;
    assert!(
        snapshot["saved_searches"][0]["runtime"]["error"]
            .as_str()
            .unwrap()
            .contains("full")
    );
    let retained: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM social_subscription_seen WHERE subscription=?",
    )
    .bind(&first.id)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(retained, 256);
    for _ in 0..7 {
        save(&engine, filter()).await;
    }
    assert!(
        engine
            .social_command(
                Command::SaveSubscription {
                    filter: Box::new(filter())
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    let mut invalid = filter();
    invalid.interval_minutes = 59;
    assert!(
        engine
            .social_command(
                Command::SaveSubscription {
                    filter: Box::new(invalid)
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut map =
        store::records::get_extension(&engine.store.pool, &organ, "lince.social.saved-searches")
            .await
            .unwrap()
            .unwrap();
    let mut merged = filter();
    merged.id = nucleus::new_uid("sub");
    map[format!("filter_{}", merged.id)] = serde_json::to_value(&merged).unwrap();
    store::records::set_extension(
        &engine.store.pool,
        &organ,
        "lince.social.saved-searches",
        &map,
    )
    .await
    .unwrap();
    let page = command(&engine, Command::Subscriptions { after: None }).await;
    assert_eq!(page["over_limit"], true);
    assert_eq!(page["saved_searches"].as_array().unwrap().len(), 8);
    assert!(engine.social_subscriptions_once(false).await.is_err());
    command(&engine, Command::RemoveSubscription { id: merged.id }).await;
    assert_eq!(
        command(&engine, Command::Subscriptions { after: None }).await["over_limit"],
        false
    );
    let search = command(
        &engine,
        Command::Search {
            query: Default::default(),
            services: vec![],
        },
    )
    .await;
    assert_eq!(search["results"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn current_mutes_and_withdrawals_filter_retained_subscription_matches_and_pending_notices() {
    let engine = Engine::open_memory().await.unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[155; 32]).public().to_string();
    let (record, document) = publish(&engine, &endpoint).await;
    let mut choice = filter();
    let hour = nucleus::execution::now()
        .with_timezone(&chrono::Local)
        .hour();
    choice.quiet_start_hour = hour;
    choice.quiet_end_hour = (hour + 1) % 24;
    let saved = save(&engine, choice).await;
    command(&engine, Command::ConfigureSubscriptions { enabled: true }).await;
    assert!(
        engine
            .social_subscriptions_once(false)
            .await
            .unwrap()
            .is_empty()
    );
    command(
        &engine,
        Command::MutePost {
            post: document.id.clone(),
            whole_author: false,
        },
    )
    .await;
    assert!(
        command(
            &engine,
            Command::SubscriptionResults {
                id: saved.id.clone()
            }
        )
        .await["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mutes = command(&engine, Command::Mutes { after: None }).await;
    let key = mutes["mutes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    command(&engine, Command::Unmute { key }).await;
    assert_eq!(
        command(
            &engine,
            Command::SubscriptionResults {
                id: saved.id.clone()
            }
        )
        .await["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let preview = command(
        &engine,
        Command::Preview {
            record: record.clone(),
            state: PostState::Withdrawn,
        },
    )
    .await;
    command(
        &engine,
        Command::Publish {
            record,
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: serde_json::from_value(preview["document"].clone()).unwrap(),
        },
    )
    .await;
    assert!(
        command(&engine, Command::SubscriptionResults { id: saved.id }).await["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
