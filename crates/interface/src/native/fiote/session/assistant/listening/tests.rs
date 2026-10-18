use super::*;
use cell::FioteService as _;
use cell::fiote_assistant::Conversation;
use cell::speech::Service;

mod recorded;

#[tokio::test]
#[ignore = "Checks the actual selected microphone through native controls without transcribing or sending"]
async fn actual_native_microphone_check_accepts_clean_input_or_refuses_distortion_without_changing_the_draft()
 {
    let (mut app, owner, host, _root) = fixture().await;
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep my unsent draft");
    if let Ok(name) = std::env::var("LINCE_SPEECH_TEST_MICROPHONE") {
        Command::Microphone(Some(name)).apply(app.world_mut(), owner);
    }
    let store = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .store
        .clone();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    for attempt in 1..=2 {
        click(app.world_mut(), "Check listening microphone");
        assert!(app.world().get::<input_check::Checking>(owner).is_some());
        tokio::time::timeout(Duration::from_secs(12), async {
            while app.world().get::<input_check::Checking>(owner).is_some() {
                app.update();
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let listening = app.world().get::<Listening>(owner).unwrap();
        let detail = app.world().get::<Text>(listening.label).unwrap().0.clone();
        assert!(
            detail.contains("Microphone check complete")
                || detail.contains("badly distorted")
                || detail.contains("No usable microphone sound")
                || detail.contains("did not settle"),
            "{detail}"
        );
        println!("microphone_attempt={attempt} {detail}");
        assert!(!listening.wanted && !listening.saved.enabled);
        assert!(listening.capture.is_none());
        assert_eq!(
            app.world()
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string(),
            "Keep my unsent draft"
        );
    }
    click(app.world_mut(), "Check listening microphone");
    click(app.world_mut(), "Stop listening");
    assert!(app.world().get::<input_check::Checking>(owner).is_none());
    let cancelled = AtomicBool::new(false);
    let selected = app
        .world()
        .get::<Listening>(owner)
        .unwrap()
        .microphone
        .clone();
    drop(
        crate::native::sound::device::Capture::start_when_available(
            selected.as_deref(),
            &cancelled,
        )
        .unwrap(),
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep my unsent draft"
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    host.shutdown().await;
}

pub(super) async fn fixture() -> (App, Entity, Arc<cell::speech::Host>, tempfile::TempDir) {
    let (mut app, owner, fiote, _script, directory, _record) = super::super::tests::fixture().await;
    let host = Arc::new(
        cell::speech::Host::open(directory.path().join("speech.json"))
            .unwrap()
            .with_fiote(Some(fiote)),
    );
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
    let cancellation =
        crate::native::cell_bridge::SpeechCancellation::for_owner(app.world(), owner);
    app.world_mut()
        .get_mut::<Listening>(owner)
        .unwrap()
        .cancellation = cancellation;
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

fn publish(app: &mut App, owner: Entity, mut status: cell::speech::Status) {
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    status.listening.owner = Some(session);
    status.listening.generation = 1;
    let id = nucleus::new_uid("voice-response");
    app.world_mut().get_mut::<Listening>(owner).unwrap().pending = Some(id.clone());
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech { id, status },
        ));
    app.update();
}

#[tokio::test]
async fn admitted_voice_recovery_preserves_edits_attachments_and_does_not_send_or_repeat() {
    let (mut app, owner, host, _directory) = fixture().await;
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Edited draft");
    let part = nucleus::message::MessagePart::Reference {
        name: "Keep attachment".into(),
        uri: "https://example.org/context.txt".into(),
    };
    app.world_mut()
        .get_mut::<crate::native::message_content::Draft>(owner)
        .unwrap()
        .parts
        .push(part.clone());
    let store = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .store
        .clone();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    let mut status = host.handle(Request::Inspect).await.unwrap();
    status.listening.unsent = Some(VoiceRequest {
        owner: app.world().get::<Listening>(owner).unwrap().owner.clone(),
        generation: 1,
        segment: 1,
        request: EntryRequest {
            request_id: "recover-voice".into(),
            target: None,
            text: "Please help with this admitted request".into(),
            content: Vec::new(),
        },
        uncertain: false,
    });
    publish(&mut app, owner, status.clone());
    click(app.world_mut(), "Stop listening");
    click(app.world_mut(), "Recover unsent voice request");
    let expected = "Edited draft\n\nPlease help with this admitted request";
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        expected
    );
    assert_eq!(
        app.world()
            .get::<crate::native::message_content::Draft>(owner)
            .unwrap()
            .parts,
        vec![part]
    );
    publish(&mut app, owner, status);
    click(app.world_mut(), "Recover unsent voice request");
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        expected
    );
    assert!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .recovery
            .is_none()
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    click(app.world_mut(), "Send assistant request");
    super::super::tests::responses(&mut app).await;
    let entry = app.world().get::<Entry>(owner).unwrap();
    let detail = app.world().get::<Text>(entry.status).unwrap().0.clone();
    assert!(entry.receipt.is_none(), "{detail}");
    assert!(
        detail.contains("connection") || detail.contains("configured Fiote"),
        "{detail}"
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        expected
    );
    click(app.world_mut(), "Native assistant");
    click(app.world_mut(), "Send assistant request");
    super::super::tests::responses(&mut app).await;
    assert!(
        app.world()
            .get::<Entry>(owner)
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .message
            .is_some()
    );
    assert!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string()
            .is_empty()
    );
}

