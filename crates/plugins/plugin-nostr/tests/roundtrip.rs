use engine::Engine;
use futures::{SinkExt, StreamExt};
use nostr::prelude::{Event, EventBuilder, FinalizeEvent, Keys, Kind, Timestamp};
use nucleus::{
    RecordKind,
    plugins::{Binding, Command, Direction},
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::{net::TcpListener, sync::Mutex};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct Relay {
    url: String,
    events: Arc<Mutex<Vec<Value>>>,
    publications: Arc<AtomicUsize>,
    reject: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Relay {
    async fn start(events: Vec<Value>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let events = Arc::new(Mutex::new(events));
        let publications = Arc::new(AtomicUsize::new(0));
        let reject = Arc::new(AtomicBool::new(false));
        let data = events.clone();
        let pushed = publications.clone();
        let rejected = reject.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let data = data.clone();
                let pushed = pushed.clone();
                let rejected = rejected.clone();
                tokio::spawn(async move {
                    let mut socket = accept_async(stream).await.unwrap();
                    while let Some(Ok(Message::Text(message))) = socket.next().await {
                        let value: Value = serde_json::from_str(&message).unwrap();
                        match value[0].as_str() {
                            Some("REQ") => {
                                let records = data.lock().await.clone();
                                for record in records {
                                    if value[2]["since"].as_u64().is_some_and(|since| {
                                        record["created_at"].as_u64().is_some_and(|at| at < since)
                                    }) {
                                        continue;
                                    }
                                    if let Some(authors) = value[2]["authors"].as_array() {
                                        if !authors.contains(&record["pubkey"]) {
                                            continue;
                                        }
                                    }
                                    if socket
                                        .send(Message::Text(
                                            json!(["EVENT", value[1], record]).to_string().into(),
                                        ))
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                let _ = socket
                                    .send(Message::Text(
                                        json!(["EOSE", value[1]]).to_string().into(),
                                    ))
                                    .await;
                            }
                            Some("EVENT") => {
                                let event: Event =
                                    serde_json::from_value(value[1].clone()).unwrap();
                                event.verify().unwrap();
                                pushed.fetch_add(1, Ordering::SeqCst);
                                let accepted = !rejected.load(Ordering::SeqCst);
                                if accepted {
                                    let mut records = data.lock().await;
                                    if !records.iter().any(|record| record["id"] == value[1]["id"])
                                    {
                                        records.push(value[1].clone());
                                    }
                                }
                                let _ = socket
                                    .send(Message::Text(
                                        json!([
                                            "OK",
                                            value[1]["id"],
                                            accepted,
                                            if accepted {
                                                "saved"
                                            } else {
                                                "blocked: test rejection"
                                            }
                                        ])
                                        .to_string()
                                        .into(),
                                    ))
                                    .await;
                            }
                            Some("CLOSE") => return,
                            _ => panic!("Unexpected Nostr message"),
                        }
                    }
                });
            }
        });
        Self {
            url,
            events,
            publications,
            reject,
            task,
        }
    }
}

fn note(body: &str) -> Value {
    serde_json::to_value(
        EventBuilder::new(Kind::TextNote, body)
            .finalize(&Keys::generate())
            .unwrap(),
    )
    .unwrap()
}

