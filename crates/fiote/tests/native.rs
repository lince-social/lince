use fiote::{
    communication::{
        auth::{Account, Auth, MemoryCredential, Session},
        chatgpt::ChatGpt,
    },
    config::{Secret, Settings},
    provider::{Message, Provider, TextOutput, ToolDefinition},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};

fn account() -> Secret {
    Account {
        client_id: "oaiapp_fixture".into(),
        subject: "fixture-user".into(),
        email: None,
        host_id: "urn:uuid:fixture".into(),
        access_token: Secret("fixture-access".into()),
        refresh_token: Secret("fixture-refresh".into()),
        id_token: Secret::default(),
        scopes: vec!["chatgpt.tokens.use.direct".into()],
        expires_at: u64::MAX,
    }
    .encode()
    .unwrap()
}
async fn read(socket: &mut tokio::net::TcpStream) -> (String, Value) {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let size = socket.read(&mut chunk).await.unwrap();
        assert!(size > 0);
        bytes.extend_from_slice(&chunk[..size]);
        if let Some(split) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let header = String::from_utf8(bytes[..split].to_vec()).unwrap();
            let length = header
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|s| s.parse::<usize>().ok())
                })
                .unwrap_or_default();
            if bytes.len() >= split + 4 + length {
                return (
                    header,
                    serde_json::from_slice(&bytes[split + 4..split + 4 + length])
                        .unwrap_or(Value::Null),
                );
            }
        }
        assert!(bytes.len() < 1024 * 1024);
    }
}
fn event(value: Value) -> String {
    format!("data: {value}\n\n")
}
fn completed(text: &str, calls: bool) -> Value {
    let output = if calls {
        vec![
            json!({"type":"reasoning","id":"rs_one","encrypted_content":"opaque-encrypted"}),
            json!({"type":"function_call","id":"fc_one","call_id":"one","name":"lince_read_record","namespace":"lince","arguments":"{\"record_uid\":\"r_1\"}"}),
        ]
    } else {
        vec![
            json!({"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[]}]}),
        ]
    };
    json!({"type":"response.completed","response":{"status":"completed","output":output,"usage":{"input_tokens":7,"output_tokens":3,"total_tokens":10}}})
}
fn provider(endpoint: &str) -> ChatGpt {
    ChatGpt::at(
        Settings {
            model: "fixture-model".into(),
            reasoning: Some("low".into()),
            ..Default::default()
        },
        Session {
            auth: Auth::production().unwrap(),
            store: Arc::new(MemoryCredential(Mutex::new(account()))),
            lock: Default::default(),
        },
        endpoint,
    )
    .unwrap()
}
struct Output {
    text: Mutex<String>,
    early: Arc<tokio::sync::Notify>,
}
#[async_trait::async_trait]
impl TextOutput for Output {
    async fn update(&self, text: &str) -> Result<(), String> {
        *self.text.lock().await = text.into();
        if text == "Hello" {
            self.early.notify_one();
        }
        Ok(())
    }
}

#[tokio::test]
async fn native_subscription_streams_validates_contract_and_replays_namespaced_tools() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let early = Arc::new(tokio::sync::Notify::new());
    let received = early.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let (headers, request) = read(&mut socket).await;
        assert!(headers.starts_with("POST /v1/responses "));
        assert!(headers.to_lowercase().contains("accept: text/event-stream"));
        assert!(
            headers
                .to_lowercase()
                .contains("authorization: bearer fixture-access")
        );
        assert_eq!(request["stream"], true);
        assert_eq!(request["store"], false);
        assert_eq!(request["reasoning"]["effort"], "low");
        assert_eq!(request["tools"][0]["type"], "namespace");
        assert_eq!(request["tools"][0]["name"], "lince");
        assert_eq!(request["instructions"], "Use authorized Actions.");
        for field in [
            "max_output_tokens",
            "previous_response_id",
            "temperature",
            "conversation",
            "background",
            "user",
        ] {
            assert!(request.get(field).is_none());
        }
        let first = event(json!({"type":"response.output_text.delta","delta":"Hello"}));
        let rest = event(
            json!({"type":"response.function_call_arguments.delta","delta":"{\"record_"}),
        ) + &event(
            json!({"type":"response.function_call_arguments.delta","delta":"uid\":\"r_1\"}"}),
        ) + &event(completed("", true));
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{first}",first.len()+rest.len()).as_bytes()).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), received.notified())
            .await
            .unwrap();
        for part in rest.as_bytes().chunks(7) {
            socket.write_all(part).await.unwrap();
        }
        let (mut socket, _) = listener.accept().await.unwrap();
        let (_, request) = read(&mut socket).await;
        assert!(
            request["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["encrypted_content"] == "opaque-encrypted")
        );
        assert_eq!(
            request["input"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["type"] == "function_call")
                .count(),
            1
        );
        assert!(
            request["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == "one")
        );
        let data = event(completed("Done", false));
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",data.len()).as_bytes()).await.unwrap();
    });
    let provider = provider(&endpoint);
    let output = Output {
        text: Default::default(),
        early,
    };
    let tool = ToolDefinition {
        name: "lince_read_record".into(),
        description: "Read a Record".into(),
        schema: json!({"type":"object"}),
    };
    let reply = provider
        .stream(
            "Use authorized Actions.",
            &[Message::User("Hello".into())],
            std::slice::from_ref(&tool),
            &output,
        )
        .await
        .unwrap();
    assert_eq!(reply.calls[0].arguments["record_uid"], "r_1");
    assert_eq!(reply.usage.as_ref().unwrap().total_tokens, Some(10));
    let messages = vec![
        Message::User("Hello".into()),
        Message::Replay {
            provider: "chatgpt".into(),
            items: reply.replay,
        },
        Message::Assistant {
            text: reply.text,
            calls: reply.calls,
        },
        Message::Tool {
            id: "one".into(),
            name: "lince_read_record".into(),
            result: json!({"ok":true}),
        },
    ];
    assert_eq!(
        provider
            .complete("Use authorized Actions.", &messages, &[tool])
            .await
            .unwrap()
            .text,
        "Done"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn native_stream_accepts_missing_content_type_only_with_a_completed_event() {
    for complete in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read(&mut socket).await;
            let data = if complete {
                event(completed("Hello", false))
            } else {
                String::new()
            };
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",
                        data.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let result = provider(&endpoint)
            .complete("", &[Message::User("hello".into())], &[])
            .await;
        if complete {
            assert_eq!(result.unwrap().text, "Hello");
        } else {
            assert!(result.unwrap_err().contains("without response.completed"));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn native_finalized_output_items_require_successful_terminal_completion() {
    for status in ["completed", "incomplete"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read(&mut socket).await;
            let mut data = String::new();
            for (index, item) in completed("", true)["response"]["output"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                data += &event(
                    json!({"type":"response.output_item.done","output_index":index,"item":item}),
                );
            }
            data += &event(
                json!({"type":"response.completed","response":{"status":status,"output":[],"usage":{"total_tokens":10}}}),
            );
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",data.len()).as_bytes()).await.unwrap();
        });
        let result = provider(&endpoint)
            .complete("", &[Message::User("hello".into())], &[])
            .await;
        if status == "completed" {
            let reply = result.unwrap();
            assert_eq!(reply.calls[0].arguments["record_uid"], "r_1");
            assert_eq!(reply.replay[0]["encrypted_content"], "opaque-encrypted");
            assert_eq!(reply.usage.unwrap().total_tokens, Some(10));
        } else {
            assert!(result.unwrap_err().contains("not completed"));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn native_stream_rejects_late_quota_incomplete_eof_and_malformed_tools() {
    for terminal in [
        Some(
            json!({"type":"response.failed","response":{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}}),
        ),
        Some(json!({"type":"response.incomplete"})),
        Some(completed("", false)),
        None,
        Some(
            json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"function_call","call_id":"one","name":"change","arguments":"{bad"}]}}),
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read(&mut socket).await;
            let mut data = event(json!({"type":"response.output_text.delta","delta":"Hello"}));
            if let Some(terminal) = terminal {
                data += &event(terminal);
            }
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",data.len()).as_bytes()).await.unwrap();
        });
        let output = Output {
            text: Default::default(),
            early: Default::default(),
        };
        let result = provider(&endpoint)
            .stream("", &[Message::User("hello".into())], &[], &output)
            .await;
        assert!(result.is_err());
        assert_eq!(*output.text.lock().await, "Hello");
        server.await.unwrap();
    }
}

