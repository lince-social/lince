use super::*;
use nucleus::social::requests::PrivateDelivery;

#[path = "late_delivery/attachments.rs"]
mod attachments;

#[path = "late_delivery/resend_races.rs"]
mod resend_races;

#[path = "late_delivery/discard_recovery.rs"]
mod discard_recovery;

#[path = "late_delivery/pending_keys.rs"]
mod pending_keys;

#[path = "late_delivery/permission_races.rs"]
mod permission_races;

#[path = "late_delivery/root_succession.rs"]
mod root_succession;

#[path = "late_delivery/faults.rs"]
mod faults;

#[path = "late_delivery/process_faults.rs"]
mod process_faults;

#[path = "late_delivery/consent_races.rs"]
mod consent_races;

#[path = "late_delivery/transport_faults.rs"]
mod transport_faults;

#[path = "late_delivery/preparation_faults.rs"]
mod preparation_faults;

struct Accepted {
    sender: Arc<Engine>,
    receiver: Arc<Engine>,
    hosts: Arc<Hosts>,
    services: Vec<String>,
    context: String,
    conversation: String,
    _directories: [tempfile::TempDir; 2],
    _host_directories: Vec<tempfile::TempDir>,
}

async fn accepted() -> Accepted {
    accepted_with_storage(false).await
}

async fn accepted_with_storage(disk: bool) -> Accepted {
    let mut nodes = BTreeMap::new();
    let mut host_directories = Vec::new();
    for secret in [235, 236] {
        let host = if disk {
            let (host, directory) = faults::person_on_disk(secret).await;
            host_directories.push(directory);
            host
        } else {
            Arc::new(Engine::open_memory().await.unwrap())
        };
        command(
            &host,
            Command::ConfigureServices {
                settings: ServiceSettings {
                    directory: true,
                    mailbox: true,
                    ..Default::default()
                },
            },
        )
        .await;
        nodes.insert(
            iroh::SecretKey::from_bytes(&[secret; 32])
                .public()
                .to_string(),
            host,
        );
    }
    let services: Vec<String> = nodes.keys().cloned().collect();
    let hosts = Arc::new(Hosts {
        nodes,
        offline: Mutex::new(BTreeSet::new()),
    });
    let network: Arc<dyn Network> = hosts.clone();
    let (receiver, receiver_dir) = if disk {
        faults::person_on_disk(237).await
    } else {
        person(237).await
    };
    let (sender, sender_dir) = if disk {
        faults::person_on_disk(238).await
    } else {
        person(238).await
    };
    receiver.attach_social_network(network.clone());
    sender.attach_social_network(network);
    let draft = command(
        &receiver,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Bicycle help".into(),
                destinations: services.clone(),
                ..Default::default()
            },
        },
    )
    .await;
    let context = draft["record"].as_str().unwrap().to_owned();
    command(
        &receiver,
        Command::PrepareReplyKeys {
            record: context.clone(),
            services: services.clone(),
        },
    )
    .await;
    let preview = command(
        &receiver,
        Command::Preview {
            record: context.clone(),
            state: PostState::Active,
        },
    )
    .await;
    let post: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    command(
        &receiver,
        Command::Publish {
            record: context.clone(),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: post.clone(),
        },
    )
    .await;
    publication(&receiver).await;
    command(
        &sender,
        Command::OpenRequest {
            post: Box::new(post),
            text: "Please help with my bicycle".into(),
            alias: "Neighbor".into(),
            services: services.clone(),
        },
    )
    .await;
    for _ in 0..2 {
        exchange(&sender, &receiver).await;
    }
    let requests = command(&receiver, Command::Requests { after: None }).await;
    let receiver_root = requests["requests"][0]["record"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &receiver,
        Command::DecideRequest {
            conversation: receiver_root,
            decision: RequestDecision::Accept,
        },
    )
    .await;
    for _ in 0..2 {
        exchange(&receiver, &sender).await;
    }
    receiver
        .social_reconcile_private_admissions()
        .await
        .unwrap();
    publication(&receiver).await;
    let requests = command(&sender, Command::Requests { after: None }).await;
    let mut diagnostics = Vec::new();
    for engine in [&sender, &receiver] {
        for table in [
            "social_message_work",
            "social_private_outbox",
            "social_private_destination",
            "social_pickup_work",
        ] {
            let errors: Vec<Option<String>> =
                store::sqlx::query_scalar(&format!("SELECT error FROM {table} LIMIT 8"))
                    .fetch_all(&engine.store.pool)
                    .await
                    .unwrap();
            diagnostics.push((table, errors));
        }
    }
    assert_eq!(
        requests["requests"][0]["state"]["state"], "accepted",
        "{diagnostics:?}"
    );
    Accepted {
        sender,
        receiver,
        hosts,
        services,
        context,
        conversation: requests["requests"][0]["record"].as_str().unwrap().into(),
        _directories: [receiver_dir, sender_dir],
        _host_directories: host_directories,
    }
}

