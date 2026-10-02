use super::*;
use nucleus::social::requests::{DELIVERY_NAMESPACE, MESSAGE_NAMESPACE, PARTICIPANTS_NAMESPACE};

async fn envelope(engine: &Engine, message: &str) -> PrivateDelivery {
    let body:String = store::sqlx::query_scalar("SELECT body FROM social_private_outbox WHERE record_uid=? AND state IN ('pending','stored') ORDER BY rowid DESC LIMIT 1").bind(message).fetch_one(&engine.store.pool).await.unwrap();
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn mailbox_stored_message_rewraps_after_recipient_key_replacement() {
    let clock = nucleus::execution::Execution::new([215; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let fixture = accepted().await;
        let text = "Stored while the recipient replaces device keys";
        let sent = command(&fixture.sender, Command::SendPrivate {
            conversation: fixture.conversation.clone(), text: text.into(),
        }).await;
        let message = sent["message"].as_str().unwrap();
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        publication(&fixture.sender).await;
        let old = envelope(&fixture.sender, message).await;
        for _ in 0..4 {
            fixture.sender.social_send_private_once().await.unwrap();
        }
        let metadata = store::records::get_extension(&fixture.sender.store.pool, message, MESSAGE_NAMESPACE).await.unwrap().unwrap();
        let status = store::records::get_extension(&fixture.sender.store.pool, message, DELIVERY_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(status["stage"], "mailbox-stored");
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_private_destination WHERE envelope=? AND state='stored'").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(), 2);
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_message_work WHERE record_uid=?").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap(), 1);
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM record WHERE kind='message' AND body=?").bind(text).fetch_one(&fixture.receiver.store.pool).await.unwrap(), 0);
        command(&fixture.receiver, Command::ResetPrivateSessions).await;
        clock.set_time(clock.now().timestamp_millis() + 2000).unwrap();
        command(&fixture.receiver, Command::PrepareReplyKeys {
            record: fixture.context.clone(), services: fixture.services.clone(),
        }).await;
        publication(&fixture.receiver).await;
        routed(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let fresh = envelope(&fixture.sender, message).await;
        assert_ne!(fresh.envelope.route, old.envelope.route);
        assert_ne!(fresh.envelope.id, old.envelope.id);
        assert_ne!(fresh.envelope.ciphertext, old.envelope.ciphertext);
        assert_eq!(fresh.envelope.message, old.envelope.message);
        assert_eq!(fresh.envelope.content_hash, old.envelope.content_hash);
        assert_eq!(fresh.envelope.expires_at, old.envelope.expires_at);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool, message, MESSAGE_NAMESPACE).await.unwrap().unwrap(), metadata);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool, message, DELIVERY_NAMESPACE).await.unwrap().unwrap()["expires_at"], status["expires_at"]);
        assert_eq!(store::sqlx::query_scalar::<_, String>("SELECT state FROM social_private_outbox WHERE id=?").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(), "held");
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_private_destination WHERE envelope=? AND state<>'cancelled'").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(), 0);
        import_once(&fixture, text).await;
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool, message, DELIVERY_NAMESPACE).await.unwrap().unwrap()["stage"], "recipient-durable");
    })).await;
}

async fn routed(engine: &Engine) {
    store::sqlx::query("UPDATE social_peer_work SET next_attempt=0")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    assert!(engine.social_refresh_private_routes_once().await.unwrap() > 0);
}

async fn import_once(fixture: &Accepted, text: &str) {
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
}

