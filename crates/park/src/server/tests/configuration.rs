use super::*;

#[test]
fn hidden_preview_rectangles_do_not_resize_a_live_wayland_client() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    for _ in 0..4 {
        harness.sync();
    }
    let id = harness.server.windows()[0].id;
    harness
        .server
        .configure_presented_state(id, [480, 320], WindowState::default(), true)
        .unwrap();
    for _ in 0..4 {
        harness.sync();
    }
    assert_eq!(harness.client.configures.last(), Some(&[480, 320]));
    let count = harness.client.configures.len();
    for size in [[600, 400], [800, 600], [1000, 700], [600, 400]] {
        harness
            .server
            .configure_presented_state(id, size, WindowState::default(), false)
            .unwrap();
        harness.sync();
    }
    assert_eq!(harness.client.configures.len(), count);
    let state = WindowState {
        maximized: true,
        ..Default::default()
    };
    harness
        .server
        .configure_presented_state(id, [1000, 700], state, false)
        .unwrap();
    for _ in 0..4 {
        harness.sync();
    }
    assert_eq!(harness.client.configures.last(), Some(&[480, 320]));
    assert_eq!(harness.client.configures.len(), count + 1);
    assert!(
        harness
            .client
            .window_states
            .last()
            .unwrap()
            .contains(&(xdg_toplevel::State::Maximized as u32))
    );
    harness
        .server
        .configure_presented_state(id, [600, 400], state, true)
        .unwrap();
    for _ in 0..4 {
        harness.sync();
    }
    assert_eq!(harness.client.configures.last(), Some(&[600, 400]));
    assert_eq!(harness.client.configures.len(), count + 2);
    window.role.destroy();
    window.surface.destroy();
    harness.sync();
    assert!(
        harness
            .server
            .configure_presented_state(id, [600, 400], state, false)
            .is_err()
    );
}
