use super::*;

#[derive(Clone, Copy, Debug)]
enum Retirement { Device, EndedTool, Control, Modal, Output, Lock, Seat, Logout }

fn empty(scene: &mut Scene) -> String {
    let field = scene.field;
    scene.fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(field).unwrap().value().to_string())
}

fn restore_outputs(scene: &mut Scene) {
    scene.fixture.desktop.server.configure_scaled_outputs(&[
        Monitor { id: A, name: A_NAME.into(), origin: [40, 40], size: [640, 480] },
        Monitor { id: B, name: B_NAME.into(), origin: [700, 80], size: [800, 600] },
    ], &std::collections::BTreeMap::from([(A, 1.25), (B, 2.0)])).unwrap();
    scene.fixture.desktop.server.registered_output_catalog().unwrap(); scene.fixture.desktop.invalidate_outputs().unwrap();
    scene.fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)));
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; supplied removal and seat reports do not prove physical device/VT behavior"]
fn retired_absolute_sources_controls_modals_outputs_locks_seats_and_save_failed_logout_cannot_replay_typing_or_keep_an_old_modifier() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for retirement in [Retirement::Device, Retirement::EndedTool, Retirement::Control, Retirement::Modal,
        Retirement::Output, Retirement::Lock, Retirement::Seat, Retirement::Logout]
    {
        eprintln!("Absolute source retirement: {retirement:?}");
        let path = directory.path().join(format!("{retirement:?}")); std::fs::create_dir(&path).unwrap();
        let mut scene = create(&runtime, &path);
        if matches!(retirement, Retirement::Logout) {
            scene.fixture.desktop.send(Input::Action(DesktopAction::ToggleTiling)).unwrap(); scene.fixture.inspect(|_| ());
        }
        if matches!(retirement, Retirement::EndedTool) {
            scene.devices[1].associated_output = Some(B_NAME.into());
            install_devices(&mut scene.fixture, &scene.devices, 2);
        }
        let point = native_point(&mut scene);
        let touch = scene.devices[0].clone();
        scene.fixture.desktop.touch_down(&touch, 0, point, 300); scene.fixture.desktop.flush_touch().unwrap();
        scene.fixture.desktop.touch_up(&touch, 0, 301); scene.fixture.until(|fixture| !fixture.desktop.input_pending());
        scene.fixture.desktop.key(29, true, 302).unwrap(); scene.fixture.inspect(|_| ());
        assert!(scene.fixture.inspect(|canvas| canvas.world_mut().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ControlLeft)));
        assert_eq!(empty(&mut scene), "");
        let point = native_point(&mut scene);
        let (_, release) = scene.fixture.pause();
        let mut resume = Resume(Some(release));
        let (replacement, received) = std::sync::mpsc::sync_channel(1);
        let queued = match retirement {
            Retirement::Control => {
                let field = scene.field;
                scene.fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
                    let world = canvas.world_mut();
                    let owner = world.query::<(Entity, &native::sand_store::StoredSand)>().iter(world)
                        .find_map(|(entity, stored)| (stored.content == Some(field)).then_some(entity)).unwrap();
                    let workspace = world.get::<native::workspace::WorkspaceMember>(owner).unwrap().0;
                    let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
                    assert!(world.despawn(owner));
                    let owner = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText,
                        "Absolute replacement", bevy::math::DVec2::ZERO);
                    let field = world.get::<native::sand_store::StoredSand>(owner).unwrap().content.unwrap();
                    world.get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text("replacement");
                    replacement.send(field).unwrap();
                }))).unwrap();
                1
            }
            Retirement::Modal => { scene.fixture.desktop.worker.input(Input::Action(DesktopAction::OpenOperation)).unwrap(); 1 }
            _ => 0,
        };
        let filled = fill(&scene.fixture, 0, queued);
        let pen_device = scene.devices[1].clone();
        if matches!(retirement, Retirement::EndedTool) {
            pen(&mut scene.fixture, &pen_device, point, TabletPhase::Enter);
            pen(&mut scene.fixture, &pen_device, point, TabletPhase::Tip(true));
        } else { scene.fixture.desktop.touch_down(&touch, 0, point, 303); }
        scene.fixture.desktop.flush_touch().unwrap();
        scene.fixture.desktop.key(29, false, 304).unwrap(); key(&mut scene.fixture, 30);
        if matches!(retirement, Retirement::EndedTool) {
            pen(&mut scene.fixture, &pen_device, point, TabletPhase::Tip(false)); pen(&mut scene.fixture, &pen_device, point, TabletPhase::Leave);
        } else { scene.fixture.desktop.touch_up(&touch, 0, 305); }
        let mut lock = None;
        let snapshots = path.join("interface.snapshots");
        let retained = path.join("retained.snapshots");
        match retirement {
            Retirement::Device => scene.fixture.desktop.touch_device_removed(&touch.id),
            Retirement::EndedTool => scene.fixture.desktop.tablet_device_removed(&pen_device.id, pen_device.generation, 306),
            Retirement::Output => {
                scene.fixture.desktop.server.configure_scaled_outputs(&[Monitor { id: A, name: A_NAME.into(), origin: [40, 40], size: [640, 480] }],
                    &std::collections::BTreeMap::from([(A, 1.25)])).unwrap();
                scene.fixture.desktop.server.registered_output_catalog().unwrap(); scene.fixture.desktop.invalidate_outputs().unwrap();
            }
            Retirement::Lock => {
                lock = Some(scene.fixture.client.lock_manager.as_ref().unwrap().lock(&scene.fixture.queue.handle(), ()));
                scene.fixture.exchange(); scene.fixture.desktop.dispatch().unwrap();
                assert!(scene.fixture.desktop.server.locked());
            }
            Retirement::Seat => { scene.fixture.desktop.event(smithay::backend::winit::WinitEvent::Focus(false)).unwrap(); }
            Retirement::Logout => {
                if snapshots.exists() { std::fs::rename(&snapshots, &retained).unwrap(); }
                std::fs::write(&snapshots, b"blocked snapshot directory").unwrap();
                scene.fixture.desktop.logout().unwrap();
                assert!(scene.fixture.desktop.running && scene.fixture.desktop.worker.is_running());
            }
            Retirement::Control | Retirement::Modal => {}
        }
        resume.0.take().unwrap().send(()).unwrap();
        scene.fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= filled) && !fixture.desktop.input_pending());
        scene.fixture.tick();
        if matches!(retirement, Retirement::Control) { scene.field = received.recv_timeout(Duration::from_secs(1)).unwrap(); }
        assert_eq!(empty(&mut scene), if matches!(retirement, Retirement::Control) { "replacement" } else { "" });
        assert!(scene.fixture.desktop.keys.is_empty());
        assert!(!scene.fixture.inspect(|canvas| canvas.world_mut().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ControlLeft)));
        let field = scene.field;
        scene.fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let editor = world.get::<bevy::text::EditableText>(field).unwrap();
            assert!(!editor.is_composing() && editor.pending_paste.is_none());
            assert_eq!(world.resource::<ButtonInput<KeyCode>>().get_pressed().count(), 0);
            assert!(world.resource::<crate::native_input::absolute::Contacts>().0.is_empty());
            assert!(world.resource::<crate::native_input::absolute::Pens>().tools.is_empty());
        });
        assert!(scene.fixture.desktop.running && scene.fixture.desktop.worker.is_running());
        if let Some(lock) = lock {
            scene.fixture.until(|fixture| fixture.client.lock_replies.last() == Some(&true));
            lock.unlock_and_destroy(); scene.fixture.until(|fixture| !fixture.desktop.server.locked());
        }
        if matches!(retirement, Retirement::Modal) {
            key(&mut scene.fixture, 1); scene.fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| !frame.frame.input.keyboard_modal));
        }
        if matches!(retirement, Retirement::Output) {
            restore_outputs(&mut scene);
            scene.fixture.desktop.send(Input::Output(B)).unwrap();
            scene.fixture.desktop.send(Input::Action(DesktopAction::Workspace(2))).unwrap();
            scene.fixture.inspect(|_| ());
            if scene.fixture.desktop.current.as_ref().is_none_or(|frame| frame.frame.output != Some(B)) {
                scene.fixture.desktop.send(Input::Action(DesktopAction::MoveWorkspaceToOutput(Direction::Right))).unwrap();
                scene.fixture.inspect(|_| ());
            }
        }
        if matches!(retirement, Retirement::Seat) { scene.fixture.desktop.event(smithay::backend::winit::WinitEvent::Focus(true)).unwrap(); scene.fixture.inspect(|_| ()); }
        if matches!(retirement, Retirement::Logout) {
            assert!(snapshots.is_file(), "The actual snapshot save failure was not preserved for recovery");
            std::fs::remove_file(&snapshots).unwrap();
            if retained.exists() { std::fs::rename(&retained, &snapshots).unwrap(); }
        }
        if matches!(retirement, Retirement::Device) { scene.devices[0].generation += 1; install_devices(&mut scene.fixture, &scene.devices, 3); }
        let point = native_point(&mut scene);
        let fresh = scene.devices[0].clone();
        scene.fixture.desktop.touch_down(&fresh, 0, point, 310); scene.fixture.desktop.flush_touch().unwrap();
        key(&mut scene.fixture, 44); scene.fixture.desktop.touch_up(&fresh, 0, 311);
        scene.fixture.until(|fixture| !fixture.desktop.input_pending());
        assert!(empty(&mut scene).contains('z'), "Fresh unmodified input did not recover after {retirement:?}");
        assert!(scene.fixture.desktop.keys.is_empty());
        scene.fixture.desktop.worker.stop().unwrap();
    }
}
