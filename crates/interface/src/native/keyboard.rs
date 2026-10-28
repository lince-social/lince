use super::actions::Modifiers;
use crate::host::input::KeyboardSnapshot;
use bevy::{
    ecs::message::MessageCursor,
    input::{
        InputSystems,
        keyboard::{Key, KeyboardFocusLost, KeyboardInput},
    },
    prelude::*,
    window::PrimaryWindow,
};
use std::collections::{BTreeMap, HashMap, HashSet};

mod host;
mod pointer;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Prepare;

#[derive(Clone)]
pub(crate) struct PreparedKey {
    pub input: KeyboardInput,
    pub modifiers: Modifiers,
    pub modifier: bool,
    pub after: Modifiers,
    pub roles: Modifiers,
    pub held: Vec<KeyCode>,
}

#[derive(Resource)]
pub(crate) struct Dispatch {
    pub window: Entity,
    pub modifiers: Modifiers,
    pub held: Vec<KeyCode>,
}

#[derive(Resource, Default)]
struct State {
    events: Vec<PreparedKey>,
    held: HashMap<Entity, BTreeMap<KeyCode, Key>>,
    seeded: HashSet<(Entity, KeyCode)>,
    active: Option<Entity>,
    ordered_loss: bool,
    cursor: MessageCursor<KeyboardInput>,
    focus: MessageCursor<KeyboardFocusLost>,
    host: host::Host,
    pointer: pointer::State,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<State>()
        .add_systems(PreUpdate, prepare.in_set(Prepare).after(InputSystems));
}

pub fn cancel(world: &mut World) {
    if let Some(mut events) = world.get_resource_mut::<Messages<KeyboardInput>>() {
        events.clear();
    }
    world.write_message(KeyboardFocusLost);
    if let Some(mut keys) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
        keys.reset_all();
    }
    if let Some(mut keys) = world.get_resource_mut::<ButtonInput<Key>>() {
        keys.reset_all();
    }
    let Some(mut state) = world.remove_resource::<State>() else {
        return;
    };
    state.host.prepare(world);
    discard(world, &mut state);
    if let Some(events) = world.get_resource::<Messages<KeyboardFocusLost>>() {
        state.focus.clear(events);
    }
    if let Some(events) = world.get_resource::<Messages<KeyboardInput>>() {
        state.cursor.clear(events);
    }
    state.ordered_loss = true;
    world.insert_resource(state);
}

pub fn synchronize_modifiers(world: &mut World, window: Entity, pressed: &[(KeyCode, Key)]) {
    if !world
        .get::<Window>(window)
        .is_some_and(|window| window.focused)
    {
        return;
    }
    let keyboard = world.get_resource::<KeyboardSnapshot>();
    let state = world.get_resource::<State>();
    let mut held = state
        .and_then(|state| state.held.get(&window))
        .cloned()
        .unwrap_or_default();
    if let Some(messages) = world.get_resource::<Messages<KeyboardInput>>() {
        let mut cursor = state.map(|state| state.cursor.clone()).unwrap_or_default();
        for event in cursor.read(messages).filter(|event| event.window == window) {
            if event.state.is_pressed() {
                held.insert(event.key_code, event.logical_key.clone());
            } else {
                held.remove(&event.key_code);
            }
        }
    }
    let pressed: BTreeMap<_, _> = pressed
        .iter()
        .filter(|(code, key)| role(*code, key, keyboard).1)
        .cloned()
        .collect();
    let mut changes = held
        .iter()
        .filter(|(code, key)| role(**code, key, keyboard).1 && !pressed.contains_key(code))
        .map(|(code, key)| (*code, key.clone(), bevy::input::ButtonState::Released))
        .collect::<Vec<_>>();
    changes.extend(
        pressed
            .into_iter()
            .filter(|(code, key)| held.get(code) != Some(key))
            .map(|(code, key)| (code, key, bevy::input::ButtonState::Pressed)),
    );
    for (key_code, logical_key, state) in changes {
        let event = KeyboardInput {
            window,
            key_code,
            logical_key,
            state,
            text: None,
            repeat: false,
        };
        if let Some(mut messages) = world.get_resource_mut::<Messages<bevy::window::WindowEvent>>()
        {
            messages.write(bevy::window::WindowEvent::KeyboardInput(event.clone()));
        }
        world.write_message(event);
    }
}

fn discard(world: &mut World, state: &mut State) {
    state.events.clear();
    state.held.clear();
    state.seeded.clear();
    state.active = None;
    state.ordered_loss = false;
    state.host.clear();
    state.pointer.clear(world);
    if let Some(mut keys) = world.get_resource_mut::<ButtonInput<Key>>() {
        keys.reset_all();
    }
}

