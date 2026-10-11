use super::*;

async fn set_directory(host: &Host, record: &str, directory: &Path) -> Status {
    host.handle(Request::WorkingDirectory {
        record: record.into(),
        directory: directory.into(),
    })
    .await
    .unwrap()
}

async fn child(host: &Host, parent: &str, title: &str) -> String {
    let record = host
        .engine
        .act(
            Action::CreateAgent {
                head: title.into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.handle(Request::Behavior {
        record: record.clone(),
        prompt_parent: Some(parent.into()),
        run_assigned: false,
    })
    .await
    .unwrap();
    record
}

async fn assert_directory(host: &Host, record: &str, directory: &Path, source: &str) {
    let status = host.working_directory_status(record).await;
    assert_eq!(status.error, None);
    assert_eq!(status.effective, Some(directory.canonicalize().unwrap()));
    assert_eq!(status.source.as_deref(), Some(source));
}

#[tokio::test]
async fn working_directory_inheritance_cascades_overrides_and_survives_reopening() {
    let (host, _, root, record, _) = fixture(false).await;
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    let grandchild = child(&host, &record, "Grandchild").await;
    let leaf = child(&host, &grandchild, "Leaf").await;
    let sibling = child(&host, &parent, "Sibling").await;
    let paths = ["first", "branch", "last", "leaf"].map(|name| {
        let path = root.path().join(name);
        std::fs::create_dir(&path).unwrap();
        path
    });
    set_directory(&host, &record, Path::new("")).await;
    let status = set_directory(&host, &parent, &paths[0]).await;
    assert!(!status.settings.enabled);
    assert!(!status.has_key);
    for uid in [&record, &grandchild, &leaf, &sibling] {
        assert_directory(&host, uid, &paths[0], &parent).await;
    }
    set_directory(&host, &record, &paths[1]).await;
    set_directory(&host, &grandchild, &paths[3]).await;
    set_directory(&host, &parent, &paths[2]).await;
    assert_directory(&host, &record, &paths[1], &record).await;
    assert_directory(&host, &grandchild, &paths[3], &grandchild).await;
    assert_directory(&host, &leaf, &paths[3], &grandchild).await;
    assert_directory(&host, &sibling, &paths[2], &parent).await;
    let reopened = Host::open(host.engine.clone(), host.directory.clone())
        .await
        .unwrap();
    assert_directory(&reopened, &leaf, &paths[3], &grandchild).await;
    assert!(reopened.load(&leaf).unwrap().is_none());
    set_directory(&reopened, &record, Path::new("")).await;
    assert_directory(&reopened, &record, &paths[2], &parent).await;
    assert_directory(&reopened, &leaf, &paths[3], &grandchild).await;
    set_directory(&reopened, &grandchild, Path::new("")).await;
    assert_directory(&reopened, &leaf, &paths[2], &parent).await;
    assert!(
        reopened
            .load(&grandchild)
            .unwrap()
            .unwrap()
            .settings
            .directory
            .as_os_str()
            .is_empty()
    );
    set_directory(&reopened, &parent, Path::new("")).await;
    for uid in [&record, &grandchild, &leaf, &sibling] {
        assert!(
            reopened
                .effective_working_directory(uid)
                .await
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn working_directory_native_turns_follow_parent_changes_in_the_same_conversation() {
    let (host, _, root, record, thread) = fixture(false).await;
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    let first = root.path().join("first");
    let second = root.path().join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    set_directory(&host, &record, Path::new("")).await;
    set_directory(&host, &parent, &first).await;
    host.send(&thread, "create a file").await.unwrap().unwrap();
    wait(&host).await;
    set_directory(&host, &parent, &second).await;
    host.send(&thread, "create a file").await.unwrap().unwrap();
    wait(&host).await;
    for directory in [&first, &second] {
        assert_eq!(
            std::fs::read_to_string(directory.join("hello.txt")).unwrap(),
            "Hello from Fiote"
        );
    }
    assert!(!root.path().join("hello.txt").exists());
    assert!(
        host.load(&record)
            .unwrap()
            .unwrap()
            .settings
            .directory
            .as_os_str()
            .is_empty()
    );
}

#[tokio::test]
async fn working_directory_reparenting_updates_descendants() {
    let (host, _, root, record, _) = fixture(false).await;
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    let grandchild = child(&host, &record, "Grandchild").await;
    let other = child(&host, &parent, "Other branch").await;
    let path = root.path().join("other");
    std::fs::create_dir(&path).unwrap();
    set_directory(&host, &parent, root.path()).await;
    set_directory(&host, &record, Path::new("")).await;
    set_directory(&host, &other, &path).await;
    host.handle(Request::Behavior {
        record: record.clone(),
        prompt_parent: Some(other.clone()),
        run_assigned: false,
    })
    .await
    .unwrap();
    assert_directory(&host, &record, &path, &other).await;
    assert_directory(&host, &grandchild, &path, &other).await;
    set_directory(&host, &record, root.path()).await;
    assert_directory(&host, &grandchild, root.path(), &record).await;
}

#[tokio::test]
async fn working_directory_missing_override_fails_without_falling_back_to_an_ancestor() {
    let (host, _, root, record, thread) = fixture(false).await;
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    set_directory(&host, &parent, root.path()).await;
    let path = root.path().join("removed");
    std::fs::create_dir(&path).unwrap();
    set_directory(&host, &record, &path).await;
    std::fs::remove_dir(&path).unwrap();
    let status = host.working_directory_status(&record).await;
    assert!(status.error.unwrap().contains(&record));
    assert!(status.effective.is_none());
    assert!(
        host.send(&thread, "create a file")
            .await
            .unwrap_err()
            .contains(&record)
    );
    assert!(!root.path().join("hello.txt").exists());
    set_directory(&host, &record, Path::new("")).await;
    assert_directory(&host, &record, root.path(), &parent).await;
    for invalid in [PathBuf::from("relative"), root.path().join("missing")] {
        assert!(
            host.handle(Request::WorkingDirectory {
                record: record.clone(),
                directory: invalid
            })
            .await
            .is_err()
        );
        assert_directory(&host, &record, root.path(), &parent).await;
    }
}

#[derive(Default)]
struct DelayedFile {
    ready: tokio::sync::Notify,
    proceed: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl Provider for DelayedFile {
    async fn complete(
        &self,
        _system: &str,
        messages: &[Message],
        _tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        if let Some(Message::Tool { result, .. }) = messages.last() {
            assert_eq!(result["ok"], true);
            return Ok(Reply {
                text: "File created.".into(),
                ..Default::default()
            });
        }
        self.ready.notify_one();
        self.proceed.notified().await;
        Ok(Reply {
            calls: vec![ToolCall {
                id: format!(
                    "delayed-file-{}",
                    messages
                        .iter()
                        .filter(|message| matches!(message, Message::Tool { .. }))
                        .count()
                        + 1
                ),
                name: "create_file".into(),
                arguments: json!({"path":"hello.txt", "content":"world"}),
                signatures: None,
            }],
            ..Default::default()
        })
    }
}

#[tokio::test]
async fn working_directory_changes_preserve_running_turns_and_apply_to_future_turns() {
    let (mut host, _, root, record, thread) = fixture(false).await;
    let provider = Arc::new(DelayedFile::default());
    Arc::get_mut(&mut host).unwrap().provider = Some(provider.clone());
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    set_directory(&host, &record, Path::new("")).await;
    set_directory(&host, &parent, root.path()).await;
    host.send(&thread, "Hello").await.unwrap().unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        provider.ready.notified(),
    )
    .await
    .unwrap();
    let next = root.path().join("next");
    std::fs::create_dir(&next).unwrap();
    assert!(
        host.handle(Request::WorkingDirectory {
            record: record.clone(),
            directory: next.clone()
        })
        .await
        .unwrap_err()
        .contains("Stop this Fiote")
    );
    set_directory(&host, &parent, &next).await;
    assert!(host.running.lock().await.contains_key(&thread));
    assert_directory(&host, &record, &next, &parent).await;
    provider.proceed.notify_one();
    wait(&host).await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("hello.txt")).unwrap(),
        "world"
    );
    assert!(!next.join("hello.txt").exists());
    host.send(&thread, "Create another file")
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        provider.ready.notified(),
    )
    .await
    .unwrap();
    provider.proceed.notify_one();
    wait(&host).await;
    assert_eq!(
        std::fs::read_to_string(next.join("hello.txt")).unwrap(),
        "world"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn working_directory_activations_execute_commands_in_the_inherited_directory() {
    let (mut host, _, root, record, _) = fixture(false).await;
    Arc::get_mut(&mut host).unwrap().provider = Some(Arc::new(ActivationCommandProvider));
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    let inherited = root.path().join("inherited");
    std::fs::create_dir(&inherited).unwrap();
    set_directory(&host, &record, Path::new("")).await;
    set_directory(&host, &parent, &inherited).await;
    host.handle(Request::Activate {
        record: record.clone(),
        value: "1".into(),
        request_id: "inherited-command".into(),
    })
    .await
    .unwrap();
    host.activation_tick().await.unwrap();
    wait(&host).await;
    host.activation_tick().await.unwrap();
    assert_eq!(
        host.engine.fiote_activations(&record).await.unwrap()[0].state,
        "finished"
    );
    assert_eq!(
        std::fs::read_to_string(inherited.join("Hello.txt")).unwrap(),
        "world"
    );
    assert!(!root.path().join("Hello.txt").exists());
}

#[tokio::test]
async fn working_directory_remote_sessions_cannot_change_local_execution_paths() {
    let (host, _, root, record, _) = fixture(false).await;
    let original = host.load(&record).unwrap().unwrap().settings.directory;
    let mut session = Session::new(
        host.engine.clone(),
        Arc::new(LaneHub::new()),
        "remote",
        None,
    )
    .with_fiote(host.clone());
    let response = session
        .handle(ClientMessage::Fiote {
            id: "directory".into(),
            request: Request::WorkingDirectory {
                record: record.clone(),
                directory: root.path().join("other"),
            },
        })
        .await;
    assert!(matches!(&response[0], ServerMessage::Error { .. }));
    assert_eq!(
        host.load(&record).unwrap().unwrap().settings.directory,
        original
    );
}

#[tokio::test]
#[ignore = "Requires LINCE_TEST_AGENT_BIN pointing to lince-acp-test-agent; uses no model"]
async fn working_directory_external_sessions_reopen_when_the_inherited_directory_changes() {
    let (host, _, root, record, thread) = fixture(false).await;
    let parent = host.behavior(&record).await.unwrap().prompt_parent.unwrap();
    let log = root.path().join("sessions.log");
    let config = serde_json::from_value(json!({
        "command": std::env::var("LINCE_TEST_AGENT_BIN").unwrap(),
        "args": [], "directory": root.path(),
        "environment": {"TEST_HOST_WORKFLOW":"1", "TEST_SESSION_LOG": log}
    }))
    .unwrap();
    host.handle(Request::AgentConfigure {
        record: record.clone(),
        config,
    })
    .await
    .unwrap();
    set_directory(&host, &record, Path::new("")).await;
    for name in ["first", "second"] {
        let directory = root.path().join(name);
        std::fs::create_dir(&directory).unwrap();
        set_directory(&host, &parent, &directory).await;
        let status = host
            .handle(Request::SessionOptions {
                thread: thread.clone(),
            })
            .await
            .unwrap();
        assert!(status.agent_session.unwrap().connected);
        host.send(&thread, "Create a file in your working directory.")
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            while !host.running.lock().await.is_empty() {
                for activity in host.agents.activity(&record).await {
                    if let Some(permission) = activity.permission {
                        host.agents
                            .answer(&record, &thread, &permission.id, Some("allow".into()))
                            .await
                            .unwrap();
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.join("hello.txt")).unwrap(),
            "Hello from the agent"
        );
    }
    let sessions: Vec<serde_json::Value> = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(sessions.len(), 2);
    assert_eq!(
        sessions[0]["cwd"],
        root.path()
            .join("first")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert_eq!(
        sessions[1]["cwd"],
        root.path()
            .join("second")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert!(!root.path().join("hello.txt").exists());
    host.stop_all().await;
}
