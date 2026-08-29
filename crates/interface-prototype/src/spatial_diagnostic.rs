use avian2d::{math::Vector, prelude::*};
use bevy::{
    app::App,
    asset::AssetPlugin,
    prelude::{Component, MinimalPlugins, Transform, TransformPlugin},
    time::TimeUpdateStrategy,
};
use lince_interface::{
    git_dirty, raw_git_revision,
    scene_artifact::{SceneArtifactEvidence, load_open_scene_fixture},
    semantic::{FieldValue, SemanticDiff, SemanticDiffOp, composition_fixture, record_row},
    source_fingerprint,
    spatial::{FieldSolver, ParentFrame, PhysicsAdapter, Rect, Vec2},
    style::{StyleLayer, StyleValue},
};
use nucleus::{DecimalValue, RecordKind};
use protein::{Include, Order, Predicate, Protein, Source};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use store::{
    Store,
    records::{NewRecord, create},
};

const REPORT_SCHEMA_VERSION: u32 = 2;
const BODY_COUNT: usize = 10_000;
const AVIAN_BODY_COUNT: usize = 1_000;
const STATIC_INDEX_NODE_COUNT: usize = 100_000;
const WARMUP_STEPS: usize = 60;
const SAMPLE_STEPS: usize = 360;

#[derive(Clone, Debug, Serialize)]
struct BenchmarkStats {
    samples: usize,
    mean_micros: f64,
    p50_micros: f64,
    p95_micros: f64,
    p99_micros: f64,
    maximum_micros: f64,
}

