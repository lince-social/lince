use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

mod activity;
pub use activity::InputActivity;

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DragActive(pub bool);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FocusPolicy {
    pub click: bool,
    pub pointer_follow: bool,
    pub warp: bool,
}

impl Default for FocusPolicy {
    fn default() -> Self {
        Self { click: true, pointer_follow: false, warp: false }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusTarget {
    pub owner: u64,
    pub workspace: u64,
    pub output: super::OutputId,
    pub item: super::ItemId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CursorWarp {
    pub sequence: u64,
    pub pointer_epoch: u64,
    pub target: FocusTarget,
}

mod pointer;
pub use pointer::*;
mod absolute;
pub use absolute::*;
mod gestures;
pub use gestures::*;
mod gesture_delivery;
pub use gesture_delivery::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeyboardSettings {
    pub layout: String,
    pub variant: String,
    pub options: String,
    pub repeat_rate: u32,
    pub repeat_delay_ms: u32,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            layout: String::new(),
            variant: String::new(),
            options: String::new(),
            repeat_rate: 25,
            repeat_delay_ms: 500,
        }
    }
}

impl KeyboardSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.repeat_rate > 100 || self.repeat_delay_ms > 5000 {
            return Err("Keyboard repeat needs a rate of 0–100 and a delay of 0–5000 ms.".into());
        }
        let valid_names = |value: &str, limit: usize, options: bool| {
            value.len() <= limit
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'_' | b'-' | b',')
                        || options && matches!(byte, b':' | b'+')
                })
        };
        if !valid_names(&self.layout, 128, false)
            || !valid_names(&self.variant, 128, false)
            || self.layout.split(',').count() > 4
            || self.variant.split(',').count() > self.layout.split(',').count()
            || !self.layout.is_empty() && self.layout.split(',').any(str::is_empty)
            || self.layout.is_empty() && !self.variant.is_empty()
        {
            return Err("Choose up to four comma-separated XKB layouts and matching variants, using letters, digits, underscores or hyphens.".into());
        }
        if !valid_names(&self.options, 512, true) {
            return Err("Use comma-separated XKB options, such as caps:escape, with at most 512 characters.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyModifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyLabel {
    pub plain: String,
    pub shifted: String,
    pub modifiers: KeyModifiers,
    pub is_modifier: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputRequest<S> {
    pub id: u64,
    pub revision: u64,
    pub settings: S,
}

pub type KeyboardRequest = InputRequest<KeyboardSettings>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputOutcome {
    Pending,
    Saved { warning: Option<String> },
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputResponse {
    pub request: u64,
    pub outcome: InputOutcome,
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyboardSnapshot {
    pub revision: u64,
    pub settings: KeyboardSettings,
    pub layouts: Vec<String>,
    pub active_layout: u32,
    pub labels: Arc<BTreeMap<String, KeyLabel>>,
    pub response: Option<InputResponse>,
    pub last_request: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_settings_bound_names_and_rates_without_accepting_paths_or_control_characters() {
        let mut settings = KeyboardSettings {
            layout: "us,br".into(),
            variant: ",abnt2".into(),
            options: "grp:alt_shift_toggle,caps:escape".into(),
            repeat_rate: 35,
            repeat_delay_ms: 300,
        };
        settings.validate().unwrap();
        for layout in ["../br", "us\0", "us,,br", "us,br,us,br,us", " br"] {
            let mut candidate = settings.clone();
            candidate.layout = layout.into();
            assert!(candidate.validate().is_err());
        }
        settings.layout = "br".into();
        settings.variant.clear();
        settings.repeat_rate = 0;
        settings.validate().unwrap();
        settings.repeat_rate = 101;
        assert!(settings.validate().is_err());
        settings.repeat_rate = 35;
        settings.repeat_delay_ms = 5001;
        assert!(settings.validate().is_err());
        settings.repeat_delay_ms = 300;
        settings.options = "caps:escape\n".into();
        assert!(settings.validate().is_err());
    }
}
