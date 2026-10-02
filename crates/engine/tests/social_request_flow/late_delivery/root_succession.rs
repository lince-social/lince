use super::*;
use engine::{
    roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
    trust::Signer,
};
use nucleus::social::requests::{
    DELIVERY_NAMESPACE, MESSAGE_NAMESPACE, PARTICIPANTS_NAMESPACE, SESSION_AUTHORITY_NAMESPACE,
};

#[tokio::test]
async fn signed_root_succession_keeps_pending_private_ciphertext_history_and_deadline() {
    pending_root_succession(false).await;
}

#[tokio::test]
async fn trusted_successor_rebinds_private_authority_after_old_root_revocation() {
    pending_root_succession(true).await;
}

#[tokio::test]
async fn root_rebinding_keeps_a_forged_historical_binding_in_review() {
    let clock = nucleus::execution::Execution::new([220; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let fixture = accepted().await;
            let participant = store::records::get_extension(
                &fixture.sender.store.pool,
                &fixture.conversation,
                PARTICIPANTS_NAMESPACE,
            )
            .await
            .unwrap()
            .unwrap();
            let context = participant["context"].as_str().unwrap();
            let mut authority = store::records::get_extension(
                &fixture.sender.store.pool,
                context,
                SESSION_AUTHORITY_NAMESPACE,
            )
            .await
            .unwrap()
            .unwrap();
            authority["binding"]["signature"] =
                serde_json::json!("forged historical owner binding");
            store::records::set_extension(
                &fixture.sender.store.pool,
                context,
                SESSION_AUTHORITY_NAMESPACE,
                &authority,
            )
            .await
            .unwrap();
            let wallet: (String, i64) =
                store::sqlx::query_as("SELECT body,version FROM social_device_state WHERE id=?")
                    .bind(format!("authority:{context}"))
                    .fetch_one(&fixture.sender.store.pool)
                    .await
                    .unwrap();
            successor(&fixture, true).await;
            assert_eq!(
                fixture
                    .sender
                    .social_refresh_reply_authorizations()
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(
                store::records::get_extension(
                    &fixture.sender.store.pool,
                    context,
                    SESSION_AUTHORITY_NAMESPACE
                )
                .await
                .unwrap()
                .unwrap(),
                authority
            );
            assert_eq!(
                store::sqlx::query_as::<_, (String, i64)>(
                    "SELECT body,version FROM social_device_state WHERE id=?"
                )
                .bind(format!("authority:{context}"))
                .fetch_one(&fixture.sender.store.pool)
                .await
                .unwrap(),
                wallet
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, String>(
                    "SELECT state FROM social_context_retention WHERE context=?"
                )
                .bind(context)
                .fetch_one(&fixture.sender.store.pool)
                .await
                .unwrap(),
                "review"
            );
            assert!(fixture.sender.social_prepare_messages_once().await.is_ok());
        }))
        .await;
}

async fn successor(fixture: &Accepted, revoke_old: bool) -> Signer {
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
    let old_root = Signer::from_bytes(&organ, ROOT_KEY_ID, [238; 32]);
    let fresh_root = Signer::from_bytes(&organ, ROOT_KEY_ID, [220; 32]);
    fixture
        .sender
        .sign_succession(&old_root, &fresh_root.public_key_b64())
        .await
        .unwrap();
    std::fs::write(
        fixture._directories[1].path().join("root.key"),
        fresh_root.secret_bytes(),
    )
    .unwrap();
    fixture
        .sender
        .publish_roster(
            &fresh_root,
            vec![CellEntry {
                cell_uid: cell,
                label: "Owner after root succession".into(),
                node_id: iroh::SecretKey::from_bytes(&[221; 32]).public().to_string(),
                operational_key: fixture
                    .sender
                    .operational_key_for(&organ)
                    .await
                    .unwrap()
                    .public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: full_capabilities(),
            }],
        )
        .await
        .unwrap();
    assert!(
        fixture
            .sender
            .key_chains(&organ, &fresh_root.public_key_b64())
            .await
            .unwrap()
    );
    if revoke_old {
        let (key, signature) = fixture.sender.revocation_certificate(&old_root);
        assert!(
            fixture
                .sender
                .adopt_revocation(&organ, &key, &signature)
                .await
                .unwrap()
        );
        assert!(
            !fixture
                .sender
                .key_chains(&organ, &old_root.public_key_b64())
                .await
                .unwrap()
        );
        assert!(
            fixture
                .sender
                .key_chains(&organ, &fresh_root.public_key_b64())
                .await
                .unwrap()
        );
    }
    fresh_root
}

