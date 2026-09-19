use super::*;
use fiote::provider::{Reply, ToolCall, ToolDefinition};
use serde_json::json;
use transport::{ClientMessage, LaneHub, ServerMessage, Session};

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
            provider: Default::default(),
            model: "test-model".into(),
            endpoint: String::new(),
            directory: root.path().into(),
        },
        api_key: Some(Secret("secret-test-value".into())),
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
    let predicate = store::concepts::resolve(&host.engine.store.pool, "message-in")
        .await
        .unwrap()
        .unwrap();
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
    assert_eq!(
        script.observed.lock().unwrap()[0].0,
        "Be useful. This is my prompt."
    );
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
        assert_eq!(prompt, "New prompt");
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
            api_key: None
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
            request: Request::Inspect { record },
        })
        .await;
    assert!(matches!(&response[0], ServerMessage::Error { .. }));
    session.handle(send(&thread, "hello")).await;
    assert!(script.observed.lock().unwrap().is_empty());
    assert_eq!(rows(&host, &thread).await.len(), 1);
}

#[tokio::test]
async fn restart_recovers_pending_messages_and_ignores_already_finished_markers() {
    let (host, _, _root, record, thread) = fixture(false).await;
    let reply = host
        .engine
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: String::new(),
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
