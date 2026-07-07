//! End-to-end pilot (blueprint Window 1b over VII.3): a real WebSocket client
//! drives the axum host — subscribe to the focus queue, receive a snapshot,
//! complete the focus with an Action, and receive the live-recomputed queue.
//! This exercises the whole path: HTTP upgrade -> transport -> Session ->
//! Protein/Actions -> engine -> fact_bus -> live Update.

use std::sync::Arc;

use engine::actions::Action;
use engine::Engine;
use futures::{SinkExt, StreamExt};
use membrane::{router, Surface};
use nucleus::RecordKind;
use store::Store;
use tokio_tungstenite::tungstenite::Message;

/// Boot the router on a real port and return the base `http://addr`. Unlike
/// `spawn_host`, no seeded data — this is for asserting the static board surface.
async fn spawn_http() -> String {
    let engine = Arc::new(Engine::new(Store::open_memory().await.unwrap()).await.unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(Surface::new(engine));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("127.0.0.1:{}", addr.port())
}

/// Minimal HTTP/1.0 GET (no deps): returns (status_code, body).
async fn http_get(addr: &str, path: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!("GET {path} HTTP/1.0\r\nHost: {addr}\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

async fn spawn_host() -> (String, Arc<Engine>) {
    let engine = Arc::new(Engine::new(Store::open_memory().await.unwrap()).await.unwrap());
    // seed three ordered Needs
    engine.act(Action::CreateConcept { name: "before".into(), parents: vec![] }, None).await.unwrap();
    for (slug, head) in [("t.a", "Stretch"), ("t.b", "Coffee"), ("t.c", "Review")] {
        engine
            .act(
                Action::CreateRecord {
                    slug: Some(slug.into()),
                    kind: RecordKind::Plain,
                    head: head.into(),
                    body: String::new(),
                    quantity: -1.0,
                },
                None,
            )
            .await
            .unwrap();
    }
    for (a, b) in [("t.a", "t.b"), ("t.b", "t.c")] {
        engine
            .act(
                Action::AddLink { from: a.into(), kind: "before".into(), to: b.into(), quantity: None },
                None,
            )
            .await
            .unwrap();
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(Surface::new(engine.clone()));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("ws://{addr}/ws"), engine)
}

async fn next_json(
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
) -> serde_json::Value {
    loop {
        match ws.next().await.unwrap().unwrap() {
            Message::Text(t) => return serde_json::from_str(&t).unwrap(),
            _ => continue,
        }
    }
}

#[tokio::test]
async fn focus_sand_speaks_protein_and_actions_over_the_wire() {
    let (url, _engine) = spawn_host().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("ws connects");

    // subscribe with the focus-queue Protein (exactly what focus.html sends)
    let sub = serde_json::json!({
        "type": "subscribe", "id": "focus",
        "protein": {
            "source": "record",
            "where": [ { "quantity_lt": 0.0 }, { "kind_eq": "plain" } ],
            "order": [ { "topo": "before" }, { "asc": "created_at" } ]
        }
    });
    ws.send(Message::Text(sub.to_string().into())).await.unwrap();

    let snapshot = next_json(&mut ws).await;
    assert_eq!(snapshot["type"], "snapshot");
    let heads: Vec<&str> = snapshot["rows"].as_array().unwrap()
        .iter().map(|r| r["head"].as_str().unwrap()).collect();
    assert_eq!(heads, vec!["Stretch", "Coffee", "Review"], "ordered by @before");

    let focus_uid = snapshot["rows"][0]["uid"].as_str().unwrap().to_string();

    // complete the focus with a set-quantity Action
    let act = serde_json::json!({
        "type": "act", "id": "d1",
        "action": { "action": "set-quantity", "target": focus_uid, "value": 0.0 }
    });
    ws.send(Message::Text(act.to_string().into())).await.unwrap();

    // expect an action_ok and a live update with the promoted queue
    let mut got_ok = false;
    let mut updated: Option<Vec<String>> = None;
    for _ in 0..4 {
        let m = tokio::time::timeout(std::time::Duration::from_secs(3), next_json(&mut ws))
            .await
            .expect("message arrives");
        match m["type"].as_str() {
            Some("action_ok") => got_ok = true,
            Some("update") if m["id"] == "focus" => {
                updated = Some(
                    m["rows"].as_array().unwrap()
                        .iter().map(|r| r["head"].as_str().unwrap().to_string()).collect(),
                );
            }
            _ => {}
        }
        if got_ok && updated.is_some() {
            break;
        }
    }
    assert!(got_ok, "the Action was acknowledged");
    assert_eq!(updated.unwrap(), vec!["Coffee", "Review"], "next task took the focus, live");
}

/// The table sand's exact contract (Stage 8b, docs/stage-8b-web-sand-migration.md):
/// subscribe to `source: record`, create a record with an Action, see it arrive
/// live, then set its quantity and see the update. This is what the re-pointed
/// widget bridge relays over postMessage — proven here at the transport level.
#[tokio::test]
async fn table_sand_creates_and_updates_records_over_the_wire() {
    let (url, _engine) = spawn_host().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("ws connects");

    // The read the table sand issues: every record, newest first.
    let sub = serde_json::json!({
        "type": "subscribe", "id": "table",
        "protein": { "source": "record", "order": [ { "desc": "created_at" } ] }
    });
    ws.send(Message::Text(sub.to_string().into())).await.unwrap();

    let snapshot = next_json(&mut ws).await;
    assert_eq!(snapshot["type"], "snapshot");
    let seed_count = snapshot["rows"].as_array().unwrap().len();
    assert_eq!(seed_count, 3, "the three seeded records");

    // The write: create-record (exactly the table sand's "Add" button), at
    // quantity 0 — the common case. Creation emits a zero-delta provenance fact
    // (blueprint I.1/II.3), so even a zero-quantity record invalidates the live
    // subscription and appears immediately.
    let act = serde_json::json!({
        "type": "act", "id": "c1",
        "action": { "action": "create-record", "kind": "plain",
                    "head": "Buy flour", "body": "", "quantity": 0.0 }
    });
    ws.send(Message::Text(act.to_string().into())).await.unwrap();

    let mut new_uid: Option<String> = None;
    for _ in 0..4 {
        let m = tokio::time::timeout(std::time::Duration::from_secs(3), next_json(&mut ws))
            .await
            .expect("message arrives");
        if m["type"] == "action_ok" {
            new_uid = m["created"].as_str().map(str::to_string);
        }
        if m["type"] == "update" && m["id"] == "table" {
            let rows = m["rows"].as_array().unwrap();
            assert_eq!(rows.len(), seed_count + 1, "the new record is in the live result");
            assert!(
                rows.iter().any(|r| r["head"] == "Buy flour"),
                "the created record appears in the live result",
            );
            break;
        }
    }
    let new_uid = new_uid.expect("create-record returned the new uid");

    // The cell edit: set-quantity on the new record.
    let set = serde_json::json!({
        "type": "act", "id": "q1",
        "action": { "action": "set-quantity", "target": new_uid, "value": 12.0 }
    });
    ws.send(Message::Text(set.to_string().into())).await.unwrap();

    let mut saw_qty = false;
    for _ in 0..4 {
        let m = tokio::time::timeout(std::time::Duration::from_secs(3), next_json(&mut ws))
            .await
            .expect("message arrives");
        if m["type"] == "update" && m["id"] == "table" {
            let row = m["rows"].as_array().unwrap()
                .iter().find(|r| r["uid"] == new_uid.as_str()).expect("the record is present");
            assert_eq!(row["quantity"].as_f64().unwrap(), 12.0, "the quantity edit is reflected live");
            saw_qty = true;
            break;
        }
    }
    assert!(saw_qty, "a live update carried the new quantity");
}

/// The provenance sand's contract (Stage 8b §5): `source: record` with
/// `include: facts` returns each record's fact log inline — the blueprint's
/// W-provenance standard, "the end of custom plumbing", delivered over the same
/// subscription the table sand uses.
#[tokio::test]
async fn provenance_sand_receives_fact_log_inline() {
    let (url, _engine) = spawn_host().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("ws connects");

    let sub = serde_json::json!({
        "type": "subscribe", "id": "prov",
        "protein": {
            "source": "record",
            "where": [ { "slug_eq": "t.a" } ],
            "include": { "facts": { "limit": 5 } }
        }
    });
    ws.send(Message::Text(sub.to_string().into())).await.unwrap();

    let snapshot = next_json(&mut ws).await;
    assert_eq!(snapshot["type"], "snapshot");
    let row = &snapshot["rows"][0];
    assert_eq!(row["slug"], "t.a");
    let facts = row["facts"].as_array().expect("facts included inline");
    assert!(!facts.is_empty(), "the creation fact is in the log");
    // t.a was seeded at quantity -1.0 -> a user_edit fact of delta -1.
    assert!(
        facts.iter().any(|f| f["cause_kind"] == "user_edit" && f["delta"].as_f64() == Some(-1.0)),
        "provenance carries the seeding user_edit (delta, cause)",
    );
}

/// A keep-alive Ping frame must NOT drop the connection (real browsers send
/// them). Regression guard for the ws driver breaking on non-Text frames, which
/// silently starved every live update after the first browser keep-alive ping.
#[tokio::test]
async fn ping_frame_does_not_drop_the_connection() {
    let (url, _engine) = spawn_host().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("ws connects");

    // send a Ping before subscribing
    ws.send(Message::Ping(vec![1, 2, 3].into())).await.unwrap();

    let sub = serde_json::json!({
        "type": "subscribe", "id": "s",
        "protein": { "source": "record", "order": [ { "desc": "created_at" } ] }
    });
    ws.send(Message::Text(sub.to_string().into())).await.unwrap();

    // the connection survived the ping: the subscription still yields a snapshot
    // (tungstenite auto-answers the ping with a pong, which the server ignores)
    let mut got_snapshot = false;
    for _ in 0..4 {
        let m = tokio::time::timeout(std::time::Duration::from_secs(3), next_json(&mut ws))
            .await
            .expect("message arrives after a ping");
        if m["type"] == "snapshot" && m["id"] == "s" {
            got_snapshot = true;
            break;
        }
    }
    assert!(got_snapshot, "the connection survived a Ping and served the subscription");
}

/// Two subscriptions on ONE connection (as the board bridge multiplexes): a
/// committed fact must push an Update to BOTH. Isolates the browser bridge's
/// "action_ok arrives but updates don't" symptom at the transport level.
#[tokio::test]
async fn two_subscriptions_both_get_live_updates() {
    let (url, _engine) = spawn_host().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("ws connects");

    for id in ["s1", "s2"] {
        let sub = serde_json::json!({
            "type": "subscribe", "id": id,
            "protein": { "source": "record", "order": [ { "desc": "created_at" } ] }
        });
        ws.send(Message::Text(sub.to_string().into())).await.unwrap();
        let snap = next_json(&mut ws).await;
        assert_eq!(snap["type"], "snapshot", "{id} snapshot");
    }

    let act = serde_json::json!({
        "type": "act", "id": "c1",
        "action": { "action": "create-record", "kind": "plain", "head": "New", "body": "", "quantity": 0.0 }
    });
    ws.send(Message::Text(act.to_string().into())).await.unwrap();

    let mut updated = std::collections::HashSet::new();
    for _ in 0..8 {
        let m = tokio::time::timeout(std::time::Duration::from_secs(3), next_json(&mut ws))
            .await
            .expect("message arrives");
        if m["type"] == "update" {
            updated.insert(m["id"].as_str().unwrap().to_string());
        }
        if updated.len() == 2 {
            break;
        }
    }
    assert!(updated.contains("s1") && updated.contains("s2"),
        "both subscriptions got a live update, got: {updated:?}");
}

/// The board-chrome surface (Stage 8b §4): the shell, the new membrane bootstrap,
/// and the six REUSED pure modules all serve, and the bootstrap actually wires
/// them. Guards the reach-in `include_str!` route wiring against regressions
/// (rendering is verified separately in a browser; this is the cheap net).
#[tokio::test]
async fn board_chrome_surface_serves_and_wires_reused_modules() {
    let addr = spawn_http().await;

    // Shell loads the bootstrap and CSS.
    let (code, shell) = http_get(&addr, "/").await;
    assert_eq!(code, 200);
    assert!(shell.contains("/board/board.js"), "shell loads the membrane bootstrap");
    assert!(shell.contains("board-canvas") && shell.contains("cards-layer"),
        "shell carries the DOM contract the reused modules bind to");

    // The six reused modules + the two new membrane files all serve.
    for path in [
        "/board/board.js", "/board/board.css", "/board/bridge.js", "/board/frame.js",
        "/board/grid.js", "/board/store.js", "/board/viewport.js",
        "/board/interactions.js", "/board/group-logic.js", "/board/LynxDS-components.js",
        "/sand/todo", "/sand/kanban", "/sand/relations",
    ] {
        let (code, _) = http_get(&addr, path).await;
        assert_eq!(code, 200, "{path} serves");
    }

    // The bootstrap imports the reused modules (not a fork) and mounts sand
    // iframes over the bridge.
    let (_, boot) = http_get(&addr, "/board/board.js").await;
    for import in ["/board/grid.js", "/board/store.js", "/board/viewport.js",
                   "/board/interactions.js", "/board/group-logic.js", "/board/bridge.js"] {
        assert!(boot.contains(import), "bootstrap imports {import}");
    }
    assert!(boot.contains("/sand/table"), "bootstrap seeds the table sand card");
    assert!(boot.contains("/sand/todo"), "bootstrap seeds the todo sand card");
    assert!(boot.contains("/sand/kanban"), "bootstrap seeds the kanban sand card");
    assert!(boot.contains("/sand/relations"), "bootstrap seeds the relations sand card");

    // A reused module is byte-identical to web's source (single source of truth).
    let (_, grid) = http_get(&addr, "/board/grid.js").await;
    assert!(grid.contains("export function createGridConfig"),
        "grid.js is the real reused module, served under /board/ so imports resolve");
}
