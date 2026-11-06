use super::*;
use native::tokens::{ThemeSettings, Token, TokenOverrides, TokenValue};
use std::time::Instant;

mod offscreen;
mod overlap;
mod schedules;
#[cfg(feature = "nested")]
mod direct;

#[test]
fn bounded_performance_reports_keep_tails_outputs_and_backpressure_visible() {
    let checks = lince_interface::host::performance::checks::bounded_distributions_keep_counts_tails_and_concurrent_samples;
    checks();
    lince_interface::host::performance::checks::output_intervals_remain_independent_and_observations_are_bounded();
    lince_interface::host::performance::checks::report_backpressure_does_not_block_and_is_reported(
    );
    lince_interface::host::performance::checks::input_traces_are_bounded_nonblocking_and_report_loss_without_payloads();
}

#[test]
fn shared_token_and_metric_behavior_cases_preserve_live_updates() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let cases = native::laboratory::catalogue()
        .into_iter()
        .filter(|case| {
            case.name.contains("::token_metrics::") || case.name.contains("::token_style::")
        })
        .collect::<Vec<_>>();
    assert!(cases.len() >= 5);
    for case in cases {
        let result = case.execute(&runtime);
        assert!(
            result.error.is_none(),
            "{}: {:?}",
            result.name,
            result.error
        );
    }
}

