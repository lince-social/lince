#![cfg(feature = "test-agent")]

use fiote::{
    acp::{Config, Connection, Output, Permission},
    config::{Secret, ToolConnection},
    provider::TextOutput,
};
use serde_json::Value;
use tokio::sync::{Mutex, Notify, watch};

#[derive(Default)]
struct Sink {
    text: Mutex<String>,
    activity: Mutex<Vec<Value>>,
    waiting: Notify,
    block: bool,
}

#[async_trait::async_trait]
impl TextOutput for Sink {
    async fn update(&self, text: &str) -> Result<(), String> {
        *self.text.lock().await = text.into();
        Ok(())
    }
}

#[async_trait::async_trait]
impl Output for Sink {
    async fn activity(&self, value: Value) -> Result<(), String> {
        self.activity.lock().await.push(value);
        Ok(())
    }
    async fn permission(&self, request: Permission) -> Result<Option<String>, String> {
        assert_eq!(request.title, "Create hello.txt");
        self.waiting.notify_one();
        if self.block {
            std::future::pending::<()>().await;
        }
        Ok(Some(request.options[0].id.clone()))
    }
}

fn tools() -> ToolConnection {
    ToolConnection {
        thread: "test".into(),
        url: "http://127.0.0.1:1/mcp".into(),
        token: Secret("test-token".into()),
    }
}

async fn fixture() -> (tempfile::TempDir, Config, std::sync::Arc<Connection>) {
    let root = tempfile::tempdir().unwrap();
    let mut config = Config {
        require_vault: false,
        command: env!("CARGO_BIN_EXE_lince-acp-test-agent").into(),
        args: vec![],
        directory: root.path().into(),
        environment: Default::default(),
        session_meta: Default::default(),
        options: Default::default(),
    };
    config.validate().unwrap();
    let connection = Connection::open(&config).await.unwrap();
    (root, config, connection)
}

#[tokio::test]
async fn streams_tools_and_resumes_without_replaying_old_messages() {
    let (root, config, connection) = fixture().await;
    assert_eq!(connection.info.auth_methods[0].id().to_string(), "browser");
    connection.authenticate("browser").await.unwrap();
    let session = connection.session(&config, tools(), None).await.unwrap();
    let (stop, receiver) = watch::channel(false);
    let output = Sink::default();
    assert_eq!(
        connection
            .prompt(&session, "hello".into(), &output, receiver.clone())
            .await
            .unwrap(),
        "Hello from agent"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("hello.txt")).unwrap(),
        "Hello from the agent"
    );
    assert!(!output.activity.lock().await.is_empty());
    let loaded = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        connection.session(&config, tools(), Some(&session)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(loaded, session);
    assert_eq!(
        connection
            .prompt(&session, "again".into(), &output, receiver)
            .await
            .unwrap(),
        "Hello from agent"
    );
    assert_eq!(*output.text.lock().await, "Hello from agent");
    drop(stop);
    connection.close();
}

#[tokio::test]
async fn stopping_permission_wait_preserves_partial_text_and_prevents_the_tool() {
    let (root, config, connection) = fixture().await;
    let session = connection.session(&config, tools(), None).await.unwrap();
    let output = Sink {
        block: true,
        ..Default::default()
    };
    let (stop, receiver) = watch::channel(false);
    let cancel = async {
        output.waiting.notified().await;
        stop.send(true).unwrap();
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(
            connection.prompt(&session, "hello".into(), &output, receiver),
            cancel
        )
    })
    .await
    .unwrap();
    assert!(result.is_err());
    assert_eq!(*output.text.lock().await, "Hello from ");
    assert!(!root.path().join("hello.txt").exists());
}

#[tokio::test]
async fn captures_device_code_without_provider_specific_logic() {
    let (_root, _config, connection) = fixture().await;
    connection
        .extension_wait(
            "_goose/unstable/providers/config/authenticate",
            serde_json::json!({"providerId":"test"}),
        )
        .await
        .unwrap();
    assert_eq!(
        connection.login_notice.lock().await.as_ref().unwrap()["userCode"],
        "test-code"
    );
    connection.close();
}

