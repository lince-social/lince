// non-lean-yet

use super::{OutputViewport, display, external, input, launch, magnification};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    pub application_windows: bool,
    pub output_management: bool,
    pub session_control: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SurfaceId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowState {
    pub minimized: bool,
    pub fullscreen: bool,
    pub windowed_fullscreen: bool,
    pub maximized: bool,
    pub resizing: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowRequest {
    pub minimized: Option<bool>,
    pub fullscreen: Option<bool>,
    pub maximized: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemId {
    Native([u8; 16]),
    Application(SurfaceId),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Manipulation {
    Move,
    Resize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceLayout {
    Free,
    Tiled,
    Row,
    Column,
    Grid,
    ScrollingColumns,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopAction {
    Unfocus,
    ToggleFocus,
    ToggleOverview,
    ToggleTiledView,
    RecentNext,
    RecentPrevious,
    CommitRecent {
        request: u64,
    },
    ToggleEdit,
    SelectPlacement {
        workspace: u64,
        item: Option<[u8; 16]>,
    },
    Focus(Direction),
    Move(Direction),
    Stack(Direction),
    ResizeColumn(i32),
    ResizeItem(i32),
    CycleColumnWidth,
    CycleItemWidth,
    Resize(Direction),
    CycleRowHeight,
    SetLayout(WorkspaceLayout),
    ToggleTiling,
    ToggleFloating,
    ToggleFullscreen,
    ToggleWindowedFullscreen,
    CreateCompactGroup,
    AddCompactMembers,
    NextCompactMember,
    PreviousCompactMember,
    MoveCompactMember(Direction),
    DetachCompactMember,
    DissolveCompactGroup,
    MarkScratch,
    ShowScratch,
    HideScratch,
    UnmarkScratch,
    Magnify(magnification::Adjustment),
    ToggleMaximized,
    Close,
    OpenOperation,
    OpenTerminal,
    OpenLauncher,
    External(external::Action),
    Workspace(u32),
    PreviousWorkspace,
    NextWorkspace,
    MoveWorkspaceEarlier,
    MoveWorkspaceLater,
    MoveToWorkspace(u32),
    FocusOutput(Direction),
    MoveToOutput(Direction),
    MoveRowToOutput(Direction),
    MoveWorkspaceToOutput(Direction),
    InspectWindowRecord,
    ReapplyOpeningRules,
    Lock,
    Logout,
    ZoomOut,
    ZoomIn,
}

impl DesktopAction {
    pub fn is_recent(self) -> bool {
        matches!(self, Self::RecentNext | Self::RecentPrevious)
    }

    pub fn requires_host(self) -> bool {
        matches!(
            self,
            Self::FocusOutput(_)
                | Self::MoveToOutput(_)
                | Self::MoveRowToOutput(_)
                | Self::MoveWorkspaceToOutput(_)
                | Self::ReapplyOpeningRules
                | Self::OpenTerminal
                | Self::OpenLauncher
                | Self::External(_)
                | Self::Magnify(_)
                | Self::Lock
                | Self::Logout
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostEvent {
    Capabilities(Capabilities),
    WindowOpened {
        surface: SurfaceId,
        title: String,
        app_id: String,
    },
    WindowMetadata {
        surface: SurfaceId,
        title: String,
        app_id: String,
    },
    WindowClosed(SurfaceId),
    ActionFailed {
        request: u64,
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionRequest {
    pub id: u64,
    pub action: DesktopAction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowInspection {
    pub owner: u64,
    pub workspace: u64,
    pub position: [f64; 2],
    pub surface: SurfaceId,
    pub placement: [u8; 16],
}

impl WindowInspection {
    pub fn valid(self) -> bool {
        self.workspace > 0
            && self
                .position
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 1_000_000_000.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpeningRevision {
    pub generation: u64,
    pub order: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpeningView {
    pub canvas: String,
    pub workspace: u64,
    pub output: Option<OutputId>,
    pub outputs: Vec<OutputViewport>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpeningRequest {
    pub target: WindowInspection,
    pub revision: OpeningRevision,
    pub view: OpeningView,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "Keep the public request payloads and avoid allocating when sending them"
)]
pub enum HostRequest {
    ControlRevoke,
    StopCapture(u64),
    OverviewHover(OverviewHoverRequest),
    DropFinished { token: u64, accepted: bool },
    Close(SurfaceId),
    Action(DesktopAction),
    Inspect(WindowInspection),
    ReapplyOpening(OpeningRequest),
    Workspace { owner: u64, id: u64 },
    CreateWorkspace { owner: u64 },
    Display(display::DisplayRequest),
    Keyboard(input::KeyboardRequest),
    Pointer(input::PointerRequest),
    Absolute(input::AbsoluteRequest),
    AbsoluteReload(input::AbsoluteReload),
    External(external::Request),
    Programs(launch::ProgramsRequest),
    ProgramsReload(u64),
    Magnification(magnification::Request),
    MagnificationReload(magnification::Reload),
    ExternalReload,
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileDragContext {
    pub owner: u64,
    pub output: Option<OutputId>,
    pub token: u64,
}

#[cfg_attr(feature = "ui", derive(bevy_ecs::prelude::Resource))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileDragSupport {
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverviewHoverRequest {
    pub drag: FileDragContext,
    pub modal: u64,
    pub sequence: u64,
    pub workspace: u64,
}
