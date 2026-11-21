// non-lean-yet
// Scope: fn fixture, struct Reply, struct Mock, fn respond.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{Response, StatusCode},
};
use lince_plugin_nomad::{
    export::write_graph,
    fetch,
    graph::{Graph, NAMESPACE, digest},
    handle, manifest, pull,
    schema::*,
    settings::Settings,
    sync,
};
use nucleus::plugins::{Contribution, MAX_BYTES, PullResult};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::io::AsyncWriteExt;

fn fixture() -> BTreeMap<String, Value> {
    serde_json::from_str(include_str!("fixtures/local-nomad.json")).unwrap()
}

#[derive(Clone)]
struct Reply {
    status: StatusCode,
    body: Vec<u8>,
    location: Option<String>,
}

#[derive(Default)]
struct Mock {
    replies: Mutex<BTreeMap<String, Reply>>,
    requests: Mutex<Vec<String>>,
}

async fn respond(State(mock): State<Arc<Mock>>, request: Request) -> Response<Body> {
    assert_eq!(request.method(), "GET");
    assert!(!request.headers().contains_key("authorization"));
    assert!(!request.headers().contains_key("cookie"));
    assert_eq!(request.headers()["user-agent"], "Lince-NOMAD/0.1");
    let path = request.uri().path();
    mock.requests.lock().unwrap().push(path.to_owned());
    let path = path.strip_prefix("/nomad").unwrap_or(path);
    let reply = mock.replies.lock().unwrap().get(path).cloned();
    let reply = reply.unwrap_or(Reply {
        status: StatusCode::NOT_FOUND,
        body: b"missing".to_vec(),
        location: None,
    });
    let mut response = Response::builder()
        .status(reply.status)
        .header("content-type", "application/json");
    if let Some(location) = reply.location {
        response = response.header("location", location);
    }
    response.body(Body::from(reply.body)).unwrap()
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
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mock = Arc::new(Mock::default());
        for (path, body) in fixture() {
            mock.replies.lock().unwrap().insert(
                path,
                Reply {
                    status: StatusCode::OK,
                    body: serde_json::to_vec(&body).unwrap(),
                    location: None,
                },
            );
        }
        let app = Router::new().fallback(respond).with_state(mock.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { url, mock, task }
    }
    fn settings(&self) -> Settings {
        Settings {
            base_url: self.url.clone(),
            instance_id: "test-library".into(),
            kiwix_url: format!("{}/kiwix", self.url),
            include_maps: true,
            include_documents: true,
            include_wikipedia: true,
            batch_size: 3,
        }
    }
    fn set(&self, path: &str, value: Value) {
        self.mock.replies.lock().unwrap().insert(
            path.into(),
            Reply {
                status: StatusCode::OK,
                body: serde_json::to_vec(&value).unwrap(),
                location: None,
            },
        );
    }
    fn fail(&self, path: &str, status: StatusCode, body: Vec<u8>, location: Option<String>) {
        self.mock.replies.lock().unwrap().insert(
            path.into(),
            Reply {
                status,
                body,
                location,
            },
        );
    }
}

fn project(source: &str) -> anicca::ProjectedDocument {
    let mut document = anicca::project(&anicca::parse(source).unwrap()).unwrap();
    let identities = document
        .records
        .iter()
        .map(|r| (r.slug.clone().unwrap(), r.uid.clone()))
        .collect();
    anicca::resolve_project_references(&mut document, &identities).unwrap();
    document
}

fn node<'a>(graph: &'a Graph, kind: &str, key: &str) -> &'a lince_plugin_nomad::graph::Node {
    graph
        .nodes
        .values()
        .find(|n| n.kind == kind && n.remote_id == key)
        .unwrap()
}

