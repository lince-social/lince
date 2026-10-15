use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use engine::Engine;
use nucleus::{
    RecordKind,
    plugins::{Binding, Command, Direction},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::Mutex;

#[derive(Default)]
struct Mock {
    statuses: Mutex<Vec<Value>>,
    published: AtomicUsize,
    keys: Mutex<BTreeMap<String, Value>>,
    queries: Mutex<Vec<BTreeMap<String, String>>>,
}

fn note(id: &str, content: &str) -> Value {
    json!({"id":id,"content":content,"visibility":"public","reblog":null,"account":{"acct":"alice@example.test"},"url":format!("https://example.test/@alice/{id}"),"created_at":"2026-10-13T12:00:00Z","in_reply_to_id":null})
}

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .is_some_and(|value| value == "Bearer test-token")
}

async fn timeline(
    State(mock): State<Arc<Mock>>,
    headers: HeaderMap,
    Query(query): Query<BTreeMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    if !authorized(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    mock.queries.lock().await.push(query.clone());
    let min = query.get("min_id").and_then(|id| id.parse::<u64>().ok());
    let limit = query
        .get("limit")
        .and_then(|limit| limit.parse::<usize>().ok())
        .unwrap_or(20);
    let mut statuses: Vec<_> = mock
        .statuses
        .lock()
        .await
        .iter()
        .filter(|status| {
            min.is_none_or(|min| status["id"].as_str().unwrap().parse::<u64>().unwrap() > min)
        })
        .cloned()
        .collect();
    statuses.sort_by_key(|status| status["id"].as_str().unwrap().parse::<u64>().unwrap());
    if min.is_some() {
        statuses.truncate(limit);
    }
    statuses.reverse();
    statuses.truncate(limit);
    Ok(Json(json!(statuses)))
}

async fn publish(
    State(mock): State<Arc<Mock>>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !authorized(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    assert_eq!(input["visibility"], "public");
    let key = headers
        .get("idempotency-key")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let mut keys = mock.keys.lock().await;
    if let Some(previous) = keys.get(&key) {
        return Ok(Json(previous.clone()));
    }
    mock.published.fetch_add(1, Ordering::SeqCst);
    let content = input["status"].as_str().unwrap();
    let html = format!("<p>{}</p>", html_escape::encode_text(content));
    let status = note("100", &html);
    keys.insert(key, status.clone());
    mock.statuses.lock().await.push(status.clone());
    Ok(Json(status))
}

struct Server {
    url: String,
    mock: Arc<Mock>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn start(statuses: Vec<Value>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mock = Arc::new(Mock {
            statuses: Mutex::new(statuses),
            ..Default::default()
        });
        let router = Router::new()
            .route("/api/v1/timelines/public", get(timeline))
            .route("/api/v1/statuses", post(publish))
            .with_state(mock.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { url, mock, task }
    }
}

async fn install(directory: &Path) {
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_lince-plugin-activitypub"))
        .args(["--install"])
        .arg(directory)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn enable(engine: &Engine) {
    let package = engine.plugin_catalog().await.unwrap().packages.remove(0);
    engine
        .plugin_command(
            Command::Enable {
                plugin: package.manifest.id,
                digest: package.digest,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
}

async fn save(engine: &Engine, binding: Binding) {
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
        .unwrap()["preview"]
        .as_str()
        .unwrap()
        .to_owned();
    engine
        .plugin_command(
            Command::SaveBinding {
                binding,
                preview: Some(preview),
            },
            None,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn real_plugin_roundtrip_survives_restart_clear_history_and_removal_without_resending() {
    let directory = tempfile::tempdir().unwrap();
    install(directory.path()).await;
    let token = directory.path().join("token");
    tokio::fs::write(&token, "test-token\n").await.unwrap();
    let server = Server::start(vec![note("1", "<p>Hello &amp; welcome</p>")]).await;
    let database = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("cell.db").display()
    );
    let engine = Engine::open(&database).await.unwrap();
    engine
        .load_plugins(directory.path().join("plugins"))
        .await
        .unwrap();
    enable(&engine).await;
    let outgoing = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Greetings",
            body: "From Lince",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    let selection = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Protein,
            head: "Published notes",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    store::records::set_extension(&engine.store.pool, &selection.uid, "lince.protein", &json!({"source":"record","where":[{"uid_eq":outgoing.uid}],"fields":["uid","head","body"]})).await.unwrap();
    let binding = Binding {
        uid: nucleus::new_uid("r"),
        provider: "lince.activitypub/notes".into(),
        name: "Mastodon".into(),
        settings: json!({"instance":server.url,"token_file":token,"limit":20}),
        protein_uid: Some(selection.uid),
        incoming_filter: json!([]),
        direction: Direction::Both,
        interval_seconds: 60,
        enabled: true,
        revision: 0,
    };
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
    save(&engine, binding.clone()).await;
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 2);
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 0);
    assert_eq!(server.mock.published.load(Ordering::SeqCst), 1);
    let rows = store::records::list_all(&engine.store.pool).await.unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row.body == "Hello & welcome")
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.body == "Greetings\n\nFrom Lince")
            .count(),
        0
    );
    engine
        .plugin_command(Command::ClearHistory, None)
        .await
        .unwrap();
    assert!(engine.plugin_catalog().await.unwrap().deliveries.is_empty());
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 0);
    drop(engine);
    let engine = Engine::open(&database).await.unwrap();
    engine
        .load_plugins(directory.path().join("plugins"))
        .await
        .unwrap();
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 0);
    assert_eq!(server.mock.published.load(Ordering::SeqCst), 1);
    let package = engine.plugin_catalog().await.unwrap().packages.remove(0);
    engine
        .plugin_command(
            Command::Remove {
                plugin: package.manifest.id,
                digest: package.digest,
            },
            None,
        )
        .await
        .unwrap();
    assert!(engine.run_plugin_binding(&binding.uid).await.is_err());
    assert!(
        store::records::get(&engine.store.pool, &outgoing.uid)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(engine.plugin_catalog().await.unwrap().bindings.len(), 1);
    install(directory.path()).await;
    engine.plugin_command(Command::Reload, None).await.unwrap();
    assert!(!engine.plugin_catalog().await.unwrap().packages[0].enabled);
    enable(&engine).await;
    assert_eq!(engine.run_plugin_binding(&binding.uid).await.unwrap(), 0);
    assert_eq!(server.mock.published.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn paginated_receiving_moves_forward_without_skipping_a_full_batch() {
    let directory = tempfile::tempdir().unwrap();
    let token = directory.path().join("token");
    tokio::fs::write(&token, "test-token").await.unwrap();
    let server = Server::start(vec![]).await;
    let settings = json!({"instance":server.url,"token_file":token,"limit":2});
    for id in 1..=5 {
        server
            .mock
            .statuses
            .lock()
            .await
            .push(note(&id.to_string(), "<p>Text</p>"));
    }
    let mut cursor = json!("0");
    let mut ids = Vec::new();
    for _ in 0..3 {
        let batch = lince_plugin_activitypub::handle(
            "notes",
            "pull",
            json!({"settings":settings,"cursor":cursor}),
        )
        .await
        .unwrap();
        ids.extend(batch["records"].as_array().unwrap().iter().map(|record| {
            record["external_id"]
                .as_str()
                .unwrap()
                .rsplit(':')
                .next()
                .unwrap()
                .to_owned()
        }));
        cursor = batch["cursor"].clone();
    }
    ids.sort();
    assert_eq!(ids, ["1", "2", "3", "4", "5"]);
    assert_eq!(cursor, "5");
    assert!(
        server
            .mock
            .queries
            .lock()
            .await
            .iter()
            .all(|query| query.contains_key("min_id"))
    );
}

#[tokio::test]
async fn access_transport_visibility_and_content_limits_are_enforced() {
    lince_plugin_activitypub::manifest("runner".into())
        .validate()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let token = directory.path().join("token");
    tokio::fs::write(&token, "wrong-token").await.unwrap();
    let server = Server::start(vec![]).await;
    let settings = json!({"instance":server.url,"token_file":token,"limit":20});
    assert!(
        lince_plugin_activitypub::handle("notes", "pull", json!({"settings":settings}))
            .await
            .unwrap_err()
            .contains("401")
    );
    let mut insecure = settings.clone();
    insecure["instance"] = json!("http://example.test");
    assert!(
        lince_plugin_activitypub::handle("notes", "validate", json!({"settings":insecure}))
            .await
            .is_err()
    );
    assert!(lince_plugin_activitypub::handle("notes", "prepare", json!({"settings":settings,"record":{"uid":"r_test","head":"x".repeat(501),"body":"","created_at":0}})).await.is_err());
    let mut private = note("1", "Private");
    private["visibility"] = json!("private");
    assert!(lince_plugin_activitypub::imported(&private).is_err());
    let mut invalid = note("bad", "Broken");
    invalid["id"] = json!("../../path");
    assert!(lince_plugin_activitypub::imported(&invalid).is_err());
}
