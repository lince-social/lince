use std::sync::atomic::{AtomicU8, Ordering};

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

static GLOBAL_KEYBINDING_MODE: AtomicU8 = AtomicU8::new(Mode::Normal as u8);

pub fn global_mode() -> Mode {
    match GLOBAL_KEYBINDING_MODE.load(Ordering::Relaxed) {
        1 => Mode::Vim,
        _ => Mode::Normal,
    }
}

pub fn global_mode_is_vim() -> bool {
    global_mode() == Mode::Vim
}

pub fn set_global_mode(mode: Mode) {
    GLOBAL_KEYBINDING_MODE.store(mode as u8, Ordering::Relaxed);
}
