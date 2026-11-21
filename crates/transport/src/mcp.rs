mod stdio;
pub use stdio::bridge_stdio;

use std::{fmt, sync::Arc};

use axum::{
    Router,
    extract::{Request, State},
    http::{
        StatusCode,
        header::{AUTHORIZATION, HOST, ORIGIN},
    },
    middleware::{Next, from_fn_with_state},
    response::Response,
    serve,
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
use serde_json::{Map, Value, json};
use tokio::{net::TcpListener, runtime::Handle, spawn, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::native::NativeTools;

const MCP_PATH: &str = "/mcp";
const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
struct Handler {
    tools: Arc<Registry>,
    definitions: Arc<[Tool]>,
    closed: CancellationToken,
}

impl Handler {
    fn new(tools: Registry, closed: CancellationToken) -> Self {
        let definitions = tools
            .definitions()
            .into_iter()
            .map(|tool| {
                let schema = match tool.schema {
                    Value::Object(schema) => schema,
                    _ => Map::new(),
                };
                Tool::new(tool.name, tool.description, schema)
            })
            .collect();
        Self {
            tools: Arc::new(tools),
            definitions,
            closed,
        }
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
            tools: self.definitions.to_vec(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.definitions
            .iter()
            .find(|tool| tool.name == name)
            .cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if self.closed.is_cancelled() {
            return Ok(CallToolResult::structured_error(
                json!({"ok":false,"error":"This Lince connection is closed."}),
            )
            .into());
        }
        let result = self
            .tools
            .run(
                &request.name,
                Value::Object(request.arguments.unwrap_or_default()),
            )
            .await;
        Ok(if result.get("ok").and_then(Value::as_bool) == Some(true) {
            CallToolResult::structured(result)
        } else {
            CallToolResult::structured_error(result)
        }
        .into())
    }
}

struct Access {
    authorization: Secret,
    host: String,
    closed: CancellationToken,
}

async fn authorize(
    State(access): State<Arc<Access>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if access.closed.is_cancelled() {
        return Err(StatusCode::GONE);
    }
    let headers = request.headers();
    if headers.contains_key(ORIGIN)
        || headers.get(HOST).and_then(|value| value.to_str().ok()) != Some(access.host.as_str())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut authorization = headers.get_all(AUTHORIZATION).iter();
    let Some(supplied) = authorization.next() else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let supplied = supplied.as_bytes();
    let expected = access.authorization.0.as_bytes();
    // Compare every byte for equal-length headers, regardless of where they differ.
    if authorization.next().is_some()
        || supplied.len() != expected.len()
        || supplied
            .iter()
            .zip(expected)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            != 0
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

/// A local MCP server and the credential issued for that connection.
pub struct Connection {
    /// Loopback URL for the MCP endpoint.
    pub url: String,
    /// Bearer token that authorizes requests to this connection.
    pub token: Secret,
    closed: CancellationToken,
    server: JoinHandle<()>,
    native: NativeTools,
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// Starts a loopback MCP server for the supplied tool scope.
    ///
    /// # Errors
    /// Returns a diagnostic if token generation fails or the listener cannot be opened.
    pub async fn open(native: NativeTools) -> Result<Self, String> {
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(|_| "Cannot generate an agent connection token.")?;
        let token = Secret(URL_SAFE_NO_PAD.encode(secret));
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let closed = CancellationToken::new();
        let mut registry = Registry::default();
        native.register(&mut registry);
        let handler = Handler::new(registry, closed.clone());
        let mut config = StreamableHttpServerConfig::default()
            .with_cancellation_token(closed.clone())
            .with_max_request_body_bytes(MAX_REQUEST_BYTES);
        config.legacy_session_mode = false;
        config.json_response = true;
        let service = StreamableHttpService::new(
            move || Ok(handler.clone()),
            Arc::new(LocalSessionManager::default()),
            config,
        );
        let router = Router::new()
            .nest_service(MCP_PATH, service)
            .layer(from_fn_with_state(
                Arc::new(Access {
                    authorization: Secret(format!("Bearer {}", token.0)),
                    host: address.to_string(),
                    closed: closed.clone(),
                }),
                authorize,
            ));
        let shutdown = closed.clone();
        let stopped = closed.clone();
        let cleanup = native.clone();
        let server = spawn(async move {
            let result = serve(listener, router)
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
            url: format!("http://{address}{MCP_PATH}"),
            token,
            closed,
            server,
            native,
        })
    }

    /// Reports whether this connection has stopped accepting tool calls.
    pub fn is_closed(&self) -> bool {
        self.closed.is_cancelled()
    }

    /// Revokes tool access and stops pending native work for this connection.
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
        if let Ok(runtime) = Handle::try_current() {
            let native = self.native.clone();
            runtime.spawn(async move {
                native.close().await;
            });
        }
    }
}

#[cfg(test)]
mod stdio_tests;
