use super::*;

mod native_connection {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/support/native_connection.rs"
    ));
}

#[derive(Resource)]
struct Trace {
    started: Instant,
    owner: Entity,
    stage: String,
    speaking: bool,
    output: u32,
}

pub(super) fn trace(app: &mut App) {
    let Some(mut trace) = app.world_mut().remove_resource::<Trace>() else {
        return;
    };
    if let Some(listening) = app.world().get::<Listening>(trace.owner) {
        let stage = format!("{:?}", listening.saved.stage);
        if stage != trace.stage {
            println!(
                "{}",
                serde_json::json!({"phase":"observed_listening_stage","stage":stage,"elapsed_ms":trace.started.elapsed().as_millis()})
            );
            trace.stage = stage;
        }
    }
    let speaking = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .any(|text| {
            text.0.starts_with("English voice: installed") && text.0.contains(" · speaking")
        });
    if speaking != trace.speaking {
        if speaking {
            trace.output += 1;
        }
        println!(
            "{}",
            serde_json::json!({"phase":if speaking { "audio_output_queued" } else { "audio_output_inactive" },"output":trace.output,"elapsed_ms":trace.started.elapsed().as_millis()})
        );
        trace.speaking = speaking;
    }
    app.insert_resource(trace);
}

fn cpu_ticks() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/stat").ok()?;
    let values = status
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    Some(values.get(11)?.parse::<u64>().ok()? + values.get(12)?.parse::<u64>().ok()?)
}

async fn idle(app: &mut App, segments: &mut Segments) {
    let seconds = std::env::var("LINCE_VOICE_TEST_IDLE_SECONDS")
        .map(|value| value.parse::<u64>().unwrap())
        .unwrap_or(0);
    assert!(seconds <= 60);
    if seconds == 0 {
        return;
    }
    let before = cpu_ticks();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(seconds) {
        assert!(
            segments
                .push(&crate::native::sound::library::Clip {
                    rate: 16000,
                    samples: vec![0.0; 1600]
                })
                .unwrap()
                .is_empty()
        );
        tick(app);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!(
        "{}",
        serde_json::json!({"phase":"simulated_idle_ui_and_wake","wall_ms":started.elapsed().as_millis(),"cpu_ticks":cpu_ticks().zip(before).map(|(after,before)| after.saturating_sub(before)),"linux_clock_ticks_per_second":100,"resources":measurements()})
    );
}

fn recording(name: &str) -> Vec<u8> {
    let reference: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/fixtures/english-conversation-reference.json"
    )))
    .unwrap();
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fiote/tests/fixtures/conversation")
            .join(format!("{name}.wav")),
    )
    .unwrap();
    assert_eq!(
        nucleus::message::digest(&bytes),
        reference["cases"][name]["sha256"].as_str().unwrap()
    );
    bytes
}

