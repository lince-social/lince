use super::{
    input::{InputRequest, InputResponse},
    launch::{Capability, Description},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    BrightnessDown,
    BrightnessUp,
    VolumeDown,
    VolumeUp,
    VolumeMute,
    MicrophoneMute,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    ScreenshotScreen,
    ScreenshotRegion,
    Editor,
}

impl Action {
    pub const ALL: [Self; 12] = [
        Self::BrightnessDown,
        Self::BrightnessUp,
        Self::VolumeDown,
        Self::VolumeUp,
        Self::VolumeMute,
        Self::MicrophoneMute,
        Self::MediaPlayPause,
        Self::MediaNext,
        Self::MediaPrevious,
        Self::ScreenshotScreen,
        Self::ScreenshotRegion,
        Self::Editor,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::BrightnessDown => "hardware.brightness-down",
            Self::BrightnessUp => "hardware.brightness-up",
            Self::VolumeDown => "hardware.volume-down",
            Self::VolumeUp => "hardware.volume-up",
            Self::VolumeMute => "hardware.volume-mute",
            Self::MicrophoneMute => "hardware.microphone-mute",
            Self::MediaPlayPause => "media.play-pause",
            Self::MediaNext => "media.next",
            Self::MediaPrevious => "media.previous",
            Self::ScreenshotScreen => "capture.screen",
            Self::ScreenshotRegion => "capture.region",
            Self::Editor => "launch.editor",
        }
    }

    pub fn operation(self) -> &'static str {
        match self {
            Self::BrightnessDown => "/hardware/brightness-down",
            Self::BrightnessUp => "/hardware/brightness-up",
            Self::VolumeDown => "/hardware/volume-down",
            Self::VolumeUp => "/hardware/volume-up",
            Self::VolumeMute => "/hardware/volume-mute",
            Self::MicrophoneMute => "/hardware/microphone-mute",
            Self::MediaPlayPause => "/media/play-pause",
            Self::MediaNext => "/media/next",
            Self::MediaPrevious => "/media/previous",
            Self::ScreenshotScreen => "/capture/screen",
            Self::ScreenshotRegion => "/capture/region",
            Self::Editor => "/launch/editor",
        }
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::Token;
        match self {
            Self::BrightnessDown => Token::ExternalBrightnessDown,
            Self::BrightnessUp => Token::ExternalBrightnessUp,
            Self::VolumeDown => Token::ExternalVolumeDown,
            Self::VolumeUp => Token::ExternalVolumeUp,
            Self::VolumeMute => Token::ExternalVolumeMute,
            Self::MicrophoneMute => Token::ExternalMicrophoneMute,
            Self::MediaPlayPause => Token::ExternalMediaPlayPause,
            Self::MediaNext => Token::ExternalMediaNext,
            Self::MediaPrevious => Token::ExternalMediaPrevious,
            Self::ScreenshotScreen => Token::ExternalScreenshotScreen,
            Self::ScreenshotRegion => Token::ExternalScreenshotRegion,
            Self::Editor => Token::ExternalEditor,
        }
        .text()
    }

    pub fn binding(self) -> &'static str {
        match self {
            Self::BrightnessDown => "XF86MonBrightnessDown",
            Self::BrightnessUp => "XF86MonBrightnessUp",
            Self::VolumeDown => "XF86AudioLowerVolume",
            Self::VolumeUp => "XF86AudioRaiseVolume",
            Self::VolumeMute => "XF86AudioMute",
            Self::MicrophoneMute => "XF86AudioMicMute",
            Self::MediaPlayPause => "XF86AudioPlay",
            Self::MediaNext => "XF86AudioNext",
            Self::MediaPrevious => "XF86AudioPrev",
            Self::ScreenshotScreen => "Print",
            Self::ScreenshotRegion => "Ctrl+Print",
            Self::Editor => "Unbound",
        }
    }

    pub fn repeats(self) -> bool {
        matches!(
            self,
            Self::BrightnessDown | Self::BrightnessUp | Self::VolumeDown | Self::VolumeUp
        )
    }

    pub fn lockable(self) -> bool {
        matches!(
            self,
            Self::BrightnessDown
                | Self::BrightnessUp
                | Self::VolumeDown
                | Self::VolumeUp
                | Self::VolumeMute
                | Self::MicrophoneMute
                | Self::MediaPlayPause
                | Self::MediaNext
                | Self::MediaPrevious
        )
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|action| *action == self).unwrap()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub command: Description,
    #[serde(default)]
    pub allow_when_locked: bool,
}