#[tokio::test]
async fn uncertain_or_already_delivered_voice_is_not_inserted_for_another_send() {
    let (mut app, owner, host, _directory) = fixture().await;
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep my unrelated draft");
    let mut status = host.handle(Request::Inspect).await.unwrap();
    status.listening.unsent = Some(VoiceRequest {
        owner: app.world().get::<Listening>(owner).unwrap().owner.clone(),
        generation: 1,
        segment: 1,
        request: EntryRequest {
            request_id: "uncertain-voice".into(),
            target: None,
            text: "Possibly delivered command".into(),
            content: Vec::new(),
        },
        uncertain: true,
    });
    publish(&mut app, owner, status);
    click(app.world_mut(), "Recover unsent voice request");
    let label = app.world().get::<Listening>(owner).unwrap().label;
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("Inspect")
    );
    app.world_mut().get_mut::<Entry>(owner).unwrap().receipt = Some(EntryReceipt {
        request_id: "uncertain-voice".into(),
        fingerprint: "committed".into(),
        target: nucleus::new_uid("r"),
        thread: Some(nucleus::new_uid("r")),
        message: Some(nucleus::new_uid("r")),
        reply: None,
        state: "completed".into(),
        detail: String::new(),
    });
    click(app.world_mut(), "Recover unsent voice request");
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("already delivered")
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep my unrelated draft"
    );
}

#[tokio::test]
async fn accepted_voice_receipt_persists_and_reopens_by_inspection_without_redispatch() {
    let (mut app, owner, host, script, directory, record) = super::super::tests::fixture().await;
    let panel = app.world().get::<Entry>(owner).unwrap().panel;
    let view = app.world().get::<Panel>(panel).unwrap().view_uid.clone();
    app.insert_resource(crate::native::workspace::WorkspaceFile::new(
        directory.path().join("interface.json"),
    ));
    let path = directory.path().join("assistant-requests").join(format!(
        "{}.json",
        nucleus::message::digest(view.as_bytes())
    ));
    app.world_mut().get_mut::<Entry>(owner).unwrap().path = Some(path.clone());
    let receipt = host
        .handle(FioteRequest::AssistantSend {
            request: EntryRequest {
                request_id: "durable-voice-request".into(),
                target: Some(record),
                text: "Hello from an admitted voice request".into(),
                content: Vec::new(),
            },
        })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    let speech = cell::speech::Host::open(directory.path().join("speech.json")).unwrap();
    let mut status = speech.handle(Request::Inspect).await.unwrap();
    status.listening.receipt = Some(receipt.clone());
    publish(&mut app, owner, status);
    let persisted: Saved = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted.request_id, receipt.request_id);
    assert!(
        !std::fs::read_to_string(path)
            .unwrap()
            .contains("Hello from")
    );
    Open.apply(app.world_mut(), panel);
    super::super::tests::responses(&mut app).await;
    let restored = app
        .world_mut()
        .query::<&Entry>()
        .iter(app.world())
        .next()
        .unwrap();
    assert_eq!(
        restored.receipt.as_ref().unwrap().request_id,
        receipt.request_id
    );
    assert_eq!(restored.receipt.as_ref().unwrap().message, receipt.message);
    tokio::time::timeout(Duration::from_secs(5), async {
        while script.requests.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(script.requests.lock().unwrap().len(), 1);
}

async fn settle(app: &mut App) {
    for _ in 0..1000 {
        app.update();
        if !app
            .world_mut()
            .query::<&Listening>()
            .iter(app.world())
            .any(|listening| listening.pending.is_some())
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
    panic!("Listening controls did not settle");
}

fn fake_capture() -> (Capture, mpsc::SyncSender<Event>, Arc<AtomicBool>) {
    let (send, receive) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    (
        Capture {
            stop: stop.clone(),
            events: Mutex::new(receive),
            feedback: Arc::new(Mutex::new(Feedback::default())),
            conversation: Arc::new(AtomicBool::new(false)),
        },
        send,
        stop,
    )
}

#[tokio::test]
async fn status_polling_accepts_one_utterance_and_ignores_the_stale_inspection_reply() {
    let (mut app, owner, host, _root) = fixture().await;
    let (outgoing, mut requests) = tokio::sync::mpsc::channel(8);
    app.world_mut()
        .non_send_mut::<crate::native::cell_bridge::CellBridge>()
        .outgoing = outgoing;
    let (capture, events, _) = fake_capture();
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.capture = Some(capture);
        listening.wanted = true;
        listening.saved.owner = Some(session.clone());
        listening.saved.generation = 7;
        listening.saved.enabled = true;
        listening.saved.mode = ListeningMode::Wake;
        listening.next = Instant::now() + Duration::from_secs(60);
    }
    send(app.world_mut(), owner, Request::Inspect).unwrap();
    let inspection = match requests.try_recv().unwrap() {
        cell::ClientMessage::Speech {
            id,
            request: Request::Inspect,
        } => id,
        other => panic!("Expected a held status poll: {other:?}"),
    };
    assert!(app.world().get::<Listening>(owner).unwrap().polling);
    events
        .send(Event::WokenSegment {
            generation: 7,
            id: 1,
            audio: "fixture-audio".into(),
        })
        .unwrap();
    app.update();
    let delivery = match requests.try_recv().unwrap() {
        cell::ClientMessage::Speech {
            id,
            request:
                Request::WokenSegment {
                    owner: requested_owner,
                    generation: 7,
                    segment: 1,
                    ..
                },
        } => {
            assert_eq!(requested_owner, session);
            id
        }
        other => panic!("A read-only poll must not drop speech: {other:?}"),
    };
    assert_ne!(inspection, delivery);
    assert!(!app.world().get::<Listening>(owner).unwrap().polling);
    let status = host.handle(Request::Inspect).await.unwrap();
    assert!(!status.listening.enabled);
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech {
                id: inspection,
                status,
            },
        ));
    app.update();
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(listening.saved.enabled && listening.capture.is_some());
    assert_eq!(listening.pending.as_deref(), Some(delivery.as_str()));
    events
        .send(Event::WokenSegment {
            generation: 7,
            id: 2,
            audio: "second-audio".into(),
        })
        .unwrap();
    app.update();
    assert!(
        requests.try_recv().is_err(),
        "An active delivery still excludes another utterance"
    );
    assert_eq!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .pending
            .as_deref(),
        Some(delivery.as_str())
    );
}