#[tokio::test]
async fn native_models_preserve_account_order_and_advertised_settings() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let (headers, _) = read(&mut socket).await;
        assert!(headers.starts_with("GET /v1/models "));
        let data = json!({"models":[{"slug":"second","display_name":"Second","visibility":"list","supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}],"supports_fast_mode":true},{"slug":"hidden","visibility":"hide"},{"slug":"first","display_name":"First","visibility":"list"}]}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",data.len()).as_bytes()).await.unwrap();
    });
    let models = provider(&endpoint).models().await.unwrap();
    assert_eq!(
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["second", "first"]
    );
    assert_eq!(models[0].reasoning, ["low", "high"]);
    assert!(models[0].fast);
    server.await.unwrap();
}

#[tokio::test]
async fn native_json_responses_require_completed_status_and_preserve_tools_and_usage() {
    for (status, calls) in [
        ("completed", false),
        ("completed", true),
        ("incomplete", true),
        ("failed", true),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (headers, request) = read(&mut socket).await;
            assert!(headers.to_lowercase().contains("accept: text/event-stream"));
            assert_eq!(request["stream"], true);
            let mut response = completed("Completed JSON hello", calls)["response"].clone();
            response["object"] = json!("response");
            response["status"] = json!(status);
            let body = response.to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        });
        let reply = provider(&endpoint)
            .complete("", &[Message::User("hello".into())], &[])
            .await;
        if status == "completed" {
            let reply = reply.unwrap();
            assert_eq!(reply.calls.len(), usize::from(calls));
            assert_eq!(reply.usage.as_ref().unwrap().total_tokens, Some(10));
            if !calls {
                assert_eq!(reply.text, "Completed JSON hello");
            }
        } else {
            assert!(reply.unwrap_err().contains("terminal status"));
        }
        server.await.unwrap();
    }
}

