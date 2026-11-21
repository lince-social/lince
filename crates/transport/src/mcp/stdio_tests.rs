use std::{env::var, process::Stdio, sync::Arc, time::Duration};

use engine::{Engine, actions::Action};
use fiote::tools::Registry;
use nucleus::{RecordKind, new_uid};
use reqwest::{Client, StatusCode};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::{Map, json, to_string};
use tokio::{io::duplex, process::Command, spawn, time::timeout};
use tokio_util::sync::CancellationToken;

use super::{Connection, Handler};
use crate::{
    LaneHub, Session,
    native::{Context, NativeTools},
};

async fn native_tools() -> NativeTools {
    let engine = Arc::new(Engine::open_memory().await.expect("open Cell"));
    Session::local(engine, Arc::new(LaneHub::new()), new_uid("agent-tools")).into_native_tools(
        Context {
            agent: new_uid("r"),
            record: new_uid("r"),
            thread: new_uid("r"),
        },
    )
}

fn describe() -> CallToolRequestParams {
    CallToolRequestParams::new("lince_describe").with_arguments(Map::new())
}

#[tokio::test]
#[ignore = "Requires LINCE_MCP_TEST_BIN pointing to the current Lince executable"]
async fn separate_stdio_process_forwards_http_actions_and_enforces_revocation() {
    let connection = Connection::open(native_tools().await)
        .await
        .expect("open local MCP server");
    let denied = Client::new()
        .post(&connection.url)
        .bearer_auth("invalid-token")
        .send()
        .await
        .expect("test credential rejection");
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    let mut child = Command::new(var("LINCE_MCP_TEST_BIN").expect("LINCE_MCP_TEST_BIN"))
        .arg("--mcp-stdio")
        .env("LINCE_MCP_URL", &connection.url)
        .env("LINCE_MCP_TOKEN", &connection.token.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("start the stdio bridge");
    let stdout = child.stdout.take().expect("read bridge output");
    let stdin = child.stdin.take().expect("write bridge input");
    let client = ().serve((stdout, stdin)).await.expect("initialize MCP client");
    let tools = client.list_tools(None).await.expect("list bridge tools");
    assert!(tools.tools.iter().any(|tool| tool.name == "lince_action"));

    let action = Action::CreateRecord {
        slug: Some("stdio-acceptance".into()),
        kind: RecordKind::Plain,
        head: "Actual stdio action".into(),
        body: "Committed through the proxy".into(),
        quantity: 1.0,
    };
    let request = CallToolRequestParams::new("lince_action").with_arguments(Map::from_iter([
        ("request_id".into(), "create-stdio-record".into()),
        (
            "action".into(),
            to_string(&action).expect("encode Action").into(),
        ),
        ("read_ids".into(), json!([])),
    ]));
    let result = client
        .call_tool(request)
        .await
        .expect("run a bridged Action");
    assert!(!result.is_error.unwrap_or_default(), "{result:?}");

    let protein = json!({
        "source":"record",
        "where":[{"slug_eq":"stdio-acceptance"}],
        "fields":["head","body"],
        "limit":1
    });
    let request = CallToolRequestParams::new("lince_query").with_arguments(Map::from_iter([(
        "protein".into(),
        protein.to_string().into(),
    )]));
    let result = client
        .call_tool(request)
        .await
        .expect("run a bridged query");
    assert!(!result.is_error.unwrap_or_default());
    assert!(
        to_string(&result)
            .expect("encode query result")
            .contains("Committed through the proxy")
    );

    connection.close().await;
    let closed = timeout(Duration::from_secs(5), client.call_tool(describe()))
        .await
        .expect("revoked connection responds promptly");
    assert!(
        closed
            .map(|reply| reply.is_error.unwrap_or_default())
            .unwrap_or(true)
    );
    let _ = client.cancel().await;
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[tokio::test]
async fn stdio_sdk_serves_normal_tools_and_enforces_revocation_without_sockets() {
    let mut tools = Registry::default();
    native_tools().await.register(&mut tools);
    let closed = CancellationToken::new();
    let handler = Handler::new(tools, closed.clone());
    let (server_io, client_io) = duplex(256 * 1024);
    let server = spawn(async move {
        handler
            .serve(server_io)
            .await
            .expect("initialize MCP server")
            .waiting()
            .await
            .expect("shut down MCP server");
    });
    let client = ().serve(client_io).await.expect("initialize MCP client");
    let tools = client
        .list_tools(None)
        .await
        .expect("list in-memory MCP tools");
    assert!(tools.tools.iter().any(|tool| tool.name == "lince_action"));
    let result = client
        .call_tool(describe())
        .await
        .expect("describe native tools");
    assert!(!result.is_error.unwrap_or_default());
    closed.cancel();
    let result = client
        .call_tool(describe())
        .await
        .expect("refuse a revoked request");
    assert!(result.is_error.unwrap_or_default());
    client.cancel().await.expect("stop MCP client");
    server.await.expect("join MCP server");
}
