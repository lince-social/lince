use super::*;
use cell::FioteService;
use cell::speech::Service;
use std::sync::Arc;

async fn fixture() -> (App, Entity, Arc<cell::speech::Host>, tempfile::TempDir) {
    let (mut app, owner, _fiote, _script, directory, _record) =
        super::super::tests::fixture().await;
    let host = Arc::new(cell::speech::Host::open(directory.path().join("speech.json")).unwrap());
    let mut runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    runtime.speech = Some(host.clone());
    app.world_mut()
        .insert_non_send(crate::native::cell_bridge::connect(
            runtime,
            crate::native::wake::WakeSignal::new(|| {}),
        ));
    (app, owner, host, directory)
}

fn click(world: &mut World, caption: &str) {
    let button = world
        .query::<(
            &crate::native::actions::ActionButton,
            &bevy::a11y::AccessibilityNode,
        )>()
        .iter(world)
        .find(|(_, node)| node.label() == Some(caption))
        .map(|(button, _)| button.clone())
        .unwrap_or_else(|| panic!("Missing {caption}"));
    button.actions.run(world, button.target);
}

async fn settle(app: &mut App) {
    for _ in 0..1000 {
        app.update();
        let entry_pending = app
            .world_mut()
            .query::<&super::super::Entry>()
            .iter(app.world())
            .any(|entry| entry.pending.is_some());
        if !entry_pending
            && !app
                .world_mut()
                .query::<&Settings>()
                .iter(app.world())
                .any(|settings| settings.pending.is_some() || settings.job.is_some())
        {
            return;
        }
        let incoming = app
            .world_mut()
            .non_send_mut::<crate::native::cell_bridge::CellBridge>()
            .incoming
            .try_recv();
        if let Ok(message) = incoming {
            app.world_mut()
                .write_message(crate::native::cell_bridge::CellMessage(message));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("Model controls did not settle");
}

async fn finished(host: &cell::fiote::Host, request_id: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let status = host
                .handle(FioteRequest::AssistantInspect {
                    request_id: request_id.into(),
                })
                .await
                .unwrap();
            if status
                .assistant_request
                .is_some_and(|receipt| receipt.state == "completed")
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

fn intercept(
    app: &mut App,
) -> (
    tokio::sync::mpsc::Receiver<cell::ClientMessage>,
    tokio::sync::mpsc::Sender<cell::ClientMessage>,
) {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let mut bridge = app
        .world_mut()
        .non_send_mut::<crate::native::cell_bridge::CellBridge>();
    let original = std::mem::replace(&mut bridge.outgoing, sender.clone());
    bridge.speech_cancellation = crate::native::cell_bridge::SpeechCancellation::new(sender);
    (receiver, original)
}

#[tokio::test]
async fn refused_download_enqueue_leaves_settings_retryable_without_a_phantom_job() {
    let (mut app, owner, _host, _directory) = fixture().await;
    let (mut requests, _original) = intercept(&mut app);
    app.world()
        .get_non_send::<crate::native::cell_bridge::CellBridge>()
        .unwrap()
        .outgoing
        .try_send(cell::ClientMessage::Speech {
            id: "occupied".into(),
            request: Request::Inspect,
        })
        .unwrap();
    click(app.world_mut(), "Download selected decision model");
    let settings = app.world().get::<Settings>(owner).unwrap();
    assert!(settings.job.is_none());
    assert!(settings.pending.is_none());
    assert!(
        matches!(requests.recv().await.unwrap(), cell::ClientMessage::Speech { id, .. } if id == "occupied")
    );
    click(app.world_mut(), "Download selected decision model");
    let settings = app.world().get::<Settings>(owner).unwrap();
    assert!(settings.job.is_some());
    assert!(settings.pending.is_some());
    assert!(matches!(
        requests.recv().await.unwrap(),
        cell::ClientMessage::Speech {
            request: Request::InstallDecision { source: None, .. },
            ..
        }
    ));
}

#[tokio::test]
async fn native_download_cancel_survives_queue_pressure_and_ignores_a_late_setup_acknowledgment() {
    let (mut app, owner, host, _directory) = fixture().await;
    let runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    let input = app.world().get::<super::super::Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep my setup draft");
    let (mut requests, _original) = intercept(&mut app);
    click(app.world_mut(), "Download selected decision model");
    let cell::ClientMessage::Speech {
        id: start,
        request: Request::InstallDecision { job, source: None },
    } = requests.recv().await.unwrap()
    else {
        panic!("Download did not use the normal setup request")
    };
    click(app.world_mut(), "Qwen local classifier");
    assert!(requests.try_recv().is_err());
    app.world()
        .get_non_send::<crate::native::cell_bridge::CellBridge>()
        .unwrap()
        .outgoing
        .try_send(cell::ClientMessage::Speech {
            id: "occupied".into(),
            request: Request::Inspect,
        })
        .unwrap();
    click(app.world_mut(), "Cancel model operation");
    let pending = app
        .world()
        .get::<Settings>(owner)
        .unwrap()
        .pending
        .clone()
        .unwrap();
    assert!(app.world().get::<Settings>(owner).unwrap().job.is_none());
    let mut status = host.handle(Request::Inspect).await.unwrap();
    status.job = Some(cell::speech::Job {
        purpose: Purpose::DecisionSetup,
        id: job.clone(),
        pending: false,
        text: Some("Late setup output must not become a Message".into()),
        decision: None,
        error: None,
    });
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech { id: start, status },
        ));
    app.update();
    assert_eq!(
        app.world().get::<Settings>(owner).unwrap().pending.as_ref(),
        Some(&pending)
    );
    assert!(
        matches!(requests.recv().await.unwrap(), cell::ClientMessage::Speech { id, .. } if id == "occupied")
    );
    let request = tokio::time::timeout(Duration::from_secs(2), requests.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(request, cell::ClientMessage::Speech { id, request: Request::Cancel { job: canceled } } if id == pending && canceled == job)
    );
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech {
                id: pending,
                status: host.handle(Request::Inspect).await.unwrap(),
            },
        ));
    app.update();
    assert!(
        app.world()
            .get::<Settings>(owner)
            .unwrap()
            .pending
            .is_none()
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep my setup draft"
    );
    assert!(
        app.world()
            .get::<super::super::Entry>(owner)
            .unwrap()
            .receipt
            .is_none()
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    click(app.world_mut(), "Qwen local classifier");
    assert!(matches!(
        requests.recv().await.unwrap(),
        cell::ClientMessage::Speech {
            request: Request::SelectDecision {
                model: ModelId::Qwen
            },
            ..
        }
    ));
}

