use engine::{Engine, EngineError, social::Network};
use nucleus::social::{
    Command, PostDraft, PostState, PublicRequest, ServiceSettings, Snippet, reports::Report,
};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

async fn command(engine: &Engine, request: Command, actor: Option<&str>) -> Value {
    engine
        .social_command(request, actor, nucleus::execution::now())
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn preview(engine: &Engine, endpoint: &str) -> (String, Value) {
    let saved = command(
        engine,
        Command::SaveDraft {
            record: None,
            source: None,
            draft: PostDraft {
                title: "Public bicycle help".into(),
                text: "A public small contribution".into(),
                destinations: vec![endpoint.into()],
                ..Default::default()
            },
        },
        None,
    )
    .await;
    let record = saved["record"].as_str().unwrap().to_owned();
    let public = command(
        engine,
        Command::Preview {
            record: record.clone(),
            state: PostState::Active,
        },
        None,
    )
    .await;
    let document: Snippet = serde_json::from_value(public["document"].clone()).unwrap();
    command(
        engine,
        Command::Publish {
            record: record.clone(),
            preview_hash: public["preview_hash"].as_str().unwrap().into(),
            document: document.clone(),
        },
        None,
    )
    .await;
    let report = command(
        engine,
        Command::PreviewReport {
            post: document.id,
            service: endpoint.into(),
            explanation: "Please review this public claim".into(),
        },
        None,
    )
    .await;
    (record, report)
}

async fn enable(host: &Engine) {
    command(
        host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                directory: true,
                ..Default::default()
            },
        },
        None,
    )
    .await;
}

