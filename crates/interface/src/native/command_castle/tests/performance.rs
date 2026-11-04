use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

struct Driver {
    pending: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
    wakes: Arc<AtomicU64>,
}

impl Driver {
    fn new() -> Self {
        Self {
            pending: Arc::new(AtomicBool::new(true)),
            notify: Arc::new(tokio::sync::Notify::new()),
            wakes: Arc::new(AtomicU64::new(0)),
        }
    }

    fn signal(&self) -> crate::host::WakeSignal {
        let pending = self.pending.clone();
        let notify = self.notify.clone();
        let wakes = self.wakes.clone();
        crate::host::WakeSignal::new(move || {
            wakes.fetch_add(1, Ordering::Relaxed);
            pending.store(true, Ordering::Release);
            notify.notify_one();
        })
    }

    async fn pump(&self, app: &mut App, duration: Duration, done: impl Fn(&World) -> bool) -> u64 {
        let deadline = tokio::time::Instant::now() + duration;
        let mut updates = 0;
        loop {
            if self.pending.swap(false, Ordering::AcqRel) {
                app.update();
                updates += 1;
            }
            if done(app.world()) || tokio::time::Instant::now() >= deadline {
                return updates;
            }
            tokio::select! {
                _ = self.notify.notified() => {},
                _ = tokio::time::sleep_until(deadline) => return updates,
            }
        }
    }
}

fn playback(app: &mut App, terminal: Entity, caption: &str) {
    let action = {
        let world = app.world_mut();
        world
            .query::<(
                &crate::native::actions::ActionButton,
                &bevy::a11y::AccessibilityNode,
            )>()
            .iter(world)
            .find(|(button, label)| button.target == terminal && label.label() == Some(caption))
            .unwrap()
            .0
            .clone()
    };
    action.actions.run(app.world_mut(), action.target);
}

