use crate::native::actions::Modifiers;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shortcut {
    DeleteSand,
    ToggleEdit,
    OpenOperation,
    Forward,
    Backward,
    Left,
    Right,
    Down,
    Up,
    TurnLeft,
    TurnRight,
    LookUp,
    LookDown,
    FocusLeft,
    FocusDown,
    FocusUp,
    FocusRight,
    MoveLeft,
    MoveDown,
    MoveUp,
    MoveRight,
    Float,
    Fullscreen,
    Maximize,
    DesktopOperation,
    FocusLeftArrow,
    FocusDownArrow,
    FocusUpArrow,
    FocusRightArrow,
    Close,
    Workspace1,
    Workspace2,
    Workspace3,
    Workspace4,
    Workspace5,
    Workspace6,
    Workspace7,
    Workspace8,
    Workspace9,
    FocusMonitorLeft,
    FocusMonitorDown,
    FocusMonitorUp,
    FocusMonitorRight,
    MoveMonitorLeft,
    MoveMonitorDown,
    MoveMonitorUp,
    MoveMonitorRight,
    WorkspaceMonitorLeft,
    WorkspaceMonitorDown,
    WorkspaceMonitorUp,
    WorkspaceMonitorRight,
    WindowRecord,
}

impl Shortcut {
    pub const ALL: [Self; 52] = [
        Self::DeleteSand,
        Self::ToggleEdit,
        Self::OpenOperation,
        Self::Forward,
        Self::Backward,
        Self::Left,
        Self::Right,
        Self::Down,
        Self::Up,
        Self::TurnLeft,
        Self::TurnRight,
        Self::LookUp,
        Self::LookDown,
        Self::FocusLeft,
        Self::FocusDown,
        Self::FocusUp,
        Self::FocusRight,
        Self::MoveLeft,
        Self::MoveDown,
        Self::MoveUp,
        Self::MoveRight,
        Self::Float,
        Self::Fullscreen,
        Self::Maximize,
        Self::DesktopOperation,
        Self::FocusLeftArrow,
        Self::FocusDownArrow,
        Self::FocusUpArrow,
        Self::FocusRightArrow,
        Self::Close,
        Self::Workspace1,
        Self::Workspace2,
        Self::Workspace3,
        Self::Workspace4,
        Self::Workspace5,
        Self::Workspace6,
        Self::Workspace7,
        Self::Workspace8,
        Self::Workspace9,
        Self::FocusMonitorLeft,
        Self::FocusMonitorDown,
        Self::FocusMonitorUp,
        Self::FocusMonitorRight,
        Self::MoveMonitorLeft,
        Self::MoveMonitorDown,
        Self::MoveMonitorUp,
        Self::MoveMonitorRight,
        Self::WorkspaceMonitorLeft,
        Self::WorkspaceMonitorDown,
        Self::WorkspaceMonitorUp,
        Self::WorkspaceMonitorRight,
        Self::WindowRecord,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::DeleteSand => "Delete selected Sands or Castles",
            Self::ToggleEdit => "Toggle edit mode",
            Self::OpenOperation => "Open Operation",
            Self::Forward => "Move forward in 3D",
            Self::Backward => "Move backward in 3D",
            Self::Left => "Move left in 3D",
            Self::Right => "Move right in 3D",
            Self::Down => "Move down in 3D",
            Self::Up => "Move up in 3D",
            Self::TurnLeft => "Turn left in 3D",
            Self::TurnRight => "Turn right in 3D",
            Self::LookUp => "Look up in 3D",
            Self::LookDown => "Look down in 3D",
            Self::FocusLeft => "Focus the item on the left",
            Self::FocusDown => "Focus the item below",
            Self::FocusUp => "Focus the item above",
            Self::FocusRight => "Focus the item on the right",
            Self::MoveLeft => "Move the focused item left",
            Self::MoveDown => "Move the focused item down",
            Self::MoveUp => "Move the focused item up",
            Self::MoveRight => "Move the focused item right",
            Self::Float => "Toggle floating for the focused item",
            Self::Fullscreen => "Toggle fullscreen for the focused item",
            Self::Maximize => "Toggle the maximized column",
            Self::DesktopOperation => "Open desktop Operation",
            Self::FocusLeftArrow => "Focus left with an arrow key",
            Self::FocusDownArrow => "Focus down with an arrow key",
            Self::FocusUpArrow => "Focus up with an arrow key",
            Self::FocusRightArrow => "Focus right with an arrow key",
            Self::Close => "Close the focused item",
            Self::Workspace1 => "Select workspace 1",
            Self::Workspace2 => "Select workspace 2",
            Self::Workspace3 => "Select workspace 3",
            Self::Workspace4 => "Select workspace 4",
            Self::Workspace5 => "Select workspace 5",
            Self::Workspace6 => "Select workspace 6",
            Self::Workspace7 => "Select workspace 7",
            Self::Workspace8 => "Select workspace 8",
            Self::Workspace9 => "Select workspace 9",
            Self::FocusMonitorLeft => "Focus the monitor on the left",
            Self::FocusMonitorDown => "Focus the monitor below",
            Self::FocusMonitorUp => "Focus the monitor above",
            Self::FocusMonitorRight => "Focus the monitor on the right",
            Self::MoveMonitorLeft => "Move the focused item to the left monitor",
            Self::MoveMonitorDown => "Move the focused item to the monitor below",
            Self::MoveMonitorUp => "Move the focused item to the monitor above",
            Self::MoveMonitorRight => "Move the focused item to the right monitor",
            Self::WorkspaceMonitorLeft => "Move the workspace to the left monitor",
            Self::WorkspaceMonitorDown => "Move the workspace to the monitor below",
            Self::WorkspaceMonitorUp => "Move the workspace to the monitor above",
            Self::WorkspaceMonitorRight => "Move the workspace to the right monitor",
            Self::WindowRecord => "Inspect the focused application's Record",
        }
    }

    pub fn default_binding(self) -> &'static str {
        match self {
            Self::DeleteSand => "Delete",
            Self::ToggleEdit => "Alt+E",
            Self::OpenOperation => "Ctrl+K",
            Self::Forward => "W",
            Self::Backward => "S",
            Self::Left => "A",
            Self::Right => "D",
            Self::Down => "Q",
            Self::Up => "E",
            Self::TurnLeft => "Left",
            Self::TurnRight => "Right",
            Self::LookUp => "Up",
            Self::LookDown => "Down",
            Self::FocusLeft => "Super+H",
            Self::FocusDown => "Super+J",
            Self::FocusUp => "Super+K",
            Self::FocusRight => "Super+L",
            Self::MoveLeft => "Super+Shift+H",
            Self::MoveDown => "Super+Shift+J",
            Self::MoveUp => "Super+Shift+K",
            Self::MoveRight => "Super+Shift+L",
            Self::Float => "Super+V",
            Self::Fullscreen => "Super+F",
            Self::Maximize => "Super+Shift+F",
            Self::DesktopOperation => "Super+Space",
            Self::FocusLeftArrow => "Super+Left",
            Self::FocusDownArrow => "Super+Down",
            Self::FocusUpArrow => "Super+Up",
            Self::FocusRightArrow => "Super+Right",
            Self::Close => "Super+Q",
            Self::Workspace1 => "Super+1",
            Self::Workspace2 => "Super+2",
            Self::Workspace3 => "Super+3",
            Self::Workspace4 => "Super+4",
            Self::Workspace5 => "Super+5",
            Self::Workspace6 => "Super+6",
            Self::Workspace7 => "Super+7",
            Self::Workspace8 => "Super+8",
            Self::Workspace9 => "Super+9",
            Self::FocusMonitorLeft => "Super+Ctrl+Left",
            Self::FocusMonitorDown => "Super+Ctrl+Down",
            Self::FocusMonitorUp => "Super+Ctrl+Up",
            Self::FocusMonitorRight => "Super+Ctrl+Right",
            Self::MoveMonitorLeft => "Super+Ctrl+Shift+Left",
            Self::MoveMonitorDown => "Super+Ctrl+Shift+Down",
            Self::MoveMonitorUp => "Super+Ctrl+Shift+Up",
            Self::MoveMonitorRight => "Super+Ctrl+Shift+Right",
            Self::WorkspaceMonitorLeft => "Super+Alt+Shift+Left",
            Self::WorkspaceMonitorDown => "Super+Alt+Shift+Down",
            Self::WorkspaceMonitorUp => "Super+Alt+Shift+Up",
            Self::WorkspaceMonitorRight => "Super+Alt+Shift+Right",
            Self::WindowRecord => "Super+I",
        }
    }

    fn parse(self, value: &str) -> Result<Chord, String> {
        if matches!(
            self,
            Self::FocusLeft
                | Self::FocusDown
                | Self::FocusUp
                | Self::FocusRight
                | Self::MoveLeft
                | Self::MoveDown
                | Self::MoveUp
                | Self::MoveRight
                | Self::Float
                | Self::Fullscreen
                | Self::Maximize
                | Self::DesktopOperation
                | Self::FocusLeftArrow
                | Self::FocusDownArrow
                | Self::FocusUpArrow
                | Self::FocusRightArrow
                | Self::Close
                | Self::Workspace1
                | Self::Workspace2
                | Self::Workspace3
                | Self::Workspace4
                | Self::Workspace5
                | Self::Workspace6
                | Self::Workspace7
                | Self::Workspace8
                | Self::Workspace9
                | Self::FocusMonitorLeft
                | Self::FocusMonitorDown
                | Self::FocusMonitorUp
                | Self::FocusMonitorRight
                | Self::MoveMonitorLeft
                | Self::MoveMonitorDown
                | Self::MoveMonitorUp
                | Self::MoveMonitorRight
                | Self::WorkspaceMonitorLeft
                | Self::WorkspaceMonitorDown
                | Self::WorkspaceMonitorUp
                | Self::WorkspaceMonitorRight
                | Self::WindowRecord
        ) {
            Chord::parse_desktop(value)
        } else {
            Chord::parse(value)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl Chord {
    pub fn parse(value: &str) -> Result<Self, String> {
        Self::parse_scoped(value, false)
    }

    pub fn parse_desktop(value: &str) -> Result<Self, String> {
        Self::parse_scoped(value, true)
    }

    fn parse_scoped(value: &str, desktop: bool) -> Result<Self, String> {
        if value.len() > 80 {
            return Err("Use a key with optional Ctrl, Alt, Shift or Super modifiers.".into());
        }
        let mut parts: Vec<_> = value.split('+').map(str::trim).collect();
        let key_name = parts.pop().unwrap_or_default();
        let key = KEYS
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(key_name))
            .map(|(key, _)| *key)
            .ok_or("Unknown key. Use letters, digits, F1–F12, arrows or names such as Delete.")?;
        let mut modifiers = Modifiers::NONE;
        for part in parts {
            let flag = match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut modifiers.control,
                "alt" => &mut modifiers.alt,
                "shift" => &mut modifiers.shift,
                "super" | "meta" | "command" => &mut modifiers.super_key,
                _ => return Err("Use Ctrl, Alt, Shift or Super before the key.".into()),
            };
            if *flag {
                return Err("Each modifier can appear only once.".into());
            }
            *flag = true;
        }
        if desktop {
            if !modifiers.control && !modifiers.alt && !modifiers.super_key {
                return Err("Desktop shortcuts need Ctrl, Alt or Super.".into());
            }
            return Ok(Self { key, modifiers });
        }
        if matches!(
            key,
            KeyCode::Escape | KeyCode::Tab | KeyCode::Enter | KeyCode::Space
        ) {
            return Err("That key is reserved for focus, activation or cancellation.".into());
        }
        if (modifiers.control || modifiers.super_key)
            && !modifiers.alt
            && matches!(
                key,
                KeyCode::KeyA
                    | KeyCode::KeyC
                    | KeyCode::KeyX
                    | KeyCode::KeyV
                    | KeyCode::KeyZ
                    | KeyCode::KeyY
                    | KeyCode::Backspace
                    | KeyCode::Delete
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::ArrowLeft
                    | KeyCode::ArrowRight
                    | KeyCode::ArrowUp
                    | KeyCode::ArrowDown
            )
        {
            return Err("That shortcut is reserved for text editing.".into());
        }
        if modifiers.alt && matches!(key, KeyCode::Home | KeyCode::End) {
            return Err("That shortcut is reserved for text editing.".into());
        }
        Ok(Self { key, modifiers })
    }

    pub fn text(self) -> String {
        let mut parts = Vec::new();
        for (enabled, name) in [
            (self.modifiers.control, "Ctrl"),
            (self.modifiers.alt, "Alt"),
            (self.modifiers.shift, "Shift"),
            (self.modifiers.super_key, "Super"),
        ] {
            if enabled {
                parts.push(name);
            }
        }
        parts.push(KEYS.iter().find(|(key, _)| *key == self.key).unwrap().1);
        parts.join("+")
    }

    pub fn pressed(self, keys: &ButtonInput<KeyCode>) -> bool {
        keys.pressed(self.key) && Modifiers::pressed(keys) == self.modifiers
    }
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    overrides: BTreeMap<Shortcut, Option<String>>,
}

