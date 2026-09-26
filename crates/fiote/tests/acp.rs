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
    async fn question(
        &self,
        request: fiote::acp::QuestionRequest,
    ) -> Result<fiote::acp::Answer, String> {
        assert_eq!(request.prompt, "Choose a direction");
        Ok(fiote::acp::Answer::Accept {
            content: Some(serde_json::json!({"choice":"left"})),
        })
    }
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
        additional_directories: Vec::new(),
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
async fn additional_directories_are_validated_gated_and_resent_on_load() {
    let (root, mut config, connection) = fixture().await;
    let extra = root.path().join("extra");
    std::fs::create_dir(&extra).unwrap();
    config.additional_directories = vec![extra.clone(), extra.clone(), root.path().into()];
    config.validate().unwrap();
    assert_eq!(config.additional_directories, vec![extra.clone()]);
    assert!(
        connection
            .session(&config, tools(), None)
            .await
            .unwrap_err()
            .contains("additional directory")
    );
    connection.close();
    let path = root.path().join("session-requests.jsonl");
    config
        .environment
        .insert("TEST_DIRECTORIES".into(), "1".into());
    config
        .environment
        .insert("TEST_SESSION_LOG".into(), path.display().to_string());
    let connection = Connection::open(&config).await.unwrap();
    let session = connection.session(&config, tools(), None).await.unwrap();
    connection
        .session(&config, tools(), Some(&session))
        .await
        .unwrap();
    let requests = std::fs::read_to_string(path).unwrap();
    for line in requests.lines() {
        let request: Value = serde_json::from_str(line).unwrap();
        assert_eq!(request["additionalDirectories"], serde_json::json!([extra]));
    }
    assert_eq!(requests.lines().count(), 2);
    config.additional_directories = vec![root.path().join("missing")];
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn terminal_auth_uses_advertised_invocation_environment_directory_and_input() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use fiote::acp::{
        AuthMethodTerminal,
        terminal::{LoginTerminal, TerminalRequest},
    };
    let (root, config, connection) = fixture().await;
    connection.close();
    let method = AuthMethodTerminal::new("terminal", "Terminal login")
        .args(vec!["--terminal-login".into()])
        .env([("TEST_AUTH_ENV".into(), "advertised".into())].into());
    let terminal = LoginTerminal::start(&config, &method).unwrap();
    let mut request = TerminalRequest {
        record: "fixture".into(),
        login: "fixture".into(),
        offset: 0,
        input: Secret(String::new()),
        cols: 80,
        rows: 24,
        close: false,
    };
    let mut output = String::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let frame = terminal.exchange(&request).unwrap();
            request.offset = frame.next;
            output.push_str(&String::from_utf8_lossy(
                &BASE64.decode(frame.data_base64).unwrap(),
            ));
            if output.contains("Private login prompt") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    request.input = Secret(BASE64.encode(b"confirm\r"));
    terminal.exchange(&request).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(result) = terminal.result() {
                result.unwrap();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("terminal-signed-in")).unwrap(),
        "yes"
    );
    request.input = Secret(BASE64.encode(vec![0; 4097]));
    assert!(terminal.exchange(&request).is_err());
}

#[tokio::test]
async fn private_urls_never_enter_the_shared_question_output() {
    let (_root, _config, connection) = fixture().await;
    let other = connection.clone();
    let task =
        tokio::spawn(async move { other.extension("test/url", serde_json::json!({})).await });
    let request = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(request) = connection.questions().await.into_iter().next() {
                break request;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(request.schema.is_none());
    assert!(!format!("{request:?}").contains("private-token"));
    assert!(
        connection
            .answer_question(
                &request.id,
                fiote::acp::Answer::Accept {
                    content: Some(serde_json::json!({}))
                }
            )
            .await
            .is_err()
    );
    connection
        .answer_question(&request.id, fiote::acp::Answer::Decline)
        .await
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap()["action"], "decline");
    assert!(connection.questions().await.is_empty());
}

#[tokio::test]
async fn speech_transcribes_fixture_audio_without_creating_a_chat_session() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let (root, mut config, old) = fixture().await;
    old.close();
    config.environment.insert("TEST_GOOSE".into(), "1".into());
    config.environment.insert(
        "TEST_REQUEST_LOG".into(),
        root.path().join("speech-requests").to_string_lossy().into(),
    );
    let connection = Connection::open(&config).await.unwrap();
    let providers = fiote::speech::catalog(&connection).await.unwrap();
    assert_eq!(providers[0].id, "local");
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(
            &mut bytes,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..1600 {
            writer.write_sample(1000i16).unwrap();
        }
        writer.finalize().unwrap();
    }
    let audio = Secret(BASE64.encode(bytes.into_inner()));
    assert_eq!(
        fiote::speech::transcribe(connection, "local".into(), audio)
            .await
            .unwrap(),
        "Fixture transcript"
    );
    let requests = std::fs::read_to_string(root.path().join("speech-requests")).unwrap();
    assert!(!requests.contains("session/new"));
    assert!(!requests.contains("session/prompt"));
}

