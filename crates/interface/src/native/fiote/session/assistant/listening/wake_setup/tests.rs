use super::*;
use cell::speech::Service as _;

fn simulated_recording(app: &mut App, owner: Entity, name: &str) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    Command::Record.apply(app.world_mut(), owner);
    let actual = app
        .world_mut()
        .get_mut::<Setup>(owner)
        .unwrap()
        .worker
        .take();
    assert!(
        actual.is_some(),
        "Record must start the normal capture worker"
    );
    drop(actual);
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fiote/tests/fixtures/conversation")
            .join(format!("{name}.wav")),
    )
    .unwrap();
    let samples = cell::fiote_communication::speech::decode_audio(&STANDARD.encode(bytes)).unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    app.world_mut().get_mut::<Setup>(owner).unwrap().worker = Some(Worker {
        stop: Arc::new(AtomicBool::new(false)),
        ready: Arc::new(AtomicBool::new(true)),
        announced: false,
        result: Mutex::new(receiver),
    });
    sender.send(Ok(Product::Recording(samples))).unwrap();
}

#[tokio::test]
async fn three_simulated_enrollment_takes_save_acoustic_features_and_discard_raw_audio() {
    let (mut app, owner, host, root) = super::super::tests::fixture().await;
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep my unsent draft");
    Command::Save.apply(app.world_mut(), owner);
    assert!(!busy(app.world(), owner));
    assert!(
        host.handle(Request::Inspect)
            .await
            .unwrap()
            .wake_profile
            .is_none()
    );
    for (index, name) in ["wake-statement", "wake-question", "wake-exclamation"]
        .into_iter()
        .enumerate()
    {
        simulated_recording(&mut app, owner, name);
        settle(&mut app, owner).await;
        assert_eq!(
            app.world().get::<Setup>(owner).unwrap().samples.len(),
            index + 1
        );
        assert!(
            host.handle(Request::Inspect)
                .await
                .unwrap()
                .wake_profile
                .is_none()
        );
    }
    Command::Save.apply(app.world_mut(), owner);
    settle(&mut app, owner).await;
    let saved = host.handle(Request::Inspect).await.unwrap();
    assert_eq!(saved.wake_profile.unwrap().references.len(), 3);
    assert!(!saved.listening.enabled);
    assert!(app.world().get::<Setup>(owner).unwrap().samples.is_empty());
    let reopened = cell::speech::Host::open(root.path().join("speech.json")).unwrap();
    assert_eq!(
        reopened
            .handle(Request::Inspect)
            .await
            .unwrap()
            .wake_profile
            .unwrap()
            .references
            .len(),
        3
    );
    simulated_recording(&mut app, owner, "wake-statement");
    settle(&mut app, owner).await;
    Command::Cancel.apply(app.world_mut(), owner);
    assert!(app.world().get::<Setup>(owner).unwrap().samples.is_empty());
    assert_eq!(
        host.handle(Request::Inspect)
            .await
            .unwrap()
            .wake_profile
            .unwrap()
            .references
            .len(),
        3
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep my unsent draft"
    );
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(!listening.wanted && !listening.saved.enabled && listening.capture.is_none());
    reopened.shutdown().await;
    host.shutdown().await;
}

async fn settle(app: &mut App, owner: Entity) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(message) = app
            .world_mut()
            .non_send_mut::<crate::native::cell_bridge::CellBridge>()
            .incoming
            .try_recv()
        {
            app.world_mut()
                .write_message(crate::native::cell_bridge::CellMessage(message));
        }
        app.update();
        if !busy(app.world(), owner) {
            return;
        }
        assert!(Instant::now() < deadline, "Wake controls did not settle");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn native_import_and_clear_persist_references_without_capture_or_draft_changes() {
    let (mut app, owner, host, root) = super::super::tests::fixture().await;
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep this editable request");
    let profile = Profile {
        version: 2,
        threshold: 0.9999,
        references: vec![vec![vec![0.0; 96]; 40]; 3],
        end_padding: vec![4000; 3],
    };
    let path = root.path().join("wake-reference.json");
    std::fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
    let field = app.world().get::<Setup>(owner).unwrap().import;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(&path.to_string_lossy());
    Command::Import.apply(app.world_mut(), owner);
    settle(&mut app, owner).await;
    assert!(
        host.handle(Request::Inspect)
            .await
            .unwrap()
            .wake_profile
            .is_some()
    );
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(!listening.wanted && !listening.saved.enabled && listening.capture.is_none());
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep this editable request"
    );
    Command::Clear.apply(app.world_mut(), owner);
    settle(&mut app, owner).await;
    assert!(
        host.handle(Request::Inspect)
            .await
            .unwrap()
            .wake_profile
            .is_none()
    );
    host.shutdown().await;
}