#[test]
fn native_attachment_capabilities_reject_audio_and_video_before_inference() {
    let provider = provider("http://127.0.0.1:1/v1/");
    for (mime, allowed) in [
        ("text/plain", true),
        ("text/csv", true),
        ("image/png", true),
        ("application/pdf", true),
        ("audio/wav", false),
        ("video/mp4", false),
    ] {
        let content = [nucleus::message::MessagePart::Attachment {
            name: "fixture".into(),
            mime_type: mime.into(),
            data: "eA==".into(),
        }];
        assert_eq!(
            provider.validate_content(&content).is_ok(),
            allowed,
            "{mime}"
        );
    }
}

#[tokio::test]
async fn broken_optional_adapters_do_not_disable_native_catalog() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("providers.json"),
        br#"[{"executable":"/missing/adapter","arguments":[]},{"broken":true}]"#,
    )
    .unwrap();
    let catalog = fiote::adapters::Catalog::load(directory.path())
        .await
        .unwrap();
    assert!(catalog.descriptors.iter().any(|d| d.id.0 == "chatgpt"));
    assert!(catalog.descriptors.iter().any(|d| d.id.0 == "openai"));
    assert_eq!(catalog.diagnostics.len(), 2);
    std::fs::write(directory.path().join("providers.json"), b"invalid").unwrap();
    assert!(
        !fiote::adapters::Catalog::load(directory.path())
            .await
            .unwrap()
            .descriptors
            .is_empty()
    );
}

#[tokio::test]
async fn native_retry_is_bounded_and_does_not_retry_plan_quota_errors() {
    for quota in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..if quota { 1 } else { 3 } {
                let (mut socket, _) = listener.accept().await.unwrap();
                read(&mut socket).await;
                let body = json!({"error":{"code":if quota {
                    "subscription_sharing_usage_limit_exceeded"
                } else {
                    "rate_limit_exceeded"
                }}})
                .to_string();
                socket.write_all(format!("HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let error = provider(&endpoint)
            .complete("", &[Message::User("hello".into())], &[])
            .await
            .unwrap_err();
        assert!(error.contains(if quota {
            "plan usage limit"
        } else {
            "rate limited"
        }));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn native_stream_cancels_after_partial_text_without_requiring_a_terminal_event() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read(&mut socket).await;
        let body = event(json!({"type":"response.output_text.delta","delta":"Hello"}));
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 1048576\r\nConnection: close\r\n\r\n{body}").as_bytes()).await.unwrap();
        std::future::pending::<()>().await;
    });
    let output = Arc::new(Output {
        text: Default::default(),
        early: Default::default(),
    });
    let progress = output.clone();
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let run = tokio::spawn(async move {
        fiote::runtime::run_streamed(
            &provider(&endpoint),
            "",
            vec![Message::User("hello".into())],
            &fiote::tools::Registry::default(),
            receiver,
            progress.as_ref(),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), output.early.notified())
        .await
        .unwrap();
    stop.send(true).unwrap();
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), run)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.contains("Stopped by you"));
    assert_eq!(*output.text.lock().await, "Hello");
    server.abort();
}
