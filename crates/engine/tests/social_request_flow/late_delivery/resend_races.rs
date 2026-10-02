use super::*;
use nucleus::social::requests::{DELIVERY_NAMESPACE, PARTICIPANTS_NAMESPACE};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

struct DelayedDeposit {
    hosts: Arc<Hosts>,
    message: String,
    held: AtomicBool,
    entered: Notify,
    release: Notify,
}

#[async_trait::async_trait]
impl Network for DelayedDeposit {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let hold = matches!(&request,PublicRequest::DeliverPrivate { document } if document.envelope.message==self.message)
            && !self.held.swap(true, Ordering::SeqCst);
        let reply = self.hosts.request(destination, request).await?;
        if hold {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(reply)
    }
}

#[tokio::test]
async fn late_stored_response_cannot_overwrite_the_replacement_delivery_attempt() {
    let clock = nucleus::execution::Execution::new([233; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let fixture = accepted().await;
            let saved = command(
                &fixture.sender,
                Command::SendPrivate {
                    conversation: fixture.conversation.clone(),
                    text: "Delayed storage response".into(),
                },
            )
            .await;
            let message = saved["message"].as_str().unwrap();
            due(&fixture.sender).await;
            fixture.sender.social_prepare_messages_once().await.unwrap();
            let old: String = store::sqlx::query_scalar(
                "SELECT body FROM social_private_outbox WHERE record_uid=? LIMIT 1",
            )
            .bind(message)
            .fetch_one(&fixture.sender.store.pool)
            .await
            .unwrap();
            let old: PrivateDelivery = serde_json::from_str(&old).unwrap();
            let network = Arc::new(DelayedDeposit {
                hosts: fixture.hosts.clone(),
                message: old.envelope.message.clone(),
                held: AtomicBool::new(false),
                entered: Notify::new(),
                release: Notify::new(),
            });
            fixture.sender.attach_social_network(network.clone());
            let (sent, ()) = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                tokio::join!(fixture.sender.social_send_private_once(), async {
                    network.entered.notified().await;
                    clock.set_time(old.envelope.expires_at * 1000).unwrap();
                    command(
                        &fixture.sender,
                        Command::ResendExpiredPrivate {
                            message: message.into(),
                        },
                    )
                    .await;
                    network.release.notify_one();
                })
            })
            .await
            .unwrap();
            sent.unwrap();
            let status = store::records::get_extension(
                &fixture.sender.store.pool,
                message,
                DELIVERY_NAMESPACE,
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(status["stage"], "waiting");
            assert_eq!(status["expires_at"], old.envelope.expires_at + 30 * 86400);
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_message_work WHERE record_uid=?"
                )
                .bind(message)
                .fetch_one(&fixture.sender.store.pool)
                .await
                .unwrap(),
                1
            );
            let state: String =
                store::sqlx::query_scalar("SELECT state FROM social_private_outbox WHERE id=?")
                    .bind(&old.envelope.id)
                    .fetch_one(&fixture.sender.store.pool)
                    .await
                    .unwrap();
            assert_eq!(state, "cancelled");
        }))
        .await;
}