fn fixture(count: usize) -> (App, Vec<Entity>) {
    let mut app = App::new();
    let mut task_pool_options = bevy::app::TaskPoolOptions::default();
    if let Some(threads) = std::env::var("LINCE_PARK_CPU_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|threads| (1..=32).contains(threads))
    {
        task_pool_options.compute.min_threads = threads;
        task_pool_options.compute.max_threads = threads;
    }
    app.add_plugins((
        bevy::app::TaskPoolPlugin { task_pool_options },
        native::token_style::TokenStylePlugin,
    ));
    let root = app.world_mut().spawn(Node::default()).id();
    let mut leaves = Vec::new();
    for index in 0..count.div_ceil(8) {
        let mut parent = root;
        for _ in 0..4 {
            parent = app
                .world_mut()
                .spawn((Node::default(), ChildOf(parent)))
                .id();
        }
        let mut overrides = TokenOverrides::default();
        overrides.set(
            Token::FontSize,
            TokenValue::Number(if index % 2 == 0 { 20.0 } else { 24.0 }),
        );
        app.world_mut().entity_mut(parent).insert(overrides);
        for child in 0..8 {
            let entity = app
                .world_mut()
                .spawn((
                    Node {
                        padding: UiRect::all(px(12)),
                        column_gap: px(4),
                        ..default()
                    },
                    TextFont {
                        font_size: bevy::text::FontSize::Px(20.0),
                        ..default()
                    },
                    native::token_style::text(Token::Ink),
                    ChildOf(parent),
                ))
                .id();
            if child % 4 == 0 {
                app.world_mut()
                    .entity_mut(entity)
                    .insert(native::icons::IconStyle::default());
            }
            leaves.push(entity);
        }
    }
    (app, leaves)
}

fn sample(app: &mut App, rounds: usize, changing: bool) -> serde_json::Value {
    let mut values = Vec::new();
    let mut cpu_values = Vec::new();
    for index in 0..rounds {
        if changing {
            let mut settings = app.world_mut().resource_mut::<ThemeSettings>();
            let size = if index % 2 == 0 { 16.0 } else { 8.0 };
            settings
                .global
                .set(Token::Padding, TokenValue::Number(size));
            settings
                .global
                .set(Token::Spacing, TokenValue::Number(size));
        }
        let started = Instant::now();
        let cpu_started = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        app.update();
        let cpu_ended = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        values.push(started.elapsed().as_secs_f64() * 1000.0);
        cpu_values.push(
            (cpu_ended.tv_sec - cpu_started.tv_sec) as f64 * 1000.0
                + (cpu_ended.tv_nsec - cpu_started.tv_nsec) as f64 / 1_000_000.0,
        );
        lince_interface::host::performance::flush_if_due();
    }
    values.sort_by(f64::total_cmp);
    cpu_values.sort_by(f64::total_cmp);
    serde_json::json!({
        "rounds":rounds,"changing":changing,
        "median_ms":values[values.len()/2],
        "p95_ms":values[(values.len()*95/100).min(values.len()-1)],
        "samples_ms":values,
        "cpu_median_ms":cpu_values[cpu_values.len()/2],
        "cpu_samples_ms":cpu_values,
    })
}

#[test]
#[ignore = "records CPU-only diagnostic costs; run without concurrent compilation"]
fn dense_native_styles_and_metrics_record_unchanged_and_changing_costs() {
    let rounds = std::env::var("LINCE_PARK_CPU_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(5, 200);
    let mut reports = Vec::new();
    for count in [64, 512, 2048] {
        let (mut app, leaves) = fixture(count);
        for _ in 0..5 {
            app.update();
        }
        let quiet = sample(&mut app, rounds, false);
        let changing = sample(&mut app, rounds, true);
        let padding =
            app.world().resource::<ThemeSettings>().global.0[&Token::Padding].number() / 8.0;
        for (index, entity) in leaves.iter().enumerate() {
            assert_eq!(
                app.world().get::<Node>(*entity).unwrap().padding,
                UiRect::all(px(12.0 * padding))
            );
            let font = if index / 8 % 2 == 0 { 25.0 } else { 30.0 };
            assert_eq!(
                app.world().get::<TextFont>(*entity).unwrap().font_size,
                bevy::text::FontSize::Px(font)
            );
        }
        reports.push(serde_json::json!({"leaves":leaves.len(),"nodes":app.world_mut().query::<&Node>().iter(app.world()).count(),"quiet":quiet,"changing":changing}));
    }
    let result = serde_json::json!({"scope":"CPU-only shared style and metric systems; diagnostic build, no GPU or physical presentation","compute_threads":bevy::tasks::ComputeTaskPool::get().thread_num(),"reports":reports});
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("native-metric-cost.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    for report in result["reports"].as_array().unwrap() {
        eprintln!(
            "native-metric-cost: {}",
            serde_json::json!({"leaves":report["leaves"],"nodes":report["nodes"],"quiet_ms":report["quiet"]["median_ms"],"changing_ms":report["changing"]["median_ms"]})
        );
    }
}

#[test]
#[ignore = "hardware Vulkan/EGL diagnostic of complete static native frames; no concurrent compilation"]
fn static_native_outputs_record_full_frame_cost_with_visible_surface_checks() {
    use lince_interface::host::{DesktopAction, OutputId, WorkspaceLayout};
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut reports = Vec::new();
    for (heads, count) in [(1, 16), (2, 16), (2, 64)] {
        let size = [640, 480];
        let mut canvas = super::appearance::launch(
            &directory
                .path()
                .join(format!("static-{heads}-{count}.json")),
            size,
        );
        if std::env::var_os("LINCE_TRACE_PERFORMANCE").is_some() {
            canvas
                .app
                .init_resource::<NativeFrameChanges>()
                .add_systems(Last, observe_native_frame_changes);
        }
        let outputs = (1..=heads)
            .map(|id| {
                let mut output =
                    super::transfers::output(id, (id - 1) as f64 * f64::from(size[0]), size);
                output.viewport.workspace = id;
                output
            })
            .collect();
        canvas.outputs(outputs).unwrap();
        let root = canvas.canvas_root().unwrap();
        for id in 1..=heads {
            canvas.focus_output(OutputId(id)).unwrap();
            let workspace = canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active;
            native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
            canvas
                .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
                .unwrap();
            *canvas
                .world_mut()
                .get_mut::<native::canvas::CanvasView>(root)
                .unwrap() = Default::default();
            for index in 0..count {
                let owner = super::appearance::item(
                    &mut canvas,
                    root,
                    Vec2::new(68.0, 48.0),
                    [
                        (index % 8) as f64 * 72.0 - 252.0,
                        (index / 8) as f64 * 52.0 - 182.0,
                    ],
                    Color::srgb(0.0, 1.0, 0.0),
                    index,
                    false,
                );
                canvas
                    .world_mut()
                    .entity_mut(owner)
                    .insert(native::workspace::WorkspaceMember(workspace));
                canvas.world_mut().spawn((
                    Text::new(format!("{index}")),
                    TextColor(Color::BLACK),
                    ChildOf(owner),
                ));
            }
        }
        canvas.focus_output(OutputId(1)).unwrap();
        canvas.desktop_action(DesktopAction::Unfocus).unwrap();
        for _ in 0..20 {
            drop(canvas.probe_frame().unwrap());
        }
        let mut elapsed = Vec::new();
        let mut cpu = Vec::new();
        for _ in 0..30 {
            let started = Instant::now();
            let cpu_started = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
            drop(canvas.probe_frame().unwrap());
            let cpu_ended = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
            elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
            cpu.push(
                (cpu_ended.tv_sec - cpu_started.tv_sec) as f64 * 1000.0
                    + (cpu_ended.tv_nsec - cpu_started.tv_nsec) as f64 / 1_000_000.0,
            );
            lince_interface::host::performance::flush_if_due();
        }
        let frame = canvas.probe_frame().unwrap();
        let mut visible = Vec::new();
        for id in 1..=heads {
            let output = OutputId(id);
            let image = if frame.output == Some(output) {
                frame.dmabuf().unwrap()
            } else {
                frame
                    .outputs
                    .iter()
                    .find(|image| image.id == output)
                    .unwrap()
                    .dmabuf()
            };
            let pixels = probe_pixels(image).unwrap();
            let green = pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[0] < 30 && pixel[1] > 230 && pixel[2] < 30)
                .count();
            assert!(
                green > 20_000,
                "The static native sources were not visible: {heads} outputs, {count} owners each, output {id}: {green} green pixels; views: {:?}",
                canvas
                    .app
                    .world()
                    .get_resource::<native::output_views::OutputViews>()
            );
            visible.push(green);
        }
        elapsed.sort_by(f64::total_cmp);
        cpu.sort_by(f64::total_cmp);
        reports.push(
            serde_json::json!({"outputs":heads,"owners_per_output":count,"pixels_per_output":size,
            "warmup_frames":20,"rounds":30,"median_ms":elapsed[15],"p95_ms":elapsed[28],
            "cpu_median_ms":cpu[15],"cpu_total_ms":cpu.iter().sum::<f64>(),"green_pixels":visible,
            "last_native_changes":canvas.app.world().get_resource::<NativeFrameChanges>().map(|changes| &changes.0)}),
        );
        eprintln!("static-output-case: {}", reports.last().unwrap());
        canvas.finish().unwrap();
    }
    let result = serde_json::json!({"scope":"Private diagnostic build: full native update, render/export and completion with visible static native labels/surfaces on one and two outputs; readback outside timing; no physical scanout or input-visible latency acceptance","reports":reports});
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("static-output-frames.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    eprintln!("static-output-frames: {result}");
}

#[derive(Resource, Default)]
struct NativeFrameChanges(serde_json::Value);

fn observe_native_frame_changes(world: &mut World) {
    fn count<C: Component>(world: &mut World) -> usize {
        world
            .query_filtered::<Entity, Changed<C>>()
            .iter(world)
            .count()
    }
    let changes = serde_json::json!({
        "nodes":count::<Node>(world),"text":count::<Text>(world),"spans":count::<TextSpan>(world),
        "fonts":count::<TextFont>(world),"text_layout":count::<TextLayout>(world),
        "text_size":count::<bevy::text::TextLayoutInfo>(world),"computed_nodes":count::<ComputedNode>(world),
        "ui_transforms":count::<UiGlobalTransform>(world),"transforms":count::<Transform>(world),
        "cameras":count::<Camera>(world),"projections":count::<Projection>(world),
        "theme":world.is_resource_changed::<ThemeSettings>(),
    });
    world.resource_mut::<NativeFrameChanges>().0 = changes;
}