impl Command {
    pub fn validate(&self, action: Action) -> Result<(), String> {
        if !self.command.valid()
            || self.command.arguments.len() > 64
            || self
                .command
                .arguments
                .iter()
                .any(|argument| argument.len() > 4096)
            || self.command.directory.as_ref().is_some_and(|directory| {
                !directory.is_absolute()
                    || directory.as_os_str().len() > 4096
                    || directory.to_string_lossy().contains('\0')
            })
        {
            return Err("Use an executable, up to 64 literal arguments of at most 4096 bytes, and an absolute working directory.".into());
        }
        if self.allow_when_locked && !action.lockable() {
            return Err("Only hardware and media controls can be allowed while locked.".into());
        }
        if self.command.capabilities.iter().any(|capability| {
            !matches!(action, Action::ScreenshotScreen | Action::ScreenshotRegion)
                || *capability != Capability::Capture
        }) {
            return Err("Only screenshot commands may request screen-capture access; other privileged capabilities are not available to these commands.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Settings(pub BTreeMap<Action, Command>);

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        for (action, command) in &self.0 {
            command
                .validate(*action)
                .map_err(|error| format!("{}: {error}", action.label()))?;
        }
        Ok(())
    }

    pub fn command(&self, action: Action, locked: bool) -> Result<&Description, String> {
        let command = self
            .0
            .get(&action)
            .ok_or("Configure this external command in Park settings.")?;
        command.validate(action)?;
        if locked && (!action.lockable() || !command.allow_when_locked) {
            return Err("This command is not permitted while locked.".into());
        }
        Ok(&command.command)
    }
}

pub type Request = InputRequest<Settings>;

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub available: bool,
    pub revision: u64,
    pub settings: Settings,
    pub last_request: u64,
    pub response: Option<InputResponse>,
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> Command {
        Command {
            command: Description {
                program: "chosen-tool".into(),
                arguments: vec![
                    String::new(),
                    "$(echo literal)".into(),
                    "line\nvalue".into(),
                ],
                directory: Some("/tmp".into()),
                capabilities: Vec::new(),
            },
            allow_when_locked: false,
        }
    }

    #[test]
    fn literal_descriptors_round_trip_and_only_explicit_hardware_controls_run_locked() {
        let mut settings = Settings::default();
        for action in Action::ALL {
            settings.0.insert(action, command());
        }
        let encoded = serde_json::to_vec(&settings).unwrap();
        assert_eq!(
            serde_json::from_slice::<Settings>(&encoded).unwrap(),
            settings
        );
        for action in Action::ALL {
            assert!(settings.command(action, false).is_ok());
            assert!(settings.command(action, true).is_err());
            settings.0.get_mut(&action).unwrap().allow_when_locked = true;
            assert_eq!(settings.command(action, true).is_ok(), action.lockable());
            settings.0.get_mut(&action).unwrap().allow_when_locked = false;
        }
        assert!(settings.0[&Action::Editor].command.arguments[0].is_empty());
        assert_eq!(
            settings.0[&Action::Editor].command.arguments[1],
            "$(echo literal)"
        );
    }

    #[test]
    fn lock_permission_never_enables_editor_or_screen_capture_commands() {
        for action in [
            Action::Editor,
            Action::ScreenshotScreen,
            Action::ScreenshotRegion,
        ] {
            let mut candidate = command();
            candidate.allow_when_locked = true;
            assert!(!action.lockable());
            assert!(candidate.validate(action).is_err());
            let settings = Settings(BTreeMap::from([(action, candidate)]));
            assert!(settings.command(action, true).is_err());
        }
    }

    #[test]
    fn configured_actions_cannot_grant_unrelated_privileges_or_bypass_bounds() {
        for action in Action::ALL {
            for capability in Capability::ALL {
                let mut candidate = command();
                candidate.command.capabilities.push(capability);
                assert_eq!(
                    candidate.validate(action).is_ok(),
                    matches!(action, Action::ScreenshotScreen | Action::ScreenshotRegion)
                        && capability == Capability::Capture
                );
            }
        }
        let mut candidate = command();
        candidate.command.directory = Some("relative".into());
        assert!(candidate.validate(Action::Editor).is_err());
        candidate.command.directory = None;
        candidate.command.arguments = vec![String::new(); 65];
        assert!(candidate.validate(Action::Editor).is_err());
        candidate.command.arguments = vec!["x\0y".into()];
        assert!(candidate.validate(Action::Editor).is_err());
        assert!(serde_json::from_str::<Settings>(r#"{"arbitrary-launch":{}}"#).is_err());
    }
}
