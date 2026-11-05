use super::*;
use crate::runtime::latency::monotonic;

#[test]
#[ignore = "private Vulkan/EGL controlled Wayland response; run tracing on and off without concurrent compilation"]
fn a_client_key_drives_verified_new_pixels_with_an_explicit_observed_proxy() {
    use crate::runtime::latency::{Dispatch, Stamp};
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::new();
    harness
        .server
        .configure_outputs(&[Monitor {
            id: OutputId(1),
            name: "Private input trace".into(),
            origin: [0, 0],
            size: [640, 480],
        }])
        .unwrap();
    let window = harness.window();
    let _red = super::pixel_tests::colored(&mut harness, &window, [255, 0, 0]);
    let previous = harness.server.windows().remove(0);
    let mut canvas = crate::presentation::Presentation::new(
        [640, 480],
        directory.path().into(),
        lince_interface::host::WakeSignal::new(|| {}),
    )
    .unwrap();
    canvas.frame().unwrap();
    canvas
        .outputs(
            harness
                .server
                .output_viewports()
                .into_iter()
                .map(|(name, viewport)| crate::presentation::Output { name, viewport })
                .collect(),
        )
        .unwrap();
    let paint = |canvas: &mut crate::presentation::Presentation,
                 windows: &[WindowSnapshot],
                 expected: [u8; 3]| {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut frames = 0;
        loop {
            canvas.sync_windows(windows).unwrap();
            let frame = canvas.probe_frame().unwrap();
            frames += 1;
            let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
            let matching = pixels
                .chunks_exact(4)
                .filter(|pixel| {
                    pixel[..3]
                        .iter()
                        .zip(expected)
                        .all(|(value, expected)| value.abs_diff(expected) < 20)
                })
                .count();
            if matching > 20_000 {
                return (pixels, frames);
            }
            assert!(
                Instant::now() < deadline,
                "The controlled client response stayed invisible after {frames} frames, matching={matching}"
            );
        }
    };
    let (red, baseline_frames) = paint(&mut canvas, &[previous.clone()], [255, 0, 0]);
    assert!(
        red.chunks_exact(4)
            .filter(|p| p[0] > 220 && p[1] < 30 && p[2] < 30)
            .count()
            > 20_000
    );
    harness.server.focus(
        Some(previous.id),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    let begin = Instant::now();
    let origin = {
        let _scope = Dispatch::new();
        let origin = Stamp::current();
        harness.server.keyboard(30, true, 1, |_| false).unwrap();
        origin
    };
    {
        let _scope = Dispatch::new();
        harness.server.keyboard(30, false, 2, |_| false).unwrap();
    }
    harness.sync();
    assert_eq!(harness.client.keys, [(30, true), (30, false)]);
    let _green = super::pixel_tests::colored(&mut harness, &window, [0, 255, 0]);
    let updated = harness.server.windows().remove(0);
    assert!(updated.nodes[0].content.version > previous.nodes[0].content.version);
    let (pixels, response_frames) = paint(&mut canvas, &[updated.clone()], [0, 255, 0]);
    let green = pixels
        .chunks_exact(4)
        .filter(|p| p[1] > 220 && p[0] < 30 && p[2] < 30)
        .count();
    assert!(green > 20_000);
    let observed =
        harness
            .server
            .observe_input_presentation(OutputId(1), &updated.nodes, monotonic(), false);
    assert_eq!(
        observed,
        usize::from(lince_interface::host::performance::enabled())
    );
    let result = serde_json::json!({"scope":"Private real Wayland key delivery followed by controlled client color response and GPU readback; observed proxy, not physical scanout or arbitrary-client causal proof","tracing":lince_interface::host::performance::enabled(),"origin":origin.map(Stamp::origin),"previous_version":previous.nodes[0].content.version,"response_version":updated.nodes[0].content.version,"matching_pixels":green,"baseline_frames":baseline_frames,"response_frames":response_frames,"input_to_verified_readback_ms":begin.elapsed().as_secs_f64()*1000.0});
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            std::path::Path::new(&path).join("client-input-visible.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    canvas.finish().unwrap();
    lince_interface::host::performance::flush();
    std::thread::sleep(Duration::from_millis(50));
}

#[test]
fn pixel_only_classification_preserves_layout_limits_and_input_regions() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _first = harness.attach(&window, [400, 300]);
    let root = harness.server.windows()[0].surface.clone();
    let initial = harness.server.state.visual(&root);
    let _second = harness.attach(&window, [400, 300]);
    let pixels = harness.server.state.visual(&root);
    assert!(initial != pixels);
    assert!(initial.same_semantics(&pixels));
    window.surface.commit();
    harness.sync();
    let callbacks = harness.server.state.visual(&root);
    assert!(pixels == callbacks);
    let _resized = harness.attach(&window, [450, 300]);
    let resized = harness.server.state.visual(&root);
    assert!(!callbacks.same_semantics(&resized));
    window.role.set_min_size(160, 90);
    window.surface.commit();
    harness.sync();
    let limits = harness.server.state.visual(&root);
    assert!(!resized.same_semantics(&limits));
    let region = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_region(&harness.queue.handle(), ());
    region.add(10, 10, 90, 90);
    window.surface.set_input_region(Some(&region));
    window.surface.commit();
    harness.sync();
    let input = harness.server.state.visual(&root);
    assert!(!limits.same_semantics(&input));
}

#[test]
#[ignore = "requires LINCE_TRACE_PERFORMANCE=1 to enable input timing"]
fn input_proxy_requires_the_forwarded_surface_and_a_newer_attached_buffer_per_output() {
    assert!(lince_interface::host::performance::enabled());
    let mut harness = Harness::new();
    let first = harness.window();
    let _first = harness.attach(&first, [400, 300]);
    let second = harness.window();
    let _second = harness.attach(&second, [400, 300]);
    let snapshots = harness.server.windows();
    let previous = snapshots
        .iter()
        .find(|window| window.surface.id().protocol_id() == first.surface.id().protocol_id())
        .unwrap()
        .clone();
    let other = snapshots
        .iter()
        .find(|window| window.id != previous.id)
        .unwrap()
        .clone();
    harness.server.focus(
        Some(previous.id),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.server.keyboard(30, false, 2, |_| false).unwrap();
    harness.sync();
    assert_eq!(harness.client.keys, [(30, true), (30, false)]);
    assert_eq!(
        harness
            .server
            .observe_input_presentation(OutputId(1), &other.nodes, monotonic(), false),
        0
    );
    assert_eq!(
        harness
            .server
            .observe_input_presentation(OutputId(1), &previous.nodes, monotonic(), false),
        0
    );
    first.surface.frame(
        &harness.queue.handle(),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    first.surface.commit();
    harness.sync();
    let callbacks_only = harness
        .server
        .windows()
        .into_iter()
        .find(|window| window.id == previous.id)
        .unwrap();
    assert_eq!(
        harness.server.observe_input_presentation(
            OutputId(1),
            &callbacks_only.nodes,
            monotonic(),
            false
        ),
        0
    );
    let _next = harness.attach(&first, [400, 300]);
    let updated = harness
        .server
        .windows()
        .into_iter()
        .find(|window| window.id == previous.id)
        .unwrap();
    assert!(updated.nodes[0].content.version > previous.nodes[0].content.version);
    assert_eq!(
        harness
            .server
            .observe_input_presentation(OutputId(1), &updated.nodes, monotonic(), false),
        1
    );
    assert_eq!(
        harness
            .server
            .observe_input_presentation(OutputId(1), &updated.nodes, monotonic(), false),
        0
    );
    assert_eq!(
        harness
            .server
            .observe_input_presentation(OutputId(2), &updated.nodes, monotonic(), false),
        1
    );
}

#[test]
#[ignore = "requires LINCE_TRACE_PERFORMANCE=1 to enable input timing"]
fn locking_and_destroying_surfaces_retire_pending_application_input_proxies() {
    assert!(lince_interface::host::performance::enabled());
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [400, 300]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    let _new = harness.attach(&window, [400, 300]);
    let nodes = harness.server.windows()[0].nodes.clone();
    let lock = harness
        .client
        .lock_manager
        .as_ref()
        .unwrap()
        .lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    assert_eq!(
        harness
            .server
            .observe_input_presentation(OutputId(1), &nodes, monotonic(), false),
        0
    );
    harness.server.lock_presented();
    harness.sync();
    lock.unlock_and_destroy();
    harness.sync();
    assert!(!harness.server.locked());
    assert_eq!(
        harness
            .server
            .observe_input_presentation(OutputId(1), &nodes, monotonic(), false),
        0
    );
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(31, true, 2, |_| false).unwrap();
    let _newer = harness.attach(&window, [400, 300]);
    let nodes = harness.server.windows()[0].nodes.clone();
    window.role.destroy();
    window.xdg.destroy();
    window.surface.destroy();
    harness.sync();
    assert!(harness.server.windows().is_empty());
    assert_eq!(
        harness
            .server
            .observe_input_presentation(OutputId(1), &nodes, monotonic(), false),
        0
    );
}
