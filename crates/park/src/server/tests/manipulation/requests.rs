use super::*;

fn press(harness: &mut Harness) -> u32 {
    let id = harness.server.windows()[0].id;
    let surface = harness.server.state.windows[&id].role.wl_surface().clone();
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness
        .server
        .pointer_motion(Some((surface, [40.0; 2])), 1)
        .unwrap();
    harness.server.pointer_button(272, true, 2);
    harness.sync();
    harness.client.pointer_serial
}

#[test]
fn client_requests_require_the_held_serial_seat_window_and_client_and_cannot_replace_a_claim() {
    let mut harness = Harness::new();
    let window = harness.window();
    harness.attach(&window, [320, 240]);
    let serial = press(&mut harness);
    let seat = harness.client.seat.clone().unwrap();
    let _inhibitor = super::super::input_control_tests::shortcuts_inhibitor(&harness, &window);
    harness.sync();
    assert!(harness.server.shortcuts_inhibited());
    let unrelated = harness.window();
    harness.attach(&unrelated, [160, 120]);
    unrelated.role._move(&seat, serial);
    window.role._move(&seat, serial.wrapping_add(1));
    harness.sync();
    assert!(harness.server.manipulations().is_empty());
    assert!(!harness.server.manipulation_grabbed());
    let mut other = Harness::connect(std::mem::replace(
        &mut harness.server,
        Server::new([640, 480]).unwrap(),
    ));
    let foreign = other.window();
    other.attach(&foreign, [160, 120]);
    foreign
        .role
        ._move(other.client.seat.as_ref().unwrap(), serial);
    other.sync();
    assert!(other.server.manipulations().is_empty());
    harness.server = std::mem::replace(&mut other.server, Server::new([640, 480]).unwrap());
    let handle = harness.server.state.handle.clone();
    let _wrong_seat = harness.server.state.seats.new_wl_seat(&handle, "other");
    harness.sync();
    window
        .role
        ._move(harness.client.seat.as_ref().unwrap(), serial);
    harness.sync();
    assert!(harness.server.manipulations().is_empty());
    window.role._move(&seat, serial);
    window
        .role
        .resize(&seat, serial, xdg_toplevel::ResizeEdge::Right);
    harness.sync();
    let requests = harness.server.manipulations();
    assert_eq!(requests.len(), 1);
    assert!(harness.server.owns_manipulation(&requests[0]));
    assert_eq!(requests[0].serial, serial.into());
    assert!(requests[0].edges.is_none());
    assert!(harness.server.shortcuts_inhibited());
    harness.server.manipulation_button(272, false, 3);
    window.role._move(&seat, serial);
    harness.sync();
    assert!(harness.server.manipulations().is_empty());
    assert!(!harness.server.owns_manipulation(&requests[0]));
    harness.server.keyboard(30, true, 4, |_| false).unwrap();
    harness.sync();
    window.role._move(&seat, harness.client.key_serial);
    harness.sync();
    assert!(harness.server.manipulations().is_empty());
    harness.server.keyboard(30, false, 5, |_| false).unwrap();
}

#[test]
fn a_subsurface_press_authorizes_only_its_actual_toplevel() {
    let mut harness = Harness::new();
    let window = harness.window();
    harness.attach(&window, [320, 240]);
    let qh = harness.queue.handle();
    let child = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let _role = harness
        .client
        .subcompositor
        .as_ref()
        .unwrap()
        .get_subsurface(&child, &window.surface, &qh, ());
    let buffer = harness.buffer([80, 60]);
    child.attach(Some(&buffer), 0, 0);
    child.commit();
    window.surface.commit();
    harness.sync();
    let snapshot = &harness.server.windows()[0];
    let id = snapshot.id;
    let source = snapshot
        .nodes
        .iter()
        .find(|node| node.surface != snapshot.surface)
        .unwrap()
        .surface
        .clone();
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness
        .server
        .pointer_motion(Some((source.clone(), [7.0, 11.0])), 1)
        .unwrap();
    harness.server.pointer_button(272, true, 2);
    harness.sync();
    window.role._move(
        harness.client.seat.as_ref().unwrap(),
        harness.client.pointer_serial,
    );
    harness.sync();
    let requests = harness.server.manipulations();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].surface, id);
    assert_eq!(requests[0].source, source);
    assert!(harness.server.owns_manipulation(&requests[0]));
    harness.server.clear_input(3);
}

