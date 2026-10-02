use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use nucleus::{MessageState, message::MessagePart, social::requests::*};

async fn thread(engine: &Engine, root: &str) -> String {
    store::sqlx::query_scalar(
        "SELECT uid FROM record WHERE replica_root=? AND kind='thread' AND deleted_at IS NULL",
    )
    .bind(root)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap()
}

fn message(thread: String, content: Vec<MessagePart>) -> Action {
    Action::CreateMessage {
        thread,
        body: "Inspect the attached files".into(),
        author: None,
        state: MessageState::Finished,
        parent: None,
        references: Vec::new(),
        content,
    }
}

fn attachment(name: &str, mime: &str, bytes: &[u8]) -> MessagePart {
    MessagePart::Attachment {
        name: name.into(),
        mime_type: mime.into(),
        data: B64.encode(bytes),
    }
}

async fn received(engine: &Engine, body: &str) -> String {
    store::sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='message' AND body=? AND deleted_at IS NULL",
    )
    .bind(body)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap()
}

async fn delivery_errors(engine: &Engine) -> Vec<(String, Vec<Option<String>>)> {
    let mut result = Vec::new();
    for table in [
        "social_message_work",
        "social_private_outbox",
        "social_private_destination",
        "social_pickup_work",
    ] {
        let errors = store::sqlx::query_scalar(&format!("SELECT error FROM {table} LIMIT 8"))
            .fetch_all(&engine.store.pool)
            .await
            .unwrap();
        result.push((table.into(), errors));
    }
    result
}

#[tokio::test]
async fn normal_messages_deliver_six_formats_at_the_shared_size_limit_with_compact_metadata() {
    let clock = nucleus::execution::Execution::new([208; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let fixture = accepted().await;
        for host in fixture.hosts.nodes.values() {
            command(host, Command::ConfigureServices { settings: ServiceSettings {
                directory: true, mailbox: true,
                incoming_bytes_per_minute: 128 * 1024 * 1024,
                outgoing_bytes_per_minute: 128 * 1024 * 1024,
                ..Default::default()
            }}).await;
        }
        let mut parts = vec![
            attachment("note.txt", "text/plain", b"Marker: Fiote attachment 724"),
            attachment("table.csv", "text/csv", b"item,value\na,2\nb,3\n"),
            attachment("document.pdf", "application/pdf", b"%PDF-1.4\ntransport fixture\n%%EOF"),
            attachment("photo.png", "image/png", b"\x89PNG\r\n\x1a\ntransport fixture"),
            attachment("video.mp4", "video/mp4", b"\0\0\0\x18ftypmp42transport fixture"),
        ];
        let used: usize = parts.iter().map(|part| match part {
            MessagePart::Attachment { data, .. } => nucleus::message::decode(data).unwrap().len(),
            _ => unreachable!(),
        }).sum();
        parts.push(attachment("audio.wav", "audio/wav", &vec![42; nucleus::message::MAX_CONTENT_BYTES - used]));
        let saved = fixture.sender.act(message(thread(&fixture.sender, &fixture.conversation).await, parts.clone()), None).await.unwrap().created.unwrap();
        assert_eq!(store::message_content::load(&fixture.sender.store.pool, &saved).await.unwrap(), parts);
        let metadata = store::records::get_extension(&fixture.sender.store.pool, &saved, MESSAGE_NAMESPACE).await.unwrap().unwrap();
        assert!(serde_json::to_vec(&metadata).unwrap().len() < 2048);
        for _ in 0..3 { exchange(&fixture.sender, &fixture.receiver).await; }
        let present: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE kind='message' AND body='Inspect the attached files' AND deleted_at IS NULL)")
            .fetch_one(&fixture.receiver.store.pool).await.unwrap();
        assert!(present, "sender={:?}, receiver={:?}", delivery_errors(&fixture.sender).await, delivery_errors(&fixture.receiver).await);
        let target = received(&fixture.receiver, "Inspect the attached files").await;
        assert_eq!(store::message_content::load(&fixture.receiver.store.pool, &target).await.unwrap(), parts);
        let manifest = store::records::get_extension(&fixture.receiver.store.pool, &target, "lince.message-content").await.unwrap().unwrap();
        assert_eq!(manifest["parts"][5]["chunks"], 22);
        for _ in 0..2 { exchange(&fixture.sender, &fixture.receiver).await; }
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='message' AND body='Inspect the attached files'")
            .fetch_one(&fixture.receiver.store.pool).await.unwrap();
        assert_eq!(count, 1);
        let status = command(&fixture.sender, Command::PrivateDeliveryStatus { message: saved }).await;
        assert_eq!(status["private_delivery"]["delivery"]["stage"], "recipient-durable");
    })).await;
}

