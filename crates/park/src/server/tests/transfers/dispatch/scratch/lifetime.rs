use super::*;
use super::super::compact::named;

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
fn queued_scratch_properties_and_choices_cannot_reveal_closed_replaced_modal_output_or_locked_targets_and_fresh_input_recovers() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for retirement in [Retire::Close, Retire::Record, Retire::Modal, Retire::Output, Retire::Lock] {
        let path = directory.path().join(format!("{retirement:?}"));
        std::fs::create_dir(&path).unwrap();
        let engine = persisted(&runtime, &path);
        let (mut fixture, windows, _buffers, ids) = windowed_fullscreen::create(&runtime, &path, [1000, 720], 1.25, engine, true);
        action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
        windows[0].role.set_title("editor one".into()); windows[0].role.set_app_id("org.example.editor".into());
        let other = spawn_note(&mut fixture, "Independent scratch choice", "Retain the independent native choice");
        select_native(&mut fixture, other.1);
        action(&mut fixture, DesktopAction::MarkScratch); action(&mut fixture, DesktopAction::HideScratch);
        let (application, placement) = application(&mut fixture, ids[0]);
        let (inspector, _, uid) = windowed_fullscreen::inspect(&mut fixture, ids[0]);
        property(&mut fixture, inspector, "Mark as scratch");
        let mut choice = None;
        let button = if matches!(retirement, Retire::Record) {
            windowed_fullscreen::button(&mut fixture, inspector, "Unmark and restore home")
        } else {
            property(&mut fixture, inspector, "Hide this scratch item");
            action(&mut fixture, DesktopAction::ShowScratch);
            choice = Some(choose(&mut fixture, application));
            labelled(&mut fixture, "Show selected item here")
        };
        magnification::focus_entity(&mut fixture, button, "Captured scratch control");
        let (_, points) = fixture.inspect(move |canvas| canvas.native_test_points(button)).unwrap();
        let point: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() * 0.25);
        let captured = fixture.desktop.current.as_ref().unwrap().frame.input.native_pointer.target(point).unwrap();
        let operation = if matches!(retirement, Retire::Modal) {
            activate(&mut fixture, "Cancel scratch selection");
            action(&mut fixture, DesktopAction::OpenOperation);
            Some(named(&mut fixture, "operation.input"))
        } else { None };
        let (_, release) = fixture.pause();
        let mut lock = None;
        match retirement {
            Retire::Close => {
                windows[0].role.destroy(); windows[0].xdg.destroy(); windows[0].surface.destroy();
                fixture.exchange(); fixture.desktop.dispatch().unwrap();
            }
            Retire::Record => {
                fixture.desktop.worker.input(Input::Test(Box::new(move |canvas| {
                    canvas.world_mut().get_mut::<native::protein_area::RecordBinding>(application).unwrap().uid = nucleus::new_uid("r");
                }))).unwrap();
            }
            Retire::Modal => {}
            Retire::Output => {
                fixture.desktop.server.disconnect_outputs(); fixture.desktop.server.registered_output_catalog().unwrap();
                fixture.desktop.invalidate_outputs().unwrap(); fixture.desktop.dispatch().unwrap();
            }
            Retire::Lock => {
                lock = Some(fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ()));
                fixture.exchange(); fixture.desktop.dispatch().unwrap();
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
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => panic!("The scratch lifetime worker disconnected before its queue filled"),
            }
        }
        assert!(filled > 0);
        release.send(()).unwrap();
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial));
        if matches!(retirement, Retire::Close) { assert!(association(&mut fixture, placement).is_none()); }
        else { assert_eq!(association(&mut fixture, placement).unwrap().1.shown(), matches!(retirement, Retire::Record)); }
        let expected_uid = uid.clone();
        assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<native::full_record::window::Metadata>(inspector)
            .is_some_and(|metadata| metadata.uid == expected_uid && metadata.placement == placement && metadata.live != matches!(retirement, Retire::Close))));
        unique(&mut fixture);
        assert!(fixture.desktop.buttons.is_empty());
        if matches!(retirement, Retire::Lock) {
            assert!(lock.as_ref().unwrap().is_alive()); assert!(fixture.client.keyboard_focus.is_none());
            fixture.desktop.worker.stop().unwrap();
            continue;
        }
        if let Some(operation) = operation {
            assert!(fixture.inspect(move |canvas| canvas.world_mut().get_resource::<bevy::input_focus::InputFocus>().unwrap().get() == Some(operation)));
            fixture.desktop.key(1, true, 40).unwrap(); fixture.desktop.key(1, false, 41).unwrap();
            assert!(absent(&mut fixture, "operation.input"));
        }
        if matches!(retirement, Retire::Output) {
            fixture.desktop.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: "Windowed-Fullscreen".into(), origin: [0, 0], size: [1000, 720] }],
                &std::collections::BTreeMap::from([(OutputId(1), 1.25)])).unwrap();
            fixture.desktop.server.registered_output_catalog().unwrap(); fixture.desktop.invalidate_outputs().unwrap();
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 1)));
        }
        let mut replacement = None;
        let target = if matches!(retirement, Retire::Close) {
            let choice = choice.unwrap();
            assert!(fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                world.get::<bevy::ui::InteractionDisabled>(button).is_some()
                    && world.get::<bevy::ui::InteractionDisabled>(choice).is_some()
                    && world.get::<bevy::a11y::AccessibilityNode>(choice).is_some_and(|node| node.toggled() == Some(accesskit::Toggled::True))
            }));
            let (window, buffer, surface) = opening::map(&mut fixture);
            let (entity, id) = super::application(&mut fixture, surface);
            assert!(association(&mut fixture, id).is_none());
            assert!(fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                !world.query::<(&Name, &native::topology::presentation::VisualOwner)>().iter(world)
                    .any(|(name, owner)| name.as_str().starts_with("scratch.choice.") && owner.0 == entity)
            }));
            replacement = Some((window, buffer, surface));
            (entity, id)
        } else { (application, placement) };
        if !absent(&mut fixture, "park.scratch-chooser") { activate(&mut fixture, "Cancel scratch selection"); }
        if matches!(retirement, Retire::Record) {
            fixture.inspect(move |canvas| canvas.world_mut().get_mut::<native::protein_area::RecordBinding>(application).unwrap().uid = uid);
            property(&mut fixture, inspector, "Unmark and restore home");
            assert!(association(&mut fixture, placement).is_none());
        } else {
            if let Some((_, _, surface)) = &replacement {
                windowed_fullscreen::select(&mut fixture, *surface);
                action(&mut fixture, DesktopAction::MarkScratch); action(&mut fixture, DesktopAction::HideScratch);
            }
            action(&mut fixture, DesktopAction::ShowScratch); choose(&mut fixture, target.0); confirm(&mut fixture);
            assert!(association(&mut fixture, target.1).unwrap().1.shown());
            action(&mut fixture, DesktopAction::UnmarkScratch);
            assert!(association(&mut fixture, target.1).is_none());
        }
        super::super::configuration::application_input(&mut fixture, ids[1], &windows[1]);
        unique(&mut fixture);
        fixture.desktop.worker.stop().unwrap();
    }
}
