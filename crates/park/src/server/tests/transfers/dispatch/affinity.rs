use super::*;

fn head(id: u64, second: bool) -> Monitor {
    Monitor { id: OutputId(id), name: if second { "Affinity-B" } else { "Affinity-A" }.into(), origin: [if second { 500 } else { 0 }, 0], size: [500, 720] }
}

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, monitors: &[Monitor], mut harness: Harness) -> Fixture {
    harness.server.configure_outputs(monitors).unwrap();
    harness.server.registered_output_catalog().unwrap();
    harness.sync();
    let mut fixture = Fixture::new(harness, runtime, directory);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.outputs.len() == monitors.len())));
    fixture
}

fn workspace(fixture: &Fixture, output: u64) -> u64 {
    fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs.iter().find(|head| head.id == OutputId(output)).unwrap().workspace
}

fn configure(fixture: &mut Fixture, monitors: &[Monitor]) {
    fixture.desktop.server.configure_outputs(monitors).unwrap();
    fixture.desktop.server.registered_output_catalog().unwrap();
    fixture.desktop.invalidate_outputs().unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.outputs.len() == monitors.len() && monitors.iter().all(|monitor| geometry.outputs.iter().any(|head| head.id == monitor.id)))));
}

fn action(fixture: &mut Fixture, action: DesktopAction) {
    fixture.desktop.send(Input::Action(action)).unwrap();
    fixture.inspect(|_| ());
    fixture.until(|fixture| fixture.desktop.pending_output.is_none());
}

fn activate(fixture: &mut Fixture, id: SurfaceId, surface: &wl_surface::WlSurface) {
    fixture.desktop.send(Input::Activate(id)).unwrap();
    fixture.inspect(|_| ());
    fixture.until(|fixture| fixture.desktop.pending_output.is_none() && fixture.client.keyboard_focus.as_ref() == Some(surface));
}

fn type_key(fixture: &mut Fixture, surface: &wl_surface::WlSurface) {
    let keys = fixture.client.key_recipients.len();
    fixture.desktop.key(45, true, 10).unwrap();
    fixture.desktop.key(45, false, 11).unwrap();
    fixture.until(|fixture| fixture.client.key_recipients.len() >= keys + 2);
    assert_eq!(&fixture.client.key_recipients[keys..], &[(Some(surface.id()), 45, true), (Some(surface.id()), 45, false)]);
}

fn view(fixture: &mut Fixture) -> (u64, bevy::math::DVec2, f64) {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let (root, spaces) = world.query::<(Entity, &native::workspace::Workspaces)>().single(world).unwrap();
        let view = world.get::<native::canvas::CanvasView>(root).unwrap();
        (spaces.active, view.center, view.zoom)
    })
}

fn follows(fixture: &mut Fixture) -> bool {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        native::layout::desktop::focus::following(world, root)
    })
}