fn has_text(world: &World, owner: Entity, caption: &str) -> bool {
    let mut pending = vec![owner];
    while let Some(entity) = pending.pop() {
        if world
            .get::<Text>(entity)
            .is_some_and(|text| text.0 == caption)
        {
            return true;
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
    false
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quiet_command_views_sleep_and_resume_for_real_output_and_completion() {
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let script =
        "printf 'ready\\n'; read -r first; printf 'seen:%s\\n' \"$first\"; read -r second; exit 7";
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Command,
                head: "Quiet command".into(),
                body: script.into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let runtime = cell::CellRuntime {
        commands: cell::terminal::commands::CommandHost::new(directory.path().into()),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: None,
        speech: None,
    };
    let driver = Driver::new();
    let mut app = app();
    app.insert_resource(crate::native::app::CellHandle(runtime.clone()))
        .insert_resource(driver.signal())
        .add_plugins((
            crate::native::cell_bridge::CellBridgePlugin,
            crate::native::terminal::TerminalPlugin,
            CommandCastlePlugin,
            crate::native::record_binding::RecordBindingPlugin,
        ));
    app.update();
    let reads = Arc::new(AtomicU64::new(0));
    let histories = Arc::new(AtomicU64::new(0));
    let read_count = reads.clone();
    let history_count = histories.clone();
    let (outgoing, mut requests) = tokio::sync::mpsc::channel(64);
    let original = std::mem::replace(
        &mut app
            .world_mut()
            .get_non_send_mut::<crate::native::cell_bridge::CellBridge>()
            .unwrap()
            .outgoing,
        outgoing,
    );
    let proxy = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            if matches!(
                request,
                ClientMessage::Command {
                    request: Request::Read { .. },
                    ..
                }
            ) {
                read_count.fetch_add(1, Ordering::Relaxed);
            }
            if matches!(
                request,
                ClientMessage::Command {
                    request: Request::History { .. },
                    ..
                }
            ) {
                history_count.fetch_add(1, Ordering::Relaxed);
            }
            if original.send(request).await.is_err() {
                break;
            }
        }
    });
    let Response::Started { run } = runtime
        .commands
        .request(
            &runtime.engine,
            Request::Run {
                command: uid.clone(),
                script: script.into(),
                cwd: directory.path().display().to_string(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("Expected a live run");
    };
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(
        app.world_mut(),
        owner,
        &crate::native::protein_area::Config::records(),
        &json!({"kind":"command", "head":"Quiet command", "body":script}),
        RecordBinding {
            area: owner,
            uid: uid.clone(),
            source: Source::Local,
        },
    );
    driver.pending.store(true, Ordering::Release);
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            world
                .get::<CommandCastle>(owner)
                .is_some_and(|castle| castle.runs.iter().any(|candidate| candidate.id == run.id))
        })
        .await;
    assert!(
        app.world()
            .get::<CommandCastle>(owner)
            .unwrap()
            .runs
            .iter()
            .any(|candidate| candidate.id == run.id)
    );
    Control::Toggle(run.id.clone()).apply(app.world_mut(), owner);
    let terminal = app
        .world_mut()
        .query_filtered::<Entity, With<crate::native::terminal::TerminalSand>>()
        .single(app.world())
        .unwrap();
    driver.pending.store(true, Ordering::Release);
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            crate::native::terminal::displayed_text(world, terminal).contains("ready")
        })
        .await;
    assert!(crate::native::terminal::displayed_text(app.world(), terminal).contains("ready"));
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    let before = (
        reads.load(Ordering::Acquire),
        histories.load(Ordering::Acquire),
        driver.wakes.load(Ordering::Acquire),
    );
    let started = Instant::now();
    let updates = driver
        .pump(&mut app, Duration::from_secs(2), |_| false)
        .await;
    let quiet_reads = reads.load(Ordering::Acquire) - before.0;
    let quiet_histories = histories.load(Ordering::Acquire) - before.1;
    let quiet_wakes = driver.wakes.load(Ordering::Acquire) - before.2;
    let report = json!({"scope":"Private minimal native App and real Cell/PTY/libghostty worker; event-driven updates, no raster or physical latency/FPS acceptance", "interval_ms":started.elapsed().as_secs_f64()*1000.0, "journal_reads":quiet_reads,"history_reads":quiet_histories,"wake_requests":quiet_wakes,"app_updates":updates});
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("quiet-command-views.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    eprintln!("quiet-command-views: {report}");
    if std::env::var_os("LINCE_COMMAND_POLL_DIAGNOSTIC").is_none() {
        assert_eq!(
            (quiet_reads, quiet_histories, quiet_wakes, updates),
            (0, 0, 0, 0)
        );
    }
    playback(&mut app, terminal, "From start");
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            crate::native::terminal::displayed_text(world, terminal).contains("ready")
        })
        .await;
    assert!(crate::native::terminal::displayed_text(app.world(), terminal).contains("ready"));
    let mut changed = runtime.commands.observe(&uid, Some(&run.id)).unwrap();
    panel::send(
        app.world(),
        terminal,
        ClientMessage::Command {
            id: run.id.clone(),
            request: Request::Input {
                command: uid.clone(),
                run: run.id.clone(),
                data_base64: "Y2hhbmdlZAo=".into(),
            },
        },
    )
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), changed.changed())
            .await
            .unwrap()
    );
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    assert!(
        !crate::native::terminal::displayed_text(app.world(), terminal).contains("seen:changed")
    );
    playback(&mut app, terminal, "Next output");
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    playback(&mut app, terminal, "Follow latest");
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            crate::native::terminal::displayed_text(world, terminal).contains("seen:changed")
        })
        .await;
    assert!(
        crate::native::terminal::displayed_text(app.world(), terminal).contains("seen:changed")
    );
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    let (forwarded, incoming) = tokio::sync::mpsc::channel(64);
    let mut original = std::mem::replace(
        &mut app
            .world_mut()
            .get_non_send_mut::<crate::native::cell_bridge::CellBridge>()
            .unwrap()
            .incoming,
        incoming,
    );
    let held = Arc::new(AtomicBool::new(false));
    let held_response = held.clone();
    let release = Arc::new(tokio::sync::Notify::new());
    let resumed = release.clone();
    let wake = driver.signal();
    let incoming_proxy = tokio::spawn(async move {
        let mut captured = false;
        while let Some(message) = original.recv().await {
            if !captured
                && matches!(&message, ServerMessage::Command { response: Response::Output { events, complete: false, .. }, .. } if events.is_empty())
            {
                captured = true;
                held_response.store(true, Ordering::Release);
                wake.ring();
                resumed.notified().await;
            }
            if forwarded.send(message).await.is_err() {
                break;
            }
            wake.ring();
        }
    });
    playback(&mut app, terminal, "Follow latest");
    driver
        .pump(&mut app, Duration::from_secs(3), |_| {
            held.load(Ordering::Acquire)
        })
        .await;
    assert!(
        held.load(Ordering::Acquire),
        "The test must hold an actual empty response while the native read remains pending"
    );
    let before = reads.load(Ordering::Acquire);
    for _ in 0..2 {
        runtime
            .commands
            .request(
                &runtime.engine,
                Request::Resize {
                    command: uid.clone(),
                    run: run.id.clone(),
                    cols: 80,
                    rows: 24,
                },
            )
            .await
            .unwrap();
        driver
            .pump(&mut app, Duration::from_millis(100), |_| false)
            .await;
    }
    let waiting_wakes = driver.wakes.load(Ordering::Acquire);
    runtime
        .commands
        .request(
            &runtime.engine,
            Request::Resize {
                command: uid.clone(),
                run: run.id.clone(),
                cols: 80,
                rows: 24,
            },
        )
        .await
        .unwrap();
    driver
        .pump(&mut app, Duration::from_millis(100), |_| false)
        .await;
    assert_eq!(
        driver.wakes.load(Ordering::Acquire),
        waiting_wakes,
        "Changes must coalesce while the existing journal read is pending"
    );
    assert_eq!(reads.load(Ordering::Acquire), before);
    release.notify_one();
    driver
        .pump(&mut app, Duration::from_secs(3), |_| {
            reads.load(Ordering::Acquire) > before
        })
        .await;
    assert!(
        reads.load(Ordering::Acquire) > before,
        "An empty response must not erase newer changes waiting behind it"
    );
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    let (closed, receiver) = tokio::sync::mpsc::channel(1);
    drop(receiver);
    let live = std::mem::replace(
        &mut app
            .world_mut()
            .get_non_send_mut::<crate::native::cell_bridge::CellBridge>()
            .unwrap()
            .outgoing,
        closed,
    );
    runtime
        .commands
        .request(
            &runtime.engine,
            Request::Resize {
                command: uid.clone(),
                run: run.id.clone(),
                cols: 80,
                rows: 24,
            },
        )
        .await
        .unwrap();
    driver
        .pump(&mut app, Duration::from_millis(100), |_| false)
        .await;
    let waiting_wakes = driver.wakes.load(Ordering::Acquire);
    let before = reads.load(Ordering::Acquire);
    runtime
        .commands
        .request(
            &runtime.engine,
            Request::Resize {
                command: uid.clone(),
                run: run.id.clone(),
                cols: 80,
                rows: 24,
            },
        )
        .await
        .unwrap();
    driver
        .pump(&mut app, Duration::from_millis(100), |_| false)
        .await;
    assert_eq!(
        driver.wakes.load(Ordering::Acquire),
        waiting_wakes,
        "A disconnected live consumer must retain its dirty signal until reconnection"
    );
    assert_eq!(reads.load(Ordering::Acquire), before);
    app.world_mut()
        .get_non_send_mut::<crate::native::cell_bridge::CellBridge>()
        .unwrap()
        .outgoing = live;
    driver.pending.store(true, Ordering::Release);
    driver
        .pump(&mut app, Duration::from_secs(3), |_| {
            reads.load(Ordering::Acquire) > before
        })
        .await;
    assert!(reads.load(Ordering::Acquire) > before);
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    let mut laboratory = crate::native::laboratory::Laboratory::default();
    laboratory.root = Some(app.world_mut().spawn_empty().id());
    app.world_mut().insert_resource(laboratory);
    let before = reads.load(Ordering::Acquire);
    runtime
        .commands
        .request(
            &runtime.engine,
            Request::Input {
                command: uid,
                run: run.id.clone(),
                data_base64: "ZG9uZQo=".into(),
            },
        )
        .await
        .unwrap();
    driver.pending.store(true, Ordering::Release);
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    assert_eq!(
        reads.load(Ordering::Acquire),
        before,
        "Suspending the workspace must not fetch journal pages"
    );
    assert!(
        app.world()
            .get::<CommandCastle>(owner)
            .unwrap()
            .runs
            .iter()
            .any(|candidate| candidate.id == run.id && candidate.finished_ms.is_none())
    );
    app.world_mut()
        .remove_resource::<crate::native::laboratory::Laboratory>();
    driver.pending.store(true, Ordering::Release);
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            has_text(world, terminal, "End of saved output · read-only")
                && world.get::<CommandCastle>(owner).is_some_and(|castle| {
                    castle
                        .runs
                        .iter()
                        .any(|candidate| candidate.id == run.id && candidate.exit_code == Some(7))
                })
        })
        .await;
    assert!(
        app.world()
            .get::<CommandCastle>(owner)
            .unwrap()
            .runs
            .iter()
            .any(|candidate| candidate.id == run.id && candidate.exit_code == Some(7))
    );
    assert!(has_text(
        app.world(),
        terminal,
        "End of saved output · read-only"
    ));
    playback(&mut app, terminal, "From start");
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            crate::native::terminal::displayed_text(world, terminal).contains("ready")
        })
        .await;
    assert!(crate::native::terminal::displayed_text(app.world(), terminal).contains("ready"));
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    let (closed, receiver) = tokio::sync::mpsc::channel(1);
    drop(receiver);
    let live = std::mem::replace(
        &mut app
            .world_mut()
            .get_non_send_mut::<crate::native::cell_bridge::CellBridge>()
            .unwrap()
            .outgoing,
        closed,
    );
    playback(&mut app, terminal, "Next output");
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    let before = driver.wakes.load(Ordering::Acquire);
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    assert_eq!(
        driver.wakes.load(Ordering::Acquire),
        before,
        "A closed connection must wait for reconnection instead of polling"
    );
    app.world_mut()
        .get_non_send_mut::<crate::native::cell_bridge::CellBridge>()
        .unwrap()
        .outgoing = live;
    driver.pending.store(true, Ordering::Release);
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            has_text(
                world,
                terminal,
                "Saved output · Next output advances through the full run",
            )
        })
        .await;
    assert!(has_text(
        app.world(),
        terminal,
        "Saved output · Next output advances through the full run"
    ));
    let mut replacement = runtime.clone();
    replacement.commands = cell::terminal::commands::CommandHost::new(directory.path().into());
    app.world_mut()
        .insert_resource(crate::native::app::CellHandle(replacement));
    let before = reads.load(Ordering::Acquire);
    playback(&mut app, terminal, "From start");
    driver
        .pump(&mut app, Duration::from_millis(200), |_| false)
        .await;
    assert_eq!(
        reads.load(Ordering::Acquire),
        before,
        "Reusing the same run directory must not retarget a saved view to another command host"
    );
    app.world_mut().despawn(owner);
    drop(app);
    proxy.abort();
    incoming_proxy.abort();
    runtime.commands.shutdown().await;
}
