use super::*;
use engine::{roster::cell_key_id, trust::Signer};
use sha2::{Digest, Sha256};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::Notify;

struct InterruptedDeposit {
    hosts: Arc<Hosts>,
    envelope: String,
    pause: bool,
    interrupted: AtomicBool,
    entered: Notify,
}

#[async_trait::async_trait]
impl Network for InterruptedDeposit {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let interrupt = matches!(&request, PublicRequest::DeliverPrivate { document } if document.envelope.id == self.envelope);
        let result = self.hosts.request(destination, request).await?;
        if interrupt && !self.interrupted.swap(true, Ordering::SeqCst) {
            self.entered.notify_one();
            if self.pause {
                std::future::pending::<()>().await;
            }
            return Err(EngineError::Consequence(
                "Simulated loss after durable mailbox deposit".into(),
            ));
        }
        Ok(result)
    }
}

pub(super) async fn person_on_disk(secret: u8) -> (Arc<Engine>, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("root.key"), [secret; 32]).unwrap();
    std::fs::write(
        directory.path().join("operational.key"),
        [secret.wrapping_add(1); 32],
    )
    .unwrap();
    (reopen(directory.path()).await, directory)
}

pub(super) async fn reopen(directory: &std::path::Path) -> Arc<Engine> {
    let engine = Arc::new(
        Engine::open(directory.join("lince.sqlite").to_str().unwrap())
            .await
            .unwrap(),
    );
    engine.set_root_key_path(directory.join("root.key"));
    engine.set_sealing_keyring_path(directory.join("sealing.json"));
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let bytes: [u8; 32] = std::fs::read(directory.join("operational.key"))
        .unwrap()
        .try_into()
        .unwrap();
    let signer = Signer::from_bytes(&organ, &cell_key_id(&cell), bytes);
    engine.set_signer(signer.clone()).await.unwrap();
    engine.set_organ_signer(signer).await.unwrap();
    engine
}

pub(super) async fn protected_session_state(engine: &Engine) -> Vec<(String, [u8; 32])> {
    let key = engine.social_storage_key().await.unwrap();
    let rows: Vec<(String, String, String, i64)> =
        store::sqlx::query_as("SELECT id,kind,body,version FROM social_device_state ORDER BY id")
            .fetch_all(&engine.store.pool)
            .await
            .unwrap();
    rows.into_iter()
        .map(|(id, kind, body, version)| {
            let protected = if kind == "account" {
                let mut account: engine::social::session::AccountState =
                    engine::social::session::open_local(&id, &body, &key).unwrap();
                account.pickup_counter = 0;
                serde_json::to_vec(&account).unwrap()
            } else {
                serde_json::to_vec(&(body, version)).unwrap()
            };
            (id, Sha256::digest(protected).into())
        })
        .collect()
}

async fn interrupted_delivery(pause: bool) {
    let clock = nucleus::execution::Execution::new([213; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let mut fixture = accepted_with_storage(true).await;
            let text = "One message despite deposit loss and interrupted worker";
            let saved = command(
                &fixture.sender,
                Command::SendPrivate {
                    conversation: fixture.conversation.clone(),
                    text: text.into(),
                },
            )
            .await;
            let message = saved["message"].as_str().unwrap().to_owned();
            due(&fixture.sender).await;
            fixture.sender.social_prepare_messages_once().await.unwrap();
            publication(&fixture.sender).await;
            let body: String = store::sqlx::query_scalar(
                "SELECT body FROM social_private_outbox WHERE record_uid=? LIMIT 1",
            )
            .bind(&message)
            .fetch_one(&fixture.sender.store.pool)
            .await
            .unwrap();
            let document: PrivateDelivery = serde_json::from_str(&body).unwrap();
            fixture
                .hosts
                .offline
                .lock()
                .unwrap()
                .insert(fixture.services[1].clone());
            let network = Arc::new(InterruptedDeposit {
                hosts: fixture.hosts.clone(),
                envelope: document.envelope.id.clone(),
                pause,
                interrupted: AtomicBool::new(false),
                entered: Notify::new(),
            });
            fixture.sender.attach_social_network(network.clone());
            let worker = {
                let sender = fixture.sender.clone();
                let clock = clock.clone();
                tokio::spawn(async move {
                    clock
                        .scope(async {
                            for _ in 0..8 {
                                due(&sender).await;
                                sender.social_send_private_once().await.unwrap();
                            }
                        })
                        .await
                })
            };
            tokio::time::timeout(Duration::from_secs(10), network.entered.notified())
                .await
                .unwrap();
            if pause {
                worker.abort();
                assert!(worker.await.unwrap_err().is_cancelled());
            } else {
                worker.await.unwrap();
            }
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM record WHERE kind='message' AND body=?"
                )
                .bind(text)
                .fetch_one(&fixture.receiver.store.pool)
                .await
                .unwrap(),
                0
            );
            let retained: String =
                store::sqlx::query_scalar("SELECT body FROM social_private_outbox WHERE id=?")
                    .bind(&document.envelope.id)
                    .fetch_one(&fixture.sender.store.pool)
                    .await
                    .unwrap();
            assert_eq!(retained, body);
            fixture.sender.store.pool.close().await;
            fixture.receiver.store.pool.close().await;
            fixture.sender = reopen(fixture._directories[1].path()).await;
            fixture.receiver = reopen(fixture._directories[0].path()).await;
            fixture.sender.attach_social_network(fixture.hosts.clone());
            fixture
                .receiver
                .attach_social_network(fixture.hosts.clone());
            exchange(&fixture.sender, &fixture.receiver).await;
            fixture.hosts.offline.lock().unwrap().clear();
            for _ in 0..2 {
                exchange(&fixture.sender, &fixture.receiver).await;
            }
            let rows: Vec<String> =
                store::sqlx::query_scalar("SELECT uid FROM record WHERE kind='message' AND body=?")
                    .bind(text)
                    .fetch_all(&fixture.receiver.store.pool)
                    .await
                    .unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(
                store::records::get(&fixture.receiver.store.pool, &rows[0])
                    .await
                    .unwrap()
                    .unwrap()
                    .quantity,
                store::exact::one()
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_message_event WHERE record_uid=?"
                )
                .bind(&rows[0])
                .fetch_one(&fixture.receiver.store.pool)
                .await
                .unwrap(),
                1
            );
            let retained: String =
                store::sqlx::query_scalar("SELECT body FROM social_private_outbox WHERE id=?")
                    .bind(&document.envelope.id)
                    .fetch_one(&fixture.sender.store.pool)
                    .await
                    .unwrap();
            assert_eq!(retained, body);
            fixture.sender.store.pool.close().await;
            fixture.receiver.store.pool.close().await;
        }))
        .await;
}

#[tokio::test]
async fn lost_deposit_response_single_host_and_client_reopen_import_once() {
    interrupted_delivery(false).await;
}

#[tokio::test]
async fn cancelled_deposit_worker_single_host_and_client_reopen_import_once() {
    interrupted_delivery(true).await;
}
