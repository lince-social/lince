use super::*;
use fiote::provider::{Reply, ToolCall, ToolDefinition};
use serde_json::json;
use transport::{ClientMessage, LaneHub, ServerMessage, Session};

#[tokio::test]
#[ignore = "Requires an installed, authenticated ACP agent and consumes a live model turn"]
async fn installed_agent_edits_code_and_record_and_resumes_thread() {
    let (host, _, root, record, thread) = fixture(false).await;
    let mut config: fiote::acp::Config = serde_json::from_str(
        &std::env::var("LINCE_ACP_TEST_CONFIG").expect("LINCE_ACP_TEST_CONFIG"),
    )
    .unwrap();
    config.directory = root.path().into();
    host.handle(Request::AgentConfigure {
        record: record.clone(),
        config,
    })
    .await
    .unwrap();
    host.send(&thread, &format!("This is an integration test in a temporary folder. Create hello.txt containing exactly ACP verified. Use the Lince MCP tools to read Record {record}, then collaboratively append the text ACP verified to its description. Do not edit any other files or records. Reply briefly confirming both operations.")).await.unwrap().unwrap();
    let deadline = std::time::Duration::from_secs(180);
    tokio::time::timeout(deadline, async {
        loop {
            let permissions = host.agents.activity(&record).await;
            for activity in permissions {
                if let Some(permission) = activity.permission {
                    let choice = permission
                        .options
                        .iter()
                        .find(|choice| {
                            choice.label.to_ascii_lowercase().contains("allow")
                                || choice.label.to_ascii_lowercase().contains("approve")
                        })
                        .unwrap();
                    host.agents
                        .answer(&record, &thread, &permission.id, Some(choice.id.clone()))
                        .await
                        .unwrap();
                }
            }
            if host.running.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let messages = rows(&host, &thread).await;
    assert!(
        messages
            .iter()
            .any(|message| message.body.contains("ACP verified")),
        "{messages:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("hello.txt"))
            .unwrap_or_else(|error| panic!("{error}: {messages:?}"))
            .trim(),
        "ACP verified",
        "{messages:?}"
    );
    assert!(
        host.record(&record)
            .await
            .unwrap()
            .body
            .ends_with("ACP verified"),
        "{messages:?}"
    );
    host.stop_all().await;
    host.send(&thread, "Reply with the exact two words written in hello.txt during our previous turn. Do not change any files or records.").await.unwrap().unwrap();
    tokio::time::timeout(deadline, async {
        while !host.running.lock().await.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let messages = rows(&host, &thread).await;
    let last = messages.first().unwrap();
    assert!(last.body.contains("ACP verified"), "{messages:?}");
    assert!(!last.body.contains("Fiote stopped"), "{messages:?}");
    host.stop_all().await;
}

struct Script {
    observed: std::sync::Mutex<Vec<(String, Vec<Message>)>>,
    block: bool,
}

#[async_trait::async_trait]
impl Provider for Script {
    async fn complete(
        &self,
        prompt: &str,
        messages: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        self.observed
            .lock()
            .unwrap()
            .push((prompt.into(), messages.to_vec()));
        if self.block {
            std::future::pending::<()>().await;
        }
        match messages.last().unwrap() {
            Message::User(body) if body == "create a file" => Ok(Reply {
                text: String::new(),
                calls: vec![ToolCall {
                    id: "call-1".into(),
                    name: "create_file".into(),
                    arguments: json!({"path":"hello.txt","content":"Hello from Fiote"}),
                    signatures: None,
                }],
            }),
            Message::Tool { result, .. } => {
                assert_eq!(result["ok"], true);
                Ok(Reply {
                    text: "Created hello.txt".into(),
                    calls: vec![],
                })
            }
            _ => Ok(Reply {
                text: "Hello!".into(),
                calls: vec![],
            }),
        }
    }
}

async fn fixture(block: bool) -> (Arc<Host>, Arc<Script>, tempfile::TempDir, String, String) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let root = tempfile::tempdir().unwrap();
    let script = Arc::new(Script {
        observed: Default::default(),
        block,
    });
    let mut host = Host::open(engine.clone(), root.path().join("settings"))
        .await
        .unwrap();
    host.provider = Some(script.clone());
    let host = Arc::new(host);
    let record = engine
        .act(
            Action::CreateAgent {
                head: "Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::EditRecordText {
                target: record.clone(),
                head: None,
                body: Some("Be useful. This is my prompt.".into()),
            },
            None,
        )
        .await
        .unwrap();
    let thread = engine
        .act(
            Action::CreateThread {
                target: record.clone(),
                head: "First session".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.handle(Request::Configure {
        record: record.clone(),
        settings: Settings {
            enabled: true,
            provider: host
                .catalog
                .descriptors
                .iter()
                .find(|provider| provider.auth_methods[0].kind == fiote::adapters::AuthKind::ApiKey)
                .unwrap()
                .id
                .clone(),
            auth_method: String::new(),
            model: "test-model".into(),
            endpoint: String::new(),
            directory: root.path().into(),
        },
        api_key: Some(Secret("secret-test-value".into())),
        password: Some(Secret("test-password".into())),
    })
    .await
    .unwrap();
    (host, script, root, record, thread)
}

async fn wait(host: &Host) {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while !host.running.lock().await.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

async fn rows(host: &Host, thread: &str) -> Vec<store::records::RecordRow> {
    let Some(predicate) = store::concepts::resolve(&host.engine.store.pool, "message-in")
        .await
        .unwrap()
    else {
        return Vec::new();
    };
    store::assertions::recent_messages(&host.engine.store.pool, &predicate, thread, 100)
        .await
        .unwrap()
}

fn local(host: &Arc<Host>) -> Session {
    Session::local(host.engine.clone(), Arc::new(LaneHub::new()), "test").with_fiote(host.clone())
}

fn send(thread: &str, body: &str) -> ClientMessage {
    ClientMessage::Act {
        id: "send".into(),
        action: Action::CreateMessage {
            thread: thread.into(),
            body: body.into(),
            author: None,
            state: MessageState::Finished,
            parent: None,
            references: vec![],
        },
    }
}

#[tokio::test]
async fn mentioning_a_fiote_replies_in_the_record_with_only_recent_context() {
    let (host, script, _root, agent, _) = fixture(false).await;
    let record = host
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Discussion".into(),
                body: "A shared discussion".into(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let thread = host
        .engine
        .act(
            Action::CreateThread {
                target: record.clone(),
                head: String::new(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    for index in 0..20 {
        let response = local(&host)
            .handle(send(&thread, &format!("Earlier message {index}")))
            .await;
        assert!(matches!(response[0], ServerMessage::ActionOk { .. }));
    }
    assert!(script.observed.lock().unwrap().is_empty());
    let response = local(&host).handle(send(&thread, "@\"Fiote\" hello")).await;
    assert!(
        matches!(response[0], ServerMessage::ActionOk { .. }),
        "{response:?}"
    );
    wait(&host).await;
    {
        let observed = script.observed.lock().unwrap();
        let (system, messages) = &observed[0];
        assert_eq!(messages.len(), 13);
        assert!(matches!(&messages[0], Message::User(body) if body == "Earlier message 8"));
        assert!(system.contains(&record));
        assert!(system.contains("Be useful. This is my prompt."));
    }
    assert_eq!(rows(&host, &thread).await[0].body, "Hello!");
    let projection = local(&host).handle(ClientMessage::Subscribe {
        id: "discussion".into(),
        protein: serde_json::from_value(json!({"source":"record","where":[{"uid_eq":record}],"fields":["uid","threads"],"include":{"threads":{"messages_limit":50}}})).unwrap(),
    }).await;
    assert!(
        projection.iter().any(|event| {
            let ServerMessage::Snapshot { rows, .. } = event else {
                return false;
            };
            rows.iter()
                .flat_map(|row| row["threads"].as_array().into_iter().flatten())
                .flat_map(|thread| thread["messages"].as_array().into_iter().flatten())
                .any(|message| message["author"] == agent && message["author_name"] == "Fiote")
        }),
        "{projection:?}"
    );
    assert_eq!(host.thread_fiote(&thread).await.unwrap(), agent);
    let status = host
        .handle(Request::OpenTools {
            record: record.clone(),
            thread: thread.clone(),
        })
        .await
        .unwrap();
    assert_eq!(status.record, agent);
    host.handle(Request::CloseTools {
        record: record.clone(),
        thread: thread.clone(),
    })
    .await
    .unwrap();
    local(&host)
        .handle(send(&thread, "A note to the other people"))
        .await;
    wait(&host).await;
    assert_eq!(script.observed.lock().unwrap().len(), 1);
    local(&host)
        .handle(send(
            &thread,
            &format!("[@old-slug](record:{agent}) hello again"),
        ))
        .await;
    wait(&host).await;
    assert_eq!(script.observed.lock().unwrap().len(), 2);
    assert_eq!(host.record_threads(&record).await.unwrap().len(), 1);
}

#[tokio::test]
async fn mentioned_agents_use_native_tools_and_cannot_be_triggered_remotely() {
    let (mut host, _, _root, agent, _) = fixture(false).await;
    let target = host
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Task".into(),
                body: "Original task".into(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let thread = host
        .engine
        .act(
            Action::CreateThread {
                target: target.clone(),
                head: String::new(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    Arc::get_mut(&mut host).unwrap().provider = Some(Arc::new(NativeScript {
        target: target.clone(),
    }));
    let response = local(&host)
        .handle(send(&thread, &format!("@{agent} update this task")))
        .await;
    assert!(
        matches!(response[0], ServerMessage::ActionOk { .. }),
        "{response:?}"
    );
    wait(&host).await;
    let record = host.record(&target).await.unwrap();
    assert_eq!(record.body, "Updated by Fiote");
    assert_eq!(record.quantity.to_string(), "-3");
    assert!(
        rows(&host, &thread)
            .await
            .iter()
            .any(|row| row.body == "Updated the task description and quantity.")
    );
    let mut remote = Session::new(
        host.engine.clone(),
        Arc::new(LaneHub::new()),
        "remote-mention",
        None,
    )
    .with_fiote(host.clone());
    remote
        .handle(send(&thread, &format!("@{agent} update this task again")))
        .await;
    assert!(host.running.lock().await.is_empty());
}

#[tokio::test]
async fn changing_the_mentioned_agent_replaces_its_instructions_and_session() {
    let (host, script, _root, agent, thread) = fixture(false).await;
    let other = host
        .handle(Request::Create {
            head: "Reviewer".into(),
        })
        .await
        .unwrap()
        .record;
    let response = local(&host).handle(send(&thread, "@Reviewer hello")).await;
    assert!(
        matches!(&response[0], ServerMessage::Error { message, .. } if message.contains("Configure"))
    );
    assert!(rows(&host, &thread).await.is_empty());
    host.engine
        .act(
            Action::EditRecordText {
                target: other.clone(),
                head: None,
                body: Some("Review carefully.".into()),
            },
            None,
        )
        .await
        .unwrap();
    let mut config = host.load(&agent).unwrap().unwrap();
    config.author = other.clone();
    save(&host.path(&other).unwrap(), &config).unwrap();
    host.send(&thread, "hello").await.unwrap();
    wait(&host).await;
    let saved_session = host.directory.join(format!("agent-session-{thread}.json"));
    save(&saved_session, &json!({"session":"older-agent-session"})).unwrap();
    host.send(&thread, "hello again").await.unwrap();
    wait(&host).await;
    assert!(saved_session.exists());
    host.send(&thread, "@Reviewer hello").await.unwrap();
    wait(&host).await;
    assert!(!saved_session.exists());
    assert_eq!(
        host.instruction_snapshot(&thread).unwrap().unwrap()["fiote"],
        other
    );
    {
        let observed = script.observed.lock().unwrap();
        let (prompt, _) = observed.last().unwrap();
        assert!(prompt.contains("Review carefully."));
        assert!(!prompt.contains("Be useful. This is my prompt."));
    }
    let count = rows(&host, &thread).await.len();
    assert!(
        host.send(&thread, &format!("@{agent} @Reviewer hello"))
            .await
            .unwrap_err()
            .contains("one Fiote")
    );
    assert_eq!(rows(&host, &thread).await.len(), count);
    host.send(&thread, "hello original Fiote").await.unwrap();
    wait(&host).await;
    assert_eq!(
        host.instruction_snapshot(&thread).unwrap().unwrap()["fiote"],
        agent
    );
}

struct NativeScript {
    target: String,
}

struct StreamingScript {
    ready: tokio::sync::Notify,
    begin: tokio::sync::Notify,
    started: tokio::sync::Notify,
    proceed: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl Provider for StreamingScript {
    async fn complete(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        Err("Expected streaming.".into())
    }

    async fn stream(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDefinition],
        output: &dyn fiote::provider::TextOutput,
    ) -> Result<Reply, String> {
        self.ready.notify_one();
        self.begin.notified().await;
        output.update("Hello").await?;
        self.started.notify_one();
        self.proceed.notified().await;
        output.update("Hello from Fiote").await?;
        Ok(Reply {
            text: "Hello from Fiote".into(),
            calls: vec![],
        })
    }
}

#[tokio::test]
async fn streamed_replies_remain_editable_before_completion_and_when_stopped() {
    for stop in [false, true] {
        let (mut host, _, _root, _, thread) = fixture(false).await;
        let script = Arc::new(StreamingScript {
            ready: Default::default(),
            begin: Default::default(),
            started: Default::default(),
            proceed: Default::default(),
        });
        Arc::get_mut(&mut host).unwrap().provider = Some(script.clone());
        let result = local(&host).handle(send(&thread, "hello")).await;
        assert!(matches!(result[0], ServerMessage::ActionOk { .. }));
        tokio::time::timeout(std::time::Duration::from_secs(15), script.ready.notified())
            .await
            .unwrap();
        let reply = rows(&host, &thread)
            .await
            .into_iter()
            .find(|row| row.body.is_empty())
            .unwrap();
        host.engine
            .act(
                Action::EditRecordText {
                    target: reply.uid.clone(),
                    head: None,
                    body: Some("Early note: ".into()),
                },
                None,
            )
            .await
            .unwrap();
        script.begin.notify_one();
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            script.started.notified(),
        )
        .await
        .unwrap();
        let reply = host.record(&reply.uid).await.unwrap();
        assert!(reply.body.contains("Hello"));
        assert!(reply.body.contains("Early note: "));
        host.engine
            .act(
                Action::EditRecordText {
                    target: reply.uid.clone(),
                    head: None,
                    body: Some(format!("My note: {}", reply.body)),
                },
                None,
            )
            .await
            .unwrap();
        if stop {
            host.handle(Request::Stop {
                thread: thread.clone(),
            })
            .await
            .unwrap();
        } else {
            script.proceed.notify_one();
        }
        wait(&host).await;
        let reply = host.record(&reply.uid).await.unwrap();
        assert!(reply.body.contains("My note: "), "{}", reply.body);
        assert!(reply.body.contains("Early note: "), "{}", reply.body);
        assert!(
            reply
                .body
                .contains(if stop { "Hello" } else { "Hello from Fiote" }),
            "{}",
            reply.body
        );
        let metadata =
            store::records::get_extension(&host.engine.store.pool, &reply.uid, "lince.message")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            metadata["state"],
            if stop { "interrupted" } else { "finished" }
        );
    }
}

#[async_trait::async_trait]
impl Provider for NativeScript {
    async fn complete(
        &self,
        _: &str,
        messages: &[Message],
        definitions: &[ToolDefinition],
    ) -> Result<Reply, String> {
        assert!(definitions.iter().any(|tool| tool.name == "lince_query"));
        let (name, arguments) = match messages.last().unwrap() {
            Message::User(_) => ("lince_read_record", json!({"record_uid":self.target})),
            Message::Tool { name, result, .. } if name == "lince_read_record" => {
                assert_eq!(result["ok"], true, "{result}");
                if messages.iter().any(|message| matches!(message, Message::Tool { name, .. } if name == "lince_edit_text")) {
                    ("lince_action", json!({"request_id":"quantity","read_ids":[result["result"]["read_id"]],"action":{
                        "action":"set-quantity-exact","target":self.target,"amount":"-3"
                    }}))
                } else {
                    ("lince_edit_text", json!({"request_id":"description","read_id":result["result"]["read_id"],"edits":[{
                        "field":"body","before":"Original task","after":"Updated by Fiote"
                    }]}))
                }
            }
            Message::Tool { name, result, .. } if name == "lince_edit_text" => {
                assert_eq!(result["ok"], true, "{result}");
                ("lince_read_record", json!({"record_uid":self.target}))
            }
            Message::Tool { result, .. } => {
                assert_eq!(result["ok"], true, "{result}");
                return Ok(Reply {
                    text: "Updated the task description and quantity.".into(),
                    calls: vec![],
                });
            }
            _ => panic!("Unexpected provider input"),
        };
        Ok(Reply {
            text: String::new(),
            calls: vec![ToolCall {
                id: format!("call-{}", messages.len()),
                name: name.into(),
                arguments,
                signatures: None,
            }],
        })
    }
}

#[tokio::test]
async fn a_thread_turn_uses_native_tools_and_saves_its_result_as_a_message() {
    let (mut host, _, _root, _, thread) = fixture(false).await;
    let target = host
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Task".into(),
                body: "Original task".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    Arc::get_mut(&mut host).unwrap().provider = Some(Arc::new(NativeScript {
        target: target.clone(),
    }));
    let result = local(&host).handle(send(&thread, "Update the task")).await;
    assert!(matches!(result[0], ServerMessage::ActionOk { .. }));
    wait(&host).await;
    let record = host.record(&target).await.unwrap();
    assert_eq!(record.body, "Updated by Fiote");
    assert_eq!(record.quantity.to_string(), "-3");
    assert!(
        rows(&host, &thread)
            .await
            .iter()
            .any(|row| row.body == "Updated the task description and quantity.")
    );
}

#[tokio::test]
async fn enabling_an_empty_prompt_record_inherits_the_editable_default() {
    let (host, _, _root, _, _) = fixture(false).await;
    let record = host
        .engine
        .act(
            Action::CreateAgent {
                head: "Another Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.handle(Request::Configure {
        record: record.clone(),
        settings: Settings {
            enabled: true,
            model: "test".into(),
            provider: host
                .catalog
                .descriptors
                .iter()
                .find(|provider| provider.auth_methods[0].kind == fiote::adapters::AuthKind::ApiKey)
                .unwrap()
                .id
                .clone(),
            ..Default::default()
        },
        api_key: None,
        password: None,
    })
    .await
    .unwrap();
    assert!(host.record(&record).await.unwrap().body.is_empty());
    assert_eq!(
        host.prompt_sources(&record).await.unwrap()[0].body,
        fiote::prompt::DEFAULT
    );
}

#[tokio::test]
async fn local_thread_runs_tools_and_persists_attributed_replies_with_session_isolation() {
    let (host, script, root, record, thread) = fixture(false).await;
    let response = local(&host).handle(send(&thread, "create a file")).await;
    assert!(
        matches!(&response[0], ServerMessage::ActionOk { .. }),
        "{response:?}"
    );
    wait(&host).await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("hello.txt")).unwrap(),
        "Hello from Fiote"
    );
    let messages = rows(&host, &thread).await;
    assert_eq!(messages.len(), 2);
    let reply = messages
        .iter()
        .find(|row| row.body == "Created hello.txt")
        .unwrap();
    let metadata =
        store::records::get_extension(&host.engine.store.pool, &reply.uid, "lince.message")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(metadata["author"], record);
    assert_eq!(metadata["state"], "finished");
    assert!(
        script.observed.lock().unwrap()[0]
            .0
            .contains("Be useful. This is my prompt.")
    );
    assert!(script.observed.lock().unwrap()[0].0.contains(&thread));
    host.engine
        .act(
            Action::EditRecordText {
                target: record.clone(),
                head: None,
                body: Some("New prompt".into()),
            },
            None,
        )
        .await
        .unwrap();
    let second = host
        .engine
        .act(
            Action::CreateThread {
                target: record.clone(),
                head: "Second session".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    local(&host).handle(send(&second, "hello")).await;
    wait(&host).await;
    {
        let observed = script.observed.lock().unwrap();
        let (prompt, messages) = observed.last().unwrap();
        assert!(prompt.contains("New prompt"));
        assert_eq!(messages.len(), 1);
    }
    let reopened = Arc::new(
        Host::open(host.engine.clone(), host.directory.clone())
            .await
            .unwrap(),
    );
    let status = reopened
        .handle(Request::Inspect {
            record: record.clone(),
        })
        .await
        .unwrap();
    assert!(status.settings.enabled && status.has_key);
    assert!(
        !serde_json::to_string(&status)
            .unwrap()
            .contains("secret-test-value")
    );
    assert_eq!(reopened.history(&thread, &record).await.unwrap().len(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(host.path(&record).unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn stop_marks_reply_interrupted_and_busy_send_keeps_the_draft_unsent() {
    let (host, _, _root, record, thread) = fixture(true).await;
    let mut session = local(&host);
    session.handle(send(&thread, "hello")).await;
    let response = session.handle(send(&thread, "second message")).await;
    assert!(
        matches!(&response[0], ServerMessage::Error { message, .. } if message.contains("already working"))
    );
    assert!(
        host.handle(Request::Configure {
            record,
            settings: Settings::default(),
            api_key: None,
            password: None,
        })
        .await
        .is_err()
    );
    host.handle(Request::Stop {
        thread: thread.clone(),
    })
    .await
    .unwrap();
    wait(&host).await;
    let messages = rows(&host, &thread).await;
    assert_eq!(messages.len(), 2);
    let interrupted = messages
        .iter()
        .find(|row| row.body.contains("Stopped by you"))
        .unwrap();
    let metadata =
        store::records::get_extension(&host.engine.store.pool, &interrupted.uid, "lince.message")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(metadata["state"], "interrupted");
}

#[tokio::test]
async fn remote_sessions_cannot_configure_or_trigger_local_fiote() {
    let (host, script, _root, record, thread) = fixture(false).await;
    let mut session = Session::new(
        host.engine.clone(),
        Arc::new(LaneHub::new()),
        "remote",
        None,
    )
    .with_fiote(host.clone());
    let response = session
        .handle(ClientMessage::Fiote {
            id: "inspect".into(),
            request: Request::Inspect {
                record: record.clone(),
            },
        })
        .await;
    assert!(matches!(&response[0], ServerMessage::Error { .. }));
    let response = session
        .handle(ClientMessage::Fiote {
            id: "tools".into(),
            request: Request::OpenTools {
                record,
                thread: thread.clone(),
            },
        })
        .await;
    assert!(matches!(&response[0], ServerMessage::Error { .. }));
    session.handle(send(&thread, "hello")).await;
    assert!(script.observed.lock().unwrap().is_empty());
    assert_eq!(rows(&host, &thread).await.len(), 1);
}

#[tokio::test]
async fn agent_tools_open_without_a_model_provider_and_lock_revokes_them() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let directory = tempfile::tempdir().unwrap();
    let host = Host::open(engine.clone(), directory.path().join("settings"))
        .await
        .unwrap();
    let record = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Fiote".into(),
                body: "My prompt".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let thread = engine
        .act(
            Action::CreateThread {
                target: record.clone(),
                head: "Thread 1".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let status = host
        .handle(Request::OpenTools {
            record: record.clone(),
            thread: thread.clone(),
        })
        .await
        .unwrap();
    assert!(!status.settings.enabled);
    assert!(!status.requires_credential);
    assert_eq!(status.tool_connections.len(), 1);
    let first = status.tool_connections[0].clone();
    let status = host
        .handle(Request::OpenTools {
            record: record.clone(),
            thread: thread.clone(),
        })
        .await
        .unwrap();
    assert_eq!(status.tool_connections[0].url, first.url);
    let author = host.load(&record).unwrap().unwrap().author;
    assert_eq!(
        host.record(&author).await.unwrap().kind,
        RecordKind::Person.as_str()
    );
    let wrong = host
        .handle(Request::OpenTools {
            record: thread.clone(),
            thread: thread.clone(),
        })
        .await;
    assert!(wrong.is_err());
    let status = host
        .handle(Request::Lock {
            record: record.clone(),
        })
        .await
        .unwrap();
    assert!(status.tool_connections.is_empty());
    let status = host
        .handle(Request::OpenTools {
            record: record.clone(),
            thread: thread.clone(),
        })
        .await
        .unwrap();
    assert_ne!(status.tool_connections[0].token.0, first.token.0);
    assert_eq!(host.load(&record).unwrap().unwrap().author, author);
    let status = host
        .handle(Request::CloseTools { record, thread })
        .await
        .unwrap();
    assert!(status.tool_connections.is_empty());
}

#[tokio::test]
async fn restart_recovers_pending_messages_and_ignores_already_finished_markers() {
    let (host, _, _root, record, thread) = fixture(false).await;
    let reply = host
        .engine
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: "Partial reply and human notes".into(),
                author: Some(record),
                state: MessageState::Writing,
                parent: None,
                references: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let path = host.directory.join(format!("turn-{thread}.json"));
    save(
        &path,
        &Pending {
            message: reply.clone(),
        },
    )
    .unwrap();
    Host::open(host.engine.clone(), host.directory.clone())
        .await
        .unwrap();
    let metadata = store::records::get_extension(&host.engine.store.pool, &reply, "lince.message")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata["state"], "interrupted");
    assert!(
        host.record(&reply)
            .await
            .unwrap()
            .body
            .starts_with("Partial reply and human notes\n\n")
    );
    assert!(!path.exists());
    save(&path, &Pending { message: reply }).unwrap();
    Host::open(host.engine.clone(), host.directory.clone())
        .await
        .unwrap();
    assert!(!path.exists());
}

#[tokio::test]
async fn failure_to_save_a_session_keeps_the_sent_message_and_explains_the_failure() {
    let (host, script, _root, _record, thread) = fixture(false).await;
    std::fs::create_dir(host.directory.join(format!("turn-{thread}.json"))).unwrap();
    let response = local(&host).handle(send(&thread, "hello")).await;
    assert!(
        matches!(&response[0], ServerMessage::ActionOk { warnings, .. } if !warnings.is_empty())
    );
    assert!(script.observed.lock().unwrap().is_empty());
    let messages = rows(&host, &thread).await;
    assert_eq!(messages.len(), 2);
    let reply = messages
        .iter()
        .find(|row| row.body.contains("session could not be saved"))
        .unwrap();
    let metadata =
        store::records::get_extension(&host.engine.store.pool, &reply.uid, "lince.message")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(metadata["state"], "interrupted");
    assert!(host.running.lock().await.is_empty());
}

#[tokio::test]
async fn provider_vault_is_shared_encrypted_and_locked_after_restart() {
    let (host, _, _root, record, thread) = fixture(false).await;
    let bytes = std::fs::read_to_string(host.path(&record).unwrap()).unwrap();
    assert!(!bytes.contains("secret-test-value"));
    let stored = store::records::resolve(&host.engine.store.pool, "fiote-vault")
        .await
        .unwrap()
        .unwrap();
    let value =
        store::records::get_extension(&host.engine.store.pool, &stored.uid, vault::NAMESPACE)
            .await
            .unwrap()
            .unwrap();
    assert!(utils::vault::is_locked(
        value["ciphertext"].as_str().unwrap()
    ));
    let ops = store::sync_ops::all_by_hlc(&host.engine.store.pool)
        .await
        .unwrap();
    for op in ops {
        assert!(!op.value.unwrap_or_default().contains("secret-test-value"));
    }
    let reopened = Host::open(host.engine.clone(), host.directory.clone())
        .await
        .unwrap();
    assert!(reopened.status(&record).await.unwrap().locked);
    assert!(
        reopened
            .send(&thread, "hello")
            .await
            .unwrap_err()
            .contains("Unlock")
    );
    assert!(
        reopened
            .handle(Request::Unlock {
                record: record.clone(),
                password: Secret("wrong".into())
            })
            .await
            .is_err()
    );
    reopened
        .handle(Request::Unlock {
            record: record.clone(),
            password: Secret("test-password".into()),
        })
        .await
        .unwrap();
    assert!(!reopened.status(&record).await.unwrap().locked);
    let key = reopened
        .vault
        .key(&vault::slot(&host.load(&record).unwrap().unwrap().settings))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(key.0, "secret-test-value");
    reopened
        .handle(Request::Lock {
            record: record.clone(),
        })
        .await
        .unwrap();
    assert!(reopened.status(&record).await.unwrap().locked);
    assert!(rows(&host, &thread).await.is_empty());
}

#[tokio::test]
async fn preparing_a_fiote_seeds_a_numbered_thread_once_and_preserves_custom_prompts() {
    let (host, _, _root, record, _) = fixture(false).await;
    for _ in 0..2 {
        host.handle(Request::Prepare {
            record: record.clone(),
        })
        .await
        .unwrap();
    }
    let row = host.record(&record).await.unwrap();
    assert_eq!(row.body, "Be useful. This is my prompt.");
    let predicate = store::concepts::resolve(&host.engine.store.pool, "thread-of")
        .await
        .unwrap()
        .unwrap();
    let threads =
        store::assertions::subjects_pointing_to(&host.engine.store.pool, &predicate, &record)
            .await
            .unwrap();
    assert_eq!(
        threads
            .iter()
            .filter(|thread| thread.head == "Thread 1")
            .count(),
        0
    );
    let fresh = host
        .engine
        .act(
            Action::CreateAgent {
                head: "Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.handle(Request::Prepare {
        record: fresh.clone(),
    })
    .await
    .unwrap();
    assert!(host.record(&fresh).await.unwrap().body.is_empty());
    assert_eq!(
        host.prompt_sources(&fresh).await.unwrap()[0].body,
        fiote::prompt::DEFAULT
    );
    let threads =
        store::assertions::subjects_pointing_to(&host.engine.store.pool, &predicate, &fresh)
            .await
            .unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].head, "Thread 1");
}

#[cfg(unix)]
#[tokio::test]
async fn browser_login_is_discovered_from_an_adapter_and_runs_the_native_loop() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("settings");
    std::fs::create_dir_all(&directory).unwrap();
    let script = r#"
while IFS= read -r request; do
case "$request" in
*'"initialize"'*) printf '%s\n' '{"id":1,"result":{"protocol":"lince.provider.v1"}}' ;;
*'"providers/list"'*) printf '%s\n' '{"id":2,"result":[{"id":"test-browser","label":"Test subscription","endpoint":"https://model.example/v1/","model_optional":true,"auth_methods":[{"id":"account","label":"Browser login","kind":"browser"},{"id":"key","label":"API key","kind":"api_key"}]}]}' ;;
*'"login/start"'*) printf '%s\n' '{"id":2,"result":{"url":"https://login.example/authorize"}}' ;;
*'"login/poll"'*) printf '%s\n' '{"id":3,"result":{"state":"complete","credential":"browser-secret"}}' ;;
*'"provider/complete"'*)
case "$request" in
*'"Tool"'*) printf '%s\n' '{"id":2,"result":{"reply":{"text":"Created the file through a native tool.","calls":[]},"credential":"refreshed-secret"}}' ;;
*) printf '%s\n' '{"id":2,"result":{"reply":{"text":"","calls":[{"id":"adapter-call","name":"create_file","arguments":{"path":"adapter.txt","content":"native file tool"},"signatures":null}]}}}' ;;
esac ;;
esac
done
"#;
    let driver = fiote::driver::Driver {
        executable: std::fs::canonicalize("/bin/sh").unwrap(),
        arguments: vec!["-c".into(), script.into()],
    };
    std::fs::write(
        directory.join("providers.json"),
        serde_json::to_vec(&vec![driver]).unwrap(),
    )
    .unwrap();
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let host = Host::open(engine.clone(), directory).await.unwrap();
    let record = engine
        .act(
            Action::CreateAgent {
                head: "Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let status = host.status(&record).await.unwrap();
    let descriptor = status
        .providers
        .iter()
        .find(|provider| provider.id.0 == "test-browser")
        .unwrap();
    assert_eq!(descriptor.auth_methods.len(), 2);
    let settings = Settings {
        enabled: true,
        provider: descriptor.id.clone(),
        auth_method: "account".into(),
        directory: root.path().into(),
        ..Default::default()
    };
    let status = host
        .handle(Request::BrowserStart {
            record: record.clone(),
            password: Secret("browser-password".into()),
            settings,
        })
        .await
        .unwrap();
    assert_eq!(
        status.login_url.as_deref(),
        Some("https://login.example/authorize")
    );
    assert!(status.login_pending);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !host.login.lock().await.as_ref().unwrap().task.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let status = host
        .handle(Request::BrowserPoll {
            record: record.clone(),
        })
        .await
        .unwrap();
    assert!(status.settings.enabled && !status.locked && !status.login_pending);
    let predicate = store::concepts::resolve(&host.engine.store.pool, "thread-of")
        .await
        .unwrap()
        .unwrap();
    let threads =
        store::assertions::subjects_pointing_to(&host.engine.store.pool, &predicate, &record)
            .await
            .unwrap();
    let thread = &threads
        .iter()
        .find(|thread| thread.head == "Thread 1")
        .unwrap()
        .uid;
    host.send(thread, "create a file").await.unwrap();
    wait(&host).await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("adapter.txt")).unwrap(),
        "native file tool"
    );
    assert!(
        rows(&host, thread)
            .await
            .iter()
            .any(|row| row.body == "Created the file through a native tool.")
    );
    assert_eq!(
        host.vault
            .key(&vault::slot(&status.settings))
            .await
            .unwrap()
            .unwrap()
            .0,
        "refreshed-secret"
    );
    let persisted = std::fs::read_to_string(host.path(&record).unwrap()).unwrap();
    assert!(!persisted.contains("secret"));
    for op in store::sync_ops::all_by_hlc(&engine.store.pool)
        .await
        .unwrap()
    {
        let value = op.value.unwrap_or_default();
        assert!(
            !value.contains("browser-secret")
                && !value.contains("refreshed-secret")
                && !value.contains("browser-password")
        );
    }
}

#[tokio::test]
async fn inherited_instructions_are_pinned_and_explicitly_refreshed() {
    let (host, _, _root, record, thread) = fixture(false).await;
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    let sources = host.prompt_sources(&record).await.unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].body, fiote::prompt::DEFAULT);
    assert_eq!(sources[1].body, "Be useful. This is my prompt.");
    let pinned = host.session_instructions(&record, &thread).await.unwrap();
    host.engine
        .act(
            Action::EditRecordText {
                target: parent.clone(),
                head: None,
                body: Some("New common instructions".into()),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        host.session_instructions(&record, &thread).await.unwrap(),
        pinned
    );
    let status = host
        .handle(Request::RefreshInstructions {
            record: record.clone(),
            thread: thread.clone(),
        })
        .await
        .unwrap();
    assert_eq!(status.instructions[0].body, "New common instructions");
    let updated = host.session_instructions(&record, &thread).await.unwrap();
    assert!(updated.contains("New common instructions"));
    assert!(!updated.contains(fiote::prompt::DEFAULT));
    assert!(
        host.engine
            .act(
                Action::ConfigureFiote {
                    target: parent,
                    prompt_parent: Some(record.clone()),
                    run_assigned: false
                },
                None
            )
            .await
            .is_err()
    );
    host.send(&thread, "hello").await.unwrap();
    wait(&host).await;
    let saved =
        store::records::get_extension(&host.engine.store.pool, &thread, "lince.fiote-session")
            .await
            .unwrap()
            .unwrap();
    assert!(saved["sources"][0].get("body").is_none());
    assert!(saved.get("system").is_none());
    assert_eq!(
        host.instruction_snapshot(&thread).unwrap().unwrap()["sources"][0]["body"],
        "New common instructions"
    );
}

#[tokio::test]
async fn preparing_plain_fiote_preserves_identity_text_and_numbers_threads() {
    let (host, _, _root, _, _) = fixture(false).await;
    let record = host
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "My agent".into(),
                body: "My private workflow".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.handle(Request::Prepare {
        record: record.clone(),
    })
    .await
    .unwrap();
    let row = host.record(&record).await.unwrap();
    assert_eq!(row.kind, "person");
    assert_eq!(row.body, "My private workflow");
    assert_eq!(row.head, "My agent");
    let first = host.record_threads(&record).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].head, "Thread 1");
    let second = host
        .engine
        .act(
            Action::CreateThread {
                target: record.clone(),
                head: String::new(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(host.record(&second).await.unwrap().head, "Thread 2");
    host.handle(Request::Prepare {
        record: record.clone(),
    })
    .await
    .unwrap();
    assert_eq!(host.record_threads(&record).await.unwrap().len(), 2);
    assert!(!host.status(&record).await.unwrap().settings.enabled);
}

#[tokio::test]
async fn assignment_starts_one_visible_session_and_survives_restart_without_replay() {
    let (host, script, root, record, _) = fixture(false).await;
    let task = host
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Assigned task".into(),
                body: "Do the assigned thing".into(),
                quantity: -1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.engine
        .act(
            Action::AssertRecord {
                subject: task.clone(),
                predicate: "assigned-to".into(),
                object: Some(record.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    host.assignment_tick().await.unwrap();
    wait(&host).await;
    host.assignment_tick().await.unwrap();
    let tasks = host.task_sessions(&record).await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].state, "finished");
    assert_eq!(tasks[0].task, task);
    assert!(
        host.record_threads(&task)
            .await
            .unwrap()
            .iter()
            .any(|thread| thread.uid == tasks[0].thread)
    );
    assert_eq!(host.thread_fiote(&tasks[0].thread).await.unwrap(), record);
    let opened = host
        .handle(Request::OpenTools {
            record: task.clone(),
            thread: tasks[0].thread.clone(),
        })
        .await
        .unwrap();
    assert_eq!(opened.record, record);
    assert_eq!(host.record(&task).await.unwrap().kind, "plain");
    host.handle(Request::CloseTools {
        record: task.clone(),
        thread: tasks[0].thread.clone(),
    })
    .await
    .unwrap();
    let messages = rows(&host, &tasks[0].thread).await;
    assert_eq!(messages.len(), 2);
    assert!(
        script.observed.lock().unwrap()[0]
            .0
            .contains("Do the assigned thing")
    );
    assert_eq!(host.load(&record).unwrap().unwrap().author, record);
    for _ in 0..3 {
        host.assignment_tick().await.unwrap();
    }
    assert_eq!(script.observed.lock().unwrap().len(), 1);
    let reopened = Host::open(host.engine.clone(), root.path().join("settings"))
        .await
        .unwrap();
    reopened.assignment_tick().await.unwrap();
    assert_eq!(reopened.task_sessions(&record).await.unwrap().len(), 1);
    assert_eq!(rows(&host, &tasks[0].thread).await.len(), 2);
}

#[tokio::test]
async fn agent_assignments_do_not_wake_agents_and_locked_jobs_wait() {
    let (host, script, _root, record, _) = fixture(false).await;
    let create = || Action::CreateRecord {
        slug: None,
        kind: RecordKind::Plain,
        head: "Task".into(),
        body: "Work".into(),
        quantity: 0.0,
    };
    let task = host
        .engine
        .act(create(), None)
        .await
        .unwrap()
        .created
        .unwrap();
    engine::operation_origin::fiote(
        &record,
        "session",
        host.engine.act(
            Action::AssertRecord {
                subject: task,
                predicate: "assigned-to".into(),
                object: Some(record.clone()),
                quantity: None,
                unit: None,
            },
            None,
        ),
    )
    .await
    .unwrap();
    host.assignment_tick().await.unwrap();
    assert!(host.task_sessions(&record).await.unwrap().is_empty());
    host.vault.lock().await;
    let task = host
        .engine
        .act(create(), None)
        .await
        .unwrap()
        .created
        .unwrap();
    host.engine
        .act(
            Action::AssertRecord {
                subject: task,
                predicate: "assigned-to".into(),
                object: Some(record.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    host.assignment_tick().await.unwrap();
    let tasks = host.task_sessions(&record).await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].state, "waiting");
    assert!(script.observed.lock().unwrap().is_empty());
    assert_eq!(rows(&host, &tasks[0].thread).await.len(), 1);
    assert!(
        rows(&host, &tasks[0].thread).await[0]
            .body
            .starts_with("Waiting to start:")
    );
    host.assignment_tick().await.unwrap();
    assert_eq!(rows(&host, &tasks[0].thread).await.len(), 1);
    host.vault
        .unlock(Secret("test-password".into()))
        .await
        .unwrap();
    host.assignment_tick().await.unwrap();
    wait(&host).await;
    host.assignment_tick().await.unwrap();
    assert_eq!(
        host.task_sessions(&record).await.unwrap()[0].state,
        "finished"
    );
}

#[tokio::test]
async fn native_and_external_tools_keep_instructions_outside_conversation_history() {
    let (host, script, _root, record, thread) = fixture(false).await;
    host.send(&thread, "create a file").await.unwrap();
    wait(&host).await;
    let system = host.session_instructions(&record, &thread).await.unwrap();
    assert_eq!(script.observed.lock().unwrap().len(), 2);
    assert!(
        script
            .observed
            .lock()
            .unwrap()
            .iter()
            .all(|(prompt, _)| prompt == &system)
    );
    let native = Session::local(
        host.engine.clone(),
        Arc::new(LaneHub::new()),
        "instructions-test",
    )
    .into_native_tools(transport::native::Context {
        record: record.clone(),
        agent: record,
        thread: thread.clone(),
    })
    .with_instructions(system.clone());
    let mut registry = Registry::default();
    native.register(&mut registry);
    let definition = registry
        .definitions()
        .into_iter()
        .find(|tool| tool.name == "lince_instructions")
        .unwrap();
    assert!(definition.description.contains(&system));
    let result = registry.run("lince_instructions", json!({})).await;
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["result"]["system"], system);
    assert!(host.instruction_snapshot("../../bad").is_err());
    native.close().await;
}

#[tokio::test]
async fn assignment_dispatch_and_lock_do_not_deadlock_or_restart_work() {
    let (host, _, _root, record, _) = fixture(true).await;
    let task = host
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Stop test".into(),
                body: "Work".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.engine
        .act(
            Action::AssertRecord {
                subject: task,
                predicate: "assigned-to".into(),
                object: Some(record.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let (tick, stopped) = tokio::join!(
            host.assignment_tick(),
            host.handle(Request::Lock {
                record: record.clone()
            })
        );
        tick.unwrap();
        stopped.unwrap();
    })
    .await
    .unwrap();
    wait(&host).await;
    host.assignment_tick().await.unwrap();
    let tasks = host.task_sessions(&record).await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(matches!(tasks[0].state.as_str(), "waiting" | "interrupted"));
    assert!(host.running.lock().await.is_empty());
}

#[tokio::test]
async fn human_identity_is_not_converted_and_deleted_ancestors_are_not_silently_skipped() {
    let (host, _, _root, record, _) = fixture(false).await;
    let human = host
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Person,
                head: "Human".into(),
                body: "Personal description".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        host.handle(Request::Prepare {
            record: human.clone()
        })
        .await
        .is_err()
    );
    assert!(
        store::records::get_extension(&host.engine.store.pool, &human, "lince.fiote")
            .await
            .unwrap()
            .is_none()
    );
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    host.engine
        .act(Action::DeleteRecord { target: parent }, None)
        .await
        .unwrap();
    let status = host.handle(Request::Inspect { record }).await.unwrap();
    assert!(status.instruction_error.is_some());
    assert!(status.instructions.is_empty());
}

#[tokio::test]
async fn development_seed_inherits_default_and_preserves_user_changes() {
    let (host, _, _directory, existing, _) = fixture(false).await;
    let choices = host.fiote_choices().await.unwrap();
    let development = choices
        .iter()
        .find(|choice| choice.title == "Development Fiote")
        .unwrap()
        .record
        .clone();
    let sources = host.prompt_sources(&development).await.unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].body, fiote::prompt::DEFAULT);
    assert_eq!(sources[1].body, fiote::prompt::DEVELOPMENT);
    assert!(host.behavior(&development).await.unwrap().run_assigned);
    assert!(
        !host
            .behavior(&sources[0].record)
            .await
            .unwrap()
            .run_assigned
    );
    let threads = host.record_threads(&development).await.unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].head, "Thread 1");
    host.engine
        .act(
            Action::EditRecordText {
                target: development.clone(),
                head: None,
                body: Some("My revised workflow".into()),
            },
            None,
        )
        .await
        .unwrap();
    host.prepare(&existing).await.unwrap();
    host.prepare(&development).await.unwrap();
    assert_eq!(host.fiote_choices().await.unwrap().len(), choices.len());
    assert_eq!(
        host.record(&development).await.unwrap().body,
        "My revised workflow"
    );
    assert_eq!(host.record_threads(&development).await.unwrap().len(), 1);
    assert_eq!(
        host.record(&existing).await.unwrap().body,
        "Be useful. This is my prompt."
    );
}

#[tokio::test]
async fn fiote_timeline_keeps_replies_tools_and_removable_transcripts_in_order() {
    let (host, _, directory, record, thread) = fixture(false).await;
    let reply = host
        .engine
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: String::new(),
                author: Some(record.clone()),
                state: MessageState::Writing,
                parent: None,
                references: vec![],
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let native = transport::Session::local(
        host.engine.clone(),
        Arc::new(transport::LaneHub::new()),
        nucleus::new_uid("timeline"),
    )
    .into_native_tools(transport::native::Context {
        agent: record.clone(),
        record: record.clone(),
        thread: thread.clone(),
    });
    native.attach_message(&reply).await.unwrap();
    let mut tools = Registry::default();
    native.register(&mut tools);
    let timeline = timeline::Timeline {
        engine: host.engine.clone(),
        tools: &tools,
        author: record.clone(),
        thread: thread.clone(),
        pending: directory.path().join("pending.json"),
        state: Mutex::new(timeline::State {
            message: reply.clone(),
            text: String::new(),
            offset: 0,
            closed: false,
            message_id: None,
            calls: HashMap::new(),
        }),
    };
    timeline.update("Inspecting files.").await.unwrap();
    host.engine
        .act(
            Action::EditRecordText {
                target: reply.clone(),
                head: None,
                body: Some("User note. Inspecting files.".into()),
            },
            None,
        )
        .await
        .unwrap();
    timeline.activity(&serde_json::json!({"sessionUpdate":"tool_call","toolCallId":"read","title":"Ran tail log","status":"in_progress","rawInput":{"command":"tail log"}})).await.unwrap();
    timeline.activity(&serde_json::json!({"sessionUpdate":"tool_call_update","toolCallId":"read","status":"completed","content":[{"type":"content","content":{"type":"text","text":"one\ntwo\nthree\nfour\nfive"}}]})).await.unwrap();
    timeline
        .update("Inspecting files.Finished verification.")
        .await
        .unwrap();
    timeline
        .finish("Inspecting files.Finished verification.", false)
        .await
        .unwrap();
    let messages = rows(&host, &thread).await;
    assert_eq!(messages.len(), 3);
    assert_eq!(
        host.record(&reply).await.unwrap().body,
        "User note. Inspecting files."
    );
    assert!(
        messages
            .iter()
            .any(|row| row.body == "Finished verification.")
    );
    let summary = messages
        .iter()
        .find(|row| row.body == "Ran tail log")
        .unwrap();
    let metadata =
        store::records::get_extension(&host.engine.store.pool, &summary.uid, "lince.tool-call")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(metadata["status"], "completed");
    assert!(metadata["preview"].as_str().unwrap().contains("+2 lines"));
    let transcript = metadata["thread"].as_str().unwrap();
    assert_eq!(rows(&host, transcript).await.len(), 2);
    host.engine
        .act(
            Action::DeleteRecord {
                target: transcript.into(),
            },
            None,
        )
        .await
        .unwrap();
    timeline.activity(&serde_json::json!({"sessionUpdate":"tool_call_update","toolCallId":"read","rawOutput":"late output"})).await.unwrap();
    assert_eq!(rows(&host, transcript).await.len(), 2);
    assert_eq!(rows(&host, &thread).await.len(), 3);
    native.close().await;
}

#[tokio::test]
async fn fiote_parent_is_an_assertion_and_native_edits_reject_cycles() {
    let (host, _, _directory, record, _) = fixture(false).await;
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    let predicate = store::concepts::resolve(&host.engine.store.pool, "descendant-of")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        store::assertions::object_uids_from_subject(&host.engine.store.pool, &record, &predicate)
            .await
            .unwrap(),
        vec![parent.clone()]
    );
    assert!(
        host.engine
            .act(
                Action::AssertRecord {
                    subject: parent,
                    predicate: predicate.clone(),
                    object: Some(record.clone()),
                    quantity: None,
                    unit: None
                },
                None
            )
            .await
            .is_err()
    );
    let assertion: String = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid = ? AND predicate_uid = ? AND retracted_at IS NULL")
        .bind(&record).bind(&predicate).fetch_one(&host.engine.store.pool).await.unwrap();
    host.engine
        .act(Action::RetractAssertion { assertion }, None)
        .await
        .unwrap();
    assert!(
        host.behavior(&record)
            .await
            .unwrap()
            .prompt_parent
            .is_none()
    );
}
