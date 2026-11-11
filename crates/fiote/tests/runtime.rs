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
            usage: None,
            replay: Vec::new(),
            text: String::new(),
            calls: vec![call("one", "hello.txt")],
        },
        Reply {
            usage: None,
            replay: Vec::new(),
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
        usage: None,
        replay: Vec::new(),
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
            .descriptors
            .iter()
            .find(|p| p.id.0 == "openai")
            .unwrap()
            .id
            .clone(),
        auth_method: String::new(),
        model: "test-model".into(),
        endpoint: format!("http://{address}/v1/"),
        directory: root.path().into(),
        ..Default::default()
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

struct OversizedDefinition;
#[async_trait]
impl Tool for OversizedDefinition {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "oversized".into(),
            description: "x".repeat(fiote::runtime::MAX_CONTEXT_BYTES),
            schema: json!({"type":"object"}),
        }
    }
    async fn run(&self, _: Value) -> Result<Value, String> {
        panic!("An oversized tool must not execute");
    }
}

#[async_trait]
impl Provider for Endless {
    async fn complete(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        let count = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count == 100 {
            return Ok(Reply {
                text: "Completed long turn".into(),
                ..Default::default()
            });
        }
        Ok(Reply {
            usage: None,
            replay: Vec::new(),
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
async fn long_turn_continues_past_old_limits_but_context_limits_prevent_requests() {
    let provider = Endless(std::sync::atomic::AtomicUsize::new(0));
    let (_stop, receiver) = watch::channel(false);
    let text = run(
        &provider,
        "",
        vec![Message::User("Complete work".into())],
        &Registry::default(),
        receiver.clone(),
    )
    .await
    .unwrap();
    assert_eq!(text, "Completed long turn");
    assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst), 101);
    let error = run(
        &provider,
        &"x".repeat(fiote::runtime::MAX_CONTEXT_BYTES + 1),
        vec![],
        &Registry::default(),
        receiver.clone(),
    )
    .await
    .unwrap_err();
    assert!(error.contains("context limit"));
    let mut oversized = Registry::default();
    oversized.register(OversizedDefinition);
    assert!(
        run(&provider, "", vec![], &oversized, receiver)
            .await
            .unwrap_err()
            .contains("context limit")
    );
    assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst), 101);
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
        ..Default::default()
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
    };
    assert!(!format!("{request:?}").contains("hidden-key"));
}

struct LargeResult;
#[async_trait]
impl Tool for LargeResult {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "large_result".into(),
            description: "Return a large fixture".into(),
            schema: json!({"type":"object"}),
        }
    }
    async fn run(&self, _: Value) -> Result<Value, String> {
        Ok(json!({"content":"private-fixture-".repeat(5000)}))
    }
}

struct LargeResults(std::sync::atomic::AtomicUsize);
#[async_trait]
impl Provider for LargeResults {
    async fn complete(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        let count = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count == 5 {
            return Err("fixture provider failed".into());
        }
        Ok(Reply {
            calls: vec![ToolCall {
                id: count.to_string(),
                name: "large_result".into(),
                arguments: json!({}),
                signatures: None,
            }],
            ..Default::default()
        })
    }
}

