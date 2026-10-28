use super::*;
use crate::launch::Capability;

fn public(pixels: &[u8]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| pixel[1] > 180 && pixel[1] < 220 && pixel[0] < 80 && pixel[2] < 80).count()
}

fn private(pixels: &[u8]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count()
}

fn selected(fixture: &mut Fixture, id: ItemId, window: Option<&ClientWindow>) {
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.focused == Some(id)))
        && window.map_or(fixture.desktop.native_keyboard, |window| fixture.client.keyboard_focus.as_ref() == Some(&window.surface)));
}

fn captured(fixture: &mut Fixture, session: &wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1) -> Vec<u8> {
    let pending = capture_lifetime::Pending::new(fixture, session, [1000, 720]);
    let pixels = capture_lifetime::completed(fixture, &pending);
    pending.destroy(fixture);
    assert_eq!(private(&pixels), 0);
    assert!(public(&pixels) > 200, "The public neighbouring client disappeared from filtered compact capture");
    pixels
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn actual_compact_clients_keep_popup_ime_drag_transfer_and_queued_capture_ownership_without_hidden_input_or_neighbour_configure_loops() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let engine = persisted(&runtime, directory.path());
    let (mut fixture, windows, _buffers, ids) = windowed_fullscreen::create_authorized(&runtime, directory.path(), [1000, 720], 1.25, engine, true,
        vec![Capability::Capture, Capability::WindowList]);
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
    let (neighbour, _neighbour_buffer, neighbour_id) = opening::map(&mut fixture);
    neighbour.role.set_title("Independent group neighbour".into());
    let (note, note_id, field) = spawn_note(&mut fixture, "Native input member", "Native compact input");
    let (first, first_id) = application(&mut fixture, ids[0]);
    let (second, second_id) = application(&mut fixture, ids[1]);
    let magenta = privacy::buffer(&fixture, [320, 240], [255, 0, 255, 255]);
    windows[1].surface.attach(Some(&magenta), 0, 0);
    windows[1].surface.damage_buffer(0, 0, 320, 240);
    windows[1].surface.commit();
    fixture.tick();
    fixture.inspect(move |canvas| {
        canvas.world_mut().get_mut::<native::layout::LayoutBox>(second).unwrap().capture_excluded = true;
        canvas.world_mut().resource::<lince_interface::host::capture::PolicyGate>().invalidate();
    });
    windowed_fullscreen::capture_shortcut(&mut fixture, "item.group-next", "group-next", 49, true);
    windowed_fullscreen::select(&mut fixture, ids[0]);
    action(&mut fixture, DesktopAction::CreateCompactGroup);
    choose(&mut fixture, second);
    choose(&mut fixture, note);
    confirm(&mut fixture, false);
    let expected = [PlacementId(first_id), PlacementId(second_id), PlacementId(note_id)];
    assert_eq!(group(&mut fixture, first_id).unwrap().members, expected);
    let private_label = named(&mut fixture, &format!("compact.member.{}", second.to_bits()));
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let labels = world.get::<Children>(private_label).unwrap().iter().collect::<Vec<_>>();
        for label in labels { if world.get::<Text>(label).is_some() { world.entity_mut(label).insert(TextColor(Color::srgb(1.0, 0.0, 1.0))); } }
    });
    let neighbour_serial = fixture.client.surface_serials[&neighbour.xdg.id()];
    for _ in 0..20 { fixture.tick(); }
    assert_eq!(fixture.client.surface_serials[&neighbour.xdg.id()], neighbour_serial);
    let (source, session) = capture_lifetime::output_session(&mut fixture);
    captured(&mut fixture, &session);
    let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    capture_lifetime::submitted(&mut fixture, &pending);
    windowed_fullscreen::chord_key(&mut fixture, 49);
    selected(&mut fixture, ItemId::Application(ids[1]), Some(&windows[1]));
    capture_lifetime::rejected(&mut fixture, &pending);
    pending.destroy(&mut fixture);
    assert!(private(&appearance::pixels(&mut fixture, "compact-private-owner")) > 20_000,
        "The private client's known buffer must first be visible to its owner");
    let public_update = privacy::buffer(&fixture, [320, 240], [10, 210, 40, 255]);
    neighbour.surface.attach(Some(&public_update), 0, 0);
    neighbour.surface.damage_buffer(0, 0, 320, 240);
    neighbour.surface.commit();
    fixture.tick();
    let pixels = captured(&mut fixture, &session);
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 20 && (205..=215).contains(&pixel[1]) && (35..=45).contains(&pixel[2])).count() > 200,
        "Filtered capture retained old public neighbour pixels");
    let previous_serial = fixture.client.surface_serials[&windows[1].xdg.id()];
    fixture.client.ignore_configure = true;
    action(&mut fixture, DesktopAction::Resize(Direction::Left));
    fixture.until(|fixture| fixture.client.surface_serials[&windows[1].xdg.id()] != previous_serial);
    let delayed_serial = fixture.client.surface_serials[&windows[1].xdg.id()];
    let delayed_size = fixture.client.window_configurations[&windows[1].role.id()].0;
    assert!(private(&appearance::pixels(&mut fixture, "compact-delayed-configure")) > 20_000);
    for _ in 0..20 { fixture.tick(); }
    assert_eq!(fixture.client.surface_serials[&windows[1].xdg.id()], delayed_serial);
    assert_eq!(fixture.client.surface_serials[&neighbour.xdg.id()], neighbour_serial);
    fixture.client.ignore_configure = false;
    windows[1].xdg.ack_configure(delayed_serial);
    let resized_private = privacy::buffer(&fixture, delayed_size.map(|value| u32::try_from(value).unwrap()), [255, 0, 255, 255]);
    windows[1].surface.attach(Some(&resized_private), 0, 0);
    windows[1].surface.damage_buffer(0, 0, delayed_size[0], delayed_size[1]);
    windows[1].surface.commit();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().find(|window| window.id == ids[1])
        .is_some_and(|window| window.nodes[0].layout.size == delayed_size.map(f64::from)));
    captured(&mut fixture, &session);
    let cyan = privacy::buffer(&fixture, [320, 240], [255, 255, 0, 255]);
    windows[0].surface.attach(Some(&cyan), 0, 0);
    windows[0].surface.damage_buffer(0, 0, 320, 240);
    windows[0].surface.commit();
    fixture.tick();
    selector(&mut fixture, first);
    selected(&mut fixture, ItemId::Application(ids[0]), Some(&windows[0]));
    let pixels = captured(&mut fixture, &session);
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] > 220 && pixel[2] < 40).count() > 200,
        "The public member's new real client buffer did not appear after selection");
    let qh = fixture.queue.handle();
    let serial = fixture.client.key_serial;
    fixture.desktop.key(30, true, 40).unwrap();
    fixture.until(|fixture| fixture.client.key_serial != serial);
    let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
    positioner.set_size(100, 60);
    positioner.set_anchor_rect(300, 40, 1, 1);
    positioner.set_anchor(xdg_positioner::Anchor::Right);
    positioner.set_gravity(xdg_positioner::Gravity::Right);
    let popup_surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let popup_xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&popup_surface, &qh, ());
    let popup = popup_xdg.get_popup(Some(&windows[0].xdg), &positioner, &qh, ());
    popup.grab(fixture.client.seat.as_ref().unwrap(), fixture.client.key_serial);
    popup_surface.commit(); fixture.exchange();
    let popup_buffer = privacy::buffer(&fixture, [100, 60], [0, 128, 255, 255]);
    popup_surface.attach(Some(&popup_buffer), 0, 0); popup_surface.damage_buffer(0, 0, 100, 60); popup_surface.commit();
    fixture.until(|fixture| fixture.desktop.server.popup_grab_active() && !fixture.desktop.popups.is_empty());
    fixture.desktop.key(30, false, 41).unwrap();
    let pixels = appearance::pixels(&mut fixture, "compact-popup");
    let orange = pixels.chunks_exact(4).enumerate().filter(|(_, pixel)| pixel[0] > 240 && (110..=145).contains(&pixel[1]) && pixel[2] < 15)
        .map(|(index, _)| [index % 1000, index / 1000]).collect::<Vec<_>>();
    assert!(orange.len() > 200);
    let pixel = std::array::from_fn(|axis| orange.iter().map(|point| point[axis] as f64 + 0.5).sum::<f64>() / orange.len() as f64);
    let point = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(OutputId(1)).unwrap().raster().logical_point(pixel);
    let buttons = fixture.client.button_recipients.len();
    fixture.desktop.move_pointer(point, 42).unwrap(); fixture.tick();
    fixture.desktop.button(272, true, 43).unwrap(); fixture.tick(); fixture.desktop.button(272, false, 44).unwrap();
    fixture.until(|fixture| fixture.client.button_recipients.len() >= buttons + 2);
    assert_eq!(&fixture.client.button_recipients[buttons..], &[(Some(popup_surface.id()), 272, true), (Some(popup_surface.id()), 272, false)]);
    action(&mut fixture, DesktopAction::NextCompactMember);
    selected(&mut fixture, ItemId::Application(ids[1]), Some(&windows[1]));
    fixture.until(|fixture| !fixture.desktop.server.popup_grab_active() && fixture.desktop.popups.is_empty());
    let dismissed = appearance::pixels(&mut fixture, "compact-popup-dismissed");
    assert_eq!(dismissed.chunks_exact(4).filter(|pixel| pixel[0] > 240 && (110..=145).contains(&pixel[1]) && pixel[2] < 15).count(), 0);
    popup.destroy(); popup_xdg.destroy(); popup_surface.destroy(); positioner.destroy(); popup_buffer.destroy();
    for step in 0..20 {
        windowed_fullscreen::chord_key(&mut fixture, 49);
        let index = (step + 2) % 3;
        let target = [ItemId::Application(ids[0]), ItemId::Application(ids[1]), ItemId::Native(note_id)][index];
        selected(&mut fixture, target, if index < 2 { Some(&windows[index]) } else { None });
        if index < 2 { super::super::configuration::application_input(&mut fixture, ids[index], &windows[index]); }
        unique(&mut fixture);
    }
    assert_eq!(fixture.client.surface_serials[&neighbour.xdg.id()], neighbour_serial, "Changing compact members reconfigured the independent neighbour");
    selector(&mut fixture, note);
    let (_, points) = fixture.inspect(move |canvas| canvas.native_test_points(field)).unwrap();
    let point = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() * 0.25);
    fixture.desktop.move_pointer(point, 50).unwrap(); fixture.tick();
    fixture.desktop.button(272, true, 51).unwrap(); fixture.tick(); fixture.desktop.button(272, false, 52).unwrap();
    let mut ime = ime::Ime::connect(&mut fixture);
    ime.until(&mut fixture, |fixture, ime| ime.active() && fixture.desktop.current.as_ref().is_some_and(|frame|
        frame.frame.input.editor.as_ref().is_some_and(|editor| editor.owner == field.to_bits())));
    ime.method.set_preedit_string("界".into(), 0, 3); ime.method.commit(ime.serial());
    ime.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.text_composing));
    let old_serial = ime.serial();
    action(&mut fixture, DesktopAction::NextCompactMember);
    selected(&mut fixture, ItemId::Application(ids[0]), Some(&windows[0]));
    ime.until(&mut fixture, |_, ime| !ime.active());
    ime.method.commit_string("late".into()); ime.method.commit(old_serial); ime.exchange(&mut fixture);
    assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(field).unwrap().value().to_string()), "Native compact input");
    ime.destroy(&mut fixture);
    let item = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(ids[0])).unwrap();
    let origin = item.mapping.surface.origin(); let size = item.mapping.surface.size();
    let point = item.mapping.canvas_point([origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5]).unwrap();
    fixture.desktop.move_pointer(point, 60).unwrap(); fixture.tick();
    fixture.desktop.server.pointer_button(272, true, 61); fixture.exchange();
    let manager = fixture.client.data_manager.as_ref().unwrap();
    let device = manager.get_data_device(fixture.client.seat.as_ref().unwrap(), &qh, ());
    let data = manager.create_data_source(&qh, ());
    data.offer("text/plain;charset=utf-8".into()); data.set_actions(wl_data_device_manager::DndAction::Copy);
    let icon = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let icon_buffer = privacy::buffer(&fixture, [32, 24], [255, 255, 255, 255]);
    device.start_drag(Some(&data), &windows[0].surface, Some(&icon), fixture.client.pointer_serial);
    icon.attach(Some(&icon_buffer), 0, 0); icon.damage_buffer(0, 0, 32, 24); icon.commit();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if fixture.inspect(|canvas| canvas.world_mut().get_resource::<lince_interface::host::input::DragActive>().is_some_and(|drag| drag.0)) { break; }
        assert!(Instant::now() < deadline, "The real drag did not reach compact command availability");
    }
    action(&mut fixture, DesktopAction::NextCompactMember);
    assert_eq!(group(&mut fixture, first_id).unwrap().active, PlacementId(first_id));
    fixture.desktop.server.pointer_motion(None, 62).unwrap(); fixture.desktop.server.pointer_button(272, false, 63);
    fixture.until(|fixture| !fixture.desktop.server.dragging());
    data.destroy(); device.release(); icon.destroy(); icon_buffer.destroy();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if fixture.inspect(|canvas| canvas.world_mut().get_resource::<lince_interface::host::input::DragActive>().is_none_or(|drag| !drag.0)) { break; }
        assert!(Instant::now() < deadline, "The real drag-end did not restore compact command availability");
    }
    action(&mut fixture, DesktopAction::Workspace(2));
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
    action(&mut fixture, DesktopAction::Workspace(1));
    windowed_fullscreen::select(&mut fixture, ids[0]);
    let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    capture_lifetime::submitted(&mut fixture, &pending);
    action(&mut fixture, DesktopAction::MoveToWorkspace(2));
    capture_lifetime::rejected(&mut fixture, &pending); pending.destroy(&mut fixture);
    let moved = expected;
    assert!(fixture.inspect(move |canvas| {
        let world = canvas.world_mut(); let spaces = world.query::<&native::workspace::Workspaces>().single(world).unwrap();
        moved.iter().all(|id| spaces.entries.iter().filter(|space| space.desktop.items().iter().any(|item| item.id == *id)).count() == 1)
            && spaces.entries.iter().find(|space| space.id == spaces.active).unwrap().desktop.compact_group(moved[0]).is_some()
    }));
    action(&mut fixture, DesktopAction::MoveToWorkspace(1));
    unique(&mut fixture);
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Free));
    captured(&mut fixture, &session);
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
    let selector = named(&mut fixture, &format!("compact.member.{}", note.to_bits()));
    magnification::focus_entity(&mut fixture, selector, "Select native member");
    let (_, points) = fixture.inspect(move |canvas| canvas.native_test_points(selector)).unwrap();
    let point: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() * 0.25);
    let captured_selector = fixture.desktop.current.as_ref().unwrap().frame.input.native_pointer.target(point).unwrap();
    let (_, release) = fixture.pause();
    windows[0].role.destroy(); windows[0].xdg.destroy(); windows[0].surface.destroy(); fixture.exchange(); fixture.desktop.dispatch().unwrap();
    fixture.desktop.worker.input(Input::CapturedButton(bevy::input::mouse::MouseButton::Left, true, point.map(|value| value as f32), 0, captured_selector)).unwrap();
    let serial = fixture.desktop.worker.input(Input::Button(bevy::input::mouse::MouseButton::Left, false, point.map(|value| value as f32), 0)).unwrap();
    release.send(()).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
    assert_eq!(group(&mut fixture, second_id).unwrap().active, PlacementId(second_id));
    assert!(!group(&mut fixture, second_id).unwrap().members.contains(&PlacementId(first_id)));
    let (_replacement, _buffer, replacement) = opening::map(&mut fixture);
    let (_, replacement_id) = application(&mut fixture, replacement);
    assert!(!group(&mut fixture, second_id).unwrap().members.contains(&PlacementId(replacement_id)));
    unique(&mut fixture);
    session.destroy(); source.destroy(); fixture.exchange();
    assert!(fixture.desktop.running);
    super::super::configuration::application_input(&mut fixture, neighbour_id, &neighbour);
}