#[tokio::test]
async fn resend_rejects_terminal_closed_provisional_intro_deleted_and_unprivileged_cases() {
    Box::pin(async {
        let fixture = accepted().await;
        let saved = command(&fixture.sender,Command::SendPrivate { conversation:fixture.conversation.clone(),text:"Retained expired text".into() }).await;
        let message = saved["message"].as_str().unwrap();
        let mut expired = store::records::get_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE).await.unwrap().unwrap();
        expired["expires_at"] = json!(nucleus::execution::now().timestamp()-1);
        for stage in ["recipient-refused","recipient-durable"] {
            let mut terminal = expired.clone();
            terminal["stage"] = json!(stage);
            store::records::set_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE,&terminal).await.unwrap();
            assert!(fixture.sender.act(Action::Social { request:Command::ResendExpiredPrivate { message:message.into() } },None).await.is_err());
            assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE).await.unwrap().unwrap(),terminal);
        }
        expired["stage"] = json!("expired");
        store::records::set_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE,&expired).await.unwrap();
        let participant = store::records::get_extension(&fixture.sender.store.pool,&fixture.conversation,PARTICIPANTS_NAMESPACE).await.unwrap().unwrap();
        for state in ["closed","blocked","declined","pending"] {
            let mut closed = participant.clone();
            closed["state"] = json!(state);
            store::records::set_extension(&fixture.sender.store.pool,&fixture.conversation,PARTICIPANTS_NAMESPACE,&closed).await.unwrap();
            assert!(fixture.sender.act(Action::Social { request:Command::ResendExpiredPrivate { message:message.into() } },None).await.is_err());
        }
        store::records::set_extension(&fixture.sender.store.pool,&fixture.conversation,PARTICIPANTS_NAMESPACE,&participant).await.unwrap();
        let context = participant["context"].as_str().unwrap();
        let previous_blocks = store::records::get_extension(&fixture.sender.store.pool,context,nucleus::social::requests::BLOCK_NAMESPACE).await.unwrap().unwrap_or_else(||json!({}));
        let mut blocked = previous_blocks.clone();
        blocked[participant["peer_owner"].as_str().unwrap()] = json!({"blocked":true,"window":nucleus::execution::now().timestamp()});
        store::records::set_extension(&fixture.sender.store.pool,context,nucleus::social::requests::BLOCK_NAMESPACE,&blocked).await.unwrap();
        assert_eq!(command(&fixture.sender,Command::PrivateDeliveryStatus { message:message.into() }).await["private_delivery"]["can_resend"],false);
        assert!(fixture.sender.act(Action::Social { request:Command::ResendExpiredPrivate { message:message.into() } },None).await.is_err());
        store::records::set_extension(&fixture.sender.store.pool,context,nucleus::social::requests::BLOCK_NAMESPACE,&previous_blocks).await.unwrap();
        store::sqlx::query("INSERT INTO social_private_outbox(id,context,body,hash,expires_at,state,record_uid) VALUES('terminal-fixture',?,'{}','terminal',?,'ready',?)")
            .bind(context).bind(nucleus::execution::now().timestamp()-1).bind(message).execute(&fixture.sender.store.pool).await.unwrap();
        assert_eq!(command(&fixture.sender,Command::PrivateDeliveryStatus { message:message.into() }).await["private_delivery"]["can_resend"],false);
        assert!(fixture.sender.act(Action::Social { request:Command::ResendExpiredPrivate { message:message.into() } },None).await.is_err());
        store::sqlx::query("DELETE FROM social_private_outbox WHERE id='terminal-fixture'").execute(&fixture.sender.store.pool).await.unwrap();
        let introduction: String = store::sqlx::query_scalar("SELECT record_uid FROM record_extension WHERE namespace='lince.social.message' AND json_extract(fds,'$.content.kind.kind')='introduction' LIMIT 1")
            .fetch_one(&fixture.sender.store.pool).await.unwrap();
        store::records::set_extension(&fixture.sender.store.pool,&introduction,DELIVERY_NAMESPACE,&expired).await.unwrap();
        assert!(fixture.sender.act(Action::Social { request:Command::ResendExpiredPrivate { message:introduction } },None).await.is_err());
        let control: String = store::sqlx::query_scalar("SELECT record_uid FROM record_extension WHERE namespace='lince.social.message' AND json_extract(fds,'$.content.kind.kind')='accept' LIMIT 1")
            .fetch_one(&fixture.receiver.store.pool).await.unwrap();
        store::records::set_extension(&fixture.receiver.store.pool,&control,DELIVERY_NAMESPACE,&expired).await.unwrap();
        assert!(fixture.receiver.act(Action::Social { request:Command::ResendExpiredPrivate { message:control } },None).await.is_err());
        let role = store::auth::ensure_role(&fixture.sender.store.pool,"delivery-viewer").await.unwrap();
        let actor = store::auth::create_person_login(&fixture.sender.store.pool,"Viewer","viewer","hash",role).await.unwrap();
        assert!(fixture.sender.act(Action::Social { request:Command::ResendExpiredPrivate { message:message.into() } },Some(actor)).await.is_err());
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE).await.unwrap().unwrap(),expired);
        store::records::mark_deleted(&fixture.sender.store.pool,&fixture.conversation).await.unwrap();
        assert!(fixture.sender.act(Action::Social { request:Command::ResendExpiredPrivate { message:message.into() } },None).await.is_err());
    }).await;
}
