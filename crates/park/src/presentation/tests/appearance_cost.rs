use super::*;
use super::{appearance::{apply, close, item, launch, open}, shortcuts::settle};
use lince_interface::{host::{DesktopAction, ItemId, OutputId, WorkspaceLayout}, native::{canvas::CanvasView, layout::LayoutBox, workspace::WorkspaceMember}};
use std::{collections::BTreeMap, time::{Duration, Instant}};

const SIZE: [u32; 2] = [1366, 768];

#[derive(serde::Serialize)]
struct Sample {
    effects: bool,
    seconds: f64,
    frames: usize,
    throughput: f64,
    median_ms: f64,
    p95_ms: f64,
    maximum_ms: f64,
    frame_ms: Vec<f64>,
}

fn quantile(sorted: &[f64], fraction: f64) -> f64 {
    sorted[((sorted.len() as f64 * fraction).ceil() as usize).saturating_sub(1).min(sorted.len() - 1)]
}

fn sample(canvas: &mut Presentation, enabled: bool) -> Sample {
    let warmup = Instant::now() + Duration::from_secs(10);
    while Instant::now() < warmup { drop(canvas.probe_frame().unwrap()); }
    let started = Instant::now();
    let mut frame_ms = Vec::new();
    while started.elapsed() < Duration::from_secs(60) {
        let frame = Instant::now();
        let image = canvas.probe_frame().unwrap();
        frame_ms.push(frame.elapsed().as_secs_f64() * 1000.0);
        drop(image);
    }
    let seconds = started.elapsed().as_secs_f64();
    assert!(!frame_ms.is_empty());
    let mut sorted = frame_ms.clone();
    sorted.sort_unstable_by(f64::total_cmp);
    Sample { effects: enabled, seconds, frames: frame_ms.len(), throughput: frame_ms.len() as f64 / seconds,
        median_ms: quantile(&sorted, 0.5), p95_ms: quantile(&sorted, 0.95), maximum_ms: *sorted.last().unwrap(), frame_ms }
}

fn pixels(frame: &Frame, output: OutputId) -> Vec<u8> {
    let buffer = if frame.output.unwrap_or(OutputId(1)) == output { frame.dmabuf().unwrap() }
        else { frame.outputs.iter().find(|image| image.id == output).unwrap().dmabuf() };
    probe_pixels(buffer).unwrap()
}

fn settings(canvas: &mut Presentation, enabled: bool) {
    open(canvas);
    apply(canvas, "Shadow radius (0–32 logical units)", if enabled { "12" } else { "0" }, SIZE);
    apply(canvas, "Background blur radius (0–24 logical units)", if enabled { "8" } else { "0" }, SIZE);
    apply(canvas, "Appearance fade (0–200 milliseconds)", "0", SIZE);
    close(canvas, SIZE);
}

fn compiler_processes() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/proc") else { return vec!["process inventory unavailable".into()]; };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else { continue; };
        let Ok(command) = std::fs::read_to_string(entry.path().join("comm")) else { continue; };
        if matches!(command.trim(), "rustc" | "cc" | "cc1" | "cc1plus" | "clang" | "clang++" | "ld" | "ld.lld" | "lld" | "mold") {
            found.push(format!("{pid}:{}", command.trim()));
        }
    }
    found.sort();
    found
}

fn density(canvas: &Presentation, owners: &[Entity]) -> BTreeMap<Entity, ([u32; 2], u32)> {
    owners.iter().map(|owner| {
        let surface = canvas.app.world().get::<native::topology::presentation::Surface>(*owner).unwrap();
        (*owner, (surface.pixels.to_array(), surface.density.to_bits()))
    }).collect()
}

fn baseline(canvas: &mut Presentation, heads: usize) -> (Frame, BTreeMap<OutputId, Vec<u8>>) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let frame = canvas.probe_frame().unwrap();
        let views = (1..=heads).map(|id| {
            let output = OutputId(id as u64);
            (output, pixels(&frame, output))
        }).collect::<BTreeMap<_, _>>();
        if views.values().all(|pixels| pixels.chunks_exact(4).filter(|pixel| pixel[0] < 30 && pixel[1] > 230 && pixel[2] < 30).count() > 200) {
            return (frame, views);
        }
        assert!(Instant::now() < deadline, "The ordinary benchmark sources were not rendered before timing");
    }
}

