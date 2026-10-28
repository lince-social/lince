use super::*;
use super::super::opening;
use lince_interface::host::magnification::Adjustment;

const BODY: &str = "A real magnified Wayland drop imports this body exactly once.";

fn action(fixture: &mut Fixture, action: DesktopAction) {
    fixture.desktop.send(Input::Action(action)).unwrap(); settle(fixture);
}

fn begin(fixture: &mut Fixture, window: &ClientWindow, surface: SurfaceId, home: (Entity, u64, PlacementId), mime: &str, payload: Vec<u8>)
    -> (wl_data_source::WlDataSource, wl_data_device::WlDataDevice)
{
    action(fixture, DesktopAction::SelectPlacement { workspace: home.1, item: Some(home.2.0) });
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface)
        && fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.keyboard_modal));
    let item = clients::geometry(fixture, surface);
    let local = std::array::from_fn(|axis| item.mapping.surface.origin()[axis] + item.mapping.surface.size()[axis] * 0.5);
    let point = item.mapping.canvas_point(local).unwrap();
    let displayed = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(item.output).unwrap().logical_to_displayed(point).unwrap();
    fixture.desktop.move_pointer(displayed, 10).unwrap(); fixture.tick();
    let buttons = fixture.client.button_recipients.len();
    fixture.desktop.button(272, true, 11).unwrap();
    fixture.until(|fixture| fixture.client.button_recipients.len() > buttons);
    assert_eq!(fixture.client.button_recipients.last(), Some(&(Some(window.surface.id()), 272, true)));
    let qh = fixture.queue.handle();
    let manager = fixture.client.data_manager.as_ref().unwrap();
    let device = manager.get_data_device(fixture.client.seat.as_ref().unwrap(), &qh, ());
    let source = manager.create_data_source(&qh, ());
    source.offer(mime.into()); source.set_actions(wl_data_device_manager::DndAction::Copy);
    fixture.client.drop_payload = payload;
    device.start_drag(Some(&source), &window.surface, None, fixture.client.pointer_serial);
    fixture.until(|fixture| fixture.desktop.server.dragging());
    (source, device)
}

fn visible_point(fixture: &mut Fixture, entity: Entity) -> ([f64; 2], OutputId) {
    let (output, points) = fixture.inspect(move |canvas| canvas.native_test_points(entity)).expect("The actual drop control needs a presented projection");
    let low = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min));
    let high: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max));
    let bounds = Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis])).unwrap();
    let mapping = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(output).unwrap();
    let visible = mapping.visible_rectangle(bounds).expect("The magnified control needs a visible drop target");
    let physical = std::array::from_fn(|axis| visible.origin()[axis] + visible.size()[axis] * 0.5);
    (mapping.physical_to_displayed(physical).unwrap(), output)
}

fn start(fixture: &mut Fixture, window: &ClientWindow, surface: SurfaceId, home: (Entity, u64, PlacementId), destination: u64, path: &std::path::Path)
    -> (wl_data_source::WlDataSource, wl_data_device::WlDataDevice)
{
    let (source, device) = begin(fixture, window, surface, home, "text/uri-list", format!("file://{}\r\n", path.display()).into_bytes());
    action(fixture, DesktopAction::ToggleOverview);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.keyboard_modal));
    let preview = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &Name)>().iter(world).find(|(_, name)| name.as_str() == format!("overview.workspace.{destination}")).unwrap().0
    });
    let (displayed, output) = visible_point(fixture, preview);
    fixture.desktop.move_pointer(displayed, 12).unwrap();
    fixture.until(|fixture| fixture.desktop.file_hover_observation().0.is_some());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.keyboard_modal
        && frame.frame.input.drop_target.as_ref().is_some_and(|target| target.output == output)));
    (source, device)
}

fn editor_offer(fixture: &mut Fixture, window: &ClientWindow, surface: SurfaceId, home: (Entity, u64, PlacementId), workspace: u64,
    field: Entity, placement: [u8; 16], text: &str) -> (wl_data_source::WlDataSource, wl_data_device::WlDataDevice)
{
    let (source, device) = begin(fixture, window, surface, home, "text/plain;charset=utf-8", text.as_bytes().to_vec());
    action(fixture, DesktopAction::SelectPlacement { workspace, item: Some(placement) });
    let (displayed, _) = visible_point(fixture, field);
    fixture.desktop.move_pointer(displayed, 20).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.drop_target.as_ref().is_some_and(|target|
        matches!(&target.destination, crate::native_input::transfer::Destination::Editor { context, .. } if context.owner == field.to_bits()))));
    (source, device)
}

