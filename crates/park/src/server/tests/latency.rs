use super::*;
use crate::runtime::latency::monotonic;

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
