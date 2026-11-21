// non-lean-yet
// Scope: struct State, fn prepare, mod tests.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 85.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   53, 58.

use super::*;
use bevy::{input::mouse::MouseButtonInput, window::WindowEvent};
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct State {
    cursor: MessageCursor<WindowEvent>,
    events: VecDeque<(MouseButtonInput, Modifiers, Option<Vec2>)>,
    positions: HashMap<Entity, Vec2>,
    observed: HashSet<Entity>,
}

impl State {
    pub fn event(&mut self, input: &MouseButtonInput) -> Option<(Modifiers, Option<Vec2>)> {
        let index = self
            .events
            .iter()
            .position(|(event, _, _)| event == input)?;
        self.events
            .drain(..=index)
            .last()
            .map(|(_, modifiers, position)| (modifiers, position))
    }

    pub fn clear(&mut self, world: &World) {
        self.events.clear();
        self.positions.clear();
        self.observed.clear();
        if let Some(events) = world.get_resource::<Messages<WindowEvent>>() {
            self.cursor.clear(events);
        }
    }
}

pub(super) fn prepare(
    world: &World,
    state: &mut super::State,
    mut masks: HashMap<Entity, Modifiers>,
) -> HashMap<Entity, Modifiers> {
    let _preparation = engine::performance::Span::new("keyboard-pointer-preparation");
    state.pointer.events.clear();
    state.pointer.observed.clear();
    let Some(events) = world.get_resource::<Messages<WindowEvent>>() else {
        return masks;
    };
    let mut next_key = 0;
    for event in state.pointer.cursor.read(events) {
        match event {
            WindowEvent::CursorMoved(event) => {
                state.pointer.positions.insert(event.window, event.position);
            }
            WindowEvent::KeyboardInput(input) => {
                if let Some(index) = state.events[next_key..]
                    .iter()
                    .position(|event| event.input == *input)
                {
                    next_key += index;
                    let event = &state.events[next_key];
                    masks.insert(input.window, event.after);
                    state.pointer.observed.insert(input.window);
                    next_key += 1;
                }
            }
            WindowEvent::MouseButtonInput(input) => {
                let host = state.host.button(input);
                let modifiers = if state.pointer.observed.contains(&input.window) {
                    masks.get(&input.window).copied()
                } else {
                    host.or_else(|| masks.get(&input.window).copied())
                }
                .unwrap_or_default();
                let position = state
                    .pointer
                    .positions
                    .get(&input.window)
                    .copied()
                    .or_else(|| {
                        world
                            .get::<Window>(input.window)
                            .and_then(Window::cursor_position)
                    });
                state
                    .pointer
                    .events
                    .push_back((input.clone(), modifiers, position));
            }
            _ => {}
        }
    }
    masks
}

#[cfg(test)]
mod tests;