#[tokio::test]
async fn message_boundaries_surround_tool_events_without_merging_thoughts() {
    let (_root, config, connection) = fixture().await;
    let session = connection.session(&config, tools(), None).await.unwrap();
    let (_stop, receiver) = watch::channel(false);
    let output = Sink::default();
    let text = connection
        .prompt(&session, "timeline".into(), &output, receiver)
        .await
        .unwrap();
    assert_eq!(text, "Inspecting files.Done.");
    let activity = output.activity.lock().await;
    assert_eq!(activity.len(), 3);
    assert_eq!(activity[0]["messageId"], "first");
    assert_eq!(activity[1]["sessionUpdate"], "tool_call");
    assert_eq!(activity[2]["messageId"], "final");
    connection.close();
}

#[tokio::test]
async fn settings_apply_on_new_and_loaded_sessions_in_the_selected_directory() {
    let (_root, mut config, connection) = fixture().await;
    let directory = tempfile::tempdir().unwrap();
    config.directory = directory.path().into();
    config.options = [
        ("provider".into(), Value::from("two")),
        ("model".into(), Value::from("small")),
        ("thinking".into(), Value::from("low")),
        ("speed".into(), Value::from(true)),
    ]
    .into();
    for previous in [None, Some("test-session")] {
        let session = connection
            .session(&config, tools(), previous)
            .await
            .unwrap();
        assert_eq!(session, "test-session");
        let values: Value =
            serde_json::from_slice(&std::fs::read(directory.path().join("options.json")).unwrap())
                .unwrap();
        assert_eq!(values[0]["currentValue"], "two");
        assert_eq!(values[1]["currentValue"], "small");
        assert_eq!(values[2]["currentValue"], "low");
        assert_eq!(values[3]["currentValue"], true);
    }
    connection.close();
}

#[tokio::test]
async fn settings_reject_unknown_values_and_refresh_dependent_choices() {
    let (root, config, connection) = fixture().await;
    let mut options = connection.options(&config).await.unwrap();
    assert!(
        connection
            .set_option(&mut options, "missing", &Value::from("x"))
            .await
            .is_err()
    );
    assert!(
        connection
            .set_option(&mut options, "model", &Value::from("invented"))
            .await
            .is_err()
    );
    assert!(
        connection
            .set_option(&mut options, "speed", &Value::from("true"))
            .await
            .is_err()
    );
    assert!(!root.path().join("options.json").exists());
    connection
        .set_option(&mut options, "thinking", &Value::from("high"))
        .await
        .unwrap();
    connection
        .set_option(&mut options, "model", &Value::from("small"))
        .await
        .unwrap();
    assert_eq!(options.values()["thinking"], "low");
    assert!(
        connection
            .set_option(&mut options, "thinking", &Value::from("high"))
            .await
            .is_err()
    );
    let mut obsolete = config.clone();
    obsolete.options = [
        ("model".into(), Value::from("small")),
        ("thinking".into(), Value::from("high")),
    ]
    .into();
    assert!(connection.options(&obsolete).await.is_err());
    obsolete.options.clear();
    assert!(connection.options(&obsolete).await.is_ok());
    connection.close();
}

#[test]
fn settings_validate_directory_and_option_values() {
    let root = tempfile::tempdir().unwrap();
    let mut config: Config = serde_json::from_value(serde_json::json!({
        "command":"test-agent", "args":[], "directory":root.path(),
        "options":{"model":{"secret":"not a choice"}}
    }))
    .unwrap();
    assert!(config.validate().is_err());
    config.options.clear();
    config.directory = "relative".into();
    assert!(config.validate().is_err());
    config.directory = root.path().join("missing");
    assert!(config.validate().is_err());
    config.directory = root.path().into();
    config.validate().unwrap();
}
