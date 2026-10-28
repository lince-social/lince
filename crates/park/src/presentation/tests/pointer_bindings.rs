use super::shortcuts::{capture, capture_button, click, key, named_button, settle};
use super::*;
use native::actions::Action;

#[derive(Resource, Default)]
struct Presses(usize);

fn start(canvas: &mut Presentation, command: &str, size: [u32; 2]) {
    let frame = settle(canvas);
    if !frame.input.keyboard_modal {
        capture(
            &format!("pointer-binding-missing-modal-{}", size[0]),
            size,
            &frame,
        );
    }
    assert!(
        frame.input.keyboard_modal,
        "The binding editor must be open before capturing {command}"
    );
    let control = capture_button(canvas, command);
    super::displays::reveal(canvas, control);
    click(canvas, control, size);
    assert!(settle(canvas).input.keyboard_capture);
}

fn press(canvas: &mut Presentation, caption: &str, size: [u32; 2]) {
    let control = named_button(canvas, caption);
    super::displays::reveal(canvas, control);
    click(canvas, control, size);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn mouse_binding_capture_shares_conflict_resolution_save_and_plain_click_isolation() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let mut canvas = Presentation::new(
            size,
            directory.path().join(format!("assets-{}", size[0])),
            WakeSignal::new(|| {}),
        )
        .unwrap();
        canvas.app.init_resource::<Presses>().add_observer(
            |event: On<Pointer<Press>>, mut presses: ResMut<Presses>| {
                if event.entity == event.original_event_target()
                    && event.button == bevy::picking::pointer::PointerButton::Secondary
                {
                    presses.0 += 1;
                }
            },
        );
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        let original: Vec<_> = native::shortcuts::desktop_bindings(canvas.app.world())
            .iter()
            .map(|binding| (binding.key, binding.modifiers, binding.action))
            .collect();
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        press(&mut canvas, "Edit all shortcuts", size);
        capture(
            &format!("pointer-binding-reopened-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        start(&mut canvas, "pointer.move", size);
        key(&mut canvas, KeyCode::SuperLeft, true);
        canvas
            .button(bevy::input::mouse::MouseButton::Right, true)
            .unwrap();
        settle(&mut canvas);
        assert_eq!(canvas.app.world().resource::<Presses>().0, 0);
        key(&mut canvas, KeyCode::SuperLeft, false);
        press(&mut canvas, "Validate combination", size);
        canvas
            .button(bevy::input::mouse::MouseButton::Right, false)
            .unwrap();
        settle(&mut canvas);
        let save = named_button(&mut canvas, "Save all changes");
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(save)
                .is_some()
        );
        capture(
            &format!("pointer-binding-conflict-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        start(&mut canvas, "pointer.resize", size);
        key(&mut canvas, KeyCode::ControlLeft, true);
        canvas
            .button(bevy::input::mouse::MouseButton::Middle, true)
            .unwrap();
        settle(&mut canvas);
        canvas
            .button(bevy::input::mouse::MouseButton::Middle, false)
            .unwrap();
        key(&mut canvas, KeyCode::ControlLeft, false);
        press(&mut canvas, "Validate combination", size);
        let save = named_button(&mut canvas, "Save all changes");
        super::displays::reveal(&mut canvas, save);
        let frame = settle(&mut canvas);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(save)
                .is_none()
        );
        capture(&format!("pointer-binding-review-{}", size[0]), size, &frame);
        click(&mut canvas, save, size);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        let bindings = native::shortcuts::pointer_bindings(canvas.app.world());
        assert_eq!(bindings.len(), 2);
        assert!(bindings.iter().any(|binding| binding.operation
            == lince_interface::host::Manipulation::Move
            && binding.chord.button == bevy::input::mouse::MouseButton::Right
            && binding.chord.modifiers.super_key));
        assert!(bindings.iter().any(|binding| binding.operation
            == lince_interface::host::Manipulation::Resize
            && binding.chord.button == bevy::input::mouse::MouseButton::Middle
            && binding.chord.modifiers.control));
        assert_eq!(
            native::shortcuts::desktop_bindings(canvas.app.world())
                .iter()
                .map(|binding| (binding.key, binding.modifiers, binding.action))
                .collect::<Vec<_>>(),
            original
        );
        press(&mut canvas, "Edit all shortcuts", size);
        start(&mut canvas, "pointer.move", size);
        let before = canvas.app.world().resource::<Presses>().0;
        canvas
            .button(bevy::input::mouse::MouseButton::Right, true)
            .unwrap();
        settle(&mut canvas);
        assert_eq!(canvas.app.world().resource::<Presses>().0, before + 1);
        canvas
            .button(bevy::input::mouse::MouseButton::Right, false)
            .unwrap();
        press(&mut canvas, "Cancel capture", size);
        press(&mut canvas, "Cancel draft", size);
    }
}
