use super::*;
use serde_json::json;
use std::{path::PathBuf, process::Stdio, time::Duration};

struct InterruptedReceiver {
    document: PrivateDelivery,
    service: String,
    admitted: i64,
    marker: PathBuf,
}

#[async_trait::async_trait]
impl Network for InterruptedReceiver {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        if destination != self.service {
            return Err(EngineError::Consequence("Simulated offline host".into()));
        }
        match request {
            PublicRequest::CollectPrivate { access } => Ok(json!({
                "service":self.service,"mailbox":access.mailbox,
                "envelopes":[{"document":self.document,"accepted_at":self.admitted}],
            })),
            PublicRequest::AcknowledgePrivate { receipts, .. } => {
                std::fs::write(&self.marker, serde_json::to_vec(&receipts)?).unwrap();
                std::future::pending().await
            }
            _ => Err(EngineError::Consequence(
                "Unexpected request in receiver crash fixture".into(),
            )),
        }
    }
}

struct ReceiverProcess(std::process::Child);

impl Drop for ReceiverProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn receiver_process_kill_after_import_before_acknowledgement_reopens_once() {
    let clock = nucleus::execution::Execution::new([214; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            if let Some(directory) = std::env::var_os("LINCE_SOCIAL_CRASH_RECEIVER") {
                let directory = PathBuf::from(directory);
                let fixture: Value = serde_json::from_slice(
                    &std::fs::read(directory.join("crash-fixture.json")).unwrap(),
                )
                .unwrap();
                let receiver = faults::reopen(&directory).await;
                let network = Arc::new(InterruptedReceiver {
                    document: serde_json::from_value(fixture["document"].clone()).unwrap(),
                    service: fixture["service"].as_str().unwrap().into(),
                    admitted: fixture["admitted"].as_i64().unwrap(),
                    marker: directory.join("acknowledgement-reached.json"),
                });
                receiver.attach_social_network(network.clone());
                due(&receiver).await;
                receiver.social_collect_private_once().await.unwrap();
                panic!("The receiver must be interrupted at acknowledgement");
            }
            let mut fixture = accepted_with_storage(true).await;
            let text = "Durable before receiver process interruption";
            let sent = command(
                &fixture.sender,
                Command::SendPrivate {
                    conversation: fixture.conversation.clone(),
                    text: text.into(),
                },
            )
            .await;
            let message = sent["message"].as_str().unwrap();
            due(&fixture.sender).await;
            fixture.sender.social_prepare_messages_once().await.unwrap();
            publication(&fixture.sender).await;
            for _ in 0..8 {
                due(&fixture.sender).await;
                fixture.sender.social_send_private_once().await.unwrap();
            }
            let body: String = store::sqlx::query_scalar(
                "SELECT body FROM social_private_outbox WHERE record_uid=? LIMIT 1",
            )
            .bind(message)
            .fetch_one(&fixture.sender.store.pool)
            .await
            .unwrap();
            let document: PrivateDelivery = serde_json::from_str(&body).unwrap();
            let host = &fixture.hosts.nodes[&fixture.services[0]];
            let admitted: i64 = store::sqlx::query_scalar(
                "SELECT created_at FROM social_service_envelope WHERE id=?",
            )
            .bind(&document.envelope.id)
            .fetch_one(&host.store.pool)
            .await
            .unwrap();
            let directory = fixture._directories[0].path();
            std::fs::write(
                directory.join("crash-fixture.json"),
                serde_json::to_vec(&json!({
                    "document":document,"service":fixture.services[0],"admitted":admitted,
                }))
                .unwrap(),
            )
            .unwrap();
            fixture.receiver.store.pool.close().await;
            let output = directory.join("receiver-process.log");
            let log = std::fs::File::create(&output).unwrap();
            let mut child = ReceiverProcess(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "late_delivery::process_faults::receiver_process_kill_after_import_before_acknowledgement_reopens_once",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("LINCE_SOCIAL_CRASH_RECEIVER", directory)
                    .stdout(Stdio::from(log.try_clone().unwrap()))
                    .stderr(Stdio::from(log))
                    .spawn()
                    .unwrap(),
            );
            let marker = directory.join("acknowledgement-reached.json");
            let ready = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    if marker.exists() {
                        return Ok(());
                    }
                    if let Some(status) = child.0.try_wait().unwrap() {
                        return Err(status);
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await;
            assert!(
                matches!(ready, Ok(Ok(()))),
                "Receiver did not reach its committed acknowledgement: {ready:?}, {}",
                std::fs::read_to_string(&output).unwrap()
            );
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_service_envelope WHERE id=?",
                )
                .bind(&document.envelope.id)
                .fetch_one(&host.store.pool)
                .await
                .unwrap(),
                1
            );
            fixture.receiver = faults::reopen(directory).await;
            fixture.receiver.attach_social_network(fixture.hosts.clone());
            let imported: String = store::sqlx::query_scalar(
                "SELECT uid FROM record WHERE kind='message' AND body=?",
            )
            .bind(text)
            .fetch_one(&fixture.receiver.store.pool)
            .await
            .unwrap();
            let state = faults::protected_session_state(&fixture.receiver).await;
            for _ in 0..2 {
                exchange(&fixture.sender, &fixture.receiver).await;
            }
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM record WHERE kind='message' AND body=?",
                )
                .bind(text)
                .fetch_one(&fixture.receiver.store.pool)
                .await
                .unwrap(),
                1
            );
            assert_eq!(
                store::records::get(&fixture.receiver.store.pool, &imported)
                    .await
                    .unwrap()
                    .unwrap()
                    .quantity,
                store::exact::one()
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_message_event WHERE record_uid=?",
                )
                .bind(&imported)
                .fetch_one(&fixture.receiver.store.pool)
                .await
                .unwrap(),
                1
            );
            assert_eq!(
                faults::protected_session_state(&fixture.receiver).await,
                state
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_service_envelope WHERE id=?",
                )
                .bind(&document.envelope.id)
                .fetch_one(&host.store.pool)
                .await
                .unwrap(),
                0
            );
            fixture.sender.store.pool.close().await;
            fixture.receiver.store.pool.close().await;
        }))
        .await;
}