fn imports(fixture: &mut Fixture) -> Vec<u64> {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let fields = world.query::<(Entity, &bevy::text::EditableText)>().iter(world)
            .filter(|(_, field)| field.value() == BODY).map(|(entity, _)| entity).collect::<Vec<_>>();
        fields.into_iter().map(|mut entity| {
            for _ in 0..native::layout::MAX_DEPTH {
                if let Some(workspace) = world.get::<native::workspace::WorkspaceMember>(entity) { return workspace.0; }
                entity = world.get::<ChildOf>(entity).unwrap().parent();
            }
            panic!("The actual imported field has no workspace owner")
        }).collect()
    })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_magnified_data_offer_retires_held_pipe_on_output_mapping_or_lock_loss_and_fresh_drop_imports_once_without_moving_its_source() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for locked in [false, true] {
        let profile = directory.path().join(if locked { "locked" } else { "mapping" }); std::fs::create_dir(&profile).unwrap();
        let settings = profile.join("park.json");
        std::fs::write(&settings, serde_json::to_vec(&serde_json::json!({"outputs":{(OUTPUT):{"scale":1.25,"magnification":2.0}}})).unwrap()).unwrap();
        let original = std::fs::read(&settings).unwrap();
        let path = profile.join("drop.txt"); std::fs::write(&path, BODY).unwrap();
        let mut fixture = create(&runtime, &profile, [1000, 720], 1.25);
        let (window, _buffer, surface) = opening::map(&mut fixture);
        let (home, destination) = fixture.inspect(move |canvas| {
            let home = member(canvas, surface);
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            canvas.create_workspace(root.to_bits()).unwrap();
            let destination = canvas.world_mut().get::<native::workspace::Workspaces>(root).unwrap().active;
            for workspace in [home.1, destination] { native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false); }
            (home, destination)
        });
        let (source, device) = start(&mut fixture, &window, surface, home, destination, &path);
        action(&mut fixture, DesktopAction::Magnify(Adjustment::Increase));
        assert!(saved(&fixture, 2.0));
        assert_eq!(std::fs::read(&settings).unwrap(), original);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let feedback = fixture.inspect(|canvas| {
                let world = canvas.world_mut();
                world.query::<&Text>().iter(world).any(|text| text.0.contains("before changing magnification") || text.0.contains("before changing display magnification"))
            });
            if feedback { break; }
            assert!(Instant::now() < deadline, "The busy magnification action gave no useful ordinary-interface feedback");
        }
        fixture.client.drop_hold = true;
        fixture.desktop.button(272, false, 13).unwrap();
        fixture.until(|fixture| !fixture.client.drop_held.is_empty());
        assert_eq!(fixture.client.drop_held.len(), 1);
        assert_eq!(fixture.client.drop_finished, 0);
        let remaining = fixture.desktop.server.native_timeout().expect("The real delayed offer must still own its reader deadline");
        assert!(remaining > Duration::from_millis(200), "The delayed-offer setup consumed its deadline before the intended retirement");
        let began = Instant::now();
        let mut locker = None;
        if locked {
            let qh = fixture.queue.handle();
            let lock = fixture.client.lock_manager.as_ref().unwrap().lock(&qh, ()); fixture.exchange();
            assert!(fixture.desktop.server.locked());
            let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
            let role = lock.get_lock_surface(&surface, fixture.client.output.as_ref().unwrap(), &qh, ()); fixture.exchange();
            let size = *fixture.client.lock_sizes.last().unwrap();
            let buffer = super::super::privacy::buffer(&fixture, size, [20, 20, 20, 255]);
            surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32); surface.commit();
            locker = Some((lock, role, surface, buffer));
        } else {
            fixture.desktop.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: OUTPUT.into(), origin: [0, 0], size: [1000, 720] }],
                &std::collections::BTreeMap::from([(OutputId(1), 2.0)])).unwrap();
            fixture.desktop.server.registered_output_catalog().unwrap(); fixture.desktop.invalidate_outputs().unwrap();
        }
        fixture.until(|fixture| fixture.client.drop_cancelled == 1 && fixture.desktop.server.native_timeout().is_none());
        assert!(began.elapsed() < remaining, "This fixture observed a reader timeout rather than the intended target retirement");
        for fd in std::mem::take(&mut fixture.client.drop_held) {
            let _ = std::fs::File::from(fd).write_all(&fixture.client.drop_payload);
        }
        fixture.client.drop_hold = false;
        if let Some((lock, role, surface, buffer)) = locker {
            fixture.until(|fixture| fixture.client.lock_replies.last() == Some(&true));
            lock.unlock_and_destroy(); fixture.exchange();
            fixture.until(|fixture| !fixture.desktop.server.locked());
            role.destroy(); surface.destroy(); buffer.destroy(); fixture.exchange();
        }
        assert!(imports(&mut fixture).is_empty(), "The retired pipe imported into a replacement output or protected scene");
        assert_eq!(fixture.client.drop_finished, 0);
        assert_eq!(fixture.client.drop_cancelled, 1);
        source.destroy(); device.release(); fixture.exchange();
        action(&mut fixture, DesktopAction::SelectPlacement { workspace: home.1, item: Some(home.2.0) });
        action(&mut fixture, DesktopAction::Magnify(Adjustment::Reset));
        fixture.until(|fixture| saved(fixture, 1.0));
        let (source, device) = start(&mut fixture, &window, surface, home, destination, &path);
        fixture.desktop.button(272, false, 14).unwrap();
        fixture.until(|fixture| fixture.client.drop_finished == 1);
        opening::activate(&mut fixture, "Use editable text Sand");
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let created = imports(&mut fixture);
            assert!(created.len() <= 1, "The genuine retry imported the same file more than once");
            if !created.is_empty() { assert_eq!(created, [destination]); break; }
            assert!(Instant::now() < deadline, "The normal file choice did not create the fresh dropped Sand");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(fixture.inspect(move |canvas| member(canvas, surface)), home);
        assert_eq!(fixture.client.drop_finished, 1); assert_eq!(fixture.client.drop_cancelled, 1);
        assert!(!fixture.desktop.server.dragging() && fixture.desktop.server.native_file_drag().is_none());
        assert!(fixture.desktop.running && fixture.desktop.keys.is_empty() && fixture.desktop.buttons.is_empty());
        source.destroy(); device.release(); fixture.exchange();
        if !locked {
            let (owner, field, placement) = fixture.inspect(|canvas| {
                let world = canvas.world_mut();
                let field = world.query::<(Entity, &bevy::text::EditableText)>().iter(world).find(|(_, field)| field.value() == BODY).unwrap().0;
                let mut owner = field;
                while world.get::<native::workspace::WorkspaceMember>(owner).is_none() { owner = world.get::<ChildOf>(owner).unwrap().parent(); }
                (owner, field, world.get::<native::layout::LayoutBox>(owner).unwrap().id)
            });
            let (source, device) = editor_offer(&mut fixture, &window, surface, home, destination, field, placement, " stale");
            fixture.client.drop_hold = true; fixture.desktop.button(272, false, 21).unwrap();
            fixture.until(|fixture| !fixture.client.drop_held.is_empty());
            let remaining = fixture.desktop.server.native_timeout().unwrap(); let began = Instant::now();
            let (replacement, replacement_id) = fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
                assert!(world.despawn(owner));
                let owner = native::sand_store::spawn_sand(world, root, destination, native::sand_store::SandKind::EditableText, "Pipe replacement editor", bevy::math::DVec2::ZERO);
                let field = world.get::<native::sand_store::StoredSand>(owner).unwrap().content.unwrap();
                world.get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text("Replacement retained.");
                if world.get::<native::layout::LayoutBox>(owner).is_none() {
                    let size = world.get::<native::canvas::CanvasItem>(owner).unwrap().size;
                    world.entity_mut(owner).insert(native::layout::LayoutBox::new(size));
                }
                (field, world.get::<native::layout::LayoutBox>(owner).unwrap().id)
            });
            action(&mut fixture, DesktopAction::SelectPlacement { workspace: destination, item: Some(replacement_id) });
            fixture.until(|fixture| fixture.client.drop_cancelled == 2 && fixture.desktop.server.native_timeout().is_none());
            assert!(began.elapsed() < remaining, "Editor replacement was observed only after the pipe timeout");
            for fd in std::mem::take(&mut fixture.client.drop_held) { let _ = std::fs::File::from(fd).write_all(&fixture.client.drop_payload); }
            fixture.client.drop_hold = false; settle(&mut fixture);
            assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(replacement).unwrap().value().to_string()), "Replacement retained.");
            source.destroy(); device.release(); fixture.exchange();
            let (source, device) = editor_offer(&mut fixture, &window, surface, home, destination, replacement, replacement_id, " fresh");
            fixture.desktop.button(272, false, 22).unwrap(); fixture.until(|fixture| fixture.client.drop_finished == 2);
            let value = fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(replacement).unwrap().value().to_string());
            assert_eq!(value.matches(" fresh").count(), 1); assert_eq!(value.replace(" fresh", ""), "Replacement retained.");
            assert_eq!(fixture.client.drop_cancelled, 2); assert_eq!(fixture.inspect(move |canvas| member(canvas, surface)), home);
            source.destroy(); device.release(); fixture.exchange();
        }
        fixture.desktop.worker.stop().unwrap();
    }
}