pub(crate) fn take(world: &mut World) -> Vec<PreparedKey> {
    world
        .get_resource_mut::<State>()
        .map(|mut state| std::mem::take(&mut state.events))
        .unwrap_or_default()
}

pub(crate) fn replace(world: &mut World, events: Vec<PreparedKey>) {
    if let Some(mut state) = world.get_resource_mut::<State>() {
        state.events = events;
    }
}

pub(crate) fn current(world: &World) -> Option<Modifiers> {
    let state = world.get_resource::<State>()?;
    aggregate(
        state,
        state.active?,
        world.get_resource::<KeyboardSnapshot>(),
    )
}

pub(crate) fn pointer_modifiers(
    world: &mut World,
    event: &bevy::input::mouse::MouseButtonInput,
) -> Modifiers {
    pointer_context(world, event).0
}

pub(crate) fn pending(world: &World) -> &[PreparedKey] {
    world
        .get_resource::<State>()
        .map_or(&[], |state| state.events.as_slice())
}

pub(crate) fn pointer_context(
    world: &mut World,
    event: &bevy::input::mouse::MouseButtonInput,
) -> (Modifiers, Option<Vec2>) {
    let context = world
        .get_resource_mut::<State>()
        .and_then(|mut state| state.pointer.event(event));
    let modifiers = context
        .map(|context| context.0)
        .or_else(|| current(world))
        .or_else(|| {
            world
                .get_resource::<ButtonInput<KeyCode>>()
                .map(Modifiers::pressed)
        })
        .unwrap_or_default();
    let point = context.and_then(|context| context.1).or_else(|| {
        world
            .get::<Window>(event.window)
            .and_then(Window::cursor_position)
    });
    (modifiers, point)
}

pub(crate) fn logical(world: &mut World, modifiers: Modifiers) {
    let Some(mut keys) = world.get_resource_mut::<ButtonInput<Key>>() else {
        return;
    };
    for (key, pressed) in [
        (Key::Control, modifiers.control),
        (Key::Alt, modifiers.alt),
        (Key::Shift, modifiers.shift),
        (Key::Super, modifiers.super_key),
    ] {
        if pressed {
            keys.press(key);
        } else {
            keys.release(key);
        }
    }
}

fn role(key: KeyCode, logical: &Key, keyboard: Option<&KeyboardSnapshot>) -> (Modifiers, bool) {
    if let Some(keyboard) = keyboard.filter(|keyboard| !keyboard.labels.is_empty()) {
        return keyboard.labels.get(&format!("{key:?}")).map_or(
            (Modifiers::NONE, false),
            |label| {
                (
                    Modifiers {
                        control: label.modifiers.control,
                        alt: label.modifiers.alt,
                        shift: label.modifiers.shift,
                        super_key: label.modifiers.super_key,
                    },
                    label.is_modifier,
                )
            },
        );
    }
    let mut modifiers = Modifiers::NONE;
    let modifier = match logical {
        Key::Control => {
            modifiers.control = true;
            true
        }
        Key::Alt => {
            modifiers.alt = true;
            true
        }
        Key::Shift => {
            modifiers.shift = true;
            true
        }
        Key::Super | Key::Meta => {
            modifiers.super_key = true;
            true
        }
        Key::AltGraph | Key::Hyper | Key::Fn | Key::FnLock | Key::Symbol | Key::SymbolLock => true,
        Key::Unidentified(_) => {
            modifiers.control = matches!(key, KeyCode::ControlLeft | KeyCode::ControlRight);
            modifiers.alt = matches!(key, KeyCode::AltLeft | KeyCode::AltRight);
            modifiers.shift = matches!(key, KeyCode::ShiftLeft | KeyCode::ShiftRight);
            modifiers.super_key = matches!(key, KeyCode::SuperLeft | KeyCode::SuperRight);
            modifiers != Modifiers::NONE
        }
        _ => false,
    };
    (modifiers, modifier)
}

fn modifiers(held: &BTreeMap<KeyCode, Key>, keyboard: Option<&KeyboardSnapshot>) -> Modifiers {
    held.iter()
        .fold(Modifiers::NONE, |mut result, (key, logical)| {
            let (value, _) = role(*key, logical, keyboard);
            result.control |= value.control;
            result.alt |= value.alt;
            result.shift |= value.shift;
            result.super_key |= value.super_key;
            result
        })
}

fn aggregate(
    state: &State,
    window: Entity,
    keyboard: Option<&KeyboardSnapshot>,
) -> Option<Modifiers> {
    if keyboard.is_none_or(|keyboard| keyboard.labels.is_empty())
        && let Some(modifiers) = state.host.current(window)
    {
        return Some(modifiers);
    }
    state
        .held
        .get(&window)
        .map(|held| modifiers(held, keyboard))
}

