use super::*;
use super::opening::{activate, click_entity, close, persisted, provider, ready, released, write};
use lince_interface::{layout::desktop::PlacementId, native::shortcuts::{Settings, Shortcut}};

pub(super) fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], scale: f64,
    engine: std::sync::Arc<engine::Engine>, applications: bool) -> (Fixture, Vec<ClientWindow>, Vec<wl_buffer::WlBuffer>, Vec<SurfaceId>)
{
    create_authorized(runtime, directory, size, scale, engine, applications, Vec::new())
}

pub(super) fn create_authorized(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], scale: f64,
    engine: std::sync::Arc<engine::Engine>, applications: bool, capabilities: Vec<crate::launch::Capability>) -> (Fixture, Vec<ClientWindow>, Vec<wl_buffer::WlBuffer>, Vec<SurfaceId>)
{
    let mut server = Server::new(size.map(|value| value as i32)).unwrap();
    server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: "Windowed-Fullscreen".into(), origin: [0, 0], size: size.map(|value| value as i32) }],
        &std::collections::BTreeMap::from([(OutputId(1), scale)])).unwrap();
    let mut harness = if capabilities.is_empty() { Harness::connect(server) } else { Harness::connect_authorized(server, Some(capabilities)) };
    let mut windows = Vec::new();
    let mut buffers = Vec::new();
    if applications {
        for color in [[0, 255, 0, 255], [255, 0, 0, 255]] {
            let window = harness.window();
            buffers.push(paint(&mut harness, &window, [320, 240], color));
            windows.push(window);
        }
    }
    let ids = harness.server.windows().into_iter().map(|window| window.id).collect::<Vec<_>>();
    let mut fixture = Fixture::with_cell(harness, runtime, directory, size, provider(engine, directory));
    ready(&mut fixture);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        ids.iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let ids = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for id in ids { native::workspace_config::set_physics(world, root, id, false); }
    });
    (fixture, windows, buffers, ids)
}

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
}

fn chord(fixture: &mut Fixture) {
    chord_key(fixture, 33);
}

pub(super) fn chord_key(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(125, true, 12).unwrap();
    fixture.desktop.key(56, true, 13).unwrap();
    tap(fixture, code);
    fixture.desktop.key(56, false, 14).unwrap();
    fixture.desktop.key(125, false, 15).unwrap();
}

pub(super) fn select(fixture: &mut Fixture, surface: SurfaceId) {
    fixture.desktop.send(Input::Activate(surface)).unwrap();
    fixture.until(|fixture| fixture.desktop.pending_output.is_none() && fixture.desktop.current.as_ref().is_some_and(|frame|
        matches!(frame.frame.input.control.context.as_ref().and_then(|context| context.item),
            Some(lince_interface::host::control::Item::Application { surface: current, .. }) if current == surface.0)));
}

pub(super) fn inspect(fixture: &mut Fixture, surface: SurfaceId) -> (Entity, [u8; 16], String) {
    select(fixture, surface);
    fixture.desktop.send(Input::Action(DesktopAction::InspectWindowRecord)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let metadata = fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let placement = world.query::<(&native::layout::desktop::ApplicationWindow, &native::layout::LayoutBox)>().iter(world)
                .find_map(|(window, layout)| (window.surface == surface).then_some(layout.id))?;
            world.query::<(Entity, &native::full_record::window::Metadata)>().iter(world)
                .find_map(|(entity, metadata)| (metadata.live && metadata.placement == placement).then(|| (entity, placement, metadata.uid.clone())))
        });
        if let Some(metadata) = metadata { return metadata; }
        assert!(Instant::now() < deadline, "The actual application Record did not open its captured Properties");
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn button(fixture: &mut Fixture, inspector: Entity, caption: &str) -> Entity {
    let caption = caption.to_owned();
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world).find_map(|(entity, node)| {
            if node.label() != Some(caption.as_str()) || world.get::<native::actions::ActionButton>(entity).is_none() { return None; }
            let mut current = Some(entity);
            for _ in 0..native::layout::MAX_DEPTH {
                let candidate = current?;
                if candidate == inspector { return Some(entity); }
                current = world.get::<ChildOf>(candidate).map(ChildOf::parent);
            }
            None
        }).expect("The original inspector must expose its own Properties control")
    })
}

