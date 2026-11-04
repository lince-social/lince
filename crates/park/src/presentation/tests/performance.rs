use super::*;
use native::tokens::{ThemeSettings, Token, TokenOverrides, TokenValue};
use std::time::Instant;

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
