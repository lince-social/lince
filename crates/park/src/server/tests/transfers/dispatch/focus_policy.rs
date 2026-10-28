use super::*;
use lince_interface::host::input::FocusPolicy;

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
}

pub(super) fn activate(fixture: &mut Fixture, caption: &'static str) {
    magnification::focus(fixture, caption);
    tap(fixture, 28);
    fixture.inspect(|_| ());
}

pub(super) fn open(fixture: &mut Fixture) {
    fixture.desktop.key(125, true, 1).unwrap();
    tap(fixture, 18);
    fixture.desktop.key(125, false, 2).unwrap();
    fixture.inspect(|_| ());
    activate(fixture, "Park");
    activate(fixture, "Edit focus settings");
}

pub(super) fn close(fixture: &mut Fixture) {
    activate(fixture, lince_interface::i18n::Token::CloseEditMode.text());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.modal && !frame.frame.input.keyboard_modal));
}

fn policy(fixture: &Fixture) -> FocusPolicy { fixture.desktop.current.as_ref().unwrap().frame.input.focus_policy }

struct Clients {
    windows: [ClientWindow; 2],
    _buffers: [wl_buffer::WlBuffer; 2],
    ids: [SurfaceId; 2],
}

fn two_outputs(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> (Fixture, Clients) {
    let mut harness = Harness::new();
    harness.server.configure_outputs(&[
        Monitor { id: OutputId(1), name: "Focus A".into(), origin: [0, 0], size: [500, 720] },
        Monitor { id: OutputId(2), name: "Focus B".into(), origin: [500, 0], size: [500, 720] },
    ]).unwrap();
    harness.sync();
    let first = harness.window();
    let first_buffer = paint(&mut harness, &first, [320, 240], [40, 180, 60, 255]);
    let first_id = harness.server.windows()[0].id;
    let second = harness.window();
    let second_buffer = paint(&mut harness, &second, [320, 240], [40, 70, 200, 255]);
    let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
    let mut fixture = Fixture::new(harness, runtime, directory);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.outputs.len() == 2 && geometry.items.iter().filter(|item| matches!(item.id, ItemId::Application(_))).count() == 2)));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let ids = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for workspace in ids { native::workspace_config::set_physics(world, root, workspace, false); }
    });
    let clients = Clients { windows: [first, second], _buffers: [first_buffer, second_buffer], ids: [first_id, second_id] };
    select(&mut fixture, &clients, 1);
    action(&mut fixture, DesktopAction::MoveToWorkspace(2));
    select(&mut fixture, &clients, 0);
    (fixture, clients)
}

fn action(fixture: &mut Fixture, action: DesktopAction) {
    fixture.desktop.send(Input::Action(action)).unwrap();
    fixture.inspect(|_| ());
    fixture.until(|fixture| fixture.desktop.pending_output.is_none());
}

fn select(fixture: &mut Fixture, clients: &Clients, index: usize) {
    fixture.desktop.send(Input::Activate(clients.ids[index])).unwrap();
    fixture.inspect(|_| ());
    fixture.until(|fixture| fixture.desktop.pending_output.is_none() && fixture.client.keyboard_focus.as_ref() == Some(&clients.windows[index].surface));
}

fn point(fixture: &Fixture, id: ItemId) -> [f64; 2] {
    let item = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == id).unwrap();
    let origin = item.mapping.surface.origin();
    let size = item.mapping.surface.size();
    let point = item.mapping.canvas_point([origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5]).unwrap();
    assert!(item.surface_point(point).is_some());
    point
}

fn type_to(fixture: &mut Fixture, surface: &wl_surface::WlSurface) {
    let count = fixture.client.key_recipients.len();
    tap(fixture, 45);
    fixture.until(|fixture| fixture.client.key_recipients.len() >= count + 2);
    assert_eq!(&fixture.client.key_recipients[count..], &[(Some(surface.id()), 45, true), (Some(surface.id()), 45, false)]);
}