#[tokio::test]
async fn questions_validate_and_route_one_response_without_an_extra_prompt() {
    let (_root, config, connection) = fixture().await;
    let pending = connection.clone();
    let request = tokio::spawn(async move {
        pending
            .extension_wait("test/question", serde_json::json!({}))
            .await
    });
    let question = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if let Some(question) = connection.questions().await.into_iter().next() {
                break question;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        connection
            .answer_question("wrong", fiote::acp::Answer::Cancel)
            .await
            .is_err()
    );
    assert!(
        connection
            .answer_question(
                &question.id,
                fiote::acp::Answer::Accept {
                    content: Some(serde_json::json!({"choice":"invalid"}))
                }
            )
            .await
            .is_err()
    );
    assert_eq!(connection.questions().await.len(), 1);
    connection
        .answer_question(
            &question.id,
            fiote::acp::Answer::Accept {
                content: Some(serde_json::json!({"choice":"right"})),
            },
        )
        .await
        .unwrap();
    let answer = request.await.unwrap().unwrap();
    assert_eq!(answer["content"]["choice"], "right");
    assert!(
        connection
            .answer_question(&question.id, fiote::acp::Answer::Cancel)
            .await
            .is_err()
    );
    let session = connection.session(&config, tools(), None).await.unwrap();
    let sink = Sink::default();
    assert_eq!(
        connection
            .prompt(&session, "question".into(), &sink, watch::channel(false).1)
            .await
            .unwrap(),
        "Answer received."
    );
}

#[tokio::test]
async fn sends_supported_images_audio_files_and_references_without_flattening_them() {
    use nucleus::message::MessagePart;
    let (root, mut config, connection) = fixture().await;
    let audio = MessagePart::Attachment {
        name: "voice.wav".into(),
        mime_type: "audio/wav".into(),
        data: "YQ==".into(),
    };
    assert!(
        connection
            .content(std::slice::from_ref(&audio))
            .unwrap_err()
            .contains("audio")
    );
    connection.close();
    config.environment.insert("TEST_AUDIO".into(), "1".into());
    let path = root.path().join("prompt.json");
    config
        .environment
        .insert("TEST_PROMPT_PATH".into(), path.display().to_string());
    let connection = Connection::open(&config).await.unwrap();
    let session = connection.session(&config, tools(), None).await.unwrap();
    let parts = vec![
        MessagePart::Attachment {
            name: "image.png".into(),
            mime_type: "image/png".into(),
            data: "YQ==".into(),
        },
        audio,
        MessagePart::Attachment {
            name: "note.txt".into(),
            mime_type: "text/plain".into(),
            data: "bm90ZQ==".into(),
        },
        MessagePart::Reference {
            name: "source".into(),
            uri: "file:///source.rs".into(),
        },
    ];
    let (_stop, receiver) = watch::channel(false);
    connection
        .prompt_content(
            &session,
            "Inspect fixtures".into(),
            &parts,
            &Sink::default(),
            receiver,
        )
        .await
        .unwrap();
    let sent: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let types: Vec<_> = sent["prompt"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        ["text", "image", "audio", "resource", "resource_link"]
    );
    assert_eq!(sent["prompt"][3]["resource"]["text"], "note");
    connection.close();
}

#[tokio::test]
async fn retains_idle_settings_commands_plans_and_usage_per_session() {
    let (_root, config, connection) = fixture().await;
    let session = connection.session(&config, tools(), None).await.unwrap();
    connection
        .extension("test/state", serde_json::json!({}))
        .await
        .unwrap();
    let state = connection.state(&session).await;
    assert_eq!(state.commands[0].name, "review");
    assert_eq!(state.plan.unwrap().entries.len(), 1);
    assert_eq!(state.usage.unwrap().used, 100);
    assert_eq!(
        fiote::acp::SessionOptions {
            session: session.clone(),
            options: state.options
        }
        .values()["model"],
        "small"
    );
    assert_eq!(
        connection.state("other-session").await.commands[0].name,
        "other"
    );
    connection
        .change_option(&session, "model", &Value::from("large"))
        .await
        .unwrap();
    assert_eq!(
        fiote::acp::SessionOptions {
            session: session.clone(),
            options: connection.state(&session).await.options
        }
        .values()["model"],
        "large"
    );
    assert!(
        connection
            .change_option(&session, "model", &Value::from("not-offered"))
            .await
            .is_err()
    );
    connection.close();
}

