use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    Engine,
    social::{document_hash, request_auth, session, signing_bytes},
    trust::Signer,
};
use nucleus::social::{PublicRequest, ServiceSettings, requests::*};
use serde_json::{Value, json};
use vodozemac::olm::{Session, SessionConfig};

#[path = "social_private_mailbox/protocol.rs"]
mod protocol;

#[path = "social_private_mailbox/stranger_flood.rs"]
mod stranger_flood;

fn certified(
    owner: &Signer,
    account: &session::AccountState,
    generation: i64,
    now: i64,
) -> CertifiedRoute {
    let mut control = OwnerControl {
        owner_key: owner.public_key_b64(),
        generation: generation.to_string(),
        issued_at: now,
        expires_at: now + AUTHORITY_LIFETIME,
        signature: String::new(),
    };
    control.signature = owner.sign_bytes(&signing_bytes("reply-owner", &control).unwrap());
    let mut certificate = DeviceCertificate {
        owner_key: owner.public_key_b64(),
        signing_key: account.route.signing_key.clone(),
        identity_key: account.route.identity_key.clone(),
        pickup_key: account.route.pickup_key.clone(),
        mailbox: account.route.mailbox.clone(),
        generation: generation.to_string(),
        issued_at: now,
        expires_at: control.expires_at,
        signature: String::new(),
    };
    certificate.signature = owner.sign_bytes(&signing_bytes("reply-device", &certificate).unwrap());
    let mut route = CertifiedRoute {
        route: account.route.clone(),
        accepting_introductions: true,
        control,
        certificate,
        expires_at: now + 30 * 86400,
        signature: String::new(),
    };
    route.signature = account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("reply-route", &route).unwrap());
    request_auth::validate_route(&route, now).unwrap();
    route
}

fn delivery(
    sender: &CertifiedRoute,
    sender_account: &session::AccountState,
    recipient: &CertifiedRoute,
    session: &mut Session,
    now: i64,
) -> PrivateDelivery {
    let message = nucleus::new_uid("msg");
    let content = json!({"text":"Private introduction","message":message});
    let hash = document_hash("private-content", &content).unwrap();
    let encrypted = session
        .encrypt(serde_json::to_vec(&content).unwrap())
        .unwrap();
    let (message_type, ciphertext) = encrypted.to_parts();
    let mut envelope = PrivateEnvelope {
        protocol: "lince.private-message.1".into(),
        id: nucleus::new_uid("env"),
        route: recipient.route.mailbox.clone(),
        sender_owner: sender.control.owner_key.clone(),
        sender_key: sender.route.signing_key.clone(),
        identity_key: sender.route.identity_key.clone(),
        control: sender.control.clone(),
        certificate: sender.certificate.clone(),
        session_id: session.session_id(),
        message,
        content_hash: hash,
        created_at: now,
        expires_at: now + 7 * 86400,
        message_type: message_type as u8,
        purpose: EnvelopePurpose::Introduction,
        ciphertext: B64.encode(ciphertext),
        signature: String::new(),
    };
    envelope.id = request_auth::envelope_id(&envelope).unwrap();
    envelope.signature = sender_account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("private-envelope", &envelope).unwrap());
    PrivateDelivery {
        envelope,
        authorization: FreshAuthorization {
            control: sender.control.clone(),
            certificate: sender.certificate.clone(),
        },
    }
}

fn access(
    route: &CertifiedRoute,
    account: &session::AccountState,
    envelopes: Vec<String>,
    now: i64,
) -> MailboxAccess {
    static SEQUENCE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let mut access = MailboxAccess {
        mailbox: route.route.mailbox.clone(),
        sequence: SEQUENCE
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .to_string(),
        at: now,
        nonce: B64.encode(nonce),
        envelopes,
        control: route.control.clone(),
        certificate: route.certificate.clone(),
        signature: String::new(),
    };
    access.signature = account
        .pickup_key()
        .unwrap()
        .sign_bytes(&signing_bytes("mailbox-access", &access).unwrap());
    access
}

fn reply_delivery(
    sender: &CertifiedRoute,
    account: &session::AccountState,
    recipient: &CertifiedRoute,
    live: &mut Session,
    now: i64,
) -> PrivateDelivery {
    let mut document = delivery(sender, account, recipient, live, now);
    document.envelope.purpose = EnvelopePurpose::Content;
    document.envelope.id = request_auth::envelope_id(&document.envelope).unwrap();
    document.envelope.signature = account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("private-envelope", &document.envelope).unwrap());
    document
}

fn typed_delivery(
    sender: &CertifiedRoute,
    account: &session::AccountState,
    recipient: &CertifiedRoute,
    live: &mut Session,
    content: &PrivateContent,
) -> PrivateDelivery {
    let encrypted = live.encrypt(serde_json::to_vec(content).unwrap()).unwrap();
    let (message_type, bytes) = encrypted.to_parts();
    let mut envelope = PrivateEnvelope {
        protocol: "lince.private-message.1".into(),
        id: String::new(),
        route: recipient.route.mailbox.clone(),
        sender_owner: sender.control.owner_key.clone(),
        sender_key: sender.route.signing_key.clone(),
        identity_key: sender.route.identity_key.clone(),
        control: sender.control.clone(),
        certificate: sender.certificate.clone(),
        session_id: live.session_id(),
        message: content.message.clone(),
        content_hash: document_hash("private-content", content).unwrap(),
        created_at: content.issued_at,
        expires_at: content.issued_at + AUTHORITY_LIFETIME,
        message_type: message_type as u8,
        purpose: content.kind.purpose(),
        ciphertext: B64.encode(bytes),
        signature: String::new(),
    };
    envelope.id = request_auth::envelope_id(&envelope).unwrap();
    envelope.signature = account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("private-envelope", &envelope).unwrap());
    PrivateDelivery {
        envelope,
        authorization: FreshAuthorization {
            control: sender.control.clone(),
            certificate: sender.certificate.clone(),
        },
    }
}