#[tokio::test]
async fn corrupt_native_model_import_keeps_manual_send_usable_and_never_activates_the_draft() {
    let (mut app, owner, host, directory) = fixture().await;
    let runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    let input = app.world().get::<super::super::Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Send this only when I ask");
    click(app.world_mut(), "Small English classifier");
    settle(&mut app).await;
    let source = directory.path().join("corrupt-assets");
    std::fs::create_dir(&source).unwrap();
    std::fs::File::create(source.join("model.safetensors"))
        .unwrap()
        .set_len(25_513_196)
        .unwrap();
    let field = app.world().get::<Settings>(owner).unwrap().import;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(source.to_str().unwrap());
    click(app.world_mut(), "Import selected decision model");
    settle(&mut app).await;
    let label = app.world().get::<Settings>(owner).unwrap().label;
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("checksum")
    );
    assert!(
        !host
            .handle(Request::Inspect)
            .await
            .unwrap()
            .decision_model
            .unwrap()
            .present
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Send this only when I ask"
    );
    assert!(
        app.world()
            .get::<super::super::Entry>(owner)
            .unwrap()
            .receipt
            .is_none()
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    click(app.world_mut(), "Native assistant");
    click(app.world_mut(), "Send assistant request");
    settle(&mut app).await;
    assert!(
        app.world()
            .get::<super::super::Entry>(owner)
            .unwrap()
            .receipt
            .is_some(),
        "{}",
        app.world()
            .get::<Text>(
                app.world()
                    .get::<super::super::Entry>(owner)
                    .unwrap()
                    .status
            )
            .unwrap()
            .0
    );
    let request_id = app
        .world()
        .get::<super::super::Entry>(owner)
        .unwrap()
        .receipt
        .as_ref()
        .unwrap()
        .request_id
        .clone();
    finished(runtime.fiote.as_ref().unwrap(), &request_id).await;
}

