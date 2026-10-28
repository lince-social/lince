use super::*;
use crate::presentation::Presentation;
use bevy::input::keyboard::Key;
use lince_interface::host::{WakeSignal, WindowRequest, WindowState};

fn invoke(canvas: &mut Presentation) {
    canvas.desktop_action(DesktopAction::OpenOperation).unwrap();
    settle(canvas);
    for character in "/item/windowed-fullscreen".chars() {
        let code = match character {
            '/' => KeyCode::Slash,
            '-' => KeyCode::Minus,
            'c' => KeyCode::KeyC,
            'd' => KeyCode::KeyD,
            'e' => KeyCode::KeyE,
            'f' => KeyCode::KeyF,
            'i' => KeyCode::KeyI,
            'l' => KeyCode::KeyL,
            'm' => KeyCode::KeyM,
            'n' => KeyCode::KeyN,
            'o' => KeyCode::KeyO,
            'r' => KeyCode::KeyR,
            's' => KeyCode::KeyS,
            't' => KeyCode::KeyT,
            'u' => KeyCode::KeyU,
            'w' => KeyCode::KeyW,
            _ => unreachable!(),
        };
        for pressed in [true, false] {
            canvas.key(code, Key::Character(character.to_string().into()), pressed.then(|| character.to_string()), pressed, false).unwrap();
        }
        settle(canvas);
    }
    for pressed in [true, false] { canvas.key(KeyCode::Enter, Key::Enter, None, pressed, false).unwrap(); }
    assert!(!settle(canvas).input.keyboard_modal);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_wayland_windowed_fullscreen_uses_operation_and_keeps_slot_bar_neighbour_and_requested_geometry() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for mode in [WorkspaceLayout::Tiled, WorkspaceLayout::Free] {
        let mut harness = Harness::new();
        harness.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Bounded fixture".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
        harness.sync();
        let first = harness.window();
        first.role.set_app_id("bounded.fixture.first".into());
        let first_buffer = paint(&mut harness, &first, [320, 240], [0, 255, 0, 255]);
        let first_id = harness.server.windows()[0].id;
        let second = harness.window();
        let second_buffer = paint(&mut harness, &second, [320, 240], [255, 0, 0, 255]);
        let second_id = harness.server.windows().iter().find(|window| window.id != first_id).unwrap().id;
        let qh = harness.queue.handle();
        let bar = harness.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let layer = harness.client.layer_shell.as_ref().unwrap().get_layer_surface(&bar, harness.client.output.as_ref(), zwlr_layer_shell_v1::Layer::Top, "bounded-fixture".into(), &qh, ());
        layer.set_size(0, 32);
        layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left | zwlr_layer_surface_v1::Anchor::Right);
        layer.set_exclusive_zone(32);
        bar.commit();
        harness.sync();
        let bar_buffer = harness.buffer([1000, 32]);
        bar.attach(Some(&bar_buffer), 0, 0);
        bar.commit();
        harness.sync();
        let mut canvas = Presentation::new([1000, 720], directory.path().join(format!("{mode:?}/assets")), WakeSignal::new(|| {})).unwrap();
        let (sender, _receiver) = std::sync::mpsc::sync_channel(32);
        canvas.host_requests(sender);
        canvas.outputs(harness.server.output_viewports().into_iter().map(|(name, viewport)| crate::presentation::Output { name, viewport }).collect()).unwrap();
        settle(&mut canvas);
        let root = {
            let world = canvas.world_mut();
            world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap()
        };
        let clients = [(&first, first_id, [0, 255, 0, 255]), (&second, second_id, [255, 0, 0, 255])];
        let mut retained = vec![first_buffer, second_buffer];
        present(&mut canvas, &mut harness, &clients, &mut retained);
        let original = member(&mut canvas, first_id);
        native::workspace_config::set_physics(canvas.world_mut(), root, original.1, false);
        canvas.world_mut().get_mut::<native::workspace::Workspaces>(root).unwrap().entries.iter_mut().find(|workspace| workspace.id == original.1).unwrap().desktop.set_mode(mode);
        canvas.focus_application(first_id);
        let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
        let before = model(&mut canvas, root, original.1).clone();
        let sizes = frame.windows.iter().map(|window| (window.surface, window.size)).collect::<std::collections::BTreeMap<_, _>>();
        for cycle in 0..20 {
            if cycle == 0 { invoke(&mut canvas); }
            else { canvas.desktop_action(DesktopAction::ToggleWindowedFullscreen).unwrap(); }
            let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
            let configured = frame.windows.iter().find(|window| window.surface == first_id).unwrap();
            assert!(configured.state.fullscreen && configured.state.windowed_fullscreen);
            assert_eq!(configured.size, sizes[&first_id]);
            assert_eq!(frame.windows.iter().find(|window| window.surface == second_id).unwrap().size, sizes[&second_id]);
            assert!(!frame.fullscreen);
            assert_eq!(model(&mut canvas, root, original.1).fullscreen(), None);
            assert!(harness.client.window_configurations[&first.role.id()].1.contains(&(xdg_toplevel::State::Fullscreen as u32)));
            assert!(harness.server.output_viewports()[0].1.usable.unwrap().origin()[1] >= 32.0);
            let serials = harness.client.serials;
            if cycle == 0 { for _ in 0..20 { present(&mut canvas, &mut harness, &clients, &mut retained); } }
            assert_eq!(harness.client.serials, serials);
            canvas.desktop_action(DesktopAction::ToggleWindowedFullscreen).unwrap();
            let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
            assert!(!frame.windows.iter().find(|window| window.surface == first_id).unwrap().state.fullscreen);
            assert_eq!(model(&mut canvas, root, original.1), &before);
            assert_eq!(member(&mut canvas, first_id), original);
            green(&frame);
        }
        canvas.desktop_action(DesktopAction::ToggleWindowedFullscreen).unwrap();
        present(&mut canvas, &mut harness, &clients, &mut retained);
        layer.set_exclusive_zone(64);
        bar.commit();
        harness.sync();
        canvas.outputs(harness.server.output_viewports().into_iter().map(|(name, viewport)| crate::presentation::Output { name, viewport }).collect()).unwrap();
        let frame = present(&mut canvas, &mut harness, &clients, &mut retained);
        assert!(frame.windows.iter().find(|window| window.surface == first_id).unwrap().state.windowed_fullscreen);
        assert_eq!(model(&mut canvas, root, original.1).item_width(original.2), before.item_width(original.2));
        canvas.desktop_action(DesktopAction::ToggleWindowedFullscreen).unwrap();
        present(&mut canvas, &mut harness, &clients, &mut retained);
        assert_eq!(model(&mut canvas, root, original.1).items(), before.items());
        layer.destroy();
        bar.destroy();
        canvas.finish().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_delayed_fullscreen_ack_keeps_the_old_buffer_without_repeating_configures_and_close_retires_mode() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::new();
    let window = harness.window();
    let mut retained = vec![paint(&mut harness, &window, [320, 240], [0, 255, 0, 255])];
    let id = harness.server.windows()[0].id;
    let mut canvas = Presentation::new([1000, 720], directory.path().join("assets"), WakeSignal::new(|| {})).unwrap();
    let (sender, _receiver) = std::sync::mpsc::sync_channel(32);
    canvas.host_requests(sender);
    let clients = [(&window, id, [0, 255, 0, 255])];
    present(&mut canvas, &mut harness, &clients, &mut retained);
    canvas.focus_application(id);
    let original = member(&mut canvas, id);
    let root = {
        let world = canvas.world_mut();
        world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap()
    };
    native::workspace_config::set_physics(canvas.world_mut(), root, original.1, false);
    harness.client.ignore_configure = true;
    canvas.desktop_action(DesktopAction::ToggleWindowedFullscreen).unwrap();
    let frame = settle(&mut canvas);
    let configuration = frame.windows.iter().find(|window| window.surface == id).unwrap();
    harness.server.configure_state(id, configuration.size.map(|value| value as i32), configuration.state).unwrap();
    harness.sync();
    let serials = harness.client.serials;
    let old_content = harness.server.windows()[0].nodes[0].content.clone();
    for _ in 0..20 {
        canvas.sync_windows(&harness.server.windows()).unwrap();
        let frame = settle(&mut canvas);
        let configuration = frame.windows.iter().find(|window| window.surface == id).unwrap();
        harness.server.configure_state(id, configuration.size.map(|value| value as i32), configuration.state).unwrap();
        harness.sync();
        assert!(Arc::ptr_eq(&old_content, &harness.server.windows()[0].nodes[0].content));
        green(&frame);
    }
    assert_eq!(harness.client.serials, serials);
    window.xdg.ack_configure(harness.client.surface_serials[&window.xdg.id()]);
    harness.client.ignore_configure = false;
    harness.sync();
    for _ in 0..3 {
        window.role.set_fullscreen(None);
        harness.sync();
        for (id, request) in harness.server.requests() { canvas.window_request(id, request); }
        present(&mut canvas, &mut harness, &clients, &mut retained);
        assert!(model(&mut canvas, root, original.1).windowed_fullscreen(original.2));
        assert_eq!(model(&mut canvas, root, original.1).fullscreen(), None);
    }
    window.role.unset_fullscreen();
    harness.sync();
    for (id, request) in harness.server.requests() { canvas.window_request(id, request); }
    present(&mut canvas, &mut harness, &clients, &mut retained);
    assert!(!model(&mut canvas, root, original.1).windowed_fullscreen(original.2));
    canvas.window_request(id, WindowRequest { fullscreen: Some(false), ..Default::default() });
    settle(&mut canvas);
    canvas.desktop_action(DesktopAction::ToggleWindowedFullscreen).unwrap();
    assert!(model(&mut canvas, root, original.1).windowed_fullscreen(original.2));
    let serials = harness.client.serials;
    let invalid = harness.server.configure_state(id, [320, 240], WindowState { windowed_fullscreen: true, ..Default::default() }).unwrap_err();
    assert_eq!(invalid.kind(), std::io::ErrorKind::InvalidInput);
    harness.sync();
    assert_eq!(harness.client.serials, serials);
    window.role.destroy();
    window.xdg.destroy();
    window.surface.destroy();
    harness.sync();
    canvas.sync_windows(&harness.server.windows()).unwrap();
    settle(&mut canvas);
    assert!(!model(&mut canvas, root, original.1).windowed_fullscreen(original.2));
    assert!(harness.server.configure_state(id, [320, 240], WindowState { windowed_fullscreen: true, ..Default::default() }).is_err());
    canvas.finish().unwrap();
}
