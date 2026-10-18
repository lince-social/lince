use super::*;
use base64::Engine as _;
use fiote::assistant::{Delegate, Source};

struct RoutingProvider {
    target: String,
}

#[async_trait::async_trait]
impl Provider for RoutingProvider {
    fn validate_content(&self, content: &[nucleus::message::MessagePart]) -> Result<(), String> {
        nucleus::message::validate(content)
    }

    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        assert!(system.contains("lince_delegate"));
        let (name, arguments) = if system.contains("Delegated conversation.") {
            if let Some(Message::Tool { name, result, .. }) = messages.last() {
                assert_eq!(name, "create_file");
                assert_eq!(result["ok"], true);
                return Ok(Reply {
                    text: "The child created handoff.txt containing world.".into(),
                    ..Default::default()
                });
            }
            assert!(tools.iter().any(|tool| tool.name == "create_file"));
            (
                "create_file",
                json!({"path":"handoff.txt","content":"world"}),
            )
        } else {
            match messages.last() {
                Some(Message::Tool { name, result, .. }) if name == "lince_fiotes" => {
                    assert_eq!(result["ok"], true);
                    assert!(
                        result["result"]["fiotes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|candidate| candidate["record"] == self.target)
                    );
                    (
                        "lince_delegate",
                        json!({"target":self.target,"request_id":"routing-proof"}),
                    )
                }
                Some(Message::Tool { name, result, .. }) if name == "lince_delegate" => {
                    assert_eq!(result["ok"], true, "{result}");
                    assert!(result["result"]["thread"].as_str().is_some());
                    return Ok(Reply {
                        text: "Handed off to the child; inspect its conversation for the result."
                            .into(),
                        ..Default::default()
                    });
                }
                _ => ("lince_fiotes", json!({"limit":30})),
            }
        };
        Ok(Reply {
            calls: vec![ToolCall {
                id: format!(
                    "call-{}",
                    messages
                        .iter()
                        .filter(|message| matches!(message, Message::Tool { .. }))
                        .count()
                        + 1
                ),
                name: name.into(),
                arguments,
                signatures: None,
            }],
            ..Default::default()
        })
    }
}