async fn pending_root_succession(revoke_old: bool) {
    let clock = nucleus::execution::Execution::new([219; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let fixture = accepted().await;
        let dormant = if revoke_old {
            let saved = command(&fixture.sender, Command::SaveDraft { record:None, source:None, draft:PostDraft { title:"Archived unused reply context".into(), ..Default::default() } }).await;
            let context = saved["record"].as_str().unwrap().to_owned();
            command(&fixture.sender, Command::PrepareReplyKeys { record:context.clone(), services:fixture.services.clone() }).await;
            command(&fixture.sender, Command::ArchivePost { record:context.clone() }).await;
            fixture.sender.social_refresh_reply_authorizations().await.unwrap();
            let retained:(String,i64) = store::sqlx::query_as("SELECT state,retire_after FROM social_context_retention WHERE context=?").bind(&context).fetch_one(&fixture.sender.store.pool).await.unwrap();
            assert_eq!(retained.0, "dormant");
            let wallet:(String,i64) = store::sqlx::query_as("SELECT body,version FROM social_device_state WHERE id=?").bind(format!("authority:{context}")).fetch_one(&fixture.sender.store.pool).await.unwrap();
            let authority = store::records::get_extension(&fixture.sender.store.pool, &context, SESSION_AUTHORITY_NAMESPACE).await.unwrap().unwrap();
            Some((context,retained,wallet,authority))
        } else { None };
        fixture.hosts.offline.lock().unwrap().extend(fixture.services.iter().cloned());
        let text = "Pending conversation survives signed root succession";
        let saved = command(&fixture.sender,Command::SendPrivate {conversation:fixture.conversation.clone(),text:text.into()}).await;
        let message = saved["message"].as_str().unwrap();
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let old:String = store::sqlx::query_scalar("SELECT body FROM social_private_outbox WHERE record_uid=? ORDER BY rowid DESC LIMIT 1").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap();
        let old:PrivateDelivery = serde_json::from_str(&old).unwrap();
        let metadata = store::records::get_extension(&fixture.sender.store.pool,message,MESSAGE_NAMESPACE).await.unwrap().unwrap();
        let status = store::records::get_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE).await.unwrap().unwrap();
        if revoke_old {
            clock.set_time(clock.now().timestamp_millis() + 86400 * 1000).unwrap();
        }
        let fresh_root = successor(&fixture, revoke_old).await;
        fixture.hosts.offline.lock().unwrap().clear();
        fixture.sender.social_refresh_reply_authorizations().await.unwrap();
        publication(&fixture.sender).await;
        let private_context: String = store::sqlx::query_scalar("SELECT context FROM social_private_outbox WHERE id=?").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap();
        let authority = store::records::get_extension(&fixture.sender.store.pool, &private_context, SESSION_AUTHORITY_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(authority["binding"]["root_key"], fresh_root.public_key_b64());
        assert_eq!(authority["binding"]["owner_key"], old.envelope.sender_owner);
        if let Some((context, retained, wallet, before)) = dormant {
            assert_eq!(store::sqlx::query_as::<_,(String,i64)>("SELECT state,retire_after FROM social_context_retention WHERE context=?").bind(&context).fetch_one(&fixture.sender.store.pool).await.unwrap(), retained);
            assert_eq!(store::sqlx::query_as::<_,(String,i64)>("SELECT body,version FROM social_device_state WHERE id=?").bind(format!("authority:{context}")).fetch_one(&fixture.sender.store.pool).await.unwrap(), wallet);
            let current = store::records::get_extension(&fixture.sender.store.pool, &context, SESSION_AUTHORITY_NAMESPACE).await.unwrap().unwrap();
            assert_eq!(current["binding"]["root_key"], fresh_root.public_key_b64());
            assert_eq!(current["control"], before["control"]);
        }
        for _ in 0..2 {
            exchange(&fixture.sender,&fixture.receiver).await;
        }
        let retained:String = store::sqlx::query_scalar("SELECT body FROM social_private_outbox WHERE id=?").bind(&old.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap();
        let retained:PrivateDelivery = serde_json::from_str(&retained).unwrap();
        assert_eq!(serde_json::to_value(&retained.envelope).unwrap(),serde_json::to_value(&old.envelope).unwrap());
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,MESSAGE_NAMESPACE).await.unwrap().unwrap(),metadata);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,DELIVERY_NAMESPACE).await.unwrap().unwrap()["expires_at"],status["expires_at"]);
        let messages:Vec<String> = store::sqlx::query_scalar("SELECT uid FROM record WHERE kind='message' AND body=?").bind(text).fetch_all(&fixture.receiver.store.pool).await.unwrap();
        assert_eq!(messages.len(),1);
        assert_eq!(store::records::get(&fixture.receiver.store.pool,&messages[0]).await.unwrap().unwrap().quantity,store::exact::one());
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_message_event WHERE record_uid=?").bind(&messages[0]).fetch_one(&fixture.receiver.store.pool).await.unwrap(),1);
    })).await;
}
