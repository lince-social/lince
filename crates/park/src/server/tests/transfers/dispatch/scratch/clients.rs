use super::*;
use crate::launch::Capability;

fn selected_output(fixture: &mut Fixture, id: OutputId) {
    let serial = fixture.desktop.worker.input(Input::Output(id)).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.output == Some(id)));
}

fn shown(fixture: &mut Fixture, entity: Entity, id: [u8; 16], surface: SurfaceId, window: &ClientWindow, output: OutputId) {
    action(fixture, DesktopAction::ShowScratch);
    choose(fixture, entity);
    confirm(fixture);
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface)
        && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
            geometry.items.iter().any(|item| item.id == ItemId::Application(surface) && item.output == output))));
    assert!(association(fixture, id).unwrap().1.shown());
}

fn private(pixels: &[u8]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_scratch_clients_keep_popup_ime_drag_delayed_configure_and_capture_ownership_for_twenty_two_output_round_trips() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let engine = persisted(&runtime, directory.path());
    let (mut fixture, windows, _buffers, ids) = windowed_fullscreen::create_authorized(&runtime, directory.path(), [1000, 720], 1.25, engine, true,
        vec![Capability::Capture, Capability::WindowList]);
    let first_output = fixture.client.output.as_ref().unwrap().clone();
    fixture.desktop.server.configure_outputs(&[
        Monitor { id: OutputId(1), name: "Windowed-Fullscreen".into(), origin: [0, 0], size: [500, 720] },
        Monitor { id: OutputId(2), name: "Scratch-B".into(), origin: [500, 0], size: [500, 720] },
    ]).unwrap();
    fixture.desktop.server.registered_output_catalog().unwrap();
    fixture.desktop.invalidate_outputs().unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)));
    let second_output = fixture.client.output.as_ref().unwrap().clone();
    assert_ne!(first_output.id(), second_output.id());
    selected_output(&mut fixture, OutputId(1));
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
    let (neighbour, _neighbour_buffer, neighbour_id) = opening::map(&mut fixture);
    let note = spawn_note(&mut fixture, "Native scratch input", "Native scratch composition");
    select_native(&mut fixture, note.1);
    action(&mut fixture, DesktopAction::MarkScratch);
    action(&mut fixture, DesktopAction::HideScratch);
    let (target, placement) = application(&mut fixture, ids[0]);
    windowed_fullscreen::select(&mut fixture, ids[0]);
    let private_buffer = privacy::buffer(&fixture, [320, 240], [255, 0, 255, 255]);
    windows[0].surface.attach(Some(&private_buffer), 0, 0); windows[0].surface.damage_buffer(0, 0, 320, 240); windows[0].surface.commit();
    fixture.inspect(move |canvas| {
        canvas.world_mut().get_mut::<native::layout::LayoutBox>(target).unwrap().capture_excluded = true;
        canvas.world_mut().resource::<lince_interface::host::capture::PolicyGate>().invalidate();
    });
    action(&mut fixture, DesktopAction::MarkScratch);
    action(&mut fixture, DesktopAction::HideScratch);
    let home = association(&mut fixture, placement).unwrap().1.home_workspace();
    windowed_fullscreen::select(&mut fixture, ids[1]);
    action(&mut fixture, DesktopAction::MoveToOutput(Direction::Right));
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
    let qh = fixture.queue.handle();
    let mut bars = Vec::new();
    let mut retained = Vec::new();
    for output in [&first_output, &second_output] {
        let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let layer = fixture.client.layer_shell.as_ref().unwrap().get_layer_surface(&surface, Some(output), zwlr_layer_shell_v1::Layer::Top, "scratch-public-control".into(), &qh, ());
        layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left | zwlr_layer_surface_v1::Anchor::Right);
        layer.set_size(0, 24); layer.set_exclusive_zone(24); surface.commit(); fixture.exchange();
        let buffer = privacy::buffer(&fixture, [500, 24], [15, 190, 35, 255]);
        surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, 500, 24); surface.commit();
        retained.push(buffer); bars.push((surface, layer));
    }
    fixture.tick();
    let sessions = [capture_lifetime::output_session_for(&mut fixture, &first_output), capture_lifetime::output_session_for(&mut fixture, &second_output)];
    for cycle in 0..20 {
        let index = cycle % 2;
        let output = OutputId(index as u64 + 1);
        selected_output(&mut fixture, output);
        action(&mut fixture, DesktopAction::SetLayout(if cycle % 4 < 2 { WorkspaceLayout::Tiled } else { WorkspaceLayout::Free }));
        fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            world.get_mut::<native::topology::view::View>(root).unwrap().spatial = cycle % 4 >= 2;
        });
        let (previous_id, previous) = if index == 0 { (neighbour_id, &neighbour) } else { (ids[1], &windows[1]) };
        windowed_fullscreen::select(&mut fixture, previous_id);
        shown(&mut fixture, target, placement, ids[0], &windows[0], output);
        assert!(private(&appearance::pixels(&mut fixture, &format!("scratch-owner-{cycle}"))) > 5_000);
        if cycle == 0 {
            let serial = fixture.client.key_serial;
            fixture.desktop.key(30, true, 20).unwrap();
            fixture.until(|fixture| fixture.client.key_serial != serial);
            let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
            positioner.set_size(100, 60); positioner.set_anchor_rect(300, 40, 1, 1);
            positioner.set_anchor(xdg_positioner::Anchor::Right); positioner.set_gravity(xdg_positioner::Gravity::Right);
            let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
            let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
            let popup = xdg.get_popup(Some(&windows[0].xdg), &positioner, &qh, ());
            popup.grab(fixture.client.seat.as_ref().unwrap(), fixture.client.key_serial); surface.commit(); fixture.exchange();
            let buffer = privacy::buffer(&fixture, [100, 60], [0, 128, 255, 255]);
            surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, 100, 60); surface.commit();
            fixture.until(|fixture| fixture.desktop.server.popup_grab_active() && !fixture.desktop.popups.is_empty());
            fixture.desktop.key(30, false, 21).unwrap();
            let pixels = appearance::pixels(&mut fixture, "scratch-popup");
            let points = pixels.chunks_exact(4).enumerate().filter(|(_, pixel)| pixel[0] > 240 && (110..=145).contains(&pixel[1]) && pixel[2] < 15)
                .map(|(position, _)| [position % 1000, position / 1000]).collect::<Vec<_>>();
            assert!(points.len() > 200);
            let point = std::array::from_fn(|axis| points.iter().map(|point| point[axis] as f64 + 0.5).sum::<f64>() / points.len() as f64);
            let buttons = fixture.client.button_recipients.len();
            fixture.desktop.move_pointer(point, 22).unwrap(); fixture.tick();
            fixture.desktop.button(272, true, 23).unwrap(); fixture.tick(); fixture.desktop.button(272, false, 24).unwrap();
            fixture.until(|fixture| fixture.client.button_recipients.len() >= buttons + 2);
            assert_eq!(&fixture.client.button_recipients[buttons..], &[(Some(surface.id()), 272, true), (Some(surface.id()), 272, false)]);
            action(&mut fixture, DesktopAction::HideScratch);
            fixture.until(|fixture| !fixture.desktop.server.popup_grab_active() && fixture.desktop.popups.is_empty());
            assert_eq!(appearance::pixels(&mut fixture, "scratch-popup-hidden").chunks_exact(4)
                .filter(|pixel| pixel[0] > 240 && (110..=145).contains(&pixel[1]) && pixel[2] < 15).count(), 0);
            popup.destroy(); xdg.destroy(); surface.destroy(); positioner.destroy(); buffer.destroy();
            shown(&mut fixture, target, placement, ids[0], &windows[0], output);
        }
        if cycle == 1 {
            action(&mut fixture, DesktopAction::HideScratch);
            action(&mut fixture, DesktopAction::ShowScratch); choose(&mut fixture, note.0); confirm(&mut fixture);
            let (_, points) = fixture.inspect(move |canvas| canvas.native_test_points(note.2)).unwrap();
            let point = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() * 0.25);
            fixture.desktop.move_pointer(point, 30).unwrap(); fixture.tick();
            fixture.desktop.button(272, true, 31).unwrap(); fixture.tick(); fixture.desktop.button(272, false, 32).unwrap();
            let mut ime = ime::Ime::connect(&mut fixture);
            ime.until(&mut fixture, |fixture, ime| ime.active() && fixture.desktop.current.as_ref().is_some_and(|frame|
                frame.frame.input.editor.as_ref().is_some_and(|editor| editor.owner == note.2.to_bits())));
            ime.method.set_preedit_string("界".into(), 0, 3); ime.method.commit(ime.serial());
            ime.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.text_composing));
            let serial = ime.serial();
            action(&mut fixture, DesktopAction::HideScratch);
            ime.until(&mut fixture, |_, ime| !ime.active());
            ime.method.commit_string("late".into()); ime.method.commit(serial); ime.exchange(&mut fixture);
            assert_eq!(content(&mut fixture, note.0), "Native scratch composition");
            ime.destroy(&mut fixture);
            windowed_fullscreen::select(&mut fixture, previous_id);
            shown(&mut fixture, target, placement, ids[0], &windows[0], output);
        }
        if cycle == 2 {
            let frame = &fixture.desktop.current.as_ref().unwrap().frame;
            let item = frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(ids[0])).unwrap();
            let point = item.mapping.canvas_point(std::array::from_fn(|axis| item.mapping.surface.origin()[axis] + item.mapping.surface.size()[axis] * 0.5)).unwrap();
            fixture.desktop.move_pointer(point, 40).unwrap(); fixture.tick();
            fixture.desktop.server.pointer_button(272, true, 41); fixture.exchange();
            let manager = fixture.client.data_manager.as_ref().unwrap();
            let device = manager.get_data_device(fixture.client.seat.as_ref().unwrap(), &qh, ());
            let data = manager.create_data_source(&qh, ()); data.offer("text/plain;charset=utf-8".into()); data.set_actions(wl_data_device_manager::DndAction::Copy);
            device.start_drag(Some(&data), &windows[0].surface, None, fixture.client.pointer_serial);
            fixture.until(|fixture| fixture.desktop.server.dragging());
            let deadline = Instant::now() + Duration::from_secs(10);
            while !fixture.inspect(|canvas| canvas.world_mut().get_resource::<lince_interface::host::input::DragActive>().is_some_and(|drag| drag.0)) {
                assert!(Instant::now() < deadline, "Scratch commands did not observe the real drag");
            }
            action(&mut fixture, DesktopAction::HideScratch);
            assert!(association(&mut fixture, placement).unwrap().1.shown());
            fixture.desktop.server.pointer_motion(None, 42).unwrap(); fixture.desktop.server.pointer_button(272, false, 43);
            fixture.until(|fixture| !fixture.desktop.server.dragging());
            data.destroy(); device.release();
            let deadline = Instant::now() + Duration::from_secs(10);
            while fixture.inspect(|canvas| canvas.world_mut().get_resource::<lince_interface::host::input::DragActive>().is_some_and(|drag| drag.0)) {
                assert!(Instant::now() < deadline, "Scratch commands did not observe the real drag end");
            }
        }
        if cycle == 3 {
            let previous = fixture.client.surface_serials[&windows[0].xdg.id()];
            fixture.client.ignore_configure = true;
            action(&mut fixture, DesktopAction::Resize(Direction::Left));
            fixture.until(|fixture| fixture.client.surface_serials[&windows[0].xdg.id()] != previous);
            let serial = fixture.client.surface_serials[&windows[0].xdg.id()];
            assert!(private(&appearance::pixels(&mut fixture, "scratch-delayed-configure")) > 5_000);
            for _ in 0..20 { fixture.tick(); }
            assert_eq!(fixture.client.surface_serials[&windows[0].xdg.id()], serial);
            fixture.client.ignore_configure = false; windows[0].xdg.ack_configure(serial);
            let size = fixture.client.window_configurations[&windows[0].role.id()].0;
            let buffer = privacy::buffer(&fixture, size.map(|value| u32::try_from(value).unwrap()), [255, 0, 255, 255]);
            windows[0].surface.attach(Some(&buffer), 0, 0); windows[0].surface.damage_buffer(0, 0, size[0], size[1]); windows[0].surface.commit(); retained.push(buffer);
            fixture.until(|fixture| fixture.desktop.server.windows().iter().find(|window| window.id == ids[0])
                .is_some_and(|window| window.nodes[0].layout.size == size.map(f64::from)));
        }
        let pending = capture_lifetime::Pending::new(&mut fixture, &sessions[index].1, [500, 720]);
        capture_lifetime::submitted(&mut fixture, &pending);
        action(&mut fixture, DesktopAction::HideScratch);
        capture_lifetime::rejected(&mut fixture, &pending); pending.destroy(&mut fixture);
        fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&previous.surface));
        assert!(!association(&mut fixture, placement).unwrap().1.shown());
        assert_eq!(association(&mut fixture, placement).unwrap().1.home_workspace(), home);
        let green = 190 + cycle as u8;
        let buffer = privacy::buffer(&fixture, [500, 24], [15, green, 35, 255]);
        bars[index].0.attach(Some(&buffer), 0, 0); bars[index].0.damage_buffer(0, 0, 500, 24); bars[index].0.commit(); retained.push(buffer); fixture.tick();
        let fresh = capture_lifetime::Pending::new(&mut fixture, &sessions[index].1, [500, 720]);
        let pixels = capture_lifetime::completed(&mut fixture, &fresh); fresh.destroy(&mut fixture);
        assert_eq!(private(&pixels), 0);
        assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0].abs_diff(15) <= 3 && pixel[1].abs_diff(green) <= 3 && pixel[2].abs_diff(35) <= 3).count() > 200,
            "The real public layer's changing pixels disappeared with the hidden scratch family");
        let keys = fixture.client.key_recipients.len();
        super::super::configuration::application_input(&mut fixture, previous_id, previous);
        assert!(fixture.client.key_recipients[keys..].iter().all(|(recipient, _, _)| recipient.as_ref() != Some(&windows[0].surface.id())));
        unique(&mut fixture);
    }
    selected_output(&mut fixture, OutputId(2));
    windowed_fullscreen::select(&mut fixture, ids[1]);
    action(&mut fixture, DesktopAction::ToggleFullscreen);
    let before = state(&mut fixture);
    action(&mut fixture, DesktopAction::ShowScratch); choose(&mut fixture, target);
    activate(&mut fixture, "Show selected item here");
    assert_eq!(state(&mut fixture), before);
    assert!(!association(&mut fixture, placement).unwrap().1.shown());
    assert!(fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        world.query::<&Text>().iter(world).any(|text| text.0.contains("Leave fullscreen"))
    }));
    activate(&mut fixture, "Cancel scratch selection");
    action(&mut fixture, DesktopAction::ToggleFullscreen);
    windowed_fullscreen::select(&mut fixture, ids[1]);
    shown(&mut fixture, target, placement, ids[0], &windows[0], OutputId(2));
    windows[1].role.destroy(); windows[1].xdg.destroy(); windows[1].surface.destroy();
    fixture.until(|fixture| !fixture.desktop.server.windows().iter().any(|window| window.id == ids[1]));
    action(&mut fixture, DesktopAction::HideScratch);
    assert!(!association(&mut fixture, placement).unwrap().1.shown());
    assert_ne!(fixture.client.keyboard_focus.as_ref(), Some(&windows[1].surface));
    windowed_fullscreen::select(&mut fixture, neighbour_id);
    action(&mut fixture, DesktopAction::MoveToOutput(Direction::Right));
    assert!(fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        native::workspace::remove(world, root, home)
    }));
    selected_output(&mut fixture, OutputId(2));
    shown(&mut fixture, target, placement, ids[0], &windows[0], OutputId(2));
    action(&mut fixture, DesktopAction::UnmarkScratch);
    assert!(association(&mut fixture, placement).is_none());
    assert!(fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<&native::workspace::Workspaces>().iter(world).all(|spaces| spaces.entries.iter().all(|workspace| workspace.id != home))
    }));
    assert_eq!(content(&mut fixture, note.0), "Native scratch composition");
    super::super::configuration::application_input(&mut fixture, ids[0], &windows[0]);
    unique(&mut fixture);
    for (source, session) in sessions { session.destroy(); source.destroy(); }
    for (surface, layer) in bars { layer.destroy(); surface.destroy(); }
    fixture.exchange();
    fixture.desktop.worker.stop().unwrap();
}