fn control(fixture: &mut Fixture, inspector: Entity, entering: bool, pointer: bool) -> Entity {
    let caption = if entering { "Enter windowed fullscreen" } else { "Leave windowed fullscreen" };
    let button = button(fixture, inspector, caption);
    assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::ui::InteractionDisabled>(button).is_none()));
    if pointer { click_entity(fixture, button, caption); }
    else { magnification::focus_entity(fixture, button, caption); tap(fixture, 28); fixture.inspect(|_| ()); }
    button
}

fn mode(fixture: &mut Fixture, surface: SurfaceId, window: &ClientWindow, bounded: bool) {
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.windows.iter().any(|configuration|
        configuration.surface == surface && configuration.state.windowed_fullscreen == bounded && configuration.state.fullscreen == bounded))
        && fixture.client.window_configurations.get(&window.role.id()).is_some_and(|(_, states)| states.contains(&(xdg_toplevel::State::Fullscreen as u32)) == bounded));
    assert!(!fixture.desktop.current.as_ref().unwrap().frame.fullscreen);
}

fn focused(fixture: &mut Fixture) -> Option<PlacementId> {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let spaces = world.get::<native::workspace::Workspaces>(root).unwrap();
        spaces.entries.iter().find(|workspace| workspace.id == spaces.active).unwrap().desktop.focused()
    })
}

fn capture_binding(fixture: &mut Fixture, save: bool) {
    capture_shortcut(fixture, "item.windowed-fullscreen", "windowed", 33, save);
}

pub(super) fn capture_shortcut(fixture: &mut Fixture, command: &str, query: &str, code: u32, save: bool) {
    fixture.desktop.key(125, true, 20).unwrap();
    tap(fixture, 18);
    fixture.desktop.key(125, false, 21).unwrap();
    fixture.inspect(|_| ());
    activate(fixture, "Park");
    activate(fixture, "Edit all shortcuts");
    write(fixture, "Search command names, scopes or key combinations", query);
    let command = command.to_owned();
    let capture = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let row = world.query::<(Entity, &Name)>().iter(world).find(|(_, name)| name.as_str() == command.as_str()).unwrap().0;
        world.query::<(Entity, &Name)>().iter(world).find_map(|(entity, name)| {
            if name.as_str() != "shortcut.capture" { return None; }
            let mut current = Some(entity);
            for _ in 0..native::layout::MAX_DEPTH {
                let parent = current?;
                if parent == row { return Some(entity); }
                current = world.get::<ChildOf>(parent).map(ChildOf::parent);
            }
            None
        }).unwrap()
    });
    click_entity(fixture, capture, "Capture command binding");
    chord_key(fixture, code);
    fixture.inspect(|_| ());
    if save {
        activate(fixture, "Validate combination");
        activate(fixture, "Save all changes");
    } else {
        activate(fixture, "Cancel capture");
        tap(fixture, 1);
        fixture.inspect(|_| ());
    }
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.keyboard_modal));
    close(fixture);
}

fn binding(fixture: &mut Fixture) -> String {
    fixture.inspect(|canvas| canvas.world_mut().resource::<Settings>().text(Shortcut::WindowedFullscreen).to_owned())
}

fn action(fixture: &mut Fixture, action: DesktopAction) {
    fixture.desktop.send(Input::Action(action)).unwrap();
    fixture.inspect(|_| ());
}