#[tokio::test]
async fn private_receive_commits_ratchets_records_and_receipts_once_and_rolls_back_bad_content() {
    let clock = nucleus::execution::Execution::new([249; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            use engine::actions::Action;
            use nucleus::social::{Command, PostDraft, PostState, Snippet};
            let recipient_engine = Engine::open_memory().await.unwrap();
            let directory = tempfile::tempdir().unwrap();
            recipient_engine.set_sealing_keyring_path(directory.path().join("sealing.json"));
            let root_path = directory.path().join("root.key");
            std::fs::write(&root_path, [225; 32]).unwrap();
            recipient_engine.set_root_key_path(root_path);
            let organ = store::organs::local(&recipient_engine.store.pool)
                .await
                .unwrap()
                .unwrap()
                .uid;
            let signer = recipient_engine.operational_key_for(&organ).await.unwrap();
            recipient_engine.set_signer(signer.clone()).await.unwrap();
            recipient_engine.set_organ_signer(signer).await.unwrap();
            let node = iroh::SecretKey::from_bytes(&[226; 32]).public().to_string();
            let second = iroh::SecretKey::from_bytes(&[227; 32]).public().to_string();
            let saved = recipient_engine
                .act(
                    Action::Social {
                        request: Command::SaveDraft {
                            record: None,
                            source: None,
                            draft: PostDraft {
                                title: "Bicycle help".into(),
                                ..Default::default()
                            },
                        },
                    },
                    None,
                )
                .await
                .unwrap()
                .data
                .unwrap();
            let context = saved["record"].as_str().unwrap().to_owned();
            let prepared = recipient_engine
                .act(
                    Action::Social {
                        request: Command::PrepareReplyKeys {
                            record: context.clone(),
                            services: vec![node.clone(), second.clone()],
                        },
                    },
                    None,
                )
                .await
                .unwrap()
                .data
                .unwrap();
            let recipient = future_certificate(
                &recipient_engine,
                &context,
                serde_json::from_value(prepared["route"].clone()).unwrap(),
            )
            .await;
            let first_access = recipient_engine
                .social_private_access(&context, vec![])
                .await
                .unwrap();
            let second_access = recipient_engine
                .social_private_access(&context, vec![])
                .await
                .unwrap();
            assert_eq!(first_access.sequence, "1");
            assert_eq!(second_access.sequence, "2");
            let reviewed = recipient_engine
                .act(
                    Action::Social {
                        request: Command::Preview {
                            record: context.clone(),
                            state: PostState::Active,
                        },
                    },
                    None,
                )
                .await
                .unwrap()
                .data
                .unwrap();
            let post: Snippet = serde_json::from_value(reviewed["document"].clone()).unwrap();
            recipient_engine
                .act(
                    Action::Social {
                        request: Command::Publish {
                            record: context.clone(),
                            preview_hash: reviewed["preview_hash"].as_str().unwrap().into(),
                            document: post.clone(),
                        },
                    },
                    None,
                )
                .await
                .unwrap();
            let now = nucleus::execution::now().timestamp();
            let key = [228; 32];
            let sender_account =
                session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
            let sender_owner = Signer::from_bytes("", "pseudonym", [229; 32]);
            let sender = certified(&sender_owner, &sender_account, 1, now);
            let mut outbound = sender_account
                .account(&key)
                .unwrap()
                .create_outbound_session(
                    SessionConfig::version_1(),
                    vodozemac::Curve25519PublicKey::from_base64(&recipient.route.identity_key)
                        .unwrap(),
                    vodozemac::Curve25519PublicKey::from_base64(&recipient.route.prekey).unwrap(),
                )
                .unwrap();
            let content = PrivateContent {
                protocol: "lince.private-content.1".into(),
                conversation: nucleus::new_uid("talk"),
                message: nucleus::new_uid("msg"),
                author_owner: sender.control.owner_key.clone(),
                issued_at: now,
                kind: ContentKind::Introduction {
                    post: post.id.clone(),
                    text: "I can help repair your bicycle".into(),
                    alias: "Neighbor".into(),
                    reply: Box::new(sender.clone()),
                },
            };
            let original = typed_delivery(
                &sender,
                &sender_account,
                &recipient,
                &mut outbound,
                &content,
            );
            let encoded = serde_json::to_string(&original).unwrap();
            let mut corrupt_host_copy = original.clone();
            corrupt_host_copy.envelope.created_at = i64::MIN;
            assert!(
                recipient_engine
                    .social_receive_private(&context, &node, &corrupt_host_copy, now)
                    .await
                    .is_err()
            );
            for private_id in [&organ, &context] {
                assert!(!encoded.contains(private_id));
            }
            let mut bad = original.clone();
            bad.envelope.content_hash = "a".repeat(64);
            bad.envelope.id = request_auth::envelope_id(&bad.envelope).unwrap();
            bad.envelope.signature = sender_account
                .signing_key()
                .unwrap()
                .sign_bytes(&signing_bytes("private-envelope", &bad.envelope).unwrap());
            assert!(
                recipient_engine
                    .social_receive_private(&context, &node, &bad, now)
                    .await
                    .is_err()
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_device_state WHERE kind='session'"
                )
                .fetch_one(&recipient_engine.store.pool)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_private_seen")
                    .fetch_one(&recipient_engine.store.pool)
                    .await
                    .unwrap(),
                0
            );
            let first = recipient_engine
                .social_receive_private(&context, &node, &original, now)
                .await
                .unwrap();
            let duplicate = recipient_engine
                .social_receive_private(&context, &second, &original, now)
                .await
                .unwrap();
            for received in [&first, &duplicate] {
                assert!(
                    received["receipt"]["at"].as_i64().unwrap() >= recipient.certificate.issued_at
                );
            }
            assert_eq!(duplicate["duplicate"], true);
            assert_eq!(
                duplicate["receipt"]["envelope_hash"],
                first["receipt"]["envelope_hash"]
            );
            assert!(
                recipient_engine
                    .social_receive_private(&context, "unselected", &original, now)
                    .await
                    .is_err()
            );
            let resealed = typed_delivery(
                &sender,
                &sender_account,
                &recipient,
                &mut outbound,
                &content,
            );
            let same_message = recipient_engine
                .social_receive_private(&context, &second, &resealed, now)
                .await
                .unwrap();
            assert_eq!(same_message["message"], first["message"]);
            assert_eq!(same_message["duplicate"], true);
            let mut altered = content.clone();
            altered.kind = ContentKind::Text {
                text: "Different content under the same logical ID".into(),
            };
            let conflicting = typed_delivery(
                &sender,
                &sender_account,
                &recipient,
                &mut outbound,
                &altered,
            );
            assert!(
                recipient_engine
                    .social_receive_private(&context, &node, &conflicting, now)
                    .await
                    .is_err()
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM record WHERE kind='message'"
                )
                .fetch_one(&recipient_engine.store.pool)
                .await
                .unwrap(),
                1
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fact WHERE record_uid=?")
                    .bind(first["message"].as_str().unwrap())
                    .fetch_one(&recipient_engine.store.pool)
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM replica_grant")
                    .fetch_one(&recipient_engine.store.pool)
                    .await
                    .unwrap(),
                0
            );
            let mut next = content.clone();
            next.message = nucleus::new_uid("msg");
            next.kind = ContentKind::Text {
                text: "The rejected message did not advance your saved ratchet".into(),
            };
            let next = typed_delivery(&sender, &sender_account, &recipient, &mut outbound, &next);
            let received = recipient_engine
                .social_receive_private(&context, &node, &next, now)
                .await
                .unwrap();
            assert_eq!(received["conversation"], first["conversation"]);
            let replacement_account =
                session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
            let replacement = certified(&sender_owner, &replacement_account, 2, now);
            let mut fresh_session = replacement_account
                .account(&key)
                .unwrap()
                .create_outbound_session(
                    SessionConfig::version_1(),
                    vodozemac::Curve25519PublicKey::from_base64(&recipient.route.identity_key)
                        .unwrap(),
                    vodozemac::Curve25519PublicKey::from_base64(&recipient.route.prekey).unwrap(),
                )
                .unwrap();
            let mut fresh_copy = typed_delivery(
                &replacement,
                &replacement_account,
                &recipient,
                &mut fresh_session,
                &content,
            );
            fresh_copy.envelope.created_at = now + 1;
            fresh_copy.envelope.expires_at = now + 1 + AUTHORITY_LIFETIME;
            fresh_copy.envelope.id = request_auth::envelope_id(&fresh_copy.envelope).unwrap();
            fresh_copy.envelope.signature = replacement_account
                .signing_key()
                .unwrap()
                .sign_bytes(&signing_bytes("private-envelope", &fresh_copy.envelope).unwrap());
            let recovered = recipient_engine
                .social_receive_private(&context, &node, &fresh_copy, now + 1)
                .await
                .unwrap();
            assert_eq!(recovered["message"], first["message"]);
            assert_eq!(recovered["duplicate"], true);
            assert!(
                recipient_engine
                    .social_receive_private(&context, &node, &original, now)
                    .await
                    .is_err()
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM record WHERE kind='message'"
                )
                .fetch_one(&recipient_engine.store.pool)
                .await
                .unwrap(),
                2
            );
            let conversation = first["conversation"].as_str().unwrap();
            recipient_engine
                .social_command(
                    Command::DecideRequest {
                        conversation: conversation.into(),
                        decision: nucleus::social::RequestDecision::Block,
                    },
                    None,
                    nucleus::execution::now(),
                )
                .await
                .unwrap();
            let count: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='message'")
                    .fetch_one(&recipient_engine.store.pool)
                    .await
                    .unwrap();
            let replay = typed_delivery(
                &replacement,
                &replacement_account,
                &recipient,
                &mut fresh_session,
                &content,
            );
            assert_eq!(
                recipient_engine
                    .social_receive_private(&context, &node, &replay, now)
                    .await
                    .unwrap()["duplicate"],
                true
            );
            let mut fresh_intro = content.clone();
            fresh_intro.conversation = nucleus::new_uid("talk");
            fresh_intro.message = nucleus::new_uid("msg");
            let fresh_intro_delivery = typed_delivery(
                &replacement,
                &replacement_account,
                &recipient,
                &mut fresh_session,
                &fresh_intro,
            );
            assert!(
                recipient_engine
                    .social_receive_private(&context, &node, &fresh_intro_delivery, now)
                    .await
                    .is_err()
            );
            let after: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='message'")
                    .fetch_one(&recipient_engine.store.pool)
                    .await
                    .unwrap();
            assert_eq!(after, count);
            recipient_engine
                .social_command(
                    Command::UnblockParticipant {
                        context: context.clone(),
                        peer: replacement.control.owner_key.clone(),
                    },
                    None,
                    nucleus::execution::now(),
                )
                .await
                .unwrap();
            let new_request = recipient_engine
                .social_receive_private(&context, &node, &fresh_intro_delivery, now)
                .await
                .unwrap();
            assert_ne!(new_request["conversation"], first["conversation"]);
            let previous = store::records::get_extension(
                &recipient_engine.store.pool,
                conversation,
                PARTICIPANTS_NAMESPACE,
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(previous["state"], "blocked");
            let new_root = new_request["conversation"].as_str().unwrap();
            recipient_engine
                .social_command(
                    Command::DecideRequest {
                        conversation: new_root.into(),
                        decision: nucleus::social::RequestDecision::Decline,
                    },
                    None,
                    nucleus::execution::now(),
                )
                .await
                .unwrap();
            let count: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='message'")
                    .fetch_one(&recipient_engine.store.pool)
                    .await
                    .unwrap();
            let replay = typed_delivery(
                &replacement,
                &replacement_account,
                &recipient,
                &mut fresh_session,
                &fresh_intro,
            );
            assert_eq!(
                recipient_engine
                    .social_receive_private(&context, &node, &replay, now)
                    .await
                    .unwrap()["duplicate"],
                true
            );
            let declined = store::records::get_extension(
                &recipient_engine.store.pool,
                new_root,
                PARTICIPANTS_NAMESPACE,
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(declined["state"], "declined");
            let after: i64 =
                store::sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE kind='message'")
                    .fetch_one(&recipient_engine.store.pool)
                    .await
                    .unwrap();
            assert_eq!(after, count);
        }))
        .await;
}

