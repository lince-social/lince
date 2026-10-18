use super::*;
use bevy::input_focus::{FocusCause, FocusedInput, InputFocus, tab_navigation::TabIndex};

pub(super) fn install(world: &mut World, viewport: Entity) {
    let mut node = accesskit::Node::new(accesskit::Role::Image);
    node.set_label("Map. Arrow keys pan; plus and minus zoom; Enter picks the center. D selects an area: Enter sets each corner. Escape clears selection. Home shows the world. L locates this device once.");
    world
        .entity_mut(viewport)
        .insert((TabIndex(0), bevy::a11y::AccessibilityNode(node)))
        .observe(focus)
        .observe(keyboard);
}

fn focus(
    mut event: On<Pointer<Press>>,
    viewports: Query<&Viewport>,
    mut focus: ResMut<InputFocus>,
) {
    if viewports.contains(event.entity) && event.button == PointerButton::Primary {
        focus.set(event.entity, FocusCause::Pressed);
        event.propagate(false);
    }
}

fn keyboard(
    mut event: On<FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    viewports: Query<&Viewport>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    let Ok(viewport) = viewports.get(event.focused_entity) else {
        return;
    };
    if !event.input.state.is_pressed()
        || [
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ]
        .iter()
        .any(|key| keys.pressed(*key))
    {
        return;
    }
    let amount = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        128.0
    } else {
        32.0
    };
    let action = match event.input.key_code {
        KeyCode::ArrowLeft => Command::Pan([amount, 0.0]),
        KeyCode::ArrowRight => Command::Pan([-amount, 0.0]),
        KeyCode::ArrowUp => Command::Pan([0.0, amount]),
        KeyCode::ArrowDown => Command::Pan([0.0, -amount]),
        KeyCode::Equal | KeyCode::NumpadAdd => Command::Zoom(1.0),
        KeyCode::Minus | KeyCode::NumpadSubtract => Command::Zoom(-1.0),
        KeyCode::Enter if !event.input.repeat => Command::PickCenter,
        KeyCode::KeyD if !event.input.repeat => Command::Select,
        KeyCode::Escape => Command::ClearSelection,
        KeyCode::Home => Command::Center,
        KeyCode::KeyL if !event.input.repeat => Command::Locate,
        _ => return,
    };
    event.propagate(false);
    let owner = viewport.0;
    commands.queue(move |world: &mut World| ui::command(world, owner, action));
}

#[derive(Component, Clone, Copy)]
pub(super) struct LayerButton {
    pub owner: Entity,
    pub layer: u8,
}

pub(super) fn maintain(world: &mut World) {
    let buttons: Vec<_> = world
        .query::<(Entity, &LayerButton)>()
        .iter(world)
        .map(|(entity, button)| (entity, *button))
        .collect();
    for (entity, button) in buttons {
        let Some(sand) = world.get::<MapSand>(button.owner) else {
            continue;
        };
        let (enabled, caption) = match button.layer {
            0 => (sand.show_saved, "Saved locations"),
            1 => (sand.show_live, "Live locations"),
            _ => (sand.show_public, "Public areas"),
        };
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
            node.set_role(accesskit::Role::Switch);
            node.set_label(caption);
            node.set_toggled(if enabled {
                accesskit::Toggled::True
            } else {
                accesskit::Toggled::False
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::actions::Action;
    use bevy::input::keyboard::{Key, KeyboardInput};

    fn key(app: &mut App, window: Entity, code: KeyCode) {
        app.world_mut().write_message(KeyboardInput {
            key_code: code,
            logical_key: Key::Character("x".into()),
            state: bevy::input::ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        app.update();
    }

    #[test]
    fn focused_keyboard_matches_pointer_commands_and_twenty_token_changes_stay_per_view() {
        let (mut app, root, owner) = super::super::tests::fixture();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_message::<KeyboardInput>()
            .add_systems(
                PreUpdate,
                bevy::input_focus::dispatch_focused_input::<KeyboardInput>,
            );
        let window = app
            .world_mut()
            .spawn((Window::default(), bevy::window::PrimaryWindow))
            .id();
        let viewport = app.world().get::<MapSand>(owner).unwrap().viewport;
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(viewport, FocusCause::Navigated);
        let second = spawn(
            app.world_mut(),
            root,
            1,
            DVec2::new(900.0, 0.0),
            Camera::default(),
            vec![],
            String::new(),
        );
        key(&mut app, window, KeyCode::ArrowRight);
        Command::Pan([-32.0, 0.0]).apply(app.world_mut(), second);
        assert_eq!(
            app.world().get::<MapSand>(owner).unwrap().camera,
            app.world().get::<MapSand>(second).unwrap().camera
        );
        key(&mut app, window, KeyCode::Equal);
        Command::Zoom(1.0).apply(app.world_mut(), second);
        assert_eq!(
            app.world().get::<MapSand>(owner).unwrap().camera,
            app.world().get::<MapSand>(second).unwrap().camera
        );
        key(&mut app, window, KeyCode::Enter);
        let sand = app.world().get::<MapSand>(owner).unwrap();
        assert_eq!(sand.picked, Some(sand.camera.center));
        key(&mut app, window, KeyCode::KeyD);
        key(&mut app, window, KeyCode::Enter);
        key(&mut app, window, KeyCode::ArrowDown);
        key(&mut app, window, KeyCode::Enter);
        assert!(
            app.world()
                .get::<MapSand>(owner)
                .unwrap()
                .selection
                .is_some()
        );
        key(&mut app, window, KeyCode::Escape);
        assert!(
            app.world()
                .get::<MapSand>(owner)
                .unwrap()
                .selection
                .is_none()
        );
        let unchanged = app.world().get::<MapSand>(owner).unwrap().camera;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        key(&mut app, window, KeyCode::ArrowRight);
        assert_eq!(app.world().get::<MapSand>(owner).unwrap().camera, unchanged);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::ControlLeft);
        let original = ui::palette(app.world(), second);
        for index in 0..20 {
            app.world_mut()
                .entity_mut(owner)
                .insert(crate::native::tokens::TokenOverrides(
                    std::collections::BTreeMap::from([(
                        crate::native::tokens::Token::Surface,
                        crate::native::tokens::TokenValue::Color([20 + index, 40, 60, 255]),
                    )]),
                ));
            assert_ne!(ui::palette(app.world(), owner), original);
            assert_eq!(ui::palette(app.world(), second), original);
        }
        let accessibility = app
            .world()
            .get::<bevy::a11y::AccessibilityNode>(viewport)
            .unwrap();
        assert!(accessibility.0.label().unwrap().contains("Arrow keys"));
        assert!(app.world().get::<TabIndex>(viewport).is_some());
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(viewport));
    }
}
