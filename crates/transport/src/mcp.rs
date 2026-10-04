mod stdio;
pub use stdio::bridge_stdio;

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use fiote::{config::Secret, tools::Registry};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use tokio_util::sync::CancellationToken;

use crate::native::NativeTools;

#[derive(Clone)]
struct Handler {
    tools: Arc<Registry>,
    closed: CancellationToken,
}

impl Handler {
    fn definitions(&self) -> Vec<Tool> {
        self.tools
            .definitions()
            .into_iter()
            .map(|tool| {
                Tool::new(
                    tool.name,
                    tool.description,
                    tool.schema.as_object().cloned().unwrap_or_default(),
                )
            })
            .collect()
    }
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Use lince_describe first. After context compaction call lince_instructions to reload the complete pinned Fiote and ancestor instructions. Read through Protein and write through the provided Actions. Text edits merge through CRDT. Read IDs and retry receipts belong to this connection; do not retry uncertain operations in a new connection without checking their effects. This connection uses the local operator's access and expires when closed or the Cell stops.")
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: self.definitions(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.definitions()
            .into_iter()
            .find(|tool| tool.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if self.closed.is_cancelled() {
            return Ok(CallToolResult::structured_error(
                serde_json::json!({"ok":false,"error":"This Lince connection is closed."}),
            )
            .into());
        }
        let result = self
            .tools
            .run(
                &request.name,
                serde_json::Value::Object(request.arguments.unwrap_or_default()),
            )
            .await;
        Ok(if result["ok"] == true {
            CallToolResult::structured(result)
        } else {
            CallToolResult::structured_error(result)
        }
        .into())
    }
}

#[derive(Clone)]
struct Access {
    token: Secret,
    host: String,
    closed: CancellationToken,
}

async fn authorize(
    State(access): State<Access>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if access.closed.is_cancelled() {
        return Err(StatusCode::GONE);
    }
    if request.headers().contains_key("origin")
        || request
            .headers()
            .get("host")
            .and_then(|value| value.to_str().ok())
            != Some(access.host.as_str())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let expected = format!("Bearer {}", access.token.0);
    if request.headers().get_all("authorization").iter().count() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if supplied.len() != expected.len()
        || supplied
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            != 0
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

pub struct Connection {
    pub url: String,
    pub token: Secret,
    closed: CancellationToken,
    server: tokio::task::JoinHandle<()>,
    native: NativeTools,
}

impl Connection {
    pub async fn open(native: NativeTools) -> Result<Self, String> {
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(|_| "Cannot generate an agent connection token.")?;
        let token = Secret(URL_SAFE_NO_PAD.encode(secret));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let closed = CancellationToken::new();
        let mut registry = Registry::default();
        native.register(&mut registry);
        let handler = Handler {
            tools: Arc::new(registry),
            closed: closed.clone(),
        };
        let mut config = StreamableHttpServerConfig::default()
            .with_cancellation_token(closed.clone())
            .with_max_request_body_bytes(8 * 1024 * 1024);
        config.legacy_session_mode = false;
        config.json_response = true;
        let service = StreamableHttpService::new(
            move || Ok(handler.clone()),
            Arc::new(LocalSessionManager::default()),
            config,
        );
        let router = axum::Router::new().nest_service("/mcp", service).layer(
            middleware::from_fn_with_state(
                Access {
                    token: token.clone(),
                    host: address.to_string(),
                    closed: closed.clone(),
                },
                authorize,
            ),
        );
        let shutdown = closed.clone();
        let cleanup = native.clone();
        let server = tokio::spawn(async move {
            let stopped = shutdown.clone();
            let result = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    stopped.cancelled().await;
                    cleanup.close().await;
                })
                .await;
            if let Err(error) = result {
                tracing::error!(%error, "Agent tool connection stopped");
            }
            shutdown.cancel();
        });
        Ok(Self {
            url: format!("http://{address}/mcp"),
            token,
            closed,
            server,
            native,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.is_cancelled()
    }

    pub async fn close(&self) {
        self.closed.cancel();
        self.native.close().await;
        self.server.abort();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.closed.cancel();
        self.server.abort();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let native = self.native.clone();
            runtime.spawn(async move {
                native.close().await;
            });
        }
    }
}

#[cfg(test)]
mod stdio_tests {
    use super::*;
    use rmcp::ServiceExt;

