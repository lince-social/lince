use super::*;

#[derive(Clone, Copy, Debug)]
enum Retire { Close, Record, Modal, Output, Lock }

fn labelled(fixture: &mut Fixture, caption: &str) -> Entity {
    let caption = caption.to_owned();
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &bevy::a11y::AccessibilityNode, &native::actions::ActionButton)>().iter(world)
            .find_map(|(entity, node, _)| (node.label() == Some(caption.as_str())).then_some(entity)).unwrap()
    })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_compact_confirmation_refuses_closed_replaced_modal_output_and_locked_owners_then_fresh_choices_recover() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for retirement in [Retire::Close, Retire::Record, Retire::Modal, Retire::Output, Retire::Lock] {
        let path = directory.path().join(format!("{retirement:?}"));
        std::fs::create_dir(&path).unwrap();
        let engine = persisted(&runtime, &path);
        let (mut fixture, windows, _buffers, ids) = windowed_fullscreen::create(&runtime, &path, [1000, 720], 1.25, engine, true);
        windows[1].role.set_title("editor one".into());
        windows[1].role.set_app_id("org.example.editor".into());
        action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
        let (first, first_id) = application(&mut fixture, ids[0]);
        let (second, _) = application(&mut fixture, ids[1]);
        let (inspector, _, uid) = windowed_fullscreen::inspect(&mut fixture, ids[0]);
        property(&mut fixture, inspector, "Create a compact group");
        let choice = choose(&mut fixture, second);
        let owner = named(&mut fixture, "park.compact-chooser");
        let confirmation = labelled(&mut fixture, "Create selected group");
        magnification::focus_entity(&mut fixture, confirmation, "Create selected group");
        let (_, points) = fixture.inspect(move |canvas| canvas.native_test_points(confirmation)).unwrap();
        let point: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() * 0.25);
        let captured = fixture.desktop.current.as_ref().unwrap().frame.input.native_pointer.target(point).unwrap();
        let mut operation = None;
        if matches!(retirement, Retire::Modal) {
            activate(&mut fixture, "Cancel compact changes");
            action(&mut fixture, DesktopAction::OpenOperation);
            operation = Some(named(&mut fixture, "operation.input"));
        }
        let (_, release) = fixture.pause();
        let mut lock = None;
        match retirement {
            Retire::Close => {
                windows[1].role.destroy(); windows[1].xdg.destroy(); windows[1].surface.destroy();
                fixture.exchange();
                fixture.desktop.dispatch().unwrap();
            }
            Retire::Record => {
                fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
                    canvas.world_mut().get_mut::<native::protein_area::RecordBinding>(first).unwrap().uid = nucleus::new_uid("r");
                }))).unwrap();
            }
            Retire::Modal => {}
            Retire::Output => {
                fixture.desktop.server.disconnect_outputs();
                fixture.desktop.server.registered_output_catalog().unwrap();
                fixture.desktop.invalidate_outputs().unwrap();
                fixture.desktop.dispatch().unwrap();
            }
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
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => panic!("The compact lifetime worker disconnected before its FIFO filled"),
            }
        }
        assert!(filled > 0);
        release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
        assert!(group(&mut fixture, first_id).is_none());
        unique(&mut fixture);
        let expected_uid = uid.clone();
        assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<native::full_record::window::Metadata>(inspector)
            .is_some_and(|metadata| metadata.uid == expected_uid && metadata.placement == first_id && metadata.live)));
        assert!(fixture.desktop.buttons.is_empty());
        if matches!(retirement, Retire::Lock) {
            assert!(lock.as_ref().unwrap().is_alive());
            assert!(fixture.client.keyboard_focus.is_none());
            fixture.desktop.worker.stop().unwrap();
            continue;
        }
        if let Some(operation) = operation {
            assert!(fixture.inspect(move |canvas| canvas.world_mut().get_resource::<bevy::input_focus::InputFocus>().unwrap().get() == Some(operation)));
            tap(&mut fixture, 1);
            assert!(absent(&mut fixture, "operation.input"));
        }
        if matches!(retirement, Retire::Output) {
            fixture.desktop.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: "Windowed-Fullscreen".into(), origin: [0, 0], size: [1000, 720] }],
                &std::collections::BTreeMap::from([(OutputId(1), 1.25)])).unwrap();
            fixture.desktop.server.registered_output_catalog().unwrap();
            fixture.desktop.invalidate_outputs().unwrap();
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 1)));
        }
        if matches!(retirement, Retire::Close | Retire::Record) {
            assert!(fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                world.get_entity(owner).is_ok() && world.get::<bevy::ui::InteractionDisabled>(confirmation).is_some()
                    && world.get::<bevy::a11y::AccessibilityNode>(choice).is_some_and(|node| node.toggled() == Some(accesskit::Toggled::True))
            }));
            assert!(fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                world.query::<(Entity, &Text)>().iter(world).any(|(entity, text)| {
                    if !["original", "changed", "unavailable"].iter().any(|reason| text.0.contains(*reason)) { return false; }
                    let mut current = Some(entity);
                    for _ in 0..128 {
                        let Some(entity) = current else { return false; };
                        if entity == owner { return true; }
                        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
                    }
                    false
                })
            }));
        }
        let mut replacement = None;
        let target = if matches!(retirement, Retire::Close) {
            assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::ui::InteractionDisabled>(choice).is_some()));
            let (window, buffer, id) = opening::map(&mut fixture);
            let (entity, _) = application(&mut fixture, id);
            assert!(absent(&mut fixture, &format!("compact.choice.{}", entity.to_bits())));
            assert!(group(&mut fixture, first_id).is_none());
            replacement = Some((window, buffer, id));
            entity
        } else { second };
        if !absent(&mut fixture, "park.compact-chooser") {
            activate(&mut fixture, "Cancel compact changes");
        }
        assert!(absent(&mut fixture, "park.compact-chooser"));
        if matches!(retirement, Retire::Record) {
            fixture.inspect(move |canvas| canvas.world_mut().get_mut::<native::protein_area::RecordBinding>(first).unwrap().uid = uid);
        }
        windowed_fullscreen::select(&mut fixture, ids[0]);
        action(&mut fixture, DesktopAction::CreateCompactGroup);
        choose(&mut fixture, target);
        confirm(&mut fixture, false);
        let (_, target_id) = if let Some((_, _, id)) = &replacement { application(&mut fixture, *id) } else { application(&mut fixture, ids[1]) };
        assert_eq!(group(&mut fixture, first_id).unwrap().members, [PlacementId(first_id), PlacementId(target_id)]);
        unique(&mut fixture);
        action(&mut fixture, DesktopAction::DissolveCompactGroup);
        super::super::configuration::application_input(&mut fixture, ids[0], &windows[0]);
        fixture.desktop.worker.stop().unwrap();
    }
}
