use rmcp::{
    ErrorData, Peer, RoleClient, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
    transport::{
        stdio,
        streamable_http_client::{
            StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
        },
    },
};

struct Proxy {
    peer: Peer<RoleClient>,
    tools: Vec<Tool>,
}

impl ServerHandler for Proxy {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("This stdio connection forwards authorized Lince tools to the running Cell. Use lince_describe first. Connection scope, permissions, receipts and revocation remain owned by that Cell.")
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|tool| tool.name == name).cloned()
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: self.tools.clone(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.peer
            .call_tool(request)
            .await
            .map(Into::into)
            .map_err(|_| {
                ErrorData::internal_error(
                    "The scoped Lince tool connection failed or closed.",
                    None,
                )
            })
    }
}

fn endpoint(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid local Lince tool endpoint.")?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || url.path() != "/mcp"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "The stdio bridge only connects to a scoped Lince endpoint on 127.0.0.1.".into(),
        );
    }
    Ok(url.to_string())
}

pub async fn bridge_stdio() -> Result<(), String> {
    let url = endpoint(
        &std::env::var("LINCE_MCP_URL").map_err(|_| "A scoped Lince tool endpoint is required.")?,
    )?;
    let token = fiote::config::Secret(
        std::env::var("LINCE_MCP_TOKEN")
            .map_err(|_| "A scoped Lince connection token is required.")?,
    );
    if token.0.is_empty() || token.0.len() > 256 || token.0.chars().any(char::is_control) {
        return Err("Invalid Lince tool connection token.".into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| "Cannot create the local tool connection.")?;
    let transport = StreamableHttpClientTransport::with_client(
        client,
        StreamableHttpClientTransportConfig::with_uri(url).auth_header(token.0.clone()),
    );
    let service = ()
        .serve(transport)
        .await
        .map_err(|_| "Cannot open the scoped Lince tool connection.")?;
    let tools = service
        .list_tools(None)
        .await
        .map_err(|_| "Cannot discover Lince tools.")?
        .tools;
    let proxy = Proxy {
        peer: service.peer().clone(),
        tools,
    };
    proxy
        .serve(stdio())
        .await
        .map_err(|_| "Cannot open the stdio tool connection.")?
        .waiting()
        .await
        .map_err(|_| "The stdio tool connection stopped unexpectedly.")?;
    service
        .cancel()
        .await
        .map_err(|_| "Cannot close the local tool connection.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_never_forwards_tokens_to_remote_or_credential_urls() {
        assert!(endpoint("http://127.0.0.1:1234/mcp").is_ok());
        for url in [
            "https://example.com/mcp",
            "http://localhost:1234/mcp",
            "http://127.0.0.1:1234/other",
            "http://user@127.0.0.1:1234/mcp",
            "http://127.0.0.1:1234/mcp?key=x",
        ] {
            assert!(endpoint(url).is_err());
        }
    }
}
