// non-lean-yet
// Scope: struct CapturePointer, fn button, fn capture.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (79).

use super::*;
use bevy::{
    input::mouse::MouseButtonInput,
    picking::pointer::{PointerAction, PointerButton, PointerInput},
};

#[derive(Default)]
pub(super) struct CapturePointer {
    cursor: MessageCursor<MouseButtonInput>,
    inputs: MessageCursor<PointerInput>,
    swallowed: HashSet<MouseButton>,
    events: Vec<MouseButtonInput>,
    affected: HashSet<MouseButton>,
    retained_inputs: Vec<PointerInput>,
}

impl CapturePointer {
    pub fn clear(&mut self) {
        self.swallowed.clear();
        self.events.clear();
        self.affected.clear();
        self.retained_inputs.clear();
    }
}

fn button(value: PointerButton) -> Option<MouseButton> {
    match value {
        PointerButton::Primary => Some(MouseButton::Left),
        PointerButton::Secondary => Some(MouseButton::Right),
        PointerButton::Middle => Some(MouseButton::Middle),
    }
}

pub(super) fn capture(world: &mut World, editor: &mut Editor) {
    let _capture = engine::performance::Span::new("shortcut-pointer-capture");
    let Some(messages) = world.get_resource::<Messages<MouseButtonInput>>() else {
        return;
    };
    let capturing = editor.modal.as_ref().is_some_and(|modal| {
        modal
            .capture
            .as_ref()
            .is_some_and(|capture| capture.shortcut.pointer_action().is_some())
    });
    if !capturing && editor.pointer.swallowed.is_empty() {
        editor.pointer.cursor.clear(messages);
        if let Some(messages) = world.get_resource::<Messages<PointerInput>>() {
            editor.pointer.inputs.clear(messages);
        }
        return;
    }
    let mut events = std::mem::take(&mut editor.pointer.events);
    events.clear();
    events.extend(editor.pointer.cursor.read(messages).cloned());
    engine::performance::add("shortcut-pointer-events-prepared", events.len() as u64);
    let mut swallowed = std::mem::take(&mut editor.pointer.affected);
    swallowed.clear();
    events.retain(|event| {
        if editor.pointer.swallowed.contains(&event.button) {
            swallowed.insert(event.button);
            if !event.state.is_pressed() {
                editor.pointer.swallowed.remove(&event.button);
            }
            return false;
        }
        let pointer = editor.modal.as_ref().is_some_and(|modal| {
            modal
                .capture
                .as_ref()
                .is_some_and(|capture| capture.shortcut.pointer_action().is_some())
        });
        if !pointer || !event.state.is_pressed() {
            return true;
        }
        let modifiers = crate::native::keyboard::pointer_modifiers(world, event);
        if !modifiers.control && !modifiers.alt && !modifiers.super_key {
            return true;
        }
        let modal = editor.modal.as_mut().unwrap();
        let value = super::super::PointerChord::from_input(event.button, modifiers)
            .map(|chord| modal.draft.captured(chord));
        captured(modal, value);
        swallowed.insert(event.button);
        editor.pointer.swallowed.insert(event.button);
        false
    });
    swallowed.extend(editor.pointer.swallowed.iter().copied());
    if swallowed.is_empty() {
        if let Some(messages) = world.get_resource::<Messages<PointerInput>>() {
            editor.pointer.inputs.clear(messages);
        }
    } else {
        if let Some(mut buttons) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
            for button in &swallowed {
                buttons.reset(*button);
            }
        }
        if let Some(mut messages) = world.get_resource_mut::<Messages<MouseButtonInput>>() {
            messages.clear();
            messages.write_batch(events.drain(..));
            editor.pointer.cursor.clear(&messages);
        }
        if let Some(mut messages) = world.get_resource_mut::<Messages<PointerInput>>() {
            editor.pointer.retained_inputs.clear();
            editor.pointer.retained_inputs.extend(
                editor
                    .pointer
                    .inputs
                    .read(&messages)
                    .filter(|event| match event.action {
                        PointerAction::Press(value) | PointerAction::Release(value) => {
                            button(value).is_none_or(|value| !swallowed.contains(&value))
                        }
                        _ => true,
                    })
                    .cloned(),
            );
            messages.clear();
            messages.write_batch(editor.pointer.retained_inputs.drain(..));
            editor.pointer.inputs.clear(&messages);
        }
    }
    events.clear();
    editor.pointer.events = events;
    editor.pointer.affected = swallowed;
}
