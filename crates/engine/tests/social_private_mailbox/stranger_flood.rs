use super::*;

#[tokio::test]
async fn many_stranger_identities_fill_only_the_stranger_partition_and_control_progress_survives_reopen()
 {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("strangers.sqlite");
    let host = Engine::open(path.to_str().unwrap()).await.unwrap();
    host.social_command(
        nucleus::social::Command::ConfigureServices {
            settings: ServiceSettings {
                mailbox: true,
                ..Default::default()
            },
        },
        None,
        nucleus::execution::now(),
    )
    .await
    .unwrap();
    let now = nucleus::execution::now().timestamp();
    let node = iroh::SecretKey::from_bytes(&[209; 32]).public().to_string();
    let key = [205; 32];
    let recipient_account =
        session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
    let recipient_owner = Signer::from_bytes("", "social", [206; 32]);
    let recipient = certified(&recipient_owner, &recipient_account, 1, now);
    request(
        &host,
        &node,
        PublicRequest::RegisterReplyRoute {
            document: recipient.clone(),
            post: None,
        },
        now,
    )
    .await
    .unwrap();
    let mut first = None;
    let mut accepted = 0;
    for index in 0..64u32 {
        let mut seed = [207; 32];
        seed[..4].copy_from_slice(&index.to_le_bytes());
        let owner = Signer::from_bytes("", "social", seed);
        let account = session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
        let sender = certified(&owner, &account, 1, now);
        let mut live = account
            .account(&key)
            .unwrap()
            .create_outbound_session(
                SessionConfig::version_1(),
                vodozemac::Curve25519PublicKey::from_base64(&recipient.route.identity_key).unwrap(),
                vodozemac::Curve25519PublicKey::from_base64(&recipient.route.prekey).unwrap(),
            )
            .unwrap();
        let document = delivery(&sender, &account, &recipient, &mut live, now);
        let result = host
            .social_public_request(
                &format!("stranger-{index}"),
                &node,
                PublicRequest::DeliverPrivate {
                    document: document.clone(),
                },
                now,
            )
            .await;
        if index < 32 {
            result.unwrap();
            accepted += 1;
            if first.is_none() {
                first = Some((owner, account, sender, live, document));
            }
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("partition is full")
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_owner_control WHERE owner=?"
                )
                .bind(sender.control.owner_key)
                .fetch_one(&host.store.pool)
                .await
                .unwrap(),
                0
            );
        }
    }
    assert_eq!(accepted, 32);
    host.store.pool.close().await;
    let host = Engine::open(path.to_str().unwrap()).await.unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_service_envelope WHERE partition='stranger'"
        )
        .fetch_one(&host.store.pool)
        .await
        .unwrap(),
        32
    );
    let (sender_owner, sender_account, sender, mut live, first_document) = first.unwrap();
    let mut admission = SenderAdmission {
        mailbox: recipient.route.mailbox.clone(),
        sender_owner: sender.control.owner_key.clone(),
        state: AdmissionState::Accepted,
        window: 0,
        issued_at: now,
        expires_at: now + 30 * 86400,
        control: recipient.control.clone(),
        certificate: recipient.certificate.clone(),
        signature: String::new(),
    };
    admission.signature = recipient_account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("sender-admission", &admission).unwrap());
    request(
        &host,
        &node,
        PublicRequest::AdmitPrivateSender {
            document: admission,
        },
        now,
    )
    .await
    .unwrap();
    let trusted = reply_delivery(&sender, &sender_account, &recipient, &mut live, now);
    request(
        &host,
        &node,
        PublicRequest::DeliverPrivate { document: trusted },
        now,
    )
    .await
    .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_service_envelope WHERE partition='trusted'"
        )
        .fetch_one(&host.store.pool)
        .await
        .unwrap(),
        1
    );
    let mut receipt = RecipientReceipt {
        envelope: first_document.envelope.id.clone(),
        envelope_hash: document_hash("private-envelope", &first_document.envelope).unwrap(),
        message: first_document.envelope.message.clone(),
        content_hash: first_document.envelope.content_hash.clone(),
        stage: ReceiptStage::RecipientDurable,
        at: now,
        certificate: recipient.certificate.clone(),
        signature: String::new(),
    };
    receipt.signature = recipient_account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("recipient-receipt", &receipt).unwrap());
    request(
        &host,
        &node,
        PublicRequest::AcknowledgePrivate {
            access: access(
                &recipient,
                &recipient_account,
                vec![first_document.envelope.id.clone()],
                now,
            ),
            receipts: vec![receipt],
        },
        now,
    )
    .await
    .unwrap();
    let revoked = certified(&sender_owner, &sender_account, 2, now);
    request(
        &host,
        &node,
        PublicRequest::UpdateReplyAuthority {
            document: revoked.control,
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_service_envelope WHERE sender=?"
        )
        .bind(&sender.control.owner_key)
        .fetch_one(&host.store.pool)
        .await
        .unwrap(),
        0
    );
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: first_document
            },
            now
        )
        .await
        .is_err()
    );
    host.store.pool.close().await;
}
