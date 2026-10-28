use crate::host::external::Action as ExternalAction;
use crate::native::actions::Modifiers;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum Shortcut {
    Overview,
    PointerMove,
    PointerResize,
    RecentNext,
    RecentPrevious,
    DesktopEdit,
    Unfocus,
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
    Tiling,
    CycleWidth,
    ShrinkWidth,
    GrowWidth,
    ShrinkHeight,
    GrowHeight,
    CycleRowHeight,
    Float,
    Fullscreen,
    WindowedFullscreen,
    CompactCreate,
    CompactAdd,
    CompactNext,
    CompactPrevious,
    CompactEarlier,
    CompactLater,
    CompactDetach,
    CompactDissolve,
    ScratchMark,
    ScratchShow,
    ScratchHide,
    ScratchUnmark,
    MagnifyIncrease,
    MagnifyDecrease,
    MagnifyReset,
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
    WorkspacePrevious,
    WorkspaceNext,
    WorkspaceEarlier,
    WorkspaceLater,
    MoveWorkspace1,
    MoveWorkspace2,
    MoveWorkspace3,
    MoveWorkspace4,
    MoveWorkspace5,
    MoveWorkspace6,
    MoveWorkspace7,
    MoveWorkspace8,
    MoveWorkspace9,
    FocusMonitorLeft,
    FocusMonitorDown,
    FocusMonitorUp,
    FocusMonitorRight,
    MoveMonitorLeft,
    MoveMonitorDown,
    MoveMonitorUp,
    MoveMonitorRight,
    RowMonitorLeft,
    RowMonitorDown,
    RowMonitorUp,
    RowMonitorRight,
    WorkspaceMonitorLeft,
    WorkspaceMonitorDown,
    WorkspaceMonitorUp,
    WorkspaceMonitorRight,
    WindowRecord,
    ReapplyOpening,
    Terminal,
    Launcher,
    Lock,
    Logout,
    External(ExternalAction),
}

impl From<Shortcut> for String {
    fn from(value: Shortcut) -> Self {
        value.id().into()
    }
}

impl TryFrom<String> for Shortcut {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_id(&value).ok_or_else(|| format!("Unknown command: {value}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Native,
    #[serde(rename = "canvas-3d")]
    Canvas3d,
    Desktop,
    Park,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "Interface",
            Self::Canvas3d => "3D canvas",
            Self::Desktop => "Canvas and desktop",
            Self::Park => "Park session",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModKey {
    #[default]
    Super,
    Alt,
}

impl ModKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::Super => "Super",
            Self::Alt => "Alt",
        }
    }
}

