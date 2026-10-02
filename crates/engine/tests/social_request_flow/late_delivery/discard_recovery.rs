use super::*;

#[tokio::test]
async fn old_mailbox_discard_intent_survives_new_keys_and_reopen_without_a_false_refusal() {
    let clock = nucleus::execution::Execution::new([231; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let fixture = accepted().await;
        let saved = command(&fixture.sender, Command::SendPrivate {
            conversation:fixture.conversation.clone(),text:"Unreadable old mailbox copy".into(),
        }).await;
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        publication(&fixture.sender).await;
        for _ in 0..4 { fixture.sender.social_send_private_once().await.unwrap(); }
        let body: String = store::sqlx::query_scalar("SELECT body FROM social_private_outbox WHERE record_uid=? LIMIT 1").bind(saved["message"].as_str().unwrap()).fetch_one(&fixture.sender.store.pool).await.unwrap();
        let document: PrivateDelivery = serde_json::from_str(&body).unwrap();
        assert_eq!(document.envelope.message_type, 1);
        store::sqlx::query("DELETE FROM social_device_state WHERE kind='session'").execute(&fixture.receiver.store.pool).await.unwrap();
        due(&fixture.receiver).await;
        assert_eq!(fixture.receiver.social_collect_private_once().await.unwrap(), 0);
        let review = command(&fixture.receiver, Command::Requests { after:None }).await;
        let failures = review["receive_failures"].as_array().unwrap();
        assert!(!failures.is_empty());
        for failure in failures {
            assert_eq!(failure["envelope"], document.envelope.id);
            command(&fixture.receiver, Command::DiscardPrivate {
                context:fixture.context.clone(), service:failure["service"].as_str().unwrap().into(), envelope:document.envelope.id.clone(),
            }).await;
        }
        let retained: Vec<(String,String)> = store::sqlx::query_as("SELECT uid,body FROM record WHERE kind='message' ORDER BY uid").fetch_all(&fixture.receiver.store.pool).await.unwrap();
        command(&fixture.receiver, Command::ResetPrivateSessions).await;
        clock.set_time(clock.now().timestamp_millis()+2000).unwrap();
        let ready = command(&fixture.receiver, Command::PrepareReplyKeys { record:fixture.context.clone(),services:fixture.services.clone() }).await;
        assert_ne!(ready["route"]["route"]["mailbox"], document.envelope.route);
        due(&fixture.receiver).await;
        fixture.receiver.social_collect_private_once().await.unwrap();
        let before = command(&fixture.receiver, Command::Requests { after:None }).await;
        assert_eq!(before["receive_failures"].as_array().unwrap().len(),failures.len());
        assert!(before["receive_failures"].as_array().unwrap().iter().all(|failure| failure["discard"]==true && failure["error"].as_str().unwrap().contains("original device's mailbox authority")));
        let path = fixture._directories[0].path().join("discard-reopen.db");
        fixture.receiver.store.snapshot_into(&path).await.unwrap();
        let reopened = Engine::open(&format!("sqlite://{}?mode=rwc",path.display())).await.unwrap();
        reopened.set_root_key_path(fixture._directories[0].path().join("root.key"));
        reopened.set_sealing_keyring_path(fixture._directories[0].path().join("sealing.json"));
        let organ = store::organs::local(&reopened.store.pool).await.unwrap().unwrap().uid;
        let signer = fixture.receiver.operational_key_for(&organ).await.unwrap();
        reopened.set_signer(signer.clone()).await.unwrap();
        reopened.set_organ_signer(signer).await.unwrap();
        let network: Arc<dyn Network> = fixture.hosts.clone();
        reopened.attach_social_network(network);
        store::sqlx::query("UPDATE social_receive_failure SET next_attempt=0").execute(&reopened.store.pool).await.unwrap();
        due(&reopened).await;
        reopened.social_collect_private_once().await.unwrap();
        let after = command(&reopened, Command::Requests { after:None }).await;
        assert!(after["receive_failures"].as_array().unwrap().iter().all(|failure| failure["discard"]==true && failure["error"].as_str().unwrap().contains("original device's mailbox authority")));
        assert_eq!(after["receive_failures"].as_array().unwrap().len(),failures.len());
        for host in fixture.hosts.nodes.values() {
            assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_service_completed WHERE id=? AND stage='recipient-refused'").bind(&document.envelope.id).fetch_one(&host.store.pool).await.unwrap(),0);
        }
        clock.set_time(clock.now().timestamp_millis()+31*86400*1000).unwrap();
        due(&reopened).await;
        reopened.social_collect_private_once().await.unwrap();
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_receive_failure").fetch_one(&reopened.store.pool).await.unwrap(),0);
        assert_eq!(store::sqlx::query_as::<_,(String,String)>("SELECT uid,body FROM record WHERE kind='message' ORDER BY uid").fetch_all(&reopened.store.pool).await.unwrap(),retained);
    })).await;
}