#[test]
fn protocol_resize_edges_keep_single_axes_corners_and_none_distinct() {
    for (edge, expected) in [
        (xdg_toplevel::ResizeEdge::None, None),
        (xdg_toplevel::ResizeEdge::Top, Some([0, -1])),
        (xdg_toplevel::ResizeEdge::Bottom, Some([0, 1])),
        (xdg_toplevel::ResizeEdge::Left, Some([-1, 0])),
        (xdg_toplevel::ResizeEdge::Right, Some([1, 0])),
        (xdg_toplevel::ResizeEdge::TopLeft, Some([-1, -1])),
        (xdg_toplevel::ResizeEdge::TopRight, Some([1, -1])),
        (xdg_toplevel::ResizeEdge::BottomLeft, Some([-1, 1])),
        (xdg_toplevel::ResizeEdge::BottomRight, Some([1, 1])),
    ] {
        let mut harness = Harness::new();
        let window = harness.window();
        harness.attach(&window, [320, 240]);
        let serial = press(&mut harness);
        window
            .role
            .resize(harness.client.seat.as_ref().unwrap(), serial, edge);
        harness.sync();
        let requests = harness.server.manipulations();
        assert_eq!(requests.len(), usize::from(expected.is_some()));
        if let Some(expected) = expected {
            assert_eq!(requests[0].edges, Some(expected));
            assert!(harness.server.owns_manipulation(&requests[0]));
        }
        harness.server.clear_input(4);
    }
}

pub(super) fn configure(
    desktop: &mut crate::desktop::Desktop,
    harness: &mut Harness,
    window: &Window,
    id: SurfaceId,
) {
    configure_client(desktop, harness, window, id);
    visible_client_pixels(desktop, id);
}

pub(super) fn configure_client(
    desktop: &mut crate::desktop::Desktop,
    harness: &mut Harness,
    window: &Window,
    id: SurfaceId,
) {
    configure_client_color(desktop, harness, window, id, [240, 20, 230, 255]);
}