fn prepare(world: &mut World) {
    let mut state = world.remove_resource::<State>().unwrap_or_default();
    state.events.clear();
    state.host.prepare(world);
    let lost = world
        .get_resource::<Messages<KeyboardFocusLost>>()
        .is_some_and(|events| state.focus.read(events).count() > 0);
    let events: Vec<_> = world
        .get_resource::<Messages<KeyboardInput>>()
        .map(|messages| state.cursor.read(messages).cloned().collect())
        .unwrap_or_default();
    state.held.retain(|entity, _| {
        world.get_entity(*entity).is_ok()
            && world
                .get::<Window>(*entity)
                .is_none_or(|window| window.focused)
    });
    if lost {
        discard(world, &mut state);
        world.insert_resource(state);
        return;
    }
    let window = state
        .host
        .active()
        .or_else(|| {
            events
                .iter()
                .find(|event| {
                    world
                        .get::<Window>(event.window)
                        .is_none_or(|window| window.focused)
                })
                .map(|event| event.window)
        })
        .or_else(|| {
            world
                .query_filtered::<(Entity, &Window), With<PrimaryWindow>>()
                .single(world)
                .ok()
                .filter(|(_, window)| window.focused)
                .map(|(entity, _)| entity)
        });
    if let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() {
        state.seeded.retain(|(window, key)| {
            if world.get_entity(*window).is_err()
                || world
                    .get::<Window>(*window)
                    .is_some_and(|window| !window.focused)
            {
                return false;
            }
            if keys.pressed(*key) {
                return true;
            }
            if let Some(held) = state.held.get_mut(window) {
                held.remove(key);
            }
            false
        });
        if let Some(window) = window
            && state.host.current(window).is_none()
        {
            let mentioned: HashSet<_> = events.iter().map(|event| event.key_code).collect();
            for key in keys.get_pressed().filter(|key| !mentioned.contains(key)) {
                let held = state.held.entry(window).or_default();
                if !held.contains_key(key) {
                    held.insert(
                        *key,
                        Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                    );
                    state.seeded.insert((window, *key));
                }
            }
        }
    }
    let keyboard = world.get_resource::<KeyboardSnapshot>();
    let pointer_before = state
        .held
        .iter()
        .map(|(window, held)| (*window, modifiers(held, keyboard)))
        .collect();
    for event in events {
        if world.get_entity(event.window).is_err() {
            continue;
        }
        state.seeded.remove(&(event.window, event.key_code));
        let held = state.held.entry(event.window).or_default();
        let before = modifiers(held, keyboard);
        let focused = world
            .get::<Window>(event.window)
            .is_none_or(|window| window.focused);
        if !focused {
            held.clear();
        } else if event.state.is_pressed() {
            held.insert(event.key_code, event.logical_key.clone());
        } else {
            held.remove(&event.key_code);
        }
        let fallback = modifiers(held, keyboard);
        let host = state.host.event(&event);
        let modifiers = if keyboard.is_some_and(|keyboard| !keyboard.labels.is_empty()) {
            fallback
        } else {
            host.unwrap_or(fallback)
        };
        let (roles, modifier) = role(event.key_code, &event.logical_key, keyboard);
        let after = if keyboard.is_some_and(|keyboard| !keyboard.labels.is_empty()) {
            fallback
        } else {
            let inherited = host.unwrap_or_default().difference(before);
            let inherited = if event.state.is_pressed() {
                inherited
            } else {
                inherited.difference(roles)
            };
            fallback.union(inherited.intersection(state.host.after().or(host).unwrap_or_default()))
        };
        if focused {
            state.active = Some(event.window);
        }
        state.events.push(PreparedKey {
            input: event,
            modifiers,
            modifier,
            after,
            roles,
            held: held.keys().copied().collect(),
        });
    }
    if state.active.is_none_or(|entity| {
        world.get_entity(entity).is_err()
            || world
                .get::<Window>(entity)
                .is_some_and(|window| !window.focused)
    }) {
        state.active = window;
    }
    let final_modifiers = state
        .active
        .and_then(|active| aggregate(&state, active, keyboard))
        .unwrap_or_default();
    if state.ordered_loss {
        if let Some(held) = state.active.and_then(|active| state.held.get(&active))
            && let Some(mut keys) = world.get_resource_mut::<ButtonInput<KeyCode>>()
        {
            for key in held.keys() {
                keys.clear_just_released(*key);
                keys.press(*key);
            }
        }
        state.ordered_loss = false;
    }
    pointer::prepare(world, &mut state, pointer_before);
    logical(world, final_modifiers);
    world.insert_resource(state);
}

#[cfg(test)]
mod tests;