async fn future_certificate(
    engine: &Engine,
    context: &str,
    mut route: CertifiedRoute,
) -> CertifiedRoute {
    let wallet_id = format!("authority:{context}");
    let wallet_body = store::social::device_state(&engine.store.pool, &wallet_id)
        .await
        .unwrap()
        .unwrap()
        .0;
    let wallet: Value = session::open_local(
        &wallet_id,
        &wallet_body,
        &engine.social_authority_storage_key().await.unwrap(),
    )
    .unwrap();
    let owner = Signer::from_bytes(
        "",
        "social",
        B64.decode(wallet["secret"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
    );
    route.certificate.issued_at = nucleus::execution::now().timestamp() + 1;
    route.certificate.signature =
        owner.sign_bytes(&signing_bytes("reply-device", &route.certificate).unwrap());
    let account_id = format!("account:{context}");
    let account_body = store::social::device_state(&engine.store.pool, &account_id)
        .await
        .unwrap()
        .unwrap()
        .0;
    let account: session::AccountState = session::open_local(
        &account_id,
        &account_body,
        &engine.social_storage_key().await.unwrap(),
    )
    .unwrap();
    route.signature = account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("reply-route", &route).unwrap());
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut authority =
        store::records::get_extension(&engine.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
            .await
            .unwrap()
            .unwrap();
    authority[format!("authorized_{cell}")] = serde_json::to_value(&route).unwrap();
    store::records::set_extension(
        &engine.store.pool,
        context,
        SESSION_AUTHORITY_NAMESPACE,
        &authority,
    )
    .await
    .unwrap();
    request_auth::validate_route(&route, nucleus::execution::now().timestamp()).unwrap();
    route
}

async fn request(
    host: &Engine,
    node: &str,
    request: PublicRequest,
    now: i64,
) -> Result<Value, engine::EngineError> {
    host.social_public_request(
        &iroh::SecretKey::from_bytes(&[171; 32]).public().to_string(),
        node,
        request,
        now,
    )
    .await
}

async fn host() -> Engine {
    let host = Engine::open_memory().await.unwrap();
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
    host
}

#[tokio::test]
async fn recipient_refusal_is_authenticated_persistent_and_never_reopens_introductions() {
    let now = nucleus::execution::now().timestamp();
    let node = iroh::SecretKey::from_bytes(&[181; 32]).public().to_string();
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("mailbox.db").display()
    );
    let host = Engine::open(&url).await.unwrap();
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
    let key = [182; 32];
    let recipient_account =
        session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
    let recipient = certified(
        &Signer::from_bytes("", "recipient", [183; 32]),
        &recipient_account,
        1,
        now,
    );
    let sender_account = session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
    let sender = certified(
        &Signer::from_bytes("", "sender", [184; 32]),
        &sender_account,
        1,
        now,
    );
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
    let mut live = sender_account
        .account(&key)
        .unwrap()
        .create_outbound_session(
            SessionConfig::version_1(),
            vodozemac::Curve25519PublicKey::from_base64(&recipient.route.identity_key).unwrap(),
            vodozemac::Curve25519PublicKey::from_base64(&recipient.route.prekey).unwrap(),
        )
        .unwrap();
    let first = delivery(&sender, &sender_account, &recipient, &mut live, now);
    request(
        &host,
        &node,
        PublicRequest::DeliverPrivate {
            document: first.clone(),
        },
        now,
    )
    .await
    .unwrap();
    let mut receipt = RecipientReceipt {
        envelope: first.envelope.id.clone(),
        envelope_hash: document_hash("private-envelope", &first.envelope).unwrap(),
        message: first.envelope.message.clone(),
        content_hash: first.envelope.content_hash.clone(),
        stage: ReceiptStage::RecipientRefused,
        at: now,
        certificate: recipient.certificate.clone(),
        signature: String::new(),
    };
    let signer = recipient_account.signing_key().unwrap();
    receipt.signature = signer.sign_bytes(&signing_bytes("recipient-receipt", &receipt).unwrap());
    let access_for = || {
        access(
            &recipient,
            &recipient_account,
            vec![first.envelope.id.clone()],
            now,
        )
    };
    let mut forged = receipt.clone();
    forged.signature = sender_account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("recipient-receipt", &forged).unwrap());
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DiscardPrivate {
                access: access_for(),
                receipts: vec![forged]
            },
            now
        )
        .await
        .is_err()
    );
    let mut changed = receipt.clone();
    changed.content_hash = "0".repeat(64);
    changed.signature = signer.sign_bytes(&signing_bytes("recipient-receipt", &changed).unwrap());
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DiscardPrivate {
                access: access_for(),
                receipts: vec![changed]
            },
            now
        )
        .await
        .is_err()
    );
    assert!(
        request(
            &host,
            &node,
            PublicRequest::AcknowledgePrivate {
                access: access_for(),
                receipts: vec![receipt.clone()]
            },
            now
        )
        .await
        .is_err()
    );
    for _ in 0..2 {
        let refused = request(
            &host,
            &node,
            PublicRequest::DiscardPrivate {
                access: access_for(),
                receipts: vec![receipt.clone()],
            },
            now,
        )
        .await
        .unwrap();
        assert_eq!(refused["stage"], "recipient-refused");
    }
    let mut false_success = receipt.clone();
    false_success.stage = ReceiptStage::RecipientDurable;
    false_success.signature =
        signer.sign_bytes(&signing_bytes("recipient-receipt", &false_success).unwrap());
    assert!(
        request(
            &host,
            &node,
            PublicRequest::AcknowledgePrivate {
                access: access_for(),
                receipts: vec![false_success]
            },
            now
        )
        .await
        .is_err()
    );
    let queued: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_service_envelope")
        .fetch_one(&host.store.pool)
        .await
        .unwrap();
    assert_eq!(queued, 0);
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: delivery(&sender, &sender_account, &recipient, &mut live, now)
            },
            now
        )
        .await
        .is_err()
    );
    host.store.pool.close().await;
    drop(host);
    let host = Engine::open(&url).await.unwrap();
    let inspected = request(
        &host,
        &node,
        PublicRequest::InspectPrivate {
            document: first.clone(),
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(inspected["stage"], "recipient-refused");
    assert_eq!(inspected["receipt"]["signature"], receipt.signature);
    let replay = request(
        &host,
        &node,
        PublicRequest::DeliverPrivate {
            document: first.clone(),
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(replay["stage"], "recipient-refused");
    let refused = request(
        &host,
        &node,
        PublicRequest::DiscardPrivate {
            access: access_for(),
            receipts: vec![receipt],
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(refused["stage"], "recipient-refused");
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_service_completed")
        .fetch_one(&host.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn unreadable_ciphertext_is_visible_and_explicit_discard_preserves_other_history() {
    let clock = nucleus::execution::Execution::new([250; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            use nucleus::social::{Command, PostDraft, PostState, Snippet};
            struct OneHost(std::sync::Arc<Engine>);
            #[async_trait::async_trait]
            impl engine::social::Network for OneHost {
                async fn request(
                    &self,
                    destination: &str,
                    request: PublicRequest,
                ) -> Result<Value, engine::EngineError> {
                    self.0
                        .social_public_request(
                            "refusal-test",
                            destination,
                            request,
                            nucleus::execution::now().timestamp(),
                        )
                        .await
                }
            }
            async fn command(engine: &Engine, command: Command) -> Value {
                engine
                    .social_command(command, None, nucleus::execution::now())
                    .await
                    .unwrap()
                    .data
                    .unwrap()
            }
            let recipient_engine = Engine::open_memory().await.unwrap();
            let dir = tempfile::tempdir().unwrap();
            recipient_engine.set_sealing_keyring_path(dir.path().join("sealing.json"));
            let root = dir.path().join("root.key");
            std::fs::write(&root, [185; 32]).unwrap();
            recipient_engine.set_root_key_path(root);
            let organ = store::organs::local(&recipient_engine.store.pool)
                .await
                .unwrap()
                .unwrap()
                .uid;
            let signer = recipient_engine.operational_key_for(&organ).await.unwrap();
            recipient_engine.set_signer(signer.clone()).await.unwrap();
            recipient_engine.set_organ_signer(signer).await.unwrap();
            let node = iroh::SecretKey::from_bytes(&[186; 32]).public().to_string();
            let saved = command(
                &recipient_engine,
                Command::SaveDraft {
                    record: None,
                    source: None,
                    draft: PostDraft {
                        title: "Bicycle help".into(),
                        ..Default::default()
                    },
                },
            )
            .await;
            let context = saved["record"].as_str().unwrap().to_owned();
            let prepared = command(
                &recipient_engine,
                Command::PrepareReplyKeys {
                    record: context.clone(),
                    services: vec![node.clone()],
                },
            )
            .await;
            let recipient = future_certificate(
                &recipient_engine,
                &context,
                serde_json::from_value(prepared["route"].clone()).unwrap(),
            )
            .await;
            let preview = command(
                &recipient_engine,
                Command::Preview {
                    record: context.clone(),
                    state: PostState::Active,
                },
            )
            .await;
            let post: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
            command(
                &recipient_engine,
                Command::Publish {
                    record: context.clone(),
                    preview_hash: preview["preview_hash"].as_str().unwrap().into(),
                    document: post.clone(),
                },
            )
            .await;
            let host = std::sync::Arc::new(host().await);
            let network: std::sync::Arc<dyn engine::social::Network> =
                std::sync::Arc::new(OneHost(host.clone()));
            recipient_engine.attach_social_network(network.clone());
            let now = nucleus::execution::now().timestamp();
            request(
                &host,
                &node,
                PublicRequest::RegisterReplyRoute {
                    document: recipient.clone(),
                    post: Some(Box::new(post.clone())),
                },
                now,
            )
            .await
            .unwrap();
            let key = [187; 32];
            let bad_account =
                session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
            let bad_sender = certified(
                &Signer::from_bytes("", "bad sender", [188; 32]),
                &bad_account,
                1,
                now,
            );
            let open = |account: &session::AccountState| {
                account
                    .account(&key)
                    .unwrap()
                    .create_outbound_session(
                        SessionConfig::version_1(),
                        vodozemac::Curve25519PublicKey::from_base64(&recipient.route.identity_key)
                            .unwrap(),
                        vodozemac::Curve25519PublicKey::from_base64(&recipient.route.prekey)
                            .unwrap(),
                    )
                    .unwrap()
            };
            let mut bad_session = open(&bad_account);
            let bad = delivery(&bad_sender, &bad_account, &recipient, &mut bad_session, now);
            request(
                &host,
                &node,
                PublicRequest::DeliverPrivate {
                    document: bad.clone(),
                },
                now,
            )
            .await
            .unwrap();
            assert_eq!(
                recipient_engine
                    .social_collect_private_once()
                    .await
                    .unwrap(),
                0
            );
            let errors = command(&recipient_engine, Command::Requests { after: None }).await;
            assert_eq!(errors["receive_failures"].as_array().unwrap().len(), 1);
            assert!(errors["receive_failures"][0]["error"].as_str().is_some());
            assert_eq!(errors["receive_failures"][0]["discard"], false);
            let good_account =
                session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
            let good_sender = certified(
                &Signer::from_bytes("", "good sender", [189; 32]),
                &good_account,
                1,
                now,
            );
            let mut good_session = open(&good_account);
            let content = PrivateContent {
                protocol: "lince.private-content.1".into(),
                conversation: nucleus::new_uid("talk"),
                message: nucleus::new_uid("msg"),
                author_owner: good_sender.control.owner_key.clone(),
                issued_at: now,
                kind: ContentKind::Introduction {
                    post: post.id,
                    text: "I can repair your bicycle".into(),
                    alias: "Neighbor".into(),
                    reply: Box::new(good_sender.clone()),
                },
            };
            let good = typed_delivery(
                &good_sender,
                &good_account,
                &recipient,
                &mut good_session,
                &content,
            );
            request(
                &host,
                &node,
                PublicRequest::DeliverPrivate { document: good },
                now,
            )
            .await
            .unwrap();
            store::sqlx::query("UPDATE social_pickup_work SET next_attempt=0")
                .execute(&recipient_engine.store.pool)
                .await
                .unwrap();
            assert_eq!(
                recipient_engine
                    .social_collect_private_once()
                    .await
                    .unwrap(),
                1
            );
            command(
                &recipient_engine,
                Command::DiscardPrivate {
                    context,
                    service: node.clone(),
                    envelope: bad.envelope.id.clone(),
                },
            )
            .await;
            store::sqlx::query("UPDATE social_pickup_work SET next_attempt=0")
                .execute(&recipient_engine.store.pool)
                .await
                .unwrap();
            assert_eq!(
                recipient_engine
                    .social_collect_private_once()
                    .await
                    .unwrap(),
                0
            );
            let state = command(&recipient_engine, Command::Requests { after: None }).await;
            assert!(state["receive_failures"].as_array().unwrap().is_empty());
            assert_eq!(state["requests"].as_array().unwrap().len(), 1);
            assert_eq!(
                state["requests"][0]["messages"][0]["body"],
                "I can repair your bicycle"
            );
            let refused = request(
                &host,
                &node,
                PublicRequest::InspectPrivate { document: bad },
                now,
            )
            .await
            .unwrap();
            assert_eq!(refused["stage"], "recipient-refused");
            assert!(refused["receipt"]["at"].as_i64().unwrap() >= recipient.certificate.issued_at);
            let seen: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_private_seen")
                .fetch_one(&recipient_engine.store.pool)
                .await
                .unwrap();
            assert_eq!(seen, 1);
        }))
        .await;
}

#[tokio::test]
async fn private_mailbox_deduplicates_and_keeps_acknowledged_introduction_limits() {
    let now = nucleus::execution::now().timestamp();
    let node = iroh::SecretKey::from_bytes(&[172; 32]).public().to_string();
    let host = host().await;
    let key = [173; 32];
    let recipient_account =
        session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
    let recipient_owner = Signer::from_bytes("", "pseudonym", [174; 32]);
    let recipient = certified(&recipient_owner, &recipient_account, 1, now);
    let sender_account = session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
    let sender_owner = Signer::from_bytes("", "pseudonym", [175; 32]);
    let sender = certified(&sender_owner, &sender_account, 1, now);
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
    let mut outbound = sender_account
        .account(&key)
        .unwrap()
        .create_outbound_session(
            SessionConfig::version_1(),
            vodozemac::Curve25519PublicKey::from_base64(&recipient.route.identity_key).unwrap(),
            vodozemac::Curve25519PublicKey::from_base64(&recipient.route.prekey).unwrap(),
        )
        .unwrap();
    let first = delivery(&sender, &sender_account, &recipient, &mut outbound, now);
    for _ in 0..2 {
        let stored = request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: first.clone(),
            },
            now,
        )
        .await
        .unwrap();
        assert_eq!(stored["stage"], "stored");
    }
    let collected = request(
        &host,
        &node,
        PublicRequest::CollectPrivate {
            access: access(&recipient, &recipient_account, vec![], now),
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(collected["envelopes"].as_array().unwrap().len(), 1);
    let stored = request(
        &host,
        &node,
        PublicRequest::InspectPrivate {
            document: first.clone(),
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(stored["stage"], "stored");
    assert!(stored["receipt"].is_null());
    let mut receipt = RecipientReceipt {
        envelope: first.envelope.id.clone(),
        envelope_hash: document_hash("private-envelope", &first.envelope).unwrap(),
        message: first.envelope.message.clone(),
        content_hash: first.envelope.content_hash.clone(),
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
                vec![first.envelope.id.clone()],
                now,
            ),
            receipts: vec![receipt],
        },
        now,
    )
    .await
    .unwrap();
    let replay = request(
        &host,
        &node,
        PublicRequest::DeliverPrivate {
            document: first.clone(),
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(replay["stage"], "recipient-durable");
    let second = reply_delivery(&sender, &sender_account, &recipient, &mut outbound, now);
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: second.clone()
            },
            now
        )
        .await
        .is_err()
    );
    let mut admission = SenderAdmission {
        mailbox: recipient.route.mailbox.clone(),
        sender_owner: sender.control.owner_key.clone(),
        state: AdmissionState::Provisional,
        window: 0,
        issued_at: now,
        expires_at: now + 7 * 86400,
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
            document: admission.clone(),
        },
        now,
    )
    .await
    .unwrap();
    request(
        &host,
        &node,
        PublicRequest::DeliverPrivate { document: second },
        now,
    )
    .await
    .unwrap();
    for _ in 0..2 {
        request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: reply_delivery(&sender, &sender_account, &recipient, &mut outbound, now),
            },
            now,
        )
        .await
        .unwrap();
    }
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: reply_delivery(&sender, &sender_account, &recipient, &mut outbound, now)
            },
            now
        )
        .await
        .is_err()
    );
    admission.state = AdmissionState::Accepted;
    admission.issued_at += 1;
    admission.signature = recipient_account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("sender-admission", &admission).unwrap());
    request(
        &host,
        &node,
        PublicRequest::AdmitPrivateSender {
            document: admission.clone(),
        },
        now,
    )
    .await
    .unwrap();
    request(
        &host,
        &node,
        PublicRequest::DeliverPrivate {
            document: delivery(&sender, &sender_account, &recipient, &mut outbound, now),
        },
        now,
    )
    .await
    .unwrap();
    admission.state = AdmissionState::Blocked;
    admission.issued_at += 1;
    admission.signature = recipient_account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("sender-admission", &admission).unwrap());
    request(
        &host,
        &node,
        PublicRequest::AdmitPrivateSender {
            document: admission.clone(),
        },
        now,
    )
    .await
    .unwrap();
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: delivery(&sender, &sender_account, &recipient, &mut outbound, now)
            },
            now
        )
        .await
        .is_err()
    );
    admission.state = AdmissionState::Provisional;
    admission.issued_at += 1;
    admission.window = now;
    admission.signature = recipient_account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("sender-admission", &admission).unwrap());
    request(
        &host,
        &node,
        PublicRequest::AdmitPrivateSender {
            document: admission.clone(),
        },
        now,
    )
    .await
    .unwrap();
    request(
        &host,
        &node,
        PublicRequest::DeliverPrivate {
            document: delivery(&sender, &sender_account, &recipient, &mut outbound, now),
        },
        now,
    )
    .await
    .unwrap();
    request(
        &host,
        &node,
        PublicRequest::AdmitPrivateSender {
            document: admission.clone(),
        },
        now,
    )
    .await
    .unwrap();
    admission.issued_at += 1;
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
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: delivery(&sender, &sender_account, &recipient, &mut outbound, now)
            },
            now
        )
        .await
        .is_err()
    );
    assert!(
        request(
            &host,
            &node,
            PublicRequest::Search {
                query: Default::default(),
                known: vec![]
            },
            now
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn known_revocations_and_pickup_replays_are_enforced_without_identity_secrets() {
    let now = nucleus::execution::now().timestamp();
    let node = iroh::SecretKey::from_bytes(&[176; 32]).public().to_string();
    let host = host().await;
    let key = [177; 32];
    let account = session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
    let owner = Signer::from_bytes("", "pseudonym", [178; 32]);
    let first = certified(&owner, &account, 1, now);
    request(
        &host,
        &node,
        PublicRequest::RegisterReplyRoute {
            document: first.clone(),
            post: None,
        },
        now,
    )
    .await
    .unwrap();
    let pickup = access(&first, &account, vec![], now);
    request(
        &host,
        &node,
        PublicRequest::CollectPrivate {
            access: pickup.clone(),
        },
        now,
    )
    .await
    .unwrap();
    assert!(
        request(
            &host,
            &node,
            PublicRequest::CollectPrivate { access: pickup },
            now
        )
        .await
        .is_err()
    );
    let fresh = certified(&owner, &account, 2, now);
    let routes = request(
        &host,
        &node,
        PublicRequest::LookupReplyRoutes {
            owner: owner.public_key_b64(),
            after: None,
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(routes["routes"].as_array().unwrap().len(), 1);
    request(
        &host,
        &node,
        PublicRequest::UpdateReplyAuthority {
            document: fresh.control.clone(),
        },
        now,
    )
    .await
    .unwrap();
    let routes = request(
        &host,
        &node,
        PublicRequest::LookupReplyRoutes {
            owner: owner.public_key_b64(),
            after: None,
        },
        now,
    )
    .await
    .unwrap();
    assert!(routes["routes"].as_array().unwrap().is_empty());
    request(
        &host,
        &node,
        PublicRequest::RegisterReplyRoute {
            document: fresh.clone(),
            post: None,
        },
        now,
    )
    .await
    .unwrap();
    assert!(
        request(
            &host,
            &node,
            PublicRequest::RegisterReplyRoute {
                document: first.clone(),
                post: None
            },
            now
        )
        .await
        .is_err()
    );
    assert!(
        request(
            &host,
            &node,
            PublicRequest::CollectPrivate {
                access: access(&first, &account, vec![], now)
            },
            now
        )
        .await
        .is_err()
    );
    request(
        &host,
        &node,
        PublicRequest::CollectPrivate {
            access: access(&fresh, &account, vec![], now),
        },
        now,
    )
    .await
    .unwrap();
    assert!(request_auth::validate_route(&fresh, fresh.control.expires_at).is_err());
}

#[tokio::test]
async fn full_mailbox_reserves_acknowledgements_and_immediate_revocation() {
    let now = nucleus::execution::now().timestamp();
    let node = iroh::SecretKey::from_bytes(&[216; 32]).public().to_string();
    let host = host().await;
    host.social_command(
        nucleus::social::Command::ConfigureServices {
            settings: ServiceSettings {
                mailbox: true,
                cache_entries: 32,
                storage_bytes: 4 * 1024 * 1024,
                ..Default::default()
            },
        },
        None,
        nucleus::execution::now(),
    )
    .await
    .unwrap();
    let key = [217; 32];
    let recipient_account =
        session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
    let recipient_owner = Signer::from_bytes("", "pseudonym", [218; 32]);
    let recipient = certified(&recipient_owner, &recipient_account, 1, now);
    let sender_account = session::new_account(&key, vec![node.clone()], now + 30 * 86400).unwrap();
    let sender_owner = Signer::from_bytes("", "pseudonym", [219; 32]);
    let sender = certified(&sender_owner, &sender_account, 1, now);
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
    for _ in 0..64 {
        let empty = request(
            &host,
            &node,
            PublicRequest::CollectPrivate {
                access: access(&recipient, &recipient_account, vec![], now),
            },
            now,
        )
        .await
        .unwrap();
        assert!(empty["envelopes"].as_array().unwrap().is_empty());
    }
    let mut outbound = sender_account
        .account(&key)
        .unwrap()
        .create_outbound_session(
            SessionConfig::version_1(),
            vodozemac::Curve25519PublicKey::from_base64(&recipient.route.identity_key).unwrap(),
            vodozemac::Curve25519PublicKey::from_base64(&recipient.route.prekey).unwrap(),
        )
        .unwrap();
    let mut long = delivery(&sender, &sender_account, &recipient, &mut outbound, now);
    long.envelope.expires_at = now + 8 * 86400;
    long.envelope.id = request_auth::envelope_id(&long.envelope).unwrap();
    long.envelope.signature = sender_account
        .signing_key()
        .unwrap()
        .sign_bytes(&signing_bytes("private-envelope", &long.envelope).unwrap());
    assert!(
        request(
            &host,
            &node,
            PublicRequest::DeliverPrivate { document: long },
            now
        )
        .await
        .is_err()
    );
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
    let first = delivery(&sender, &sender_account, &recipient, &mut outbound, now);
    request(
        &host,
        &node,
        PublicRequest::DeliverPrivate {
            document: first.clone(),
        },
        now,
    )
    .await
    .unwrap();
    let mut full = false;
    for _ in 0..32 {
        let result = request(
            &host,
            &node,
            PublicRequest::DeliverPrivate {
                document: delivery(&sender, &sender_account, &recipient, &mut outbound, now),
            },
            now,
        )
        .await;
        if result.is_err() {
            full = true;
            break;
        }
    }
    assert!(full);
    let mut limited = false;
    for index in 0..1600 {
        let result = host
            .social_public_request(
                &format!("ordinary-flood-{index}"),
                &node,
                PublicRequest::DescribeService,
                now,
            )
            .await;
        if let Err(error) = result {
            assert!(error.to_string().contains("limited"), "{error}");
            limited = true;
            break;
        }
    }
    assert!(limited);
    let mut receipt = RecipientReceipt {
        envelope: first.envelope.id.clone(),
        envelope_hash: document_hash("private-envelope", &first.envelope).unwrap(),
        message: first.envelope.message.clone(),
        content_hash: first.envelope.content_hash.clone(),
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
                vec![first.envelope.id.clone()],
                now,
            ),
            receipts: vec![receipt.clone()],
        },
        now,
    )
    .await
    .unwrap();
    let status = request(
        &host,
        &node,
        PublicRequest::InspectPrivate { document: first },
        now,
    )
    .await
    .unwrap();
    assert_eq!(status["stage"], "recipient-durable");
    assert_eq!(status["receipt"]["signature"], receipt.signature);
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
    let collected = request(
        &host,
        &node,
        PublicRequest::CollectPrivate {
            access: access(&recipient, &recipient_account, vec![], now),
        },
        now,
    )
    .await
    .unwrap();
    assert!(collected["envelopes"].as_array().unwrap().is_empty());
}
