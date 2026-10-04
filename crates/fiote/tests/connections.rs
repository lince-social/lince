use fiote::{
    adapters::Catalog,
    communication::{
        check::{Stage, probe},
        connection::{Connection, Profile, Support},
    },
    config::{ProviderKind, Secret, Settings},
};

#[tokio::test]
async fn a_missing_executable_is_a_diagnostic_and_external_tools_do_not_claim_chat() {
    let root = tempfile::tempdir().unwrap();
    let catalog = Catalog::load(root.path()).await.unwrap();
    let config = serde_json::from_value(serde_json::json!({
        "command":root.path().join("missing"),"args":[],"directory":root.path(),
        "environment":{},"session_meta":{},"options":{},"require_vault":false
    }))
    .unwrap();
    let mut profile = Profile {
        id: "own-ai".into(),
        name: "My AI".into(),
        connection: Connection::Harness { config },
    };
    let check = probe(&profile, &catalog, None).await;
    assert!(!check.ready);
    assert_eq!(check.stage, Stage::Executable);
    assert!(check.detail.contains("missing"));
    profile.connection = Connection::External;
    let check = probe(&profile, &catalog, None).await;
    assert!(check.ready);
    assert_eq!(check.capabilities.tools, Support::Supported);
    assert_eq!(
        check.capabilities.automatic_activation,
        Support::Unsupported
    );
    assert_eq!(check.capabilities.text, Support::Unknown);
}

#[tokio::test]
async fn api_model_discovery_uses_the_configured_endpoint_without_an_inference_request() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let root = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 1024];
            let read = socket.read(&mut chunk).await.unwrap();
            assert!(read > 0);
            bytes.extend_from_slice(&chunk[..read]);
            if bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                break;
            }
            assert!(bytes.len() <= 16384);
        }
        let request = String::from_utf8(bytes).unwrap();
        assert!(request.starts_with("GET /v1/models "), "{request}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-key")
        );
        let body = r#"{"object":"list","data":[{"id":"my-own-model","object":"model"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let catalog = Catalog::load(root.path()).await.unwrap();
    let profile = Profile {
        id: "api".into(),
        name: "My endpoint".into(),
        connection: Connection::Model {
            settings: Settings {
                provider: ProviderKind("openai".into()),
                model: "my-own-model".into(),
                endpoint: format!("http://{address}/v1/"),
                ..Default::default()
            },
        },
    };
    let no_key = probe(&profile, &catalog, None).await;
    assert!(!no_key.ready);
    assert_eq!(no_key.stage, Stage::Authentication);
    let check = probe(&profile, &catalog, Some(&Secret("fixture-key".into()))).await;
    assert!(check.ready, "{}", check.detail);
    assert_eq!(check.stage, Stage::ModelDiscovery);
    assert_eq!(check.settings["models"][0], "my-own-model");
    assert!(check.detail.contains("inference"));
    server.await.unwrap();
}

#[cfg(feature = "test-agent")]
#[tokio::test]
async fn generic_harness_probe_negotiates_settings_and_never_sends_a_prompt() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("requests");
    let config = serde_json::from_value(serde_json::json!({
        "command":env!("CARGO_BIN_EXE_lince-acp-test-agent"),"args":[],"directory":root.path(),
        "environment":{"TEST_NO_HTTP":"1","TEST_REQUEST_LOG":log},"session_meta":{},"options":{},"require_vault":false
    })).unwrap();
    let profile = Profile {
        id: "harness".into(),
        name: "My harness".into(),
        connection: Connection::Harness { config },
    };
    let check = probe(&profile, &Catalog::load(root.path()).await.unwrap(), None).await;
    assert!(check.ready, "{}", check.detail);
    assert_eq!(check.stage, Stage::Session);
    assert_eq!(check.capabilities.settings, Support::Supported);
    assert!(
        !std::fs::read_to_string(log)
            .unwrap()
            .contains("session/prompt")
    );
}

#[tokio::test]
async fn keyless_compatible_endpoint_can_discover_and_answer_without_a_vault_or_api_key() {
    use fiote::provider::{GenaiProvider, Message, Provider};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let root = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for path in ["GET /v1/models ", "POST /v1/chat/completions "] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let read = socket.read(&mut chunk).await.unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&chunk[..read]);
                if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8(bytes[..offset].to_vec()).unwrap();
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= offset + 4 + length {
                        break;
                    }
                }
                assert!(bytes.len() <= 65536);
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.starts_with(path), "{request}");
            for header in request.lines().take_while(|line| !line.is_empty()) {
                if header.to_ascii_lowercase().starts_with("authorization:") {
                    assert_eq!(header.trim(), "authorization: Bearer");
                }
            }
            let body = if path.starts_with("GET") {
                r#"{"data":[{"id":"local-model"}]}"#
            } else {
                r#"{"id":"local","object":"chat.completion","created":1,"model":"local-model","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Local hello"}}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}}"#
            };
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    let mut settings = Settings {
        provider: ProviderKind("openai".into()),
        auth_method: "none".into(),
        model: "local-model".into(),
        endpoint: format!("http://{address}/v1/"),
        ..Default::default()
    };
    let catalog = Catalog::load(root.path()).await.unwrap();
    catalog.validate(&mut settings).unwrap();
    let profile = Profile {
        id: "local".into(),
        name: "My local setup".into(),
        connection: Connection::Model {
            settings: settings.clone(),
        },
    };
    let checked = probe(&profile, &catalog, None).await;
    assert!(checked.ready, "{}", checked.detail);
    let reply = GenaiProvider::new(&settings, &Secret::default())
        .unwrap()
        .complete("Be helpful.", &[Message::User("Hello".into())], &[])
        .await
        .unwrap();
    assert_eq!(reply.text, "Local hello");
    server.await.unwrap();
}
