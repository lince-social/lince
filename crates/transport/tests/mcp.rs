#![cfg(feature = "mcp")]

use std::sync::Arc;

use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use serde_json::{Value, json};
use transport::{LaneHub, Session, mcp::Connection, native::Context};

struct Client {
    http: reqwest::Client,
    connection: Connection,
}

impl Client {
    async fn rpc(&self, method: &str, params: Value) -> Value {
        let response = self
            .http
            .post(&self.connection.url)
            .bearer_auth(&self.connection.token.0)
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-06-18")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = response.text().await.unwrap();
        assert!(status.is_success(), "{status}: {body}");
        serde_json::from_str(&body).unwrap_or_else(|_| {
            let data = body
                .lines()
                .find_map(|line| {
                    line.strip_prefix("data: ")
                        .or_else(|| line.strip_prefix("data:"))
                })
                .unwrap();
            serde_json::from_str(data).unwrap()
        })
    }

    async fn tool(&self, name: &str, args: Value) -> Value {
        let response = self
            .rpc("tools/call", json!({"name":name,"arguments":args}))
            .await;
        assert!(response.get("error").is_none(), "{response}");
        let result = &response["result"];
        let value: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            result["isError"].as_bool().unwrap_or(false),
            value["ok"] == false
        );
        value
    }

    async fn ok(&self, name: &str, args: Value) -> Value {
        let result = self.tool(name, args).await;
        assert_eq!(result["ok"], true, "{result}");
        result["result"].clone()
    }

    async fn action(&self, action: Value, reads: Vec<Value>) -> Value {
        self.ok("lince_action", json!({"request_id":nucleus::new_uid("request"),"action":action.to_string(),"read_ids":reads})).await
    }
}

