use super::*;
use std::{path::Path, process::Stdio, time::Duration};

struct Child(std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn interrupt(directory: &Path, test: &str, mode: &str) {
    let output = directory.join("process.log");
    let log = std::fs::File::create(&output).unwrap();
    let mut child = Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env("LINCE_SOCIAL_PREPARATION_DIRECTORY", directory)
            .env("LINCE_SOCIAL_PREPARATION_MODE", mode)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    let marker = directory.join("boundary.reached");
    let ready = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if marker.exists() {
                return Ok(());
            }
            if let Some(status) = child.0.try_wait().unwrap() {
                return Err(status);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        matches!(ready, Ok(Ok(()))),
        "Boundary {mode} was not reached: {ready:?}: {}",
        std::fs::read_to_string(output).unwrap()
    );
    child.0.kill().unwrap();
    assert!(!child.0.wait().unwrap().success());
}

#[tokio::test]
async fn sender_process_boundaries_preserve_atomic_ciphertext() {
    if let Some(directory) = std::env::var_os("LINCE_SOCIAL_PREPARATION_DIRECTORY") {
        let directory = std::path::PathBuf::from(directory);
        let sender = faults::reopen(&directory).await;
        if std::env::var("LINCE_SOCIAL_PREPARATION_MODE").unwrap() == "committed" {
            due(&sender).await;
            assert_eq!(sender.social_prepare_messages_once().await.unwrap(), 1);
        }
        std::fs::write(directory.join("boundary.reached"), b"ready").unwrap();
        std::future::pending::<()>().await;
    }
    for mode in ["before-preparation", "committed"] {
        let mut fixture = accepted_with_storage(true).await;
        let text = format!("Sender process interruption at {mode}");
        let saved = command(
            &fixture.sender,
            Command::SendPrivate {
                conversation: fixture.conversation.clone(),
                text: text.clone(),
            },
        )
        .await;
        let message = saved["message"].as_str().unwrap().to_owned();
        let before = faults::protected_session_state(&fixture.sender).await;
        fixture.sender.store.pool.close().await;
        interrupt(fixture._directories[1].path(), "late_delivery::preparation_faults::sender_process_boundaries_preserve_atomic_ciphertext", mode).await;
        fixture.sender = faults::reopen(fixture._directories[1].path()).await;
        fixture.sender.attach_social_network(fixture.hosts.clone());
        let bodies: Vec<String> =
            store::sqlx::query_scalar("SELECT body FROM social_private_outbox WHERE record_uid=?")
                .bind(&message)
                .fetch_all(&fixture.sender.store.pool)
                .await
                .unwrap();
        if mode == "before-preparation" {
            assert!(bodies.is_empty());
            assert_eq!(
                faults::protected_session_state(&fixture.sender).await,
                before
            );
        } else {
            assert_eq!(bodies.len(), 1);
            assert_ne!(
                faults::protected_session_state(&fixture.sender).await,
                before
            );
        }
        for _ in 0..2 {
            exchange(&fixture.sender, &fixture.receiver).await;
        }
        transport_faults::assert_once(&fixture.receiver, &text).await;
        if !bodies.is_empty() {
            let retried: Vec<String> = store::sqlx::query_scalar(
                "SELECT body FROM social_private_outbox WHERE record_uid=?",
            )
            .bind(&message)
            .fetch_all(&fixture.sender.store.pool)
            .await
            .unwrap();
            assert_eq!(retried, bodies);
        }
        fixture.sender.store.pool.close().await;
        fixture.receiver.store.pool.close().await;
    }
}

#[tokio::test]
async fn failure_after_session_write_rolls_back_ratchet_outbox_and_destinations() {
    let mut fixture = accepted_with_storage(true).await;
    let text = "Atomic preparation after a real SQLite statement failure";
    let saved = command(
        &fixture.sender,
        Command::SendPrivate {
            conversation: fixture.conversation.clone(),
            text: text.into(),
        },
    )
    .await;
    let message = saved["message"].as_str().unwrap();
    let before = faults::protected_session_state(&fixture.sender).await;
    let events: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_message_event")
        .fetch_one(&fixture.sender.store.pool)
        .await
        .unwrap();
    let grants: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM replica_grant")
        .fetch_one(&fixture.sender.store.pool)
        .await
        .unwrap();
    store::sqlx::query("CREATE TRIGGER fail_prepared_outbox BEFORE INSERT ON social_private_outbox BEGIN SELECT RAISE(ABORT,'preparation statement failure'); END").execute(&fixture.sender.store.pool).await.unwrap();
    due(&fixture.sender).await;
    assert_eq!(
        fixture.sender.social_prepare_messages_once().await.unwrap(),
        0
    );
    assert_eq!(
        faults::protected_session_state(&fixture.sender).await,
        before
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_private_outbox WHERE record_uid=?"
        )
        .bind(message)
        .fetch_one(&fixture.sender.store.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_private_destination d LEFT JOIN social_private_outbox o ON o.id=d.envelope WHERE o.id IS NULL").fetch_one(&fixture.sender.store.pool).await.unwrap(), 0);
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_message_event")
            .fetch_one(&fixture.sender.store.pool)
            .await
            .unwrap(),
        events
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM replica_grant")
            .fetch_one(&fixture.sender.store.pool)
            .await
            .unwrap(),
        grants
    );
    fixture.sender.store.pool.close().await;
    fixture.sender = faults::reopen(fixture._directories[1].path()).await;
    fixture.sender.attach_social_network(fixture.hosts.clone());
    store::sqlx::query("DROP TRIGGER fail_prepared_outbox")
        .execute(&fixture.sender.store.pool)
        .await
        .unwrap();
    exchange(&fixture.sender, &fixture.receiver).await;
    transport_faults::assert_once(&fixture.receiver, text).await;
    fixture.sender.store.pool.close().await;
    fixture.receiver.store.pool.close().await;
}

#[tokio::test]
async fn host_process_kill_after_storage_before_response_keeps_exact_delivery() {
    if let Some(directory) = std::env::var_os("LINCE_SOCIAL_PREPARATION_DIRECTORY") {
        let directory = std::path::PathBuf::from(directory);
        let fixture: Value =
            serde_json::from_slice(&std::fs::read(directory.join("host-fixture.json")).unwrap())
                .unwrap();
        let host = faults::reopen(&directory).await;
        let node = fixture["service"].as_str().unwrap();
        host.social_public_request(
            "crash-sender",
            node,
            PublicRequest::DeliverPrivate {
                document: serde_json::from_value(fixture["document"].clone()).unwrap(),
            },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
        std::fs::write(
            directory.join("boundary.reached"),
            b"stored-before-response",
        )
        .unwrap();
        std::future::pending::<()>().await;
    }
    let mut fixture = accepted_with_storage(true).await;
    let text = "Host process stops after storage before returning its response";
    let saved = command(
        &fixture.sender,
        Command::SendPrivate {
            conversation: fixture.conversation.clone(),
            text: text.into(),
        },
    )
    .await;
    let message = saved["message"].as_str().unwrap();
    due(&fixture.sender).await;
    fixture.sender.social_prepare_messages_once().await.unwrap();
    let body: String = store::sqlx::query_scalar(
        "SELECT body FROM social_private_outbox WHERE record_uid=? LIMIT 1",
    )
    .bind(message)
    .fetch_one(&fixture.sender.store.pool)
    .await
    .unwrap();
    let document: PrivateDelivery = serde_json::from_str(&body).unwrap();
    let service = iroh::SecretKey::from_bytes(&[235; 32]).public().to_string();
    let directory = fixture._host_directories[0].path();
    std::fs::write(
        directory.join("host-fixture.json"),
        serde_json::to_vec(&serde_json::json!({"document":document,"service":service})).unwrap(),
    )
    .unwrap();
    fixture.hosts.nodes[&service].store.pool.close().await;
    interrupt(directory, "late_delivery::preparation_faults::host_process_kill_after_storage_before_response_keeps_exact_delivery", "stored").await;
    let host = faults::reopen(directory).await;
    let stored: String =
        store::sqlx::query_scalar("SELECT body FROM social_service_envelope WHERE id=?")
            .bind(&document.envelope.id)
            .fetch_one(&host.store.pool)
            .await
            .unwrap();
    assert_eq!(stored, body);
    let mut nodes = fixture.hosts.nodes.clone();
    nodes.insert(service, host);
    fixture.hosts = Arc::new(Hosts {
        nodes,
        offline: Mutex::new(BTreeSet::new()),
    });
    fixture.sender.attach_social_network(fixture.hosts.clone());
    fixture
        .receiver
        .attach_social_network(fixture.hosts.clone());
    for _ in 0..2 {
        exchange(&fixture.sender, &fixture.receiver).await;
    }
    transport_faults::assert_once(&fixture.receiver, text).await;
    fixture.sender.store.pool.close().await;
    fixture.receiver.store.pool.close().await;
}

#[tokio::test]
async fn reordered_chat_across_receiver_reopen_imports_each_message_once() {
    let mut fixture = accepted_with_storage(true).await;
    let mut documents = Vec::new();
    for index in 0..6 {
        let saved = command(
            &fixture.sender,
            Command::SendPrivate {
                conversation: fixture.conversation.clone(),
                text: format!("Reordered chat {index}"),
            },
        )
        .await;
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        let body: String = store::sqlx::query_scalar(
            "SELECT body FROM social_private_outbox WHERE record_uid=? LIMIT 1",
        )
        .bind(saved["message"].as_str().unwrap())
        .fetch_one(&fixture.sender.store.pool)
        .await
        .unwrap();
        documents.push(serde_json::from_str::<PrivateDelivery>(&body).unwrap());
    }
    for (position, index) in [5, 2, 0, 4, 1, 3].into_iter().enumerate() {
        fixture
            .receiver
            .social_receive_private(
                &fixture.context,
                &fixture.services[0],
                &documents[index],
                nucleus::execution::now().timestamp(),
            )
            .await
            .unwrap();
        if position == 1 {
            fixture.receiver.store.pool.close().await;
            fixture.receiver = faults::reopen(fixture._directories[0].path()).await;
            fixture
                .receiver
                .attach_social_network(fixture.hosts.clone());
        }
    }
    let state = faults::protected_session_state(&fixture.receiver).await;
    for (index, document) in documents.iter().enumerate() {
        let duplicate = fixture
            .receiver
            .social_receive_private(
                &fixture.context,
                &fixture.services[0],
                document,
                nucleus::execution::now().timestamp(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate["duplicate"], true);
        transport_faults::assert_once(&fixture.receiver, &format!("Reordered chat {index}")).await;
    }
    assert_eq!(
        faults::protected_session_state(&fixture.receiver).await,
        state
    );
    fixture.sender.store.pool.close().await;
    fixture.receiver.store.pool.close().await;
}

async fn announce(engine: &Engine, services: &[String], title: &str) -> Snippet {
    let saved = command(
        engine,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: title.into(),
                destinations: services.to_vec(),
                ..Default::default()
            },
        },
    )
    .await;
    let record = saved["record"].as_str().unwrap().to_owned();
    command(
        engine,
        Command::PrepareReplyKeys {
            record: record.clone(),
            services: services.to_vec(),
        },
    )
    .await;
    let preview = command(
        engine,
        Command::Preview {
            record: record.clone(),
            state: PostState::Active,
        },
    )
    .await;
    let document: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    command(
        engine,
        Command::Publish {
            record,
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document: document.clone(),
        },
    )
    .await;
    publication(engine).await;
    document
}

#[tokio::test]
async fn simultaneous_introductions_keep_separate_conversations_and_sessions() {
    let fixture = accepted_with_storage(true).await;
    let alice_post = announce(&fixture.sender, &fixture.services, "Alice can help").await;
    let bob_post = announce(&fixture.receiver, &fixture.services, "Bob needs help").await;
    tokio::join!(
        command(
            &fixture.sender,
            Command::OpenRequest {
                post: Box::new(bob_post),
                text: "Simultaneous Alice introduction".into(),
                alias: "Alice alias".into(),
                services: fixture.services.clone()
            }
        ),
        command(
            &fixture.receiver,
            Command::OpenRequest {
                post: Box::new(alice_post),
                text: "Simultaneous Bob introduction".into(),
                alias: "Bob alias".into(),
                services: fixture.services.clone()
            }
        )
    );
    for _ in 0..3 {
        tokio::join!(
            exchange(&fixture.sender, &fixture.receiver),
            exchange(&fixture.receiver, &fixture.sender)
        );
    }
    for person in [&fixture.sender, &fixture.receiver] {
        let requests = command(person, Command::Requests { after: None }).await;
        let rows = requests["requests"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        let tokens: BTreeSet<&str> = rows
            .iter()
            .map(|row| row["state"]["token"].as_str().unwrap())
            .collect();
        assert_eq!(tokens.len(), 3);
        for text in [
            "Simultaneous Alice introduction",
            "Simultaneous Bob introduction",
        ] {
            transport_faults::assert_once(person, text).await;
        }
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM replica_grant")
                .fetch_one(&person.store.pool)
                .await
                .unwrap(),
            0
        );
    }
    fixture.sender.store.pool.close().await;
    fixture.receiver.store.pool.close().await;
}
