use super::*;
use bevy::input::{ButtonState, keyboard::Key};

#[test]
fn modifier_release_keeps_before_and_after_masks_separate_for_the_matching_event() {
    let mut world = World::new();
    let window = world.spawn_empty().id();
    let mut host = Host::default();
    host.modifiers(window, ModifiersState::ALT);
    let mut release = key(window, KeyCode::AltLeft);
    release.state = ButtonState::Released;
    release.logical_key = Key::Alt;
    host.keyboard(release.clone());
    host.modifiers(window, ModifiersState::empty());
    assert_eq!(
        host.event(&release),
        Some(Modifiers {
            alt: true,
            ..Modifiers::NONE
        })
    );
    assert_eq!(host.after(), Some(Modifiers::NONE));
    host.clear();
    assert_eq!(host.after(), None);
}

fn key(window: Entity, code: KeyCode) -> KeyboardInput {
    KeyboardInput {
        window,
        key_code: code,
        logical_key: Key::Character("s".into()),
        state: ButtonState::Pressed,
        repeat: false,
        text: Some("s".into()),
    }
}

#[test]
fn converted_host_events_retain_masks_from_their_position_in_the_batch() {
    let mut world = World::new();
    let window = world.spawn_empty().id();
    let input = key(window, KeyCode::KeyS);
    let mut host = Host::default();
    host.keyboard(input.clone());
    host.modifiers(window, ModifiersState::CONTROL);
    host.keyboard(input.clone());
    host.modifiers(window, ModifiersState::CONTROL | ModifiersState::SHIFT);
    host.keyboard(input.clone());
    host.modifiers(window, ModifiersState::empty());
    host.keyboard(input.clone());
    assert_eq!(host.event(&input), None);
    assert_eq!(host.event(&input), Some(Modifiers::CONTROL));
    assert_eq!(
        host.event(&input),
        Some(Modifiers {
            control: true,
            shift: true,
            ..Modifiers::NONE
        })
    );
    assert_eq!(host.event(&input), Some(Modifiers::NONE));
    assert_eq!(host.current(window), Some(Modifiers::NONE));
    assert!(host.keys.is_empty());
}

#[test]
fn synthetic_events_do_not_consume_another_window_or_keys_host_context() {
    let mut world = World::new();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    let input = key(first, KeyCode::KeyS);
    let mut host = Host::default();
    host.modifiers(first, ModifiersState::CONTROL);
    host.modifiers(second, ModifiersState::SHIFT);
    host.keyboard(input.clone());
    host.modifiers(first, ModifiersState::empty());
    assert_eq!(
        host.event(&key(first, KeyCode::KeyA)),
        Some(Modifiers::NONE)
    );
    assert_eq!(
        host.event(&key(second, KeyCode::KeyS)),
        Some(Modifiers {
            shift: true,
            ..Modifiers::NONE
        })
    );
    assert_eq!(host.event(&input), Some(Modifiers::CONTROL));
    assert!(host.keys.is_empty());
    host.remove(first);
    assert_eq!(host.current(first), None);
    assert!(host.current(second).is_some());
    host.clear();
    assert_eq!(host.current(second), None);
    assert_eq!(host.active(), None);
}

#[test]
fn discarded_window_events_cannot_hide_a_later_keys_chronological_mask() {
    let mut world = World::new();
    let retired = world.spawn_empty().id();
    let live = world.spawn_empty().id();
    let input = key(live, KeyCode::KeyS);
    let mut host = Host::default();
    host.modifiers(retired, ModifiersState::SHIFT);
    host.keyboard(key(retired, KeyCode::KeyS));
    host.modifiers(live, ModifiersState::CONTROL);
    host.keyboard(input.clone());
    host.modifiers(live, ModifiersState::empty());
    world.despawn(retired);
    assert_eq!(host.event(&input), Some(Modifiers::CONTROL));
    assert!(host.keys.is_empty());
    assert_eq!(host.current(live), Some(Modifiers::NONE));
}