#[tokio::test]
async fn deposited_chat_is_readable_after_sender_lease_expires_without_sender_renewal() {
    let execution = nucleus::execution::Execution::new([239; 32], 1_790_899_200_000).unwrap();
    execution
        .scope(Box::pin(async {
            let fixture = accepted().await;
            let saved = command(
                &fixture.sender,
                Command::SendPrivate {
                    conversation: fixture.conversation.clone(),
                    text: "I will be offline for ten days".into(),
                },
            )
            .await;
            let message = saved["message"].as_str().unwrap();
            due(&fixture.sender).await;
            fixture.sender.social_prepare_messages_once().await.unwrap();
            publication(&fixture.sender).await;
            for _ in 0..4 {
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
            let admitted = nucleus::execution::now().timestamp();
            assert!(document.envelope.expires_at > admitted + 10 * 86400);
            execution.set_time((admitted + 10 * 86400) * 1000).unwrap();
            assert!(
                document.authorization.certificate.expires_at
                    < nucleus::execution::now().timestamp()
            );
            assert!(
                engine::social::request_auth::validate_delivery(
                    &document,
                    nucleus::execution::now().timestamp()
                )
                .is_err()
            );
            for service in &fixture.services {
                assert!(
                    fixture.hosts.nodes[service]
                        .social_public_request(
                            "request-flow",
                            service,
                            PublicRequest::DeliverPrivate {
                                document: document.clone()
                            },
                            nucleus::execution::now().timestamp()
                        )
                        .await
                        .is_err()
                );
            }
            command(
                &fixture.receiver,
                Command::PrepareReplyKeys {
                    record: fixture.context.clone(),
                    services: fixture.services.clone(),
                },
            )
            .await;
            publication(&fixture.receiver).await;
            due(&fixture.receiver).await;
            assert_eq!(
                fixture
                    .receiver
                    .social_collect_private_once()
                    .await
                    .unwrap(),
                1,
                "{:?}",
                store::sqlx::query_as::<_, (String, Option<String>)>(
                    "SELECT context,error FROM social_pickup_work"
                )
                .fetch_all(&fixture.receiver.store.pool)
                .await
                .unwrap()
            );
            let requests = command(&fixture.receiver, Command::Requests { after: None }).await;
            assert_eq!(
                requests["requests"][0]["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|message| message["body"] == "I will be offline for ten days")
                    .count(),
                1
            );
            due(&fixture.receiver).await;
            assert_eq!(
                fixture
                    .receiver
                    .social_collect_private_once()
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(
                serde_json::from_str::<PrivateDelivery>(&body)
                    .unwrap()
                    .authorization
                    .certificate,
                document.authorization.certificate
            );
        }))
        .await;
}

#[tokio::test]
async fn unreadable_late_deposit_remains_visible_and_can_be_deliberately_discarded() {
    let execution = nucleus::execution::Execution::new([232; 32], 1_790_899_200_000).unwrap();
    execution
        .scope(Box::pin(async {
            let fixture = accepted().await;
            let saved = command(
                &fixture.sender,
                Command::SendPrivate {
                    conversation: fixture.conversation.clone(),
                    text: "Late unreadable ciphertext with retained history".into(),
                },
            )
            .await;
            let message = saved["message"].as_str().unwrap();
            due(&fixture.sender).await;
            fixture.sender.social_prepare_messages_once().await.unwrap();
            publication(&fixture.sender).await;
            for _ in 0..4 {
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
            assert_eq!(document.envelope.message_type, 1);
            store::sqlx::query("DELETE FROM social_device_state WHERE kind='session'")
                .execute(&fixture.receiver.store.pool)
                .await
                .unwrap();
            execution
                .set_time(execution.now().timestamp_millis() + 10 * 86400 * 1000)
                .unwrap();
            assert!(document.authorization.certificate.expires_at < execution.now().timestamp());
            command(
                &fixture.receiver,
                Command::PrepareReplyKeys {
                    record: fixture.context.clone(),
                    services: fixture.services.clone(),
                },
            )
            .await;
            publication(&fixture.receiver).await;
            due(&fixture.receiver).await;
            assert_eq!(
                fixture
                    .receiver
                    .social_collect_private_once()
                    .await
                    .unwrap(),
                0
            );
            let review = command(&fixture.receiver, Command::Requests { after: None }).await;
            let failures = review["receive_failures"].as_array().unwrap();
            assert!(!failures.is_empty());
            for failure in failures {
                assert_eq!(failure["envelope"], document.envelope.id);
                assert_eq!(failure["discard"], false);
                command(
                    &fixture.receiver,
                    Command::DiscardPrivate {
                        context: fixture.context.clone(),
                        service: failure["service"].as_str().unwrap().into(),
                        envelope: document.envelope.id.clone(),
                    },
                )
                .await;
            }
            due(&fixture.receiver).await;
            fixture
                .receiver
                .social_collect_private_once()
                .await
                .unwrap();
            let retained = command(&fixture.receiver, Command::Requests { after: None }).await;
            assert!(retained["receive_failures"].as_array().unwrap().is_empty());
            assert!(
                !retained["requests"][0]["messages"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            for host in fixture.hosts.nodes.values() {
                assert_eq!(
                    store::sqlx::query_scalar::<_, String>(
                        "SELECT stage FROM social_service_completed WHERE id=?"
                    )
                    .bind(&document.envelope.id)
                    .fetch_one(&host.store.pool)
                    .await
                    .unwrap(),
                    "recipient-refused"
                );
            }
        }))
        .await;
}

#[tokio::test]
async fn expired_resend_rolls_back_on_failure_and_preserves_logical_history_after_lost_receipt() {
    let execution = nucleus::execution::Execution::new([234; 32], 1_790_899_200_000).unwrap();
    execution.scope(Box::pin(async {
        let fixture = accepted().await;
        let saved = command(&fixture.sender, Command::SendPrivate {
            conversation: fixture.conversation.clone(), text: "One retained Message across delivery attempts".into(),
        }).await;
        let message = saved["message"].as_str().unwrap();
        let metadata = store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::MESSAGE_NAMESPACE).await.unwrap().unwrap();
        let facts: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE record_uid=?").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap();
        let events: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_message_event WHERE record_uid=?").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap();
        assert!(fixture.sender.act(Action::Social { request: Command::ResendExpiredPrivate { message: message.into() } },None).await.is_err());
        exchange(&fixture.sender,&fixture.receiver).await;
        let original: Vec<(String,String,String)> = store::sqlx::query_as("SELECT id,body,state FROM social_private_outbox WHERE record_uid=? ORDER BY id")
            .bind(message).fetch_all(&fixture.sender.store.pool).await.unwrap();
        assert!(!original.is_empty());
        let old: PrivateDelivery = serde_json::from_str(&original[0].1).unwrap();
        let receiver_message = command(&fixture.receiver,Command::Requests { after: None }).await["requests"][0]["messages"].as_array().unwrap().iter()
            .find(|message| message["body"] == "One retained Message across delivery attempts").unwrap()["uid"].as_str().unwrap().to_owned();
        let receiver_facts: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE record_uid=?").bind(&receiver_message).fetch_one(&fixture.receiver.store.pool).await.unwrap();
        execution.set_time(old.envelope.expires_at * 1000).unwrap();
        let view = command(&fixture.sender,Command::PrivateDeliveryStatus { message: message.into() }).await;
        assert_eq!(view["private_delivery"]["can_resend"],true);
        assert!(fixture.sender.act(Action::Social { request: Command::ResumePrivate { message: message.into() } },None).await.is_err());
        let status = store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::DELIVERY_NAMESPACE).await.unwrap().unwrap();
        let destinations: Vec<(String,String,String)> = store::sqlx::query_as("SELECT envelope,service,state FROM social_private_destination WHERE envelope IN (SELECT id FROM social_private_outbox WHERE record_uid=?) ORDER BY envelope,service")
            .bind(message).fetch_all(&fixture.sender.store.pool).await.unwrap();
        let trigger = format!("CREATE TRIGGER resend_failure BEFORE UPDATE ON record_extension WHEN NEW.record_uid='{message}' AND NEW.namespace='lince.social.delivery' BEGIN SELECT RAISE(ABORT,'injected resend rollback'); END");
        store::sqlx::query(&trigger).execute(&fixture.sender.store.pool).await.unwrap();
        assert!(fixture.sender.act(Action::Social { request: Command::ResendExpiredPrivate { message: message.into() } },None).await.is_err());
        let retained: Vec<(String,String,String)> = store::sqlx::query_as("SELECT id,body,state FROM social_private_outbox WHERE record_uid=? ORDER BY id")
            .bind(message).fetch_all(&fixture.sender.store.pool).await.unwrap();
        assert_eq!(retained,original);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::DELIVERY_NAMESPACE).await.unwrap().unwrap(),status);
        let unchanged: Vec<(String,String,String)> = store::sqlx::query_as("SELECT envelope,service,state FROM social_private_destination WHERE envelope IN (SELECT id FROM social_private_outbox WHERE record_uid=?) ORDER BY envelope,service")
            .bind(message).fetch_all(&fixture.sender.store.pool).await.unwrap();
        assert_eq!(unchanged,destinations);
        store::sqlx::query("DROP TRIGGER resend_failure").execute(&fixture.sender.store.pool).await.unwrap();
        command(&fixture.receiver,Command::PrepareReplyKeys { record: fixture.context.clone(), services: fixture.services.clone() }).await;
        publication(&fixture.receiver).await;
        let resent = command(&fixture.sender,Command::ResendExpiredPrivate { message: message.into() }).await;
        assert_eq!(resent["message"],message);
        assert_eq!(resent["expires_at"],old.envelope.expires_at+30*86400);
        assert!(fixture.sender.act(Action::Social { request: Command::ResendExpiredPrivate { message: message.into() } },None).await.is_err());
        due(&fixture.sender).await;
        fixture.sender.social_refresh_private_routes_once().await.unwrap();
        publication(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let fresh: Vec<(String,String)> = store::sqlx::query_as("SELECT id,body FROM social_private_outbox WHERE record_uid=? AND state='pending'").bind(message).fetch_all(&fixture.sender.store.pool).await.unwrap();
        assert!(!fresh.is_empty(),"{:?}",store::sqlx::query_as::<_, (String,Option<String>)>("SELECT record_uid,error FROM social_message_work").fetch_all(&fixture.sender.store.pool).await.unwrap());
        for (id,body) in &fresh {
            assert!(!original.iter().any(|(old,_,_)| old==id));
            let copy: PrivateDelivery = serde_json::from_str(body).unwrap();
            assert_eq!(copy.envelope.message,old.envelope.message);
            assert_eq!(copy.envelope.content_hash,old.envelope.content_hash);
            assert_eq!(copy.envelope.created_at,old.envelope.expires_at);
            assert!(copy.authorization.certificate.expires_at > nucleus::execution::now().timestamp());
        }
        for _ in 0..4 { fixture.sender.social_send_private_once().await.unwrap(); }
        due(&fixture.receiver).await;
        assert_eq!(fixture.receiver.social_collect_private_once().await.unwrap(),0);
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::MESSAGE_NAMESPACE).await.unwrap().unwrap(),metadata);
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM fact WHERE record_uid=?").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap(),facts);
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_message_event WHERE record_uid=?").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap(),events);
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM fact WHERE record_uid=?").bind(&receiver_message).fetch_one(&fixture.receiver.store.pool).await.unwrap(),receiver_facts);
        assert_eq!(store::records::quantity(&fixture.receiver.store.pool,&receiver_message).await.unwrap().unwrap().to_string(),"1");
        due(&fixture.sender).await;
        fixture.sender.social_send_private_once().await.unwrap();
        assert_eq!(store::records::get_extension(&fixture.sender.store.pool,message,nucleus::social::requests::DELIVERY_NAMESPACE).await.unwrap().unwrap()["stage"],"recipient-durable");
    })).await;
}