#[tokio::test]
async fn speaking_pauses_and_resumes_the_bound_conversation_and_end_can_be_cancelled_by_stop() {
    let (mut app, owner, host, _root) = fixture().await;
    let (sender, mut requests) = tokio::sync::mpsc::channel(8);
    app.world_mut()
        .non_send_mut::<crate::native::cell_bridge::CellBridge>()
        .outgoing = sender.clone();
    let conversation = Conversation {
        target: nucleus::new_uid("r"),
        thread: nucleus::new_uid("r"),
    };
    let (capture, _events, stopped) = fake_capture();
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.wanted = true;
        listening.saved.owner = Some(session.clone());
        listening.saved.generation = 9;
        listening.saved.enabled = true;
        listening.saved.conversation = Some(conversation.clone());
        listening.capture = Some(capture);
        listening.cancellation = Some(crate::native::cell_bridge::SpeechCancellation::new(
            sender.clone(),
        ));
    }
    suspend(app.world_mut(), owner);
    assert!(stopped.load(Ordering::Acquire));
    assert!(app.world().get::<Resume>(owner).is_some());
    assert!(
        matches!(requests.try_recv().unwrap(), cell::ClientMessage::Speech { request: Request::PauseListening { owner, generation: 9 }, .. } if owner == session)
    );
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        assert_eq!(listening.saved.conversation.as_ref(), Some(&conversation));
        listening.pending = None;
        listening.stopping = false;
        listening.saved.enabled = false;
        listening.saved.paused = true;
    }
    resume(app.world_mut(), owner);
    assert!(
        matches!(requests.try_recv().unwrap(), cell::ClientMessage::Speech { request: Request::ResumeListening { owner, generation: 9 }, .. } if owner == session)
    );
    assert_eq!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .saved
            .conversation
            .as_ref(),
        Some(&conversation)
    );
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.pending = None;
        listening.saved.paused = false;
        listening.saved.enabled = true;
        listening.saved.generation = 10;
        listening.cancellation = Some(crate::native::cell_bridge::SpeechCancellation::new(sender));
    }
    click(app.world_mut(), "End voice conversation");
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(listening.saved.conversation.is_none());
    assert!(listening.restart && listening.stopping);
    assert!(matches!(
        requests.try_recv().unwrap(),
        cell::ClientMessage::Speech {
            request: Request::StopListening { generation: 10, .. },
            ..
        }
    ));
    click(app.world_mut(), "Stop listening");
    assert!(!app.world().get::<Listening>(owner).unwrap().restart);
    host.shutdown().await;
}

