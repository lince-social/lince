use super::*;
use native::record_binding::{EditObservation, TextBinding, observe_edit};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Surface {
    Fiote,
    Kanban,
}

struct Edit {
    at: Duration,
    character: Option<char>,
}

struct Receipt {
    scheduled: Duration,
    delivered: Duration,
    echo: Duration,
    revision: Option<u64>,
    request: Option<String>,
    durable: Option<Duration>,
    acknowledged: Option<Duration>,
}

pub fn run() -> io::Result<()> {
    if !performance::enabled() || std::env::var_os("LINCE_PERFORMANCE_LOG").is_none() {
        return Err(error(
            "Record edit measurements require retained performance traces",
        ));
    }
    let arguments: Vec<_> = std::env::args().skip(2).collect();
    let [surface, cadence, scene] = arguments.as_slice() else {
        return Err(error(
            "Use --record-edits fiote|kanban paced|burst quiet|mixed|settled",
        ));
    };
    let surface = match surface.as_str() {
        "fiote" => Surface::Fiote,
        "kanban" => Surface::Kanban,
        _ => return Err(error("Unknown Record edit surface")),
    };
    let interval = match cadence.as_str() {
        "paced" => Duration::from_millis(100),
        "burst" => Duration::from_millis(8),
        _ => return Err(error("Unknown Record edit cadence")),
    };
    let mixed = match scene.as_str() {
        "quiet" => false,
        "mixed" | "settled" => true,
        _ => return Err(error("Unknown Record edit scene")),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(32 * 1024 * 1024)
        .enable_all()
        .build()?;
    let native = interface()?;
    println!("{native}");
    if native["passed"] != true {
        return Err(error("Required native interface checks failed"));
    }
    let result = measure(&runtime, surface, interval, mixed, scene == "settled");
    let drained = performance::finish(Duration::from_secs(3));
    println!(
        "{}",
        json!({
            "suite":"park-record-edits-v1",
            "scope":"scheduled native keyboard input to completed GPU frame and observed local durability; no physical scanout",
            "optimized":!cfg!(debug_assertions),
            "revision":option_env!("LINCE_REVISION").unwrap_or("unknown"),
            "native":native,
            "arguments":arguments,
            "trace_drained":drained,
            "error":result.as_ref().err().map(ToString::to_string),
            "case":result.as_ref().ok(),
        })
    );
    let result = result?;
    if !drained || result["passed"] != true {
        return Err(error(
            "Record edit acceptance failed; retain the measured result",
        ));
    }
    Ok(())
}

fn key(canvas: &mut Presentation, code: KeyCode, key: Key, text: Option<&str>) -> io::Result<()> {
    canvas
        .key(code, key.clone(), text.map(Into::into), true, false)
        .map_err(error)?;
    canvas.key(code, key, None, false, false).map_err(error)
}

fn observe(canvas: &mut Presentation, editor: Entity) -> io::Result<EditObservation> {
    let observation = observe_edit(canvas.world_mut(), editor)
        .ok_or_else(|| error("The bound Record editor or its document disappeared"))?;
    if observation.failed || !observation.writable {
        return Err(error(format!(
            "The bound editor stopped accepting edits: {observation:?}"
        )));
    }
    Ok(observation)
}

fn motion(canvas: &mut Presentation) -> (usize, bool) {
    let world = canvas.world_mut();
    let awake = native::topology::physics::awake(world);
    let bodies = world
        .query::<&native::topology::physics::Body>()
        .iter(world)
        .count();
    (bodies, awake)
}

fn measure(
    runtime: &tokio::runtime::Runtime,
    surface: Surface,
    interval: Duration,
    mixed: bool,
    settle: bool,
) -> io::Result<Value> {
    performance::set_phase(Phase::Warmup);
    let directory = tempfile::tempdir()?;
    let url = format!("sqlite://{}", directory.path().join("lince.db").display());
    let engine = Arc::new(
        runtime
            .block_on(engine::Engine::open(&url))
            .map_err(error)?,
    );
    let original = "Small example title";
    let replacement = "New title!";
    if surface == Surface::Kanban {
        for name in std::iter::once("task").chain(
            lince_interface::records::KANBAN_COLUMNS
                .iter()
                .map(|(_, slug, _)| *slug),
        ) {
            runtime
                .block_on(engine.act(
                    engine::actions::Action::CreateConcept {
                        lingua: "g_local".into(),
                        name: name.into(),
                        parents: Vec::new(),
                    },
                    None,
                ))
                .map_err(error)?;
        }
    }
    let action = if surface == Surface::Fiote {
        engine::actions::Action::CreateAgent {
            head: original.into(),
            operated_by: None,
        }
    } else {
        engine::actions::Action::CreateRecordWithTags {
            head: original.into(),
            body: String::new(),
            quantity: 0.0,
            tags: vec!["task".into(), "backlog".into()],
        }
    };
    let uid = runtime
        .block_on(engine.act(action, None))
        .map_err(error)?
        .created
        .ok_or_else(|| error("Synthetic Record was not created"))?;
    let mut local = cell(engine.clone(), directory.path());
    if surface == Surface::Fiote {
        local.0.fiote = Some(Arc::new(
            runtime
                .block_on(cell::fiote::Host::open(
                    engine.clone(),
                    directory.path().join("private-fiote"),
                ))
                .map_err(error)?,
        ));
    }
    let _entered = runtime.enter();
    let _observer = crate::presentation::observe_systems();
    let mut canvas = Presentation::new_with_cell(
        [1920, 1080],
        directory.path().join("assets"),
        native::wake::WakeSignal::new(|| {}),
        Some((local, directory.path().to_path_buf())),
    )
    .map_err(error)?;
    completed(&mut canvas)?;
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .iter(world)
            .next()
            .ok_or_else(|| error("No canonical canvas"))?
    };
    canvas
        .outputs(vec![output(1, 1, [1920, 1080], 0.0)])
        .map_err(error)?;
    canvas
        .move_workspace_to_output(1, OutputId(1))
        .map_err(error)?;
    canvas.focus_output(OutputId(1)).map_err(error)?;
    canvas
        .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
        .map_err(error)?;
    let owner = if surface == Surface::Fiote {
        native::full_record::open_fiote(canvas.world_mut(), root, &uid)
    } else {
        native::kanban::spawn(canvas.world_mut(), root, 1, DVec2::new(2040.0, 0.0))
    }
    .ok_or_else(|| error("Could not construct the requested native component"))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let editor = loop {
        completed(&mut canvas)?;
        let candidates: Vec<_> = {
            let world = canvas.world_mut();
            world
                .query::<(Entity, &TextBinding, Option<&bevy::a11y::AccessibilityNode>)>()
                .iter(world)
                .filter(|(_, binding, label)| {
                    binding.record.uid == uid
                        && binding.property == "head"
                        && (surface != Surface::Fiote
                            || label.is_some_and(|label| label.label() == Some("Name")))
                })
                .map(|(entity, _, _)| entity)
                .filter(|entity| {
                    observe_edit(world, *entity)
                        .is_some_and(|state| state.writable && !state.failed)
                })
                .collect()
        };
        if let Some(editor) = candidates
            .into_iter()
            .find(|entity| input_point(&mut canvas, *entity).is_ok())
        {
            break editor;
        }
        if Instant::now() >= deadline {
            return Err(error("Native Record editor did not become writable"));
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    if mixed {
        native::workspace::create(canvas.world_mut(), root);
        native::workspace::create(canvas.world_mut(), root);
        for index in 0..24 {
            let kind = [
                native::sand_store::SandKind::Text,
                native::sand_store::SandKind::EditableText,
                native::sand_store::SandKind::WorkTimer,
                native::sand_store::SandKind::Sync,
                native::sand_store::SandKind::Configuration,
                native::sand_store::SandKind::Trust,
            ][index % 6];
            native::sand_store::spawn_sand(
                canvas.world_mut(),
                root,
                1,
                kind,
                "Synthetic neighboring content",
                DVec2::new(
                    600.0 + (index % 4) as f64 * 180.0,
                    (index / 4) as f64 * 160.0 - 400.0,
                ),
            );
        }
        native::full_record::open_in(
            canvas.world_mut(),
            root,
            1,
            DVec2::new(-650.0, 0.0),
            &uid,
            native::protein_area::Source::Local,
        )
        .ok_or_else(|| error("Could not open the shared Record view"))?;
        for index in 0..8 {
            native::full_record::open_in(
                canvas.world_mut(),
                root,
                3,
                DVec2::new(index as f64 * 600.0, 0.0),
                &uid,
                native::protein_area::Source::Local,
            )
            .ok_or_else(|| error("Could not open the inactive Record view"))?;
        }
        canvas
            .move_workspace_to_output(1, OutputId(1))
            .map_err(error)?;
        canvas.focus_output(OutputId(1)).map_err(error)?;
    }
    for _ in 0..8 {
        completed(&mut canvas)?;
    }
    let settling = Instant::now();
    let mut settling_frames = 0;
    let mut quiet_frames = 0;
    while settle && quiet_frames < 3 {
        completed(&mut canvas)?;
        settling_frames += 1;
        let (bodies, awake) = motion(&mut canvas);
        quiet_frames = if awake { 0 } else { quiet_frames + 1 };
        println!(
            "{}",
            json!({"record_edit_settling_frame":settling_frames,
            "bodies":bodies,"awake":awake,"elapsed_ms":settling.elapsed().as_secs_f64()*1000.0})
        );
        if settling.elapsed() >= Duration::from_secs(60) {
            return Err(error(
                "The populated scene did not settle within 60 seconds",
            ));
        }
    }
    let settling_ms = settling.elapsed().as_secs_f64() * 1000.0;
    let point = input_point(&mut canvas, editor)?;
    canvas.focus(true).map_err(error)?;
    canvas.pointer(point).map_err(error)?;
    completed(&mut canvas)?;
    canvas
        .button(bevy::input::mouse::MouseButton::Left, true)
        .map_err(error)?;
    completed(&mut canvas)?;
    canvas
        .button(bevy::input::mouse::MouseButton::Left, false)
        .map_err(error)?;
    key(&mut canvas, KeyCode::End, Key::End, None)?;
    completed(&mut canvas)?;
    if canvas.world_mut().resource::<InputFocus>().get() != Some(editor) {
        return Err(error(
            "The actual Record field did not receive pointer focus",
        ));
    }
    let initial = canvas
        .world_mut()
        .get::<bevy::text::EditableText>(editor)
        .ok_or_else(|| error("Missing bound editor"))?
        .value()
        .to_string();
    if initial != original {
        return Err(error(
            "The actual Record field has an unexpected initial value",
        ));
    }
    let mut edits = Vec::new();
    let mut previous = original;
    for turn in 0..5 {
        let next = if turn % 2 == 0 { replacement } else { original };
        for character in
            std::iter::repeat_n(None, previous.chars().count()).chain(next.chars().map(Some))
        {
            edits.push(Edit {
                at: interval * (edits.len() as u32 + 1),
                character,
            });
        }
        previous = next;
    }
    let mut expected = original.to_owned();
    let mut receipts: Vec<Receipt> = Vec::new();
    let mut frames = 0;
    let mut awake_frames = 0;
    let mut maximum_pending = 0;
    let mut warned = false;
    let mut dirty_since = None;
    let mut longest_dirty = Duration::ZERO;
    performance::set_phase(Phase::LocalSave);
    let cpu_ms = || {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1_000_000.0
    };
    let projection_counts = || {
        let metrics = &engine.projection.metrics;
        [&metrics.snapshots, &metrics.steps, &metrics.hits]
            .map(|count| count.load(std::sync::atomic::Ordering::Relaxed))
    };
    let projection_before = projection_counts();
    let cpu_before = cpu_ms();
    let started = Instant::now();
    loop {
        let before = receipts.len();
        while receipts.len() < edits.len() && edits[receipts.len()].at <= started.elapsed() {
            let edit = &edits[receipts.len()];
            if let Some(character) = edit.character {
                let text = character.to_string();
                key(
                    &mut canvas,
                    KeyCode::KeyA,
                    Key::Character(text.clone().into()),
                    Some(&text),
                )?;
                expected.push(character);
            } else {
                key(&mut canvas, KeyCode::Backspace, Key::Backspace, None)?;
                expected.pop();
            }
            receipts.push(Receipt {
                scheduled: edit.at,
                delivered: started.elapsed(),
                echo: Duration::ZERO,
                revision: None,
                request: None,
                durable: None,
                acknowledged: None,
            });
        }
        completed(&mut canvas)?;
        frames += 1;
        let now = started.elapsed();
        if canvas.world_mut().resource::<InputFocus>().get() != Some(editor) {
            return Err(error(format!(
                "Record edit focus was lost after {} inputs",
                receipts.len()
            )));
        }
        let actual = canvas
            .world_mut()
            .get::<bevy::text::EditableText>(editor)
            .ok_or_else(|| error("The edited field was replaced during typing"))?
            .value()
            .to_string();
        if actual != expected {
            return Err(error(format!(
                "Native edit mismatch after {} scheduled inputs",
                receipts.len()
            )));
        }
        let observation = observe(&mut canvas, editor)?;
        let (bodies, awake) = motion(&mut canvas);
        awake_frames += usize::from(awake);
        for receipt in &mut receipts[before..] {
            receipt.echo = now;
        }
        for receipt in &mut receipts {
            if receipt.revision.is_none() && observation.applied {
                receipt.revision = Some(observation.revision);
                receipt.request = observation.pending.last().cloned();
            }
            if receipt
                .revision
                .is_some_and(|revision| observation.durable_revision >= revision)
            {
                receipt.durable.get_or_insert(now);
            }
            if receipt.revision.is_some()
                && receipt
                    .request
                    .as_ref()
                    .map_or(!observation.unsaved, |request| {
                        !observation.pending.contains(request)
                    })
            {
                receipt.acknowledged.get_or_insert(now);
            }
        }
        maximum_pending = maximum_pending.max(observation.pending.len());
        warned |= observation.warning;
        if observation.unsaved {
            let since = *dirty_since.get_or_insert(now);
            longest_dirty = longest_dirty.max(now - since);
        } else {
            if let Some(since) = dirty_since.take() {
                longest_dirty = longest_dirty.max(now - since);
            }
        }
        println!(
            "{}",
            json!({"record_edit_frame":frames,"inputs":receipts.len(),"elapsed_ms":now.as_secs_f64()*1000.0,
            "pending":observation.pending.len(),"revision":observation.revision,"durable_revision":observation.durable_revision,
            "unsaved":observation.unsaved,"warning":observation.warning,
            "bodies":bodies,"physics_awake":awake})
        );
        if receipts.len() == edits.len()
            && !observation.unsaved
            && !observation.saving
            && observation.pending.is_empty()
            && observation.durable_revision == observation.revision
        {
            break;
        }
        if now >= Duration::from_secs(90) {
            return Err(error(
                "The scheduled Record edit cohort did not settle within 90 seconds",
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let process_cpu_ms = cpu_ms() - cpu_before;
    let cohort_elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let projection_after = projection_counts();
    let projection_work: [u64; 3] = std::array::from_fn(|index| projection_after[index] - projection_before[index]);
    if expected != replacement || receipts.len() < 120 {
        return Err(error("Incomplete Record edit cohort"));
    }
    let echo: Vec<_> = receipts
        .iter()
        .map(|row| (row.echo - row.scheduled).as_secs_f64() * 1000.0)
        .collect();
    let durable: Vec<_> = receipts
        .iter()
        .map(|row| {
            row.durable
                .map(|at| (at - row.scheduled).as_secs_f64() * 1000.0)
                .ok_or_else(|| error("Missing durable recovery coverage"))
        })
        .collect::<Result<_, _>>()?;
    let acknowledged: Vec<_> = receipts
        .iter()
        .map(|row| {
            row.acknowledged
                .map(|at| (at - row.scheduled).as_secs_f64() * 1000.0)
                .ok_or_else(|| error("Missing canonical acknowledgement"))
        })
        .collect::<Result<_, _>>()?;
    let delivery: Vec<_> = receipts
        .iter()
        .map(|row| (row.delivered - row.scheduled).as_secs_f64() * 1000.0)
        .collect();
    performance::set_phase(Phase::Reopen);
    drop(canvas);
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), engine.store.pool.close())
            .await
            .map_err(error)
    })?;
    drop(engine);
    let reopened = runtime
        .block_on(engine::Engine::open(&url))
        .map_err(error)?;
    if runtime.block_on(reopened.doc_text(&uid)).map_err(error)?.0 != replacement {
        return Err(error("Reopened Record title does not match the final edit"));
    }
    let activations: i64 = runtime
        .block_on(
            store::sqlx::query_scalar("SELECT COUNT(*) FROM fiote_activation")
                .fetch_one(&reopened.store.pool),
        )
        .map_err(error)?;
    if activations != 0 {
        return Err(error(
            "A private edit fixture unexpectedly created a Fiote activation",
        ));
    }
    runtime.block_on(reopened.store.pool.close());
    let durable_summary = summary(&durable);
    let acknowledged_summary = summary(&acknowledged);
    let passed = passes(&echo)
        && durable_summary["p95_ms"].as_f64().unwrap() <= 100.0
        && durable_summary["worst_ms"].as_f64().unwrap() <= 250.0
        && acknowledged_summary["p95_ms"].as_f64().unwrap() <= 100.0
        && acknowledged_summary["worst_ms"].as_f64().unwrap() <= 250.0
        && !warned;
    Ok(
        json!({"surface":if surface==Surface::Fiote {"fiote"} else {"kanban"},"mixed":mixed,
        "interval_ms":interval.as_secs_f64()*1000.0,"logical_inputs":receipts.len(),"frames":frames,
        "process_cpu_ms":process_cpu_ms,"cohort_elapsed_ms":cohort_elapsed_ms,
        "resource_scope":"scheduled editing until local acknowledgement; all process threads including recorder and probe; excludes setup, reopen and unfinished background work",
        "projection_snapshots_started":projection_work[0],"projection_steps":projection_work[1],"projection_cache_hits":projection_work[2],
        "settling_frames":settling_frames,"settling_ms":settling_ms,"awake_frames":awake_frames,
        "scheduled_delivery":summary(&delivery),"scheduled_visible_response":summary(&echo),
        "scheduled_durable_recovery":durable_summary,"scheduled_canonical_acknowledgement":acknowledged_summary,
        "maximum_pending_requests":maximum_pending,"warning_observed":warned,
        "longest_unsaved_ms":longest_dirty.as_secs_f64()*1000.0,"reopened_exactly":true,
        "input_order_and_focus":true,"component_owner":owner.to_string(),"passed":passed}),
    )
}