async fn fixture() -> (Arc<Engine>, Client, Context) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let agent = engine
        .act(
            Action::CreateAgent {
                head: "External Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let thread = engine
        .act(
            Action::CreateThread {
                target: agent.clone(),
                head: "Thread 1".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let context = Context {
        agent: agent.clone(),
        record: agent,
        thread,
    };
    let native = Session::local(engine.clone(), Arc::new(LaneHub::new()), "mcp-test")
        .into_native_tools(context.clone());
    let connection = Connection::open(native).await.unwrap();
    let client = Client {
        connection,
        http: reqwest::Client::new(),
    };
    let initialized = client.rpc("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}})).await;
    assert!(
        initialized["result"]["capabilities"]["tools"].is_object(),
        "{initialized}"
    );
    (engine, client, context)
}

#[tokio::test]
async fn mcp_reads_writes_assertions_and_crdt_use_live_native_operations() {
    let (engine, client, context) = fixture().await;
    let list = client.rpc("tools/list", json!({})).await;
    assert!(
        list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "lince_message")
    );
    let description = client.ok("lince_describe", json!({})).await;
    assert_eq!(description["context"]["thread"], context.thread);
    let created = client.action(json!({"action":"create-record","slug":null,"kind":"plain","head":"Task","body":"Hello world","quantity":0.0}), vec![]).await;
    let uid = created["created"].as_str().unwrap();
    let read = client
        .ok("lince_read_record", json!({"record_uid":uid}))
        .await;
    engine
        .act(
            Action::EditRecordText {
                target: uid.into(),
                head: None,
                body: Some("Hello world · human 👩‍💻".into()),
            },
            None,
        )
        .await
        .unwrap();
    let mut events = engine.subscribe();
    let edit = json!({"request_id":"edit-1","read_id":read["read_id"],"edits":[{"field":"body","before":"world","after":"agent"}]});
    let result = client.ok("lince_edit_text", edit.clone()).await;
    assert_eq!(client.ok("lince_edit_text", edit).await, result);
    assert_eq!(
        engine.doc_text(uid).await.unwrap().1,
        "Hello agent · human 👩‍💻"
    );
    let event = events.recv().await.unwrap();
    assert_eq!(event.cause.kind, nucleus::CauseKind::Fiote);
    let released = client
        .ok("lince_release_reads", json!({"read_ids":[read["read_id"]]}))
        .await;
    assert_eq!(released["released"], 1);
    let read = client
        .ok("lince_read_record", json!({"record_uid":uid}))
        .await;
    client
        .action(
            json!({"action":"set-quantity-exact","target":uid,"amount":"-3"}),
            vec![read["read_id"].clone()],
        )
        .await;
    let stale = client.tool("lince_action", json!({"request_id":"stale","action":json!({"action":"set-quantity-exact","target":uid,"amount":"1"}).to_string(),"read_ids":[read["read_id"]]})).await;
    assert_eq!(stale["ok"], false);
    let lingua = engine
        .act(
            Action::CreateLingua {
                name: "Task relations".into(),
                visibility: "public".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    for name in ["assigned-to", "wip"] {
        if store::concepts::resolve(&engine.store.pool, name)
            .await
            .unwrap()
            .is_none()
        {
            engine
                .act(
                    Action::CreateConcept {
                        lingua: lingua.clone(),
                        name: name.into(),
                        parents: vec![],
                    },
                    None,
                )
                .await
                .unwrap();
        }
        let predicate = store::concepts::resolve(&engine.store.pool, name)
            .await
            .unwrap()
            .unwrap();
        let read = client
            .ok("lince_read_record", json!({"record_uid":uid}))
            .await;
        client.action(json!({"action":"assert-record","subject":uid,"predicate":predicate,"object":if name == "assigned-to" { Some(context.agent.clone()) } else { None }}), vec![read["read_id"].clone()]).await;
    }
    let read = client
        .ok("lince_read_record", json!({"record_uid":uid}))
        .await;
    assert_eq!(read["record"]["quantity"], "-3");
    assert!(read["record"]["assertions"].as_array().unwrap().len() >= 2);
    let sync = client.ok("lince_sync_status", json!({})).await;
    assert_eq!(sync["type"], "sync_status");
    assert_eq!(engine.store.pool.is_closed(), false);
    assert_eq!(
        store::records::get(&engine.store.pool, uid)
            .await
            .unwrap()
            .unwrap()
            .kind,
        RecordKind::Plain.as_str()
    );
}

#[tokio::test]
async fn mcp_streamed_messages_preserve_human_edits_and_close_interrupts_writing() {
    let (engine, client, context) = fixture().await;
    let start = json!({"request_id":"start","operation":"start","text":"Hello"});
    let message = client.ok("lince_message", start.clone()).await;
    assert_eq!(client.ok("lince_message", start).await, message);
    let uid = message["message_uid"].as_str().unwrap();
    engine
        .act(
            Action::EditRecordText {
                target: uid.into(),
                head: None,
                body: Some("Hello · my note".into()),
            },
            None,
        )
        .await
        .unwrap();
    client.ok("lince_message", json!({"request_id":"update","operation":"update","message_uid":uid,"text":"Hello world"})).await;
    let body = engine.doc_text(uid).await.unwrap().1;
    assert!(body.contains("world") && body.contains("my note"), "{body}");
    client.ok("lince_message", json!({"request_id":"finish","operation":"finish","message_uid":uid,"text":"Hello world!"})).await;
    let final_body = engine.doc_text(uid).await.unwrap().1;
    assert!(
        final_body.contains("world!") && final_body.contains("my note"),
        "{final_body}"
    );
    let metadata = store::records::get_extension(&engine.store.pool, uid, "lince.message")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata["state"], "finished");
    assert_eq!(metadata["author"], context.agent);
    let late = client
        .tool(
            "lince_message",
            json!({"request_id":"late","operation":"update","message_uid":uid,"text":"Overwrite"}),
        )
        .await;
    assert_eq!(late["ok"], false);
    let pending = client
        .ok(
            "lince_message",
            json!({"request_id":"pending","operation":"start","text":"Partial reply"}),
        )
        .await;
    client.connection.close().await;
    let uid = pending["message_uid"].as_str().unwrap();
    assert_eq!(engine.doc_text(uid).await.unwrap().1, "Partial reply");
    assert_eq!(
        store::records::get_extension(&engine.store.pool, uid, "lince.message")
            .await
            .unwrap()
            .unwrap()["state"],
        "interrupted"
    );
    assert!(
        client
            .http
            .post(&client.connection.url)
            .bearer_auth(&client.connection.token.0)
            .send()
            .await
            .map_or(true, |response| !response.status().is_success())
    );
}

#[tokio::test]
async fn mcp_rejects_missing_credentials_browser_origins_wrong_hosts_and_large_bodies() {
    let (_, client, _) = fixture().await;
    for (header, value, expected) in [
        (
            "authorization",
            "Bearer wrong",
            reqwest::StatusCode::UNAUTHORIZED,
        ),
        (
            "origin",
            "https://other.example",
            reqwest::StatusCode::FORBIDDEN,
        ),
        ("host", "other.example", reqwest::StatusCode::FORBIDDEN),
    ] {
        let response = client
            .http
            .post(&client.connection.url)
            .bearer_auth(&client.connection.token.0)
            .header(header, value)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    assert_eq!(
        client
            .http
            .post(&client.connection.url)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let response = client
        .http
        .post(&client.connection.url)
        .bearer_auth(&client.connection.token.0)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body("x".repeat(300 * 1024))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success());
    assert_eq!(client.tool("not_a_tool", json!({})).await["ok"], false);
}