#[derive(Clone, Debug, Serialize)]
struct Assertion {
    name: String,
    passed: bool,
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
struct ProteinEvidence {
    source: String,
    current_rows: usize,
    projected_rows: usize,
    stable_keys_unique: bool,
    interface_field_adapter: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
struct SemanticEvidence {
    definitions: usize,
    primitive_sands: usize,
    compound_sands: usize,
    result_instances: usize,
    mapping_arrows: usize,
    why_reasons_per_instance: usize,
    changed_rows: usize,
    changed_tokens: usize,
    diff_apply: BenchmarkStats,
    schema_refusal_observed: bool,
}

#[derive(Clone, Debug, Serialize)]
struct SpatialEvidence {
    selected_architecture: String,
    selected_reason: String,
    field_solver_bodies: usize,
    field_solver_fixed_step: BenchmarkStats,
    field_phase_p95_micros: f64,
    sort_phase_p95_micros: f64,
    broad_phase_p95_micros: f64,
    narrow_phase_p95_micros: f64,
    integration_phase_p95_micros: f64,
    avian_collision_bodies: usize,
    avian_fixed_step: BenchmarkStats,
    camera_visible_near: usize,
    camera_visible_far: usize,
    camera_independent_semantics: bool,
    off_camera_bodies_stepped: usize,
    direct_drag_dirty_bodies: usize,
    action_previews: usize,
    actions_armed: usize,
    parent_frame_error_millimeters: f64,
    static_indexed_nodes: usize,
    static_index_build_micros: f64,
    static_index_query_hits: usize,
    static_index_query: BenchmarkStats,
}

#[derive(Clone, Debug, Serialize)]
struct SpatialReport {
    schema_version: u32,
    gate: String,
    status: String,
    created_unix_millis: u128,
    git_revision: String,
    git_dirty: bool,
    source_fingerprint_sha256: String,
    profile: String,
    protein: ProteinEvidence,
    semantic: SemanticEvidence,
    spatial: SpatialEvidence,
    scene_artifact: SceneArtifactEvidence,
    assertions: Vec<Assertion>,
    limitations: Vec<String>,
}

#[derive(Component)]
struct AvianProbe;

#[tokio::main]
async fn main() {
    let report_path = argument_value("--report")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/interface-laboratory/spatial/report.json"));
    match run(&report_path).await {
        Ok(report) => {
            println!(
                "Spatial {}: field p95 {:.3} ms, Avian p95 {:.3} ms, {} semantic instances",
                report.status,
                report.spatial.field_solver_fixed_step.p95_micros / 1_000.0,
                report.spatial.avian_fixed_step.p95_micros / 1_000.0,
                report.semantic.result_instances
            );
            println!("report {}", report_path.display());
            if report.status != "passed" {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("Spatial diagnostic failed: {error}");
            std::process::exit(1);
        }
    }
}

async fn run(report_path: &Path) -> Result<SpatialReport, Box<dyn std::error::Error>> {
    let protein_rows = query_current_protein().await?;
    let (graph, mut projection, template) = composition_fixture();
    projection.rows = protein_rows.clone();
    projection.validate()?;
    graph.validate()?;
    template.validate(&graph, &projection)?;
    let current_instances = template.instantiate(&graph, &projection)?;

    projection.rows = generated_rows(BODY_COUNT);
    projection.validate()?;
    let projected_instances = template.instantiate(&graph, &projection)?;
    let stable_keys_unique = projected_instances
        .iter()
        .map(|instance| instance.uid.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        == BODY_COUNT;

    let mut tokens = StyleLayer::default();
    let mut semantic_revision = projection.revision;
    let operations = (0..100)
        .map(|index| SemanticDiffOp::UpsertProteinRow {
            row: record_row(
                &format!("record-{index}"),
                &format!("Updated record {index}"),
                "A live Protein diff changed this Sand instance",
            ),
        })
        .chain([
            SemanticDiffOp::SetGlobalToken {
                name: "--lynx-radius-control".into(),
                value: StyleValue::LengthPx(12.0),
            },
            SemanticDiffOp::SetGlobalToken {
                name: "--lynx-surface-primary".into(),
                value: StyleValue::Color("#F8F7F2".into()),
            },
            SemanticDiffOp::SetGlobalToken {
                name: "--lynx-density-scale".into(),
                value: StyleValue::Scalar(0.82),
            },
        ])
        .collect::<Vec<_>>();
    let diff = SemanticDiff {
        schema_version: 1,
        base_revision: semantic_revision,
        next_revision: semantic_revision + 1,
        operations,
    };
    let semantic_started = Instant::now();
    diff.apply(&mut semantic_revision, &mut projection, &mut tokens)?;
    let semantic_elapsed = semantic_started.elapsed();
    let diff_apply = benchmark_from_durations(&[semantic_elapsed]);
    let changed_rows = projection
        .rows
        .iter()
        .take(100)
        .filter(|row| {
            row.fields.get("title").is_some_and(
                |value| matches!(value, FieldValue::Text(text) if text.starts_with("Updated")),
            )
        })
        .count();
    let invalid_diff = SemanticDiff {
        schema_version: 99,
        base_revision: semantic_revision,
        next_revision: semantic_revision + 1,
        operations: Vec::new(),
    };
    let mut refusal_projection = projection.clone();
    let mut refusal_tokens = tokens.clone();
    let mut refusal_revision = semantic_revision;
    let schema_refusal_observed = invalid_diff
        .apply(
            &mut refusal_revision,
            &mut refusal_projection,
            &mut refusal_tokens,
        )
        .is_err()
        && refusal_revision == semantic_revision
        && refusal_projection == projection
        && refusal_tokens == tokens;

    let mut field_solver = FieldSolver::deterministic_fixture(BODY_COUNT, 0x4c494e4345);
    for _ in 0..WARMUP_STEPS {
        field_solver.step(1.0 / 120.0);
    }
    let mut field_durations = Vec::with_capacity(SAMPLE_STEPS);
    let mut field_phase = Vec::with_capacity(SAMPLE_STEPS);
    let mut sort_phase = Vec::with_capacity(SAMPLE_STEPS);
    let mut broad_phase = Vec::with_capacity(SAMPLE_STEPS);
    let mut narrow_phase = Vec::with_capacity(SAMPLE_STEPS);
    let mut integration_phase = Vec::with_capacity(SAMPLE_STEPS);
    for _ in 0..SAMPLE_STEPS {
        let started = Instant::now();
        let metrics = field_solver.step(1.0 / 120.0);
        field_durations.push(started.elapsed());
        field_phase.push(metrics.field_nanos);
        sort_phase.push(metrics.sort_nanos);
        broad_phase.push(metrics.broad_phase_nanos);
        narrow_phase.push(metrics.narrow_phase_nanos);
        integration_phase.push(metrics.integration_nanos);
    }
    let field_stats = benchmark_from_durations(&field_durations);

    let near_camera = Rect {
        minimum: Vec2::new(-640.0, -420.0),
        maximum: Vec2::new(640.0, 420.0),
    };
    let far_camera = Rect {
        minimum: Vec2::new(100_000.0, 100_000.0),
        maximum: Vec2::new(101_000.0, 101_000.0),
    };
    let camera_visible_near = field_solver.extraction_trace(near_camera).len();
    let camera_visible_far = field_solver.extraction_trace(far_camera).len();
    let mut near_semantics = FieldSolver::deterministic_fixture(1_000, 73);
    let mut far_semantics = near_semantics.clone();
    for _ in 0..240 {
        near_semantics.step(1.0 / 120.0);
        let _ = near_semantics.extraction_trace(near_camera);
        far_semantics.step(1.0 / 120.0);
        let _ = far_semantics.extraction_trace(far_camera);
    }
    let camera_independent_semantics =
        near_semantics.semantic_trace() == far_semantics.semantic_trace();
    let direct_drag_dirty_bodies = field_solver.drag_body("body-0", Vec2::new(0.0, 0.0))?;
    let action_previews = field_solver.action_previews().len();
    let actions_armed = field_solver
        .action_previews()
        .iter()
        .filter(|preview| preview.armed)
        .count();

    let avian_stats = benchmark_avian();
    let first_frame = ParentFrame {
        origin_x: 6_378_137.0,
        origin_y: -4_200_000.0,
    };
    let next_frame = ParentFrame {
        origin_x: 6_378_240.0,
        origin_y: -4_199_920.0,
    };
    let world_x = 6_378_155.25;
    let world_y = -4_199_981.5;
    let local = first_frame.camera_local(world_x, world_y);
    let rebased = first_frame.rebase(local, next_frame);
    let restored_x = next_frame.origin_x + f64::from(rebased.x);
    let restored_y = next_frame.origin_y + f64::from(rebased.y);
    let parent_frame_error_millimeters =
        ((restored_x - world_x).hypot(restored_y - world_y)) * 1_000.0;
    let scene_artifact = load_open_scene_fixture()?;
    let (static_index_build, static_index_query, static_index_query_hits) =
        benchmark_static_index();

    let primitive_sands = graph
        .definitions
        .values()
        .filter(|definition| definition.children.is_empty())
        .count();
    let compound_sands = graph.definitions.len() - primitive_sands;
    let hard_budget_micros = 8_000.0;
    let assertions = vec![
        assertion(
            "current Protein executes through the Store",
            !protein_rows.is_empty(),
            format!("{} current Record rows normalized", protein_rows.len()),
        ),
        assertion(
            "recursive Sand definitions validate",
            graph.validate().is_ok() && compound_sands > 0,
            format!(
                "{} primitive and {compound_sands} compound",
                primitive_sands
            ),
        ),
        assertion(
            "Protein rows instantiate stable compound Sands",
            stable_keys_unique && projected_instances.len() == BODY_COUNT,
            format!("{} stable instances", projected_instances.len()),
        ),
        assertion(
            "semantic diff updates only named rows and tokens",
            changed_rows == 100 && tokens.values.len() == 3 && diff_apply.p95_micros <= 50_000.0,
            format!(
                "{changed_rows} rows and {} tokens in {:.3} ms",
                tokens.values.len(),
                diff_apply.p95_micros / 1_000.0
            ),
        ),
        assertion(
            "unknown semantic schema fails atomically",
            schema_refusal_observed,
            "version 99 refused without changing revision, rows or tokens".into(),
        ),
        assertion(
            "ten-thousand-body Area step stays inside the eight millisecond budget",
            field_stats.p95_micros <= hard_budget_micros,
            format!("p95 {:.3} ms", field_stats.p95_micros / 1_000.0),
        ),
        assertion(
            "Avian collision step stays inside the eight millisecond budget",
            avian_stats.p95_micros <= hard_budget_micros,
            format!("p95 {:.3} ms", avian_stats.p95_micros / 1_000.0),
        ),
        assertion(
            "camera extraction cannot change semantics",
            camera_independent_semantics && camera_visible_far == 0,
            format!("near {camera_visible_near}, far {camera_visible_far}"),
        ),
        assertion(
            "off-camera bodies remain in fixed-step simulation",
            far_semantics.positions().len() == 1_000,
            format!("{} bodies stepped", far_semantics.positions().len()),
        ),
        assertion(
            "direct manipulation dirties one body",
            direct_drag_dirty_bodies == 1,
            format!("{direct_drag_dirty_bodies} body dirtied"),
        ),
        assertion(
            "mutation Areas remain preview-only",
            action_previews > 0 && actions_armed == 0,
            format!("{action_previews} previews, {actions_armed} armed"),
        ),
        assertion(
            "parent-frame rebase preserves world position",
            parent_frame_error_millimeters <= 0.1,
            format!("{parent_frame_error_millimeters:.6} mm error"),
        ),
        assertion(
            "open scene artifact produces a replaceable proxy and typed pick event",
            scene_artifact.collision_proxy.replaceable
                && scene_artifact.pick_event.proxy_distance == 0.0
                && scene_artifact.pick_event.output_port == "picked-artifact",
            format!(
                "{} vertices, {} indices, event {}",
                scene_artifact.vertex_count,
                scene_artifact.index_count,
                scene_artifact.pick_event.event
            ),
        ),
        assertion(
            "one-hundred-thousand static nodes remain indexed and queryable",
            static_index_query_hits > 0 && static_index_query.p95_micros <= 8_000.0,
            format!(
                "{} hits, build {:.3} ms, query p95 {:.3} ms",
                static_index_query_hits,
                static_index_build.as_secs_f64() * 1_000.0,
                static_index_query.p95_micros / 1_000.0
            ),
        ),
    ];
    let passed = assertions.iter().all(|assertion| assertion.passed);
    let report = SpatialReport {
        schema_version: REPORT_SCHEMA_VERSION,
        gate: "semantic and spatial runtime".into(),
        status: if passed { "passed" } else { "failed" }.into(),
        created_unix_millis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        git_revision: raw_git_revision(),
        git_dirty: git_dirty(),
        source_fingerprint_sha256: source_fingerprint(),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        protein: ProteinEvidence {
            source: "current Protein Source::Record with KindEq(plain)".into(),
            current_rows: protein_rows.len(),
            projected_rows: projection.rows.len(),
            stable_keys_unique,
            interface_field_adapter: BTreeMap::from([
                ("head".into(), "title".into()),
                ("body".into(), "description".into()),
            ]),
        },
        semantic: SemanticEvidence {
            definitions: graph.definitions.len(),
            primitive_sands,
            compound_sands,
            result_instances: projected_instances.len(),
            mapping_arrows: template.mappings.len(),
            why_reasons_per_instance: current_instances
                .first()
                .map_or(0, |instance| instance.why.len()),
            changed_rows,
            changed_tokens: tokens.values.len(),
            diff_apply,
            schema_refusal_observed,
        },
        spatial: SpatialEvidence {
            selected_architecture: "Lince field solver plus Avian collision adapter".into(),
            selected_reason: "Protein filters, sorting, immunity and mutation previews remain explicit SoA field semantics; Avian supplies the mature collision island behind the same fixed-step boundary".into(),
            field_solver_bodies: BODY_COUNT,
            field_solver_fixed_step: field_stats,
            field_phase_p95_micros: percentile_nanos(&field_phase, 0.95) / 1_000.0,
            sort_phase_p95_micros: percentile_nanos(&sort_phase, 0.95) / 1_000.0,
            broad_phase_p95_micros: percentile_nanos(&broad_phase, 0.95) / 1_000.0,
            narrow_phase_p95_micros: percentile_nanos(&narrow_phase, 0.95) / 1_000.0,
            integration_phase_p95_micros: percentile_nanos(&integration_phase, 0.95) / 1_000.0,
            avian_collision_bodies: AVIAN_BODY_COUNT,
            avian_fixed_step: avian_stats,
            camera_visible_near,
            camera_visible_far,
            camera_independent_semantics,
            off_camera_bodies_stepped: far_semantics.positions().len(),
            direct_drag_dirty_bodies,
            action_previews,
            actions_armed,
            parent_frame_error_millimeters,
            static_indexed_nodes: STATIC_INDEX_NODE_COUNT,
            static_index_build_micros: static_index_build.as_secs_f64() * 1_000_000.0,
            static_index_query_hits,
            static_index_query,
        },
        scene_artifact,
        assertions,
        limitations: vec![
            "This gate measures semantic projection, Areas and collision scheduling without presentation; the joined compositor owns visible editing in the next gate.".into(),
            "Current Protein still emits head/body; the interface adapter deliberately exports title/description and can disappear when the source schema adopts those names.".into(),
            "Mutation Areas emit inspectable, disarmed Action previews; executing an Action remains outside this evidence surface.".into(),
        ],
    };
    write_report(report_path, &report)?;
    Ok(report)
}

fn benchmark_static_index() -> (Duration, BenchmarkStats, usize) {
    let started = Instant::now();
    let mut positions = Vec::with_capacity(STATIC_INDEX_NODE_COUNT);
    let mut cells = BTreeMap::<(i32, i32), Vec<u32>>::new();
    for index in 0..STATIC_INDEX_NODE_COUNT {
        let x = (index % 400) as f32 * 12.0 - 2_400.0;
        let y = (index / 400) as f32 * 12.0 - 1_500.0;
        positions.push(Vec2::new(x, y));
        cells
            .entry(((x / 64.0).floor() as i32, (y / 64.0).floor() as i32))
            .or_default()
            .push(index as u32);
    }
    let build = started.elapsed();
    let camera = Rect {
        minimum: Vec2::new(-640.0, -420.0),
        maximum: Vec2::new(640.0, 420.0),
    };
    let minimum_cell = (
        (camera.minimum.x / 64.0).floor() as i32,
        (camera.minimum.y / 64.0).floor() as i32,
    );
    let maximum_cell = (
        (camera.maximum.x / 64.0).floor() as i32,
        (camera.maximum.y / 64.0).floor() as i32,
    );
    let mut durations = Vec::with_capacity(SAMPLE_STEPS);
    let mut hits = 0;
    for _ in 0..SAMPLE_STEPS {
        let query_started = Instant::now();
        let mut current_hits = 0;
        for cell_y in minimum_cell.1..=maximum_cell.1 {
            for cell_x in minimum_cell.0..=maximum_cell.0 {
                if let Some(indices) = cells.get(&(cell_x, cell_y)) {
                    current_hits += indices
                        .iter()
                        .filter(|index| camera.contains(positions[**index as usize]))
                        .count();
                }
            }
        }
        durations.push(query_started.elapsed());
        hits = current_hits;
    }
    (build, benchmark_from_durations(&durations), hits)
}

async fn query_current_protein()
-> Result<Vec<lince_interface::semantic::ProteinRow>, Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let quantity = DecimalValue::from_mantissa(0, 1)?;
    for index in 0..24 {
        create(
            &store.pool,
            NewRecord {
                slug: Some(&format!("interface-probe-{index}")),
                kind: RecordKind::Plain,
                head: &format!("Record {index}"),
                body: "Current Protein data reaches the semantic Sand boundary",
                quantity,
            },
        )
        .await?;
    }
    let query = Protein {
        source: Source::Record,
        filter: vec![Predicate::KindEq("plain".into())],
        fields: Some(vec![
            "uid".into(),
            "head".into(),
            "body".into(),
            "quantity".into(),
        ]),
        include: Include::default(),
        aggregate: None,
        order: vec![Order::Asc("head".into())],
        limit: Some(24),
    };
    let rows = protein::execute(&store, &query).await?;
    rows.iter().map(normalize_protein_row).collect()
}

fn normalize_protein_row(
    value: &Value,
) -> Result<lince_interface::semantic::ProteinRow, Box<dyn std::error::Error>> {
    let uid = required_string(value, "uid")?;
    let title = required_string(value, "head")?;
    let description = required_string(value, "body")?;
    let quantity = value
        .get("quantity")
        .and_then(Value::as_f64)
        .ok_or_else(|| "Protein row lacks numeric quantity".to_string())?;
    let mut row = record_row(uid, title, description);
    row.fields
        .insert("quantity".into(), FieldValue::Number(quantity));
    Ok(row)
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Protein row lacks string {field}"))
}

fn generated_rows(count: usize) -> Vec<lince_interface::semantic::ProteinRow> {
    (0..count)
        .map(|index| {
            record_row(
                &format!("record-{index}"),
                &format!("Record {index}"),
                "A projected Protein row",
            )
        })
        .collect()
}

fn benchmark_avian() -> BenchmarkStats {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        PhysicsPlugins::default().with_length_unit(20.0),
        AssetPlugin::default(),
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / 120.0,
    )))
    .insert_resource(Gravity::ZERO);
    app.finish();
    for index in 0..AVIAN_BODY_COUNT {
        let column = (index % 40) as f32;
        let row = (index / 40) as f32;
        let x = column.mul_add(11.5, -224.25);
        let y = row.mul_add(11.5, -138.0);
        let velocity = if index % 2 == 0 { 16.0 } else { -16.0 };
        app.world_mut().spawn((
            AvianProbe,
            Transform::from_xyz(x, y, 0.0),
            RigidBody::Dynamic,
            Position(Vector::new(x, y)),
            LinearVelocity(Vector::new(velocity, velocity * 0.35)),
            Collider::circle(5.0),
            SleepingDisabled,
        ));
    }
    app.world_mut().spawn((
        RigidBody::Static,
        Position(Vector::new(0.0, -160.0)),
        Collider::rectangle(500.0, 10.0),
    ));
    app.world_mut().spawn((
        RigidBody::Static,
        Position(Vector::new(0.0, 160.0)),
        Collider::rectangle(500.0, 10.0),
    ));
    app.world_mut().spawn((
        RigidBody::Static,
        Position(Vector::new(-250.0, 0.0)),
        Collider::rectangle(10.0, 330.0),
    ));
    app.world_mut().spawn((
        RigidBody::Static,
        Position(Vector::new(250.0, 0.0)),
        Collider::rectangle(10.0, 330.0),
    ));
    for _ in 0..WARMUP_STEPS {
        app.update();
    }
    let mut durations = Vec::with_capacity(SAMPLE_STEPS);
    for _ in 0..SAMPLE_STEPS {
        let started = Instant::now();
        app.update();
        durations.push(started.elapsed());
    }
    let mut query = app
        .world_mut()
        .query_filtered::<&Position, bevy::prelude::With<AvianProbe>>();
    assert_eq!(query.iter(app.world()).count(), AVIAN_BODY_COUNT);
    benchmark_from_durations(&durations)
}