impl Shortcut {
    pub const ALL: [Self; 116] = [
        Self::Overview,
        Self::PointerMove,
        Self::PointerResize,
        Self::RecentNext,
        Self::RecentPrevious,
        Self::DesktopEdit,
        Self::Unfocus,
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
        Self::Tiling,
        Self::CycleWidth,
        Self::ShrinkWidth,
        Self::GrowWidth,
        Self::ShrinkHeight,
        Self::GrowHeight,
        Self::CycleRowHeight,
        Self::Float,
        Self::Fullscreen,
        Self::WindowedFullscreen,
        Self::CompactCreate,
        Self::CompactAdd,
        Self::CompactNext,
        Self::CompactPrevious,
        Self::CompactEarlier,
        Self::CompactLater,
        Self::CompactDetach,
        Self::CompactDissolve,
        Self::ScratchMark,
        Self::ScratchShow,
        Self::ScratchHide,
        Self::ScratchUnmark,
        Self::MagnifyIncrease,
        Self::MagnifyDecrease,
        Self::MagnifyReset,
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
        Self::WorkspacePrevious,
        Self::WorkspaceNext,
        Self::WorkspaceEarlier,
        Self::WorkspaceLater,
        Self::MoveWorkspace1,
        Self::MoveWorkspace2,
        Self::MoveWorkspace3,
        Self::MoveWorkspace4,
        Self::MoveWorkspace5,
        Self::MoveWorkspace6,
        Self::MoveWorkspace7,
        Self::MoveWorkspace8,
        Self::MoveWorkspace9,
        Self::FocusMonitorLeft,
        Self::FocusMonitorDown,
        Self::FocusMonitorUp,
        Self::FocusMonitorRight,
        Self::MoveMonitorLeft,
        Self::MoveMonitorDown,
        Self::MoveMonitorUp,
        Self::MoveMonitorRight,
        Self::RowMonitorLeft,
        Self::RowMonitorDown,
        Self::RowMonitorUp,
        Self::RowMonitorRight,
        Self::WorkspaceMonitorLeft,
        Self::WorkspaceMonitorDown,
        Self::WorkspaceMonitorUp,
        Self::WorkspaceMonitorRight,
        Self::WindowRecord,
        Self::ReapplyOpening,
        Self::Terminal,
        Self::Launcher,
        Self::Lock,
        Self::Logout,
        Self::External(ExternalAction::BrightnessDown),
        Self::External(ExternalAction::BrightnessUp),
        Self::External(ExternalAction::VolumeDown),
        Self::External(ExternalAction::VolumeUp),
        Self::External(ExternalAction::VolumeMute),
        Self::External(ExternalAction::MicrophoneMute),
        Self::External(ExternalAction::MediaPlayPause),
        Self::External(ExternalAction::MediaNext),
        Self::External(ExternalAction::MediaPrevious),
        Self::External(ExternalAction::ScreenshotScreen),
        Self::External(ExternalAction::ScreenshotRegion),
        Self::External(ExternalAction::Editor),
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Overview => "view.overview",
            Self::PointerMove => "pointer.move",
            Self::PointerResize => "pointer.resize",
            Self::RecentNext => "focus.recent",
            Self::RecentPrevious => "focus.recent-previous",
            Self::DesktopEdit => "edit.toggle",
            Self::Unfocus => "focus.toggle",
            Self::DeleteSand => "selection.delete",
            Self::ToggleEdit => "edit.toggle-native",
            Self::OpenOperation => "operation.open-native",
            Self::Forward => "camera.forward",
            Self::Backward => "camera.backward",
            Self::Left => "camera.left",
            Self::Right => "camera.right",
            Self::Down => "camera.down",
            Self::Up => "camera.up",
            Self::TurnLeft => "camera.turn-left",
            Self::TurnRight => "camera.turn-right",
            Self::LookUp => "camera.look-up",
            Self::LookDown => "camera.look-down",
            Self::FocusLeft => "focus.left",
            Self::FocusDown => "focus.down",
            Self::FocusUp => "focus.up",
            Self::FocusRight => "focus.right",
            Self::MoveLeft => "item.move-left",
            Self::MoveDown => "item.move-down",
            Self::MoveUp => "item.move-up",
            Self::MoveRight => "item.move-right",
            Self::Tiling => "layout.toggle-tiling",
            Self::CycleWidth => "item.cycle-width",
            Self::ShrinkWidth => "item.shrink-width",
            Self::GrowWidth => "item.grow-width",
            Self::ShrinkHeight => "item.shrink-height",
            Self::GrowHeight => "item.grow-height",
            Self::CycleRowHeight => "row.cycle-height",
            Self::Float => "item.toggle-floating",
            Self::Fullscreen => "item.fullscreen",
            Self::WindowedFullscreen => "item.windowed-fullscreen",
            Self::CompactCreate => "item.group-create",
            Self::CompactAdd => "item.group-add",
            Self::CompactNext => "item.group-next",
            Self::CompactPrevious => "item.group-previous",
            Self::CompactEarlier => "item.group-earlier",
            Self::CompactLater => "item.group-later",
            Self::CompactDetach => "item.group-detach",
            Self::CompactDissolve => "item.group-dissolve",
            Self::ScratchMark => "item.scratch-mark",
            Self::ScratchShow => "item.scratch-show",
            Self::ScratchHide => "item.scratch-hide",
            Self::ScratchUnmark => "item.scratch-unmark",
            Self::MagnifyIncrease => "accessibility.magnification-increase",
            Self::MagnifyDecrease => "accessibility.magnification-decrease",
            Self::MagnifyReset => "accessibility.magnification-reset",
            Self::Maximize => "item.maximize",
            Self::DesktopOperation => "operation.open",
            Self::FocusLeftArrow => "focus.left-arrow",
            Self::FocusDownArrow => "focus.down-arrow",
            Self::FocusUpArrow => "focus.up-arrow",
            Self::FocusRightArrow => "focus.right-arrow",
            Self::Close => "item.close",
            Self::WindowRecord => "window.properties",
            Self::ReapplyOpening => "window.reapply-opening",
            Self::Terminal => "launch.terminal",
            Self::Launcher => "launch.launcher",
            Self::Lock => "session.lock",
            Self::Logout => "session.logout",
            Self::External(action) => action.id(),
            Self::Workspace1 => "workspace.select-1",
            Self::Workspace2 => "workspace.select-2",
            Self::Workspace3 => "workspace.select-3",
            Self::Workspace4 => "workspace.select-4",
            Self::Workspace5 => "workspace.select-5",
            Self::Workspace6 => "workspace.select-6",
            Self::Workspace7 => "workspace.select-7",
            Self::Workspace8 => "workspace.select-8",
            Self::Workspace9 => "workspace.select-9",
            Self::WorkspacePrevious => "workspace.previous",
            Self::WorkspaceNext => "workspace.next",
            Self::WorkspaceEarlier => "workspace.move-earlier",
            Self::WorkspaceLater => "workspace.move-later",
            Self::MoveWorkspace1 => "item.workspace-1",
            Self::MoveWorkspace2 => "item.workspace-2",
            Self::MoveWorkspace3 => "item.workspace-3",
            Self::MoveWorkspace4 => "item.workspace-4",
            Self::MoveWorkspace5 => "item.workspace-5",
            Self::MoveWorkspace6 => "item.workspace-6",
            Self::MoveWorkspace7 => "item.workspace-7",
            Self::MoveWorkspace8 => "item.workspace-8",
            Self::MoveWorkspace9 => "item.workspace-9",
            Self::FocusMonitorLeft => "focus.monitor-left",
            Self::FocusMonitorDown => "focus.monitor-down",
            Self::FocusMonitorUp => "focus.monitor-up",
            Self::FocusMonitorRight => "focus.monitor-right",
            Self::MoveMonitorLeft => "item.monitor-left",
            Self::MoveMonitorDown => "item.monitor-down",
            Self::MoveMonitorUp => "item.monitor-up",
            Self::MoveMonitorRight => "item.monitor-right",
            Self::RowMonitorLeft => "row.monitor-left",
            Self::RowMonitorDown => "row.monitor-down",
            Self::RowMonitorUp => "row.monitor-up",
            Self::RowMonitorRight => "row.monitor-right",
            Self::WorkspaceMonitorLeft => "workspace.monitor-left",
            Self::WorkspaceMonitorDown => "workspace.monitor-down",
            Self::WorkspaceMonitorUp => "workspace.monitor-up",
            Self::WorkspaceMonitorRight => "workspace.monitor-right",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|shortcut| shortcut.id() == value)
    }

    pub fn scope(self) -> Scope {
        if self
            .desktop_action()
            .is_some_and(crate::host::DesktopAction::requires_host)
        {
            return Scope::Park;
        }
        match self {
            Self::WindowedFullscreen => Scope::Park,
            Self::DeleteSand | Self::ToggleEdit | Self::OpenOperation => Scope::Native,
            Self::Forward
            | Self::Backward
            | Self::Left
            | Self::Right
            | Self::Down
            | Self::Up
            | Self::TurnLeft
            | Self::TurnRight
            | Self::LookUp
            | Self::LookDown => Scope::Canvas3d,
            _ => Scope::Desktop,
        }
    }

    pub fn repeats(self) -> bool {
        matches!(self, Self::External(action) if action.repeats())
            || matches!(
                self,
                Self::Forward
                    | Self::Backward
                    | Self::Left
                    | Self::Right
                    | Self::Down
                    | Self::Up
                    | Self::TurnLeft
                    | Self::TurnRight
                    | Self::LookUp
                    | Self::LookDown
                    | Self::FocusLeft
                    | Self::FocusDown
                    | Self::FocusUp
                    | Self::FocusRight
                    | Self::FocusLeftArrow
                    | Self::FocusDownArrow
                    | Self::FocusUpArrow
                    | Self::FocusRightArrow
                    | Self::MoveLeft
                    | Self::MoveDown
                    | Self::MoveUp
                    | Self::MoveRight
                    | Self::ShrinkWidth
                    | Self::GrowWidth
                    | Self::ShrinkHeight
                    | Self::GrowHeight
            )
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::Token;
        match self {
            Self::Overview => Token::ShortcutOverview.text(),
            Self::PointerMove => Token::ShortcutPointerMove.text(),
            Self::PointerResize => Token::ShortcutPointerResize.text(),
            Self::RecentNext => Token::ShortcutRecentNext.text(),
            Self::RecentPrevious => Token::ShortcutRecentPrevious.text(),
            Self::DesktopEdit => Token::ShortcutDesktopEdit.text(),
            Self::Unfocus => Token::ShortcutUnfocus.text(),
            Self::DeleteSand => Token::ShortcutDeleteSand.text(),
            Self::ToggleEdit => Token::ShortcutToggleEdit.text(),
            Self::OpenOperation => Token::ShortcutOpenOperation.text(),
            Self::Forward => Token::ShortcutForward.text(),
            Self::Backward => Token::ShortcutBackward.text(),
            Self::Left => Token::ShortcutLeft.text(),
            Self::Right => Token::ShortcutRight.text(),
            Self::Down => Token::ShortcutDown.text(),
            Self::Up => Token::ShortcutUp.text(),
            Self::TurnLeft => Token::ShortcutTurnLeft.text(),
            Self::TurnRight => Token::ShortcutTurnRight.text(),
            Self::LookUp => Token::ShortcutLookUp.text(),
            Self::LookDown => Token::ShortcutLookDown.text(),
            Self::FocusLeft => Token::ShortcutFocusLeft.text(),
            Self::FocusDown => Token::ShortcutFocusDown.text(),
            Self::FocusUp => Token::ShortcutFocusUp.text(),
            Self::FocusRight => Token::ShortcutFocusRight.text(),
            Self::MoveLeft => Token::ShortcutMoveLeft.text(),
            Self::MoveDown => Token::ShortcutMoveDown.text(),
            Self::MoveUp => Token::ShortcutMoveUp.text(),
            Self::MoveRight => Token::ShortcutMoveRight.text(),
            Self::Tiling => Token::ShortcutTiling.text(),
            Self::CycleWidth => Token::ShortcutCycleWidth.text(),
            Self::ShrinkWidth => Token::ShortcutShrinkWidth.text(),
            Self::GrowWidth => Token::ShortcutGrowWidth.text(),
            Self::ShrinkHeight => Token::ShortcutShrinkHeight.text(),
            Self::GrowHeight => Token::ShortcutGrowHeight.text(),
            Self::CycleRowHeight => Token::ShortcutCycleRowHeight.text(),
            Self::Float => Token::ShortcutFloat.text(),
            Self::Fullscreen => Token::ShortcutFullscreen.text(),
            Self::WindowedFullscreen => "Windowed fullscreen",
            Self::CompactCreate => "Create a compact group",
            Self::CompactAdd => "Add items to this compact group",
            Self::CompactNext => "Select next compact member",
            Self::CompactPrevious => "Select previous compact member",
            Self::CompactEarlier => "Move compact member earlier",
            Self::CompactLater => "Move compact member later",
            Self::CompactDetach => "Remove item from compact group",
            Self::CompactDissolve => "Ungroup compact members",
            Self::ScratchMark => "Mark this item as scratch",
            Self::ScratchShow => "Show a scratch item here",
            Self::ScratchHide => "Hide this scratch item",
            Self::ScratchUnmark => "Unmark scratch and restore its home",
            Self::MagnifyIncrease => "Increase display magnification",
            Self::MagnifyDecrease => "Decrease display magnification",
            Self::MagnifyReset => "Reset display magnification to 1×",
            Self::Maximize => Token::ShortcutMaximize.text(),
            Self::DesktopOperation => Token::ShortcutDesktopOperation.text(),
            Self::FocusLeftArrow => Token::ShortcutFocusLeftArrow.text(),
            Self::FocusDownArrow => Token::ShortcutFocusDownArrow.text(),
            Self::FocusUpArrow => Token::ShortcutFocusUpArrow.text(),
            Self::FocusRightArrow => Token::ShortcutFocusRightArrow.text(),
            Self::Close => Token::ShortcutClose.text(),
            Self::Workspace1 => Token::ShortcutWorkspace1.text(),
            Self::Workspace2 => Token::ShortcutWorkspace2.text(),
            Self::Workspace3 => Token::ShortcutWorkspace3.text(),
            Self::Workspace4 => Token::ShortcutWorkspace4.text(),
            Self::Workspace5 => Token::ShortcutWorkspace5.text(),
            Self::Workspace6 => Token::ShortcutWorkspace6.text(),
            Self::Workspace7 => Token::ShortcutWorkspace7.text(),
            Self::Workspace8 => Token::ShortcutWorkspace8.text(),
            Self::Workspace9 => Token::ShortcutWorkspace9.text(),
            Self::WorkspacePrevious => Token::ShortcutWorkspacePrevious.text(),
            Self::WorkspaceNext => Token::ShortcutWorkspaceNext.text(),
            Self::WorkspaceEarlier => Token::ShortcutWorkspaceEarlier.text(),
            Self::WorkspaceLater => Token::ShortcutWorkspaceLater.text(),
            Self::MoveWorkspace1 => Token::ShortcutMoveWorkspace1.text(),
            Self::MoveWorkspace2 => Token::ShortcutMoveWorkspace2.text(),
            Self::MoveWorkspace3 => Token::ShortcutMoveWorkspace3.text(),
            Self::MoveWorkspace4 => Token::ShortcutMoveWorkspace4.text(),
            Self::MoveWorkspace5 => Token::ShortcutMoveWorkspace5.text(),
            Self::MoveWorkspace6 => Token::ShortcutMoveWorkspace6.text(),
            Self::MoveWorkspace7 => Token::ShortcutMoveWorkspace7.text(),
            Self::MoveWorkspace8 => Token::ShortcutMoveWorkspace8.text(),
            Self::MoveWorkspace9 => Token::ShortcutMoveWorkspace9.text(),
            Self::FocusMonitorLeft => Token::ShortcutFocusMonitorLeft.text(),
            Self::FocusMonitorDown => Token::ShortcutFocusMonitorDown.text(),
            Self::FocusMonitorUp => Token::ShortcutFocusMonitorUp.text(),
            Self::FocusMonitorRight => Token::ShortcutFocusMonitorRight.text(),
            Self::MoveMonitorLeft => Token::ShortcutMoveMonitorLeft.text(),
            Self::MoveMonitorDown => Token::ShortcutMoveMonitorDown.text(),
            Self::MoveMonitorUp => Token::ShortcutMoveMonitorUp.text(),
            Self::MoveMonitorRight => Token::ShortcutMoveMonitorRight.text(),
            Self::RowMonitorLeft => Token::ShortcutRowMonitorLeft.text(),
            Self::RowMonitorDown => Token::ShortcutRowMonitorDown.text(),
            Self::RowMonitorUp => Token::ShortcutRowMonitorUp.text(),
            Self::RowMonitorRight => Token::ShortcutRowMonitorRight.text(),
            Self::WorkspaceMonitorLeft => Token::ShortcutWorkspaceMonitorLeft.text(),
            Self::WorkspaceMonitorDown => Token::ShortcutWorkspaceMonitorDown.text(),
            Self::WorkspaceMonitorUp => Token::ShortcutWorkspaceMonitorUp.text(),
            Self::WorkspaceMonitorRight => Token::ShortcutWorkspaceMonitorRight.text(),
            Self::WindowRecord => Token::ShortcutWindowRecord.text(),
            Self::ReapplyOpening => "Reapply opening rules",
            Self::Terminal => Token::ShortcutTerminal.text(),
            Self::Launcher => Token::ShortcutLauncher.text(),
            Self::Lock => Token::ShortcutLock.text(),
            Self::Logout => Token::ShortcutLogout.text(),
            Self::External(action) => action.label(),
        }
    }

    pub fn default_binding(self) -> &'static str {
        match self {
            Self::Overview => "Mod+Space",
            Self::PointerMove => "Mod+MouseLeft",
            Self::PointerResize => "Mod+MouseRight",
            Self::RecentNext => "Alt+Tab",
            Self::RecentPrevious => "Alt+Shift+Tab",
            Self::DesktopEdit => "Mod+E",
            Self::Unfocus => "Mod+Backspace",
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
            Self::FocusLeft => "Mod+H",
            Self::FocusDown => "Mod+J",
            Self::FocusUp => "Mod+K",
            Self::FocusRight => "Mod+L",
            Self::MoveLeft => "Mod+Shift+H",
            Self::MoveDown => "Mod+Shift+J",
            Self::MoveUp => "Mod+Shift+K",
            Self::MoveRight => "Mod+Shift+L",
            Self::Tiling => "Mod+T",
            Self::CycleWidth => "Mod+R",
            Self::ShrinkWidth
            | Self::GrowWidth
            | Self::ShrinkHeight
            | Self::GrowHeight
            | Self::CycleRowHeight => "Unbound",
            Self::Float => "Mod+Shift+T",
            Self::Fullscreen => "Mod+Shift+F",
            Self::WindowedFullscreen => "Unbound",
            Self::CompactCreate | Self::CompactAdd | Self::CompactNext | Self::CompactPrevious
            | Self::CompactEarlier | Self::CompactLater | Self::CompactDetach | Self::CompactDissolve => "Unbound",
            Self::ScratchMark | Self::ScratchShow | Self::ScratchHide | Self::ScratchUnmark => "Unbound",
            Self::MagnifyIncrease | Self::MagnifyDecrease | Self::MagnifyReset => "Unbound",
            Self::Maximize => "Mod+F",
            Self::DesktopOperation => "Mod+P",
            Self::FocusLeftArrow => "Mod+Left",
            Self::FocusDownArrow => "Mod+Down",
            Self::FocusUpArrow => "Mod+Up",
            Self::FocusRightArrow => "Mod+Right",
            Self::Close => "Mod+Q",
            Self::Workspace1 => "Mod+1",
            Self::Workspace2 => "Mod+2",
            Self::Workspace3 => "Mod+3",
            Self::Workspace4 => "Mod+4",
            Self::Workspace5 => "Mod+5",
            Self::Workspace6 => "Mod+6",
            Self::Workspace7 => "Mod+7",
            Self::Workspace8 => "Mod+8",
            Self::Workspace9 => "Mod+9",
            Self::WorkspacePrevious
            | Self::WorkspaceNext
            | Self::WorkspaceEarlier
            | Self::WorkspaceLater => "Unbound",
            Self::MoveWorkspace1 => "Mod+Shift+1",
            Self::MoveWorkspace2 => "Mod+Shift+2",
            Self::MoveWorkspace3 => "Mod+Shift+3",
            Self::MoveWorkspace4 => "Mod+Shift+4",
            Self::MoveWorkspace5 => "Mod+Shift+5",
            Self::MoveWorkspace6 => "Mod+Shift+6",
            Self::MoveWorkspace7 => "Mod+Shift+7",
            Self::MoveWorkspace8 => "Mod+Shift+8",
            Self::MoveWorkspace9 => "Mod+Shift+9",
            Self::FocusMonitorLeft => "Mod+Y",
            Self::FocusMonitorDown => "Mod+U",
            Self::FocusMonitorUp => "Mod+I",
            Self::FocusMonitorRight => "Mod+O",
            Self::MoveMonitorLeft => "Mod+Ctrl+Y",
            Self::MoveMonitorDown => "Mod+Ctrl+U",
            Self::MoveMonitorUp => "Mod+Ctrl+I",
            Self::MoveMonitorRight => "Mod+Ctrl+O",
            Self::RowMonitorLeft => "Mod+Shift+Y",
            Self::RowMonitorDown => "Mod+Shift+U",
            Self::RowMonitorUp => "Mod+Shift+I",
            Self::RowMonitorRight => "Mod+Shift+O",
            Self::WorkspaceMonitorLeft => "Unbound",
            Self::WorkspaceMonitorDown => "Unbound",
            Self::WorkspaceMonitorUp => "Unbound",
            Self::WorkspaceMonitorRight => "Unbound",
            Self::WindowRecord => "Unbound",
            Self::ReapplyOpening => "Unbound",
            Self::Terminal => "Mod+N",
            Self::Launcher => "Mod+D",
            Self::Lock => "Mod+Escape",
            Self::Logout => "Mod+Shift+E",
            Self::External(action) => action.binding(),
        }
    }

    fn parse(self, value: &str) -> Result<Trigger, String> {
        if self.pointer_action().is_some() {
            return super::PointerChord::parse(value).map(Trigger::Pointer);
        }
        if matches!(self.scope(), Scope::Desktop | Scope::Park) {
            Chord::parse_desktop(value).map(Trigger::Key)
        } else {
            Chord::parse(value).map(Trigger::Key)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Trigger {
    Key(Chord),
    Pointer(super::PointerChord),
}

impl From<Chord> for Trigger {
    fn from(value: Chord) -> Self {
        Self::Key(value)
    }
}

impl From<super::PointerChord> for Trigger {
    fn from(value: super::PointerChord) -> Self {
        Self::Pointer(value)
    }
}

impl Trigger {
    pub fn text(self) -> String {
        match self {
            Self::Key(value) => value.text(),
            Self::Pointer(value) => value.text(),
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
                "super" | "meta" | "command" | "mod" => &mut modifiers.super_key,
                _ => return Err("Use Ctrl, Alt, Shift or Super before the key.".into()),
            };
            if *flag {
                return Err("Each modifier can appear only once.".into());
            }
            *flag = true;
        }
        if cfg!(target_os = "linux")
            && modifiers.control
            && modifiers.alt
            && matches!(
                key,
                KeyCode::F1
                    | KeyCode::F2
                    | KeyCode::F3
                    | KeyCode::F4
                    | KeyCode::F5
                    | KeyCode::F6
                    | KeyCode::F7
                    | KeyCode::F8
                    | KeyCode::F9
                    | KeyCode::F10
                    | KeyCode::F11
                    | KeyCode::F12
            )
        {
            return Err(
                "Ctrl+Alt+F1–F12 is reserved for switching Linux virtual terminals.".into(),
            );
        }
        if desktop {
            if !modifiers.control && !modifiers.alt && !modifiers.super_key && !hardware_key(key) {
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

    pub fn from_input(key: KeyCode, modifiers: Modifiers) -> Result<Self, String> {
        if !KEYS.iter().any(|(known, _)| *known == key) {
            return Err("This physical key is not supported for shortcuts yet.".into());
        }
        Ok(Self { key, modifiers })
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
    #[serde(default)]
    mod_key: ModKey,
}

impl Settings {
    pub fn text(&self, shortcut: Shortcut) -> &str {
        match self.overrides.get(&shortcut) {
            Some(Some(value)) => value,
            Some(None) => "Unbound",
            None if shortcut == Shortcut::ToggleEdit && self.mod_key == ModKey::Alt => "Unbound",
            None => shortcut.default_binding(),
        }
    }

    pub fn chord(&self, shortcut: Shortcut) -> Option<Chord> {
        match self.trigger(shortcut)? {
            Trigger::Key(chord) => Some(chord),
            _ => None,
        }
    }

    pub fn pointer(&self, shortcut: Shortcut) -> Option<super::PointerChord> {
        match self.trigger(shortcut)? {
            Trigger::Pointer(chord) => Some(chord),
            _ => None,
        }
    }

    pub fn trigger(&self, shortcut: Shortcut) -> Option<Trigger> {
        self.parse(shortcut, self.text(shortcut)).ok()
    }

    pub fn display_text(
        &self,
        shortcut: Shortcut,
        keyboard: Option<&crate::host::input::KeyboardSnapshot>,
    ) -> String {
        let text = self.text(shortcut);
        let Some(chord) = self.chord(shortcut) else {
            return text.into();
        };
        let Some(label) =
            keyboard.and_then(|keyboard| keyboard.labels.get(&format!("{:?}", chord.key)))
        else {
            return text.into();
        };
        let symbol = if chord.modifiers.shift && !label.shifted.is_empty() {
            &label.shifted
        } else {
            &label.plain
        };
        let physical = text.rsplit('+').next().unwrap_or(text);
        if symbol.is_empty() || symbol.eq_ignore_ascii_case(physical) {
            text.into()
        } else {
            format!("{text} ({symbol}; physical {physical})")
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut assigned = HashMap::new();
        for shortcut in Shortcut::ALL {
            if self.text(shortcut).eq_ignore_ascii_case("unbound") {
                continue;
            }
            let chord = self.parse(shortcut, self.text(shortcut))?;
            if let Some(previous) = assigned.insert(chord, shortcut) {
                return Err(format!(
                    "{} has a conflicting shortcut: {} also uses {}.",
                    shortcut.label(),
                    previous.label(),
                    chord.text()
                ));
            }
        }
        self.recent_hold()?;
        Ok(())
    }

    pub(crate) fn recent_hold(&self) -> Result<Option<Modifiers>, String> {
        let mut common: Option<Modifiers> = None;
        for command in [Shortcut::RecentNext, Shortcut::RecentPrevious] {
            if let Some(chord) = self.chord(command) {
                if chord.key == KeyCode::Escape {
                    return Err(
                        "Escape is reserved for cancelling the recent-window chooser.".into(),
                    );
                }
                let modifiers = Modifiers {
                    shift: false,
                    ..chord.modifiers
                };
                if modifiers == Modifiers::NONE {
                    return Err(
                        "Recent-window switching needs a Ctrl, Alt or Super hold modifier.".into(),
                    );
                }
                common = Some(common.map_or(modifiers, |current| current.intersection(modifiers)));
            }
        }
        if common == Some(Modifiers::NONE) {
            return Err("The recent-window forward and reverse bindings need a shared Ctrl, Alt or Super hold modifier.".into());
        }
        Ok(common)
    }

    pub fn modifier(&self) -> ModKey {
        self.mod_key
    }

    pub fn conflicts(&self) -> Vec<(Trigger, Vec<Shortcut>)> {
        let mut groups: Vec<(Trigger, Vec<Shortcut>)> = Vec::new();
        for shortcut in Shortcut::ALL {
            if let Some(chord) = self.trigger(shortcut) {
                if let Some((_, commands)) = groups.iter_mut().find(|(key, _)| *key == chord) {
                    commands.push(shortcut);
                } else {
                    groups.push((chord, vec![shortcut]));
                }
            }
        }
        groups.retain(|(_, commands)| commands.len() > 1);
        groups
    }

    pub fn stage(&mut self, shortcut: Shortcut, value: Option<&str>) -> Result<(), String> {
        let value = value.filter(|value| !value.trim().eq_ignore_ascii_case("unbound"));
        let value = value
            .map(|value| self.normalize(shortcut, value))
            .transpose()?;
        self.overrides.insert(shortcut, value);
        Ok(())
    }

    pub fn stage_reset(&mut self, shortcut: Shortcut) {
        self.overrides.remove(&shortcut);
    }

    pub fn stage_modifier(&mut self, modifier: ModKey) -> Result<(), String> {
        let mut candidate = self.clone();
        candidate.mod_key = modifier;
        for shortcut in Shortcut::ALL {
            if !candidate.text(shortcut).eq_ignore_ascii_case("unbound") {
                candidate.parse(shortcut, candidate.text(shortcut))?;
            }
        }
        *self = candidate;
        Ok(())
    }

    pub fn captured(&self, chord: impl Into<Trigger>) -> String {
        let physical = chord.into().text();
        physical
            .split('+')
            .map(|part| {
                if part == self.mod_key.label() {
                    "Mod"
                } else {
                    part
                }
            })
            .collect::<Vec<_>>()
            .join("+")
    }

    fn parse(&self, shortcut: Shortcut, value: &str) -> Result<Trigger, String> {
        if value.len() > 80 {
            return Err("Use a key with optional Mod, Ctrl, Alt, Shift or Super modifiers.".into());
        }
        let mut parts: Vec<_> = value.split('+').map(str::trim).collect();
        let key = parts.pop().unwrap_or_default();
        let mut written = std::collections::BTreeSet::new();
        let mut resolved = Vec::new();
        for part in parts {
            let name = modifier_name(part)?;
            if !written.insert(name) {
                return Err("Each modifier can appear only once.".into());
            }
            let physical = if name == "Mod" {
                self.mod_key.label()
            } else {
                name
            };
            if !resolved.contains(&physical) {
                resolved.push(physical);
            }
        }
        resolved.push(key);
        shortcut.parse(&resolved.join("+"))
    }

    fn normalize(&self, shortcut: Shortcut, value: &str) -> Result<String, String> {
        let chord = self.parse(shortcut, value)?;
        let physical = chord.text();
        let mut written = std::collections::BTreeSet::new();
        let mut parts: Vec<_> = value.split('+').collect();
        parts.pop();
        for part in parts {
            written.insert(modifier_name(part.trim())?);
        }
        let mut normalized: Vec<_> = ["Mod", "Ctrl", "Alt", "Shift", "Super"]
            .into_iter()
            .filter(|name| written.contains(name))
            .collect();
        normalized.push(physical.rsplit('+').next().unwrap());
        Ok(normalized.join("+"))
    }

    pub fn overrides(&self) -> impl Iterator<Item = (Shortcut, Option<&str>)> {
        self.overrides
            .iter()
            .map(|(shortcut, value)| (*shortcut, value.as_deref()))
    }

    pub fn from_overrides(
        mod_key: ModKey,
        values: impl IntoIterator<Item = (Shortcut, Option<String>)>,
    ) -> Result<Self, String> {
        let mut settings = Self {
            mod_key,
            ..Default::default()
        };
        settings.apply(values)?;
        Ok(settings)
    }

    pub fn apply(
        &mut self,
        values: impl IntoIterator<Item = (Shortcut, Option<String>)>,
    ) -> Result<(), String> {
        let mut next = self.clone();
        for (shortcut, value) in values {
            let value = value.filter(|value| !value.trim().eq_ignore_ascii_case("unbound"));
            let value = value
                .map(|value| next.normalize(shortcut, &value))
                .transpose()?;
            next.overrides.insert(shortcut, value);
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn set(&mut self, shortcut: Shortcut, value: &str) -> Result<(), String> {
        self.apply([(shortcut, Some(value.to_string()))])
    }

    pub fn reset(&mut self, shortcut: Shortcut) -> Result<(), String> {
        let mut next = self.clone();
        next.overrides.remove(&shortcut);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

pub(super) fn modifier_name(value: &str) -> Result<&'static str, String> {
    match value.to_ascii_lowercase().as_str() {
        "mod" => Ok("Mod"),
        "ctrl" | "control" => Ok("Ctrl"),
        "alt" => Ok("Alt"),
        "shift" => Ok("Shift"),
        "super" | "meta" | "command" => Ok("Super"),
        _ => Err("Use Mod, Ctrl, Alt, Shift or Super before the key.".into()),
    }
}

const KEYS: &[(KeyCode, &str)] = &[
    (KeyCode::PrintScreen, "Print"),
    (KeyCode::AudioVolumeDown, "XF86AudioLowerVolume"),
    (KeyCode::AudioVolumeUp, "XF86AudioRaiseVolume"),
    (KeyCode::AudioVolumeMute, "XF86AudioMute"),
    (KeyCode::MediaPlayPause, "XF86AudioPlay"),
    (KeyCode::MediaTrackNext, "XF86AudioNext"),
    (KeyCode::MediaTrackPrevious, "XF86AudioPrev"),
    (
        KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Xkb(224)),
        "XF86MonBrightnessDown",
    ),
    (
        KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Xkb(225)),
        "XF86MonBrightnessUp",
    ),
    (
        KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Xkb(248)),
        "XF86AudioMicMute",
    ),
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

fn hardware_key(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::PrintScreen
            | KeyCode::AudioVolumeDown
            | KeyCode::AudioVolumeUp
            | KeyCode::AudioVolumeMute
            | KeyCode::MediaPlayPause
            | KeyCode::MediaTrackNext
            | KeyCode::MediaTrackPrevious
            | KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Xkb(224 | 225 | 248))
    )
}