#[tokio::test]
async fn report_queue_is_bounded_and_expiry_is_visible_without_renewal_or_network_work() {
    let sender = Engine::open_memory().await.unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[166; 32]).public().to_string();
    let (_, preview) = preview(&sender, &endpoint).await;
    let report: Report = serde_json::from_value(preview["report_preview"].clone()).unwrap();
    for _ in 0..32 {
        let mut fresh = report.clone();
        fresh.id = nucleus::new_uid("report");
        let hash = engine::social::document_hash("public-report", &fresh).unwrap();
        command(
            &sender,
            Command::SendReport {
                document: Box::new(fresh),
                preview_hash: hash,
            },
            None,
        )
        .await;
    }
    let hash = engine::social::document_hash("public-report", &report).unwrap();
    assert!(
        sender
            .social_command(
                Command::SendReport {
                    document: Box::new(report.clone()),
                    preview_hash: hash
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    let clock = nucleus::execution::Execution::new([164; 32], report.expires_at * 1000).unwrap();
    clock
        .scope(async {
            assert_eq!(sender.social_send_reports_once().await.unwrap(), 0);
            let expired = command(&sender, Command::Reports, None).await;
            assert_eq!(expired["reports"].as_array().unwrap().len(), 32);
            assert!(
                expired["reports"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row["state"] == "expired")
            );
            clock.set_time((report.expires_at + 86400) * 1000).unwrap();
            assert!(
                command(&sender, Command::Reports, None).await["reports"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        })
        .await;
}

struct ReportNetwork {
    host: Arc<Engine>,
    endpoint: String,
    source: String,
    calls: Arc<Mutex<Vec<String>>>,
    lose: AtomicBool,
}

#[async_trait::async_trait]
impl Network for ReportNetwork {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        assert_eq!(destination, self.endpoint);
        self.calls
            .lock()
            .unwrap()
            .push(serde_json::to_string(&request).unwrap());
        let response = self
            .host
            .social_public_request(
                &self.source,
                &self.endpoint,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await?;
        if self.lose.swap(false, Ordering::SeqCst) {
            return Err(EngineError::Consequence(
                "Lost report receipt after commit".into(),
            ));
        }
        Ok(response)
    }
}

#[tokio::test]
async fn deliberate_reports_retry_exact_public_evidence_after_restart_without_private_identity() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("reporter.sqlite").display()
    );
    let sender = Engine::open(&url).await.unwrap();
    sender.set_sealing_keyring_path(directory.path().join("keys/sealing.json"));
    let host = Arc::new(Engine::open_memory().await.unwrap());
    enable(&host).await;
    let endpoint = iroh::SecretKey::from_bytes(&[168; 32]).public().to_string();
    let source = iroh::SecretKey::from_bytes(&[169; 32]).public().to_string();
    let (record, preview) = preview(&sender, &endpoint).await;
    let report: Report = serde_json::from_value(preview["report_preview"].clone()).unwrap();
    let organ = store::organs::local(&sender.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let network = Arc::new(ReportNetwork {
        host: host.clone(),
        endpoint,
        source,
        calls: calls.clone(),
        lose: AtomicBool::new(true),
    });
    sender.attach_social_network(network.clone());
    assert!(calls.lock().unwrap().is_empty());
    let mut altered = report.clone();
    altered.explanation = "Changed after preview".into();
    assert!(
        sender
            .social_command(
                Command::SendReport {
                    document: Box::new(altered),
                    preview_hash: preview["preview_hash"].as_str().unwrap().into()
                },
                None,
                nucleus::execution::now()
            )
            .await
            .is_err()
    );
    command(
        &sender,
        Command::SendReport {
            document: Box::new(report.clone()),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
        },
        None,
    )
    .await;
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(sender.social_send_reports_once().await.unwrap(), 0);
    assert_eq!(
        command(&sender, Command::Reports, None).await["reports"][0]["state"],
        "pending"
    );
    let retained = command(&host, Command::ReceivedReports { after: None }, None).await;
    assert_eq!(retained["received_reports"].as_array().unwrap().len(), 1);
    drop(sender);
    let sender = Engine::open(&url).await.unwrap();
    sender.set_sealing_keyring_path(directory.path().join("keys/sealing.json"));
    sender.attach_social_network(network.clone());
    store::sqlx::query("UPDATE social_report_work SET next_attempt=0")
        .execute(&sender.store.pool)
        .await
        .unwrap();
    assert_eq!(sender.social_send_reports_once().await.unwrap(), 1);
    let sent = calls.lock().unwrap().clone();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], sent[1]);
    assert!(!sent[0].contains(&record));
    assert!(!sent[0].contains(&organ));
    assert!(report.document.profile.is_none());
    assert_eq!(
        command(&host, Command::ReceivedReports { after: None }, None).await["received_reports"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        command(&sender, Command::Reports, None).await["reports"][0]["state"],
        "accepted"
    );
    command(&sender, Command::ClearReports, None).await;
    assert_eq!(sender.social_send_reports_once().await.unwrap(), 0);
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert_eq!(
        command(&host, Command::ReceivedReports { after: None }, None).await["received_reports"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn report_daily_limits_and_dismissal_dedup_survive_reopen_and_new_transport_identities() {
    let sender = Engine::open_memory().await.unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[170; 32]).public().to_string();
    let (_, preview) = preview(&sender, &endpoint).await;
    let template: Report = serde_json::from_value(preview["report_preview"].clone()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("operator.sqlite").display()
    );
    let host = Engine::open(&url).await.unwrap();
    enable(&host).await;
    let source = iroh::SecretKey::from_bytes(&[171; 32]).public().to_string();
    let now = nucleus::execution::now().timestamp();
    let deliver = |report: Report| PublicRequest::SubmitReport {
        document: Box::new(report),
    };
    assert_eq!(
        host.social_public_request(&source, &endpoint, deliver(template.clone()), now)
            .await
            .unwrap()["accepted"],
        true
    );
    command(
        &host,
        Command::DismissReport {
            id: template.id.clone(),
        },
        None,
    )
    .await;
    drop(host);
    let host = Engine::open(&url).await.unwrap();
    assert_eq!(
        host.social_public_request(&source, &endpoint, deliver(template.clone()), now)
            .await
            .unwrap()["accepted"],
        true
    );
    assert!(
        command(&host, Command::ReceivedReports { after: None }, None).await["received_reports"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut conflict = template.clone();
    conflict.explanation = "Conflicting ID reuse".into();
    assert!(
        host.social_public_request(&source, &endpoint, deliver(conflict), now)
            .await
            .is_err()
    );
    for _ in 0..7 {
        let mut report = template.clone();
        report.id = nucleus::new_uid("report");
        assert_eq!(
            host.social_public_request(&source, &endpoint, deliver(report), now)
                .await
                .unwrap()["accepted"],
            true
        );
    }
    let mut ninth = template.clone();
    ninth.id = nucleus::new_uid("report");
    assert_eq!(
        host.social_public_request(&source, &endpoint, deliver(ninth.clone()), now)
            .await
            .unwrap()["accepted"],
        false
    );
    for seed in 1..32 {
        let source = iroh::SecretKey::from_bytes(&[seed; 32])
            .public()
            .to_string();
        for _ in 0..8 {
            let mut report = template.clone();
            report.id = nucleus::new_uid("report");
            assert_eq!(
                host.social_public_request(&source, &endpoint, deliver(report), now)
                    .await
                    .unwrap()["accepted"],
                true
            );
        }
    }
    let another = iroh::SecretKey::from_bytes(&[32; 32]).public().to_string();
    assert_eq!(
        host.social_public_request(&another, &endpoint, deliver(ninth.clone()), now)
            .await
            .unwrap()["accepted"],
        false
    );
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_received_report")
        .fetch_one(&host.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 255);
    assert_eq!(
        host.social_public_request(&source, &endpoint, deliver(ninth.clone()), now + 86400)
            .await
            .unwrap()["accepted"],
        true
    );
    assert!(
        host.social_public_request(&source, &endpoint, deliver(ninth), template.expires_at)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn report_work_is_actor_private_and_disabled_hosts_reject_invalid_or_unselected_intake() {
    let sender = Engine::open_memory().await.unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[172; 32]).public().to_string();
    let source = iroh::SecretKey::from_bytes(&[173; 32]).public().to_string();
    let (_, preview) = preview(&sender, &endpoint).await;
    let report: Report = serde_json::from_value(preview["report_preview"].clone()).unwrap();
    let host = Engine::open_memory().await.unwrap();
    let now = nucleus::execution::now().timestamp();
    let deliver = |report: Report| PublicRequest::SubmitReport {
        document: Box::new(report),
    };
    assert!(
        host.social_public_request(&source, &endpoint, deliver(report.clone()), now)
            .await
            .is_err()
    );
    command(
        &host,
        Command::ConfigureServices {
            settings: ServiceSettings {
                mailbox: true,
                ..Default::default()
            },
        },
        None,
    )
    .await;
    assert!(
        host.social_public_request(&source, &endpoint, deliver(report.clone()), now)
            .await
            .is_err()
    );
    enable(&host).await;
    let mut wrong = report.clone();
    wrong.service = source.clone();
    assert!(
        host.social_public_request(&source, &endpoint, deliver(wrong), now)
            .await
            .is_err()
    );
    let mut forged = report.clone();
    forged.document.title = "Unsigned alteration".into();
    assert!(
        host.social_public_request(&source, &endpoint, deliver(forged), now)
            .await
            .is_err()
    );
    let mut oversized = report.clone();
    oversized.explanation = "x".repeat(2001);
    assert!(
        host.social_public_request(&source, &endpoint, deliver(oversized), now)
            .await
            .is_err()
    );
    let mut future = report.clone();
    future.created_at = now + 301;
    future.expires_at = future.created_at + 7 * 86400;
    assert!(
        host.social_public_request(&source, &endpoint, deliver(future), now)
            .await
            .is_err()
    );
    let role = store::auth::ensure_role(&sender.store.pool, "reporter")
        .await
        .unwrap();
    for (action, subject) in [("organ", "update"), ("view", "stream")] {
        let permission = store::auth::ensure_permission(&sender.store.pool, action, subject)
            .await
            .unwrap();
        store::auth::grant(&sender.store.pool, role, permission)
            .await
            .unwrap();
    }
    let actor =
        store::auth::create_person_login(&sender.store.pool, "Reporter", "reporter", "hash", role)
            .await
            .unwrap();
    command(
        &sender,
        Command::SendReport {
            document: Box::new(report),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
        },
        Some(&actor),
    )
    .await;
    assert_eq!(
        command(&sender, Command::Reports, Some(&actor)).await["reports"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        command(&sender, Command::Reports, None).await["reports"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    command(&sender, Command::ClearReports, None).await;
    assert_eq!(
        command(&sender, Command::Reports, Some(&actor)).await["reports"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    command(&sender, Command::ClearReports, Some(&actor)).await;
    assert!(
        command(&sender, Command::Reports, Some(&actor)).await["reports"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let reader = store::auth::ensure_role(&sender.store.pool, "reader")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&sender.store.pool, "view", "stream")
        .await
        .unwrap();
    store::auth::grant(&sender.store.pool, reader, permission)
        .await
        .unwrap();
    let viewer =
        store::auth::create_person_login(&sender.store.pool, "Reader", "reader", "hash", reader)
            .await
            .unwrap();
    for command in [
        Command::Reports,
        Command::ReceivedReports { after: None },
        Command::ClearReports,
    ] {
        assert!(
            sender
                .social_command(command, Some(&viewer), nucleus::execution::now())
                .await
                .is_err()
        );
    }
    assert_eq!(sender.social_send_reports_once().await.unwrap(), 0);
    assert_eq!(
        command(&host, Command::ReceivedReports { after: None }, None).await["received_reports"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