#[tokio::test]
async fn local_schema_becomes_connected_native_records_with_shared_sources_and_provenance() {
    let server = Server::start().await;
    let graph = fetch(server.settings()).await.unwrap();
    assert_eq!(
        graph
            .nodes
            .values()
            .filter(|n| n.kind == "resource" && n.data["id"] == "manuals_en_all")
            .count(),
        1
    );
    let shared = node(&graph, "resource", "zim:manuals_en_all");
    assert_eq!(shared.data["catalog_variants"].as_array().unwrap().len(), 2);
    assert_eq!(
        graph
            .nodes
            .values()
            .filter(|n| n.relations.iter().any(|r| r.target == shared.uid))
            .count(),
        3
    );
    assert_eq!(
        graph
            .nodes
            .values()
            .flat_map(|n| &n.relations)
            .filter(|r| r.predicate == "nomad-includes-bundle")
            .count(),
        1
    );
    assert!(
        node(&graph, "archive", "manuals_en_all_2025-12.zim")
            .relations
            .iter()
            .any(|r| r.target == shared.uid)
    );
    assert!(
        node(&graph, "archive", "wikipedia_en_all_2026-01.zim")
            .source_url
            .ends_with("/kiwix/content/wikipedia_en_all_2026-01/")
    );
    let doc = node(
        &graph,
        "document",
        "/opt/project-nomad/storage/uploads/Water & soil #1.pdf",
    );
    let url = reqwest::Url::parse(&doc.source_url).unwrap();
    assert_eq!(
        url.query_pairs().collect::<BTreeMap<_, _>>()["source"],
        doc.remote_id
    );
    assert!(doc.resource.is_some());
    assert_eq!(
        doc.resource.as_ref().unwrap()["media_type"],
        "application/pdf"
    );
    assert_eq!(
        node(
            &graph,
            "document",
            "/opt/project-nomad/storage/zim/wikipedia_en_all_2026-01.zim"
        )
        .relations
        .len(),
        1
    );
    assert_eq!(server.mock.requests.lock().unwrap().len(), 6);

    let mut visited = BTreeSet::new();
    let mut pending = vec![graph.root.clone()];
    while let Some(id) = pending.pop() {
        if visited.insert(id.clone()) {
            pending.extend(graph.nodes[&id].relations.iter().map(|r| r.target.clone()));
        }
    }
    assert_eq!(
        visited.len(),
        graph.nodes.len(),
        "Every source must be reachable from the instance entrance"
    );

    let output = tempfile::tempdir().unwrap();
    let report = write_graph(&graph, output.path()).unwrap();
    assert!(report.changed);
    let source = fs::read_to_string(&report.path).unwrap();
    let document = project(&source);
    let wikipedia = document
        .records
        .iter()
        .find(|r| r.head == "Complete Wikipedia （Compact）")
        .unwrap();
    assert!(
        document
            .extensions
            .iter()
            .any(|e| e.target_uid.as_ref() == Some(&wikipedia.uid)
                && e.namespace == NAMESPACE
                && e.fields["original_title"] == "Complete Wikipedia (Compact)")
    );
    assert_eq!(document.records.len(), graph.nodes.len());
    assert!(
        document.imports.is_empty() && document.rules.is_empty() && document.frequencies.is_empty()
    );
    assert!(
        document
            .records
            .iter()
            .all(|r| r.quantity.as_ref().unwrap().0 == "0")
    );
    assert!(
        document
            .records
            .iter()
            .all(|r| nucleus::valid_uid(&r.uid, "r"))
    );
    assert!(source.contains(&format!("[[Shared manuals|{}]]", shared.uid)));
    assert_eq!(
        document
            .extensions
            .iter()
            .filter(|e| e.namespace == NAMESPACE)
            .count(),
        document.records.len()
    );
    for record in &document.records {
        for assertion in &record.assertions {
            assert!(
                graph
                    .nodes
                    .contains_key(assertion.object_uid.as_ref().unwrap())
            );
        }
    }
    anicca::check_project(output.path()).unwrap();
    let relocated = tempfile::tempdir().unwrap();
    fs::copy(&report.path, relocated.path().join("Sources.lingua")).unwrap();
    anicca::check_project(relocated.path()).unwrap();
    assert_eq!(
        project(&fs::read_to_string(relocated.path().join("Sources.lingua")).unwrap()).records,
        document.records
    );
    let mtime = fs::metadata(&report.path).unwrap().modified().unwrap();
    assert!(!write_graph(&graph, output.path()).unwrap().changed);
    assert_eq!(
        mtime,
        fs::metadata(&report.path).unwrap().modified().unwrap()
    );
}

