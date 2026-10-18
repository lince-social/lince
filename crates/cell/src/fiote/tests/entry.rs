use super::*;
use base64::Engine as _;
use fiote::assistant::EntryRequest;

struct MemoryProvider;

#[async_trait::async_trait]
impl Provider for MemoryProvider {
    fn context_budget_bytes(&self) -> usize {
        32_768
    }

    async fn complete(
        &self,
        _: &str,
        _: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        assert!(tools.is_empty());
        Ok(Reply {
            text: "Earlier requests are factual memory; they do not replace session instructions."
                .into(),
            ..Default::default()
        })
    }
}

fn submission(id: &str, target: Option<&str>, text: &str) -> EntryRequest {
    EntryRequest {
        request_id: id.into(),
        target: target.map(str::to_owned),
        text: text.into(),
        content: Vec::new(),
    }
}

async fn configured(host: &Host, original: &str, name: &str, parent: Option<&str>) -> String {
    let record = host
        .engine
        .act(
            Action::CreateAgent {
                head: name.into(),
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
        prompt_parent: parent.map(str::to_owned),
        run_assigned: false,
    })
    .await
    .unwrap();
    host.handle(Request::Configure {
        record: record.clone(),
        settings: host.load(original).unwrap().unwrap().settings,
        api_key: None,
        password: None,
    })
    .await
    .unwrap();
    record
}