fn field(fixture: &mut Fixture, entity: Entity) -> (String, bool) {
    fixture.inspect(move |canvas| {
        let text = canvas.world_mut().get::<bevy::text::EditableText>(entity).unwrap();
        (text.value().to_string(), text.is_composing())
    })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_client_focus_keeps_click_pointer_queue_popup_ime_and_lock_ownership_across_two_outputs() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, clients) = two_outputs(&runtime, directory.path());
    open(&mut fixture);
    activate(&mut fixture, "Click to focus: On");
    activate(&mut fixture, "Save focus settings");
    close(&mut fixture);
    assert_eq!(policy(&fixture), FocusPolicy { click: false, pointer_follow: false, warp: false });
    let second_point = point(&fixture, ItemId::Application(clients.ids[1]));
    fixture.desktop.move_pointer(second_point, 20).unwrap();
    fixture.tick();
    let buttons = fixture.client.button_recipients.len();
    fixture.desktop.button(272, true, 21).unwrap();
    fixture.desktop.button(272, false, 22).unwrap();
    fixture.until(|fixture| fixture.client.button_recipients.len() >= buttons + 2);
    assert_eq!(&fixture.client.button_recipients[buttons..], &[(Some(clients.windows[1].surface.id()), 272, true), (Some(clients.windows[1].surface.id()), 272, false)]);
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.output, Some(OutputId(1)));
    type_to(&mut fixture, &clients.windows[0].surface);
    open(&mut fixture);
    activate(&mut fixture, "Focus follows pointer: Off");
    activate(&mut fixture, "Save focus settings");
    close(&mut fixture);
    select(&mut fixture, &clients, 0);
    let first_point = point(&fixture, ItemId::Application(clients.ids[0]));
    fixture.desktop.move_pointer(first_point, 25).unwrap();
    fixture.inspect(|_| ());
    let (_, release) = fixture.pause();
    let mut queued = 0;
    let mut last_serial = 0;
    loop {
        match fixture.desktop.worker.input(Input::Pointer(first_point.map(|value| value as f32))) {
            Ok(serial) => { queued += 1; last_serial = serial; }
            Err(std::sync::mpsc::TrySendError::Full(_)) => break,
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => panic!("The pointer-focus worker disconnected"),
        }
    }
    assert_eq!(queued, 128);
    fixture.desktop.move_pointer(second_point, 26).unwrap();
    release.send(()).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= last_serial));
    fixture.inspect(|_| ());
    assert_eq!(fixture.client.keyboard_focus.as_ref(), Some(&clients.windows[0].surface));
    type_to(&mut fixture, &clients.windows[0].surface);
    fixture.desktop.move_pointer([second_point[0] + 1.0, second_point[1]], 27).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&clients.windows[1].surface)
        && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.output == Some(OutputId(2))));
    type_to(&mut fixture, &clients.windows[1].surface);
    for _ in 0..120 {
        fixture.inspect(|_| ());
        assert_eq!(fixture.client.keyboard_focus.as_ref(), Some(&clients.windows[1].surface));
    }
    action(&mut fixture, DesktopAction::Unfocus);
    fixture.desktop.move_pointer([second_point[0] + 2.0, second_point[1]], 28).unwrap();
    fixture.inspect(|_| ());
    assert!(fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().focused.is_none());
    assert!(fixture.client.keyboard_focus.is_none());
    select(&mut fixture, &clients, 1);
    select(&mut fixture, &clients, 0);
    fixture.desktop.move_pointer(first_point, 30).unwrap();
    fixture.inspect(|_| ());
    let (_, release) = fixture.pause();
    fixture.desktop.move_pointer(second_point, 31).unwrap();
    fixture.desktop.event(smithay::backend::winit::WinitEvent::Focus(false)).unwrap();
    fixture.desktop.event(smithay::backend::winit::WinitEvent::Focus(true)).unwrap();
    release.send(()).unwrap();
    fixture.inspect(|_| ());
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.output, Some(OutputId(1)));
    type_to(&mut fixture, &clients.windows[0].surface);
    select(&mut fixture, &clients, 1);
    let serial = fixture.client.key_serial;
    fixture.desktop.key(30, true, 40).unwrap();
    fixture.until(|fixture| fixture.client.key_serial != serial);
    let qh = fixture.queue.handle();
    let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
    positioner.set_size(120, 70);
    positioner.set_anchor_rect(30, 30, 1, 1);
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&clients.windows[1].xdg), &positioner, &qh, ());
    popup.grab(fixture.client.seat.as_ref().unwrap(), fixture.client.key_serial);
    surface.commit();
    fixture.exchange();
    let popup_buffer = privacy::buffer(&fixture, [120, 70], [180, 100, 40, 255]);
    surface.attach(Some(&popup_buffer), 0, 0);
    surface.commit();
    fixture.until(|fixture| fixture.desktop.server.popup_grab_active()
        && fixture.desktop.server.windows().iter().find(|window| window.id == clients.ids[1]).is_some_and(|window| !window.popups.is_empty()));
    fixture.desktop.key(30, false, 41).unwrap();
    fixture.tick();
    let popup_focus = fixture.client.keyboard_focus.clone().unwrap();
    assert!(popup_focus == clients.windows[1].surface || popup_focus == surface);
    fixture.desktop.move_pointer(first_point, 42).unwrap();
    fixture.inspect(|_| ());
    assert!(fixture.desktop.server.popup_grab_active());
    assert_eq!(fixture.client.keyboard_focus.as_ref(), Some(&popup_focus));
    type_to(&mut fixture, &popup_focus);
    popup.destroy();
    xdg.destroy();
    surface.destroy();
    positioner.destroy();
    fixture.until(|fixture| !fixture.desktop.server.popup_grab_active());
    select(&mut fixture, &clients, 0);
    select(&mut fixture, &clients, 1);
    fixture.client.activation.as_ref().unwrap().activate("invalid-focus-fixture".into(), &clients.windows[0].surface);
    for _ in 0..20 { fixture.tick(); }
    assert_eq!(fixture.client.keyboard_focus.as_ref(), Some(&clients.windows[1].surface));
    fixture.desktop.key(56, true, 50).unwrap();
    tap(&mut fixture, 15);
    fixture.inspect(|_| ());
    fixture.desktop.move_pointer(second_point, 51).unwrap();
    fixture.tick();
    fixture.desktop.key(56, false, 52).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&clients.windows[0].surface));
    type_to(&mut fixture, &clients.windows[0].surface);
    let (note, editor, workspace) = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
        let note = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText, "Focus IME field", bevy::math::DVec2::ZERO);
        let editor = world.get::<native::sand_store::StoredSand>(note).unwrap().content.unwrap();
        world.get_mut::<bevy::text::EditableText>(editor).unwrap().editor.set_text("");
        (note, editor, workspace)
    });
    let note = fixture.inspect(move |canvas| canvas.world_mut().get::<native::layout::LayoutBox>(note).unwrap().id);
    action(&mut fixture, DesktopAction::SelectPlacement { workspace, item: Some(note) });
    let (_, corners) = fixture.inspect(move |canvas| canvas.native_test_points(editor)).expect("The native field must have a rendered input projection");
    let note_point = std::array::from_fn(|axis| corners.iter().map(|corner| corner[axis]).sum::<f64>() / 4.0);
    assert!(matches!(fixture.desktop.target_at(note_point), crate::desktop::input::Target::Native), "The native field's projected point must reach its native input owner");
    fixture.desktop.move_pointer(note_point, 60).unwrap();
    fixture.desktop.button(272, true, 61).unwrap();
    fixture.desktop.button(272, false, 62).unwrap();
    let mut ime = ime::Ime::connect(&mut fixture);
    ime.until(&mut fixture, |fixture, ime| ime.active() && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.editor.is_some()));
    assert!(!fixture.desktop.current.as_ref().unwrap().frame.input.modal);
    assert!(!fixture.desktop.current.as_ref().unwrap().frame.input.text_composing);
    let owner = fixture.desktop.current.as_ref().unwrap().frame.input.editor.as_ref().unwrap().owner;
    let grab = ime.keyboard_grab();
    ime.until(&mut fixture, |fixture, _| fixture.desktop.server.input_method_grabbed());
    let keys = ime.keys().len();
    let app_keys = fixture.client.key_recipients.len();
    fixture.desktop.move_pointer(second_point, 63).unwrap();
    tap(&mut fixture, 45);
    ime.until(&mut fixture, |_, ime| ime.keys().len() >= keys + 2);
    assert_eq!(&ime.keys()[keys..], &[(45, true), (45, false)]);
    assert_eq!(fixture.client.key_recipients.len(), app_keys);
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.output, Some(OutputId(1)));
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.input.editor.as_ref().unwrap().owner, owner);
    assert_eq!(field(&mut fixture, editor), (String::new(), false));
    grab.release();
    ime.until(&mut fixture, |fixture, _| !fixture.desktop.server.input_method_grabbed());
    ime.method.set_preedit_string("界".into(), 0, 3);
    ime.method.commit(ime.serial());
    ime.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.text_composing));
    fixture.desktop.move_pointer([second_point[0] + 3.0, second_point[1]], 64).unwrap();
    for _ in 0..8 { ime.tick(&mut fixture); }
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.input.editor.as_ref().unwrap().owner, owner);
    assert_eq!(field(&mut fixture, editor), (String::new(), true));
    ime.method.commit_string("é".into());
    ime.method.set_preedit_string(String::new(), 0, 0);
    ime.method.commit(ime.serial());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ime.tick(&mut fixture);
        if field(&mut fixture, editor) == ("é".into(), false) { break; }
        assert!(Instant::now() < deadline, "The real focus-policy IME commit did not recover");
        std::thread::sleep(Duration::from_millis(2));
    }
    fixture.desktop.move_pointer([second_point[0] + 4.0, second_point[1]], 65).unwrap();
    ime.until(&mut fixture, |fixture, _| fixture.client.keyboard_focus.as_ref() == Some(&clients.windows[1].surface));
    ime.destroy(&mut fixture);
    type_to(&mut fixture, &clients.windows[1].surface);
    let keys = fixture.client.key_recipients.len();
    let buttons = fixture.client.button_recipients.len();
    let lock = fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ());
    fixture.until(|fixture| fixture.desktop.server.locked());
    fixture.desktop.move_pointer(first_point, 70).unwrap();
    fixture.desktop.button(272, true, 71).unwrap();
    fixture.desktop.button(272, false, 72).unwrap();
    tap(&mut fixture, 45);
    for _ in 0..8 { fixture.tick(); }
    assert_eq!(fixture.client.key_recipients.len(), keys);
    assert_eq!(fixture.client.button_recipients.len(), buttons);
    assert!(fixture.client.keyboard_focus.is_none());
    assert!(fixture.desktop.server.locked());
    assert!(lock.is_alive());
    assert_eq!(fixture.desktop.server.windows().iter().filter(|window| window.mapped).count(), 2);
    assert!(!fixture.inspect(|canvas| {
        let keys = canvas.world_mut().resource::<ButtonInput<KeyCode>>();
        keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::KeyX)
    }));
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn overlapping_real_clients_keep_keyboard_focus_for_120_presentations_while_an_area_moves_native_content_under_a_stationary_pointer() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, clients) = two_outputs(&runtime, directory.path());
    select(&mut fixture, &clients, 1);
    action(&mut fixture, DesktopAction::MoveToWorkspace(1));
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Free));
    let ids = clients.ids;
    let (moving, note) = fixture.inspect(move |canvas| {
        let members = [member(canvas, ids[0]), member(canvas, ids[1])];
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
        for (index, (entity, _, id)) in members.into_iter().enumerate() {
            world.get_mut::<native::workspace::Workspaces>(root).unwrap().entries.iter_mut().find(|space| space.id == workspace).unwrap().desktop
                .set_free_geometry(id, lince_interface::layout::desktop::Geometry { origin: [0.0; 2], size: [320.0, 240.0] }).unwrap();
            world.entity_mut(entity).insert(native::sand_placement::Pinned { anchor: [if index == 0 { 0.42 } else { 0.58 }, 0.5], scale: 1.0 });
        }
        let moving = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText, "Moving pointer-focus note", bevy::math::DVec2::new(20.0, -250.0));
        world.get_mut::<native::layout::LayoutBox>(moving).unwrap().rules = lince_interface::layout::Rules::fixed([100.0, 80.0]);
        world.get_mut::<native::canvas_item::CanvasItem>(moving).unwrap().size = Vec2::new(100.0, 80.0);
        let note = world.get::<native::layout::LayoutBox>(moving).unwrap().id;
        let mut area = native::area::InfluenceArea::new(native::area::AreaShape::Circle, bevy::math::DVec2::new(20.0, 250.0), bevy::math::DVec2::splat(1000.0));
        area.strength = 2500.0;
        native::area::spawn_area(world, root, workspace, area).unwrap();
        (moving, note)
    });
    open(&mut fixture);
    activate(&mut fixture, "Focus follows pointer: Off");
    activate(&mut fixture, "Save focus settings");
    close(&mut fixture);
    select(&mut fixture, &clients, 1);
    select(&mut fixture, &clients, 0);
    let (overlap, second_only) = {
        let geometry = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap();
        let a = geometry.items.iter().find(|item| item.id == ItemId::Application(clients.ids[0])).unwrap();
        let b = geometry.items.iter().find(|item| item.id == ItemId::Application(clients.ids[1])).unwrap();
        let low = std::array::from_fn::<_, 2, _>(|axis| a.mapping.canvas.origin()[axis].max(b.mapping.canvas.origin()[axis]));
        let high = std::array::from_fn::<_, 2, _>(|axis| (a.mapping.canvas.origin()[axis] + a.mapping.canvas.size()[axis]).min(b.mapping.canvas.origin()[axis] + b.mapping.canvas.size()[axis]));
        assert!((0..2).all(|axis| high[axis] - low[axis] > 20.0));
        let overlap = std::array::from_fn(|axis| (low[axis] + high[axis]) * 0.5);
        let second_only = [b.mapping.canvas.origin()[0] + b.mapping.canvas.size()[0] - 8.0, overlap[1]];
        assert!(a.surface_point(overlap).is_some() && b.surface_point(overlap).is_some());
        assert!(a.surface_point(second_only).is_none() && b.surface_point(second_only).is_some());
        (overlap, second_only)
    };
    fixture.desktop.move_pointer(overlap, 80).unwrap();
    fixture.until(|fixture| fixture.client.pointer_focus.as_ref() == Some(&clients.windows[0].surface));
    let pixels = appearance::pixels(&mut fixture, "focus-overlapping-clients");
    let index = ((overlap[1].round() as usize) * fixture.size[0] as usize + overlap[0].round() as usize) * 4;
    assert!(pixels[index + 1] > 140 && pixels[index] < 100 && pixels[index + 2] < 100, "The actual overlap pixel is not the selected green client");
    let initial_pointer = fixture.desktop.pointer;
    let initial_position = fixture.inspect(move |canvas| canvas.world_mut().get::<native::canvas_item::CanvasItem>(moving).unwrap().position);
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let root = world.get::<ChildOf>(moving).unwrap().parent();
        let workspace = world.get::<native::workspace::WorkspaceMember>(moving).unwrap().0;
        assert!(native::workspace_config::set_physics(world, root, workspace, true));
    });
    let mut positions = Vec::new();
    let mut crossed = false;
    for _ in 0..120 {
        fixture.inspect(|_| ());
        assert_eq!(fixture.desktop.pointer, initial_pointer);
        assert_eq!(fixture.client.keyboard_focus.as_ref(), Some(&clients.windows[0].surface));
        let item = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Native(note)).unwrap();
        positions.push(item.mapping.canvas.origin());
        crossed |= item.surface_point(overlap).is_some();
        std::thread::sleep(Duration::from_millis(16));
    }
    let final_position = fixture.inspect(move |canvas| canvas.world_mut().get::<native::canvas_item::CanvasItem>(moving).unwrap().position);
    assert!(initial_position.distance(final_position) > 1.0, "The real Area/physics workload never moved the native item");
    assert!(crossed, "The real moving native geometry never crossed beneath the stationary pointer");
    assert!(positions.iter().any(|position| position.iter().zip(positions[0]).any(|(actual, initial)| (*actual - initial).abs() > 1.0)), "The real published native geometry did not move with physics");
    type_to(&mut fixture, &clients.windows[0].surface);
    fixture.desktop.move_pointer(second_only, 81).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&clients.windows[1].surface));
    type_to(&mut fixture, &clients.windows[1].surface);
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_focus_settings_controls_save_cancel_and_reload_requested_warp_without_claiming_a_nested_cursor_capability() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let profile = directory.path().join(size[0].to_string());
        std::fs::create_dir_all(&profile).unwrap();
        let configuration = profile.join("interface.lingua");
        let mut fixture = magnification::create(&runtime, &profile, size, scale);
        let original = std::fs::read(&configuration).ok();
        assert_eq!(policy(&fixture), FocusPolicy::default());
        open(&mut fixture);
        activate(&mut fixture, "Click to focus: On");
        activate(&mut fixture, "Focus follows pointer: Off");
        activate(&mut fixture, "Warp cursor after keyboard focus: Off");
        assert_eq!(policy(&fixture), FocusPolicy::default());
        activate(&mut fixture, "Cancel focus changes");
        magnification::focus(&mut fixture, "Click to focus: On");
        magnification::focus(&mut fixture, "Focus follows pointer: Off");
        magnification::focus(&mut fixture, "Warp cursor after keyboard focus: Off");
        assert_eq!(std::fs::read(&configuration).ok(), original);
        activate(&mut fixture, "Click to focus: On");
        activate(&mut fixture, "Focus follows pointer: Off");
        activate(&mut fixture, "Warp cursor after keyboard focus: Off");
        activate(&mut fixture, "Save focus settings");
        let requested = FocusPolicy { click: false, pointer_follow: true, warp: true };
        assert_eq!(policy(&fixture), requested);
        let saved = std::fs::read(&configuration).unwrap();
        let supported = fixture.inspect(|canvas| canvas.world_mut().get_resource::<native::layout::desktop::focus::CursorWarpSupport>().is_some_and(|support| support.0));
        assert!(!supported);
        assert!(fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            world.query::<(&Text, &InheritedVisibility)>().iter(world).any(|(text, visible)| visible.get()
                && text.0 == "This window host cannot warp the system cursor. You can save the preference for a physical Park session.")
        }));
        activate(&mut fixture, "Click to focus: Off");
        assert_eq!(policy(&fixture), requested);
        activate(&mut fixture, "Cancel focus changes");
        close(&mut fixture);
        assert_eq!(std::fs::read(&configuration).unwrap(), saved);
        fixture.desktop.worker.stop().unwrap();
        drop(fixture);
        let mut reopened = magnification::create(&runtime, &profile, size, scale);
        assert_eq!(policy(&reopened), requested);
        open(&mut reopened);
        magnification::focus(&mut reopened, "Click to focus: Off");
        magnification::focus(&mut reopened, "Focus follows pointer: On");
        magnification::focus(&mut reopened, "Warp cursor after keyboard focus: On");
        activate(&mut reopened, "Warp cursor after keyboard focus: On");
        assert_eq!(policy(&reopened), requested);
        activate(&mut reopened, "Cancel focus changes");
        close(&mut reopened);
        assert_eq!(policy(&reopened), requested);
        assert_eq!(std::fs::read(&configuration).unwrap(), saved);
        assert!(reopened.desktop.current.as_ref().unwrap().frame.input.cursor_warp.is_none());
        reopened.desktop.worker.stop().unwrap();
    }
}
