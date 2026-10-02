use super::*;
use engine::{roster::ROOT_KEY_ID, trust::Signer};
use nucleus::social::requests::PARTICIPANTS_NAMESPACE;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

struct DelayedLookup {
    hosts: Arc<Hosts>,
    deposit: bool,
    held: AtomicBool,
    entered: Notify,
    release: Notify,
}

#[async_trait::async_trait]
impl Network for DelayedLookup {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let hold = (matches!(&request, PublicRequest::LookupReplyRoutes { .. }) && !self.deposit
            || matches!(&request, PublicRequest::DeliverPrivate { .. }) && self.deposit)
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
async fn in_flight_route_lookup_cannot_commit_after_local_write_authority_changes() {
    let clock = nucleus::execution::Execution::new([216; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            for change in ["read-only", "removed", "replaced-operational-key"] {
                let fixture = accepted().await;
                let (_second, _directory) = enrol(&fixture.sender, 238).await;
                publication(&fixture.sender).await;
                let organ = store::organs::local(&fixture.sender.store.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .uid;
                let cell = store::cells::local(&fixture.sender.store.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .uid;
                let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [238; 32]);
                let before = store::records::get_extension(
                    &fixture.sender.store.pool,
                    &fixture.conversation,
                    PARTICIPANTS_NAMESPACE,
                )
                .await
                .unwrap()
                .unwrap();
                let history: i64 =
                    store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='message'")
                        .fetch_one(&fixture.sender.store.pool)
                        .await
                        .unwrap();
                command(&fixture.receiver, Command::ResetPrivateSessions).await;
                clock
                    .set_time(clock.now().timestamp_millis() + 2000)
                    .unwrap();
                command(
                    &fixture.receiver,
                    Command::PrepareReplyKeys {
                        record: fixture.context.clone(),
                        services: fixture.services.clone(),
                    },
                )
                .await;
                publication(&fixture.receiver).await;
                store::sqlx::query("UPDATE social_peer_work SET next_attempt=0")
                    .execute(&fixture.sender.store.pool)
                    .await
                    .unwrap();
                let network = Arc::new(DelayedLookup {
                    hosts: fixture.hosts.clone(),
                    deposit: false,
                    held: AtomicBool::new(false),
                    entered: Notify::new(),
                    release: Notify::new(),
                });
                fixture.sender.attach_social_network(network.clone());
                let (result, ()) =
                    tokio::time::timeout(std::time::Duration::from_secs(30), async {
                        tokio::join!(fixture.sender.social_refresh_private_routes_once(), async {
                            network.entered.notified().await;
                            let mut members = fixture
                                .sender
                                .roster_of(&organ)
                                .await
                                .unwrap()
                                .unwrap()
                                .roster
                                .cells;
                            match change {
                                "removed" => members.retain(|entry| entry.cell_uid != cell),
                                "read-only" => members
                                    .iter_mut()
                                    .find(|entry| entry.cell_uid == cell)
                                    .unwrap()
                                    .capabilities
                                    .clear(),
                                _ => {
                                    members
                                        .iter_mut()
                                        .find(|entry| entry.cell_uid == cell)
                                        .unwrap()
                                        .operational_key =
                                        Signer::from_bytes(&organ, "replacement", [217; 32])
                                            .public_key_b64()
                                }
                            }
                            fixture.sender.publish_roster(&root, members).await.unwrap();
                            network.release.notify_one();
                        })
                    })
                    .await
                    .unwrap();
                assert!(
                    result.is_err(),
                    "{change}: stale in-flight response committed: {result:?}"
                );
                assert_eq!(
                    store::records::get_extension(
                        &fixture.sender.store.pool,
                        &fixture.conversation,
                        PARTICIPANTS_NAMESPACE
                    )
                    .await
                    .unwrap()
                    .unwrap(),
                    before,
                    "{change}"
                );
                assert_eq!(
                    store::sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM record WHERE kind='message'"
                    )
                    .fetch_one(&fixture.sender.store.pool)
                    .await
                    .unwrap(),
                    history
                );
                assert!(
                    fixture
                        .sender
                        .social_refresh_private_routes_once()
                        .await
                        .is_err(),
                    "{change}: a later lookup started with obsolete authority"
                );
            }
        }))
        .await;
}

#[tokio::test]
async fn in_flight_deposit_preserves_evidence_but_holds_work_after_write_permission_is_removed() {
    let clock = nucleus::execution::Execution::new([218; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let fixture = accepted().await;
        let (_second, _directory) = enrol(&fixture.sender,238).await;
        publication(&fixture.sender).await;
        let organ = store::organs::local(&fixture.sender.store.pool).await.unwrap().unwrap().uid;
        let cell = store::cells::local(&fixture.sender.store.pool).await.unwrap().unwrap().uid;
        let root = Signer::from_bytes(&organ,ROOT_KEY_ID,[238;32]);
        let sent = command(&fixture.sender,Command::SendPrivate {conversation:fixture.conversation.clone(),text:"Permission changes during mailbox deposit".into()}).await;
        let message = sent["message"].as_str().unwrap();
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let id:String = store::sqlx::query_scalar("SELECT id FROM social_private_outbox WHERE record_uid=? ORDER BY rowid DESC LIMIT 1").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap();
        store::sqlx::query("UPDATE social_private_destination SET next_attempt=? WHERE envelope<>?")
            .bind(nucleus::execution::now().timestamp()+3600).bind(&id).execute(&fixture.sender.store.pool).await.unwrap();
        let before = store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::DELIVERY_NAMESPACE).await.unwrap().unwrap();
        let metadata = store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::MESSAGE_NAMESPACE).await.unwrap().unwrap();
        let network = Arc::new(DelayedLookup {hosts:fixture.hosts.clone(),deposit:true,held:AtomicBool::new(false),entered:Notify::new(),release:Notify::new()});
        fixture.sender.attach_social_network(network.clone());
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(30),async {
            tokio::join!(fixture.sender.social_send_private_once(),async {
                network.entered.notified().await;
                let mut members = fixture.sender.roster_of(&organ).await.unwrap().unwrap().roster.cells;
                members.iter_mut().find(|entry|entry.cell_uid==cell).unwrap().capabilities.clear();
                fixture.sender.publish_roster(&root,members).await.unwrap();
                network.release.notify_one();
            })
        }).await.unwrap();
        assert_eq!(result.unwrap(),0);
        assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT state FROM social_private_outbox WHERE id=?").bind(&id).fetch_one(&fixture.sender.store.pool).await.unwrap(),"held");
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_private_destination WHERE envelope=? AND state<>'cancelled'").bind(&id).fetch_one(&fixture.sender.store.pool).await.unwrap(),0);
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_private_destination WHERE envelope=? AND json_extract(receipt,'$.stage')='stored'").bind(&id).fetch_one(&fixture.sender.store.pool).await.unwrap(),1);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::DELIVERY_NAMESPACE).await.unwrap().unwrap(),before);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::MESSAGE_NAMESPACE).await.unwrap().unwrap(),metadata);
        let mut stored = 0;
        for host in fixture.hosts.nodes.values() {
            stored+=store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_service_envelope WHERE id=?").bind(&id).fetch_one(&host.store.pool).await.unwrap();
        }
        assert_eq!(stored,1);
        assert!(fixture.sender.social_send_private_once().await.is_err());
    })).await;
}