#[tokio::test]
async fn pending_sender_and_recipient_key_replacement_preserves_logical_messages_and_deadlines() {
    let clock = nucleus::execution::Execution::new([214; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let fixture = accepted().await;
        fixture.hosts.offline.lock().unwrap().extend(fixture.services.iter().cloned());
        let text = "Pending text from fresh sender keys";
        let sent = command(&fixture.sender,Command::SendPrivate { conversation:fixture.conversation.clone(),text:text.into() }).await;
        let message = sent["message"].as_str().unwrap();
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let old = envelope(&fixture.sender,message).await;
        let metadata = store::records::get_extension(&fixture.sender.store.pool,message,MESSAGE_NAMESPACE).await.unwrap().unwrap();
        let status = store::records::get_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(fixture.sender.social_send_private_once().await.unwrap(),0);
        command(&fixture.sender,Command::ResetPrivateSessions).await;
        clock.set_time(clock.now().timestamp_millis()+2000).unwrap();
        command(&fixture.sender,Command::ResumePrivate { message:message.into() }).await;
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let fresh = envelope(&fixture.sender,message).await;
        assert_ne!(fresh.envelope.id,old.envelope.id);
        assert_ne!(fresh.envelope.identity_key,old.envelope.identity_key);
        assert_ne!(fresh.envelope.ciphertext,old.envelope.ciphertext);
        assert_eq!(fresh.envelope.message,old.envelope.message);
        assert_eq!(fresh.envelope.content_hash,old.envelope.content_hash);
        assert_eq!(fresh.envelope.sender_owner,old.envelope.sender_owner);
        assert_eq!(fresh.envelope.expires_at,old.envelope.expires_at);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,MESSAGE_NAMESPACE).await.unwrap().unwrap(),metadata);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE).await.unwrap().unwrap()["expires_at"],status["expires_at"]);
        assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT state FROM social_private_outbox WHERE id=?").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(),"held");
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_private_destination WHERE envelope=? AND state<>'cancelled'").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(),0);
        fixture.hosts.offline.lock().unwrap().clear();
        publication(&fixture.sender).await;
        import_once(&fixture,text).await;
        routed(&fixture.receiver).await;
        fixture.hosts.offline.lock().unwrap().extend(fixture.services.iter().cloned());
        let text = "Pending text for fresh recipient keys";
        let sent = command(&fixture.sender,Command::SendPrivate { conversation:fixture.conversation.clone(),text:text.into() }).await;
        let message = sent["message"].as_str().unwrap();
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let old = envelope(&fixture.sender,message).await;
        let metadata = store::records::get_extension(&fixture.sender.store.pool,message,MESSAGE_NAMESPACE).await.unwrap().unwrap();
        let p = store::records::get_extension(&fixture.sender.store.pool,&fixture.conversation,PARTICIPANTS_NAMESPACE).await.unwrap().unwrap();
        command(&fixture.receiver,Command::ResetPrivateSessions).await;
        clock.set_time(clock.now().timestamp_millis()+2000).unwrap();
        command(&fixture.receiver,Command::PrepareReplyKeys { record:fixture.context.clone(),services:fixture.services.clone() }).await;
        fixture.hosts.offline.lock().unwrap().clear();
        publication(&fixture.receiver).await;
        routed(&fixture.sender).await;
        let current = store::records::get_extension(&fixture.sender.store.pool,&fixture.conversation,PARTICIPANTS_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(current["token"],p["token"]);
        assert_eq!(current["peer_owner"],p["peer_owner"]);
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let fresh = envelope(&fixture.sender,message).await;
        assert_ne!(fresh.envelope.route,old.envelope.route);
        assert_ne!(fresh.envelope.id,old.envelope.id);
        assert_eq!(fresh.envelope.message,old.envelope.message);
        assert_eq!(fresh.envelope.content_hash,old.envelope.content_hash);
        assert_eq!(fresh.envelope.expires_at,old.envelope.expires_at);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,MESSAGE_NAMESPACE).await.unwrap().unwrap(),metadata);
        for service in &fixture.services {
            assert!(fixture.hosts.nodes[service].social_public_request("pending-key-fixture",service,PublicRequest::DeliverPrivate { document:old.clone() },nucleus::execution::now().timestamp()).await.is_err());
        }
        import_once(&fixture,text).await;
        assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT state FROM social_private_outbox WHERE id=?").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(),"held");
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_private_destination WHERE envelope=? AND state<>'cancelled'").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(),0);
    })).await;
}