#[tokio::test]
async fn supported_unvalidated_routing_persists_and_identical_retry_bypasses_changed_model() {
    let (mut app, owner, speech, directory) = fixture().await;
    let runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    let host = runtime.fiote.clone().unwrap();
    let panel = app.world().get::<super::super::Entry>(owner).unwrap().panel;
    let record = app
        .world()
        .get::<super::super::Panel>(panel)
        .unwrap()
        .binding
        .uid
        .clone();
    let settings = host
        .handle(FioteRequest::Inspect { record })
        .await
        .unwrap()
        .settings;
    let child = host
        .handle(FioteRequest::Create {
            head: "Chosen recipient".into(),
        })
        .await
        .unwrap()
        .record;
    host.handle(FioteRequest::Configure {
        record: child.clone(),
        settings,
        api_key: None,
        password: None,
    })
    .await
    .unwrap();
    super::super::Command::Directory(0).apply(app.world_mut(), owner);
    settle(&mut app).await;
    let input = app.world().get::<super::super::Entry>(owner).unwrap().input;
    let body = "One routed request, retried with a different local model";
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(body);
    let path = directory.path().join("request.json");
    app.world_mut()
        .get_mut::<super::super::Entry>(owner)
        .unwrap()
        .path = Some(path.clone());
    click(app.world_mut(), "Toggle local destination routing");
    let mut status = speech.handle(Request::Inspect).await.unwrap();
    let capabilities = status.decision_capabilities.as_mut().unwrap();
    capabilities.routing_supported = true;
    capabilities.routing_validated = false;
    capabilities.admission_validated = false;
    status.decision_model.as_mut().unwrap().present = true;
    app.world_mut().get_mut::<Settings>(owner).unwrap().saved = Some(status.clone());
    click(app.world_mut(), "Send assistant request");
    let settings = app.world().get::<Settings>(owner).unwrap();
    let routing = settings.routing.as_ref().unwrap();
    let request_id = routing.request.request_id.clone();
    let pending = settings.pending.clone().unwrap();
    status.job = Some(cell::speech::Job {
        purpose: Purpose::Decision,
        id: settings.job.clone().unwrap(),
        pending: false,
        text: None,
        error: None,
        decision: Some(cell::fiote_communication::decision::Output {
            request_id: request_id.clone(),
            generation: 1,
            model: routing.model.clone(),
            revision: routing.revision.clone(),
            admission: None,
            destination: Some(cell::fiote_communication::decision::Choice {
                value: Some(child.clone()),
                probability: None,
                margin: None,
                reason: "Controlled native lifecycle fixture".into(),
            }),
            cold_load_ms: 0,
            inference_ms: 0,
        }),
    });
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech {
                id: pending,
                status,
            },
        ));
    settle(&mut app).await;
    let receipt = app
        .world()
        .get::<super::super::Entry>(owner)
        .unwrap()
        .receipt
        .clone()
        .unwrap();
    assert_eq!(receipt.target, child);
    finished(&host, &request_id).await;
    let saved: super::super::Saved = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(saved.resolved);
    assert_eq!(saved.destination.as_deref(), Some(child.as_str()));
    assert!(saved.target.is_none());
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    click(app.world_mut(), "Small English classifier");
    settle(&mut app).await;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(body);
    click(app.world_mut(), "Send assistant request");
    assert!(
        app.world()
            .get::<Settings>(owner)
            .unwrap()
            .routing
            .is_none()
    );
    settle(&mut app).await;
    let retried = app
        .world()
        .get::<super::super::Entry>(owner)
        .unwrap()
        .receipt
        .as_ref()
        .unwrap();
    assert_eq!(retried.request_id, request_id);
    assert_eq!(retried.target, child);
    assert_eq!(retried.message, receipt.message);
    assert_eq!(retried.thread, receipt.thread);
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn failed_destination_routing_falls_back_once_and_ignores_a_late_decision() {
    let (mut app, owner, speech, _directory) = fixture().await;
    let runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    let host = runtime.fiote.clone().unwrap();
    let panel = app.world().get::<super::super::Entry>(owner).unwrap().panel;
    let record = app
        .world()
        .get::<super::super::Panel>(panel)
        .unwrap()
        .binding
        .uid
        .clone();
    for index in 0..5 {
        let child = runtime
            .engine
            .act(
                engine::actions::Action::CreateAgent {
                    head: format!("Routing child {index}"),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        host.handle(FioteRequest::Behavior {
            record: child,
            prompt_parent: Some(record.clone()),
            run_assigned: false,
        })
        .await
        .unwrap();
    }
    super::super::Command::Directory(0).apply(app.world_mut(), owner);
    settle(&mut app).await;
    let input = app.world().get::<super::super::Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Please answer through the ranked default");
    click(app.world_mut(), "Toggle local destination routing");
    let mut status = speech.handle(Request::Inspect).await.unwrap();
    status
        .decision_capabilities
        .as_mut()
        .unwrap()
        .routing_validated = true;
    status.decision_model.as_mut().unwrap().present = true;
    app.world_mut().get_mut::<Settings>(owner).unwrap().saved = Some(status);
    click(app.world_mut(), "Send assistant request");
    let pending = app
        .world()
        .get::<Settings>(owner)
        .unwrap()
        .pending
        .clone()
        .unwrap();
    assert!(
        app.world()
            .get::<Settings>(owner)
            .unwrap()
            .routing
            .is_some()
    );
    click(app.world_mut(), "Send assistant request");
    settle(&mut app).await;
    let receipt = app
        .world()
        .get::<super::super::Entry>(owner)
        .unwrap()
        .receipt
        .clone()
        .unwrap();
    assert_eq!(receipt.target, record);
    assert!(receipt.message.is_some());
    finished(&host, &receipt.request_id).await;
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    let status = speech.handle(Request::Inspect).await.unwrap();
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech {
                id: pending,
                status,
            },
        ));
    app.update();
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn native_model_selection_uses_the_service_and_shows_unaccepted_capabilities() {
    let (mut app, owner, host, directory) = fixture().await;
    click(app.world_mut(), "Local routing model settings");
    settle(&mut app).await;
    click(app.world_mut(), "Small English classifier");
    settle(&mut app).await;
    let status = host.handle(Request::Inspect).await.unwrap();
    assert_eq!(status.decision_selection, ModelId::EnglishNli);
    let settings = app.world().get::<Settings>(owner).unwrap();
    let caption = &app.world().get::<Text>(settings.label).unwrap().0;
    assert!(caption.contains("english-nli"));
    assert!(caption.contains("general listening unavailable"));
    assert!(caption.contains("needs installation"));
    assert!(!directory.path().join("models").exists());
    assert!(!status.decision_capabilities.unwrap().admission_validated);
}

#[tokio::test]
async fn changing_recipient_cancels_pending_routing_without_delivery() {
    let (mut app, owner, speech, _directory) = fixture().await;
    let runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    let input = app.world().get::<super::super::Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep the cancelled request");
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    click(app.world_mut(), "Toggle local destination routing");
    let mut status = speech.handle(Request::Inspect).await.unwrap();
    status
        .decision_capabilities
        .as_mut()
        .unwrap()
        .routing_validated = true;
    status.decision_model.as_mut().unwrap().present = true;
    app.world_mut().get_mut::<Settings>(owner).unwrap().saved = Some(status.clone());
    click(app.world_mut(), "Send assistant request");
    let pending = app
        .world()
        .get::<Settings>(owner)
        .unwrap()
        .pending
        .clone()
        .unwrap();
    click(app.world_mut(), "Native assistant");
    assert!(
        app.world()
            .get::<Settings>(owner)
            .unwrap()
            .routing
            .is_none()
    );
    settle(&mut app).await;
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech {
                id: pending,
                status,
            },
        ));
    app.update();
    assert!(
        app.world()
            .get::<super::super::Entry>(owner)
            .unwrap()
            .receipt
            .is_none()
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep the cancelled request"
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn model_setup_failure_never_inserts_text_or_sends_the_assistant_draft() {
    let (mut app, owner, host, directory) = fixture().await;
    let input = app.world().get::<super::super::Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep the person's draft");
    click(app.world_mut(), "Small English classifier");
    settle(&mut app).await;
    let field = app.world().get::<Settings>(owner).unwrap().import;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(directory.path().join("missing-assets").to_str().unwrap());
    click(app.world_mut(), "Import selected decision model");
    settle(&mut app).await;
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep the person's draft"
    );
    assert!(
        app.world()
            .get::<super::super::Entry>(owner)
            .unwrap()
            .receipt
            .is_none()
    );
    assert!(
        !host
            .handle(Request::Inspect)
            .await
            .unwrap()
            .decision_model
            .unwrap()
            .present
    );
}

