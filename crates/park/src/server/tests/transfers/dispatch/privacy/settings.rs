use super::*;
use super::super::opening::{activate, click, click_entity, close, persisted, provider, ready, released};
use crate::launch::Capability;
use lince_interface::native::actions::Action;

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], scale: f64, engine: std::sync::Arc<engine::Engine>, windows: bool) -> (Fixture, Vec<ClientWindow>, Vec<wl_buffer::WlBuffer>, Vec<SurfaceId>) {
    let mut server = Server::new(size.map(|value| value as i32)).unwrap();
    server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name:"Privacy-Settings".into(), origin:[0, 0], size:size.map(|value| value as i32) }],
        &std::collections::BTreeMap::from([(OutputId(1), scale)])).unwrap();
    let mut harness = Harness::connect_authorized(server, Some(vec![Capability::Capture, Capability::WindowList]));
    let mut clients = Vec::new();
    let mut buffers = Vec::new();
    if windows {
        for color in [[255, 0, 255, 255], [0, 255, 0, 255]] {
            let window = harness.window();
            buffers.push(paint(&mut harness, &window, [320, 240], color));
            clients.push(window);
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
    (fixture, clients, buffers, ids)
}

fn tap(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
}

fn native_value(fixture: &mut Fixture, placement: [u8; 16]) -> Option<bool> {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(&native::layout::LayoutBox, &native::sand_store::StoredSand)>().iter(world).find_map(|(layout, _)| (layout.id == placement).then_some(layout.capture_excluded))
    })
}

fn profile(directory: &std::path::Path, placement: [u8; 16]) -> Option<(String, bool)> {
    let bytes = std::fs::read(directory.join("park-windows.json")).ok()?;
    let values: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    values.as_array().unwrap().iter().find_map(|profile| {
        let id: [u8; 16] = serde_json::from_value(profile["layout"]["id"].clone()).ok()?;
        if id != placement { return None; }
        Some((profile["uid"].as_str()?.to_owned(), profile["layout"]["capture_excluded"].as_bool()?))
    })
}

fn inspect(fixture: &mut Fixture, surface: SurfaceId) -> (Entity, [u8; 16], String) {
    fixture.desktop.send(Input::Activate(surface)).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| matches!(frame.frame.input.control.context.as_ref().and_then(|context| context.item),
        Some(lince_interface::host::control::Item::Application { surface: current, .. }) if current == surface.0)));
    fixture.desktop.send(Input::Action(DesktopAction::InspectWindowRecord)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let found = fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let placement = world.query::<(&native::layout::desktop::ApplicationWindow, &native::layout::LayoutBox)>().iter(world).find_map(|(window, layout)| (window.surface == surface).then_some(layout.id))?;
            world.query::<(Entity, &native::full_record::window::Metadata)>().iter(world).find_map(|(entity, metadata)| (metadata.placement == placement && metadata.live).then(|| (entity, placement, metadata.uid.clone())))
        });
        if let Some(found) = found { return found; }
        assert!(Instant::now() < deadline, "The real Window properties Record did not become available");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn screenshot(fixture: &mut Fixture, name: &str) {
    let _ = appearance::pixels(fixture, name);
}