    #[tokio::test]
    #[ignore = "Requires LINCE_MCP_TEST_BIN pointing to the current Lince executable"]
    async fn separate_stdio_process_forwards_http_actions_and_enforces_revocation() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let native = crate::Session::local(engine, Arc::new(crate::LaneHub::new()), nucleus::new_uid("agent-tools")).into_native_tools(crate::native::Context { agent:nucleus::new_uid("r"), record:nucleus::new_uid("r"), thread:nucleus::new_uid("r") });
        let connection = Connection::open(native).await.unwrap();
        let denied = reqwest::Client::new().post(&connection.url).bearer_auth("invalid-token").send().await.unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
        let mut child = tokio::process::Command::new(std::env::var("LINCE_MCP_TEST_BIN").expect("LINCE_MCP_TEST_BIN"))
            .arg("--mcp-stdio").env("LINCE_MCP_URL", &connection.url).env("LINCE_MCP_TOKEN", &connection.token.0)
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).kill_on_drop(true).spawn().unwrap();
        let client = ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap())).await.unwrap();
        assert!(client.list_tools(None).await.unwrap().tools.iter().any(|tool| tool.name == "lince_action"));
        let action = engine::actions::Action::CreateRecord { slug:Some("stdio-acceptance".into()), kind:nucleus::RecordKind::Plain, head:"Actual stdio action".into(), body:"Committed through the proxy".into(), quantity:1.0 };
        let request = serde_json::from_value(serde_json::json!({"name":"lince_action","arguments":{"request_id":"create-stdio-record","action":serde_json::to_string(&action).unwrap(),"read_ids":[]}})).unwrap();
        let result = client.call_tool(request).await.unwrap();
        assert!(!result.is_error.unwrap_or(false), "{result:?}");
        let protein = serde_json::json!({"source":"record","where":[{"slug_eq":"stdio-acceptance"}],"fields":["head","body"],"limit":1});
        let request = serde_json::from_value(serde_json::json!({"name":"lince_query","arguments":{"protein":protein.to_string()}})).unwrap();
        let result = client.call_tool(request).await.unwrap();
        assert!(!result.is_error.unwrap_or(false));
        assert!(serde_json::to_string(&result).unwrap().contains("Committed through the proxy"));
        connection.close().await;
        let request = serde_json::from_value(serde_json::json!({"name":"lince_describe","arguments":{}})).unwrap();
        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), client.call_tool(request)).await.unwrap();
        assert!(closed.is_err() || closed.unwrap().is_error.unwrap_or(false));
        let _ = client.cancel().await;
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    #[tokio::test]
    async fn stdio_sdk_serves_normal_tools_and_enforces_revocation_without_sockets() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let native = crate::Session::local(engine, Arc::new(crate::LaneHub::new()), nucleus::new_uid("agent-tools")).into_native_tools(crate::native::Context { agent: nucleus::new_uid("r"), record: nucleus::new_uid("r"), thread: nucleus::new_uid("r") });
        let mut tools = Registry::default();
        native.register(&mut tools);
        let closed = CancellationToken::new();
        let handler = Handler { tools: Arc::new(tools), closed: closed.clone() };
        let (server_io, client_io) = tokio::io::duplex(256 * 1024);
        let server = tokio::spawn(async move { handler.serve(server_io).await.unwrap().waiting().await.unwrap(); });
        let client = ().serve(client_io).await.unwrap();
        let tools = client.list_tools(None).await.unwrap();
        assert!(tools.tools.iter().any(|tool| tool.name == "lince_action"));
        let request = serde_json::from_value(serde_json::json!({"name":"lince_describe","arguments":{}})).unwrap();
        let result = client.call_tool(request).await.unwrap();
        assert!(!result.is_error.unwrap_or(false));
        closed.cancel();
        let request = serde_json::from_value(serde_json::json!({"name":"lince_describe","arguments":{}})).unwrap();
        let result = client.call_tool(request).await.unwrap();
        assert!(result.is_error.unwrap_or(false));
        client.cancel().await.unwrap();
        server.await.unwrap();
    }
}
