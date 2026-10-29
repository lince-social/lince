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
use std::collections::{HashMap, HashSet};

#[derive(Resource, Default)]
struct Native {
    windows: MessageCursor<WindowEvent>,
    raw: MessageCursor<MouseButtonInput>,
    pointers: MessageCursor<PointerInput>,
    owner: Option<(Entity, MouseButton)>,
    swallowed: HashSet<(Entity, MouseButton)>,
    escapes: HashSet<Entity>,
    keys: Vec<(KeyboardInput, bool)>,
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
    let binding = crate::native::shortcuts::pointer_bindings(world)
        .into_iter()
        .find(|binding| {
            binding.chord.button == event.button && binding.chord.modifiers == modifiers
        });
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
    world.init_resource::<Native>();
    let mut state = world.remove_resource::<Native>().unwrap();
    state.keys.clear();
    if !super::super::active(world) {
        state.owner = None;
    }
    let mut raw: Vec<_> = world
        .get_resource::<Messages<MouseButtonInput>>()
        .map(|messages| state.raw.read(messages).cloned().collect())
        .unwrap_or_default();
    let events: Vec<_> = world
        .get_resource::<Messages<WindowEvent>>()
        .map(|messages| state.windows.read(messages).cloned().collect())
        .unwrap_or_default();
    let keys = crate::native::keyboard::pending(world).to_vec();
    let mut next_key = 0;
    let initial = state.swallowed.clone();
    let mut decisions = Vec::new();
    for event in events {
        match event {
            WindowEvent::CursorMoved(event) => {
                if state
                    .owner
                    .is_some_and(|(window, _)| window == event.window)
                    && !motion(world, event.position)
                {
                    state.owner = None;
                }
            }
            WindowEvent::MouseButtonInput(event) => {
                if let Some(index) = raw.iter().position(|input| input == &event) {
                    raw.remove(index);
                    let consumed = mouse(world, &mut state, &event);
                    decisions.push((event, consumed));
                }
            }
            WindowEvent::WindowFocused(event) if !event.focused => {
                if state
                    .owner
                    .is_some_and(|(window, _)| window == event.window)
                {
                    cancel(world);
                    state.owner = None;
                }
            }
            WindowEvent::KeyboardInput(event) => {
                let modifiers = keys[next_key..]
                    .iter()
                    .position(|key| key.input == event)
                    .map(|index| {
                        next_key += index;
                        let value = keys[next_key].modifiers;
                        next_key += 1;
                        value
                    })
                    .or_else(|| crate::native::keyboard::current(world))
                    .or_else(|| {
                        world
                            .get_resource::<ButtonInput<KeyCode>>()
                            .map(Modifiers::pressed)
                    });
                if event.key_code == KeyCode::Escape {
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
            _ => {}
        }
    }
    for event in raw {
        let consumed = mouse(world, &mut state, &event);
        decisions.push((event, consumed));
    }
    let consumed: HashSet<_> = decisions
        .iter()
        .filter(|(_, consumed)| *consumed)
        .map(|(event, _)| event.button)
        .chain(initial.iter().map(|(_, button)| *button))
        .chain(state.swallowed.iter().map(|(_, button)| *button))
        .collect();
    if !consumed.is_empty() {
        if let Some(mut buttons) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
            for button in &consumed {
                buttons.reset(*button);
                let last = decisions
                    .iter()
                    .rposition(|(event, consumed)| event.button == *button && *consumed);
                for (event, swallowed) in decisions
                    .iter()
                    .skip(last.map_or(0, |index| index + 1))
                    .filter(|(event, _)| event.button == *button)
                {
                    if !swallowed {
                        if event.state.is_pressed() {
                            buttons.press(*button);
                        } else {
                            buttons.release(*button);
                        }
                    }
                }
            }
        }
        if let Some(mut messages) = world.get_resource_mut::<Messages<MouseButtonInput>>() {
            messages.clear();
            messages.write_batch(
                decisions
                    .iter()
                    .filter(|(_, consumed)| !consumed)
                    .map(|(event, _)| event.clone()),
            );
            state.raw.clear(&messages);
        }
        if let Some(mut messages) = world.get_resource_mut::<Messages<PointerInput>>() {
            let mut positions: HashMap<PointerId, usize> = HashMap::new();
            let retained: Vec<_> = state
                .pointers
                .read(&messages)
                .filter(|event| {
                    if event.pointer_id != PointerId::Mouse
                        && event.pointer_id != crate::native::topology::input::CONTENT_POINTER
                    {
                        return true;
                    }
                    let (button, pressed) = match event.action {
                        PointerAction::Press(button) => (button, true),
                        PointerAction::Release(button) => (button, false),
                        _ => return true,
                    };
                    let button = self::button(button);
                    let cursor = positions.entry(event.pointer_id).or_default();
                    if let Some(index) = decisions[*cursor..].iter().position(|(input, _)| {
                        input.button == button && input.state.is_pressed() == pressed
                    }) {
                        *cursor += index;
                        let consumed = decisions[*cursor].1;
                        *cursor += 1;
                        !consumed
                    } else {
                        !consumed.contains(&button)
                    }
                })
                .cloned()
                .collect();
            messages.clear();
            messages.write_batch(retained);
            state.pointers.clear(&messages);
        }
    } else if let Some(messages) = world.get_resource::<Messages<PointerInput>>() {
        state.pointers.clear(messages);
    }
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