#[tokio::test]
async fn backend_conversation_changes_update_the_acoustic_gate_and_visible_destination() {
    let (mut app, owner, host, _root) = fixture().await;
    let (capture, _events, _stopped) = fake_capture();
    let active = capture.conversation.clone();
    app.world_mut().get_mut::<Listening>(owner).unwrap().capture = Some(capture);
    app.world_mut().get_mut::<Listening>(owner).unwrap().wanted = true;
    let mut status = host.handle(Request::Inspect).await.unwrap();
    status.listening.enabled = true;
    status.listening.mode = ListeningMode::Wake;
    let conversation = Conversation {
        target: nucleus::new_uid("r"),
        thread: nucleus::new_uid("r"),
    };
    status.listening.conversation = Some(conversation.clone());
    publish(&mut app, owner, status.clone());
    assert!(active.load(Ordering::Acquire));
    let label = app
        .world()
        .get::<Listening>(owner)
        .unwrap()
        .conversation_label;
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains(&conversation.thread)
    );
    status.listening.conversation = None;
    publish(&mut app, owner, status);
    assert!(!active.load(Ordering::Acquire));
    assert_eq!(
        app.world().get::<Text>(label).unwrap().0,
        "Voice conversation: none"
    );
    click(app.world_mut(), "Stop listening");
    host.shutdown().await;
}

#[tokio::test]
async fn speaking_blocks_activation_and_explicit_stop_cancels_pending_resumption() {
    let (mut app, owner, host, _root) = fixture().await;
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    app.world_mut()
        .entity_mut(owner)
        .insert((Resume(session), crate::native::sound::VoiceOutputActive));
    Command::Start.apply(app.world_mut(), owner);
    assert!(!app.world().get::<Listening>(owner).unwrap().wanted);
    assert!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .capture
            .is_none()
    );
    Command::Stop.apply(app.world_mut(), owner);
    assert!(app.world().get::<Resume>(owner).is_none());
    app.world_mut()
        .entity_mut(owner)
        .remove::<crate::native::sound::VoiceOutputActive>();
    resume_all(app.world_mut());
    assert!(!app.world().get::<Listening>(owner).unwrap().wanted);
    host.shutdown().await;
}

#[tokio::test]
async fn changing_recipient_while_speech_pauses_listening_cannot_restart_capture() {
    let (mut app, owner, host, _root) = fixture().await;
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    app.world_mut().entity_mut(owner).insert(Resume(session));
    super::super::Command::Select(nucleus::new_uid("r")).apply(app.world_mut(), owner);
    assert!(app.world().get::<Resume>(owner).is_none());
    resume_all(app.world_mut());
    assert!(!app.world().get::<Listening>(owner).unwrap().wanted);
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
async fn closing_listening_controls_keeps_cancellation_when_the_outgoing_queue_is_full() {
    let (mut app, owner, _host, _directory) = fixture().await;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    sender
        .try_send(cell::ClientMessage::Speech {
            id: "occupied".into(),
            request: Request::Inspect,
        })
        .unwrap();
    let cancellation = crate::native::cell_bridge::SpeechCancellation::new(sender);
    let (capture, _events, stopped) = fake_capture();
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.wanted = true;
        listening.pending = Some("unacknowledged-start".into());
        listening.capture = Some(capture);
        listening.cancellation = Some(cancellation);
    }
    app.world_mut().despawn(owner);
    assert!(stopped.load(Ordering::Acquire));
    receiver.recv().await.unwrap();
    let request = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(request, cell::ClientMessage::Speech { request: Request::StopListening { owner, generation: 0 }, .. } if owner == session)
    );
}

#[tokio::test]
async fn a_closed_owned_speech_channel_stops_capture_without_waiting_for_a_global_error() {
    let (mut app, owner, host, _root) = fixture().await;
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let cancellation = crate::native::cell_bridge::SpeechCancellation::new(sender);
    let (capture, _events, stopped) = fake_capture();
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.wanted = true;
        listening.saved.enabled = true;
        listening.pending = Some("pending-at-disconnect".into());
        listening.capture = Some(capture);
        listening.cancellation = Some(cancellation);
    }
    drop(receiver);
    app.update();
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(stopped.load(Ordering::Acquire));
    assert!(!listening.wanted && !listening.saved.enabled);
    assert!(listening.capture.is_none() && listening.pending.is_none());
    assert!(
        app.world()
            .get::<Text>(listening.label)
            .unwrap()
            .0
            .contains("connection closed")
    );
    host.shutdown().await;
}

