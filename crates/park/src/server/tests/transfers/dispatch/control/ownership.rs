use super::*;
use lince_interface::host::DesktopAction;

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> (Fixture, [ClientWindow; 2], [wl_buffer::WlBuffer; 2], [SurfaceId; 2]) {
    let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::WindowList, Capability::DesktopControl, Capability::WindowControl]));
    let first = harness.window();
    let first_buffer = paint(&mut harness, &first, [320, 240], [200, 70, 20, 255]);
    let first_id = harness.server.windows()[0].id;
    let second = harness.window();
    let second_buffer = paint(&mut harness, &second, [320, 240], [20, 70, 200, 255]);
    let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
    let mut fixture = Fixture::new(harness, runtime, directory);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| [first_id, second_id].iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
        native::workspace_config::set_physics(world, root, workspace, false);
    });
    select(&mut fixture, first_id);
    (fixture, [first, second], [first_buffer, second_buffer], [first_id, second_id])
}

fn select(fixture: &mut Fixture, id: SurfaceId) {
    fixture.desktop.send(Input::Activate(id)).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref().is_some_and(|surface| fixture.desktop.server.windows().iter().any(|window| window.id == id && window.surface.id().protocol_id() == surface.id().protocol_id()))
        && fixture.desktop.current.as_ref().is_some_and(|frame| matches!(frame.frame.input.control.context.as_ref().and_then(|context| context.item), Some(lince_interface::host::control::Item::Application { surface, .. }) if surface == id.0)));
}

fn mode(fixture: &mut Fixture) -> lince_interface::host::WorkspaceLayout {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let spaces = world.get::<native::workspace::Workspaces>(root).unwrap();
        spaces.entries.iter().find(|space| space.id == spaces.active).unwrap().desktop.mode()
    })
}

