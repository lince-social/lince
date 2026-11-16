use super::*;
use crate::native::actions::Modifiers;
use bevy::{
    ecs::message::MessageCursor,
    input::{
        keyboard::KeyboardInput,
        mouse::{MouseButton, MouseButtonInput},
    },
    picking::pointer::{PointerAction, PointerButton, PointerId, PointerInput},
    window::WindowEvent,
};
use std::collections::HashSet;

mod batch;

#[derive(Resource, Default)]
struct Native {
    windows: MessageCursor<WindowEvent>,
    raw: MessageCursor<MouseButtonInput>,
    pointers: MessageCursor<PointerInput>,
    owner: Option<(Entity, MouseButton)>,
    swallowed: HashSet<(Entity, MouseButton)>,
    escapes: HashSet<Entity>,
    keys: Vec<(KeyboardInput, bool)>,
    batch: batch::Batch,
}

fn button(button: PointerButton) -> MouseButton {
    match button {
        PointerButton::Primary => MouseButton::Left,
        PointerButton::Secondary => MouseButton::Right,
        PointerButton::Middle => MouseButton::Middle,
    }
}

fn root(world: &World, window: Entity) -> Option<Entity> {
    world
        .get::<crate::native::actions::WindowActionTarget>(window)
        .map(|target| target.0)
}

fn mouse(world: &mut World, state: &mut Native, event: &MouseButtonInput) -> bool {
    let identity = (event.window, event.button);
    let (modifiers, position) = crate::native::keyboard::pointer_context(world, event);
    if state.swallowed.contains(&identity) {
        if !event.state.is_pressed() {
            state.swallowed.remove(&identity);
            if state.owner == Some(identity) {
                if let Some(position) = position {
                    release(world, position);
                } else {
                    cancel(world);
                }
                state.owner = None;
            }
        }
        return true;
    }
    if state.owner.is_some() && event.state.is_pressed() {
        state.swallowed.insert(identity);
        return true;
    }
    if !event.state.is_pressed() {
        return false;
    }
    let binding = crate::native::shortcuts::pointer_binding(world, event.button, modifiers);
    let Some(binding) = binding else {
        return false;
    };
    if crate::native::canvas_pan::dragged(world).is_some()
        || world
            .get_resource::<crate::native::topology::input::PointerState>()
            .is_some_and(|state| state.drag.is_some())
    {
        return false;
    }
    let Some((root, position)) = root(world, event.window).zip(position) else {
        return false;
    };
    if !press(world, root, event.window, position, binding.operation, None) {
        return false;
    }
    state.owner = Some(identity);
    state.swallowed.insert(identity);
    true
}

pub(crate) fn input(world: &mut World) {
    let _input = engine::performance::Span::new("native-manipulation-input");
    world.init_resource::<Native>();
    let mut state = world.remove_resource::<Native>().unwrap();
    state.keys.clear();
    if !super::super::active(world) {
        state.owner = None;
    }
    let mut batch = std::mem::take(&mut state.batch);
    batch.read(world, &mut state.windows, &mut state.raw);
    batch
        .decisions
        .swallow(state.swallowed.iter().map(|(_, button)| *button));
    for event in batch.events.drain(..) {
        match event {
            batch::Event::Motion(window, position) => {
                if state.owner.is_some_and(|(owner, _)| owner == window) && !motion(world, position)
                {
                    state.owner = None;
                }
            }
            batch::Event::Button(event) => {
                let consumed = mouse(world, &mut state, &event);
                batch.decisions.push(event, consumed);
            }
            batch::Event::FocusLost(window) => {
                if state.owner.is_some_and(|(owner, _)| owner == window) {
                    cancel(world);
                    state.owner = None;
                }
            }
            batch::Event::Escape(event, modifiers) => {
                let modifiers = modifiers
                    .or_else(|| crate::native::keyboard::current(world))
                    .or_else(|| {
                        world
                            .get_resource::<ButtonInput<KeyCode>>()
                            .map(Modifiers::pressed)
                    });
                let mut consumed = state.escapes.contains(&event.window);
                if event.state.is_pressed()
                    && modifiers == Some(Modifiers::NONE)
                    && super::super::active(world)
                {
                    cancel(world);
                    state.owner = None;
                    state.escapes.insert(event.window);
                    consumed = true;
                }
                if !event.state.is_pressed() {
                    state.escapes.remove(&event.window);
                }
                state.keys.push((event, consumed));
            }
        }
    }
    batch
        .decisions
        .swallow(state.swallowed.iter().map(|(_, button)| *button));
    if batch.decisions.any() {
        let _rewrite = engine::performance::Span::new("native-manipulation-rewrite");
        if let Some(mut buttons) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
            batch.decisions.replay(&mut buttons);
        }
        if let Some(mut messages) = world.get_resource_mut::<Messages<MouseButtonInput>>() {
            messages.clear();
            messages.write_batch(batch.decisions.forwarded());
            state.raw.clear(&messages);
        }
        if let Some(mut messages) = world.get_resource_mut::<Messages<PointerInput>>() {
            batch.decisions.index();
            let mut positions = [0; 2];
            batch.pointers.extend(
                state
                    .pointers
                    .read(&messages)
                    .filter(|event| batch.decisions.keep(event, &mut positions))
                    .cloned(),
            );
            engine::performance::add(
                "native-manipulation-pointer-retained",
                batch.pointers.len() as u64,
            );
            messages.clear();
            messages.write_batch(batch.pointers.drain(..));
            state.pointers.clear(&messages);
        }
    } else if let Some(messages) = world.get_resource::<Messages<PointerInput>>() {
        state.pointers.clear(messages);
    }
    state.batch = batch;
    world.insert_resource(state);
}

pub(in crate::native) fn keyboard(
    world: &mut World,
    key: &crate::native::keyboard::PreparedKey,
) -> bool {
    if key.input.key_code != KeyCode::Escape {
        return false;
    }
    if let Some(mut state) = world.get_resource_mut::<Native>()
        && let Some(index) = state.keys.iter().position(|(input, _)| input == &key.input)
    {
        return state.keys.remove(index).1;
    }
    if key.input.state.is_pressed()
        && key.modifiers == Modifiers::NONE
        && super::super::active(world)
    {
        cancel(world);
        return true;
    }
    false
}

#[cfg(test)]
mod tests;