#[tokio::test]
async fn stop_keeps_the_original_cancellation_route_when_the_view_source_is_unavailable() {
    let (mut app, owner, host, _directory) = fixture().await;
    let (sender, mut requests) = tokio::sync::mpsc::channel(1);
    let cancellation = crate::native::cell_bridge::SpeechCancellation::new(sender);
    let (capture, _events, stopped) = fake_capture();
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.wanted = true;
        listening.pending = Some("unacknowledged-start".into());
        listening.capture = Some(capture);
        listening.cancellation = Some(cancellation);
    }
    app.world_mut()
        .entity_mut(owner)
        .insert(crate::native::practice_cells::PracticeSource(
            "disconnected-speech-cell".into(),
        ));
    assert!(
        crate::native::cell_bridge::SpeechCancellation::for_owner(app.world(), owner).is_none()
    );
    Command::Stop.apply(app.world_mut(), owner);
    assert!(stopped.load(Ordering::Acquire));
    let request = tokio::time::timeout(Duration::from_secs(2), requests.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(request, cell::ClientMessage::Speech { request: Request::StopListening { owner, generation: 0 }, .. } if owner == session)
    );
    assert!(!app.world().get::<Listening>(owner).unwrap().wanted);
    host.shutdown().await;
}

#[tokio::test]
async fn hidden_or_inactive_workspace_cancels_pending_start_and_ignores_its_late_acknowledgment() {
    for hidden in [true, false] {
        let (mut app, owner, host, _directory) = fixture().await;
        let input = app.world().get::<Entry>(owner).unwrap().input;
        app.world_mut()
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("Keep this draft");
        let panel = app.world().get::<Entry>(owner).unwrap().panel;
        let area = app.world().get::<Panel>(panel).unwrap().binding.area;
        let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
        let (capture, _events, stopped) = fake_capture();
        {
            let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
            listening.wanted = true;
            listening.pending = Some("late-start".into());
            listening.capture = Some(capture);
        }
        if hidden {
            app.world_mut().get_mut::<Node>(area).unwrap().display = Display::None;
        } else {
            app.world_mut().entity_mut(area).insert((
                crate::native::workspace::WorkspaceMember(1),
                crate::native::workspace::Workspaces {
                    active: 2,
                    ..Default::default()
                },
            ));
        }
        app.update();
        assert!(stopped.load(Ordering::Acquire));
        let mut status = host.handle(Request::Inspect).await.unwrap();
        status.listening.owner = Some(session);
        status.listening.enabled = true;
        status.listening.generation = 1;
        app.world_mut()
            .write_message(crate::native::cell_bridge::CellMessage(
                cell::ServerMessage::Speech {
                    id: "late-start".into(),
                    status,
                },
            ));
        app.update();
        settle(&mut app).await;
        let listening = app.world().get::<Listening>(owner).unwrap();
        assert!(!listening.wanted);
        assert!(listening.capture.is_none());
        assert_eq!(
            app.world()
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string(),
            "Keep this draft"
        );
    }
}

#[test]
fn microphone_readiness_cannot_displace_the_first_retained_utterance() {
    let (capture, send, _stopped) = fake_capture();
    assert!(emit(&send, &capture.feedback, Event::Started, None));
    assert!(emit(
        &send,
        &capture.feedback,
        Event::Segment {
            generation: 1,
            id: 1,
            audio: "first clean utterance".into(),
        },
        None,
    ));
    assert!(matches!(capture.event(), Some(Event::Started)));
    assert!(matches!(
        capture.event(),
        Some(Event::Segment { id: 1, audio, .. }) if audio == "first clean utterance"
    ));
    assert!(capture.event().is_none());
}

#[test]
fn capture_overflow_reports_refusal_and_device_failure_takes_priority_over_queued_audio() {
    let (capture, send, _stopped) = fake_capture();
    let segment = |id| Event::Segment {
        generation: 1,
        id,
        audio: "queued audio".into(),
    };
    assert!(emit(&send, &capture.feedback, segment(1), None));
    assert!(emit(&send, &capture.feedback, segment(2), None));
    assert!(
        matches!(capture.event(), Some(Event::Notice(detail)) if detail.contains("not queued"))
    );
    emit(
        &send,
        &capture.feedback,
        Event::Notice("low priority notice".into()),
        None,
    );
    emit(
        &send,
        &capture.feedback,
        Event::Failed("Microphone disconnected".into()),
        None,
    );
    assert!(
        matches!(capture.event(), Some(Event::Failed(detail)) if detail == "Microphone disconnected")
    );
    assert!(matches!(capture.event(), Some(Event::Notice(_))));
    assert!(matches!(
        capture.event(),
        Some(Event::Segment { id: 1, .. })
    ));
    assert!(capture.event().is_none());
}