#[tokio::test]
async fn cancelling_an_import_ignores_its_late_result_without_saving_or_starting_capture() {
    let (mut app, owner, host, root) = super::super::tests::fixture().await;
    let path = root.path().join("wake-reference.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&Profile {
            version: 2,
            threshold: 0.9999,
            references: vec![vec![vec![0.0; 96]; 40]; 3],
            end_padding: vec![4000; 3],
        })
        .unwrap(),
    )
    .unwrap();
    let field = app.world().get::<Setup>(owner).unwrap().import;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(&path.to_string_lossy());
    Command::Import.apply(app.world_mut(), owner);
    Command::Cancel.apply(app.world_mut(), owner);
    settle(&mut app, owner).await;
    assert!(
        host.handle(Request::Inspect)
            .await
            .unwrap()
            .wake_profile
            .is_none()
    );
    assert!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .capture
            .is_none()
    );
    host.shutdown().await;
}

#[tokio::test]
async fn wake_recording_shows_the_speak_cue_only_after_microphone_startup_finishes() {
    let (mut app, owner, host, _root) = super::super::tests::fixture().await;
    let (sender, receiver) = mpsc::sync_channel(1);
    let ready = Arc::new(AtomicBool::new(false));
    let label = app.world().get::<Setup>(owner).unwrap().label;
    app.world_mut().get_mut::<Setup>(owner).unwrap().worker = Some(Worker {
        stop: Arc::new(AtomicBool::new(false)),
        ready: ready.clone(),
        announced: false,
        result: Mutex::new(receiver),
    });
    update(app.world_mut());
    assert!(
        !app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("Microphone ready")
    );
    ready.store(true, Ordering::Release);
    update(app.world_mut());
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("Microphone ready. Say Hey Fiote once now")
    );
    cancel(app.world_mut(), owner);
    assert!(
        sender
            .send(Ok(Product::Recording(vec![0.1; 16_000])))
            .is_err()
    );
    assert!(app.world().get::<Setup>(owner).unwrap().samples.is_empty());
    host.shutdown().await;
}

#[tokio::test]
async fn closing_the_view_discards_unsaved_raw_examples_and_cancels_the_capture_worker() {
    let (mut app, owner, host, _root) = super::super::tests::fixture().await;
    let (sender, receiver) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut setup = app.world_mut().get_mut::<Setup>(owner).unwrap();
        setup.samples = vec![vec![0.1; 16_000]; 2];
        setup.worker = Some(Worker {
            ready: Arc::new(AtomicBool::new(false)),
            announced: false,
            stop: stop.clone(),
            result: Mutex::new(receiver),
        });
    }
    app.world_mut().get_mut::<Node>(owner).unwrap().display = Display::None;
    update(app.world_mut());
    let setup = app.world().get::<Setup>(owner).unwrap();
    assert!(setup.samples.is_empty());
    assert!(setup.worker.is_none());
    assert!(stop.load(Ordering::Acquire));
    assert!(
        sender
            .send(Ok(Product::Recording(vec![0.2; 16_000])))
            .is_err()
    );
    assert!(
        host.handle(Request::Inspect)
            .await
            .unwrap()
            .wake_profile
            .is_none()
    );
    host.shutdown().await;
}
