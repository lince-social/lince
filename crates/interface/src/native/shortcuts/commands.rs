use super::{Settings, Shortcut};
use crate::native::layout::desktop::CommandContext;

impl Shortcut {
    pub(crate) fn operation_path(self) -> Option<&'static str> {
        Some(match self {
            Self::Overview => "/view/overview",
            Self::RecentNext => "/focus/recent",
            Self::RecentPrevious => "/focus/recent-previous",
            Self::DesktopEdit => "/edit",
            Self::Unfocus => "/focus/toggle",
            Self::Tiling => "/layout/tiling",
            Self::CycleWidth => "/item/width",
            Self::ShrinkWidth => "/item/width/shrink",
            Self::GrowWidth => "/item/width/grow",
            Self::ShrinkHeight => "/item/height/shrink",
            Self::GrowHeight => "/item/height/grow",
            Self::CycleRowHeight => "/row/height",
            Self::Float => "/item/float",
            Self::Fullscreen => "/item/fullscreen",
            Self::WindowedFullscreen => "/item/windowed-fullscreen",
            Self::CompactCreate => "/item/group/create",
            Self::CompactAdd => "/item/group/add",
            Self::CompactNext => "/item/group/next",
            Self::CompactPrevious => "/item/group/previous",
            Self::CompactEarlier => "/item/group/earlier",
            Self::CompactLater => "/item/group/later",
            Self::CompactDetach => "/item/group/detach",
            Self::CompactDissolve => "/item/group/ungroup",
            Self::ScratchMark => "/item/scratch/mark",
            Self::ScratchShow => "/item/scratch/show",
            Self::ScratchHide => "/item/scratch/hide",
            Self::ScratchUnmark => "/item/scratch/unmark",
            Self::MagnifyIncrease => "/accessibility/magnification/increase",
            Self::MagnifyDecrease => "/accessibility/magnification/decrease",
            Self::MagnifyReset => "/accessibility/magnification/reset",
            Self::Maximize => "/item/maximize",
            Self::DesktopOperation => "/operation",
            Self::Close => "/item/close",
            Self::FocusLeft => "/focus/left",
            Self::FocusDown => "/focus/down",
            Self::FocusUp => "/focus/up",
            Self::FocusRight => "/focus/right",
            Self::MoveLeft => "/move/left",
            Self::MoveDown => "/move/down",
            Self::MoveUp => "/move/up",
            Self::MoveRight => "/move/right",
            Self::Workspace1 => "/workspace/1",
            Self::Workspace2 => "/workspace/2",
            Self::Workspace3 => "/workspace/3",
            Self::Workspace4 => "/workspace/4",
            Self::Workspace5 => "/workspace/5",
            Self::Workspace6 => "/workspace/6",
            Self::Workspace7 => "/workspace/7",
            Self::Workspace8 => "/workspace/8",
            Self::Workspace9 => "/workspace/9",
            Self::WorkspacePrevious => "/workspace/previous",
            Self::WorkspaceNext => "/workspace/next",
            Self::WorkspaceEarlier => "/workspace/move-earlier",
            Self::WorkspaceLater => "/workspace/move-later",
            Self::MoveWorkspace1 => "/item/workspace/1",
            Self::MoveWorkspace2 => "/item/workspace/2",
            Self::MoveWorkspace3 => "/item/workspace/3",
            Self::MoveWorkspace4 => "/item/workspace/4",
            Self::MoveWorkspace5 => "/item/workspace/5",
            Self::MoveWorkspace6 => "/item/workspace/6",
            Self::MoveWorkspace7 => "/item/workspace/7",
            Self::MoveWorkspace8 => "/item/workspace/8",
            Self::MoveWorkspace9 => "/item/workspace/9",
            Self::FocusMonitorLeft => "/monitor/left",
            Self::FocusMonitorDown => "/monitor/down",
            Self::FocusMonitorUp => "/monitor/up",
            Self::FocusMonitorRight => "/monitor/right",
            Self::MoveMonitorLeft => "/item/monitor/left",
            Self::MoveMonitorDown => "/item/monitor/down",
            Self::MoveMonitorUp => "/item/monitor/up",
            Self::MoveMonitorRight => "/item/monitor/right",
            Self::RowMonitorLeft => "/row/monitor/left",
            Self::RowMonitorDown => "/row/monitor/down",
            Self::RowMonitorUp => "/row/monitor/up",
            Self::RowMonitorRight => "/row/monitor/right",
            Self::WorkspaceMonitorLeft => "/workspace/monitor/left",
            Self::WorkspaceMonitorDown => "/workspace/monitor/down",
            Self::WorkspaceMonitorUp => "/workspace/monitor/up",
            Self::WorkspaceMonitorRight => "/workspace/monitor/right",
            Self::WindowRecord => "/window/properties",
            Self::ReapplyOpening => "/window/reapply-opening",
            Self::Terminal => "/launch/terminal",
            Self::Launcher => "/launch/launcher",
            Self::Lock => "/session/lock",
            Self::Logout => "/session/logout",
            Self::External(action) => action.operation(),
            _ => return None,
        })
    }

    pub(crate) fn from_operation(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|shortcut| shortcut.operation_path() == Some(value))
    }

    pub(crate) fn caption(
        self,
        settings: &Settings,
        keyboard: Option<&crate::host::input::KeyboardSnapshot>,
    ) -> String {
        format!(
            "{} · {}",
            self.label(),
            settings.display_text(self, keyboard)
        )
    }

    pub(crate) fn help(
        self,
        settings: &Settings,
        context: CommandContext,
        keyboard: Option<&crate::host::input::KeyboardSnapshot>,
    ) -> String {
        let mut value = format!(
            "{} · {}",
            self.caption(settings, keyboard),
            self.scope().label()
        );
        if let Some(reason) = context.shortcut(self) {
            value.push_str(" · ");
            value.push_str(reason);
        }
        if self == Self::WindowedFullscreen && let Some(enabled) = context.windowed_fullscreen() {
            value.push_str(if enabled { " · On" } else { " · Off" });
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_named_controls_follow_remapped_logical_keys_while_letter_bindings_stay_physical() {
        use crate::native::actions::native_key_matches;
        use bevy::{input::keyboard::Key, prelude::KeyCode};
        assert!(native_key_matches(
            KeyCode::CapsLock,
            &Key::Escape,
            KeyCode::Escape
        ));
        assert!(!native_key_matches(
            KeyCode::Escape,
            &Key::Character("a".into()),
            KeyCode::Escape
        ));
        assert!(native_key_matches(
            KeyCode::Space,
            &Key::Character(" ".into()),
            KeyCode::Space
        ));
        assert!(!native_key_matches(
            KeyCode::KeyH,
            &Key::Character("g".into()),
            KeyCode::KeyG
        ));
    }

    #[test]
    fn configured_modifier_roles_follow_xkb_metadata_instead_of_the_physical_key_name() {
        use crate::{
            host::input::{KeyLabel, KeyModifiers, KeyboardSnapshot},
            native::actions::Modifiers,
        };
        use bevy::prelude::*;
        let mut keyboard = KeyboardSnapshot::default();
        std::sync::Arc::make_mut(&mut keyboard.labels).insert(
            "CapsLock".into(),
            KeyLabel {
                plain: "Caps_Lock".into(),
                is_modifier: true,
                modifiers: KeyModifiers {
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        std::sync::Arc::make_mut(&mut keyboard.labels).insert(
            "ControlLeft".into(),
            KeyLabel {
                plain: "a".into(),
                ..Default::default()
            },
        );
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::CapsLock);
        assert_eq!(
            Modifiers::pressed_for(&keys, Some(&keyboard)),
            Modifiers::CONTROL
        );
        keys.release(KeyCode::CapsLock);
        keys.press(KeyCode::ControlLeft);
        assert_eq!(
            Modifiers::pressed_for(&keys, Some(&keyboard)),
            Modifiers::NONE
        );
        assert_eq!(Modifiers::pressed_for(&keys, None), Modifiers::CONTROL);
    }

    #[test]
    fn layout_labels_show_shifted_symbols_without_changing_the_saved_physical_binding() {
        let mut settings = Settings::default();
        settings
            .set(Shortcut::CycleWidth, "Mod+Shift+Semicolon")
            .unwrap();
        let mut keyboard = crate::host::input::KeyboardSnapshot::default();
        std::sync::Arc::make_mut(&mut keyboard.labels).insert(
            "Semicolon".into(),
            crate::host::input::KeyLabel {
                plain: "ç".into(),
                shifted: "Ç".into(),
                ..Default::default()
            },
        );
        let text = settings.display_text(Shortcut::CycleWidth, Some(&keyboard));
        assert!(text.contains("Ç; physical Semicolon"));
        assert_eq!(settings.text(Shortcut::CycleWidth), "Mod+Shift+Semicolon");
        assert_eq!(
            settings.chord(Shortcut::CycleWidth).unwrap().key,
            bevy::prelude::KeyCode::Semicolon
        );
    }

    #[test]
    fn each_desktop_action_has_one_discoverable_primary_command() {
        let mut paths = std::collections::HashSet::new();
        let mut actions = Vec::new();
        for shortcut in Shortcut::ALL {
            if let Some(path) = shortcut.operation_path() {
                assert!(paths.insert(path));
                let action = shortcut.desktop_action().unwrap();
                assert!(
                    !actions.contains(&action),
                    "Duplicate primary command for {action:?}"
                );
                actions.push(action);
                assert_eq!(Shortcut::from_operation(path), Some(shortcut));
            }
        }
        for shortcut in Shortcut::ALL {
            if let Some(action) = shortcut.desktop_action() {
                assert!(
                    actions.contains(&action),
                    "Missing discovery for {}",
                    shortcut.id()
                );
                assert_eq!(
                    shortcut.scope() == super::super::Scope::Park,
                    action.requires_host()
                );
            }
        }
    }

    #[test]
    fn desktop_scope_parsing_covers_width_commands_without_a_separate_key_allowlist() {
        let mut settings = Settings::default();
        settings.set(Shortcut::Overview, "Unbound").unwrap();
        settings.set(Shortcut::CycleWidth, "Mod+Space").unwrap();
        assert_eq!(
            settings.chord(Shortcut::CycleWidth).unwrap().key,
            bevy::prelude::KeyCode::Space
        );
        settings.set(Shortcut::Lock, "Unbound").unwrap();
        settings.set(Shortcut::CycleWidth, "Mod+Escape").unwrap();
        assert_eq!(
            settings.chord(Shortcut::CycleWidth).unwrap().key,
            bevy::prelude::KeyCode::Escape
        );
        assert!(settings.set(Shortcut::ToggleEdit, "Mod+Space").is_err());
    }
}