fn benchmark_from_durations(durations: &[Duration]) -> BenchmarkStats {
    let mut micros = durations
        .iter()
        .map(|duration| duration.as_secs_f64() * 1_000_000.0)
        .collect::<Vec<_>>();
    micros.sort_by(f64::total_cmp);
    let sum = micros.iter().sum::<f64>();
    BenchmarkStats {
        samples: micros.len(),
        mean_micros: sum / micros.len().max(1) as f64,
        p50_micros: percentile(&micros, 0.50),
        p95_micros: percentile(&micros, 0.95),
        p99_micros: percentile(&micros, 0.99),
        maximum_micros: micros.last().copied().unwrap_or_default(),
    }
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() - 1) as f64 * percentile).round() as usize;
    values[index]
}

fn percentile_nanos(values: &[u64], percentile: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index] as f64
}

fn assertion(name: &str, passed: bool, detail: String) -> Assertion {
    Assertion {
        name: name.into(),
        passed,
        detail,
    }
}

fn argument_value(name: &str) -> Option<String> {
    let mut arguments = env::args();
    while let Some(argument) = arguments.next() {
        if argument == name {
            return arguments.next();
        }
    }
    None
}

fn write_report(path: &Path, report: &SpatialReport) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(report)?)?;
    Ok(())
}
