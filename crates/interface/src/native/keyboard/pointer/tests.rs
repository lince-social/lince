use super::*;
use bevy::input::{
    ButtonState,
    keyboard::{Key, NativeKey},
    mouse::MouseButton,
};

fn key(window: Entity, pressed: bool) -> PreparedKey {
    PreparedKey {
        input: KeyboardInput {
            window,
            key_code: KeyCode::ControlLeft,
            logical_key: Key::Control,
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: None,
            repeat: false,
        },
        modifiers: Modifiers::CONTROL,
        after: if pressed {
            Modifiers::CONTROL
        } else {
            Modifiers::NONE
        },
        modifier: true,
        roles: Modifiers::CONTROL,
        held: if pressed {
            vec![KeyCode::ControlLeft]
        } else {
            Vec::new()
        },
    }
}

#[test]
fn pointer_buttons_keep_the_key_context_at_their_position_in_a_complete_batch() {
    let mut world = World::new();
    world.init_resource::<Messages<WindowEvent>>();
    let window = world.spawn_empty().id();
    let press = key(window, true);
    let release = key(window, false);
    let down = MouseButtonInput {
        window,
        button: MouseButton::Right,
        state: ButtonState::Pressed,
    };
    let up = MouseButtonInput {
        state: ButtonState::Released,
        ..down.clone()
    };
    for event in [
        WindowEvent::KeyboardInput(press.input.clone()),
        WindowEvent::MouseButtonInput(down.clone()),
        WindowEvent::KeyboardInput(release.input.clone()),
        WindowEvent::MouseButtonInput(up.clone()),
    ] {
        world.write_message(event);
    }
    let mut state = crate::native::keyboard::State::default();
    state.events = vec![press, release];
    prepare(&world, &mut state, HashMap::new());
    assert_eq!(state.pointer.event(&down), Some((Modifiers::CONTROL, None)));
    assert_eq!(state.pointer.event(&up), Some((Modifiers::NONE, None)));
    assert!(state.pointer.event(&down).is_none());
}

#[test]
fn pointer_context_uses_prepared_modifier_roles_and_drops_lost_window_history() {
    let mut world = World::new();
    world.init_resource::<Messages<WindowEvent>>();
    let window = world.spawn_empty().id();
    let mut release = key(window, false);
    release.input.key_code = KeyCode::CapsLock;
    release.input.logical_key = Key::Unidentified(NativeKey::Unidentified);
    release.after = Modifiers::CONTROL;
    let down = MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: ButtonState::Pressed,
    };
    world.write_message(WindowEvent::KeyboardInput(release.input.clone()));
    world.write_message(WindowEvent::MouseButtonInput(down.clone()));
    let mut state = crate::native::keyboard::State::default();
    state.events = vec![release];
    prepare(&world, &mut state, HashMap::new());
    assert_eq!(state.pointer.event(&down), Some((Modifiers::CONTROL, None)));
    state.pointer.clear(&world);
    prepare(&world, &mut state, HashMap::new());
    assert!(state.pointer.event(&down).is_none());
}

#[test]
fn press_and_release_keep_their_own_cursor_positions_in_one_update() {
    let mut world = World::new();
    world.init_resource::<Messages<WindowEvent>>();
    let window = world.spawn_empty().id();
    let down = MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: ButtonState::Pressed,
    };
    let up = MouseButtonInput {
        state: ButtonState::Released,
        ..down
    };
    for (position, input) in [(Vec2::new(40.0, 50.0), down), (Vec2::new(240.0, 150.0), up)] {
        world.write_message(WindowEvent::CursorMoved(bevy::window::CursorMoved {
            window,
            position,
            delta: None,
        }));
        world.write_message(WindowEvent::MouseButtonInput(input));
    }
    let mut state = crate::native::keyboard::State::default();
    prepare(&world, &mut state, HashMap::new());
    assert_eq!(
        state.pointer.event(&down),
        Some((Modifiers::NONE, Some(Vec2::new(40.0, 50.0))))
    );
    assert_eq!(
        state.pointer.event(&up),
        Some((Modifiers::NONE, Some(Vec2::new(240.0, 150.0))))
    );
}
