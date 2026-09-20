use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fiote::provider::{Provider, TextOutput};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Notify,
};

struct Output {
    text: Mutex<Vec<String>>,
    seen: Arc<Notify>,
}

#[async_trait]
impl TextOutput for Output {
    async fn update(&self, text: &str) -> Result<(), String> {
        self.text.lock().unwrap().push(text.into());
        self.seen.notify_one();
        Ok(())
    }
}

async fn request(socket: &mut tokio::net::TcpStream) -> Value {
    let mut bytes = Vec::new();
    loop {
        let mut part = [0; 4096];
        let count = socket.read(&mut part).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&part[..count]);
        if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..offset]);
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse().ok())
                })
                .unwrap();
            if bytes.len() >= offset + 4 + length {
                return serde_json::from_slice(&bytes[offset + 4..offset + 4 + length]).unwrap();
            }
        }
    }
}

fn event(delta: Value, finish: Value) -> String {
    format!(
        "data: {}\n\n",
        json!({"id":"test", "object":"chat.completion.chunk", "created":1, "model":"test-model", "choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    )
}

#[tokio::test]
async fn genai_emits_text_before_completion_and_collects_complete_tool_arguments() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen = Arc::new(Notify::new());
    let server_seen = seen.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = request(&mut socket).await;
        assert_eq!(request["stream"], true);
        let first = event(
            json!({"role":"assistant","content":"Hello 👋"}),
            Value::Null,
        );
        let rest = format!(
            "{}{}{}data: [DONE]\n\n",
            event(
                json!({"tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"create_file","arguments":"{\"path\":"}}]}),
                Value::Null
            ),
            event(
                json!({"tool_calls":[{"index":0,"function":{"arguments":"\"hello.txt\",\"content\":\"Hi\"}"}}]}),
                Value::Null
            ),
            event(json!({}), json!("tool_calls"))
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{first}", first.len() + rest.len()).as_bytes()).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), server_seen.notified())
            .await
            .unwrap();
        socket.write_all(rest.as_bytes()).await.unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let catalog = fiote::adapters::Catalog::load(root.path()).await.unwrap();
    let settings = fiote::config::Settings {
        enabled: true,
        provider: catalog.descriptors[0].id.clone(),
        auth_method: String::new(),
        model: "test-model".into(),
        endpoint: format!("http://{address}/v1/"),
        directory: Default::default(),
    };
    let provider =
        fiote::provider::GenaiProvider::new(&settings, &fiote::config::Secret("test".into()))
            .unwrap();
    let output = Output {
        text: Default::default(),
        seen,
    };
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        provider.stream(
            "Prompt",
            &[fiote::provider::Message::User("Hello".into())],
            &[],
            &output,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(reply.text, "Hello 👋");
    assert_eq!(reply.calls[0].id, "call-1");
    assert_eq!(
        reply.calls[0].arguments,
        json!({"path":"hello.txt","content":"Hi"})
    );
    assert_eq!(output.text.lock().unwrap().first().unwrap(), "Hello 👋");
    server.await.unwrap();
}
