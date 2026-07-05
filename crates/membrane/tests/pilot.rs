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