#[tokio::test]
#[ignore = "Requires LINCE_DECISION_TEST_CACHE with the verified English NLI model"]
async fn real_native_model_import_is_offline_lazy_and_preserves_the_unsent_draft() {
    let (mut app, owner, host, _directory) = fixture().await;
    let input = app.world().get::<super::super::Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep the unsent request");
    click(app.world_mut(), "Small English classifier");
    settle(&mut app).await;
    let cache = std::path::PathBuf::from(std::env::var_os("LINCE_DECISION_TEST_CACHE").unwrap());
    let source = cache.join(format!(
        "english-nli-{}",
        cell::fiote_communication::decision::nli::REVISION
    ));
    let field = app.world().get::<Settings>(owner).unwrap().import;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(source.to_str().unwrap());
    click(app.world_mut(), "Import selected decision model");
    settle(&mut app).await;
    let status = host.handle(Request::Inspect).await.unwrap();
    let model = status.decision_model.as_ref().unwrap();
    assert!(model.present);
    assert!(!model.loaded);
    assert!(!model.busy);
    assert!(!status.decision_capabilities.unwrap().admission_validated);
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep the unsent request"
    );
    assert!(
        app.world()
            .get::<super::super::Entry>(owner)
            .unwrap()
            .receipt
            .is_none()
    );
    assert!(
        app.world()
            .get::<Text>(app.world().get::<Settings>(owner).unwrap().label)
            .unwrap()
            .0
            .contains("installed offline")
    );
}

