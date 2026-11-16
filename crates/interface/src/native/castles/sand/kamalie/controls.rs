use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    GrabTowardMouse,
    ReleaseTowardMouse,
    TogglePause,
    RestartRun,
    ReleaseFocus,
}

pub const KEY_BINDINGS: &[(KeyCode, Action)] = &[
    (KeyCode::KeyW, Action::MoveUp),
    (KeyCode::ArrowUp, Action::MoveUp),
    (KeyCode::KeyS, Action::MoveDown),
    (KeyCode::ArrowDown, Action::MoveDown),
    (KeyCode::KeyA, Action::MoveLeft),
    (KeyCode::ArrowLeft, Action::MoveLeft),
    (KeyCode::KeyD, Action::MoveRight),
    (KeyCode::ArrowRight, Action::MoveRight),
    (KeyCode::KeyZ, Action::GrabTowardMouse),
    (KeyCode::KeyX, Action::ReleaseTowardMouse),
    (KeyCode::F6, Action::TogglePause),
    (KeyCode::F7, Action::RestartRun),
    (KeyCode::Tab, Action::ReleaseFocus),
];

pub const ACTIVATE_TOWARD_MOUSE: MouseButton = MouseButton::Left;
pub const SWING: &str = "Hold left click and move the mouse sideways";
pub const THRUST: &str = "Hold left click and move the mouse away from the body";

pub fn action(key: KeyCode) -> Option<Action> {
    KEY_BINDINGS
        .iter()
        .find_map(|(code, action)| (*code == key).then_some(*action))
}

#[derive(Default)]
pub(super) struct Controls {
    pub held: Vec<KeyCode>,
    pub pointer: Option<Vec2>,
    pub activating: bool,
    pub grab: bool,
    pub release: bool,
}

impl Controls {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn movement(&self) -> Vec2 {
        let pressed = |action| {
            self.held
                .iter()
                .any(|key| super::controls::action(*key) == Some(action))
        };
        Vec2::new(
            i32::from(pressed(Action::MoveRight)) as f32
                - i32::from(pressed(Action::MoveLeft)) as f32,
            i32::from(pressed(Action::MoveDown)) as f32 - i32::from(pressed(Action::MoveUp)) as f32,
        )
        .normalize_or_zero()
    }
}