#[tokio::test]
#[ignore = "Requires installed Phonon weights and the independently referenced goforward recording"]
async fn real_streamed_recording_finalizes_once_and_uses_the_normal_local_speech_service() {
    use base64::Engine as _;
    let root = tempfile::tempdir().unwrap();
    let host = cell::speech::Host::open(root.path().join("speech.json")).unwrap();
    let model = host.handle(Request::Inspect).await.unwrap().local.unwrap();
    let source = std::path::PathBuf::from(
        std::env::var_os("LINCE_SPEECH_TEST_CACHE").expect("Choose installed model cache"),
    )
    .join(format!("{}-{}", model.model, model.revision))
    .join("phonon-2.bps.tar.zst");
    let installation = nucleus::new_uid("speech");
    host.handle(Request::InstallLocal {
        job: installation.clone(),
        source: Some(source),
    })
    .await
    .unwrap();
    async fn finished(host: &cell::speech::Host, job: &str) -> cell::speech::Job {
        tokio::time::timeout(Duration::from_secs(180), async {
            loop {
                let result = host
                    .handle(Request::Poll { job: job.into() })
                    .await
                    .unwrap()
                    .job
                    .unwrap();
                if !result.pending {
                    return result;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    assert!(finished(&host, &installation).await.error.is_none());
    let recorded = base64::engine::general_purpose::STANDARD.encode(
        std::fs::read(
            std::env::var_os("LINCE_SPEECH_TEST_WAV").expect("Choose the goforward recording"),
        )
        .unwrap(),
    );
    let samples = cell::fiote_communication::speech::decode_audio(&recorded).unwrap();
    let mut segments = Segments::new(77);
    for _ in 0..10 {
        assert!(
            segments
                .push(&crate::native::sound::library::Clip {
                    rate: 48_000,
                    samples: vec![0.0; 4800]
                })
                .unwrap()
                .is_empty()
        );
    }
    let mut events = Vec::new();
    let samples = samples
        .into_iter()
        .flat_map(|sample| std::iter::repeat_n(sample, 3))
        .collect::<Vec<_>>();
    for chunk in samples.chunks(4800) {
        events.extend(
            segments
                .push(&crate::native::sound::library::Clip {
                    rate: 48_000,
                    samples: chunk.to_vec(),
                })
                .unwrap(),
        );
    }
    for _ in 0..10 {
        events.extend(
            segments
                .push(&crate::native::sound::library::Clip {
                    rate: 48_000,
                    samples: vec![0.0; 4800],
                })
                .unwrap(),
        );
    }
    assert_eq!(events.len(), 1);
    let audio = match events.pop().unwrap() {
        Event::Segment {
            generation: 77,
            id: 1,
            audio,
        } => audio,
        _ => panic!("The recording did not finalize as one owned utterance"),
    };
    let job = nucleus::new_uid("speech");
    host.handle(Request::StartLocal {
        job: job.clone(),
        audio: cell::FioteSecret(audio),
    })
    .await
    .unwrap();
    let result = finished(&host, &job).await;
    assert!(result.error.is_none(), "{result:?}");
    assert_eq!(
        result.text.unwrap().to_lowercase().trim_end_matches('.'),
        "go forward ten meters"
    );
    for _ in 0..10 {
        assert!(
            segments
                .push(&crate::native::sound::library::Clip {
                    rate: 48_000,
                    samples: vec![0.0; 4800]
                })
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
#[ignore = "Requires the machine's audio input host to enumerate actual devices"]
async fn actual_missing_microphone_reports_failure_without_retaining_the_capture_lease() {
    let name = nucleus::new_uid("missing-microphone");
    let capture = super::capture(Some(name.clone()), 1, None, None, false);
    let error = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Event::Failed(error)) = capture.event() {
                break error;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        error.contains("selected microphone is unavailable"),
        "{error}"
    );
    drop(capture);
    let error = match crate::native::sound::device::Capture::start_on(Some(&name)) {
        Ok(_) => panic!("The intentionally missing microphone became available"),
        Err(error) => error,
    };
    assert!(
        error.contains("selected microphone is unavailable"),
        "{error}"
    );
}

#[tokio::test]
async fn ownership_changes_stop_this_capture_without_stopping_the_other_listener() {
    let (mut app, owner, host, _directory) = fixture().await;
    let (capture, _events, stopped) = fake_capture();
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.wanted = true;
        listening.saved.enabled = true;
        listening.capture = Some(capture);
        listening.pending = Some("ownership-response".into());
    }
    let mut status = host.handle(Request::Inspect).await.unwrap();
    status.listening.enabled = true;
    status.listening.owner = Some(nucleus::new_uid("voice"));
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech {
                id: "ownership-response".into(),
                status,
            },
        ));
    app.update();
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(stopped.load(Ordering::Acquire));
    assert!(!listening.wanted);
    assert!(!listening.saved.enabled);
    assert!(listening.pending.is_none());
    assert!(listening.capture.is_none());
}

#[tokio::test]
async fn device_failure_with_queued_audio_stops_capture_without_creating_a_request() {
    let (mut app, owner, _host, _directory) = fixture().await;
    let store = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .store
        .clone();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    let (capture, send, stopped) = fake_capture();
    emit(
        &send,
        &capture.feedback,
        Event::Segment {
            generation: 1,
            id: 1,
            audio: "must never be dispatched".into(),
        },
        None,
    );
    emit(
        &send,
        &capture.feedback,
        Event::Failed("Microphone disconnected".into()),
        None,
    );
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.wanted = true;
        listening.saved.enabled = true;
        listening.saved.generation = 1;
        listening.saved.owner = Some(listening.owner.clone());
        listening.capture = Some(capture);
    }
    app.update();
    assert!(stopped.load(Ordering::Acquire));
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(!listening.wanted);
    assert!(listening.capture.is_none());
    assert_eq!(
        app.world().get::<Text>(listening.label).unwrap().0,
        "Microphone disconnected"
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert!(app.world().get::<Entry>(owner).unwrap().receipt.is_none());
    settle(&mut app).await;
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(
        app.world()
            .get::<Text>(listening.label)
            .unwrap()
            .0
            .contains("Microphone disconnected")
    );
}

#[tokio::test]
async fn microphone_changes_require_stopping_and_connection_loss_preserves_the_draft() {
    let (mut app, owner, _host, _directory) = fixture().await;
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep my draft");
    let (capture, _events, stopped) = fake_capture();
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.wanted = true;
        listening.saved.enabled = true;
        listening.saved.generation = 1;
        listening.microphone = Some("original".into());
        listening.capture = Some(capture);
    }
    Command::Microphone(Some("other".into())).apply(app.world_mut(), owner);
    assert_eq!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .microphone
            .as_deref(),
        Some("original")
    );
    assert!(!stopped.load(Ordering::Acquire));
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Error {
                id: crate::native::cell_bridge::CONNECTION.into(),
                message: "Disconnected".into(),
                code: None,
            },
        ));
    app.update();
    assert!(stopped.load(Ordering::Acquire));
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(!listening.saved.enabled);
    assert!(!listening.wanted);
    assert!(listening.capture.is_none());
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep my draft"
    );
}

