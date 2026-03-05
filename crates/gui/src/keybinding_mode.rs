use gpui::{BorrowAppContext, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal = 0,
    Vim = 1,
}

impl Mode {
    pub fn from_db(value: i64) -> Self {
        match value {
            1 => Self::Vim,
            _ => Self::Normal,
        }
    }
}

pub struct KeybindingModeGlobal {
    pub mode: Mode,
}

impl Default for KeybindingModeGlobal {
    fn default() -> Self {
        Self { mode: Mode::Normal }
    }
}

impl Global for KeybindingModeGlobal {}

pub fn global_mode<C>(cx: &mut C) -> Mode
where
    C: BorrowAppContext,
{
    cx.update_default_global::<KeybindingModeGlobal, _>(|global, _| global.mode)
}

pub fn global_mode_is_vim<C>(cx: &mut C) -> bool
where
    C: BorrowAppContext,
{
    global_mode(cx) == Mode::Vim
}

pub fn set_global_mode<C>(cx: &mut C, mode: Mode)
where
    C: BorrowAppContext,
{
    cx.update_default_global::<KeybindingModeGlobal, _>(|global, _| {
        global.mode = mode;
    });
}