#[tokio::test]
async fn missing_agent_reports_how_to_fix_the_executable() {
    let (_root, mut config, connection) = fixture().await;
    connection.close();
    config.command = "lince-missing-test-agent".into();
    config.environment.insert("PATH".into(), String::new());
    let error = Connection::open(&config).await.err().unwrap();
    assert!(error.contains("lince-missing-test-agent"));
    assert!(error.contains("Agent options"));
    assert!(!error.contains("spawned_at"));
}

#[tokio::test]
async fn agent_lookup_respects_the_configured_path() {
    let (root, mut config, connection) = fixture().await;
    connection.close();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&config.command, root.path().join("local-agent")).unwrap();
    #[cfg(not(unix))]
    std::fs::copy(&config.command, root.path().join("local-agent")).unwrap();
    config.command = "local-agent".into();
    config
        .environment
        .insert("PATH".into(), root.path().display().to_string());
    let connection = Connection::open(&config).await.unwrap();
    assert_eq!(
        connection.info.agent_info.as_ref().unwrap().name,
        "test-agent"
    );
    connection.close();
}

#[tokio::test]
async fn connection_check_requires_login_and_never_sends_a_prompt() {
    let (root, mut config, connection) = fixture().await;
    connection.close();
    let log = root.path().join("requests.log");
    config.environment.extend([
        ("TEST_CHECK_ONLY".into(), "1".into()),
        ("TEST_GOOSE".into(), "1".into()),
        ("TEST_REQUEST_LOG".into(), log.display().to_string()),
        (
            "TEST_LOGIN_STATE".into(),
            root.path().join("login").display().to_string(),
        ),
    ]);
    let connection = Connection::open(&config).await.unwrap();
    let mut check = fiote::acp::ConnectionCheck::default();
    assert!(
        connection
            .check(&config, &[], &mut check)
            .await
            .err()
            .unwrap()
            .contains("Authentication required")
    );
    assert!(!check.ready);
    connection
        .extension_wait(
            "_goose/unstable/providers/config/authenticate",
            serde_json::json!({"providerId":"one"}),
        )
        .await
        .unwrap();
    connection.close();
    let connection = Connection::open(&config).await.unwrap();
    let mut check = fiote::acp::ConnectionCheck::default();
    let session = connection.check(&config, &[], &mut check).await.unwrap();
    assert!(check.ready);
    assert_eq!(session.values()["model"], "large");
    assert!(check.model.contains("listed by provider"));
    assert_eq!(check.login, "Provider settings present");
    assert_eq!(check.session, "Opened successfully");
    connection.close();
    let requests = std::fs::read_to_string(log).unwrap();
    assert!(!requests.contains("session/prompt"));
    assert!(requests.contains("supported-models/list"));
}

#[tokio::test]
async fn connection_check_rejects_invalid_credentials_and_expired_provider_login() {
    let (_root, mut config, connection) = fixture().await;
    connection.close();
    config.environment.extend([
        ("TEST_CHECK_ONLY".into(), "1".into()),
        ("TEST_GOOSE".into(), "1".into()),
        ("TEST_INVALID_CREDENTIAL".into(), "1".into()),
    ]);
    let connection = Connection::open(&config).await.unwrap();
    let mut check = fiote::acp::ConnectionCheck::default();
    assert!(
        connection
            .check(&config, &[], &mut check)
            .await
            .err()
            .unwrap()
            .contains("Authentication required")
    );
    assert!(!check.ready);
    let providers = [serde_json::json!({"providerId":"one","acp":true})];
    assert!(
        connection
            .check(&config, &providers, &mut check)
            .await
            .err()
            .unwrap()
            .contains("sign-in expired")
    );
    assert!(!check.ready);
    connection.close();
}

#[tokio::test]
async fn connection_check_rejects_agents_without_lince_tool_transport() {
    let (_root, mut config, connection) = fixture().await;
    connection.close();
    config.environment.insert("TEST_NO_HTTP".into(), "1".into());
    let connection = Connection::open(&config).await.unwrap();
    let mut check = fiote::acp::ConnectionCheck::default();
    assert!(
        connection
            .check(&config, &[], &mut check)
            .await
            .err()
            .unwrap()
            .contains("HTTP MCP")
    );
    assert!(!check.ready);
    assert_eq!(check.session, "Not checked");
    connection.close();
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