fn wait_follow(fixture: &mut Fixture) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !follows(fixture) {
        if Instant::now() >= deadline {
            let state = fixture.inspect(|canvas| {
                let world = canvas.world_mut();
                let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
                let input = world.resource::<bevy::input_focus::InputFocus>().get();
                let input_details = input.map(|entity| serde_json::json!({"entity":format!("{entity:?}"),
                    "name":world.get::<Name>(entity).map(Name::as_str),
                    "label":world.get::<bevy::a11y::AccessibilityNode>(entity).and_then(|node| node.label()),
                    "editable":world.get::<bevy::text::EditableText>(entity).is_some(),
                    "window":world.get::<bevy::window::Window>(entity).is_some(),
                    "primary_window":world.get::<bevy::window::PrimaryWindow>(entity).is_some(),
                    "root":entity == root}));
                let items = world.query::<(Entity, &native::layout::LayoutBox, &native::workspace::WorkspaceMember, &native::canvas::CanvasItem)>()
                    .iter(world).map(|(entity, layout, member, item)| serde_json::json!({"entity":format!("{entity:?}"),"id":layout.id,"workspace":member.0,"position":item.position.to_array(),"size":item.size.to_array()})).collect::<Vec<_>>();
                let spaces = world.get::<native::workspace::Workspaces>(root).unwrap();
                serde_json::json!({"root":format!("{root:?}"),"active":spaces.active,"workspaces":spaces.entries,"input":input_details,"items":items})
            });
            panic!("Saved native follow intent did not restore through real startup/loading: {state}");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn two_real_output_affinities_reconnect_without_stealing_input_and_cold_native_focus_and_unfocus_restore_by_stable_identity() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::new();
    let first = harness.window();
    let _first_buffer = paint(&mut harness, &first, [320, 240], [40, 190, 50, 255]);
    let first_id = harness.server.windows()[0].id;
    let second = harness.window();
    let _second_buffer = paint(&mut harness, &second, [320, 240], [40, 70, 210, 255]);
    let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
    let mut fixture = create(&runtime, directory.path(), &[head(1, false), head(2, true)], harness);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.len() == 2));
    let a = workspace(&fixture, 1);
    let b = workspace(&fixture, 2);
    assert_ne!(a, b);
    activate(&mut fixture, second_id, &second.surface);
    action(&mut fixture, DesktopAction::MoveToWorkspace(2));
    let (first_member, second_member, note) = fixture.inspect(move |canvas| {
        let first_member = member(canvas, first_id);
        let second_member = member(canvas, second_id);
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let ids = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for workspace in ids { native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false); }
        let note = native::sand_store::spawn_sand(canvas.world_mut(), root, b, native::sand_store::SandKind::EditableText, "Durable affinity note", bevy::math::DVec2::ZERO);
        let field = canvas.world_mut().get::<native::sand_store::StoredSand>(note).unwrap().content.unwrap();
        canvas.world_mut().get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text("Durable affinity content");
        (first_member, second_member, note)
    });
    let note = fixture.inspect(move |canvas| canvas.world_mut().get::<native::layout::LayoutBox>(note).unwrap().id);
    assert_eq!(first_member.1, a);
    assert_eq!(second_member.1, b);
    activate(&mut fixture, first_id, &first.surface);
    type_key(&mut fixture, &first.surface);
    let before = view(&mut fixture);
    configure(&mut fixture, &[head(1, false)]);
    assert_eq!(view(&mut fixture), before);
    assert_eq!(workspace(&fixture, 1), a);
    type_key(&mut fixture, &first.surface);
    let preferences: Vec<lince_interface::layout::outputs::Preference> = serde_json::from_slice(&std::fs::read(directory.path().join("park-outputs.json")).unwrap()).unwrap();
    assert_eq!(preferences.iter().find(|preference| preference.workspace == b).unwrap().output, "Affinity-B");
    configure(&mut fixture, &[head(2, true), head(1, false)]);
    assert_eq!(workspace(&fixture, 1), a);
    assert_eq!(workspace(&fixture, 2), b);
    assert_eq!(view(&mut fixture), before);
    type_key(&mut fixture, &first.surface);
    configure(&mut fixture, &[head(1, false)]);
    action(&mut fixture, DesktopAction::Workspace(2));
    activate(&mut fixture, second_id, &second.surface);
    assert_eq!(workspace(&fixture, 1), b);
    type_key(&mut fixture, &second.surface);
    let occupied = view(&mut fixture);
    configure(&mut fixture, &[head(2, true), head(1, false)]);
    assert_eq!(workspace(&fixture, 1), b);
    assert_ne!(workspace(&fixture, 2), b);
    assert_eq!(view(&mut fixture), occupied);
    type_key(&mut fixture, &second.surface);
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        canvas.create_workspace(root.to_bits()).unwrap();
    });
    action(&mut fixture, DesktopAction::Workspace(1));
    activate(&mut fixture, first_id, &first.surface);
    assert_eq!(workspace(&fixture, 1), a);
    assert_eq!(workspace(&fixture, 2), b);
    type_key(&mut fixture, &first.surface);
    activate(&mut fixture, second_id, &second.surface);
    action(&mut fixture, DesktopAction::MoveWorkspaceToOutput(Direction::Left));
    assert_eq!(workspace(&fixture, 1), b);
    type_key(&mut fixture, &second.surface);
    configure(&mut fixture, &[head(1, false)]);
    configure(&mut fixture, &[head(2, true), head(1, false)]);
    assert_eq!(workspace(&fixture, 1), b);
    assert_ne!(workspace(&fixture, 2), b);
    type_key(&mut fixture, &second.surface);
    let preferences: Vec<lince_interface::layout::outputs::Preference> = serde_json::from_slice(&std::fs::read(directory.path().join("park-outputs.json")).unwrap()).unwrap();
    assert_eq!(preferences.iter().find(|preference| preference.workspace == b).unwrap().output, "Affinity-A");
    fixture.desktop.server.disconnect_outputs();
    fixture.desktop.server.output_catalog(Vec::new()).unwrap();
    fixture.desktop.invalidate_outputs().unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.dmabuf().is_none()));
    assert_eq!(fixture.desktop.server.windows().iter().filter(|window| window.mapped).count(), 2);
    assert_eq!(fixture.inspect(move |canvas| (member(canvas, first_id), member(canvas, second_id))), (first_member, second_member));
    configure(&mut fixture, &[head(2, true), head(1, false)]);
    assert_eq!(workspace(&fixture, 1), b);
    activate(&mut fixture, second_id, &second.surface);
    type_key(&mut fixture, &second.surface);
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Free));
    action(&mut fixture, DesktopAction::SelectPlacement { workspace: b, item: Some(note) });
    if !follows(&mut fixture) { action(&mut fixture, DesktopAction::ToggleFocus); }
    wait_follow(&mut fixture);
    fixture.desktop.worker.stop().unwrap();
    drop(fixture);
    let mut reopened = create(&runtime, directory.path(), &[head(9, true), head(7, false)], Harness::new());
    reopened.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.items.iter().any(|item| item.id == ItemId::Native(note)))));
    assert_eq!(workspace(&reopened, 7), b);
    assert_ne!(workspace(&reopened, 9), b);
    assert_eq!(view(&mut reopened).0, b);
    assert!(reopened.desktop.server.windows().is_empty());
    wait_follow(&mut reopened);
    let retained = reopened.inspect(move |canvas| {
        let world = canvas.world_mut();
        let note = world.query::<(Entity, &native::layout::LayoutBox)>().iter(world).find(|(_, layout)| layout.id == note).unwrap().0;
        let field = world.get::<native::sand_store::StoredSand>(note).unwrap().content.unwrap();
        world.get::<bevy::text::EditableText>(field).unwrap().value().to_string()
    });
    assert_eq!(retained, "Durable affinity content");
    action(&mut reopened, DesktopAction::Unfocus);
    assert!(!follows(&mut reopened));
    reopened.desktop.worker.stop().unwrap();
    drop(reopened);
    let mut unfocused = create(&runtime, directory.path(), &[head(17, true), head(13, false)], Harness::new());
    unfocused.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.items.iter().any(|item| item.id == ItemId::Native(note)))));
    assert_eq!(workspace(&unfocused, 13), b);
    assert!(!follows(&mut unfocused));
    assert!(unfocused.desktop.server.windows().is_empty());
    action(&mut unfocused, DesktopAction::ToggleFocus);
    wait_follow(&mut unfocused);
    unfocused.desktop.worker.stop().unwrap();
}