async fn child(host: &Host, parent: &str) -> String {
    let uid = host
        .engine
        .act(
            Action::CreateAgent {
                head: "Coding child".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.handle(Request::Behavior {
        record: uid.clone(),
        prompt_parent: Some(parent.into()),
        run_assigned: false,
    })
    .await
    .unwrap();
    let mut settings = host.load(parent).unwrap().unwrap().settings;
    settings.directory = PathBuf::new();
    host.handle(Request::Configure {
        record: uid.clone(),
        settings,
        api_key: Some(Secret("test-child-key".into())),
        password: None,
    })
    .await
    .unwrap();
    uid
}

#[tokio::test]
async fn handoff_actual_native_tools_launch_child_preserve_attribution_attachments_and_receipts() {
    let (mut host, _, root, record, thread) = fixture(false).await;
    let target = child(&host, &record).await;
    Arc::get_mut(&mut host).unwrap().provider = Some(Arc::new(RoutingProvider {
        target: target.clone(),
    }));
    let content = vec![
        nucleus::message::MessagePart::Text {
            text: "Attached task context".into(),
        },
        nucleus::message::MessagePart::Attachment {
            name: "task.csv".into(),
            mime_type: "text/csv".into(),
            data: base64::engine::general_purpose::STANDARD.encode(b"name,value\nexample,1\n"),
        },
        nucleus::message::MessagePart::Attachment {
            name: "context.png".into(),
            mime_type: "image/png".into(),
            data: base64::engine::general_purpose::STANDARD.encode(include_bytes!(
                "../../../../interface/tests/fixtures/fiote/photo.png"
            )),
        },
    ];
    host.send_content(
        &thread,
        "Please create the fixture file through the coding child",
        &content,
    )
    .await
    .unwrap();
    wait(&host).await;
    let receipts = host.handoff_receipts(Some(&thread), None).await.unwrap();
    assert_eq!(receipts.len(), 1);
    let receipt = &receipts[0];
    assert_eq!(receipt.state, "completed", "{receipt:?}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("handoff.txt")).unwrap(),
        "world"
    );
    assert_eq!(receipt.target, target);
    assert_eq!(receipt.chain, vec![record.clone(), target.clone()]);
    let message = receipt.message.as_ref().unwrap();
    let metadata = store::records::get_extension(&host.engine.store.pool, message, "lince.message")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata["author"], record);
    let delivered = store::message_content::load(&host.engine.store.pool, message)
        .await
        .unwrap();
    assert!(content.iter().all(|part| delivered.contains(part)));
    assert!(delivered.iter().any(|part| matches!(part,
        nucleus::message::MessagePart::Reference { uri, .. } if uri == &format!("record:{}", receipt.source.message))));
    let pinned = host
        .instruction_snapshot(receipt.thread.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert!(
        pinned["system"]
            .as_str()
            .unwrap()
            .contains("Delegation chain and source references")
    );
    assert!(
        host.inspect_context(&thread).await.unwrap().unwrap()["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|child| child["thread"] == *receipt.thread.as_ref().unwrap())
    );
    let source = receipt.source.clone();
    let request = Delegate {
        request_id: "routing-proof".into(),
        target: target.clone(),
        request: String::new(),
    };
    let repeated = host
        .delegate_request(source.clone(), request.clone())
        .await
        .unwrap();
    assert_eq!(repeated.thread, receipt.thread);
    assert!(
        host.delegate_request(
            source.clone(),
            Delegate {
                request: "Different operation".into(),
                ..request.clone()
            }
        )
        .await
        .is_err()
    );
    let reopened = Host::open(host.engine.clone(), host.directory.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened
            .delegate_request(source, request)
            .await
            .unwrap()
            .thread,
        receipt.thread
    );
    assert_eq!(
        reopened
            .handoff_receipts(Some(&thread), None)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn handoff_refuses_cycles_forged_sources_disabled_targets_and_remote_tools() {
    let (host, _, _root, record, thread) = fixture(false).await;
    let target = child(&host, &record).await;
    let outcome = host
        .send(&thread, "Original request")
        .await
        .unwrap()
        .unwrap();
    wait(&host).await;
    let source = Source {
        fiote: record.clone(),
        author: record.clone(),
        thread: thread.clone(),
        message: outcome.created.unwrap(),
    };
    let request = Delegate {
        request_id: "guard-proof".into(),
        target: target.clone(),
        request: String::new(),
    };
    assert!(
        host.delegate_request(
            source.clone(),
            Delegate {
                target: record.clone(),
                ..request.clone()
            }
        )
        .await
        .is_err()
    );
    assert!(
        host.delegate_request(
            Source {
                author: target.clone(),
                ..source.clone()
            },
            request.clone()
        )
        .await
        .is_err()
    );
    let other = host
        .engine
        .act(
            Action::CreateThread {
                target: record.clone(),
                head: "Foreign source".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        host.delegate_request(
            Source {
                thread: other,
                ..source.clone()
            },
            request.clone()
        )
        .await
        .is_err()
    );
    let receipt = host
        .delegate_request(source.clone(), request.clone())
        .await
        .unwrap();
    assert!(receipt.message.is_some(), "{receipt:?}");
    wait(&host).await;
    let child_thread = receipt.thread.clone();
    let child_source = Source {
        fiote: target.clone(),
        author: target.clone(),
        thread: receipt.thread.unwrap(),
        message: receipt.message.unwrap(),
    };
    assert!(
        host.delegate_request(
            child_source,
            Delegate {
                request_id: "cycle".into(),
                target: record.clone(),
                request: String::new()
            }
        )
        .await
        .unwrap_err()
        .contains("cycles")
    );
    let mut settings = host.load(&target).unwrap().unwrap().settings;
    settings.enabled = false;
    host.handle(Request::Configure {
        record: target.clone(),
        settings,
        api_key: None,
        password: None,
    })
    .await
    .unwrap();
    assert_eq!(
        host.delegate_request(source.clone(), request.clone())
            .await
            .unwrap()
            .thread,
        child_thread
    );
    assert!(
        host.delegate_request(
            source.clone(),
            Delegate {
                request_id: "disabled".into(),
                ..request.clone()
            }
        )
        .await
        .is_err()
    );
    let native = Session::new(
        host.engine.clone(),
        Arc::new(LaneHub::new()),
        "remote-handoff",
        None,
    )
    .into_native_tools(transport::native::Context {
        agent: record.clone(),
        record,
        thread,
    })
    .with_fiote_service(host.clone());
    native.set_source_message(Some(&source.message));
    let mut tools = Registry::default();
    native.register(&mut tools);
    let result = tools
        .run("lince_delegate", serde_json::to_value(request).unwrap())
        .await;
    assert_eq!(result["ok"], false);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("local authorized")
    );
}

#[tokio::test]
async fn handoff_four_hop_limit_does_not_create_a_child_or_receipt() {
    let (host, _, _root, record, thread) = fixture(false).await;
    let target = child(&host, &record).await;
    let request = host
        .send(&thread, "A request at the delegation limit")
        .await
        .unwrap()
        .unwrap();
    wait(&host).await;
    let mut chain = (0..4).map(|_| nucleus::new_uid("r")).collect::<Vec<_>>();
    chain.push(record.clone());
    host.engine
        .act(
            Action::SetExtension {
                target: thread.clone(),
                namespace: "lince.fiote-handoff".into(),
                fds: json!({"chain":chain}),
            },
            None,
        )
        .await
        .unwrap();
    let result = host
        .delegate_request(
            Source {
                fiote: record.clone(),
                author: record,
                thread: thread.clone(),
                message: request.created.unwrap(),
            },
            Delegate {
                request_id: "hop-limit".into(),
                target,
                request: String::new(),
            },
        )
        .await
        .unwrap_err();
    assert!(result.contains("four delegation hops"));
    assert!(
        host.handoff_receipts(Some(&thread), None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        host.inspect_context(&thread).await.unwrap().unwrap()["children"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn handoff_busy_limit_preserves_request_and_stop_marks_a_child_interrupted() {
    let (host, _, _root, record, thread) = fixture(true).await;
    let target = child(&host, &record).await;
    let first = host
        .send(&thread, "Original pending request")
        .await
        .unwrap()
        .unwrap();
    let source = Source {
        fiote: record.clone(),
        author: record.clone(),
        thread: thread.clone(),
        message: first.created.unwrap(),
    };
    let mut threads = vec![thread.clone()];
    for index in 0..7 {
        let thread = host
            .engine
            .act(
                Action::CreateThread {
                    target: record.clone(),
                    head: format!("Busy fixture {index}"),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        host.send(&thread, "Wait until stopped").await.unwrap();
        threads.push(thread);
    }
    assert_eq!(host.running.lock().await.len(), 8);
    let request = Delegate {
        request_id: "busy-handoff".into(),
        target,
        request: String::new(),
    };
    assert!(
        host.delegate_request(source.clone(), request.clone())
            .await
            .unwrap_err()
            .contains("Eight Fiote conversations")
    );
    assert!(
        host.handoff_receipts(Some(&thread), None)
            .await
            .unwrap()
            .is_empty()
    );
    host.handle(Request::Stop {
        thread: threads.last().unwrap().clone(),
    })
    .await
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while host.running.lock().await.len() == 8 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let receipt = host
        .delegate_request(source.clone(), request.clone())
        .await
        .unwrap();
    assert_eq!(receipt.state, "running");
    let child = receipt.thread.as_ref().unwrap();
    let status = host
        .handle(Request::StopChild {
            record: record.clone(),
            thread: child.clone(),
        })
        .await
        .unwrap();
    assert_eq!(status.record, record);
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while host.running.lock().await.contains_key(child) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let stopped = host.delegate_request(source, request).await.unwrap();
    assert_eq!(stopped.thread, receipt.thread);
    assert_eq!(stopped.state, "interrupted");
    assert!(host.running.lock().await.contains_key(&thread));
    let context = host.inspect_context(&thread).await.unwrap().unwrap();
    assert!(
        context["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["thread"] == *child && entry["state"] == "interrupted")
    );
    host.stop_all().await;
    wait(&host).await;
    assert!(host.running.lock().await.is_empty());
}