fn refused(fixture: &mut Fixture, id: u32) {
    fixture.until(|fixture| fixture.client.control.replies.contains_key(&id));
    let Response::Failed { failure } = &fixture.client.control.replies[&id] else { panic!("An owned or retired request cannot execute"); };
    assert!(matches!(failure.code, Error::Unavailable | Error::Cancelled | Error::StaleTarget));
}

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 40).unwrap();
    fixture.desktop.key(code, false, 41).unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_automation_refuses_real_popup_modal_preedit_and_ime_grab_owners_then_fresh_commands_and_client_input_recover() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, windows, _buffers, ids) = create(&runtime, directory.path());
    let initial = mode(&mut fixture);
    let serial = fixture.client.key_serial;
    fixture.desktop.key(30, true, 10).unwrap();
    fixture.until(|fixture| fixture.client.key_serial != serial);
    let qh = fixture.queue.handle();
    let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
    positioner.set_size(120, 70);
    positioner.set_anchor_rect(30, 30, 1, 1);
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&windows[0].xdg), &positioner, &qh, ());
    let (_, release) = fixture.pause();
    invoke(&mut fixture, 1, "layout.toggle-tiling");
    fixture.exchange();
    fixture.desktop.dispatch().unwrap();
    popup.grab(fixture.client.seat.as_ref().unwrap(), fixture.client.key_serial);
    surface.commit();
    fixture.exchange();
    let popup_buffer = privacy::buffer(&fixture, [120, 70], [180, 100, 40, 255]);
    surface.attach(Some(&popup_buffer), 0, 0);
    surface.commit();
    fixture.exchange();
    assert!(fixture.desktop.server.popup_grab_active());
    release.send(()).unwrap();
    refused(&mut fixture, 1);
    assert_eq!(mode(&mut fixture), initial);
    fixture.desktop.key(30, false, 11).unwrap();
    invoke(&mut fixture, 2, "layout.toggle-tiling");
    refused(&mut fixture, 2);
    assert!(fixture.desktop.server.popup_grab_active());
    popup.destroy();
    xdg.destroy();
    surface.destroy();
    positioner.destroy();
    fixture.until(|fixture| !fixture.desktop.server.popup_grab_active());
    invoke(&mut fixture, 3, "layout.toggle-tiling");
    fixture.until(|fixture| matches!(fixture.client.control.replies.get(&3), Some(Response::Complete { outcome: Disposition::Applied })));
    assert_ne!(mode(&mut fixture), initial);
    fixture.desktop.key(125, true, 20).unwrap();
    tap(&mut fixture, 25);
    fixture.desktop.key(125, false, 21).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.modal || frame.frame.input.keyboard_modal));
    invoke(&mut fixture, 4, "layout.toggle-tiling");
    refused(&mut fixture, 4);
    tap(&mut fixture, 1);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.modal && !frame.frame.input.keyboard_modal));
    let (note, editor, workspace) = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
        let note = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText, "Automation IME owner", bevy::math::DVec2::ZERO);
        let editor = world.get::<native::sand_store::StoredSand>(note).unwrap().content.unwrap();
        world.get_mut::<bevy::text::EditableText>(editor).unwrap().editor.set_text("");
        (world.get::<native::layout::LayoutBox>(note).unwrap().id, editor, workspace)
    });
    fixture.desktop.send(Input::Action(DesktopAction::SelectPlacement { workspace, item: Some(note) })).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Native(note)))));
    let item = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Native(note)).unwrap();
    let origin = item.mapping.surface.origin();
    let size = item.mapping.surface.size();
    let point = item.mapping.canvas_point([origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5]).unwrap();
    assert!(item.surface_point(point).is_some());
    fixture.desktop.move_pointer(point, 30).unwrap();
    fixture.desktop.button(272, true, 31).unwrap();
    fixture.desktop.button(272, false, 32).unwrap();
    let mut ime = ime::Ime::connect(&mut fixture);
    ime.until(&mut fixture, |fixture, ime| ime.active() && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.editor.is_some()));
    let (_, release) = fixture.pause();
    ime.method.set_preedit_string("界".into(), 0, 3);
    ime.method.commit(ime.serial());
    ime.exchange(&mut fixture);
    invoke(&mut fixture, 5, "layout.toggle-tiling");
    fixture.exchange();
    fixture.desktop.dispatch().unwrap();
    release.send(()).unwrap();
    refused(&mut fixture, 5);
    ime.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.text_composing));
    assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(editor).unwrap().is_composing()));
    let grab = ime.keyboard_grab();
    ime.until(&mut fixture, |fixture, _| fixture.desktop.server.input_method_grabbed());
    invoke(&mut fixture, 6, "layout.toggle-tiling");
    refused(&mut fixture, 6);
    let before = ime.keys().len();
    let application_keys = fixture.client.key_recipients.len();
    tap(&mut fixture, 45);
    ime.until(&mut fixture, |_, ime| ime.keys().len() >= before + 2);
    assert_eq!(&ime.keys()[before..], &[(45, true), (45, false)]);
    assert_eq!(fixture.client.key_recipients.len(), application_keys);
    grab.release();
    ime.until(&mut fixture, |fixture, _| !fixture.desktop.server.input_method_grabbed());
    ime.method.commit_string("é".into());
    ime.method.set_preedit_string(String::new(), 0, 0);
    ime.method.commit(ime.serial());
    ime.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.text_composing));
    assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(editor).unwrap().value().to_string()), "é");
    invoke(&mut fixture, 7, "layout.toggle-tiling");
    fixture.until(|fixture| matches!(fixture.client.control.replies.get(&7), Some(Response::Complete { outcome: Disposition::Applied })));
    ime.destroy(&mut fixture);
    select(&mut fixture, ids[1]);
    let keys = fixture.client.key_recipients.len();
    tap(&mut fixture, 30);
    fixture.until(|fixture| fixture.client.key_recipients.len() >= keys + 2);
    assert_eq!(&fixture.client.key_recipients[keys..], &[(Some(windows[1].surface.id()), 30, true), (Some(windows[1].surface.id()), 30, false)]);
    assert_eq!(fixture.desktop.server.control_status().pending, 0);
    assert!(fixture.desktop.running);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn destroying_a_queued_commands_window_and_reusing_its_actual_wayland_object_number_cannot_move_or_close_the_replacement() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, windows, _buffers, ids) = create(&runtime, directory.path());
    let number = windows[0].surface.id().protocol_id();
    let (_, release) = fixture.pause();
    invoke(&mut fixture, 1, "item.move-right");
    invoke(&mut fixture, 2, "item.close");
    fixture.exchange();
    fixture.desktop.dispatch().unwrap();
    windows[0].role.destroy();
    windows[0].xdg.destroy();
    windows[0].surface.destroy();
    fixture.exchange();
    let qh = fixture.queue.handle();
    let mut retained = Vec::new();
    let surface = (0..64).find_map(|_| {
        let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        if surface.id().protocol_id() == number { Some(surface) } else { retained.push(surface); None }
    }).expect("The fixture must reuse the actual wire object number rather than inventing an equivalent identity");
    let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let role = xdg.get_toplevel(&qh, ());
    role.set_title("Replacement with a reused object number".into());
    surface.commit();
    fixture.exchange();
    let replacement_buffer = privacy::buffer(&fixture, [320, 240], [30, 170, 120, 255]);
    surface.attach(Some(&replacement_buffer), 0, 0);
    surface.commit();
    fixture.exchange();
    let replacement = fixture.desktop.server.windows().into_iter().find(|window| window.surface.id().protocol_id() == number && window.mapped).unwrap().id;
    assert_ne!(replacement, ids[0]);
    release.send(()).unwrap();
    refused(&mut fixture, 1);
    refused(&mut fixture, 2);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(replacement)) && geometry.items.iter().all(|item| item.id != ItemId::Application(ids[0])))));
    assert!(!fixture.client.closed);
    assert!(fixture.desktop.server.windows().iter().any(|window| window.id == ids[1] && window.mapped));
    select(&mut fixture, replacement);
    invoke(&mut fixture, 3, "item.move-right");
    fixture.until(|fixture| matches!(fixture.client.control.replies.get(&3), Some(Response::Complete { outcome: Disposition::Applied })));
    let keys = fixture.client.key_recipients.len();
    tap(&mut fixture, 30);
    fixture.until(|fixture| fixture.client.key_recipients.len() >= keys + 2);
    assert_eq!(&fixture.client.key_recipients[keys..], &[(Some(surface.id()), 30, true), (Some(surface.id()), 30, false)]);
    assert_eq!(fixture.desktop.server.control_status().pending, 0);
    assert!(fixture.desktop.running);
}