#[derive(Default)]
struct Checkpoint(std::sync::Mutex<Vec<Message>>);
#[async_trait]
impl fiote::provider::TextOutput for Checkpoint {
    async fn update(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    async fn context(&self, messages: &[Message]) -> Result<(), String> {
        *self.0.lock().unwrap() = messages.to_vec();
        Ok(())
    }
}

#[tokio::test]
async fn provider_failure_keeps_full_journal_receipts_and_a_savable_error() {
    let mut tools = Registry::default();
    tools.register(LargeResult);
    let output = Checkpoint::default();
    let provider = LargeResults(std::sync::atomic::AtomicUsize::new(0));
    let (_stop, receiver) = watch::channel(false);
    let error = fiote::runtime::run_streamed(
        &provider,
        "",
        vec![Message::User("Test".into())],
        &tools,
        receiver,
        &output,
    )
    .await
    .unwrap_err();
    assert!(error.contains("fixture provider failed"), "{error}");
    assert!(error.contains("large_result [0]: completed"));
    assert!(!error.contains("private-fixture"));
    let encoded = serde_json::to_vec(
        &json!({"operation":"interrupt","request_id":"test","message_uid":"message","text":error}),
    )
    .unwrap();
    assert!(encoded.len() < 4096);
    let checkpoint = output.0.lock().unwrap();
    let receipts: Vec<_> = checkpoint
        .iter()
        .filter_map(|message| {
            if let Message::Tool { result, .. } = message {
                Some(result)
            } else {
                None
            }
        })
        .collect();
    assert!(receipts.len() > 1);
    assert!(serde_json::to_vec(&receipts).unwrap().len() > 128 * 1024);
    assert!(
        receipts
            .iter()
            .all(|result| result["result"]["content"].as_str().unwrap().len() == 80000)
    );
}

struct LongProvider {
    requests: std::sync::atomic::AtomicUsize,
    summaries: std::sync::atomic::AtomicUsize,
    repeat: bool,
}
#[async_trait]
impl Provider for LongProvider {
    fn context_budget_bytes(&self) -> usize {
        16384
    }
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        assert!(fiote::runtime::context_bytes(messages)? + system.len() < 16384);
        if tools.is_empty() {
            self.summaries
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Ok(Reply { text: "Continue the original work. Completed tools have receipts; do not repeat effects.".into(), ..Default::default() });
        }
        assert_eq!(system, "Original permissions; never reboot");
        let count = self
            .requests
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count == 100 {
            return Ok(Reply {
                text: "Completed".into(),
                ..Default::default()
            });
        }
        Ok(Reply {
            calls: vec![ToolCall {
                id: if self.repeat && count == 75 {
                    "0".into()
                } else {
                    count.to_string()
                },
                name: "large_result".into(),
                arguments: json!({}),
                signatures: None,
            }],
            ..Default::default()
        })
    }
}
#[derive(Default)]
struct Archives(std::sync::Mutex<Vec<Vec<Message>>>);
#[async_trait]
impl fiote::provider::TextOutput for Archives {
    async fn update(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    async fn compacted(&self, before: &[Message], _: &[Message]) -> Result<(), String> {
        self.0.lock().unwrap().push(before.to_vec());
        Ok(())
    }
}
#[tokio::test]
async fn long_single_turn_repeatedly_compacts_without_repeating_effects() {
    for repeat in [false, true] {
        let provider = LongProvider {
            requests: Default::default(),
            summaries: Default::default(),
            repeat,
        };
        let mut tools = Registry::default();
        tools.register(LargeResult);
        let (_stop, receiver) = watch::channel(false);
        let output = Archives::default();
        let result = fiote::runtime::run_streamed(
            &provider,
            "Original permissions; never reboot",
            vec![Message::User("Complete this whole task".into())],
            &tools,
            receiver,
            &output,
        )
        .await;
        if repeat {
            assert!(
                result
                    .unwrap_err()
                    .contains("repeated a tool call identifier")
            );
        } else {
            assert_eq!(result.unwrap(), "Completed");
            assert_eq!(
                provider.requests.load(std::sync::atomic::Ordering::SeqCst),
                101
            );
        }
        assert!(provider.summaries.load(std::sync::atomic::Ordering::SeqCst) > 20);
        let archives = output.0.lock().unwrap();
        let ids: Vec<_> = archives
            .iter()
            .flat_map(|frames| frames.iter())
            .filter_map(|frame| match frame {
                Message::Tool { id, result, .. }
                    if result["result"]["content"]
                        .as_str()
                        .is_some_and(|text| text.len() == 80000) =>
                {
                    Some(id.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), if repeat { 75 } else { 100 });
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            ids.len()
        );
    }
}

struct Overflow(std::sync::atomic::AtomicUsize);
#[async_trait]
impl Provider for Overflow {
    async fn complete(
        &self,
        system: &str,
        _: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        match self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
            0 => Err("HTTP 400, context_window_exceeded".into()),
            1 => {
                assert!(system.contains("Summarize"));
                Ok(Reply {
                    text: "Continue owner task without reboot".into(),
                    ..Default::default()
                })
            }
            2 => {
                assert_eq!(system, "Owner permissions");
                Ok(Reply {
                    text: "Recovered".into(),
                    ..Default::default()
                })
            }
            _ => panic!("Overflow retry must be bounded"),
        }
    }
}
#[tokio::test]
async fn provider_context_overflow_compacts_and_retries_without_tools() {
    let provider = Overflow(Default::default());
    let (_stop, receiver) = watch::channel(false);
    assert_eq!(
        run(
            &provider,
            "Owner permissions",
            vec![Message::User("Complete original goal".into())],
            &Registry::default(),
            receiver
        )
        .await
        .unwrap(),
        "Recovered"
    );
    assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst), 3);
}

