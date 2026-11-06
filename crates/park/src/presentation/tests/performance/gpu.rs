use super::{
    schedules::{Clock, frame_pixels, summary},
    *,
};
use bevy::{diagnostic::DiagnosticsStore, render::diagnostic::DiagnosticsRecorder};

#[derive(Resource, Default)]
struct MainFrames(u64);

fn drain(canvas: &mut Presentation, enabled: bool) {
    canvas
        .renderer
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(5)),
        })
        .unwrap();
    if enabled {
        canvas
            .app
            .sub_app_mut(RenderApp)
            .world_mut()
            .resource_mut::<DiagnosticsRecorder>()
            .begin_frame();
    }
    canvas.app.world_mut().run_schedule(PreUpdate);
}

fn clear(canvas: &mut Presentation) {
    for diagnostic in canvas
        .app
        .world_mut()
        .resource_mut::<DiagnosticsStore>()
        .iter_mut()
    {
        diagnostic.clear_history();
    }
}

fn diagnostics(canvas: &Presentation) -> Vec<serde_json::Value> {
    let mut values = canvas
        .app
        .world()
        .resource::<DiagnosticsStore>()
        .iter()
        .filter(|diagnostic| diagnostic.path().to_string().starts_with("render/"))
        .filter(|diagnostic| diagnostic.history_len() > 0)
        .map(|diagnostic| {
            let values = diagnostic.values().copied().collect::<Vec<_>>();
            assert!(
                values.len() <= 256
                    && values
                        .iter()
                        .all(|value| value.is_finite() && *value >= 0.0)
            );
            serde_json::json!({
                "path": diagnostic.path().to_string(), "suffix": diagnostic.suffix,
                "values": values,
            })
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    assert!(values.len() <= 2048);
    values
}

#[test]
#[ignore = "private hardware native-pass diagnostics with isolated frame identity and exact output pixels"]
fn native_pass_hardware_intervals_belong_to_one_completed_full_or_pixel_frame() {
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(checks.first().is_some_and(|check| {
        check
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    }));
    assert!(
        checks.iter().all(|check| check.error.is_none()),
        "{checks:?}"
    );
    let enabled = std::env::var_os("LINCE_PARK_GPU_DIAGNOSTICS").is_some();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut canvas, _) = super::offscreen::fixture(directory.path(), "visible-only", 0);
    let required = wgpu::Features::TIMESTAMP_QUERY
        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
    assert!(
        !enabled || canvas.renderer.device.features().contains(required),
        "Native pass hardware measurements are unavailable: {:?}",
        canvas.renderer.device.features()
    );
    assert_eq!(
        canvas
            .app
            .sub_app_mut(RenderApp)
            .world()
            .contains_resource::<DiagnosticsRecorder>(),
        enabled
    );
    canvas
        .app
        .init_resource::<MainFrames>()
        .add_systems(Last, |mut count: ResMut<MainFrames>| count.0 += 1);
    for _ in 0..12 {
        drop(canvas.probe_frame().unwrap());
    }
    let reference = frame_pixels(&canvas.probe_frame().unwrap());
    for bytes in &reference {
        assert!(
            bytes
                .chunks_exact(4)
                .filter(|pixel| pixel[0] < 30 && pixel[1] > 230 && pixel[2] < 30)
                .count()
                > 20000
        );
        let glyphs = (0..8)
            .flat_map(|index| {
                (200..280).flat_map(move |y| {
                    ((34 + index * 72)..(102 + index * 72)).map(move |x| (y * 640 + x) * 4)
                })
            })
            .filter(|offset| {
                bytes[*offset..*offset + 3]
                    .iter()
                    .all(|channel| *channel < 180)
            })
            .count();
        assert!(glyphs > 200, "Native labels are not drawn: {glyphs}");
    }
    drain(&mut canvas, enabled);
    clear(&mut canvas);
    let mut samples = Vec::new();
    let mut totals = Vec::new();
    for pixel_only in [false, true] {
        let mut readings = Vec::new();
        for sample in 0..24 {
            let main_before = canvas.app.world().resource::<MainFrames>().0;
            let clock = Clock::start();
            let frame = if pixel_only {
                canvas.pixel_frame().unwrap()
            } else {
                canvas.probe_frame().unwrap()
            };
            frame.wait_for_probe().unwrap();
            readings.push(clock.finish());
            let main_after = canvas.app.world().resource::<MainFrames>().0;
            assert_eq!(main_after - main_before, u64::from(!pixel_only));
            let drain_started = Instant::now();
            drain(&mut canvas, enabled);
            let drain_wall = drain_started.elapsed();
            let values = diagnostics(&canvas);
            let ui_values = values
                .iter()
                .filter(|value| value["path"].as_str().unwrap().ends_with("/ui/elapsed_gpu"))
                .flat_map(|value| value["values"].as_array().unwrap())
                .filter_map(serde_json::Value::as_f64)
                .collect::<Vec<_>>();
            if enabled {
                assert!(
                    !ui_values.is_empty() && ui_values.iter().any(|value| *value > 0.0),
                    "No available native UI hardware timestamps for this frame: {values:?}"
                );
                for value in &values {
                    if let Some(prefix) =
                        value["path"].as_str().unwrap().strip_suffix("elapsed_cpu")
                    {
                        let gpu_path = format!("{prefix}elapsed_gpu");
                        let gpu = values
                            .iter()
                            .find(|value| value["path"] == gpu_path)
                            .expect("A native CPU pass has no available GPU counterpart");
                        assert_eq!(
                            value["values"].as_array().unwrap().len(),
                            gpu["values"].as_array().unwrap().len()
                        );
                    }
                }
            } else {
                assert!(values.is_empty());
            }
            if sample == 23 {
                assert_eq!(frame_pixels(&frame), reference);
            }
            samples.push(serde_json::json!({
                "pixel_only": pixel_only, "sample": sample,
                "epoch": samples.len() + 1, "main_updates": main_after - main_before,
                "diagnostic_drain_ns_outside_host_frame_timer": drain_wall.as_nanos(),
                "one_outstanding_diagnostic_frame": true, "diagnostics": values,
            }));
            clear(&mut canvas);
        }
        totals.push(serde_json::json!({"pixel_only":pixel_only,
            "host_frame": summary(&readings), "host_samples": readings}));
    }
    assert_eq!(frame_pixels(&canvas.probe_frame().unwrap()), reference);
    let report = serde_json::json!({
        "enabled": enabled,
        "adapter": format!("{:?}", canvas.renderer.adapter.get_info()),
        "features": format!("{:?}", canvas.renderer.device.features()),
        "timestamp_period_ns": canvas.renderer.queue.get_timestamp_period(),
        "scope": "Private debug native-only two-output 640x480 fixture; 16 text-bearing 34-node trees. One completed diagnostic frame between drain/clear boundaries. Drain polls and runs PreUpdate outside the measured frame in both controls; not normal throughput or physical input latency.",
        "counts": {"samples": samples.len(), "full":24, "pixel_only":24,
            "ordered_common_cases":checks.len()},
        "gpu_scopes_are_not_additive_or_calibrated_to_host": true,
        "totals": totals, "frames": samples,
        "reference_pixels_match": true,
    });
    let path = std::path::PathBuf::from(
        std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").expect("Set a private artifact directory"),
    );
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("native-pass-reference.rgba"), reference.concat()).unwrap();
    std::fs::write(
        path.join("native-pass-hardware.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    eprintln!(
        "Native pass hardware diagnostics enabled={enabled} frames={}",
        samples.len()
    );
    canvas.finish().unwrap();
}
