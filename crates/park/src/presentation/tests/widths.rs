use super::shortcuts::{capture, click, key, named_button, settle};
use super::*;
use lince_interface::native::actions::Action;

fn labelled(canvas: &mut Presentation, label: &str) -> Entity {
    canvas
        .world_mut()
        .query::<(Entity, &bevy::a11y::AccessibilityNode)>()
        .iter(canvas.app.world())
        .find(|(_, node)| node.label() == Some(label))
        .map(|(entity, _)| entity)
        .unwrap()
}

fn write(canvas: &mut Presentation, entity: Entity, size: [u32; 2], text: &str) {
    click(canvas, entity, size);
    for (physical, logical, pressed) in [
        (KeyCode::ControlLeft, Key::Control, true),
        (KeyCode::KeyA, Key::Character("a".into()), true),
        (KeyCode::KeyA, Key::Character("a".into()), false),
        (KeyCode::ControlLeft, Key::Control, false),
    ] {
        canvas.key(physical, logical, None, pressed, false).unwrap();
    }
    settle(canvas);
    canvas.text(text.into()).unwrap();
    settle(canvas);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(entity)
            .unwrap()
            .value()
            .to_string(),
        text
    );
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn width_presets_use_native_fields_validation_reordering_and_narrow_modal_controls() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut size = [1280, 720];
    let mut canvas = Presentation::new(
        size,
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
    settle(&mut canvas);
    let open = named_button(&mut canvas, "Edit window layout");
    click(&mut canvas, open, size);
    let frame = settle(&mut canvas);
    assert!(frame.input.keyboard_modal);
    assert!(!frame.input.keyboard_capture);
    assert!(frame.input.bindings.is_empty());
    let first = labelled(&mut canvas, "Width preset 1");
    write(&mut canvas, first, size, "1/2");
    let save = named_button(&mut canvas, "Save window layout");
    assert!(
        canvas
            .app
            .world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_some()
    );
    write(&mut canvas, first, size, "25%");
    assert!(
        canvas
            .app
            .world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_none()
    );
    let down = labelled(&mut canvas, "Move down width preset 1");
    click(&mut canvas, down, size);
    assert_eq!(labelled(&mut canvas, "Width preset 2"), first);
    capture("widths-wide", size, &settle(&mut canvas));
    for next in [[640, 640], [360, 720]] {
        size = next;
        canvas.resize(size).unwrap();
        let frame = settle(&mut canvas);
        assert!(frame.input.keyboard_modal);
        capture(&format!("widths-narrow-{}", size[0]), size, &frame);
        let point = canvas
            .app
            .world()
            .get::<UiGlobalTransform>(first)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::CalculatedClip>(first)
                .is_none_or(|clip| clip.clip.contains(point))
        );
    }
    click(&mut canvas, save, size);
    assert!(!settle(&mut canvas).input.keyboard_modal);
    let open = named_button(&mut canvas, "Edit window layout");
    click(&mut canvas, open, size);
    let first = labelled(&mut canvas, "Width preset 1");
    let second = labelled(&mut canvas, "Width preset 2");
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(first)
            .unwrap()
            .value()
            .to_string(),
        "50%"
    );
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(second)
            .unwrap()
            .value()
            .to_string(),
        "25%"
    );
    key(&mut canvas, KeyCode::Escape, true);
    key(&mut canvas, KeyCode::Escape, false);
    assert!(!settle(&mut canvas).input.keyboard_modal);
    canvas.finish().unwrap();
}
