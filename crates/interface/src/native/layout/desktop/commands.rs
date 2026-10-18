use crate::host::{DesktopAction, Direction, WorkspaceLayout};

pub const COMMANDS: &[(&str, &str, DesktopAction)] = &[
    (
        "/launch/terminal",
        "Open the chosen terminal",
        DesktopAction::OpenTerminal,
    ),
    (
        "/launch/launcher",
        "Open the chosen application launcher",
        DesktopAction::OpenLauncher,
    ),
    (
        "/session/lock",
        "Request the chosen screen locker",
        DesktopAction::Lock,
    ),
    ("/session/logout", "Log out of Park", DesktopAction::Logout),
    (
        "/window/record",
        "Inspect the focused application's Record",
        DesktopAction::InspectWindowRecord,
    ),
    (
        "/layout/free",
        "Use free placement",
        DesktopAction::SetLayout(WorkspaceLayout::Free),
    ),
    (
        "/layout/row",
        "Arrange items in a row",
        DesktopAction::SetLayout(WorkspaceLayout::Row),
    ),
    (
        "/layout/column",
        "Arrange items in a column",
        DesktopAction::SetLayout(WorkspaceLayout::Column),
    ),
    (
        "/layout/grid",
        "Arrange items in a grid",
        DesktopAction::SetLayout(WorkspaceLayout::Grid),
    ),
    (
        "/layout/scrolling",
        "Use scrolling columns",
        DesktopAction::SetLayout(WorkspaceLayout::ScrollingColumns),
    ),
    (
        "/focus/left",
        "Focus the item on the left",
        DesktopAction::Focus(Direction::Left),
    ),
    (
        "/focus/down",
        "Focus the item below",
        DesktopAction::Focus(Direction::Down),
    ),
    (
        "/focus/up",
        "Focus the item above",
        DesktopAction::Focus(Direction::Up),
    ),
    (
        "/focus/right",
        "Focus the item on the right",
        DesktopAction::Focus(Direction::Right),
    ),
    (
        "/move/left",
        "Move the focused item left",
        DesktopAction::Move(Direction::Left),
    ),
    (
        "/move/down",
        "Move the focused item down",
        DesktopAction::Move(Direction::Down),
    ),
    (
        "/move/up",
        "Move the focused item up",
        DesktopAction::Move(Direction::Up),
    ),
    (
        "/move/right",
        "Move the focused item right",
        DesktopAction::Move(Direction::Right),
    ),
    (
        "/stack/left",
        "Stack with the column on the left",
        DesktopAction::Stack(Direction::Left),
    ),
    (
        "/stack/right",
        "Stack with the column on the right",
        DesktopAction::Stack(Direction::Right),
    ),
    (
        "/column/width",
        "Cycle column width",
        DesktopAction::CycleColumnWidth,
    ),
    (
        "/column/wider",
        "Make the focused column wider",
        DesktopAction::ResizeColumn(32),
    ),
    (
        "/column/narrower",
        "Make the focused column narrower",
        DesktopAction::ResizeColumn(-32),
    ),
    (
        "/item/taller",
        "Give the focused item more stack height",
        DesktopAction::ResizeItem(32),
    ),
    (
        "/item/shorter",
        "Give the focused item less stack height",
        DesktopAction::ResizeItem(-32),
    ),
    (
        "/item/float",
        "Toggle floating",
        DesktopAction::ToggleFloating,
    ),
    (
        "/item/fullscreen",
        "Toggle fullscreen",
        DesktopAction::ToggleFullscreen,
    ),
    (
        "/column/maximize",
        "Toggle the maximized column",
        DesktopAction::ToggleMaximized,
    ),
    (
        "/monitor/left",
        "Focus the left monitor",
        DesktopAction::FocusOutput(Direction::Left),
    ),
    (
        "/monitor/down",
        "Focus the monitor below",
        DesktopAction::FocusOutput(Direction::Down),
    ),
    (
        "/monitor/up",
        "Focus the monitor above",
        DesktopAction::FocusOutput(Direction::Up),
    ),
    (
        "/monitor/right",
        "Focus the right monitor",
        DesktopAction::FocusOutput(Direction::Right),
    ),
    (
        "/item/monitor/left",
        "Move the item to the left monitor",
        DesktopAction::MoveToOutput(Direction::Left),
    ),
    (
        "/item/monitor/down",
        "Move the item to the monitor below",
        DesktopAction::MoveToOutput(Direction::Down),
    ),
    (
        "/item/monitor/up",
        "Move the item to the monitor above",
        DesktopAction::MoveToOutput(Direction::Up),
    ),
    (
        "/item/monitor/right",
        "Move the item to the right monitor",
        DesktopAction::MoveToOutput(Direction::Right),
    ),
    (
        "/workspace/monitor/left",
        "Move the workspace to the left monitor",
        DesktopAction::MoveWorkspaceToOutput(Direction::Left),
    ),
    (
        "/workspace/monitor/down",
        "Move the workspace to the monitor below",
        DesktopAction::MoveWorkspaceToOutput(Direction::Down),
    ),
    (
        "/workspace/monitor/up",
        "Move the workspace to the monitor above",
        DesktopAction::MoveWorkspaceToOutput(Direction::Up),
    ),
    (
        "/workspace/monitor/right",
        "Move the workspace to the right monitor",
        DesktopAction::MoveWorkspaceToOutput(Direction::Right),
    ),
];
