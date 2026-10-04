use crate::actions::Modifiers;
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
}

impl Shortcut {
    pub const ALL: [Self; 13] = [
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
        Chord::parse(self.text(shortcut)).ok()
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut assigned = HashMap::new();
        for shortcut in Shortcut::ALL {
            if self.overrides.get(&shortcut) == Some(&None) {
                continue;
            }
            let chord = Chord::parse(self.text(shortcut))?;
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
            Some(Chord::parse(value)?.text())
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