#[tokio::test]
async fn stable_identities_survive_moves_reordering_and_source_revisions() {
    let server = Server::start().await;
    let before = fetch(server.settings()).await.unwrap();
    let mut categories = fixture()["/api/zim/curated-categories"].clone();
    categories.as_array_mut().unwrap().reverse();
    server.set("/api/zim/curated-categories", categories);
    let mut moved = server.settings();
    moved.base_url.push_str("/nomad");
    let after = fetch(moved).await.unwrap();
    assert_eq!(
        before.nodes.keys().collect::<Vec<_>>(),
        after.nodes.keys().collect::<Vec<_>>()
    );
    assert!(
        server
            .mock
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|p| p == "/nomad/api/zim/list")
    );
    // Changing the endpoint also changes source links, but not canonical IDs.
    let after = fetch(server.settings()).await.unwrap();
    assert_eq!(
        before.fingerprint(),
        after.fingerprint(),
        "Remote ordering must not create updates"
    );

    let mut files = fixture()["/api/zim/list"].clone();
    files["files"][0]["title"] = json!("A revised archive title");
    server.set("/api/zim/list", files);
    let revised = fetch(server.settings()).await.unwrap();
    let old = node(&before, "archive", "manuals_en_all_2025-12.zim");
    let new = node(&revised, "archive", "manuals_en_all_2025-12.zim");
    assert_eq!(old.uid, new.uid);
    assert_ne!(
        before.imported(old).unwrap().external_id,
        revised.imported(new).unwrap().external_id
    );
    assert_eq!(
        before.imported(old).unwrap().metadata["identity"],
        revised.imported(new).unwrap().metadata["identity"]
    );
    let output = tempfile::tempdir().unwrap();
    let first = write_graph(&before, output.path()).unwrap();
    let next = write_graph(&revised, output.path()).unwrap();
    assert_eq!(first.path, next.path);
    assert!(next.changed);
    assert!(
        fs::read_to_string(next.path)
            .unwrap()
            .contains("A revised archive title")
    );
}

#[tokio::test]
async fn bounded_pulls_resume_without_gaps_and_changed_snapshots_restart_safely() {
    let server = Server::start().await;
    let graph = fetch(server.settings()).await.unwrap();
    let mut cursor = Value::Null;
    let mut retained = BTreeMap::new();
    loop {
        let batch = pull(&graph, 3, cursor).unwrap();
        assert!(batch.records.len() <= 3);
        cursor = batch.cursor;
        if batch.records.is_empty() {
            break;
        }
        for record in batch.records {
            assert!(
                retained
                    .insert(record.external_id.clone(), record)
                    .is_none()
            );
        }
    }
    assert_eq!(retained.len(), graph.nodes.len());
    assert!(pull(&graph, 3, cursor.clone()).unwrap().records.is_empty());
    let retried = pull(&graph, 3, Value::Null).unwrap();
    for record in retried.records {
        assert_eq!(
            serde_json::to_value(&record).unwrap(),
            serde_json::to_value(&retained[&record.external_id]).unwrap()
        );
    }
    let mut revised = graph.clone();
    revised
        .nodes
        .get_mut(&graph.root)
        .unwrap()
        .description
        .push_str(" Updated.");
    let restart = pull(&revised, 3, cursor.clone()).unwrap();
    assert_eq!(restart.records.len(), 3);
    assert_eq!(restart.cursor["offset"], 3);
    assert!(
        pull(
            &graph,
            3,
            json!({"version":2,"snapshot":graph.fingerprint(),"offset":0})
        )
        .is_err()
    );
    assert!(
        pull(
            &graph,
            3,
            json!({"version":1,"snapshot":graph.fingerprint(),"offset":graph.nodes.len()+1})
        )
        .is_err()
    );
    assert!(pull(&graph, 0, cursor).is_err());
}

