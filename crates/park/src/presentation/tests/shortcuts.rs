use super::*;
use lince_interface::native::actions::Action;

#[derive(Resource, Default)]
struct Received(Vec<KeyCode>);

pub(super) fn settle(canvas: &mut Presentation) -> Frame {
    let mut frame = canvas.probe_frame().unwrap();
    for _ in 0..3 {
        frame = canvas.probe_frame().unwrap();
    }
    frame.wait_for_probe().unwrap();
    frame
}

pub(super) fn named_button(canvas: &mut Presentation, text: &str) -> Entity {
    let world = canvas.world_mut();
    world
        .query::<(Entity, &Children, &native::actions::ActionButton)>()
        .iter(world)
        .find(|(entity, children, _)| {
            let mut ancestor = Some(*entity);
            for _ in 0..32 {
                let Some(current) = ancestor else { break };
                if world
                    .get::<Node>(current)
                    .is_some_and(|node| node.display == Display::None)
                {
                    return false;
                }
                ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
            }
            world
                .get::<InheritedVisibility>(*entity)
                .is_some_and(|visible| visible.get())
                && world
                    .get::<ComputedNode>(*entity)
                    .is_some_and(|node| node.size().min_element() > 0.0)
                && children.iter().any(|child| {
                    world
                        .get::<Text>(child)
                        .is_some_and(|value| value.0 == text)
                })
        })
        .map(|(entity, _, _)| entity)
        .unwrap()
}

pub(super) fn capture_button(canvas: &mut Presentation, command: &str) -> Entity {
    let world = canvas.world_mut();
    let row = world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(_, name)| name.as_str() == command)
        .map(|(entity, _)| entity)
        .unwrap();
    let button = world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(entity, name)| {
            if name.as_str() != "shortcut.capture" {
                return false;
            }
            let mut cursor = Some(*entity);
            for _ in 0..16 {
                let Some(entity) = cursor else {
                    break;
                };
                if entity == row {
                    return true;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        })
        .map(|(entity, _)| entity)
        .unwrap();
    super::displays::reveal(canvas, button);
    button
}

pub(super) fn click_with_keys(
    canvas: &mut Presentation,
    entity: Entity,
    size: [u32; 2],
    input: &[(KeyCode, bool)],
) {
    let world = canvas.app.world();
    let node = world.get::<ComputedNode>(entity).unwrap();
    assert!(node.size().min_element() > 0.0);
    let position = world
        .get::<UiGlobalTransform>(entity)
        .unwrap()
        .transform_point2(Vec2::ZERO);
    assert!(
        position.x >= 0.0
            && position.y >= 0.0
            && position.x < size[0] as f32
            && position.y < size[1] as f32,
        "control outside viewport: {position:?}"
    );
    if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(entity) {
        assert!(clip.clip.contains(position), "control center is clipped");
    }
    canvas.pointer(position.to_array()).unwrap();
    settle(canvas);
    canvas.button(MouseButton::Left, true).unwrap();
    settle(canvas);
    if !input.is_empty() {
        let pressed = canvas
            .app
            .world()
            .get::<UiGlobalTransform>(entity)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        assert!(
            pressed.distance(position) < 0.5,
            "Capture control moved under a held pointer: {position:?} -> {pressed:?}"
        );
    }
    canvas.button(MouseButton::Left, false).unwrap();
    for (input, pressed) in input {
        canvas
            .key(
                *input,
                Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                None,
                *pressed,
                false,
            )
            .unwrap();
    }
    settle(canvas);
}

pub(super) fn click(canvas: &mut Presentation, entity: Entity, size: [u32; 2]) {
    click_with_keys(canvas, entity, size, &[]);
}

pub(super) fn key(canvas: &mut Presentation, physical: KeyCode, pressed: bool) {
    let logical = match physical {
        KeyCode::ControlLeft | KeyCode::ControlRight => Key::Control,
        KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
        KeyCode::SuperLeft | KeyCode::SuperRight => Key::Super,
        KeyCode::Escape => Key::Escape,
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Space => Key::Space,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        _ => Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
    };
    canvas.key(physical, logical, None, pressed, false).unwrap();
    settle(canvas);
}