#[tokio::test]
async fn unvalidated_native_listening_creates_no_capture_or_messages_and_keeps_the_draft() {
    let (mut app, owner, host, _directory) = fixture().await;
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep my normal request");
    let store = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .store
        .clone();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    click(app.world_mut(), "Model-based admission");
    click(app.world_mut(), "Start listening");
    settle(&mut app).await;
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(!listening.saved.enabled);
    assert!(!listening.wanted);
    assert!(listening.capture.is_none());
    assert!(
        app.world()
            .get::<Text>(listening.label)
            .unwrap()
            .0
            .contains("validated speech admission")
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep my normal request"
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert!(
        !host
            .handle(Request::Inspect)
            .await
            .unwrap()
            .listening
            .enabled
    );
}

#[tokio::test]
async fn listening_mode_controls_start_off_and_refuse_changes_while_listening() {
    let (mut app, owner, _host, _directory) = fixture().await;
    assert_eq!(
        app.world().get::<Listening>(owner).unwrap().mode,
        ListeningMode::Wake
    );
    assert!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .capture
            .is_none()
    );
    click(app.world_mut(), "Model-based admission");
    assert_eq!(
        app.world().get::<Listening>(owner).unwrap().mode,
        ListeningMode::Model
    );
    click(app.world_mut(), "Addressed requests");
    assert_eq!(
        app.world().get::<Listening>(owner).unwrap().mode,
        ListeningMode::Addressed
    );
    app.world_mut()
        .get_mut::<Listening>(owner)
        .unwrap()
        .saved
        .enabled = true;
    click(app.world_mut(), "Model-based admission");
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert_eq!(listening.mode, ListeningMode::Addressed);
    assert!(
        app.world()
            .get::<Text>(listening.label)
            .unwrap()
            .0
            .contains("Stop listening")
    );
}

#[tokio::test]
async fn a_different_confirmed_mode_never_opens_capture_for_this_view() {
    let (mut app, owner, host, _directory) = fixture().await;
    let mut status = host.handle(Request::Inspect).await.unwrap();
    status.listening.enabled = true;
    status.listening.mode = ListeningMode::Model;
    app.world_mut().get_mut::<Listening>(owner).unwrap().wanted = true;
    publish(&mut app, owner, status);
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(listening.capture.is_none());
    assert!(!listening.wanted);
    assert!(listening.stopping);
    assert!(
        listening
            .failure
            .as_deref()
            .unwrap()
            .contains("mode differs")
    );
}