fn width(fixture: &mut Fixture, surface: SurfaceId) -> Option<lince_interface::layout::desktop::Width> {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let (entity, layout) = world.query::<(Entity, &native::layout::desktop::ApplicationWindow, &native::layout::LayoutBox)>().iter(world)
            .find_map(|(entity, window, layout)| (window.surface == surface).then_some((entity, *layout))).unwrap();
        let workspace = world.get::<native::workspace::WorkspaceMember>(entity).unwrap().0;
        world.query::<&native::workspace::Workspaces>().iter(world).find_map(|spaces|
            spaces.entries.iter().find(|space| space.id == workspace).and_then(|space| space.desktop.item_width(PlacementId(layout.id))))
    })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn bounded_fullscreen_keeps_actual_popup_pixels_and_recipients_requested_edits_and_explicit_mode_transitions_in_flat_and_spatial_slots() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let engine = persisted(&runtime, directory.path());
    let (mut fixture, windows, _buffers, ids) = create(&runtime, directory.path(), [1000, 720], 1.25, engine, true);
    let qh = fixture.queue.handle();
    let bar = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let layer = fixture.client.layer_shell.as_ref().unwrap().get_layer_surface(&bar, fixture.client.output.as_ref(),
        zwlr_layer_shell_v1::Layer::Top, "bounded-input".into(), &qh, ());
    layer.set_size(0, 28);
    layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left | zwlr_layer_surface_v1::Anchor::Right);
    layer.set_exclusive_zone(28);
    bar.commit();
    fixture.exchange();
    let bar_buffer = privacy::buffer(&fixture, [800, 28], [24, 24, 24, 255]);
    bar.attach(Some(&bar_buffer), 0, 0);
    bar.damage_buffer(0, 0, 800, 28);
    bar.commit();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
        geometry.outputs[0].usable.is_some_and(|usable| usable.origin()[1] >= 28.0))));
    capture_binding(&mut fixture, true);
    for (spatial, layout, floating) in [(false, WorkspaceLayout::Tiled, false), (false, WorkspaceLayout::Free, false),
        (true, WorkspaceLayout::Tiled, true), (true, WorkspaceLayout::Free, false)]
    {
        select(&mut fixture, ids[0]);
        action(&mut fixture, DesktopAction::SetLayout(layout));
        fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            world.get_mut::<native::topology::view::View>(root).unwrap().spatial = spatial;
        });
        if floating { action(&mut fixture, DesktopAction::ToggleFloating); }
        let neighbour = fixture.desktop.current.as_ref().unwrap().frame.windows.iter().find(|window| window.surface == ids[1]).unwrap().size;
        chord(&mut fixture);
        mode(&mut fixture, ids[0], &windows[0], true);
        assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.windows.iter().find(|window| window.surface == ids[1]).unwrap().size, neighbour);
        let buttons = fixture.client.button_recipients.len();
        fixture.desktop.move_pointer([400.0, 14.0], 35).unwrap();
        fixture.tick();
        fixture.desktop.button(272, true, 36).unwrap();
        fixture.tick();
        fixture.desktop.button(272, false, 37).unwrap();
        fixture.until(|fixture| fixture.client.button_recipients.len() >= buttons + 2);
        assert_eq!(&fixture.client.button_recipients[buttons..], &[(Some(bar.id()), 272, true), (Some(bar.id()), 272, false)]);
        select(&mut fixture, ids[0]);
        let qh = fixture.queue.handle();
        let serial = fixture.client.key_serial;
        fixture.desktop.key(30, true, 40).unwrap();
        fixture.until(|fixture| fixture.client.key_serial != serial);
        let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
        positioner.set_size(120, 70);
        positioner.set_anchor_rect(300, 40, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::Right);
        positioner.set_gravity(xdg_positioner::Gravity::Right);
        positioner.set_offset(20, 0);
        let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
        let popup = xdg.get_popup(Some(&windows[0].xdg), &positioner, &qh, ());
        popup.grab(fixture.client.seat.as_ref().unwrap(), fixture.client.key_serial);
        surface.commit();
        fixture.exchange();
        let buffer = privacy::buffer(&fixture, [120, 70], [0, 128, 255, 255]);
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, 120, 70);
        surface.commit();
        fixture.until(|fixture| fixture.desktop.server.popup_grab_active() && fixture.desktop.popups.iter()
            .any(|popup| popup.surface.id().protocol_id() == surface.id().protocol_id()));
        fixture.desktop.key(30, false, 41).unwrap();
        fixture.tick();
        let pixels = appearance::pixels(&mut fixture, &format!("windowed-popup-{spatial}-{layout:?}"));
        let orange = pixels.chunks_exact(4).enumerate().filter(|(_, pixel)| pixel[0] > 240 && (110..=145).contains(&pixel[1]) && pixel[2] < 15)
            .map(|(index, _)| [index % 1000, index / 1000]).collect::<Vec<_>>();
        assert!(orange.len() > 200, "The real popup was cropped or missing in the bounded {spatial}/{layout:?} slot");
        let pixel = std::array::from_fn(|axis| orange.iter().map(|point| point[axis] as f64 + 0.5).sum::<f64>() / orange.len() as f64);
        let mapping = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(OutputId(1)).unwrap();
        let point = mapping.raster().logical_point(pixel);
        let parent = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter()
            .find(|item| item.id == ItemId::Application(ids[0])).unwrap();
        assert!(parent.surface_point(point).is_none(), "The fixture did not actually expose popup pixels outside the parent");
        let buttons = fixture.client.button_recipients.len();
        fixture.desktop.move_pointer(point, 42).unwrap();
        fixture.tick();
        fixture.desktop.button(272, true, 43).unwrap();
        fixture.tick();
        fixture.desktop.button(272, false, 44).unwrap();
        fixture.until(|fixture| fixture.client.button_recipients.len() >= buttons + 2);
        assert_eq!(&fixture.client.button_recipients[buttons..], &[(Some(surface.id()), 272, true), (Some(surface.id()), 272, false)]);
        for _ in 0..3 { windows[0].role.set_fullscreen(None); fixture.tick(); }
        mode(&mut fixture, ids[0], &windows[0], true);
        popup.destroy();
        xdg.destroy();
        surface.destroy();
        positioner.destroy();
        buffer.destroy();
        fixture.until(|fixture| !fixture.desktop.server.popup_grab_active() && fixture.desktop.popups.is_empty());
        let pixels = appearance::pixels(&mut fixture, &format!("windowed-dismissed-{spatial}-{layout:?}"));
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 240 && (110..=145).contains(&pixel[1]) && pixel[2] < 15).count(), 0);
        let requested = if layout == WorkspaceLayout::Tiled && !floating {
            let before = width(&mut fixture, ids[0]);
            fixture.desktop.key(125, true, 50).unwrap();
            tap(&mut fixture, 19);
            fixture.desktop.key(125, false, 51).unwrap();
            fixture.inspect(|_| ());
            let after = width(&mut fixture, ids[0]);
            assert_ne!(before, after);
            Some(after)
        } else { None };
        chord(&mut fixture);
        mode(&mut fixture, ids[0], &windows[0], false);
        if let Some(requested) = requested { assert_eq!(width(&mut fixture, ids[0]), requested); }
        chord(&mut fixture);
        mode(&mut fixture, ids[0], &windows[0], true);
        action(&mut fixture, DesktopAction::ToggleMaximized);
        mode(&mut fixture, ids[0], &windows[0], false);
        assert!(fixture.desktop.current.as_ref().unwrap().frame.windows.iter().find(|window| window.surface == ids[0]).unwrap().state.maximized);
        action(&mut fixture, DesktopAction::ToggleMaximized);
        chord(&mut fixture);
        mode(&mut fixture, ids[0], &windows[0], true);
        action(&mut fixture, DesktopAction::ToggleFullscreen);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.fullscreen
            && frame.frame.windows.iter().any(|window| window.surface == ids[0] && window.state.fullscreen && !window.state.windowed_fullscreen)));
        action(&mut fixture, DesktopAction::ToggleFullscreen);
        mode(&mut fixture, ids[0], &windows[0], false);
        if floating { action(&mut fixture, DesktopAction::ToggleFloating); }
        super::configuration::application_input(&mut fixture, ids[1], &windows[1]);
    }
    layer.destroy();
    bar.destroy();
    bar_buffer.destroy();
    fixture.tick();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_properties_and_saved_rebound_input_keep_the_captured_record_slot_and_native_focus_then_cold_start_without_session_mode() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let path = directory.path().join(size[0].to_string());
        std::fs::create_dir(&path).unwrap();
        let engine = persisted(&runtime, &path);
        let weak = std::sync::Arc::downgrade(&engine);
        let (mut fixture, windows, _buffers, ids) = create(&runtime, &path, size, scale, engine.clone(), true);
        let (inspector, placement, uid) = inspect(&mut fixture, ids[0]);
        let (other, _, _) = inspect(&mut fixture, ids[1]);
        assert_ne!(inspector, other);
        let (workspace, inspector_placement) = fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            (world.get::<native::workspace::WorkspaceMember>(inspector).unwrap().0, world.get::<native::layout::LayoutBox>(inspector).unwrap().id)
        });
        fixture.desktop.send(Input::Action(DesktopAction::SelectPlacement { workspace, item: Some(inspector_placement) })).unwrap();
        fixture.inspect(|_| ());
        let before = focused(&mut fixture);
        let neighbour = fixture.desktop.current.as_ref().unwrap().frame.windows.iter().find(|window| window.surface == ids[1]).unwrap().size;
        let button = control(&mut fixture, inspector, true, true);
        mode(&mut fixture, ids[0], &windows[0], true);
        assert_eq!(focused(&mut fixture), before);
        assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::a11y::AccessibilityNode>(button).unwrap().toggled()), Some(accesskit::Toggled::True));
        assert!(!fixture.desktop.current.as_ref().unwrap().frame.windows.iter().find(|window| window.surface == ids[1]).unwrap().state.fullscreen);
        assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.windows.iter().find(|window| window.surface == ids[1]).unwrap().size, neighbour);
        let expected = uid.clone();
        assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<native::full_record::window::Metadata>(inspector)
            .is_some_and(|metadata| metadata.placement == placement && metadata.uid == expected && metadata.live)));
        control(&mut fixture, inspector, false, false);
        mode(&mut fixture, ids[0], &windows[0], false);
        let _ = appearance::pixels(&mut fixture, &format!("windowed-properties-{}", size[0]));
        let previous = std::fs::read(path.join("interface.lingua")).ok();
        assert_eq!(binding(&mut fixture), "Unbound");
        capture_binding(&mut fixture, false);
        assert_eq!(binding(&mut fixture), "Unbound");
        assert_eq!(std::fs::read(path.join("interface.lingua")).ok(), previous);
        capture_binding(&mut fixture, true);
        assert_eq!(binding(&mut fixture), "Mod+Alt+F");
        select(&mut fixture, ids[0]);
        chord(&mut fixture);
        mode(&mut fixture, ids[0], &windows[0], true);
        super::configuration::application_input(&mut fixture, ids[1], &windows[1]);
        select(&mut fixture, ids[0]);
        fixture.desktop.key(125, true, 30).unwrap();
        fixture.desktop.key(56, true, 31).unwrap();
        fixture.desktop.key(33, true, 32).unwrap();
        for _ in 0..5 { fixture.desktop.key(33, true, 33).unwrap(); }
        fixture.desktop.key(33, false, 34).unwrap();
        fixture.desktop.key(56, false, 35).unwrap();
        fixture.desktop.key(125, false, 36).unwrap();
        mode(&mut fixture, ids[0], &windows[0], false);
        chord(&mut fixture);
        mode(&mut fixture, ids[0], &windows[0], true);
        fixture.desktop.worker.stop().unwrap();
        drop(fixture);
        drop(engine);
        released(&runtime, weak);
        let engine = persisted(&runtime, &path);
        let (mut restored, _, _, _) = create(&runtime, &path, size, scale, engine, false);
        assert!(restored.desktop.server.windows().is_empty());
        assert!(restored.desktop.current.as_ref().unwrap().frame.windows.is_empty());
        assert_eq!(binding(&mut restored), "Mod+Alt+F");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let expected = uid.clone();
            if restored.inspect(move |canvas| {
                let world = canvas.world_mut();
                world.query::<&native::full_record::window::Metadata>().iter(world).any(|metadata| metadata.uid == expected && !metadata.live)
            }) { break; }
            assert!(Instant::now() < deadline, "The saved inspector did not return as closed metadata after cold startup");
            std::thread::sleep(Duration::from_millis(2));
        }
        restored.desktop.worker.stop().unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum Retire { Record, Close, Output, Modal, Lock }

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_properties_cannot_toggle_after_record_close_output_modal_or_lock_retirement_and_fresh_input_recovers() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for retirement in [Retire::Record, Retire::Close, Retire::Output, Retire::Modal, Retire::Lock] {
        let path = directory.path().join(format!("{retirement:?}"));
        std::fs::create_dir(&path).unwrap();
        let engine = persisted(&runtime, &path);
        let (mut fixture, windows, _buffers, ids) = create(&runtime, &path, [1000, 720], 1.25, engine, true);
        let (inspector, placement, _) = inspect(&mut fixture, ids[0]);
        let button = button(&mut fixture, inspector, "Enter windowed fullscreen");
        magnification::focus_entity(&mut fixture, button, "Enter windowed fullscreen");
        let (_, points) = fixture.inspect(move |canvas| canvas.native_test_points(button)).unwrap();
        let point: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() * 0.25);
        let captured = fixture.desktop.current.as_ref().unwrap().frame.input.native_pointer.target(point).unwrap();
        let (_, release) = fixture.pause();
        let mut lock = None;
        match retirement {
            Retire::Record => {
                fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
                    let world = canvas.world_mut();
                    let entity = world.query::<(Entity, &native::layout::LayoutBox)>().iter(world)
                        .find(|(_, layout)| layout.id == placement).unwrap().0;
                    let uid = nucleus::new_uid("r");
                    world.get_mut::<native::protein_area::RecordBinding>(entity).unwrap().uid = uid.clone();
                    world.get_mut::<native::full_record::window::Metadata>(inspector).unwrap().uid = uid.clone();
                    if let Some(mut binding) = world.get_mut::<native::protein_area::RecordBinding>(inspector) { binding.uid = uid; }
                }))).unwrap();
            }
            Retire::Close => {
                windows[0].role.destroy(); windows[0].xdg.destroy(); windows[0].surface.destroy();
                fixture.exchange();
                fixture.desktop.dispatch().unwrap();
            }
            Retire::Output => {
                fixture.desktop.server.disconnect_outputs();
                fixture.desktop.server.registered_output_catalog().unwrap();
                fixture.desktop.invalidate_outputs().unwrap();
                fixture.desktop.dispatch().unwrap();
            }
            Retire::Modal => { fixture.desktop.worker.input(Input::Action(DesktopAction::OpenOperation)).unwrap(); }
            Retire::Lock => {
                lock = Some(fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ()));
                fixture.exchange();
                fixture.desktop.dispatch().unwrap();
                assert!(fixture.desktop.server.locked());
            }
        }
        fixture.desktop.worker.input(Input::CapturedButton(bevy::input::mouse::MouseButton::Left, true, point.map(|value| value as f32), 0, captured)).unwrap();
        let mut serial = fixture.desktop.worker.input(Input::Button(bevy::input::mouse::MouseButton::Left, false, point.map(|value| value as f32), 0)).unwrap();
        let mut filled = 0;
        loop {
            match fixture.desktop.worker.input(Input::Pointer([0.0; 2])) {
                Ok(next) => { serial = next; filled += 1; assert!(filled <= 128); }
                Err(std::sync::mpsc::TrySendError::Full(_)) => break,
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => panic!("The actual worker disconnected before its queue reached capacity"),
            }
        }
        assert!(filled > 0);
        release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
        assert!(fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            world.query::<&native::workspace::Workspaces>().iter(world).all(|spaces|
                spaces.entries.iter().all(|workspace| !workspace.desktop.windowed_fullscreen(PlacementId(placement))))
        }));
        assert!(!fixture.desktop.current.as_ref().unwrap().frame.windows.iter().any(|window| window.state.windowed_fullscreen));
        assert!(fixture.desktop.buttons.is_empty());
        if matches!(retirement, Retire::Lock) {
            assert!(lock.as_ref().unwrap().is_alive());
            assert!(fixture.client.keyboard_focus.is_none());
            fixture.desktop.worker.stop().unwrap();
            continue;
        }
        if matches!(retirement, Retire::Output) {
            fixture.desktop.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: "Windowed-Fullscreen".into(), origin: [0, 0], size: [1000, 720] }],
                &std::collections::BTreeMap::from([(OutputId(1), 1.25)])).unwrap();
            fixture.desktop.server.registered_output_catalog().unwrap();
            fixture.desktop.invalidate_outputs().unwrap();
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 1)));
        }
        if matches!(retirement, Retire::Modal) {
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.keyboard_modal));
            tap(&mut fixture, 1);
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.keyboard_modal));
        }
        if matches!(retirement, Retire::Close) {
            let button = self::button(&mut fixture, inspector, "Enter windowed fullscreen");
            assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::ui::InteractionDisabled>(button).is_some()));
        } else {
            let inspector = if matches!(retirement, Retire::Record) { inspect(&mut fixture, ids[0]).0 } else { inspector };
            control(&mut fixture, inspector, true, false);
            mode(&mut fixture, ids[0], &windows[0], true);
            control(&mut fixture, inspector, false, false);
            mode(&mut fixture, ids[0], &windows[0], false);
        }
        super::configuration::application_input(&mut fixture, ids[1], &windows[1]);
        fixture.desktop.worker.stop().unwrap();
    }
}
