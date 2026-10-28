use super::*;
use std::io::Write;

#[path = "manipulation/lifecycle.rs"]
mod lifecycle;
#[path = "manipulation/requests.rs"]
mod requests;
#[path = "manipulation/tiled.rs"]
mod tiled;

fn opaque_buffer(harness: &Harness, size: [i32; 2]) -> wl_buffer::WlBuffer {
    colored_buffer(harness, size, [240, 20, 230, 255])
}

fn colored_buffer(harness: &Harness, size: [i32; 2], color: [u8; 4]) -> wl_buffer::WlBuffer {
    let mut file = tempfile::tempfile().unwrap();
    let pixels = color.repeat((size[0] * size[1]) as usize);
    file.write_all(&pixels).unwrap();
    let qh = harness.queue.handle();
    let pool = harness.client.shm.as_ref().unwrap().create_pool(
        file.as_fd(),
        pixels.len() as i32,
        &qh,
        (),
    );
    let buffer = pool.create_buffer(
        0,
        size[0],
        size[1],
        size[0] * 4,
        wl_shm::Format::Argb8888,
        &qh,
        (),
    );
    pool.destroy();
    buffer
}

#[test]
fn management_grabs_keep_seat_buttons_without_client_events_or_activation_authority() {
    let mut harness = Harness::new();
    let window = harness.window();
    harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    harness
        .server
        .focus(Some(harness.server.windows()[0].id), 1.into());
    harness
        .server
        .pointer_motion(Some((surface.clone(), [50.0; 2])), 1)
        .unwrap();
    harness.sync();
    let authority = harness.server.state.authority.len();
    assert!(harness.server.begin_manipulation(272));
    assert!(harness.server.manipulation_grabbed());
    harness.server.manipulation_button(272, true, 2);
    harness.server.manipulation_button(273, true, 3);
    harness
        .server
        .pointer_motion(Some((surface.clone(), [70.0; 2])), 4)
        .unwrap();
    harness.server.manipulation_button(272, false, 5);
    assert!(harness.server.manipulation_grabbed());
    harness.sync();
    assert!(harness.client.pointer_buttons.is_empty());
    assert_eq!(harness.server.state.authority.len(), authority);
    harness.server.manipulation_button(273, false, 6);
    assert!(!harness.server.manipulation_grabbed());
    assert!(harness.server.state.buttons.is_empty());
    harness
        .server
        .pointer_motion(Some((surface, [60.0; 2])), 7)
        .unwrap();
    harness.server.pointer_button(273, true, 8);
    harness.server.pointer_button(273, false, 9);
    harness.sync();
    assert_eq!(
        harness.client.pointer_buttons,
        vec![(273, true), (273, false)]
    );
    harness.server.pointer_button(272, true, 10);
    assert!(!harness.server.begin_manipulation(273));
    harness.server.clear_input(11);
    assert!(harness.server.begin_manipulation(272));
    harness.server.manipulation_button(272, true, 12);
    harness.server.clear_input(13);
    assert!(!harness.server.manipulation_grabbed());
    assert!(harness.server.state.buttons.is_empty());
}

fn client_sync(desktop: &mut crate::desktop::Desktop, harness: &mut Harness) {
    synchronize(
        &mut desktop.server,
        &harness.connection,
        &mut harness.queue,
        &mut harness.client,
    )
    .unwrap();
}

fn advance(desktop: &mut crate::desktop::Desktop, serial: u64) {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        assert!(desktop.worker.failure().is_none());
        assert!(desktop.worker.is_running());
        if let Some(frame) = desktop.worker.frame()
            && frame.input_serial >= serial
        {
            frame.frame.wait_for_probe().unwrap();
            desktop.clips = frame
                .frame
                .geometry
                .as_ref()
                .unwrap()
                .outputs
                .iter()
                .filter_map(|output| output.usable.map(|usable| (output.id, usable)))
                .collect();
            desktop.adopt(Arc::new(frame));
            desktop.focus().unwrap();
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No acknowledged manipulation frame"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn mapping(
    desktop: &crate::desktop::Desktop,
    surface: SurfaceId,
) -> lince_interface::host::ContentMapping {
    desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .geometry
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|item| item.id == lince_interface::host::ItemId::Application(surface))
        .unwrap()
        .mapping
}

fn center(mapping: lince_interface::host::ContentMapping) -> [f64; 2] {
    mapping
        .canvas_point(std::array::from_fn(|axis| {
            mapping.surface.origin()[axis] + mapping.surface.size()[axis] * 0.5
        }))
        .unwrap()
}

fn barrier(desktop: &mut crate::desktop::Desktop) -> u64 {
    let origin = desktop.current.as_ref().map_or([0.0; 2], |current| {
        let bounds = current
            .frame
            .geometry
            .as_ref()
            .and_then(|geometry| {
                geometry
                    .outputs
                    .iter()
                    .find(|output| Some(output.id) == current.frame.output)
            })
            .map_or([0.0; 2], |output| output.bounds.origin());
        std::array::from_fn(|axis| bounds[axis] + current.frame.origin[axis])
    });
    desktop
        .worker
        .input(crate::runtime::Input::Pointer(std::array::from_fn(
            |axis| (desktop.pointer[axis] - origin[axis]) as f32,
        )))
        .unwrap()
}

