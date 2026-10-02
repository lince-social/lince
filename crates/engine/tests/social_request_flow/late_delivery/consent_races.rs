use super::*;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::Notify;

struct DelayedCollection {
    hosts: Arc<Hosts>,
    envelope: String,
    held: AtomicBool,
    entered: Notify,
    release: Notify,
}

#[async_trait::async_trait]
impl Network for DelayedCollection {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let collect = matches!(&request, PublicRequest::CollectPrivate { .. });
        let response = self.hosts.request(destination, request).await?;
        let present = response["envelopes"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["document"]["envelope"]["id"] == self.envelope)
        });
        if collect && present && !self.held.swap(true, Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(response)
    }
}

#[tokio::test]
async fn block_or_close_during_collection_rolls_back_ratchets_history_events_and_receipts() {
    let clock = nucleus::execution::Execution::new([215; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            for decision in [RequestDecision::Block, RequestDecision::Close] {
                let fixture = accepted().await;
                let requests = command(&fixture.receiver, Command::Requests { after: None }).await;
                let conversation = requests["requests"][0]["record"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let text = "Consent changes while collection is in flight";
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
                let network = Arc::new(DelayedCollection {
                    hosts: fixture.hosts.clone(),
                    envelope: document.envelope.id.clone(),
                    held: AtomicBool::new(false),
                    entered: Notify::new(),
                    release: Notify::new(),
                });
                fixture.receiver.attach_social_network(network.clone());
                due(&fixture.receiver).await;
                let (received, (state, events)) =
                    tokio::time::timeout(Duration::from_secs(30), async {
                        tokio::join!(fixture.receiver.social_collect_private_once(), async {
                            network.entered.notified().await;
                            command(
                                &fixture.receiver,
                                Command::DecideRequest {
                                    conversation,
                                    decision,
                                },
                            )
                            .await;
                            let state = faults::protected_session_state(&fixture.receiver).await;
                            let events: i64 = store::sqlx::query_scalar(
                                "SELECT COUNT(*) FROM social_message_event",
                            )
                            .fetch_one(&fixture.receiver.store.pool)
                            .await
                            .unwrap();
                            network.release.notify_one();
                            (state, events)
                        })
                    })
                    .await
                    .unwrap();
                assert_eq!(received.unwrap(), 0, "{decision:?}");
                assert_eq!(
                    faults::protected_session_state(&fixture.receiver).await,
                    state,
                    "{decision:?}"
                );
                assert_eq!(
                    store::sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM social_message_event"
                    )
                    .fetch_one(&fixture.receiver.store.pool)
                    .await
                    .unwrap(),
                    events,
                    "{decision:?}"
                );
                assert_eq!(
                    store::sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM record WHERE kind='message' AND body=?",
                    )
                    .bind(text)
                    .fetch_one(&fixture.receiver.store.pool)
                    .await
                    .unwrap(),
                    0
                );
                assert_eq!(
                    store::sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM social_private_seen WHERE envelope=?",
                    )
                    .bind(&document.envelope.id)
                    .fetch_one(&fixture.receiver.store.pool)
                    .await
                    .unwrap(),
                    0
                );
                for host in fixture.hosts.nodes.values() {
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
                }
            }
        }))
        .await;
}