#[tokio::test]
async fn stop_before_activation_acknowledgment_invalidates_the_owner_and_suppresses_late_start() {
    let (mut app, owner, host, _directory) = fixture().await;
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.pending = Some("old-start".into());
        listening.wanted = true;
    }
    click(app.world_mut(), "Stop listening");
    settle(&mut app).await;
    let mut status = host.handle(Request::Inspect).await.unwrap();
    status.listening.enabled = true;
    status.listening.owner = Some(session);
    status.listening.generation = 1;
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech {
                id: "old-start".into(),
                status,
            },
        ));
    app.update();
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(!listening.saved.enabled);
    assert!(!listening.wanted);
    assert!(listening.capture.is_none());
}

#[tokio::test]
async fn stop_control_signals_capture_cancellation_without_stopping_fiote_work() {
    let (mut app, owner, _host, _directory) = fixture().await;
    let stop = Arc::new(AtomicBool::new(false));
    let (_send, receive) = mpsc::sync_channel(1);
    app.world_mut().get_mut::<Listening>(owner).unwrap().capture = Some(Capture {
        stop: stop.clone(),
        events: Mutex::new(receive),
        feedback: Arc::new(Mutex::new(Feedback::default())),
        conversation: Arc::new(AtomicBool::new(false)),
    });
    click(app.world_mut(), "Stop listening");
    assert!(stop.load(Ordering::Acquire));
    assert!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .capture
            .is_none()
    );
    assert!(app.world().get::<Entry>(owner).unwrap().receipt.is_none());
}

#[tokio::test]
async fn changing_recipient_closes_pending_listening_and_preserves_the_draft() {
    let (mut app, owner, host, _directory) = fixture().await;
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep my next request");
    let session = app.world().get::<Listening>(owner).unwrap().owner.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let (_send, receive) = mpsc::sync_channel(1);
    {
        let mut listening = app.world_mut().get_mut::<Listening>(owner).unwrap();
        listening.pending = Some("old-start".into());
        listening.wanted = true;
        listening.capture = Some(Capture {
            stop: stop.clone(),
            events: Mutex::new(receive),
            feedback: Arc::new(Mutex::new(Feedback::default())),
            conversation: Arc::new(AtomicBool::new(false)),
        });
    }
    click(app.world_mut(), "Native assistant");
    assert!(stop.load(Ordering::Acquire));
    settle(&mut app).await;
    let mut status = host.handle(Request::Inspect).await.unwrap();
    status.listening.enabled = true;
    status.listening.owner = Some(session);
    status.listening.generation = 1;
    app.world_mut()
        .write_message(crate::native::cell_bridge::CellMessage(
            cell::ServerMessage::Speech {
                id: "old-start".into(),
                status,
            },
        ));
    app.update();
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(!listening.wanted);
    assert!(!listening.saved.enabled);
    assert!(listening.capture.is_none());
    assert!(app.world().get::<Entry>(owner).unwrap().target.is_some());
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep my next request"
    );
}

#[test]
fn streaming_normalization_preserves_silence_and_bounds_invalid_capture() {
    let quiet = crate::native::sound::library::Clip {
        rate: 44_100,
        samples: vec![0.0; 4410],
    };
    let normalized = crate::native::speech::mono16k(&quiet).unwrap();
    assert_eq!(normalized.len(), 1600);
    assert!(normalized.iter().all(|sample| *sample == 0.0));
    for clip in [
        crate::native::sound::library::Clip {
            rate: 0,
            samples: vec![0.0],
        },
        crate::native::sound::library::Clip {
            rate: 16_000,
            samples: vec![f32::NAN],
        },
    ] {
        assert!(crate::native::speech::mono16k(&clip).is_err());
    }
}

#[test]
fn streaming_resampling_preserves_fractional_samples_across_arbitrary_drains() {
    for rate in [8000, 11_025, 16_000, 44_100, 48_000, 96_000, 192_000] {
        let clip = crate::native::sound::library::Clip {
            rate,
            samples: (0..rate + 123)
                .map(|index| (index as f32 * 0.01).sin() * 0.3)
                .collect(),
        };
        let expected = crate::native::speech::mono16k(&clip).unwrap();
        assert_eq!(
            expected.len(),
            (clip.samples.len() as u64 * 16_000 / u64::from(rate)) as usize
        );
        for chunk in [1, 127, 320, 4096] {
            let mut resampler = crate::native::speech::Resampler::new(rate).unwrap();
            let mut output = Vec::new();
            for samples in clip.samples.chunks(chunk) {
                output.extend(
                    resampler
                        .push(&crate::native::sound::library::Clip {
                            rate,
                            samples: samples.to_vec(),
                        })
                        .unwrap(),
                );
            }
            assert_eq!(output, expected, "Sample rate {rate}, drain size {chunk}");
        }
        let mut resampler = crate::native::speech::Resampler::new(rate).unwrap();
        assert!(
            resampler
                .push(&crate::native::sound::library::Clip {
                    rate: rate + 1,
                    samples: vec![0.0],
                })
                .is_err()
        );
    }
}