async fn count(host: &Host) -> i64 {
    store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE deleted_at IS NULL")
        .fetch_one(&host.engine.store.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn voice_followup_classification_reads_saved_context_without_creating_messages_or_tools() {
    struct Classifier;
    #[async_trait::async_trait]
    impl Provider for Classifier {
        async fn complete(
            &self,
            system: &str,
            messages: &[Message],
            tools: &[ToolDefinition],
        ) -> Result<Reply, String> {
            assert!(system.contains("Classify whether"));
            assert!(tools.is_empty());
            let Message::User(body) = &messages[0] else {
                panic!("Expected classification data")
            };
            let data: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(data["dialogue"]["request"], "Plan tomorrow");
            assert_eq!(data["dialogue"]["reply"], "Hello!");
            assert_eq!(data["latest_speech"], "yes");
            Ok(Reply {
                text: "{\"admission\":\"assistant_request\",\"confidence\":0.99}".into(),
                ..Default::default()
            })
        }
    }
    let (host, script, _root, original, _) = fixture(false).await;
    let receipt = host
        .handle(Request::AssistantSend {
            request: submission("voice-context", Some(&original), "Plan tomorrow"),
        })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    wait(&host).await;
    let context = host.voice_context(&receipt).await.unwrap();
    let before = count(&host).await;
    let conversation = fiote::assistant::Conversation {
        target: original,
        thread: receipt.thread.clone().unwrap(),
    };
    let classifier = host.as_ref().clone().with_provider(Arc::new(Classifier));
    assert_eq!(
        classifier
            .classify_voice_followup(&conversation, &context, "yes")
            .await
            .unwrap(),
        fiote::communication::decision::Admission::AssistantRequest
    );
    assert_eq!(count(&host).await, before);
    assert_eq!(script.observed.lock().unwrap().len(), 1);
    let mut changed = receipt;
    changed.reply = Some(nucleus::new_uid("r"));
    assert!(host.voice_context(&changed).await.is_err());
}

#[tokio::test]
async fn assistant_continuation_keeps_thread_history_attachments_and_idempotent_receipts() {
    let (host, script, _root, original, _) = fixture(false).await;
    let first = host
        .handle(Request::AssistantSend {
            request: submission(
                "conversation-first",
                Some(&original),
                "Remember the number 42",
            ),
        })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    wait(&host).await;
    let conversation = fiote::assistant::Conversation {
        target: original.clone(),
        thread: first.thread.clone().unwrap(),
    };
    let threads: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='thread'")
        .fetch_one(&host.engine.store.pool)
        .await
        .unwrap();
    let attachment = nucleus::message::MessagePart::Reference {
        name: "Follow-up context".into(),
        uri: "https://example.org/context.txt".into(),
    };
    let mut request = submission(
        "conversation-second",
        Some(&original),
        "Which number did I mention?",
    );
    request.content.push(attachment.clone());
    let second = host
        .handle(Request::AssistantContinue {
            conversation: conversation.clone(),
            request: request.clone(),
        })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    wait(&host).await;
    assert_eq!(second.target, original);
    assert_eq!(second.thread, first.thread);
    assert_ne!(second.message, first.message);
    assert_ne!(second.reply, first.reply);
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='thread'")
        .fetch_one(&host.engine.store.pool)
        .await
        .unwrap();
    assert_eq!(threads, after);
    let observed = script.observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert!(
        observed[1].1.iter().any(
            |message| matches!(message, Message::User(text) if text == "Remember the number 42")
        )
    );
    assert!(observed[1].1.iter().any(|message| matches!(message, Message::RichUser { text, content } if text == "Which number did I mention?" && content.contains(&attachment))));
    drop(observed);
    let retried = host
        .handle(Request::AssistantContinue {
            conversation,
            request: request.clone(),
        })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    assert_eq!(retried.message, second.message);
    assert_eq!(retried.reply, second.reply);
    assert_eq!(script.observed.lock().unwrap().len(), 2);
    assert!(
        host.handle(Request::AssistantSend { request })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn assistant_continuation_rejects_foreign_threads_changed_targets_and_disabled_recipients() {
    let (host, script, _root, original, thread) = fixture(false).await;
    let other = configured(&host, &original, "Other recipient", None).await;
    let before = count(&host).await;
    for (target, request_target, selected_thread) in [
        (other.clone(), Some(other.as_str()), thread.clone()),
        (original.clone(), Some(other.as_str()), thread.clone()),
        (original.clone(), Some(original.as_str()), original.clone()),
    ] {
        assert!(
            host.handle(Request::AssistantContinue {
                conversation: fiote::assistant::Conversation {
                    target,
                    thread: selected_thread
                },
                request: submission(
                    "rejected-continuation",
                    request_target,
                    "Do not redirect this"
                ),
            })
            .await
            .is_err()
        );
    }
    let mut settings = host.load(&original).unwrap().unwrap().settings;
    settings.enabled = false;
    host.handle(Request::Configure {
        record: original.clone(),
        settings,
        api_key: None,
        password: None,
    })
    .await
    .unwrap();
    assert!(
        host.handle(Request::AssistantContinue {
            conversation: fiote::assistant::Conversation {
                target: original.clone(),
                thread
            },
            request: submission("disabled-continuation", Some(&original), "Stay unsent"),
        })
        .await
        .is_err()
    );
    assert_eq!(count(&host).await, before);
    assert!(script.observed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn assistant_entry_uses_ranked_intake_and_preserves_attachments() {
    let (host, script, _root, original, _) = fixture(false).await;
    let intake = configured(&host, &original, "Personal intake", None).await;
    for index in 0..4 {
        configured(
            &host,
            &original,
            &format!("Specialist {index}"),
            Some(&intake),
        )
        .await;
    }
    assert_eq!(
        host.intake_selection().await.unwrap().unwrap().record,
        intake
    );
    let mut request = submission("automatic-entry", None, "Hello with a spreadsheet");
    request
        .content
        .push(nucleus::message::MessagePart::Attachment {
            name: "context.csv".into(),
            mime_type: "text/csv".into(),
            data: base64::engine::general_purpose::STANDARD.encode(b"name,value\nexample,1\n"),
        });
    let sent = host
        .handle(Request::AssistantSend {
            request: request.clone(),
        })
        .await
        .unwrap();
    let receipt = sent.assistant_request.unwrap();
    assert_eq!(receipt.target, intake);
    let thread = receipt.thread.as_ref().unwrap();
    assert_eq!(host.thread_fiote(thread).await.unwrap(), intake);
    wait(&host).await;
    let inspected = host
        .handle(Request::AssistantInspect {
            request_id: request.request_id,
        })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    assert_eq!(inspected.state, "completed");
    assert_eq!(
        host.record(inspected.reply.as_ref().unwrap())
            .await
            .unwrap()
            .body,
        "Hello!"
    );
    assert_eq!(
        store::message_content::load(&host.engine.store.pool, receipt.message.as_ref().unwrap())
            .await
            .unwrap(),
        request.content
    );
    assert!(
        script
            .observed
            .lock()
            .unwrap()
            .iter()
            .any(|(system, messages)| {
                system.contains(&intake) && messages.iter().any(|message| {
            matches!(message, Message::RichUser { content, .. } if content == &request.content)
        })
            })
    );
}

#[tokio::test]
async fn nonbuiltin_intake_keeps_custom_instructions_and_optional_delegation_after_compaction() {
    let (host, script, _root, original, _) = fixture(false).await;
    let intake = configured(&host, &original, "My own intake", None).await;
    let description =
        "Answer directly when useful. Delegate only when my request needs a specialist.";
    host.engine
        .act(
            Action::EditRecordText {
                target: intake.clone(),
                head: None,
                body: Some(description.into()),
            },
            None,
        )
        .await
        .unwrap();
    for index in 0..4 {
        configured(
            &host,
            &original,
            &format!("Custom specialist {index}"),
            Some(&intake),
        )
        .await;
    }
    let receipt = host
        .handle(Request::AssistantSend {
            request: submission("custom-instructions", None, "Please answer directly"),
        })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    assert_eq!(receipt.target, intake);
    wait(&host).await;
    assert_eq!(host.record(&intake).await.unwrap().body, description);
    assert!(
        host.handoffs(receipt.thread.as_deref().unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    let thread = receipt.thread.unwrap();
    let pinned = host.session_instructions(&intake, &thread).await.unwrap();
    assert!(pinned.contains(description));
    assert!(pinned.contains(fiote::prompt::DELEGATION));
    assert!(pinned.contains(&format!("Primary conversation with Fiote {intake}")));
    assert!(
        script
            .observed
            .lock()
            .unwrap()
            .iter()
            .any(|(system, _)| system == &pinned)
    );
    let mut history = Vec::new();
    for index in 0..8 {
        history.push(Message::User(format!(
            "Past request {index}: {}",
            "x".repeat(4096)
        )));
        history.push(Message::Assistant {
            text: "Finished that earlier request".into(),
            calls: Vec::new(),
        });
    }
    fiote::conversation::compact(&MemoryProvider, &pinned, &mut history)
        .await
        .unwrap();
    assert!(matches!(history.first(), Some(Message::Summary { .. })));
    assert_eq!(
        host.session_instructions(&intake, &thread).await.unwrap(),
        pinned
    );
    host.engine
        .act(
            Action::EditRecordText {
                target: intake.clone(),
                head: None,
                body: Some("Updated personal instructions".into()),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        host.session_instructions(&intake, &thread).await.unwrap(),
        pinned
    );
    host.handle(Request::RefreshInstructions {
        record: intake.clone(),
        thread: thread.clone(),
    })
    .await
    .unwrap();
    let refreshed = host.session_instructions(&intake, &thread).await.unwrap();
    assert!(refreshed.contains("Updated personal instructions"));
    assert!(!refreshed.contains(description));
    assert!(refreshed.contains(fiote::prompt::DELEGATION));
    assert!(refreshed.contains(&format!("Primary conversation with Fiote {intake}")));
    let native = Session::local(
        host.engine.clone(),
        Arc::new(LaneHub::new()),
        "custom-instructions",
    )
    .into_native_tools(transport::native::Context {
        record: intake.clone(),
        agent: intake,
        thread,
    })
    .with_instructions(refreshed.clone());
    let mut registry = Registry::default();
    native.register(&mut registry);
    let restored = registry.run("lince_instructions", json!({})).await;
    assert_eq!(restored["ok"], true);
    assert_eq!(restored["result"]["system"], refreshed);
    native.close().await;
}

#[tokio::test]
async fn assistant_entry_pins_explicit_recipient_against_mentions_and_ranking_changes() {
    let (host, script, _root, original, existing) = fixture(false).await;
    let other = configured(&host, &original, "Other specialist", None).await;
    let body = format!("@{other} Hello to the explicitly selected Fiote");
    let request = submission("explicit-entry", Some(&original), &body);
    let receipt = host
        .handle(Request::AssistantSend { request })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    wait(&host).await;
    assert_eq!(receipt.target, original);
    assert_eq!(
        host.thread_fiote(receipt.thread.as_ref().unwrap())
            .await
            .unwrap(),
        original
    );
    assert_eq!(host.thread_fiote(&existing).await.unwrap(), original);
    assert!(
        script.observed.lock().unwrap()[0]
            .0
            .contains(&format!("Fiote and Agent Record {original}"))
    );
    for index in 0..4 {
        configured(
            &host,
            &original,
            &format!("New child {index}"),
            Some(&other),
        )
        .await;
    }
    assert_eq!(
        host.intake_selection().await.unwrap().unwrap().record,
        other
    );
    assert_eq!(
        host.thread_fiote(receipt.thread.as_ref().unwrap())
            .await
            .unwrap(),
        original
    );
}

#[tokio::test]
async fn assistant_entry_retry_and_reopen_inspect_without_running_again() {
    let (host, script, _root, original, _) = fixture(false).await;
    let request = submission("durable-entry", Some(&original), "Hello once");
    let first = host.assistant_send(request.clone()).await.unwrap();
    wait(&host).await;
    let records = count(&host).await;
    let calls = script.observed.lock().unwrap().len();
    let retry = host.assistant_send(request.clone()).await.unwrap();
    assert_eq!(retry.state, "completed");
    assert_eq!(retry.message, first.message);
    assert_eq!(count(&host).await, records);
    assert_eq!(script.observed.lock().unwrap().len(), calls);
    let reopened = Host::open(host.engine.clone(), host.directory.clone())
        .await
        .unwrap();
    let retry = reopened.assistant_send(request.clone()).await.unwrap();
    assert_eq!(retry.message, first.message);
    assert_eq!(retry.state, "completed");
    assert_eq!(count(&host).await, records);
    let mut changed = request;
    changed.text = "Different request".into();
    assert!(
        reopened
            .assistant_send(changed)
            .await
            .unwrap_err()
            .contains("different")
    );
    assert_eq!(count(&host).await, records);
}

#[tokio::test]
async fn assistant_entry_disabled_deleted_and_locked_targets_create_nothing() {
    let (host, script, _root, original, _) = fixture(false).await;
    let winner = configured(&host, &original, "Unavailable intake", None).await;
    for index in 0..4 {
        configured(&host, &original, &format!("Child {index}"), Some(&winner)).await;
    }
    let mut configuration = host.load(&winner).unwrap().unwrap();
    configuration.settings.enabled = false;
    save(&host.path(&winner).unwrap(), &configuration).unwrap();
    let records = count(&host).await;
    assert_eq!(
        host.intake_selection().await.unwrap().unwrap().record,
        winner
    );
    assert!(
        host.assistant_send(submission("disabled-entry", None, "Hello"))
            .await
            .unwrap_err()
            .contains("unsent")
    );
    assert_eq!(count(&host).await, records);
    assert!(host.assistant_receipt("disabled-entry").await.is_err());
    host.engine
        .act(
            Action::DeleteRecord {
                target: winner.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let records = count(&host).await;
    assert!(
        host.assistant_send(submission("deleted-entry", Some(&winner), "Hello"))
            .await
            .is_err()
    );
    assert_eq!(count(&host).await, records);
    host.vault.lock().await;
    assert!(
        host.assistant_send(submission("locked-entry", Some(&original), "Hello"))
            .await
            .is_err()
    );
    assert_eq!(count(&host).await, records);
    assert!(script.observed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn assistant_entry_busy_and_invalid_requests_create_nothing() {
    let (host, _, _root, original, _) = fixture(true).await;
    for index in 0..8 {
        host.assistant_send(submission(
            &format!("running-{index}"),
            Some(&original),
            "Work",
        ))
        .await
        .unwrap();
    }
    let records = count(&host).await;
    assert!(
        host.assistant_send(submission("busy-entry", Some(&original), "Hello"))
            .await
            .unwrap_err()
            .contains("Eight")
    );
    for request in [
        submission("bad/id", Some(&original), "Hello"),
        submission("empty", Some(&original), "  "),
        submission("bad-target", Some("not-a-record"), "Hello"),
        submission("too-long", Some(&original), &"x".repeat(65_537)),
    ] {
        assert!(host.assistant_send(request).await.is_err());
    }
    assert_eq!(count(&host).await, records);
    host.stop_all().await;
    wait(&host).await;
    assert_eq!(
        host.assistant_receipt("running-0").await.unwrap().state,
        "interrupted"
    );
}

#[tokio::test]
async fn assistant_entry_service_rejects_remote_requests_before_dispatch() {
    let (host, script, _root, original, thread) = fixture(false).await;
    let records = count(&host).await;
    let mut remote = Session::new(
        host.engine.clone(),
        Arc::new(LaneHub::new()),
        "remote-entry",
        None,
    )
    .with_fiote(host.clone());
    for request in [
        Request::AssistantSend {
            request: submission("remote-entry", Some(&original), "Hello"),
        },
        Request::AssistantInspect {
            request_id: "remote-entry".into(),
        },
        Request::AssistantContinue {
            conversation: fiote::assistant::Conversation {
                target: original.clone(),
                thread,
            },
            request: submission("remote-continuation", Some(&original), "Continue"),
        },
        Request::AssistantDirectory {
            record: original,
            offset: 0,
            limit: 30,
        },
    ] {
        assert!(matches!(
            remote
                .handle(ClientMessage::Fiote {
                    id: "entry".into(),
                    request
                })
                .await
                .first(),
            Some(ServerMessage::Error { .. })
        ));
    }
    assert_eq!(count(&host).await, records);
    assert!(script.observed.lock().unwrap().is_empty());
}

struct TextOnly;

#[async_trait::async_trait]
impl Provider for TextOnly {
    async fn complete(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        panic!("Rejected attachments must not start a provider request");
    }
}

#[tokio::test]
async fn assistant_entry_rejected_attachments_can_retry_after_choosing_a_capable_connection() {
    let (host, script, _root, original, _) = fixture(false).await;
    let text_only = host.as_ref().clone().with_provider(Arc::new(TextOnly));
    let mut request = submission(
        "attachment-retry",
        Some(&original),
        "Please inspect this file",
    );
    request
        .content
        .push(nucleus::message::MessagePart::Attachment {
            name: "note.txt".into(),
            mime_type: "text/plain".into(),
            data: base64::engine::general_purpose::STANDARD.encode(b"Fixture context"),
        });
    let records = count(&host).await;
    assert!(
        text_only
            .assistant_send(request.clone())
            .await
            .unwrap_err()
            .contains("attachments")
    );
    assert_eq!(count(&host).await, records);
    assert!(host.assistant_receipt("attachment-retry").await.is_err());
    assert!(script.observed.lock().unwrap().is_empty());
    let receipt = host.assistant_send(request).await.unwrap();
    wait(&host).await;
    assert!(receipt.message.is_some());
    assert_eq!(
        host.assistant_receipt("attachment-retry")
            .await
            .unwrap()
            .state,
        "completed"
    );
}

#[tokio::test]
async fn assistant_entry_saved_node_launcher_and_login_text_create_no_conversation() {
    let (host, script, root, original, _) = fixture(false).await;
    let native = host.load(&original).unwrap().unwrap();
    let mut legacy = native.clone();
    legacy.agent = Some(fiote::acp::Config {
        require_vault: false,
        command: "node".into(),
        args: vec!["missing-codex-acp.js".into()],
        directory: root.path().into(),
        additional_directories: Vec::new(),
        environment: Default::default(),
        session_meta: Default::default(),
        options: Default::default(),
    });
    save(&host.path(&original).unwrap(), &legacy).unwrap();
    let request = submission(
        "legacy-entry",
        Some(&original),
        "Hello without external launchers",
    );
    let records = count(&host).await;
    assert!(
        host.assistant_send(request.clone())
            .await
            .unwrap_err()
            .contains("Node.js")
    );
    assert_eq!(count(&host).await, records);
    assert!(host.assistant_receipt("legacy-entry").await.is_err());
    save(&host.path(&original).unwrap(), &native).unwrap();
    assert!(
        host.assistant_send(submission(
            "reserved-entry",
            Some(&original),
            "/login do-not-send-credentials"
        ))
        .await
        .is_err()
    );
    assert_eq!(count(&host).await, records);
    assert!(script.observed.lock().unwrap().is_empty());
    let receipt = host.assistant_send(request).await.unwrap();
    wait(&host).await;
    assert!(receipt.message.is_some());
}