fn control(fixture: &mut Fixture, owner: Entity, caption: &str, pointer: bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let entity = loop {
        let label = caption.to_owned();
        let found = fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world).find_map(|(entity, node)| {
                if node.label() != Some(label.as_str()) || world.get::<native::actions::ActionButton>(entity).is_none()
                    || world.get::<bevy::ui::InteractionDisabled>(entity).is_some() { return None; }
                let mut current = Some(entity);
                for _ in 0..native::layout::MAX_DEPTH {
                    let candidate = current?;
                    if candidate == owner { return Some(entity); }
                    current = world.get::<ChildOf>(candidate).map(ChildOf::parent);
                }
                None
            })
        });
        if let Some(entity) = found { break entity; }
        assert!(Instant::now() < deadline, "The captured Window properties did not expose its own eligible {caption} control");
        std::thread::sleep(Duration::from_millis(2));
    };
    if pointer { click_entity(fixture, entity, caption); }
    else { magnification::focus_entity(fixture, entity, caption); tap(fixture, 28); fixture.inspect(|_| ()); }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn native_capture_settings_use_real_scaled_controls_cancel_apply_and_file_backed_cell_startup_without_spawning_missing_applications() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let path = directory.path().join(format!("native-{}", size[0]));
        std::fs::create_dir(&path).unwrap();
        let engine = persisted(&runtime, &path);
        let weak = std::sync::Arc::downgrade(&engine);
        let (mut fixture, _windows, _buffers, _) = create(&runtime, &path, size, scale, engine.clone(), false);
        let (target, placement) = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
            let target = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::Square, "Capture native source", bevy::math::DVec2::ZERO);
            (target, world.get::<native::layout::LayoutBox>(target).unwrap().id)
        });
        fixture.desktop.key(125, true, 1).unwrap();
        tap(&mut fixture, 18);
        fixture.desktop.key(125, false, 2).unwrap();
        fixture.inspect(|_| ());
        click(&mut fixture, lince_interface::i18n::Token::Customization.text());
        fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            native::customization::CustomizationAction::Scope(native::customization::Scope::Sand(target)).apply(world, root);
        });
        click(&mut fixture, "Allowed in external capture");
        assert_eq!(native_value(&mut fixture, placement), Some(false));
        activate(&mut fixture, "Cancel / refresh");
        assert_eq!(native_value(&mut fixture, placement), Some(false));
        activate(&mut fixture, "Allowed in external capture");
        click(&mut fixture, "Apply exclusion");
        assert_eq!(native_value(&mut fixture, placement), Some(true));
        screenshot(&mut fixture, &format!("capture-native-settings-{}", size[0]));
        close(&mut fixture);
        fixture.desktop.worker.stop().unwrap();
        drop(fixture);
        drop(engine);
        released(&runtime, weak);
        let engine = persisted(&runtime, &path);
        let (mut restored, _, _, _) = create(&runtime, &path, size, scale, engine, false);
        let deadline = Instant::now() + Duration::from_secs(30);
        while native_value(&mut restored, placement).is_none() {
            assert!(Instant::now() < deadline, "The actual saved native placement did not return after the Engine/Cell reopen");
            restored.tick();
        }
        assert_eq!(native_value(&mut restored, placement), Some(true));
        assert!(restored.desktop.server.windows().is_empty());
        restored.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_window_properties_exclusion_retains_a_failed_draft_retries_the_captured_uid_and_keeps_closed_metadata_private_after_cold_start() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let path = directory.path().join(format!("window-{}", size[0]));
        std::fs::create_dir(&path).unwrap();
        let engine = persisted(&runtime, &path);
        let weak = std::sync::Arc::downgrade(&engine);
        let (mut fixture, windows, _buffers, ids) = create(&runtime, &path, size, scale, engine.clone(), true);
        let (_, other, other_uid) = inspect(&mut fixture, ids[1]);
        let (metadata, placement, uid) = inspect(&mut fixture, ids[0]);
        let deadline = Instant::now() + Duration::from_secs(30);
        while profile(&path, placement).is_none() || profile(&path, other).is_none() {
            assert!(Instant::now() < deadline);
            fixture.tick();
        }
        control(&mut fixture, metadata, "Allowed in external capture", true);
        control(&mut fixture, metadata, "Cancel / refresh", false);
        assert_eq!(profile(&path, placement), Some((uid.clone(), false)));
        control(&mut fixture, metadata, "Allowed in external capture", false);
        let file = path.join("park-windows.json");
        let backup = path.join("window-profile-last-good.json");
        std::fs::rename(&file, &backup).unwrap();
        let good = std::fs::read(&backup).unwrap();
        std::fs::create_dir(&file).unwrap();
        control(&mut fixture, metadata, "Save exclusion", true);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let failed = fixture.inspect(|canvas| {
                let world = canvas.world_mut();
                world.query::<&Text>().iter(world).any(|text| text.0.contains("directory") || text.0.contains("Directory"))
            });
            if failed { break; }
            assert!(Instant::now() < deadline, "The real profile write failure did not remain visible for retry");
        }
        assert_eq!(std::fs::read(&backup).unwrap(), good);
        screenshot(&mut fixture, &format!("capture-profile-write-retry-{}", size[0]));
        std::fs::remove_dir(&file).unwrap();
        std::fs::rename(&backup, &file).unwrap();
        control(&mut fixture, metadata, "Save exclusion", true);
        let deadline = Instant::now() + Duration::from_secs(30);
        while profile(&path, placement) != Some((uid.clone(), true)) {
            assert!(Instant::now() < deadline);
            fixture.tick();
        }
        assert_eq!(profile(&path, other), Some((other_uid.clone(), false)));
        fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            world.spawn((Node { position_type:PositionType::Absolute, left:px(0), top:px(0), width:px(100), height:px(60), ..default() }, BackgroundColor(Color::srgb(1.0, 0.0, 1.0)), GlobalZIndex(10), bevy::picking::Pickable::IGNORE, ChildOf(metadata)));
        });
        let qh = fixture.queue.handle();
        let source = fixture.client.capture.outputs.as_ref().unwrap().create_source(fixture.client.output.as_ref().unwrap(), &qh, ());
        let session = fixture.client.capture.manager.as_ref().unwrap().create_session(&source, Options::empty(), &qh, ());
        fixture.until(|fixture| fixture.desktop.server.capture_status().sessions.len() == 1);
        let external = capture(&mut fixture, &session);
        let owner = appearance::pixels(&mut fixture, "private-metadata-owner");
        assert!(owner.chunks_exact(4).any(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 140));
        assert_eq!(private_pixels(&external), 0);
        assert!(external.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count() > 100);
        windows[0].role.destroy();
        windows[0].xdg.destroy();
        windows[0].surface.destroy();
        fixture.until(|fixture| fixture.desktop.server.windows().iter().all(|window| window.id != ids[0]));
        let closed_uid = uid.clone();
        assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<native::full_record::window::Metadata>(metadata).is_some_and(|metadata| metadata.uid == closed_uid && !metadata.live)));
        assert_eq!(private_pixels(&capture(&mut fixture, &session)), 0);
        fixture.desktop.key(125, true, 30).unwrap();
        tap(&mut fixture, 18);
        fixture.desktop.key(125, false, 31).unwrap();
        fixture.inspect(|_| ());
        click(&mut fixture, "Park");
        let (stop, caption) = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world).find_map(|(entity, node)| {
                let caption = node.label()?;
                (caption.starts_with("Stop capture:") && world.get::<native::actions::ActionButton>(entity).is_some()).then(|| (entity, caption.to_owned()))
            }).expect("The actual authorized session must expose its ordinary Stop control")
        });
        click_entity(&mut fixture, stop, &caption);
        fixture.until(|fixture| fixture.desktop.server.capture_status().sessions.is_empty() && fixture.desktop.server.capture_demand().is_empty());
        close(&mut fixture);
        session.destroy();
        source.destroy();
        fixture.until(|fixture| fixture.desktop.server.capture_status().sessions.is_empty());
        fixture.desktop.worker.stop().unwrap();
        drop(fixture);
        drop(engine);
        released(&runtime, weak);
        let engine = persisted(&runtime, &path);
        let (mut restored, _, _, _) = create(&runtime, &path, size, scale, engine, false);
        assert_eq!(profile(&path, placement).map(|(_, private)| private), Some(true));
        assert_eq!(profile(&path, other), Some((other_uid.clone(), false)));
        assert!(restored.desktop.server.windows().is_empty());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let uid = uid.clone();
            let other_uid = other_uid.clone();
            let loaded = restored.inspect(move |canvas| {
                let world = canvas.world_mut();
                let metadata = world.query::<(Entity, &native::full_record::window::Metadata)>().iter(world).find_map(|(entity, metadata)| (metadata.placement == placement && metadata.uid == uid && !metadata.live).then_some(entity));
                let other = world.query::<(Entity, &native::full_record::window::Metadata)>().iter(world).find_map(|(entity, metadata)| (metadata.placement == other && metadata.uid == other_uid && !metadata.live).then_some(entity));
                if let (Some(private), Some(public)) = (metadata, other) {
                    for (parent, color) in [(private, Color::srgb(1.0, 0.0, 1.0)), (public, Color::srgb(0.0, 1.0, 0.0))] {
                        world.spawn((Node { position_type:PositionType::Absolute, left:px(0), top:px(0), width:px(100), height:px(60), ..default() }, BackgroundColor(color), GlobalZIndex(10), bevy::picking::Pickable::IGNORE, ChildOf(parent)));
                    }
                    return true;
                }
                false
            });
            if loaded { break; }
            assert!(Instant::now() < deadline, "Cold Window properties did not retain the actual saved private/public UID associations");
        }
        let qh = restored.queue.handle();
        let source = restored.client.capture.outputs.as_ref().unwrap().create_source(restored.client.output.as_ref().unwrap(), &qh, ());
        let session = restored.client.capture.manager.as_ref().unwrap().create_session(&source, Options::empty(), &qh, ());
        restored.until(|fixture| fixture.desktop.server.capture_status().sessions.len() == 1);
        let pixels = capture(&mut restored, &session);
        assert_eq!(private_pixels(&pixels), 0);
        assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count() > 100, "The saved public inspector was blanked with its private sibling");
        session.destroy();
        source.destroy();
        restored.until(|fixture| fixture.desktop.server.capture_status().sessions.is_empty());
        restored.desktop.worker.stop().unwrap();
    }
}