fn require_effect_state(canvas: &mut Presentation, before: &BTreeMap<OutputId, Vec<u8>>, faces: &[(OutputId, ItemId)], enabled: bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let frame = canvas.probe_frame().unwrap();
        let geometry = frame.geometry.as_ref().unwrap();
        let after = before.keys().map(|output| (*output, pixels(&frame, *output))).collect::<BTreeMap<_, _>>();
        let ready = faces.iter().all(|(output, id)| {
            let item = geometry.items.iter().find(|item| item.id == *id && item.output == *output).unwrap();
            let viewport = geometry.outputs.iter().find(|view| view.id == *output).unwrap();
            let changed = before[output].chunks_exact(4).zip(after[output].chunks_exact(4)).enumerate().filter(|(index, (old, new))| {
                let point = viewport.raster.logical_point([(index % SIZE[0] as usize) as f64 + 0.5, (index / SIZE[0] as usize) as f64 + 0.5]);
                let Some(local) = item.surface_point(point) else { return false; };
                let origin = item.mapping.surface.origin();
                let extent = item.mapping.surface.size();
                (0..2).all(|axis| (0.1..0.9).contains(&((local[axis] - origin[axis]) / extent[axis])))
                    && (old[0].abs_diff(new[0]) > 12 || old[2].abs_diff(new[2]) > 12)
            }).count();
            if enabled { changed > 200 } else { changed <= 4 }
        });
        if ready { return; }
        assert!(Instant::now() < deadline, "The cost comparison did not reach its requested actual pixel state: effects={enabled}");
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL, optimized code and an isolated timing run"]
fn paired_appearance_cost_records_three_warmed_release_comparisons_on_one_and_two_outputs() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let artifacts = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").map(std::path::PathBuf::from).expect("The release comparison needs a retained artifact directory");
    std::fs::create_dir_all(&artifacts).unwrap();
    let mut reports = Vec::new();
    let mut misses = Vec::new();
    for heads in [1, 2] {
        let mut canvas = launch(&directory.path().join(format!("cost-{heads}.json")), SIZE);
        let outputs = (1..=heads).map(|id| {
            let mut output = super::transfers::output(id as u64, (id - 1) as f64 * f64::from(SIZE[0]), SIZE);
            output.viewport.workspace = id as u64;
            output
        }).collect();
        canvas.outputs(outputs).unwrap();
        drop(settle(&mut canvas));
        let root = canvas.canvas_root().unwrap();
        let mut owners = Vec::new();
        let mut faces = Vec::new();
        for output in 1..=heads {
            canvas.focus_output(OutputId(output as u64)).unwrap();
            let workspace = canvas.app.world().get::<native::workspace::Workspaces>(root).unwrap().active;
            native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
            canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
            *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
            let backdrop = item(&mut canvas, root, Vec2::new(1200.0, 650.0), [0.0; 2], Color::BLACK, 0, false);
            canvas.world_mut().entity_mut(backdrop).insert(WorkspaceMember(workspace));
            owners.push(backdrop);
            for stripe in 0..100 {
                canvas.world_mut().spawn((Node { position_type: PositionType::Absolute, left: px(stripe as f32 * 12.0), top: px(0), width: px(6), height: percent(100), ..default() }, BackgroundColor(Color::WHITE), ChildOf(backdrop)));
            }
            for (index, x) in [-320.0, 0.0, 320.0].into_iter().enumerate() {
                let face = item(&mut canvas, root, Vec2::new(300.0, 320.0), [x, 0.0], Color::srgba(0.0, 0.0, 0.0, 0.3), index as i32 + 1, false);
                canvas.world_mut().entity_mut(face).insert(WorkspaceMember(workspace));
                owners.push(face);
                faces.push((OutputId(output as u64), ItemId::Native(canvas.app.world().get::<LayoutBox>(face).unwrap().id)));
                if index == 0 {
                    canvas.world_mut().spawn((Node { position_type: PositionType::Absolute, left: px(145), top: px(80), width: px(10), height: px(120), ..default() }, BackgroundColor(Color::srgb(0.0, 1.0, 0.0)), ChildOf(face)));
                }
            }
        }
        canvas.focus_output(OutputId(1)).unwrap();
        canvas.desktop_action(DesktopAction::Unfocus).unwrap();
        settings(&mut canvas, false);
        let (_, before) = baseline(&mut canvas, heads);
        let native_density = density(&canvas, &owners);
        let adapter = canvas.renderer.adapter.get_info();
        for repeat in 0..3 {
            let compilers_before = compiler_processes();
            assert!(compilers_before.is_empty(), "The release cost comparison requires compiler work to finish first: {compilers_before:?}");
            let mut samples = Vec::new();
            for enabled in if repeat == 1 { [true, false] } else { [false, true] } {
                settings(&mut canvas, enabled);
                require_effect_state(&mut canvas, &before, &faces, enabled);
                assert_eq!(density(&canvas, &owners), native_density, "Appearance admission changed ordinary source resolution");
                samples.push(sample(&mut canvas, enabled));
            }
            let off = samples.iter().find(|sample| !sample.effects).unwrap();
            let on = samples.iter().find(|sample| sample.effects).unwrap();
            let median_delta = on.median_ms - off.median_ms;
            let p95_delta = on.p95_ms - off.p95_ms;
            let compilers_after = compiler_processes();
            if !compilers_after.is_empty() { misses.push(format!("{heads} outputs/repeat {repeat}: compiler work appeared: {compilers_after:?}")); }
            if median_delta > heads as f64 || p95_delta > 2.0 * heads as f64 {
                misses.push(format!("{heads} outputs/repeat {repeat}: median delta {median_delta:.3} ms, p95 delta {p95_delta:.3} ms"));
            }
            reports.push(serde_json::json!({ "outputs": heads, "repeat": repeat, "size_per_output": SIZE,
                "warmup_seconds_per_sample": 10, "measurement_seconds_per_sample": 60,
                "scope": "native update, GPU render/export and fence completion; uncapped throughput, no physical presentation",
                "scene": "free flat workspace, striped backdrop and three translucent native faces per output; capture demand absent",
                "adapter": { "name": &adapter.name, "driver": &adapter.driver, "driver_info": &adapter.driver_info, "backend": format!("{:?}", adapter.backend) },
                "compilers_before": compilers_before, "compilers_after": compilers_after,
                "host_gpu_contention": "not inferred; retain the external quiet-run observations for P5 acceptance",
                "median_delta_ms_total": median_delta, "p95_delta_ms_total": p95_delta,
                "median_delta_ms_per_output_average": median_delta / heads as f64,
                "p95_delta_ms_per_output_average": p95_delta / heads as f64,
                "samples": &samples, "misses_so_far": &misses }));
            std::fs::write(artifacts.join("appearance-release-cost.json"), serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
        }
        canvas.finish().unwrap();
    }
    assert!(misses.is_empty(), "The paired release appearance budget failed: {misses:?}");
}
