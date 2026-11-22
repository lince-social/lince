use bevy::{
    input::{ButtonState, mouse::MouseButtonInput},
    picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    prelude::*,
    window::{CursorMoved, PrimaryWindow, WindowEvent},
};
use lince_sandbox::wake::WakeSignal;

pub(super) fn pointer(world: &mut World, position: Vec2) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    let previous = world
        .get::<Window>(window)
        .unwrap()
        .cursor_position()
        .unwrap_or(position);
    world
        .get_mut::<Window>(window)
        .unwrap()
        .bypass_change_detection()
        .set_cursor_position(Some(position));
    world.write_message(WindowEvent::CursorMoved(CursorMoved {
        window,
        position,
        delta: None,
    }));
    mouse_input(
        world,
        window,
        position,
        PointerAction::Move {
            delta: position - previous,
        },
    );
    world.resource::<WakeSignal>().ring();
}

fn mouse_input(world: &mut World, window: Entity, position: Vec2, action: PointerAction) {
    let target = bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window))
        .normalize(Some(window))
        .unwrap();
    world.write_message(PointerInput::new(
        PointerId::Mouse,
        Location { target, position },
        action,
    ));
}

pub(super) fn button(world: &mut World, pressed: bool) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    }));
    let position = world
        .get::<Window>(window)
        .unwrap()
        .cursor_position()
        .unwrap();
    mouse_input(
        world,
        window,
        position,
        if pressed {
            PointerAction::Press(PointerButton::Primary)
        } else {
            PointerAction::Release(PointerButton::Primary)
        },
    );
    world.resource::<WakeSignal>().ring();
}
