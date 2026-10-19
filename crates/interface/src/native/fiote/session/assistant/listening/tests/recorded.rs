use super::*;

mod frozen;
mod live;
mod performance;

mod speech_reference {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/support/speech_reference.rs"
    ));
}
use base64::{Engine as _, engine::general_purpose::STANDARD};

fn tick(app: &mut App) {
    let incoming = app
        .world_mut()
        .non_send_mut::<crate::native::cell_bridge::CellBridge>()
        .incoming
        .try_recv();
    if let Ok(message) = incoming {
        app.world_mut()
            .write_message(crate::native::cell_bridge::CellMessage(message));
    }
    app.update();
    live::trace(app);
}

async fn until(app: &mut App, owner: Entity, ready: impl Fn(&Listening) -> bool) {
    let result = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            tick(app);
            let listening = app.world().get::<Listening>(owner).unwrap();
            assert_ne!(
                listening.saved.stage,
                cell::speech::ListeningStage::Degraded,
                "Real listening failed: {:?}",
                listening.saved
            );
            assert!(listening.failure.is_none(), "{:?}", listening.failure);
            if ready(listening) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let listening = app.world().get::<Listening>(owner).unwrap();
    assert!(
        result.is_ok(),
        "Listening did not finish: {:?}; UI: {}",
        listening.saved,
        app.world().get::<Text>(listening.label).unwrap().0
    );
}

fn recorded_segment(segments: &mut Segments, bytes: &[u8]) -> Event {
    let all = recorded_events(segments, bytes);
    let notices = all
        .iter()
        .filter_map(|event| match event {
            Event::Notice(text) => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut events: Vec<_> = all
        .into_iter()
        .filter(|event| !matches!(event, Event::Notice(_)))
        .collect();
    assert_eq!(
        events.len(),
        1,
        "One recording must finalize exactly once: {notices:?}"
    );
    events.pop().unwrap()
}

fn recorded_events(segments: &mut Segments, bytes: &[u8]) -> Vec<Event> {
    let samples = cell::fiote_communication::speech::decode_audio(&STANDARD.encode(bytes)).unwrap();
    let mut events = Vec::new();
    for chunk in samples.chunks(1600) {
        events.extend(
            segments
                .push(&crate::native::sound::library::Clip {
                    rate: 16_000,
                    samples: chunk.to_vec(),
                })
                .unwrap(),
        );
    }
    for _ in 0..15 {
        events.extend(
            segments
                .push(&crate::native::sound::library::Clip {
                    rate: 16_000,
                    samples: vec![0.0; 1600],
                })
                .unwrap(),
        );
    }
    events
}

#[tokio::test]
#[ignore = "Requires installed Phonon, goforward and speech_fixtures --addressed recordings"]
async fn actual_recorded_address_uses_native_controls_real_asr_normal_delivery_and_one_reply() {
    verify_reference(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fiote/tests/fixtures/english-addressed-reference.json"
        )),
        false,
    )
    .await;
}

#[tokio::test]
#[ignore = "Requires installed Phonon, goforward and speech_fixtures --command recordings"]
async fn actual_composite_command_uses_native_controls_real_asr_and_normal_delivery() {
    verify_reference(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fiote/tests/fixtures/english-addressed-command-reference.json"
        )),
        false,
    )
    .await;
}

#[tokio::test]
#[ignore = "Plays public speech through default output and captures the real default microphone; requires a working audio path"]
async fn actual_audio_devices_deliver_a_spoken_command_through_native_listening() {
    tokio::time::timeout(Duration::from_secs(90), verify_reference(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/fixtures/english-addressed-command-reference.json"
    )), true))
    .await
    .expect("The default input did not complete the public spoken command within 90 seconds; check input/output routing, audibility and gain.");
}

async fn microphone_ready(app: &mut App, owner: Entity) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tick(app);
            let listening = app.world().get::<Listening>(owner).unwrap();
            assert!(listening.failure.is_none(), "{:?}", listening.failure);
            if app
                .world()
                .get::<Text>(listening.label)
                .unwrap()
                .0
                .starts_with("Microphone active")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("The native microphone did not report that capture started.");
}

