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
}

impl CapturePointer {
    pub fn clear(&mut self) {
        self.swallowed.clear();
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
    let Some(messages) = world.get_resource::<Messages<MouseButtonInput>>() else {
        return;
    };
    let events: Vec<_> = editor.pointer.cursor.read(messages).cloned().collect();
    let mut swallowed = HashSet::new();
    let mut retained = Vec::new();
    for event in events {
        if editor.pointer.swallowed.contains(&event.button) {
            swallowed.insert(event.button);
            if !event.state.is_pressed() {
                editor.pointer.swallowed.remove(&event.button);
            }
            continue;
        }
        let pointer = editor.modal.as_ref().is_some_and(|modal| {
            modal
                .capture
                .as_ref()
                .is_some_and(|capture| capture.shortcut.pointer_action().is_some())
        });
        if !pointer || !event.state.is_pressed() {
            retained.push(event);
            continue;
        }
        let modifiers = crate::native::keyboard::pointer_modifiers(world, &event);
        if !modifiers.control && !modifiers.alt && !modifiers.super_key {
            retained.push(event);
            continue;
        }
        let modal = editor.modal.as_mut().unwrap();
        let value = super::super::PointerChord::from_input(event.button, modifiers)
            .map(|chord| modal.draft.captured(chord));
        captured(modal, value);
        swallowed.insert(event.button);
        editor.pointer.swallowed.insert(event.button);
    }
    swallowed.extend(editor.pointer.swallowed.iter().copied());
    if swallowed.is_empty() {
        if let Some(messages) = world.get_resource::<Messages<PointerInput>>() {
            editor.pointer.inputs.clear(messages);
        }
        return;
    }
    if let Some(mut buttons) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
        for button in &swallowed {
            buttons.reset(*button);
        }
    }
    if let Some(mut messages) = world.get_resource_mut::<Messages<MouseButtonInput>>() {
        messages.clear();
        messages.write_batch(retained);
        editor.pointer.cursor.clear(&messages);
    }
    if let Some(mut messages) = world.get_resource_mut::<Messages<PointerInput>>() {
        let retained: Vec<_> = editor
            .pointer
            .inputs
            .read(&messages)
            .filter(|event| match event.action {
                PointerAction::Press(value) | PointerAction::Release(value) => {
                    button(value).is_none_or(|value| !swallowed.contains(&value))
                }
                _ => true,
            })
            .cloned()
            .collect();
        messages.clear();
        messages.write_batch(retained);
        editor.pointer.inputs.clear(&messages);
    }
}
