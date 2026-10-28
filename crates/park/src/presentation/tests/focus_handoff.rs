use super::*;

#[derive(Component, Default)]
struct Count(usize);

struct Increment;

impl native::actions::Action for Increment {
    fn apply(&self, world: &mut World, target: Entity) {
        world.get_mut::<Count>(target).unwrap().0 += 1;
    }
}

fn key(app: &mut App, window: Entity, code: KeyCode, logical: Key, pressed: bool) {
    let event = KeyboardInput {
        key_code: code,
        logical_key: logical,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        text: None,
        repeat: false,
        window,
    };
    app.world_mut()
        .write_message(WindowEvent::KeyboardInput(event.clone()));
    app.world_mut().write_message(event);
}

#[test]
fn a_fresh_chord_after_an_ordered_focus_loss_does_not_need_an_intermediate_frame() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        InputPlugin,
        WindowPlugin {
            primary_window: None,
            ..default()
        },
        native::actions::ActionsPlugin,
    ))
    .add_message::<PointerInput>();
    let root = app
        .world_mut()
        .spawn((
            Count::default(),
            native::actions::KeyBindings(vec![native::actions::KeyBinding::new(
                KeyCode::KeyS,
                native::actions::Modifiers::ALT,
                native::actions![Increment],
            )]),
        ))
        .id();
    let window = app
        .world_mut()
        .spawn((
            Window::default(),
            PrimaryWindow,
            native::actions::WindowActionTarget(root),
        ))
        .id();
    key(&mut app, window, KeyCode::ControlLeft, Key::Control, true);
    app.update();
    key(
        &mut app,
        window,
        KeyCode::KeyS,
        Key::Character("s".into()),
        true,
    );
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    cancel_input(app.world_mut(), window);
    app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
    for (code, logical, pressed) in [
        (KeyCode::AltLeft, Key::Alt, true),
        (KeyCode::KeyS, Key::Character("s".into()), true),
        (KeyCode::KeyS, Key::Character("s".into()), false),
    ] {
        key(&mut app, window, code, logical, pressed);
    }
    app.update();
    assert_eq!(app.world().get::<Count>(root).unwrap().0, 1);
    assert!(
        !app.world()
            .resource::<ButtonInput<Key>>()
            .pressed(Key::Control)
    );
    let keys = app.world().resource::<ButtonInput<KeyCode>>();
    assert!(keys.pressed(KeyCode::AltLeft));
    assert!(!keys.just_released(KeyCode::AltLeft));
    assert!(!keys.pressed(KeyCode::ControlLeft));
    assert!(app.world().resource::<ButtonInput<Key>>().pressed(Key::Alt));
    assert_eq!(
        MessageCursor::<bevy::input::keyboard::KeyboardFocusLost>::default()
            .read(
                app.world()
                    .resource::<Messages<bevy::input::keyboard::KeyboardFocusLost>>()
            )
            .count(),
        1
    );
    key(&mut app, window, KeyCode::AltLeft, Key::Alt, false);
    app.update();
    assert_eq!(
        app.world()
            .resource::<ButtonInput<KeyCode>>()
            .get_pressed()
            .count(),
        0
    );
}