fn feed(
    source: &[u8],
    segments: &mut Segments,
    sender: Option<&mpsc::SyncSender<Event>>,
    output: Option<&crate::native::sound::device::Output>,
    owner: Entity,
) -> Option<u64> {
    if let Some(output) = output {
        let samples =
            cell::fiote_communication::speech::decode_audio(&STANDARD.encode(source)).unwrap();
        output.play(
            owner,
            Arc::new(crate::native::sound::library::Clip {
                samples,
                rate: 16_000,
            }),
            0.5,
            true,
        );
        None
    } else {
        let event = recorded_segment(segments, source);
        let id = match &event {
            Event::Segment { id, .. } | Event::WokenSegment { id, .. } => *id,
            _ => panic!("Recorded input did not produce a speech segment"),
        };
        sender.unwrap().send(event).unwrap();
        Some(id)
    }
}

fn measurements() -> serde_json::Value {
    let memory = std::fs::read_to_string("/proc/self/status")
        .ok()
        .map(|status| {
            status
                .lines()
                .filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
    let compilers = std::fs::read_dir("/proc").ok().map(|entries| {
        entries
            .filter_map(Result::ok)
            .filter(|entry| {
                std::fs::read_to_string(entry.path().join("comm"))
                    .is_ok_and(|name| name.trim() == "rustc")
            })
            .count()
    });
    serde_json::json!({"memory":memory,"observed_compilers":compilers,"load":std::fs::read_to_string("/proc/loadavg").ok()})
}

async fn verify_reference(reference: &str, acoustic: bool) {
    verify_voice_reference(reference, acoustic, VoiceCase::Addressed).await;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VoiceCase {
    Addressed,
    WakeCombined,
    WakeSeparate,
}

#[tokio::test]
#[ignore = "Requires installed Phonon, independent speech references and LINCE_VOICE_TEST_OUTPUT wake.wav"]
async fn actual_native_wake_accepts_combined_request_with_normal_delivery() {
    let reference = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/fixtures/english-addressed-command-reference.json"
    ));
    verify_voice_reference(reference, false, VoiceCase::WakeCombined).await;
}

#[tokio::test]
#[ignore = "Requires installed Phonon, independent speech references and LINCE_VOICE_TEST_OUTPUT wake.wav"]
async fn actual_native_wake_accepts_separate_request_with_normal_delivery() {
    let reference = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/fixtures/english-addressed-command-reference.json"
    ));
    verify_voice_reference(reference, false, VoiceCase::WakeSeparate).await;
}

fn generated_wake() -> Vec<u8> {
    generated_voice("wake.wav")
}

fn generated_voice(file: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(
        std::env::var("LINCE_VOICE_TEST_OUTPUT").expect("Choose generated public voice recordings"),
    )
    .join(file);
    let reader = hound::WavReader::open(path).unwrap();
    assert_eq!(reader.spec().sample_rate, 24_000);
    assert_eq!(reader.spec().channels, 1);
    let clip = crate::native::sound::library::Clip {
        samples: reader
            .into_samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / 32768.0)
            .collect(),
        rate: 24_000,
    };
    STANDARD
        .decode(crate::native::speech::encode(clip).unwrap())
        .unwrap()
}