#[test]
fn response_frames_obey_byte_budget_as_well_as_item_budget() {
    let resources = (0..100)
        .map(|i| Resource {
            id: format!("book-{i}"),
            version: "2026-01".into(),
            title: format!("Book {i}"),
            description: "x".repeat(11000),
            url: format!("https://catalog.invalid/book-{i}.zim"),
            size_mb: 1.0,
            kind: None,
            auth: None,
        })
        .collect();
    let graph = Graph::build(
        &Settings::default(),
        Snapshot {
            categories: vec![Category {
                name: "Library".into(),
                slug: "library".into(),
                description: "Books".into(),
                language: "en".into(),
                tiers: vec![Tier {
                    name: "All".into(),
                    slug: "all".into(),
                    description: "All books".into(),
                    includes_tier: None,
                    resources,
                }],
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let mut cursor = Value::Null;
    let mut count = 0;
    let first = pull(&graph, 100, Value::Null).unwrap();
    assert!(first.records.len() < 100);
    loop {
        let batch = pull(&graph, 100, cursor).unwrap();
        assert!(serde_json::to_vec(&batch).unwrap().len() + 100 < MAX_BYTES);
        count += batch.records.len();
        cursor = batch.cursor;
        if batch.records.is_empty() {
            break;
        }
    }
    assert_eq!(count, graph.nodes.len());
}

#[tokio::test]
async fn refresh_preserves_local_edits_and_previous_export_when_remote_fails() {
    let server = Server::start().await;
    let directory = tempfile::tempdir().unwrap();
    let report = sync(server.settings(), directory.path()).await.unwrap();
    let original = fs::read(&report.path).unwrap();
    server.fail(
        "/api/maps/regions",
        StatusCode::SERVICE_UNAVAILABLE,
        b"offline".to_vec(),
        None,
    );
    assert!(
        sync(server.settings(), directory.path())
            .await
            .unwrap_err()
            .contains("503")
    );
    assert_eq!(fs::read(&report.path).unwrap(), original);
    server.set("/api/maps/regions", fixture()["/api/maps/regions"].clone());
    let edited = [original.as_slice(), b"\nA local edit\n"].concat();
    fs::write(&report.path, &edited).unwrap();
    assert!(
        sync(server.settings(), directory.path())
            .await
            .unwrap_err()
            .contains("edited outside NOMAD")
    );
    assert_eq!(fs::read(&report.path).unwrap(), edited);
}

#[tokio::test]
async fn journal_recovers_an_interrupted_rename_without_accepting_foreign_content() {
    let server = Server::start().await;
    let mut graph = fetch(server.settings()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let old = write_graph(&graph, dir.path()).unwrap();
    graph
        .nodes
        .get_mut(&graph.root)
        .unwrap()
        .description
        .push_str(" New snapshot.");
    let new = graph.lingua().unwrap();
    let state_path = dir.path().join(format!(
        ".{}.state.json",
        old.path.file_stem().unwrap().to_str().unwrap()
    ));
    fs::write(
        &state_path,
        serde_json::to_vec(
            &json!({"last_digest":old.digest,"pending_digest":digest(new.as_bytes())}),
        )
        .unwrap(),
    )
    .unwrap();
    fs::write(&old.path, &new).unwrap();
    assert!(!write_graph(&graph, dir.path()).unwrap().changed);
    let state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    assert!(state["pending_digest"].is_null());
    assert_eq!(state["last_digest"], digest(new.as_bytes()));
}

#[tokio::test]
async fn optional_backends_are_opt_in_and_malformed_or_incomplete_responses_fail() {
    let server = Server::start().await;
    let settings = Settings {
        base_url: server.url.clone(),
        ..Default::default()
    };
    fetch(settings.clone()).await.unwrap();
    assert_eq!(server.mock.requests.lock().unwrap().len(), 2);
    server.fail(
        "/api/zim/list",
        StatusCode::OK,
        b"<html>login</html>".to_vec(),
        None,
    );
    assert!(
        fetch(settings.clone())
            .await
            .unwrap_err()
            .contains("invalid response")
    );
    server.set("/api/zim/list", json!({"files":[],"next":"unread-page"}));
    assert!(
        fetch(settings.clone())
            .await
            .unwrap_err()
            .contains("incomplete snapshot")
    );
    server.set("/api/zim/list", json!({"unrelated":[]}));
    assert!(fetch(settings.clone()).await.is_err());
    server.fail(
        "/api/zim/list",
        StatusCode::OK,
        vec![b' '; 4 * 1024 * 1024 + 1],
        None,
    );
    assert!(
        fetch(settings.clone())
            .await
            .unwrap_err()
            .contains("exceeds 4 MiB")
    );
    server.fail(
        "/api/zim/list",
        StatusCode::FOUND,
        Vec::new(),
        Some(format!("{}/must-not-follow", server.url)),
    );
    assert!(fetch(settings).await.unwrap_err().contains("302"));
    assert!(
        !server
            .mock
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.contains("must-not-follow"))
    );
}

#[tokio::test]
async fn invalid_catalog_graphs_are_rejected_and_descriptions_stay_inert() {
    let server = Server::start().await;
    let mut categories = fixture()["/api/zim/curated-categories"].clone();
    categories[0]["tiers"][0]["includesTier"] = json!("comprehensive");
    server.set("/api/zim/curated-categories", categories.clone());
    assert!(
        fetch(server.settings())
            .await
            .unwrap_err()
            .contains("Cyclic")
    );
    categories[0]["tiers"][0]["includesTier"] = json!("missing");
    server.set("/api/zim/curated-categories", categories);
    assert!(
        fetch(server.settings())
            .await
            .unwrap_err()
            .contains("Missing inherited tier")
    );
    let mut categories = fixture()["/api/zim/curated-categories"].clone();
    categories[1]["tiers"][0]["resources"][0]["version"] = json!("2099-01");
    server.set("/api/zim/curated-categories", categories);
    assert!(
        fetch(server.settings())
            .await
            .unwrap_err()
            .contains("Conflicting definitions")
    );

    let injection = "A description\n} r_00000000000000000000000000\nInjected (@injected: 1) { r_00000000000000000000000001\nprint(\"do not execute\")\n} r_00000000000000000000000001";
    let mut categories = fixture()["/api/zim/curated-categories"].clone();
    categories[0]["description"] = json!(injection);
    categories[0]["name"] = json!("Import (title) {source} ^ annotation");
    server.set("/api/zim/curated-categories", categories);
    let graph = fetch(server.settings()).await.unwrap();
    let document = project(&graph.lingua().unwrap());
    assert_eq!(document.records.len(), graph.nodes.len());
    assert!(document.records.iter().any(|r| r.body.contains(injection)));
    assert!(
        !document
            .records
            .iter()
            .any(|r| r.slug.as_deref() == Some("injected"))
    );
}

#[tokio::test]
async fn native_manifest_and_framed_rpc_work_in_the_actual_plugin_host() {
    let server = Server::start().await;
    let data = tempfile::tempdir().unwrap();
    let install = tokio::process::Command::new(env!("CARGO_BIN_EXE_lince-plugin-nomad"))
        .arg("--install")
        .arg(data.path())
        .output()
        .await
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let host = plugins::Host::discover(data.path().join("plugins"))
        .await
        .unwrap();
    assert!(host.diagnostics().is_empty(), "{:?}", host.diagnostics());
    let package = host.packages().remove(0);
    package.manifest.validate().unwrap();
    let input = json!({"settings":server.settings(),"cursor":null});
    let response = host
        .call(
            "lince.nomad",
            &package.digest,
            "sources",
            "pull",
            input.clone(),
        )
        .await
        .unwrap();
    let pulled: PullResult = serde_json::from_value(response).unwrap();
    assert_eq!(pulled.records.len(), 3);
    let fields = package
        .manifest
        .contributions
        .iter()
        .find_map(|c| match c {
            Contribution::Schema { fields, .. } => Some(fields),
            _ => None,
        })
        .unwrap();
    for record in &pulled.records {
        nucleus::plugins::validate_values(fields, &record.metadata).unwrap();
        assert!(record.body.len() <= 16384 && record.head.len() <= 1024);
    }
    let valid = host
        .call("lince.nomad", &package.digest, "sources", "validate", input)
        .await
        .unwrap();
    assert_eq!(valid["valid"], true);
    let destination = tempfile::tempdir().unwrap();
    let exported = host
        .call(
            "lince.nomad",
            &package.digest,
            "export",
            "invoke",
            json!({"settings":server.settings(),"directory":destination.path()}),
        )
        .await
        .unwrap();
    let path = Path::new(exported["path"].as_str().unwrap());
    assert!(path.is_file());
    assert!(project(&fs::read_to_string(path).unwrap()).records.len() > 3);
    assert!(
        host.call(
            "lince.nomad",
            "unapproved-digest",
            "sources",
            "pull",
            json!({})
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn cli_sync_and_protocol_errors_are_reviewable_and_do_not_install_anything() {
    let server = Server::start().await;
    let output = tempfile::tempdir().unwrap();
    let config = output.path().join("settings.json");
    fs::write(&config, serde_json::to_vec(&server.settings()).unwrap()).unwrap();
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_lince-plugin-nomad"))
        .arg("--sync")
        .arg(&config)
        .arg(output.path())
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["changed"], true);
    assert!(Path::new(report["path"].as_str().unwrap()).is_file());
    assert!(!output.path().join("plugins").exists());

    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_lince-plugin-nomad"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = process.stdin.take().unwrap();
    stdin.write_all(b"{\"protocol\":\"invalid\",\"id\":1,\"contribution\":\"sources\",\"operation\":\"pull\",\"input\":{}}\n").await.unwrap();
    drop(stdin);
    let result = process.wait_with_output().await.unwrap();
    assert!(result.status.success());
    let response: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["error"], "Unsupported plugin protocol");
    assert!(handle("sources", "publish", json!({})).await.is_err());
}

#[test]
fn settings_accept_local_http_and_proxy_prefixes_but_never_tokens_or_credentials() {
    for base_url in [
        "http://192.168.1.2:8080",
        "http://nomad.local",
        "http://[::1]:8080/nomad",
    ] {
        Settings {
            base_url: base_url.into(),
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
    for base_url in [
        "file:///etc/passwd",
        "http://user:secret@localhost",
        "http://localhost/?token=secret",
        "http://localhost/#secret",
    ] {
        assert!(
            Settings {
                base_url: base_url.into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(serde_json::from_value::<Settings>(json!({"token":"secret"})).is_err());
    manifest("lince-plugin-nomad".into()).validate().unwrap();
}

#[tokio::test]
async fn manifest_envelopes_and_gated_dataset_references_need_no_external_fetches() {
    let server = Server::start().await;
    let mut categories = fixture()["/api/zim/curated-categories"].clone();
    categories[0]["tiers"][0]["resources"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"reference-dataset", "version":"2026-01", "title":"Reference dataset",
            "description":"A catalog reference with an entitlement requirement.",
            "url":format!("{}/must-not-download", server.url), "size_mb":10,
            "type":"dataset", "auth":"nomad_app_key", "future_upstream_field":"ignored"
        }));
    // An explicit default type and an omitted default type describe the same
    // shared archive, and still preserve their original descriptor variants.
    categories[1]["tiers"][0]["resources"][0]["type"] = json!("zim");
    server.set(
        "/api/zim/curated-categories",
        json!({"spec_version":"2026-01","categories":categories}),
    );
    server.set(
        "/api/maps/curated-collections",
        json!({"spec_version":"2026-01","collections":fixture()["/api/maps/curated-collections"]}),
    );
    let graph = fetch(server.settings()).await.unwrap();
    let resource = node(&graph, "resource", "dataset:reference-dataset");
    assert_eq!(resource.data["auth"], "nomad_app_key");
    assert_eq!(
        graph
            .nodes
            .values()
            .filter(|n| n.remote_id == "zim:manuals_en_all")
            .count(),
        1
    );
    assert!(
        project(&graph.lingua().unwrap())
            .records
            .iter()
            .any(|r| r.head == "Reference dataset")
    );
    assert_eq!(server.mock.requests.lock().unwrap().len(), 6);
    assert!(
        !server
            .mock
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.contains("must-not-download"))
    );
}

#[tokio::test]
async fn disappearing_sources_update_the_export_without_erasing_authored_files() {
    let server = Server::start().await;
    let before = fetch(server.settings()).await.unwrap();
    let removed = node(&before, "archive", "manuals_en_all_2025-12.zim")
        .uid
        .clone();
    let directory = tempfile::tempdir().unwrap();
    let report = write_graph(&before, directory.path()).unwrap();
    let authored_path = directory.path().join("My-notes.md");
    fs::write(&authored_path, "My authored learning notes").unwrap();
    let mut archives = fixture()["/api/zim/list"].clone();
    archives["files"].as_array_mut().unwrap().remove(0);
    server.set("/api/zim/list", archives);
    let after = fetch(server.settings()).await.unwrap();
    assert!(!after.nodes.contains_key(&removed));
    for uid in after.nodes.keys() {
        assert!(before.nodes.contains_key(uid));
    }
    write_graph(&after, directory.path()).unwrap();
    let reopened = project(&fs::read_to_string(report.path).unwrap());
    assert!(!reopened.records.iter().any(|r| r.uid == removed));
    assert_eq!(
        fs::read_to_string(authored_path).unwrap(),
        "My authored learning notes"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn export_refuses_symlinks_and_simultaneous_writers() {
    let server = Server::start().await;
    let graph = fetch(server.settings()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let report = write_graph(&graph, dir.path()).unwrap();
    let lock_path = dir.path().join(format!(
        ".{}.lock",
        report.path.file_stem().unwrap().to_str().unwrap()
    ));
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    lock.try_lock().unwrap();
    assert!(
        write_graph(&graph, dir.path())
            .unwrap_err()
            .contains("Another NOMAD export")
    );
    drop(lock);
    fs::remove_file(&report.path).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), b"preserve me").unwrap();
    std::os::unix::fs::symlink(outside.path(), &report.path).unwrap();
    assert!(write_graph(&graph, dir.path()).is_err());
    assert_eq!(fs::read(outside.path()).unwrap(), b"preserve me");
}
