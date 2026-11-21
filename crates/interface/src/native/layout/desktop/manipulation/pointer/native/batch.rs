// non-lean-yet
// Scope: enum Event, struct Copies, struct Batch, fn identity.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 42, 67, 85, 100,
//   163.
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   push:consumed (128).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   73, 78, 198.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (96).

use super::*;
use std::collections::HashMap;

pub(super) enum Event {
    Motion(Entity, Vec2),
    Button(MouseButtonInput),
    FocusLost(Entity),
    Escape(KeyboardInput, Option<Modifiers>),
}

#[derive(Default)]
struct Copies {
    available: usize,
    matched: usize,
}

#[derive(Default)]
pub(super) struct Batch {
    pub events: Vec<Event>,
    copies: HashMap<(Entity, MouseButton, bool), Copies>,
    pub decisions: Decisions,
    pub pointers: Vec<PointerInput>,
}

fn identity(input: &MouseButtonInput) -> (Entity, MouseButton, bool) {
    (input.window, input.button, input.state.is_pressed())
}

impl Batch {
    pub fn read(
        &mut self,
        world: &World,
        windows: &mut MessageCursor<WindowEvent>,
        buttons: &mut MessageCursor<MouseButtonInput>,
    ) {
        let _preparation = engine::performance::Span::new("native-manipulation-batch");
        self.events.clear();
        self.copies.clear();
        self.decisions.clear();
        self.pointers.clear();
        let raw = world.get_resource::<Messages<MouseButtonInput>>();
        let mut unmatched = buttons.clone();
        let mut raw_count = 0;
        if let Some(messages) = raw {
            for event in buttons.read(messages) {
                self.copies.entry(identity(event)).or_default().available += 1;
                raw_count += 1;
            }
        }
        let keys = crate::native::keyboard::pending(world);
        let mut next_key = 0;
        let mut observed = 0;
        if let Some(messages) = world.get_resource::<Messages<WindowEvent>>() {
            for event in windows.read(messages) {
                observed += 1;
                let event = match event {
                    WindowEvent::CursorMoved(event) => Event::Motion(event.window, event.position),
                    WindowEvent::MouseButtonInput(event) => {
                        let Some(copies) = self.copies.get_mut(&identity(event)) else {
                            continue;
                        };
                        if copies.available == 0 {
                            continue;
                        }
                        copies.available -= 1;
                        copies.matched += 1;
                        Event::Button(event.clone())
                    }
                    WindowEvent::WindowFocused(event) if !event.focused => {
                        Event::FocusLost(event.window)
                    }
                    WindowEvent::KeyboardInput(event) => {
                        let modifiers = keys[next_key..]
                            .iter()
                            .position(|key| key.input == *event)
                            .map(|index| {
                                next_key += index;
                                let modifiers = keys[next_key].modifiers;
                                next_key += 1;
                                modifiers
                            });
                        if event.key_code != KeyCode::Escape {
                            continue;
                        }
                        Event::Escape(event.clone(), modifiers)
                    }
                    _ => continue,
                };
                self.events.push(event);
            }
        }
        engine::performance::add("native-manipulation-window-events", observed);
        engine::performance::add("native-manipulation-button-events", raw_count);
        if let Some(messages) = raw {
            for event in unmatched.read(messages) {
                let copies = self.copies.get_mut(&identity(event)).unwrap();
                if copies.matched > 0 {
                    copies.matched -= 1;
                } else {
                    self.events.push(Event::Button(event.clone()));
                }
            }
        }
        self.copies.clear();
        engine::performance::add("native-manipulation-commands", self.events.len() as u64);
    }
}

#[derive(Default)]
pub(super) struct Decisions {
    events: Vec<(MouseButtonInput, bool)>,
    affected: HashMap<MouseButton, Option<usize>>,
    matching: HashMap<(MouseButton, bool), Vec<usize>>,
}

impl Decisions {
    fn clear(&mut self) {
        self.events.clear();
        self.affected.clear();
    }

    pub fn swallow(&mut self, buttons: impl Iterator<Item = MouseButton>) {
        for button in buttons {
            self.affected.entry(button).or_default();
        }
    }

    pub fn push(&mut self, event: MouseButtonInput, consumed: bool) {
        if consumed {
            self.affected.insert(event.button, Some(self.events.len()));
        }
        self.events.push((event, consumed));
    }

    pub fn any(&self) -> bool {
        !self.affected.is_empty()
    }

    pub fn replay(&self, buttons: &mut ButtonInput<MouseButton>) {
        for button in self.affected.keys() {
            buttons.reset(*button);
        }
        for (index, (event, consumed)) in self.events.iter().enumerate() {
            if !consumed
                && self
                    .affected
                    .get(&event.button)
                    .is_some_and(|last| last.is_none_or(|last| index > last))
            {
                if event.state.is_pressed() {
                    buttons.press(event.button);
                } else {
                    buttons.release(event.button);
                }
            }
        }
    }

    pub fn forwarded(&self) -> impl Iterator<Item = MouseButtonInput> + '_ {
        self.events
            .iter()
            .filter(|(_, consumed)| !consumed)
            .map(|(event, _)| event.clone())
    }

    pub fn index(&mut self) {
        for indices in self.matching.values_mut() {
            indices.clear();
        }
        for (index, (event, _)) in self.events.iter().enumerate() {
            self.matching
                .entry((event.button, event.state.is_pressed()))
                .or_default()
                .push(index);
        }
    }

    pub fn keep(&self, event: &PointerInput, positions: &mut [usize; 2]) -> bool {
        let cursor = if event.pointer_id == PointerId::Mouse {
            &mut positions[0]
        } else if event.pointer_id == crate::native::topology::input::CONTENT_POINTER {
            &mut positions[1]
        } else {
            return true;
        };
        let (button, pressed) = match event.action {
            PointerAction::Press(button) => (super::button(button), true),
            PointerAction::Release(button) => (super::button(button), false),
            _ => return true,
        };
        let index = self.matching.get(&(button, pressed)).and_then(|indices| {
            indices
                .get(indices.partition_point(|index| *index < *cursor))
                .copied()
        });
        if let Some(index) = index {
            *cursor = index + 1;
            !self.events[index].1
        } else {
            !self.affected.contains_key(&button)
        }
    }
}