fn visible_client_pixels(desktop: &crate::desktop::Desktop, surface: SurfaceId) {
    let point = center(mapping(desktop, surface));
    assert!((0.0..640.0).contains(&point[0]) && (0.0..480.0).contains(&point[1]));
    let pixels = crate::presentation::probe_pixels(
        desktop.current.as_ref().unwrap().frame.dmabuf().unwrap(),
    )
    .unwrap();
    let offset = (point[1] as usize * 640 + point[0] as usize) * 4;
    let pixel = &pixels[offset..offset + 4];
    assert!(
        pixel[0] > 170 && pixel[1] < 80 && pixel[2] > 170,
        "Client pixels missing at {point:?}: {pixel:?}"
    );
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_client_modifier_drags_use_the_worker_and_preserve_clicks_after_commit_and_escape() {
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
    let mut file = tempfile::tempfile().unwrap();
    let pixels = [240_u8, 20, 230, 255].repeat(320 * 240);
    file.write_all(&pixels).unwrap();
    let qh = harness.queue.handle();
    let pool = harness.client.shm.as_ref().unwrap().create_pool(
        file.as_fd(),
        pixels.len() as i32,
        &qh,
        (),
    );
    let buffer = pool.create_buffer(0, 320, 240, 320 * 4, wl_shm::Format::Argb8888, &qh, ());
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.damage_buffer(0, 0, 320, 240);
    window.surface.commit();
    pool.destroy();
    harness.sync();
    let id = harness.server.windows()[0].id;
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
    let before = mapping(&desktop, id);
    visible_client_pixels(&desktop, id);
    let start = center(before);
    desktop.move_pointer(start, 1).unwrap();
    assert!(
        matches!(desktop.target(), crate::desktop::input::Target::Application(target, _, _) if target == id)
    );
    desktop.key(125, true, 2).unwrap();
    let stale = desktop.current.clone().unwrap();
    desktop.button(272, true, 3).unwrap();
    assert!(desktop.server.manipulation_grabbed());
    desktop.adopt(stale);
    desktop.focus().unwrap();
    assert!(desktop.native_keyboard);
    desktop
        .move_pointer([start[0] + 60.0, start[1] + 20.0], 4)
        .unwrap();
    desktop.key(125, false, 5).unwrap();
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
    let active = desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .manipulation
        .unwrap();
    let serial = desktop
        .worker
        .input(Input::Manipulation(
            crate::presentation::manipulation::Event::End {
                token: active.token + 1,
                position: [0.0; 2],
                commit: true,
            },
            0,
        ))
        .unwrap();
    advance(&mut desktop, serial);
    assert_eq!(
        desktop.current.as_ref().unwrap().frame.input.manipulation,
        Some(active)
    );
    desktop.button(272, false, 6).unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    let mut committed = mapping(&desktop, id);
    visible_client_pixels(&desktop, id);
    let moved = center(committed);
    for axis in 0..2 {
        assert!((moved[axis] - start[axis] - [60.0, 20.0][axis]).abs() < 1.0);
    }
    client_sync(&mut desktop, &mut harness);
    assert!(harness.client.pointer_buttons.is_empty());
    desktop.move_pointer(moved, 7).unwrap();
    desktop.key(125, true, 8).unwrap();
    desktop.button(273, true, 9).unwrap();
    desktop
        .move_pointer([moved[0] + 40.0, moved[1] + 30.0], 10)
        .unwrap();
    desktop.key(125, false, 11).unwrap();
    desktop.key(1, true, 12).unwrap();
    desktop.key(1, false, 13).unwrap();
    desktop.button(273, false, 14).unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    assert_eq!(mapping(&desktop, id), committed);
    visible_client_pixels(&desktop, id);
    client_sync(&mut desktop, &mut harness);
    assert!(harness.client.pointer_buttons.is_empty());
    let original = center(committed);
    desktop.move_pointer(original, 15).unwrap();
    desktop.key(125, true, 16).unwrap();
    desktop.button(272, true, 17).unwrap();
    desktop
        .move_pointer([original[0] - 20.0, original[1] + 15.0], 18)
        .unwrap();
    desktop.button(272, false, 19).unwrap();
    desktop.key(125, false, 20).unwrap();
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    committed = mapping(&desktop, id);
    visible_client_pixels(&desktop, id);
    let final_point = center(committed);
    for axis in 0..2 {
        assert!((final_point[axis] - original[axis] - [-20.0, 15.0][axis]).abs() < 1.0);
    }
    client_sync(&mut desktop, &mut harness);
    assert!(harness.client.pointer_buttons.is_empty());
    desktop.move_pointer(center(committed), 21).unwrap();
    desktop.button(273, true, 22).unwrap();
    client_sync(&mut desktop, &mut harness);
    let authority = harness.client.pointer_serial.into();
    let surface = desktop.server.windows()[0].surface.clone();
    assert!(desktop.server.state.authorized_input(authority, &surface));
    desktop.button(273, false, 23).unwrap();
    client_sync(&mut desktop, &mut harness);
    assert_eq!(
        harness.client.pointer_buttons,
        vec![(273, true), (273, false)]
    );
    let serial = barrier(&mut desktop);
    advance(&mut desktop, serial);
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    client_sync(&mut desktop, &mut harness);
    desktop.dispatch().unwrap();
    desktop.key(125, true, 24).unwrap();
    desktop.button(272, true, 25).unwrap();
    assert!(desktop.server.manipulation_grabbed());
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
            .is_none()
    );
    desktop.button(272, false, 26).unwrap();
    desktop.key(125, false, 27).unwrap();
    client_sync(&mut desktop, &mut harness);
    assert_eq!(
        harness.client.pointer_buttons,
        vec![(273, true), (273, false)]
    );
    assert!(!desktop.server.manipulation_grabbed());
    desktop.worker.stop().unwrap();
}
