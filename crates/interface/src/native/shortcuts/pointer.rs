use super::{Shortcut, model::modifier_name};
use crate::{host::Manipulation, native::actions::Modifiers};
use bevy::input::mouse::MouseButton;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerChord {
    pub button: MouseButton,
    pub modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerBinding {
    pub operation: Manipulation,
    pub chord: PointerChord,
}

impl PointerChord {
    pub fn from_input(button: MouseButton, modifiers: Modifiers) -> Result<Self, String> {
        if !matches!(
            button,
            MouseButton::Left
                | MouseButton::Right
                | MouseButton::Middle
                | MouseButton::Back
                | MouseButton::Forward
        ) {
            return Err("Choose a left, right, middle, back or forward mouse button.".into());
        }
        if !modifiers.control && !modifiers.alt && !modifiers.super_key {
            return Err(
                "Mouse move/resize needs Ctrl, Alt or Super so ordinary clicks stay available."
                    .into(),
            );
        }
        Ok(Self { button, modifiers })
    }

    pub(super) fn parse(value: &str) -> Result<Self, String> {
        let mut parts: Vec<_> = value.split('+').map(str::trim).collect();
        let button = match parts
            .pop()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "mouseleft" => MouseButton::Left,
            "mouseright" => MouseButton::Right,
            "mousemiddle" => MouseButton::Middle,
            "mouseback" => MouseButton::Back,
            "mouseforward" => MouseButton::Forward,
            _ => {
                return Err(
                    "Press a mouse button with Ctrl, Alt or Super for this command.".into(),
                );
            }
        };
        let mut modifiers = Modifiers::NONE;
        for part in parts {
            let flag = match modifier_name(part)? {
                "Ctrl" => &mut modifiers.control,
                "Alt" => &mut modifiers.alt,
                "Shift" => &mut modifiers.shift,
                _ => &mut modifiers.super_key,
            };
            if *flag {
                return Err("Each modifier can appear only once.".into());
            }
            *flag = true;
        }
        Self::from_input(button, modifiers)
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
        parts.push(match self.button {
            MouseButton::Left => "MouseLeft",
            MouseButton::Right => "MouseRight",
            MouseButton::Middle => "MouseMiddle",
            MouseButton::Back => "MouseBack",
            MouseButton::Forward => "MouseForward",
            MouseButton::Other(_) => "UnsupportedMouseButton",
        });
        parts.join("+")
    }
}

impl Shortcut {
    pub(super) fn pointer_action(self) -> Option<Manipulation> {
        match self {
            Self::PointerMove => Some(Manipulation::Move),
            Self::PointerResize => Some(Manipulation::Resize),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