struct UsageTrigger(std::sync::atomic::AtomicUsize);
#[async_trait]
impl Provider for UsageTrigger {
    async fn compact(
        &self,
        system: &str,
        _: &[Message],
        _: &dyn fiote::provider::TextOutput,
    ) -> Result<Option<Vec<Message>>, String> {
        assert_eq!(system, "Pinned owner instructions");
        Ok(Some(vec![
            Message::Summary {
                covered: 3,
                text: "Goal and completed receipt".into(),
            },
            Message::User("Continue task".into()),
        ]))
    }
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        assert_eq!(system, "Pinned owner instructions");
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            let mut usage = nucleus::operation::Usage::request(
                "fixture".into(),
                Some(200000),
                Some(1),
                Some(200001),
            );
            usage.context_used = Some(200001);
            return Ok(Reply {
                usage: Some(usage),
                calls: vec![ToolCall {
                    id: "first".into(),
                    name: "unavailable".into(),
                    arguments: json!({}),
                    signatures: None,
                }],
                ..Default::default()
            });
        }
        assert!(matches!(messages.first(), Some(Message::Summary { .. })));
        Ok(Reply {
            text: "Continued after measured token threshold".into(),
            ..Default::default()
        })
    }
}
#[tokio::test]
async fn reported_usage_triggers_compaction_before_the_next_request() {
    let provider = UsageTrigger(Default::default());
    let (_stop, receiver) = watch::channel(false);
    let result = run(
        &provider,
        "Pinned owner instructions",
        vec![Message::User("Continue task".into())],
        &Registry::default(),
        receiver,
    )
    .await
    .unwrap();
    assert_eq!(result, "Continued after measured token threshold");
}

struct CancelLong {
    count: std::sync::atomic::AtomicUsize,
    stop: watch::Sender<bool>,
}
#[async_trait]
impl Provider for CancelLong {
    async fn complete(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        let index = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if index == 75 {
            self.stop.send(true).unwrap();
        }
        Ok(Reply {
            calls: vec![ToolCall {
                id: index.to_string(),
                name: "unavailable".into(),
                arguments: json!({}),
                signatures: None,
            }],
            ..Default::default()
        })
    }
}
#[tokio::test]
async fn owner_can_still_cancel_after_the_old_turn_limit() {
    let (stop, receiver) = watch::channel(false);
    let provider = CancelLong {
        count: Default::default(),
        stop,
    };
    let result = run(
        &provider,
        "",
        vec![Message::User("Long task".into())],
        &Registry::default(),
        receiver,
    )
    .await
    .unwrap_err();
    assert!(result.contains("Stopped by you"));
    assert_eq!(provider.count.load(std::sync::atomic::Ordering::SeqCst), 76);
    assert!(result.len() < 8192);
}