pub(super) fn configure_client_color(
    desktop: &mut crate::desktop::Desktop,
    harness: &mut Harness,
    window: &Window,
    id: SurfaceId,
    color: [u8; 4],
) {
    let requested = desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .windows
        .iter()
        .find(|window| window.surface == id)
        .unwrap();
    let size = requested.size.map(|value| value as i32);
    let state = requested.state;
    desktop.server.configure_state(id, size, state).unwrap();
    client_sync(desktop, harness);
    let configured = &harness.client.window_configurations[&window.role.id()];
    assert_eq!(configured.0, size);
    let expected = state.resizing;
    assert_eq!(
        configured.1
            .contains(&(xdg_toplevel::State::Resizing as u32)),
        expected
    );
    let buffer = colored_buffer(harness, size, color);
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.damage_buffer(0, 0, size[0], size[1]);
    window.surface.commit();
    client_sync(desktop, harness);
    desktop.dispatch().unwrap();
    let serial = barrier(desktop);
    advance(desktop, serial);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn titlebar_move_and_edge_resize_use_the_original_press_and_acknowledged_client_buffers() {
    use crate::runtime::{CanvasWorker, Configuration, Input};
    use lince_interface::host::{DesktopAction, WorkspaceLayout};
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::new();
    let window = harness.window();
    let buffer = opaque_buffer(&harness, [320, 240]);
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.commit();
    harness.sync();
    let id = harness.server.windows()[0].id;
    let seat = harness.client.seat.clone().unwrap();
    let server = std::mem::replace(&mut harness.server, Server::new([640, 480]).unwrap());
    let worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [640, 480],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: None,
    })
    .unwrap();
    let mut desktop = crate::desktop::Desktop::new(server, worker, [640, 480], 1, true);
    desktop.dispatch().unwrap();
    advance(&mut desktop, 0);
    desktop
        .worker
        .input(Input::Action(DesktopAction::SetLayout(
            WorkspaceLayout::Free,
        )))
        .unwrap();
    let serial = desktop
        .worker
        .input(Input::Window(
            id,
            WindowRequest {
                maximized: Some(false),
                ..Default::default()
            },
        ))
        .unwrap();
    advance(&mut desktop, serial);
    configure(&mut desktop, &mut harness, &window, id);
    let initial = mapping(&desktop, id);
    let _inhibitor = super::super::input_control_tests::shortcuts_inhibitor(&harness, &window);
    client_sync(&mut desktop, &mut harness);
    let start = center(initial);
    desktop.move_pointer(start, 1).unwrap();
    desktop.button(272, true, 2).unwrap();
    client_sync(&mut desktop, &mut harness);
    assert!(desktop.server.shortcuts_inhibited());
    let serial = harness.client.pointer_serial;
    desktop
        .move_pointer([start[0] + 35.0, start[1] + 20.0], 3)
        .unwrap();
    window.role._move(&seat, serial);
    client_sync(&mut desktop, &mut harness);
    desktop.dispatch().unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    assert!(
        desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .manipulation
            .is_some()
    );
    let moved = center(mapping(&desktop, id));
    for axis in 0..2 {
        assert!((moved[axis] - start[axis] - [35.0, 20.0][axis]).abs() < 1.0);
    }
    desktop.button(272, false, 4).unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    configure(&mut desktop, &mut harness, &window, id);
    let original = mapping(&desktop, id);
    assert!(desktop.server.shortcuts_inhibited());
    desktop.move_pointer(center(original), 5).unwrap();
    desktop.button(272, true, 6).unwrap();
    client_sync(&mut desktop, &mut harness);
    let serial = harness.client.pointer_serial;
    window
        .role
        .resize(&seat, serial, xdg_toplevel::ResizeEdge::Right);
    client_sync(&mut desktop, &mut harness);
    desktop.dispatch().unwrap();
    desktop
        .move_pointer([center(original)[0] + 40.0, center(original)[1] + 70.0], 7)
        .unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    configure(&mut desktop, &mut harness, &window, id);
    let resized = mapping(&desktop, id);
    assert_eq!(resized.surface.size()[0], original.surface.size()[0] + 40.0);
    assert_eq!(resized.surface.size()[1], original.surface.size()[1]);
    assert!((resized.canvas.origin()[0] - original.canvas.origin()[0]).abs() < 1.0);
    desktop.button(272, false, 8).unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    configure(&mut desktop, &mut harness, &window, id);
    assert_eq!(mapping(&desktop, id).surface.size(), resized.surface.size());
    let committed = mapping(&desktop, id);
    desktop.move_pointer(center(committed), 9).unwrap();
    desktop.button(272, true, 10).unwrap();
    client_sync(&mut desktop, &mut harness);
    window.role.resize(
        &seat,
        harness.client.pointer_serial,
        xdg_toplevel::ResizeEdge::Left,
    );
    client_sync(&mut desktop, &mut harness);
    desktop.dispatch().unwrap();
    desktop
        .move_pointer([center(committed)[0] - 25.0, center(committed)[1]], 11)
        .unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    configure(&mut desktop, &mut harness, &window, id);
    desktop.key(1, true, 12).unwrap();
    desktop.key(1, false, 13).unwrap();
    desktop.button(272, false, 14).unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    configure(&mut desktop, &mut harness, &window, id);
    assert_eq!(
        mapping(&desktop, id).surface.size(),
        committed.surface.size()
    );
    assert_eq!(
        harness.client.pointer_buttons,
        vec![(272, true), (272, true), (272, true)]
    );
    desktop
        .move_pointer(center(mapping(&desktop, id)), 15)
        .unwrap();
    desktop.button(273, true, 16).unwrap();
    client_sync(&mut desktop, &mut harness);
    let surface = desktop
        .server
        .windows()
        .iter()
        .find(|window| window.id == id)
        .unwrap()
        .surface
        .clone();
    assert!(
        desktop
            .server
            .state
            .authorized_input(harness.client.pointer_serial.into(), &surface)
    );
    desktop.button(273, false, 17).unwrap();
    client_sync(&mut desktop, &mut harness);
    assert_eq!(
        harness.client.pointer_buttons,
        vec![
            (272, true),
            (272, true),
            (272, true),
            (273, true),
            (273, false)
        ]
    );
    assert!(desktop.server.shortcuts_inhibited());
    desktop.worker.stop().unwrap();
}
