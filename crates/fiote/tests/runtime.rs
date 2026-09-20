use async_trait::async_trait;
use fiote::{
    provider::{Message, Provider, Reply, ToolCall, ToolDefinition},
    runtime::run,
    tools::{CreateFile, Registry, Tool},
};
use serde_json::{Value, json};
use std::sync::Mutex;
use tokio::sync::watch;

struct Script(Mutex<Vec<Reply>>);

#[async_trait]
impl Provider for Script {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        assert_eq!(system, "Be helpful.");
        assert_eq!(tools[0].name, "create_file");
        if let Some(Message::Tool { result, .. }) = messages.last() {
            assert_eq!(result["ok"], true);
        }
        Ok(self.0.lock().unwrap().remove(0))
    }
}

fn call(id: &str, path: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: "create_file".into(),
        arguments: json!({"path":path,"content":"hello"}),
        signatures: None,
    }
}

#[tokio::test]
async fn tools_return_to_the_model_before_the_final_reply() {
    let root = tempfile::tempdir().unwrap();
    let mut tools = Registry::default();
    tools.register(CreateFile::new(root.path()).unwrap());
    let provider = Script(Mutex::new(vec![
        Reply {
            text: String::new(),
            calls: vec![call("one", "hello.txt")],
        },
        Reply {
            text: "Created hello.txt".into(),
            calls: vec![],
        },
    ]));
    let (_stop, receiver) = watch::channel(false);
    let reply = run(
        &provider,
        "Be helpful.",
        vec![Message::User("Create a file".into())],
        &tools,
        receiver,
    )
    .await
    .unwrap();
    assert_eq!(reply, "Created hello.txt");
    assert_eq!(
        std::fs::read_to_string(root.path().join("hello.txt")).unwrap(),
        "hello"
    );
}

#[tokio::test]
async fn file_tool_refuses_overwrites_traversal_and_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let tool = CreateFile::new(root.path()).unwrap();
    let request = |path: &str| json!({"path":path,"content":"first"});
    tool.run(request("file.txt")).await.unwrap();
    assert!(
        tool.run(json!({"path":"file.txt","content":"replacement"}))
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("file.txt")).unwrap(),
        "first"
    );
    for path in ["../escape.txt", "/tmp/escape.txt", "", "a/../escape.txt"] {
        assert!(tool.run(request(path)).await.is_err(), "{path}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        assert!(tool.run(request("escape/file.txt")).await.is_err());
        assert!(!outside.path().join("file.txt").exists());
    }
    assert!(
        tool.run(json!({"path":"large","content":"x".repeat(1_048_577)}))
            .await
            .is_err()
    );
    assert!(
        tool.run(json!({"path":"extra","content":"x","shell":"bad"}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_and_duplicate_call_ids_do_not_repeat_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let mut tools = Registry::default();
    tools.register(CreateFile::new(root.path()).unwrap());
    let provider = Script(Mutex::new(vec![Reply {
        text: String::new(),
        calls: vec![call("one", "first"), call("one", "second")],
    }]));
    let (_stop, receiver) = watch::channel(false);
    assert!(
        run(&provider, "Be helpful.", vec![], &tools, receiver)
            .await
            .unwrap_err()
            .contains("repeated")
    );
    assert!(!root.path().join("first").exists());
    let (stop, receiver) = watch::channel(true);
    assert!(
        run(&provider, "Be helpful.", vec![], &tools, receiver)
            .await
            .unwrap_err()
            .contains("Stopped")
    );
    drop(stop);
}

#[tokio::test]
async fn genai_adapter_sends_system_history_and_tool_results() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let (offset, length) = loop {
            let mut part = [0; 4096];
            let read = socket.read(&mut part).await.unwrap();
            assert!(read > 0);
            bytes.extend_from_slice(&part[..read]);
            if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..offset]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|v| v.parse::<usize>().ok())
                    })
                    .unwrap();
                if bytes.len() >= offset + 4 + length {
                    break (offset + 4, length);
                }
            }
        };
        let body: Value = serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
        assert_eq!(body["messages"][0]["content"], "Prompt from a Record");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][2]["tool_calls"][0]["id"], "one");
        assert_eq!(body["messages"][3]["tool_call_id"], "one");
        assert!(
            body["messages"][3]["content"]
                .as_str()
                .unwrap()
                .contains("hello.txt")
        );
        assert_eq!(body["tools"][0]["function"]["name"], "create_file");
        let response = json!({"id":"test", "object":"chat.completion", "created":1, "model":"test-model", "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Done"}}],"usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12}}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let settings = fiote::config::Settings {
        enabled: true,
        provider: fiote::adapters::Catalog::load(root.path())
            .await
            .unwrap()
            .descriptors[0]
            .id
            .clone(),
        auth_method: String::new(),
        model: "test-model".into(),
        endpoint: format!("http://{address}/v1/"),
        directory: root.path().into(),
    };
    let provider = fiote::provider::GenaiProvider::new(
        &settings,
        &fiote::config::Secret("fake-test-key".into()),
    )
    .unwrap();
    let reply = provider
        .complete(
            "Prompt from a Record",
            &[
                Message::User("Create a file".into()),
                Message::Assistant {
                    text: String::new(),
                    calls: vec![call("one", "hello.txt")],
                },
                Message::Tool {
                    id: "one".into(),
                    name: "create_file".into(),
                    result: json!({"path":"hello.txt"}),
                },
            ],
            &[CreateFile::new(root.path()).unwrap().definition()],
        )
        .await
        .unwrap();
    assert_eq!(reply.text, "Done");
    server.await.unwrap();
}

