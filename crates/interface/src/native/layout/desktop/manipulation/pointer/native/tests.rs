use super::*;
use crate::native::shortcuts::{Settings, Shortcut};
use bevy::{camera::RenderTarget, input::ButtonState, window::WindowRef};

fn fixture() -> (World, Entity, Entity, Entity, Vec2) {
    let (mut world, root, target, other) = crate::native::layout::desktop::tests::fixture();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(WorkspaceLayout::Free);
    world.get_mut::<CanvasItem>(target).unwrap().position.x = -200.0;
    world.get_mut::<CanvasItem>(other).unwrap().position.x = 200.0;
    focus::ensure(&mut world, root);
    super::super::super::super::publish(&mut world);
    let id = world.get::<Managed>(target).unwrap().id;
    let geometry = world
        .resource::<PublishedGeometry>()
        .0
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|item| item.id == ItemId::Native(id.0))
        .unwrap();
    let point = geometry
        .mapping
        .canvas_point(geometry.mapping.surface.size().map(|size| size * 0.5))
        .unwrap();
    let point = bevy::math::DVec2::from_array(point).as_vec2();
    let window = world
        .spawn((
            Window::default(),
            crate::native::actions::WindowActionTarget(root),
        ))
        .id();
    world
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(point));
    world.init_resource::<Messages<WindowEvent>>();
    world.init_resource::<Messages<MouseButtonInput>>();
    world.init_resource::<Messages<PointerInput>>();
    world.init_resource::<ButtonInput<KeyCode>>();
    world.init_resource::<ButtonInput<MouseButton>>();
    (world, root, target, window, point)
}

fn event(world: &mut World, window: Entity, button: MouseButton, pressed: bool) {
    let event = MouseButtonInput {
        window,
        button,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    };
    world.write_message(WindowEvent::MouseButtonInput(event));
    world.write_message(event);
    if pressed {
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(button);
    } else {
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(button);
    }
    let value = match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        _ => PointerButton::Middle,
    };
    let point = world
        .get::<Window>(window)
        .unwrap()
        .cursor_position()
        .unwrap();
    for pointer_id in [
        PointerId::Mouse,
        crate::native::topology::input::CONTENT_POINTER,
    ] {
        world.write_message(PointerInput {
            pointer_id,
            action: if pressed {
                PointerAction::Press(value)
            } else {
                PointerAction::Release(value)
            },
            location: bevy::picking::pointer::Location {
                target: RenderTarget::Window(WindowRef::Entity(window))
                    .normalize(Some(window))
                    .unwrap(),
                position: point,
            },
        });
    }
}

fn move_to(world: &mut World, window: Entity, point: Vec2) {
    let delta = world
        .get::<Window>(window)
        .unwrap()
        .cursor_position()
        .map(|last| point - last);
    world
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(point));
    world.write_message(WindowEvent::CursorMoved(bevy::window::CursorMoved {
        window,
        position: point,
        delta,
    }));
}

#[test]
fn modifier_move_is_owned_until_release_and_preserves_the_authored_preview_baseline() {
    let (mut world, root, target, window, start) = fixture();
    let original = *world.get::<CanvasItem>(target).unwrap();
    let saved = model(&world, root, 1).unwrap().clone();
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::SuperLeft);
    event(&mut world, window, MouseButton::Left, true);
    input(&mut world);
    assert!(super::super::super::active(&world));
    assert_eq!(world.resource::<Messages<PointerInput>>().len(), 0);
    assert!(
        !world
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    );
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::SuperLeft);
    move_to(&mut world, window, start + Vec2::new(80.0, 30.0));
    input(&mut world);
    assert!(super::super::super::active(&world));
    assert_eq!(model(&world, root, 1), Some(&saved));
    assert_eq!(
        world.get::<CanvasItem>(target).unwrap().position,
        original.position
    );
    event(&mut world, window, MouseButton::Left, false);
    input(&mut world);
    assert!(!super::super::super::active(&world));
    assert_eq!(
        world.get::<CanvasItem>(target).unwrap().position,
        original.position + bevy::math::DVec2::new(80.0, 30.0)
    );
    assert!(world.resource::<Native>().swallowed.is_empty());
    assert_eq!(world.resource::<Messages<PointerInput>>().len(), 0);
}

#[test]
fn rebound_pointer_cancel_quarantines_only_until_release_and_keeps_the_next_context_click() {
    let (mut world, root, target, window, start) = fixture();
    let original = *world.get::<CanvasItem>(target).unwrap();
    let mut settings = Settings::default();
    settings
        .set(Shortcut::PointerMove, "Alt+MouseRight")
        .unwrap();
    world.insert_resource(settings);
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::AltLeft);
    event(&mut world, window, MouseButton::Right, true);
    input(&mut world);
    assert!(super::super::super::active(&world));
    move_to(&mut world, window, start + Vec2::new(50.0, 0.0));
    input(&mut world);
    cancel(&mut world);
    assert_eq!(
        world.get::<CanvasItem>(target).unwrap().position,
        original.position
    );
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::AltLeft);
    event(&mut world, window, MouseButton::Right, false);
    event(&mut world, window, MouseButton::Right, true);
    event(&mut world, window, MouseButton::Right, false);
    input(&mut world);
    assert!(!super::super::super::active(&world));
    assert!(world.resource::<Native>().swallowed.is_empty());
    assert_eq!(world.resource::<Messages<PointerInput>>().len(), 4);
    assert!(
        world
            .resource::<Messages<MouseButtonInput>>()
            .iter_current_update_messages()
            .all(|event| event.button == MouseButton::Right)
    );
    let saved = model(&world, root, 1).unwrap().clone();
    input(&mut world);
    assert_eq!(model(&world, root, 1), Some(&saved));
    assert_eq!(world.resource::<Messages<PointerInput>>().len(), 4);
}

#[test]
fn focus_loss_cancels_the_preview_and_quarantines_the_held_button_after_return() {
    let (mut world, _, target, window, start) = fixture();
    let original = *world.get::<CanvasItem>(target).unwrap();
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::SuperLeft);
    event(&mut world, window, MouseButton::Left, true);
    input(&mut world);
    move_to(&mut world, window, start + Vec2::new(50.0, 0.0));
    world.write_message(WindowEvent::WindowFocused(bevy::window::WindowFocused {
        window,
        focused: false,
    }));
    input(&mut world);
    assert!(!super::super::super::active(&world));
    assert_eq!(
        world.get::<CanvasItem>(target).unwrap().position,
        original.position
    );
    event(&mut world, window, MouseButton::Left, false);
    input(&mut world);
    assert!(world.resource::<Native>().swallowed.is_empty());
    assert_eq!(world.resource::<Messages<PointerInput>>().len(), 0);
}
