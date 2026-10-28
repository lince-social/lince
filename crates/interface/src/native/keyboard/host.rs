use super::Modifiers;
use bevy::{
    ecs::message::MessageCursor,
    input::keyboard::KeyboardInput,
    prelude::*,
    winit::{RawWinitWindowEvent, WINIT_WINDOWS, converters},
};
use std::collections::{HashMap, VecDeque};
use winit::{event::WindowEvent, keyboard::ModifiersState};

#[derive(Default)]
pub(super) struct Host {
    cursor: MessageCursor<RawWinitWindowEvent>,
    masks: HashMap<Entity, Modifiers>,
    keys: VecDeque<(KeyboardInput, Option<Modifiers>, Option<Modifiers>)>,
    buttons: VecDeque<(bevy::input::mouse::MouseButtonInput, Option<Modifiers>)>,
    active: Option<Entity>,
    after: Option<Modifiers>,
}

impl Host {
    pub(super) fn prepare(&mut self, world: &World) {
        self.keys.clear();
        self.buttons.clear();
        self.masks
            .retain(|entity, _| world.get_entity(*entity).is_ok());
        if let Some(events) = world.get_resource::<Messages<RawWinitWindowEvent>>() {
            let mut cursor = std::mem::take(&mut self.cursor);
            for event in cursor.read(events) {
                let Some(window) = WINIT_WINDOWS
                    .with_borrow(|windows| windows.winit_to_entity.get(&event.window_id).copied())
                else {
                    continue;
                };
                match &event.event {
                    WindowEvent::ModifiersChanged(modifiers) => {
                        self.modifiers(window, modifiers.state());
                    }
                    WindowEvent::KeyboardInput {
                        event,
                        is_synthetic: false,
                        ..
                    } => self.keyboard(converters::convert_keyboard_input(event, window)),
                    WindowEvent::MouseInput { button, state, .. } => {
                        self.buttons.push_back((
                            bevy::input::mouse::MouseButtonInput {
                                window,
                                button: converters::convert_mouse_button(*button),
                                state: match state {
                                    winit::event::ElementState::Pressed => {
                                        bevy::input::ButtonState::Pressed
                                    }
                                    winit::event::ElementState::Released => {
                                        bevy::input::ButtonState::Released
                                    }
                                },
                            },
                            self.current(window),
                        ));
                    }
                    WindowEvent::Focused(true) => self.active = Some(window),
                    WindowEvent::Focused(false) | WindowEvent::Destroyed => self.remove(window),
                    _ => {}
                }
            }
            self.cursor = cursor;
        }
        self.masks.retain(|entity, _| {
            world
                .get::<Window>(*entity)
                .is_some_and(|window| window.focused)
        });
        self.active = self.active.filter(|entity| {
            world
                .get::<Window>(*entity)
                .is_some_and(|window| window.focused)
        });
    }

    fn modifiers(&mut self, window: Entity, state: ModifiersState) {
        self.masks.insert(
            window,
            Modifiers {
                control: state.control_key(),
                alt: state.alt_key(),
                shift: state.shift_key(),
                super_key: state.super_key(),
            },
        );
        self.active = Some(window);
        if let Some((input, _, after)) = self.keys.back_mut()
            && input.window == window
        {
            *after = self.masks.get(&window).copied();
        }
    }

    fn keyboard(&mut self, input: KeyboardInput) {
        let modifiers = self.current(input.window);
        self.keys.push_back((input, modifiers, None));
    }

    pub(super) fn event(&mut self, input: &KeyboardInput) -> Option<Modifiers> {
        self.after = None;
        if let Some(index) = self.keys.iter().position(|(key, _, _)| key == input) {
            if let Some((_, modifiers, after)) = self.keys.drain(..=index).last() {
                self.after = after;
                return modifiers;
            }
        }
        self.current(input.window)
    }

    pub(super) fn after(&self) -> Option<Modifiers> {
        self.after
    }

    pub(super) fn button(
        &mut self,
        input: &bevy::input::mouse::MouseButtonInput,
    ) -> Option<Modifiers> {
        let index = self.buttons.iter().position(|(event, _)| event == input)?;
        self.buttons
            .drain(..=index)
            .last()
            .and_then(|(_, modifiers)| modifiers)
    }

    pub(super) fn current(&self, window: Entity) -> Option<Modifiers> {
        self.masks.get(&window).copied()
    }

    pub(super) fn active(&self) -> Option<Entity> {
        self.active
    }

    fn remove(&mut self, window: Entity) {
        self.masks.remove(&window);
        if self.active == Some(window) {
            self.active = None;
        }
    }

    pub(super) fn clear(&mut self) {
        self.masks.clear();
        self.keys.clear();
        self.buttons.clear();
        self.after = None;
        self.active = None;
    }
}

#[cfg(test)]
mod tests;