#[tokio::test]
#[ignore = "Explicitly downloads the pinned small English classifier through native controls"]
async fn actual_native_download_and_invalid_reimport_preserve_verified_assets_without_loading_or_sending()
 {
    let (mut app, owner, host, directory) = fixture().await;
    let runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    let input = app.world().get::<super::super::Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep the real download draft");
    click(app.world_mut(), "Small English classifier");
    settle(&mut app).await;
    click(app.world_mut(), "Download selected decision model");
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            app.update();
            while let Ok(message) = app
                .world_mut()
                .non_send_mut::<crate::native::cell_bridge::CellBridge>()
                .incoming
                .try_recv()
            {
                app.world_mut()
                    .write_message(crate::native::cell_bridge::CellMessage(message));
            }
            let settings = app.world().get::<Settings>(owner).unwrap();
            if settings.pending.is_none() && settings.job.is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let status = host.handle(Request::Inspect).await.unwrap();
    let model = status.decision_model.as_ref().unwrap();
    assert!(
        model.present,
        "{}",
        app.world()
            .get::<Text>(app.world().get::<Settings>(owner).unwrap().label)
            .unwrap()
            .0
    );
    assert!(!model.loaded);
    assert!(!model.busy);
    assert!(
        !status
            .decision_capabilities
            .as_ref()
            .unwrap()
            .admission_validated
    );
    assert!(
        !status
            .decision_capabilities
            .as_ref()
            .unwrap()
            .routing_validated
    );
    let weights = directory
        .path()
        .join("models")
        .join(format!("{}-{}", model.model, model.revision))
        .join("model.safetensors");
    let hash = cell::fiote_communication::models::digest(&std::fs::read(&weights).unwrap());
    assert_eq!(
        hash,
        "bf2d83426aa6487f5cc3dc5c71d1566f33a65d883ebdb80fcb4e477c09ff0150"
    );
    let field = app.world().get::<Settings>(owner).unwrap().import;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(weights.parent().unwrap().to_str().unwrap());
    click(app.world_mut(), "Import selected decision model");
    settle(&mut app).await;
    let label = app.world().get::<Settings>(owner).unwrap().label;
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("installed and verified")
    );
    let source = directory.path().join("invalid-reimport");
    std::fs::create_dir(&source).unwrap();
    std::fs::File::create(source.join("model.safetensors"))
        .unwrap()
        .set_len(25_513_196)
        .unwrap();
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(source.to_str().unwrap());
    click(app.world_mut(), "Import selected decision model");
    settle(&mut app).await;
    let status = host.handle(Request::Inspect).await.unwrap();
    assert!(status.decision_model.as_ref().unwrap().present);
    assert!(!status.decision_model.as_ref().unwrap().loaded);
    assert_eq!(
        cell::fiote_communication::models::digest(&std::fs::read(&weights).unwrap()),
        hash
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep the real download draft"
    );
    assert!(
        app.world()
            .get::<super::super::Entry>(owner)
            .unwrap()
            .receipt
            .is_none()
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&runtime.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let reopened = cell::speech::Host::open(directory.path().join("speech.json"))
        .unwrap()
        .handle(Request::Inspect)
        .await
        .unwrap();
    assert_eq!(reopened.decision_selection, ModelId::EnglishNli);
    assert!(reopened.decision_model.unwrap().present);
}