async fn completed(
    app: &mut App,
    host: &cell::fiote::Host,
    request: &str,
) -> cell::fiote_assistant::EntryReceipt {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            tick(app);
            let receipt = host
                .handle(FioteRequest::AssistantInspect {
                    request_id: request.into(),
                })
                .await
                .unwrap()
                .assistant_request
                .unwrap();
            if !matches!(receipt.state.as_str(), "accepted" | "running") {
                assert_eq!(receipt.state, "completed", "{receipt:?}");
                break receipt;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

async fn wake(
    app: &mut App,
    owner: Entity,
    segments: &mut Segments,
    sender: &mpsc::SyncSender<Event>,
) {
    let events = recorded_events(segments, &recording("wake-statement"));
    assert!(events.iter().all(|event| matches!(event, Event::Notice(_))));
    let feedback = app
        .world()
        .get::<Listening>(owner)
        .unwrap()
        .capture
        .as_ref()
        .unwrap()
        .feedback
        .clone();
    feedback.lock().unwrap().window = segments.acoustic.as_ref().unwrap().deadline();
    for event in events {
        emit(sender, &feedback, event, None);
    }
    until(app, owner, |listening| {
        listening.saved.stage == cell::speech::ListeningStage::RequestWindow
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Consumes native provider usage; protected connection variables and LINCE_SPEECH_TEST_CACHE required; synthetic PCM enters the real acoustic gate and native controls without room capture"]
async fn actual_native_controls_use_authenticated_voice_conversation_and_natural_ending() {
    verify(false, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Consumes native provider usage; protected connection variables, LINCE_SPEECH_TEST_CACHE, LINCE_VOICE_TEST_ASSETS and a default speaker required; synthetic input through native controls"]
async fn actual_native_controls_speak_authenticated_replies_and_resume_unwoken_followups() {
    verify(true, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Consumes native provider usage; protected connection variables, LINCE_SPEECH_TEST_CACHE, LINCE_VOICE_TEST_ASSETS and a default speaker required; synthetic request through native controls"]
async fn actual_native_stop_during_spoken_reply_keeps_capture_off_after_playback() {
    verify(true, true).await;
}

fn deliver(
    app: &mut App,
    owner: Entity,
    segments: &mut Segments,
    sender: &mpsc::SyncSender<Event>,
    name: &str,
) {
    feed(&recording(name), segments, Some(sender), None, owner);
    app.world()
        .get::<Listening>(owner)
        .unwrap()
        .capture
        .as_ref()
        .unwrap()
        .feedback
        .lock()
        .unwrap()
        .window = segments.acoustic.as_ref().unwrap().deadline();
}

async fn spoken_resume(
    app: &mut App,
    owner: Entity,
    profile: &cell::fiote_communication::speech::wake::Profile,
    generation: u64,
    stopped: &AtomicBool,
) -> (Segments, mpsc::SyncSender<Event>, Arc<AtomicBool>, u64) {
    until(app, owner, |listening| listening.saved.paused).await;
    assert!(
        app.world()
            .get::<Listening>(owner)
            .unwrap()
            .capture
            .is_none()
    );
    assert!(stopped.load(Ordering::Acquire));
    until(app, owner, |listening| {
        listening.saved.enabled
            && !listening.saved.paused
            && listening.saved.generation != generation
            && listening.saved.conversation.is_some()
            && listening.pending.is_none()
    })
    .await;
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.starts_with("Spoken reply finished."))
    );
    assert!(!crate::native::sound::speaking(app.world_mut()));
    let generation = app
        .world()
        .get::<Listening>(owner)
        .unwrap()
        .saved
        .generation;
    let (capture, sender, stopped) = fake_capture();
    app.world_mut().get_mut::<Listening>(owner).unwrap().capture = Some(capture);
    let mut segments = Segments::new(generation);
    let mut gate = acoustic::Gate::new(profile.clone(), generation).unwrap();
    gate.set_conversation(true);
    segments.acoustic = Some(gate);
    (segments, sender, stopped, generation)
}

async fn verify(spoken: bool, stop_during_playback: bool) {
    let (mut settings, credential) = native_connection::existing_connection().await;
    let started = Instant::now();
    let (mut app, owner, fixture, script, directory, record) =
        super::super::super::super::tests::fixture_named("Voice test").await;
    if std::env::var_os("LINCE_VOICE_TEST_TRACE").is_some() {
        app.insert_resource(Trace {
            started,
            owner,
            stage: String::new(),
            speaking: false,
            output: 0,
        });
    }
    let mut runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    let store = runtime.store.clone();
    let host = Arc::new(
        cell::fiote::Host::open(
            runtime.engine.clone(),
            directory.path().join("native-settings"),
        )
        .await
        .unwrap(),
    );
    settings.enabled = true;
    settings.directory = directory.path().join("workspace");
    std::fs::create_dir(&settings.directory).unwrap();
    host.handle(FioteRequest::Configure {
        record: record.clone(),
        settings,
        api_key: Some(credential),
        password: Some(cell::FioteSecret("fixture-password".into())),
    })
    .await
    .unwrap();
    let speech = Arc::new(
        cell::speech::Host::open(directory.path().join("speech.json"))
            .unwrap()
            .with_fiote(Some(host.clone())),
    );
    let references = ["wake-statement", "wake-question", "wake-exclamation"].map(|name| {
        cell::fiote_communication::speech::decode_audio(&STANDARD.encode(recording(name))).unwrap()
    });
    let profile = cell::fiote_communication::speech::wake::Profile::enroll(&references).unwrap();
    speech
        .handle(Request::ConfigureWake {
            profile: Some(profile.clone()),
        })
        .await
        .unwrap();
    let local = speech
        .handle(Request::Inspect)
        .await
        .unwrap()
        .local
        .unwrap();
    let archive = std::path::PathBuf::from(std::env::var_os("LINCE_SPEECH_TEST_CACHE").unwrap())
        .join(format!("{}-{}", local.model, local.revision))
        .join("phonon-2.bps.tar.zst");
    let job = nucleus::new_uid("speech");
    speech
        .handle(Request::InstallLocal {
            job: job.clone(),
            source: Some(archive),
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let result = speech
                .handle(Request::Poll { job: job.clone() })
                .await
                .unwrap()
                .job
                .unwrap();
            if !result.pending {
                assert!(result.error.is_none(), "{result:?}");
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    runtime.fiote = Some(host.clone());
    runtime.speech = Some(speech.clone());
    app.insert_resource(crate::native::app::CellHandle(runtime.clone()));
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
    click(app.world_mut(), "Voice test");
    click(app.world_mut(), "Hey Fiote wake mode");
    if spoken {
        app.insert_resource(crate::native::sound::Audio::open(
            directory.path().to_path_buf(),
            None,
        ));
        app.add_plugins(crate::native::sound::SoundPlugin);
        let input = app
            .world_mut()
            .query::<(Entity, &bevy::a11y::AccessibilityNode)>()
            .iter(app.world())
            .find(|(_, node)| {
                node.label()
                    == Some(
                        "Compatible Kokoro voice directory (kokoro.btm, voices.btv, lexicon.btl)",
                    )
            })
            .map(|(entity, _)| entity)
            .unwrap();
        app.world_mut()
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text(&std::env::var("LINCE_VOICE_TEST_ASSETS").unwrap());
        click(app.world_mut(), "Import English voice");
        tokio::time::timeout(Duration::from_secs(180), async {
            loop {
                tick(&mut app);
                if app
                    .world_mut()
                    .query::<&Text>()
                    .iter(app.world())
                    .any(|text| {
                        text.0.starts_with("English voice: installed")
                            && !text.0.contains("working")
                    })
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        click(app.world_mut(), "Toggle automatic spoken replies");
    }
    let (capture, mut sender, mut stopped) = fake_capture();
    app.world_mut().get_mut::<Listening>(owner).unwrap().capture = Some(capture);
    click(app.world_mut(), "Start listening");
    settle(&mut app).await;
    assert!(app.world().get::<Listening>(owner).unwrap().saved.enabled);
    println!(
        "native_ui_setup_completed_ms={}",
        started.elapsed().as_millis()
    );
    let mut generation = app
        .world()
        .get::<Listening>(owner)
        .unwrap()
        .saved
        .generation;
    let mut segments = Segments::new(generation);
    segments.acoustic = Some(acoustic::Gate::new(profile.clone(), generation).unwrap());
    idle(&mut app, &mut segments).await;
    assert!(
        recorded_events(&mut segments, &recording("background"))
            .iter()
            .all(|event| matches!(event, Event::Notice(_)))
    );
    println!("native_ui_first_input_ms={}", started.elapsed().as_millis());
    wake(&mut app, owner, &mut segments, &sender).await;
    deliver(&mut app, owner, &mut segments, &sender, "first");
    until(&mut app, owner, |listening| {
        listening.saved.receipt.is_some()
    })
    .await;
    println!(
        "native_ui_first_accepted_ms={}",
        started.elapsed().as_millis()
    );
    let request = app
        .world()
        .get::<Listening>(owner)
        .unwrap()
        .saved
        .receipt
        .clone()
        .unwrap();
    let first = completed(&mut app, &host, &request.request_id).await;
    let message = store::records::get(&store.pool, first.message.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(
        message.body.to_lowercase().contains("sapphire"),
        "{}",
        message.body
    );
    println!(
        "native_ui_first_completed_ms={}",
        started.elapsed().as_millis()
    );
    if stop_during_playback {
        until(&mut app, owner, |listening| listening.saved.paused).await;
        tokio::time::timeout(Duration::from_secs(180), async {
            loop {
                tick(&mut app);
                if app
                    .world_mut()
                    .query::<&Text>()
                    .iter(app.world())
                    .any(|text| {
                        text.0.starts_with("English voice: installed")
                            && text.0.contains(" · speaking")
                    })
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(stopped.load(Ordering::Acquire));
        click(app.world_mut(), "Stop listening");
        settle(&mut app).await;
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                tick(&mut app);
                let listening = app.world().get::<Listening>(owner).unwrap();
                assert!(
                    !listening.saved.enabled && !listening.wanted && listening.capture.is_none()
                );
                if app
                    .world_mut()
                    .query::<&Text>()
                    .iter(app.world())
                    .any(|text| text.0.starts_with("Spoken reply finished."))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let after_playback = Instant::now() + Duration::from_millis(750);
        while Instant::now() < after_playback {
            tick(&mut app);
            let listening = app.world().get::<Listening>(owner).unwrap();
            assert!(!listening.saved.enabled && !listening.wanted && listening.capture.is_none());
            assert!(app.world().get::<Resume>(owner).is_none());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(script.requests.lock().unwrap().is_empty());
        speech.shutdown().await;
        host.stop_all().await;
        fixture.stop_all().await;
        println!(
            "native_ui_stop_during_playback_completed_ms={}",
            started.elapsed().as_millis()
        );
        return;
    }
    if spoken {
        (segments, sender, stopped, generation) =
            spoken_resume(&mut app, owner, &profile, generation, &stopped).await;
        println!(
            "native_ui_first_playback_completed_ms={}",
            started.elapsed().as_millis()
        );
    }
    until(&mut app, owner, |listening| {
        listening.saved.conversation.is_some()
    })
    .await;
    segments.acoustic.as_mut().unwrap().set_conversation(true);
    println!(
        "native_ui_followup_input_ms={}",
        started.elapsed().as_millis()
    );
    deliver(&mut app, owner, &mut segments, &sender, "followup");
    until(&mut app, owner, |listening| {
        listening
            .saved
            .receipt
            .as_ref()
            .is_some_and(|receipt| receipt.request_id != first.request_id)
    })
    .await;
    let request = app
        .world()
        .get::<Listening>(owner)
        .unwrap()
        .saved
        .receipt
        .clone()
        .unwrap();
    let second = completed(&mut app, &host, &request.request_id).await;
    println!(
        "native_ui_followup_completed_ms={}",
        started.elapsed().as_millis()
    );
    assert_eq!(second.target, first.target);
    assert_eq!(second.thread, first.thread);
    let reply = store::records::get(&store.pool, second.reply.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(
        reply.body.to_lowercase().contains("sapphire"),
        "{}",
        reply.body
    );
    if spoken {
        (segments, sender, stopped, _) =
            spoken_resume(&mut app, owner, &profile, generation, &stopped).await;
        println!(
            "native_ui_followup_playback_completed_ms={}",
            started.elapsed().as_millis()
        );
    }
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    until(&mut app, owner, |listening| {
        !listening.saved.pending && listening.pending.is_none()
    })
    .await;
    deliver(&mut app, owner, &mut segments, &sender, "ending");
    until(&mut app, owner, |listening| {
        listening.saved.conversation.is_none()
            && listening.saved.stage == cell::speech::ListeningStage::Ignored
            && listening.pending.is_none()
    })
    .await;
    segments.acoustic.as_mut().unwrap().set_conversation(false);
    assert!(
        recorded_events(&mut segments, &recording("background"))
            .iter()
            .all(|event| matches!(event, Event::Notice(_)))
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert!(script.requests.lock().unwrap().is_empty());
    if spoken {
        click(app.world_mut(), "Mute spoken replies");
    }
    click(app.world_mut(), "Stop listening");
    settle(&mut app).await;
    assert!(stopped.load(Ordering::Acquire));
    assert!(
        !speech
            .handle(Request::Inspect)
            .await
            .unwrap()
            .listening
            .enabled
    );
    speech.shutdown().await;
    host.stop_all().await;
    fixture.stop_all().await;
    println!(
        "native_ui_authenticated_conversation_completed_ms={} resources={}",
        started.elapsed().as_millis(),
        measurements()
    );
}