struct Endless(std::sync::atomic::AtomicUsize);

#[async_trait]
impl Provider for Endless {
    async fn complete(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        let count = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Reply {
            text: String::new(),
            calls: vec![ToolCall {
                id: count.to_string(),
                name: "unavailable".into(),
                arguments: json!({}),
                signatures: None,
            }],
        })
    }
}

#[tokio::test]
async fn turn_and_context_limits_stop_before_more_provider_calls() {
    let provider = Endless(std::sync::atomic::AtomicUsize::new(0));
    let (_stop, receiver) = watch::channel(false);
    let error = run(
        &provider,
        "",
        vec![],
        &Registry::default(),
        receiver.clone(),
    )
    .await
    .unwrap_err();
    assert!(error.contains("model requests"));
    assert_eq!(
        provider.0.load(std::sync::atomic::Ordering::SeqCst),
        fiote::runtime::MAX_MODEL_REQUESTS
    );
    assert!(error.contains("unavailable"));
    let error = run(
        &provider,
        &"x".repeat(fiote::runtime::MAX_CONTEXT_BYTES + 1),
        vec![],
        &Registry::default(),
        receiver,
    )
    .await
    .unwrap_err();
    assert!(error.contains("context limit"));
    assert_eq!(
        provider.0.load(std::sync::atomic::Ordering::SeqCst),
        fiote::runtime::MAX_MODEL_REQUESTS
    );
}

#[test]
fn configuration_rejects_insecure_endpoints_and_debug_redacts_keys() {
    let root = tempfile::tempdir().unwrap();
    let mut settings = fiote::config::Settings {
        enabled: true,
        provider: Default::default(),
        auth_method: String::new(),
        model: "a-model".into(),
        endpoint: "https://provider.example/v1/".into(),
        directory: root.path().into(),
    };
    settings.validate().unwrap();
    assert_eq!(settings.endpoint, "https://provider.example/v1/");
    for endpoint in [
        "http://example.org/v1",
        "https://user:secret@example.org/v1",
        "https://example.org/v1?key=secret",
        "file:///tmp",
    ] {
        settings.endpoint = endpoint.into();
        assert!(settings.validate().is_err());
    }
    let request = fiote::config::Request::Configure {
        record: "record".into(),
        settings,
        api_key: Some(fiote::config::Secret("hidden-key".into())),
        password: None,
    };
    assert!(!format!("{request:?}").contains("hidden-key"));
}