#[tokio::test]
async fn private_files_reject_unconsented_controls_oversize_and_large_provisional_messages() {
    let clock = nucleus::execution::Execution::new([209; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let fixture = accepted().await;
            let thread = thread(&fixture.sender, &fixture.conversation).await;
            let before: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='message'")
                    .fetch_one(&fixture.sender.store.pool)
                    .await
                    .unwrap();
            for content in [
                vec![MessagePart::Reference {
                    name: "Private Record".into(),
                    uri: format!("record:{}", fixture.conversation),
                }],
                vec![MessagePart::Steps {
                    steps: vec![nucleus::operation::Step {
                        content: "Review".into(),
                        priority: nucleus::operation::Priority::Medium,
                        status: nucleus::operation::StepState::Pending,
                    }],
                }],
                vec![attachment(
                    "oversized.bin",
                    "application/octet-stream",
                    &vec![0; nucleus::message::MAX_CONTENT_BYTES + 1],
                )],
            ] {
                assert!(
                    fixture
                        .sender
                        .act(message(thread.clone(), content), None)
                        .await
                        .is_err()
                );
            }
            let mut participant = store::records::get_extension(
                &fixture.sender.store.pool,
                &fixture.conversation,
                PARTICIPANTS_NAMESPACE,
            )
            .await
            .unwrap()
            .unwrap();
            participant["state"] = serde_json::json!("pending");
            store::records::set_extension(
                &fixture.sender.store.pool,
                &fixture.conversation,
                PARTICIPANTS_NAMESPACE,
                &participant,
            )
            .await
            .unwrap();
            let error = fixture
                .sender
                .act(
                    message(
                        thread,
                        vec![attachment(
                            "large.bin",
                            "application/octet-stream",
                            &vec![0; 32 * 1024],
                        )],
                    ),
                    None,
                )
                .await
                .unwrap_err();
            assert!(error.to_string().contains("Accept this conversation"));
            let after: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='message'")
                    .fetch_one(&fixture.sender.store.pool)
                    .await
                    .unwrap();
            assert_eq!(before, after);
        }))
        .await;
}

#[tokio::test]
async fn expired_attachment_resends_the_same_message_and_rejects_damaged_retained_chunks() {
    let clock = nucleus::execution::Execution::new([210; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let fixture = accepted().await;
            let parts = vec![attachment(
                "note.txt",
                "text/plain",
                b"Only this file contains marker 381",
            )];
            let saved = fixture
                .sender
                .act(
                    message(
                        thread(&fixture.sender, &fixture.conversation).await,
                        parts.clone(),
                    ),
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap();
            let status = command(
                &fixture.sender,
                Command::PrivateDeliveryStatus {
                    message: saved.clone(),
                },
            )
            .await;
            let expiry = status["private_delivery"]["delivery"]["expires_at"]
                .as_i64()
                .unwrap();
            clock.set_time(expiry * 1000).unwrap();
            let status = command(
                &fixture.sender,
                Command::PrivateDeliveryStatus {
                    message: saved.clone(),
                },
            )
            .await;
            assert_eq!(status["private_delivery"]["can_resend"], true);
            let resent = command(
                &fixture.sender,
                Command::ResendExpiredPrivate {
                    message: saved.clone(),
                },
            )
            .await;
            assert_eq!(resent["message"], saved);
            assert_eq!(
                store::message_content::load(&fixture.sender.store.pool, &saved)
                    .await
                    .unwrap(),
                parts
            );
            clock
                .set_time(resent["expires_at"].as_i64().unwrap() * 1000)
                .unwrap();
            store::records::set_extension(
                &fixture.sender.store.pool,
                &saved,
                &nucleus::message::chunk_namespace(0, 0),
                &serde_json::json!({"data":""}),
            )
            .await
            .unwrap();
            let status = command(
                &fixture.sender,
                Command::PrivateDeliveryStatus {
                    message: saved.clone(),
                },
            )
            .await;
            assert_eq!(status["private_delivery"]["can_resend"], false);
            assert!(
                fixture
                    .sender
                    .act(
                        Action::Social {
                            request: Command::ResendExpiredPrivate { message: saved }
                        },
                        None
                    )
                    .await
                    .is_err()
            );
        }))
        .await;
}
