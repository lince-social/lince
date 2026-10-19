use super::*;
use cell::fiote_communication::speech::voice;
use std::{collections::VecDeque, path::PathBuf, sync::atomic::AtomicU64};

const TEXT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../fiote/tests/fixtures/long-request.txt"
));

struct Fixture {
    app: App,
    owner: Entity,
    speech: Arc<cell::speech::Host>,
    host: Arc<cell::fiote::Host>,
    script: Arc<super::super::super::super::tests::Script>,
    directory: tempfile::TempDir,
    samples: Vec<f32>,
    profile: cell::fiote_communication::speech::wake::Profile,
    output: PathBuf,
}

async fn poll(speech: &cell::speech::Host, job: &str, spoken: bool) {
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let status = speech
                .handle(if spoken {
                    Request::Voice {
                        request: voice::Request::Poll { job: job.into() },
                    }
                } else {
                    Request::Poll { job: job.into() }
                })
                .await
                .unwrap();
            if spoken {
                let result = status.voice.job.unwrap();
                if !result.pending {
                    assert!(result.error.is_none(), "{result:?}");
                    return;
                }
            } else {
                let result = status.job.unwrap();
                if !result.pending {
                    assert!(result.error.is_none(), "{result:?}");
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}

async fn fixture(rendered: bool) -> Fixture {
    let output =
        PathBuf::from(std::env::var_os("LINCE_VOICE_TEST_REPORT_DIR").unwrap()).join(format!(
            "{}-{}",
            if rendered { "rendered" } else { "near-limit" },
            nucleus::new_uid("run")
        ));
    std::fs::create_dir_all(&output).unwrap();
    let started = Instant::now();
    let mut preparation = Vec::new();
    let mut observe = |stage: &str| {
        let observation = serde_json::json!({"stage":stage,"elapsed_ms":started.elapsed().as_millis(),"resources":measurements()});
        println!("{observation}");
        preparation.push(observation);
        std::fs::write(
            output.join("preparation.json"),
            serde_json::to_vec_pretty(&preparation).unwrap(),
        )
        .unwrap();
    };
    observe("before_setup");
    let presentation = rendered.then(|| {
        let plugins = DefaultPlugins
            .build()
            .disable::<bevy::log::LogPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
            .set(bevy::render::RenderPlugin {
                render_creation: bevy::render::settings::WgpuSettings {
                    memory_hints: bevy::render::settings::MemoryHints::MemoryUsage,
                    ..default()
                }
                .into(),
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .set(bevy::winit::WinitPlugin {
                run_on_any_thread: true,
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Lince · Fiote performance laboratory".into(),
                    resolution: (1200, 900).into(),
                    ..default()
                }),
                ..default()
            });
        let assets = output.join("assets");
        let mut app = crate::native::app::presentation_app(assets, plugins);
        let wake = crate::native::wake::from_proxy(
            app.world().resource::<bevy::winit::EventLoopProxyWrapper>(),
        );
        app.insert_resource(wake);
        app.add_systems(Startup, crate::native::app::canvas);
        app
    });
    let (mut app, owner, host, script, directory, _) =
        super::super::super::super::tests::fixture_named_in("Performance test", presentation).await;
    observe("presentation_ready");
    let speech = Arc::new(
        cell::speech::Host::open(directory.path().join("speech.json"))
            .unwrap()
            .with_fiote(Some(host.clone())),
    );
    let cache = PathBuf::from(std::env::var_os("LINCE_SPEECH_TEST_CACHE").unwrap());
    let local = speech
        .handle(Request::Inspect)
        .await
        .unwrap()
        .local
        .unwrap();
    let job = nucleus::new_uid("speech");
    speech
        .handle(Request::InstallLocal {
            job: job.clone(),
            source: Some(
                cache
                    .join(format!("{}-{}", local.model, local.revision))
                    .join("phonon-2.bps.tar.zst"),
            ),
        })
        .await
        .unwrap();
    poll(&speech, &job, false).await;
    observe("recognition_assets_installed");
    let job = nucleus::new_uid("speech");
    speech
        .handle(Request::Voice {
            request: voice::Request::Install {
                job: job.clone(),
                source: Some(PathBuf::from(
                    std::env::var_os("LINCE_VOICE_TEST_ASSETS").unwrap(),
                )),
            },
        })
        .await
        .unwrap();
    poll(&speech, &job, true).await;
    observe("voice_assets_installed");
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fiote/tests/fixtures");
    let reference: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixtures.join("long-request-reference.json")).unwrap(),
    )
    .unwrap();
    let bytes = std::fs::read(fixtures.join("long-request.wav")).unwrap();
    assert_eq!(reference["text"].as_str().unwrap(), TEXT.trim());
    assert_eq!(
        reference["sha256"].as_str().unwrap(),
        nucleus::message::digest(&bytes)
    );
    let samples =
        cell::fiote_communication::speech::decode_audio(&STANDARD.encode(&bytes)).unwrap();
    assert_eq!(samples.len(), 291_200);
    observe("verified_long_fixture_loaded");
    std::fs::write(output.join("request.wav"), &bytes).unwrap();
    std::fs::write(
        output.join("input.json"),
        serde_json::to_vec_pretty(&reference).unwrap(),
    )
    .unwrap();
    let references = ["wake-statement", "wake-question", "wake-exclamation"].map(|name| {
        cell::fiote_communication::speech::decode_audio(&STANDARD.encode(live::recording(name)))
            .unwrap()
    });
    let profile = cell::fiote_communication::speech::wake::Profile::enroll(&references).unwrap();
    speech
        .handle(Request::ConfigureWake {
            profile: Some(profile.clone()),
        })
        .await
        .unwrap();
    let mut connected = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    connected.speech = Some(speech.clone());
    app.insert_resource(crate::native::app::CellHandle(connected.clone()));
    let wake = app
        .world()
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    app.world_mut()
        .insert_non_send(crate::native::cell_bridge::connect(
            connected,
            wake.clone()
                .unwrap_or_else(|| crate::native::wake::WakeSignal::new(|| {})),
        ));
    let cancellation =
        crate::native::cell_bridge::SpeechCancellation::for_owner(app.world(), owner);
    app.world_mut()
        .get_mut::<Listening>(owner)
        .unwrap()
        .cancellation = cancellation;
    app.insert_resource(crate::native::sound::Audio::open(
        directory.path().into(),
        wake,
    ));
    if !app.is_plugin_added::<crate::native::sound::SoundPlugin>() {
        app.add_plugins(crate::native::sound::SoundPlugin);
    }
    click(app.world_mut(), "Performance test");
    click(app.world_mut(), "Hey Fiote wake mode");
    click(app.world_mut(), "Check English voice");
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            tick(&mut app);
            if app
                .world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| {
                    text.0.starts_with("English voice: installed") && !text.0.contains("working")
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
    Fixture {
        app,
        owner,
        speech,
        host,
        script,
        directory,
        samples,
        profile,
        output,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Start,
    Enabling,
    Capture,
    Reply,
    Playback,
    Stopping,
    Finished,
}

#[derive(Resource)]
struct Run {
    owner: Entity,
    started: Instant,
    last_tick: Instant,
    max_update_gap_ms: u128,
    cpu_before: Option<u64>,
    reference_match: bool,
    output_active: bool,
    output_started_ms: Option<u128>,
    output_finished_ms: Option<u128>,
    stop_until: Instant,
    phase: Phase,
    chunks: VecDeque<Vec<f32>>,
    input_task: Option<std::thread::JoinHandle<u64>>,
    profile: cell::fiote_communication::speech::wake::Profile,
    sender: Option<mpsc::SyncSender<Event>>,
    stopped: Option<Arc<AtomicBool>>,
    script: Arc<super::super::super::super::tests::Script>,
    output: PathBuf,
    stages: Vec<serde_json::Value>,
    previous: String,
    frames: Arc<AtomicU64>,
    speech_frames: Option<u64>,
    rendered: bool,
    done: Arc<AtomicBool>,
}

fn receive(world: &mut World) {
    for _ in 0..64 {
        let message = world
            .non_send_mut::<crate::native::cell_bridge::CellBridge>()
            .incoming
            .try_recv();
        let Ok(message) = message else { break };
        world.write_message(crate::native::cell_bridge::CellMessage(message));
    }
}

fn capture(run: &mut Run, world: &World) {
    let mut chunks = std::mem::take(&mut run.chunks);
    let profile = run.profile.clone();
    let listening = world.get::<Listening>(run.owner).unwrap();
    let generation = listening.saved.generation;
    let feedback = listening.capture.as_ref().unwrap().feedback.clone();
    let sender = run.sender.as_ref().unwrap().clone();
    let stopped = run.stopped.as_ref().unwrap().clone();
    let frames = run.frames.clone();
    let output = run.output.clone();
    let started = run.started;
    run.input_task = Some(std::thread::spawn(move || {
        let mut segments = Segments::new(generation);
        segments.acoustic = Some(acoustic::Gate::new(profile, generation).unwrap());
        let clock = Instant::now();
        let mut fed = 0_u64;
        let mut max_lag_ms = 0_u128;
        while let Some(samples) = chunks.pop_front() {
            assert!(
                !stopped.load(Ordering::Acquire),
                "Capture stopped before finalization"
            );
            fed += samples.len() as u64;
            let deadline = clock + Duration::from_nanos(fed * 62_500);
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            max_lag_ms = max_lag_ms.max(
                Instant::now()
                    .saturating_duration_since(deadline)
                    .as_millis(),
            );
            let events = segments
                .push(&crate::native::sound::library::Clip {
                    rate: 16_000,
                    samples,
                })
                .unwrap();
            feedback.lock().unwrap().window = segments.acoustic.as_ref().unwrap().deadline();
            for event in events {
                let finalized = matches!(&event, Event::WokenSegment { .. });
                if finalized {
                    assert!(chunks.len() <= 15, "The long request was prematurely split");
                    let Event::WokenSegment { audio, .. } = &event else {
                        unreachable!()
                    };
                    let bytes = STANDARD.decode(audio).unwrap();
                    let reader = hound::WavReader::new(std::io::Cursor::new(&bytes)).unwrap();
                    assert!((288_000..=320_000).contains(&reader.duration()));
                    std::fs::write(output.join("captured.wav"), &bytes).unwrap();
                    std::fs::write(
                        output.join("capture.json"),
                        serde_json::to_vec_pretty(&serde_json::json!({
                            "audio_ms":reader.duration()/16,
                            "elapsed_ms":started.elapsed().as_millis(),
                            "sha256":nucleus::message::digest(&bytes),
                            "render_frames":frames.load(Ordering::Relaxed),
                            "max_input_lag_ms":max_lag_ms
                        }))
                        .unwrap(),
                    )
                    .unwrap();
                }
                assert!(emit(&sender, &feedback, event, None));
                if finalized {
                    return frames.load(Ordering::Relaxed);
                }
            }
        }
        panic!("The near-limit request never finalized");
    }));
}

fn drive(world: &mut World) {
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.after(Duration::from_millis(100));
    }
    let mut run = world.remove_resource::<Run>().unwrap();
    let now = Instant::now();
    if matches!(run.phase, Phase::Capture | Phase::Reply | Phase::Playback) {
        run.max_update_gap_ms = run
            .max_update_gap_ms
            .max(now.duration_since(run.last_tick).as_millis());
    }
    run.last_tick = now;
    assert!(
        run.started.elapsed() < Duration::from_secs(180),
        "Voice performance workflow timed out: {}",
        run.previous
    );
    let listening = world.get::<Listening>(run.owner).unwrap();
    assert!(listening.failure.is_none(), "{:?}", listening.failure);
    assert_ne!(
        listening.saved.stage,
        cell::speech::ListeningStage::Degraded,
        "{:?}",
        listening.saved
    );
    let current_stage = listening.saved.stage;
    let enabled = listening.saved.enabled;
    let paused = listening.saved.paused;
    let ready = listening.pending.is_none();
    let playback_finished = run.phase == Phase::Playback
        && world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.starts_with("Spoken reply finished."));
    let speaking = world.query::<&Text>().iter(world).any(|text| {
        text.0.starts_with("English voice: installed") && text.0.contains(" · speaking")
    });
    if speaking != run.output_active {
        let elapsed = run.started.elapsed().as_millis();
        if speaking {
            assert!(run.output_started_ms.is_none(), "Duplicate spoken output");
            run.output_started_ms = Some(elapsed);
        } else {
            run.output_finished_ms = Some(elapsed);
        }
        run.output_active = speaking;
        let observation = serde_json::json!({"stage":if speaking { "audio_output_queued" } else { "audio_output_finished" },"elapsed_ms":elapsed,"render_frames":run.frames.load(Ordering::Relaxed),"resources":measurements()});
        println!("{observation}");
        run.stages.push(observation);
        std::fs::write(
            run.output.join("stages.json"),
            serde_json::to_vec_pretty(&run.stages).unwrap(),
        )
        .unwrap();
    }
    let stage = format!("{current_stage:?}");
    if stage != run.previous {
        let observation = serde_json::json!({"stage":stage,"elapsed_ms":run.started.elapsed().as_millis(),"render_frames":run.frames.load(Ordering::Relaxed),"resources":measurements()});
        println!("{observation}");
        run.stages.push(observation);
        std::fs::write(
            run.output.join("stages.json"),
            serde_json::to_vec_pretty(&run.stages).unwrap(),
        )
        .unwrap();
        run.previous = stage;
        if run.rendered && current_stage == cell::speech::ListeningStage::Transcribing {
            world
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(
                    run.output.join("transcribing.png"),
                ));
        }
    }
    match run.phase {
        Phase::Start if ready => {
            let (capture, sender, stopped) = fake_capture();
            world.get_mut::<Listening>(run.owner).unwrap().capture = Some(capture);
            run.sender = Some(sender);
            run.stopped = Some(stopped);
            click(world, "Start listening");
            run.phase = Phase::Enabling;
        }
        Phase::Enabling if enabled && ready => {
            capture(&mut run, world);
            run.phase = Phase::Capture;
        }
        Phase::Capture if run.input_task.as_ref().unwrap().is_finished() => {
            run.speech_frames = Some(run.input_task.take().unwrap().join().unwrap());
            run.phase = Phase::Reply;
        }
        Phase::Reply if paused => {
            run.phase = Phase::Playback;
        }
        Phase::Playback if playback_finished => {
            assert!(paused, "Stop must precede reopening a physical microphone");
            assert!(run.output_started_ms.is_some());
            assert!(run.output_finished_ms.is_some());
            let requests = run.script.requests.lock().unwrap();
            assert_eq!(
                requests.len(),
                1,
                "The utterance must reach the provider exactly once"
            );
            let transcript = requests[0]
                .1
                .iter()
                .rev()
                .find_map(|message| match message {
                    cell::fiote_provider::Message::User(text)
                    | cell::fiote_provider::Message::RichUser { text, .. } => Some(text),
                    _ => None,
                })
                .unwrap()
                .clone();
            drop(requests);
            std::fs::write(run.output.join("transcript.txt"), &transcript).unwrap();
            run.reference_match =
                speech_reference::words(&transcript) == speech_reference::words(TEXT);
            assert!(run.stopped.as_ref().unwrap().load(Ordering::Acquire));
            if run.rendered {
                assert!(
                    run.frames.load(Ordering::Relaxed) > run.speech_frames.unwrap() + 3,
                    "Rendering stopped during speech work"
                );
                world
                    .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                    .observe(bevy::render::view::screenshot::save_to_disk(
                        run.output.join("reply.png"),
                    ));
            }
            click(world, "Stop listening");
            run.phase = Phase::Stopping;
            run.stop_until = Instant::now() + Duration::from_secs(2);
        }
        Phase::Stopping if !enabled && ready && Instant::now() >= run.stop_until => {
            assert_eq!(run.script.requests.lock().unwrap().len(), 1);
            if run.rendered {
                assert!(run.output.join("reply.png").is_file());
                assert!(run.output.join("transcribing.png").is_file());
            }
            #[cfg(target_os = "linux")]
            let ticks_per_second = Some(rustix::param::clock_ticks_per_second());
            #[cfg(not(target_os = "linux"))]
            let ticks_per_second: Option<u64> = None;
            let report = serde_json::json!({"passed":run.reference_match,"flow_completed":true,"transcript_reference_match":run.reference_match,"rendered":run.rendered,"input_ms":18_200,"elapsed_ms":run.started.elapsed().as_millis(),"render_frames":run.frames.load(Ordering::Relaxed),"max_ui_update_gap_ms":run.max_update_gap_ms,"audio_output_queued_ms":run.output_started_ms,"audio_output_finished_ms":run.output_finished_ms,"cpu_ticks":live::cpu_ticks().zip(run.cpu_before).map(|(after,before)|after.saturating_sub(before)),"clock_ticks_per_second":ticks_per_second,"stages":run.stages,"resources":measurements(),"scope":"Stored verified synthetic PCM fed in real time through native wake gate and UI, actual Phonon and Kokoro, fixture reply provider, no subscription usage; recognition and voice models cold until first request/reply; process peak includes setup; rendered case uses the full native presentation and canvas with an assistant panel; timestamps are not DAC onset"});
            std::fs::write(
                run.output.join("report.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
            println!("{report}");
            run.done.store(true, Ordering::Release);
            world.write_message(AppExit::Success);
            run.phase = Phase::Finished;
        }
        _ => {}
    }
    world.insert_resource(run);
}

fn run(rendered: bool) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let Fixture {
        mut app,
        owner,
        speech,
        host,
        script,
        directory,
        samples,
        profile,
        output,
    } = runtime.block_on(fixture(rendered));
    let _entered = runtime.enter();
    let frames = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    if rendered {
        let mut root = owner;
        while let Some(parent) = app.world().get::<ChildOf>(root) {
            root = parent.parent();
        }
        {
            let mut node = app.world_mut().get_mut::<Node>(root).unwrap();
            node.position_type = PositionType::Absolute;
            node.right = px(0);
            node.top = px(0);
            node.width = px(700);
            node.height = percent(100);
            node.overflow = Overflow::scroll_y();
        }
        app.world_mut().entity_mut(root).insert(GlobalZIndex(100));
        let counter = frames.clone();
        app.sub_app_mut(bevy::render::RenderApp).add_systems(
            bevy::render::Render,
            (move || {
                counter.fetch_add(1, Ordering::Relaxed);
            })
            .after(bevy::render::RenderSystems::Render),
        );
    }
    let mut input = cell::fiote_communication::speech::decode_audio(
        &STANDARD.encode(live::recording("wake-statement")),
    )
    .unwrap();
    input.extend(vec![0.0; 16_000]);
    input.extend(samples);
    input.extend(vec![0.0; 24_000]);
    app.insert_resource(Run {
        owner,
        started: Instant::now(),
        last_tick: Instant::now(),
        max_update_gap_ms: 0,
        cpu_before: live::cpu_ticks(),
        reference_match: false,
        output_active: false,
        output_started_ms: None,
        output_finished_ms: None,
        stop_until: Instant::now(),
        phase: Phase::Start,
        chunks: input.chunks(1600).map(<[f32]>::to_vec).collect(),
        input_task: None,
        profile,
        sender: None,
        stopped: None,
        script,
        output: output.clone(),
        stages: Vec::new(),
        previous: String::new(),
        frames,
        speech_frames: None,
        rendered,
        done: done.clone(),
    });
    if !app.is_plugin_added::<crate::native::cell_bridge::CellBridgePlugin>() {
        app.add_systems(PreUpdate, receive);
    }
    app.add_systems(Last, drive);
    if rendered {
        assert_eq!(app.run(), AppExit::Success);
    } else {
        app.finish();
        app.cleanup();
        while !done.load(Ordering::Acquire) {
            app.update();
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(done.load(Ordering::Acquire));
    runtime.block_on(async {
        speech.shutdown().await;
        host.stop_all().await;
        assert!(
            !speech
                .handle(Request::Inspect)
                .await
                .unwrap()
                .local
                .unwrap()
                .loaded
        );
    });
    std::fs::write(
        output.join("after-shutdown.json"),
        serde_json::to_vec_pretty(&measurements()).unwrap(),
    )
    .unwrap();
    drop(app);
    drop(speech);
    drop(host);
    runtime.block_on(tokio::time::sleep(Duration::from_millis(100)));
    std::fs::write(
        output.join("after-drop.json"),
        serde_json::to_vec_pretty(&measurements()).unwrap(),
    )
    .unwrap();
    drop(directory);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(
        report["passed"], true,
        "Long-request transcription changed content; inspect transcript.txt and the retained reference"
    );
}

#[test]
#[ignore = "Requires local Phonon/Kokoro assets, output device and LINCE_VOICE_TEST_REPORT_DIR; real-time near-limit synthetic speech, no account usage"]
fn actual_near_limit_request_preserves_complete_delivery_and_spoken_reply() {
    run(false);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "Requires a desktop display, local Phonon/Kokoro assets, output device and LINCE_VOICE_TEST_REPORT_DIR; owned rendered window plus real-time voice processing, no account usage"]
fn actual_rendered_window_processes_near_limit_voice_without_stalling() {
    run(true);
}