async fn verify_voice_reference(reference: &str, acoustic: bool, voice_case: VoiceCase) {
    let reference: serde_json::Value = serde_json::from_str(reference).unwrap();
    let recordings = std::path::PathBuf::from(
        std::env::var_os("LINCE_SPEECH_TEST_REFERENCES_DIR")
            .expect("Choose prepared independent recordings"),
    );
    let addressed = std::fs::read(recordings.join(reference["file"].as_str().unwrap())).unwrap();
    assert_eq!(
        nucleus::message::digest(&addressed),
        reference["sha256"].as_str().unwrap()
    );
    let license = std::fs::read(
        recordings.join(
            reference["license_file"]
                .as_str()
                .unwrap_or("LICENSE-Computer-Kris-Gesling.txt"),
        ),
    )
    .unwrap();
    assert_eq!(
        nucleus::message::digest(&license),
        reference["license_sha256"].as_str().unwrap()
    );
    let ordinary = std::fs::read(recordings.join("goforward.wav")).unwrap();
    assert_eq!(
        nucleus::message::digest(&ordinary),
        "97ec638609b9c7eeb616764b4e5713d7ed2d0a8e1a2f8246631d9074d47aaf11"
    );
    let (mut app, owner, fiote, script, directory, record) =
        super::super::super::tests::fixture_named("Computer").await;
    fiote
        .handle(FioteRequest::Behavior {
            record: record.clone(),
            prompt_parent: None,
            run_assigned: false,
        })
        .await
        .unwrap();
    let candidates = fiote.discover(0, 30).await.unwrap();
    assert!(candidates.next_offset.is_none());
    for candidate in candidates.fiotes {
        if candidate.record != record {
            fiote
                .handle(FioteRequest::Behavior {
                    record: candidate.record,
                    prompt_parent: Some(record.clone()),
                    run_assigned: false,
                })
                .await
                .unwrap();
        }
    }
    let selected = fiote.intake_selection().await.unwrap().unwrap();
    assert_eq!(selected.record, record);
    assert_eq!(selected.title, "Computer");
    let speech = Arc::new(
        cell::speech::Host::open(directory.path().join("speech.json"))
            .unwrap()
            .with_fiote(Some(fiote.clone())),
    );
    let wake_profile = if voice_case != VoiceCase::Addressed {
        let samples =
            cell::fiote_communication::speech::decode_audio(&STANDARD.encode(generated_wake()))
                .unwrap();
        let profile = cell::fiote_communication::speech::wake::Profile::enroll(&[
            samples.clone(),
            samples.clone(),
            samples,
        ])
        .unwrap();
        speech
            .handle(Request::ConfigureWake {
                profile: Some(profile.clone()),
            })
            .await
            .unwrap();
        Some(profile)
    } else {
        None
    };
    let model = speech
        .handle(Request::Inspect)
        .await
        .unwrap()
        .local
        .unwrap();
    let archive = std::path::PathBuf::from(
        std::env::var_os("LINCE_SPEECH_TEST_CACHE").expect("Choose installed model cache"),
    )
    .join(format!("{}-{}", model.model, model.revision))
    .join("phonon-2.bps.tar.zst");
    let setup = nucleus::new_uid("speech");
    speech
        .handle(Request::InstallLocal {
            job: setup.clone(),
            source: Some(archive),
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let result = speech
                .handle(Request::Poll { job: setup.clone() })
                .await
                .unwrap()
                .job
                .unwrap();
            if !result.pending {
                assert!(result.error.is_none(), "{result:?}");
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    runtime.speech = Some(speech.clone());
    app.world_mut()
        .insert_non_send(crate::native::cell_bridge::connect(
            runtime,
            crate::native::wake::WakeSignal::new(|| {}),
        ));
    let store = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .store
        .clone();
    let extra = std::env::var("LINCE_SPEECH_TEST_EXTRA_WARM_RUNS")
        .map(|value| {
            value
                .parse::<usize>()
                .expect("Choose a bounded warm-run count")
        })
        .unwrap_or(0);
    assert!(extra <= 20);
    assert!(!acoustic || extra == 0);
    let mut accepted_times = Vec::new();
    for (index, explicit) in [false, true]
        .into_iter()
        .chain(std::iter::repeat_n(true, extra))
        .enumerate()
    {
        let previous = app
            .world()
            .get::<Listening>(owner)
            .unwrap()
            .saved
            .receipt
            .as_ref()
            .map(|receipt| receipt.request_id.clone());
        if explicit {
            click(app.world_mut(), "Computer");
        }
        assert_eq!(
            app.world().get::<Entry>(owner).unwrap().target.as_deref(),
            explicit.then_some(record.as_str())
        );
        let send = if acoustic {
            if let Ok(name) = std::env::var("LINCE_SPEECH_TEST_MICROPHONE") {
                click(app.world_mut(), "Choose listening microphone");
                click(app.world_mut(), &name);
            }
            None
        } else {
            let (capture, send, _) = fake_capture();
            app.world_mut().get_mut::<Listening>(owner).unwrap().capture = Some(capture);
            Some(send)
        };
        click(
            app.world_mut(),
            if voice_case == VoiceCase::Addressed {
                "Addressed requests"
            } else {
                "Hey Fiote wake mode"
            },
        );
        click(app.world_mut(), "Start listening");
        settle(&mut app).await;
        let saved = &app.world().get::<Listening>(owner).unwrap().saved;
        assert!(saved.enabled, "{saved:?}");
        assert_eq!(
            saved.mode,
            if voice_case == VoiceCase::Addressed {
                ListeningMode::Addressed
            } else {
                ListeningMode::Wake
            }
        );
        let stopped = app
            .world()
            .get::<Listening>(owner)
            .unwrap()
            .capture
            .as_ref()
            .unwrap()
            .stop
            .clone();
        assert!(!stopped.load(Ordering::Acquire));
        let generation = saved.generation;
        let mut segments = Segments::new(generation);
        if let Some(profile) = &wake_profile {
            segments.acoustic = Some(acoustic::Gate::new(profile.clone(), generation).unwrap());
        }
        let output = if acoustic {
            microphone_ready(&mut app, owner).await;
            Some(
                crate::native::sound::device::Output::open()
                    .expect("The default audio output must be available"),
            )
        } else {
            None
        };
        if !explicit {
            let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
                .fetch_one(&store.pool)
                .await
                .unwrap();
            let started = Instant::now();
            if voice_case == VoiceCase::Addressed {
                let _ = feed(
                    &ordinary,
                    &mut segments,
                    send.as_ref(),
                    output.as_ref(),
                    owner,
                );
                until(&mut app, owner, |listening| {
                    listening.saved.stage == cell::speech::ListeningStage::Ignored
                        && listening.pending.is_none()
                })
                .await;
            } else {
                let events = recorded_events(&mut segments, &ordinary);
                assert!(
                    events.iter().all(|event| matches!(event, Event::Notice(_))),
                    "Unaddressed speech cannot cross the acoustic gate"
                );
                assert!(
                    !speech
                        .handle(Request::Inspect)
                        .await
                        .unwrap()
                        .local
                        .unwrap()
                        .loaded
                );
            }
            println!(
                "{}",
                serde_json::json!({"phase":"ordinary_cold","elapsed_ms":started.elapsed().as_millis(),"resources":measurements()})
            );
            assert!(script.requests.lock().unwrap().is_empty());
            assert!(app.world().get::<Entry>(owner).unwrap().receipt.is_none());
            let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
                .fetch_one(&store.pool)
                .await
                .unwrap();
            assert_eq!(before, after);
        }
        let started = Instant::now();
        if voice_case == VoiceCase::WakeSeparate {
            let events = recorded_events(&mut segments, &generated_wake());
            assert!(
                events.iter().all(|event| matches!(event, Event::Notice(_))),
                "Wake-only recording cannot create a request"
            );
            let window = segments.acoustic.as_ref().and_then(|gate| gate.deadline());
            assert!(
                window.is_some(),
                "Wake-only recording did not leave a request window: {:?}",
                events
                    .iter()
                    .filter_map(|event| match event {
                        Event::Notice(detail) => Some(detail.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            );
            let feedback = app
                .world()
                .get::<Listening>(owner)
                .unwrap()
                .capture
                .as_ref()
                .unwrap()
                .feedback
                .clone();
            feedback.lock().unwrap().window = window;
            for event in events {
                emit(send.as_ref().unwrap(), &feedback, event, None);
            }
            tick(&mut app);
            println!(
                "wake_window_remaining_ms={} visible_stage={:?}",
                feedback
                    .lock()
                    .unwrap()
                    .window
                    .map(|deadline| deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis())
                    .unwrap_or(0),
                app.world().get::<Listening>(owner).unwrap().saved.stage
            );
            tokio::time::timeout(
                Duration::from_secs(15),
                until(&mut app, owner, |listening| {
                    listening.saved.stage == cell::speech::ListeningStage::RequestWindow
                }),
            )
            .await
            .expect("A detected wake must show its ten-second request window promptly");
            assert_eq!(script.requests.lock().unwrap().len(), index);
        }
        let combined = if voice_case == VoiceCase::WakeCombined {
            Some(generated_voice("command.wav"))
        } else {
            None
        };
        let recorded_id = feed(
            if voice_case == VoiceCase::WakeSeparate {
                &ordinary
            } else if let Some(combined) = &combined {
                combined
            } else {
                &addressed
            },
            &mut segments,
            send.as_ref(),
            output.as_ref(),
            owner,
        );
        if voice_case != VoiceCase::Addressed {
            app.world()
                .get::<Listening>(owner)
                .unwrap()
                .capture
                .as_ref()
                .unwrap()
                .feedback
                .lock()
                .unwrap()
                .window = segments.acoustic.as_ref().and_then(|gate| gate.deadline());
        }
        until(&mut app, owner, |listening| {
            listening
                .saved
                .receipt
                .as_ref()
                .is_some_and(|receipt| Some(receipt.request_id.as_str()) != previous.as_deref())
        })
        .await;
        let accepted_ms = started.elapsed().as_millis();
        accepted_times.push(accepted_ms);
        let receipt = app
            .world()
            .get::<Listening>(owner)
            .unwrap()
            .saved
            .receipt
            .clone()
            .unwrap();
        assert_eq!(receipt.target, record);
        let message = store::records::get(&store.pool, receipt.message.as_ref().unwrap())
            .await
            .unwrap()
            .unwrap();
        let expected = if voice_case == VoiceCase::Addressed {
            reference["expected"].as_str().unwrap()
        } else {
            "go forward ten meters"
        };
        let matched = speech_reference::words(&message.body) == speech_reference::words(expected);
        assert!(
            matched,
            "The delivered command did not match the independent reference; microphone contents were omitted from diagnostics."
        );
        let completed = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let result = fiote
                    .handle(FioteRequest::AssistantInspect {
                        request_id: receipt.request_id.clone(),
                    })
                    .await
                    .unwrap()
                    .assistant_request
                    .unwrap();
                if result.state == "completed" {
                    break result;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            store::records::get(&store.pool, completed.reply.as_ref().unwrap())
                .await
                .unwrap()
                .unwrap()
                .body,
            "Hello from the native assistant workflow"
        );
        assert_eq!(script.requests.lock().unwrap().len(), index + 1);
        if !acoustic {
            let listening = app.world().get::<Listening>(owner).unwrap();
            let duplicate = speech
                .handle(Request::ListeningSegment {
                    owner: listening.owner.clone(),
                    generation,
                    segment: recorded_id.unwrap(),
                    audio: cell::FioteSecret(STANDARD.encode(&addressed)),
                })
                .await;
            let Err(duplicate) = duplicate else {
                panic!("A repeated recorded segment was accepted");
            };
            assert!(duplicate.contains("already been seen"), "{duplicate}");
        }
        println!(
            "{}",
            serde_json::json!({"acoustic":acoustic,"explicit":explicit,"actual":message.body,"expected":expected,"request_id":receipt.request_id,"provider":"fixture","state":completed.state,"accepted_ms":accepted_ms,"completed_ms":started.elapsed().as_millis(),"resources":measurements()})
        );
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
    }
    println!(
        "{}",
        serde_json::json!({"phase":"summary","first_accepted_ms":accepted_times[0],"subsequent_accepted_ms":&accepted_times[1..],"resources":measurements()})
    );
    speech.shutdown().await;
    println!(
        "{}",
        serde_json::json!({"phase":"shutdown","resources":measurements()})
    );
}
