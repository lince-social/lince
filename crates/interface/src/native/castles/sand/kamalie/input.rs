use super::{
    controls::{self, Action},
    host::{Game, Screen},
};
use bevy::{
    input::keyboard::KeyboardInput,
    input_focus::{FocusCause, FocusLost, FocusedInput, InputFocus},
    prelude::*,
};

pub(super) fn install(world: &mut World, screen: Entity) {
    world
        .entity_mut(screen)
        .observe(press)
        .observe(release)
        .observe(keyboard)
        .observe(lost);
}

pub(super) fn press_elsewhere(
    event: On<Pointer<Press>>,
    screens: Query<&Screen>,
    mut games: Query<&mut Game>,
    mut focus: ResMut<InputFocus>,
) {
    let Some(focused) = focus.get() else { return };
    let Ok(screen) = screens.get(focused) else {
        return;
    };
    if event.original_event_target() == focused {
        return;
    }
    if let Ok(mut game) = games.get_mut(screen.0) {
        game.input.clear();
        game.run.suspend();
    }
    if focus.get() == Some(focused) {
        focus.clear();
    }
}

fn press(
    mut event: On<Pointer<Press>>,
    screens: Query<&Screen>,
    mut games: Query<&mut Game>,
    mut focus: ResMut<InputFocus>,
) {
    let Ok(screen) = screens.get(event.entity) else {
        return;
    };
    if event.button != PointerButton::Primary {
        return;
    }
    if let Ok(mut game) = games.get_mut(screen.0) {
        let already_focused = focus.get() == Some(event.entity);
        game.input.activating = already_focused && !game.run.paused;
        focus.set(event.entity, FocusCause::Pressed);
        event.propagate(false);
    }
}

fn release(mut event: On<Pointer<Release>>, screens: Query<&Screen>, mut games: Query<&mut Game>) {
    let Ok(screen) = screens.get(event.entity) else {
        return;
    };
    if event.button == PointerButton::Primary
        && let Ok(mut game) = games.get_mut(screen.0)
    {
        game.input.activating = false;
        event.propagate(false);
    }
}

fn lost(event: On<FocusLost>, screens: Query<&Screen>, mut games: Query<&mut Game>) {
    if let Ok(screen) = screens.get(event.entity)
        && let Ok(mut game) = games.get_mut(screen.0)
    {
        game.input.clear();
        game.run.suspend();
    }
}

fn keyboard(
    mut event: On<FocusedInput<KeyboardInput>>,
    screens: Query<&Screen>,
    mut games: Query<&mut Game>,
    dispatch: Option<Res<crate::native::keyboard::Dispatch>>,
    mut focus: ResMut<InputFocus>,
) {
    let Ok(screen) = screens.get(event.focused_entity) else {
        return;
    };
    let Ok(mut game) = games.get_mut(screen.0) else {
        return;
    };
    let key = event.input.key_code;
    let Some(action) = controls::action(key) else {
        return;
    };
    if !event.input.state.is_pressed() {
        game.input.held.retain(|held| *held != key);
        event.propagate(false);
        return;
    }
    if dispatch
        .as_ref()
        .is_some_and(|dispatch| dispatch.modifiers != crate::native::actions::Modifiers::NONE)
    {
        return;
    }
    if action == Action::ReleaseFocus {
        game.input.clear();
        game.run.suspend();
        focus.clear();
        event.propagate(false);
        return;
    }
    event.propagate(false);
    if event.input.repeat {
        return;
    }
    match action {
        Action::TogglePause => {
            game.run.paused = !game.run.paused;
            game.input.clear();
            game.run.suspend();
        }
        Action::RestartRun => {
            game.restart = true;
            game.input.clear();
        }
        _ if game.run.paused => {}
        Action::GrabTowardMouse => game.input.grab = true,
        Action::ReleaseTowardMouse => game.input.release = true,
        Action::MoveUp | Action::MoveDown | Action::MoveLeft | Action::MoveRight => {
            if !game.input.held.contains(&key) {
                game.input.held.push(key);
            }
        }
        Action::ReleaseFocus => {}
    }
}