pub(super) fn capture(name: &str, size: [u32; 2], frame: &Frame) {
    let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let mut image = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    for pixel in pixels.chunks_exact(4) {
        image.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(directory.join(format!("{name}.ppm")), image).unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn shortcut_capture_uses_real_pointer_keyboard_and_rendered_modal_at_wide_and_narrow_sizes() {
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
    canvas.app.init_resource::<Received>().add_observer(
        |event: On<bevy::input_focus::FocusedInput<KeyboardInput>>,
         mut received: ResMut<Received>| {
            if event.focused_entity == event.original_event_target()
                && event.input.state.is_pressed()
            {
                received.0.push(event.input.key_code);
            }
        },
    );
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
    let frame = settle(&mut canvas);
    assert!(!frame.input.bindings.is_empty());
    let open = named_button(&mut canvas, "Edit all shortcuts");
    super::displays::reveal(&mut canvas, open);
    let frame = settle(&mut canvas);
    let position = canvas
        .app
        .world()
        .get::<UiGlobalTransform>(open)
        .unwrap()
        .transform_point2(Vec2::ZERO)
        .as_dvec2()
        .to_array();
    assert!(
        frame
            .input
            .keyboard_modal_triggers
            .iter()
            .any(|bounds| bounds.contains(position))
    );
    click(&mut canvas, open, size);
    let frame = settle(&mut canvas);
    assert!(frame.input.keyboard_modal);
    assert!(!frame.input.keyboard_capture);
    assert!(frame.input.bindings.is_empty());
    let binding = capture_button(&mut canvas, "selection.delete");
    click_with_keys(&mut canvas, binding, size, &[(KeyCode::Delete, true)]);
    key(&mut canvas, KeyCode::Delete, false);
    assert!(
        !canvas
            .app
            .world()
            .resource::<Received>()
            .0
            .contains(&KeyCode::Delete),
        "capture leaked its activation-frame key to normal input handlers"
    );
    let validate = named_button(&mut canvas, "Validate combination");
    assert!(
        canvas
            .app
            .world()
            .get::<bevy::ui::InteractionDisabled>(validate)
            .is_none(),
        "the key typed in the capture click frame was lost"
    );
    let cancel = named_button(&mut canvas, "Cancel capture");
    click(&mut canvas, cancel, size);
    canvas.world_mut().resource_mut::<Received>().0.clear();
    click_with_keys(
        &mut canvas,
        binding,
        size,
        &[
            (KeyCode::F2, true),
            (KeyCode::F2, false),
            (KeyCode::Enter, true),
            (KeyCode::Enter, false),
            (KeyCode::KeyB, true),
            (KeyCode::KeyB, false),
        ],
    );
    assert!(!settle(&mut canvas).input.keyboard_capture);
    assert_eq!(canvas.app.world().resource::<Received>().0, [KeyCode::KeyB]);
    let children = canvas.app.world().get::<Children>(binding).unwrap();
    assert!(children.iter().any(|child| {
        canvas
            .app
            .world()
            .get::<Text>(child)
            .is_some_and(|text| text.0 == "F2")
    }));
    click(&mut canvas, binding, size);
    assert!(settle(&mut canvas).input.keyboard_capture);
    key(&mut canvas, KeyCode::KeyW, true);
    key(&mut canvas, KeyCode::KeyW, false);
    key(&mut canvas, KeyCode::Enter, true);
    key(&mut canvas, KeyCode::Enter, false);
    let frame = settle(&mut canvas);
    assert!(!frame.input.keyboard_capture);
    capture("shortcuts-conflicts-wide", size, &frame);
    let save = named_button(&mut canvas, "Save all changes");
    assert!(
        canvas
            .app
            .world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_some()
    );
    let forward = capture_button(&mut canvas, "camera.forward");
    click(&mut canvas, forward, size);
    key(&mut canvas, KeyCode::Delete, true);
    key(&mut canvas, KeyCode::Delete, false);
    key(&mut canvas, KeyCode::Enter, true);
    key(&mut canvas, KeyCode::Enter, false);
    assert!(
        canvas
            .app
            .world()
            .get::<bevy::ui::InteractionDisabled>(save)
            .is_none()
    );
    for next_size in [[640, 640], [360, 720]] {
        size = next_size;
        canvas.resize(size).unwrap();
        let frame = settle(&mut canvas);
        assert!(frame.input.keyboard_modal);
        capture(&format!("shortcuts-narrow-{}", size[0]), size, &frame);
        let focused = canvas
            .app
            .world()
            .get::<UiGlobalTransform>(forward)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::CalculatedClip>(forward)
                .is_none_or(|clip| clip.clip.contains(focused)),
            "focused binding is clipped after resizing"
        );
        let cancel = named_button(&mut canvas, "Cancel draft");
        let world = canvas.app.world();
        let position = world
            .get::<UiGlobalTransform>(cancel)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        assert!(
            position.x > 0.0
                && position.x < size[0] as f32
                && position.y > 0.0
                && position.y < size[1] as f32,
            "modal footer inaccessible at {size:?}: {position:?}"
        );
    }
    click(&mut canvas, save, size);
    let frame = settle(&mut canvas);
    assert!(!frame.input.keyboard_modal);
    assert!(!frame.input.bindings.is_empty());
    canvas.finish().unwrap();
}