async fn install(directory: &std::path::Path) {
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_lince-plugin-nostr"))
        .arg("--install")
        .arg(directory)
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

async fn cell(directory: &std::path::Path) -> Engine {
    let engine = Engine::open_memory().await.unwrap();
    engine
        .load_plugins(directory.join("plugins"))
        .await
        .unwrap();
    engine
}

async fn enable(engine: &Engine) {
    let package = engine.plugin_catalog().await.unwrap().packages.remove(0);
    engine
        .act(
            engine::actions::Action::Plugins {
                request: Command::Enable {
                    plugin: package.manifest.id,
                    digest: package.digest,
                    enabled: true,
                },
            },
            None,
        )
        .await
        .unwrap();
}

fn binding(relay: &Relay, direction: Direction, protein_uid: Option<String>) -> Binding {
    Binding {
        uid: nucleus::new_uid("r"),
        provider: "example.nostr/notes".into(),
        name: "Local relay".into(),
        settings: json!({"relays":relay.url,"authors":"","limit":20}),
        protein_uid,
        incoming_filter: json!([]),
        direction,
        interval_seconds: 60,
        enabled: true,
        revision: 0,
    }
}

async fn save(engine: &Engine, binding: Binding) {
    let preview = if binding.direction.outgoing() {
        let preview = engine
            .plugin_command(
                Command::Preview {
                    binding: binding.clone(),
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        Some(preview["preview"].as_str().unwrap().to_owned())
    } else {
        None
    };
    engine
        .plugin_command(Command::SaveBinding { binding, preview }, None)
        .await
        .unwrap();
}

async fn record(engine: &Engine, kind: RecordKind, head: &str, body: &str) -> String {
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head,
            body,
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn protein(engine: &Engine, uid: &str) -> String {
    let protein = record(engine, RecordKind::Protein, "Publication selection", "").await;
    store::records::set_extension(&engine.store.pool,&protein,"lince.protein",&json!({"source":"record","where":[{"all":[{"uid_eq":uid}]}],"fields":["uid","head","body"]})).await.unwrap();
    protein
}

#[tokio::test]
async fn incoming_notes_are_verified_filtered_retained_and_deduplicated() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path()).await;
    let event = note("Rust community news");
    let old = serde_json::to_value(
        EventBuilder::new(Kind::TextNote, "Other topic")
            .custom_created_at(Timestamp::from(Timestamp::now().as_secs() - 3600))
            .finalize(&Keys::generate())
            .unwrap(),
    )
    .unwrap();
    let relay = Relay::start(vec![event.clone(), event, old]).await;
    let engine = cell(directory.path()).await;
    assert_eq!(
        engine.plugin_catalog().await.unwrap().packages[0]
            .manifest
            .contributions
            .len(),
        3
    );
    let mut binding = binding(&relay, Direction::Incoming, None);
    binding.incoming_filter = json!([{"text_contains":"rust"}]);
    assert!(
        engine
            .plugin_command(
                Command::SaveBinding {
                    binding: binding.clone(),
                    preview: None
                },
                None
            )
            .await
            .is_err()
    );
    enable(&engine).await;
    save(&engine, binding.clone()).await;
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 1);
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 0);
    let records = store::records::list_all(&engine.store.pool).await.unwrap();
    let imported = records
        .iter()
        .find(|row| row.body == "Rust community news")
        .unwrap();
    assert_eq!(imported.kind, "plain");
    let source = store::records::get_extension(&engine.store.pool, &imported.uid, "lince.source")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source["plugin"], "example.nostr");
    let metadata = store::records::get_extension(
        &engine.store.pool,
        &imported.uid,
        "plugin.example.nostr.note",
    )
    .await
    .unwrap()
    .unwrap();
    assert!(metadata["signed_event"]["sig"].is_string());
    assert!(records.iter().all(|row| row.body != "Other topic"));
    let mut changed = engine
        .plugin_catalog()
        .await
        .unwrap()
        .bindings
        .remove(0)
        .binding;
    changed.incoming_filter = json!([{"text_contains":"other"}]);
    save(&engine, changed).await;
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 1);
    assert!(
        store::records::list_all(&engine.store.pool)
            .await
            .unwrap()
            .iter()
            .any(|row| row.body == "Other topic")
    );
    store::records::mark_deleted(&engine.store.pool, &imported.uid)
        .await
        .unwrap();
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 0);
    engine
        .plugin_command(
            Command::Stop {
                binding: binding.uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(engine.run_plugin_binding(&binding.uid).await.is_err());
    assert!(!engine.sync_overview(None).await.unwrap().history.is_empty());
}

#[tokio::test]
async fn invalid_signed_events_roll_back_the_entire_import() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path()).await;
    let mut invalid = note("Signed original");
    invalid["content"] = json!("Forged text");
    let relay = Relay::start(vec![note("Valid note"), invalid]).await;
    let engine = cell(directory.path()).await;
    enable(&engine).await;
    let binding = binding(&relay, Direction::Incoming, None);
    save(&engine, binding.clone()).await;
    assert!(
        engine
            .run_plugin_binding(&binding.uid)
            .await
            .unwrap_err()
            .to_string()
            .contains("signature")
    );
    assert!(
        store::records::list_all(&engine.store.pool)
            .await
            .unwrap()
            .iter()
            .all(|row| row.body != "Valid note" && row.body != "Forged text")
    );
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM plugin_object")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn publication_requires_a_current_preview_and_reuses_a_signed_event_on_retry() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path()).await;
    let relay = Relay::start(Vec::new()).await;
    relay.reject.store(true, Ordering::SeqCst);
    let engine = cell(directory.path()).await;
    enable(&engine).await;
    let identity = engine
        .plugin_command(
            Command::Invoke {
                contribution: "example.nostr/identity".into(),
                arguments: json!({}),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(identity["result"]["public_key"].as_str().unwrap().len(), 64);
    assert!(!identity.to_string().contains("secret"));
    let uid = record(&engine, RecordKind::Plain, "My note", "Hello from Lince").await;
    let excluded = record(&engine, RecordKind::Plain, "Private note", "Do not publish").await;
    let selection = protein(&engine, &uid).await;
    let binding = binding(&relay, Direction::Both, Some(selection));
    assert!(
        engine
            .plugin_command(
                Command::SaveBinding {
                    binding: binding.clone(),
                    preview: None
                },
                None
            )
            .await
            .is_err()
    );
    let preview = engine
        .plugin_command(
            Command::Preview {
                binding: binding.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(preview["count"], 1);
    store::records::set_text(&engine.store.pool, &uid, None, Some("Updated note"))
        .await
        .unwrap();
    assert!(
        engine
            .plugin_command(
                Command::SaveBinding {
                    binding: binding.clone(),
                    preview: Some(preview["preview"].as_str().unwrap().into())
                },
                None
            )
            .await
            .is_err()
    );
    save(&engine, binding.clone()).await;
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 0);
    let delivery = engine.plugin_catalog().await.unwrap().deliveries.remove(0);
    assert_eq!(delivery.state, "failed");
    assert!(engine.sync_overview(None).await.unwrap().held > 0);
    let prepared: String =
        store::sqlx::query_scalar("SELECT prepared FROM plugin_outbox WHERE uid=?")
            .bind(&delivery.uid)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    assert!(!prepared.contains("Do not publish"));
    let before = serde_json::from_str::<Value>(&prepared).unwrap()["payload"]["id"].clone();
    relay.reject.store(false, Ordering::SeqCst);
    engine
        .plugin_command(
            Command::Retry {
                delivery: delivery.uid,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 1);
    assert_eq!(relay.events.lock().await[0]["id"], before);
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 0);
    assert_eq!(relay.publications.load(Ordering::SeqCst), 2);
    let count: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM record WHERE kind='plain' AND deleted_at IS NULL",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    assert_eq!(
        store::records::get(&engine.store.pool, &excluded)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Do not publish"
    );
    let key = directory
        .path()
        .join("plugins/.state/example.nostr/identity.key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(key).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn startup_recovers_interrupted_work_and_changed_packages_require_enablement_again() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path()).await;
    let database = format!("sqlite://{}", directory.path().join("lince.db").display());
    let relay = Relay::start(Vec::new()).await;
    let engine = Engine::open(&database).await.unwrap();
    engine
        .load_plugins(directory.path().join("plugins"))
        .await
        .unwrap();
    enable(&engine).await;
    let uid = record(&engine, RecordKind::Plain, "Note", "Content").await;
    let selection = protein(&engine, &uid).await;
    let binding = binding(&relay, Direction::Outgoing, Some(selection));
    save(&engine, binding.clone()).await;
    assert!(engine.run_plugin_binding(&binding.uid).await.is_err());
    store::sqlx::query("UPDATE plugin_outbox SET state='running'")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&database).await.unwrap();
    engine
        .load_plugins(directory.path().join("plugins"))
        .await
        .unwrap();
    assert_eq!(
        engine.plugin_catalog().await.unwrap().deliveries[0].state,
        "uncertain"
    );
    assert_eq!(relay.publications.load(Ordering::SeqCst), 0);
    assert!(
        engine
            .plugin_command(Command::Inspect, Some("remote-person"))
            .await
            .is_err()
    );
    assert!(
        engine::operation_origin::fiote(
            "agent",
            "thread",
            engine.plugin_command(Command::Inspect, None)
        )
        .await
        .is_err()
    );
    let manifest_path = directory.path().join("plugins/example.nostr/plugin.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["version"] = json!(2);
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&database).await.unwrap();
    engine
        .load_plugins(directory.path().join("plugins"))
        .await
        .unwrap();
    assert!(!engine.plugin_catalog().await.unwrap().packages[0].enabled);
    assert!(engine.run_plugin_binding(&binding.uid).await.is_err());
    assert!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn connection_validation_and_missing_publication_identity_are_explicit() {
    let state = tempfile::tempdir().unwrap();
    let invalid = lince_plugin_nostr::handle(
        "notes",
        "validate",
        json!({"settings":{"relays":"ws://public.example","authors":"","limit":20}}),
        state.path(),
    )
    .await;
    assert!(invalid.is_err());
    let valid = lince_plugin_nostr::handle(
        "notes",
        "validate",
        json!({"settings":{"relays":"wss://relay.example","authors":"","limit":20}}),
        state.path(),
    )
    .await;
    assert!(valid.is_ok());
    let missing = lince_plugin_nostr::handle("notes","prepare",json!({"settings":{"relays":"wss://relay.example","authors":"","limit":20},"record":{"uid":nucleus::new_uid("r"),"head":"Note","body":"Content","created_at":0}}),state.path()).await;
    assert!(missing.unwrap_err().contains("publication identity"));
}