impl Settings {
    pub fn text(&self, shortcut: Shortcut) -> &str {
        match self.overrides.get(&shortcut) {
            Some(Some(value)) => value,
            Some(None) => "Unbound",
            None => shortcut.default_binding(),
        }
    }

    pub fn chord(&self, shortcut: Shortcut) -> Option<Chord> {
        shortcut.parse(self.text(shortcut)).ok()
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut assigned = HashMap::new();
        for shortcut in Shortcut::ALL {
            if self.overrides.get(&shortcut) == Some(&None) {
                continue;
            }
            let chord = shortcut.parse(self.text(shortcut))?;
            if let Some(previous) = assigned.insert(chord, shortcut) {
                return Err(format!(
                    "{} has a conflicting shortcut: {} also uses {}.",
                    shortcut.label(),
                    previous.label(),
                    chord.text()
                ));
            }
        }
        Ok(())
    }

    pub fn set(&mut self, shortcut: Shortcut, value: &str) -> Result<(), String> {
        let value = if value.trim().eq_ignore_ascii_case("unbound") {
            None
        } else {
            Some(shortcut.parse(value)?.text())
        };
        let mut next = self.clone();
        next.overrides.insert(shortcut, value);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn reset(&mut self, shortcut: Shortcut) -> Result<(), String> {
        let mut next = self.clone();
        next.overrides.remove(&shortcut);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

const KEYS: &[(KeyCode, &str)] = &[
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyD, "D"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyJ, "J"),
    (KeyCode::KeyK, "K"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyO, "O"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyT, "T"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::F1, "F1"),
    (KeyCode::F2, "F2"),
    (KeyCode::F3, "F3"),
    (KeyCode::F4, "F4"),
    (KeyCode::F5, "F5"),
    (KeyCode::F6, "F6"),
    (KeyCode::F7, "F7"),
    (KeyCode::F8, "F8"),
    (KeyCode::F9, "F9"),
    (KeyCode::F10, "F10"),
    (KeyCode::F11, "F11"),
    (KeyCode::F12, "F12"),
    (KeyCode::Delete, "Delete"),
    (KeyCode::Backspace, "Backspace"),
    (KeyCode::Escape, "Escape"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Space, "Space"),
    (KeyCode::Insert, "Insert"),
    (KeyCode::Home, "Home"),
    (KeyCode::End, "End"),
    (KeyCode::PageUp, "PageUp"),
    (KeyCode::PageDown, "PageDown"),
    (KeyCode::ArrowLeft, "Left"),
    (KeyCode::ArrowRight, "Right"),
    (KeyCode::ArrowUp, "Up"),
    (KeyCode::ArrowDown, "Down"),
    (KeyCode::Minus, "Minus"),
    (KeyCode::Equal, "Equal"),
    (KeyCode::BracketLeft, "BracketLeft"),
    (KeyCode::BracketRight, "BracketRight"),
    (KeyCode::Backslash, "Backslash"),
    (KeyCode::Slash, "Slash"),
    (KeyCode::Comma, "Comma"),
    (KeyCode::Period, "Period"),
    (KeyCode::Semicolon, "Semicolon"),
    (KeyCode::Quote, "Quote"),
    (KeyCode::Backquote, "Backquote"),
];
