use crate::domain::widget_bridge::WidgetBridgeSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardCard {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub text: String,
    pub html: String,
    pub author: String,
    pub permissions: Vec<String>,
    pub package_name: String,
    #[serde(default)]
    pub requires_server: bool,
    #[serde(default)]
    pub server_id: String,
    #[serde(default = "default_true")]
    pub streams_enabled: bool,
    #[serde(default = "default_widget_state")]
    pub widget_state: Value,
    #[serde(default = "default_card_x")]
    pub x: f64,
    #[serde(default = "default_card_y")]
    pub y: f64,
    #[serde(default = "default_card_width", alias = "w")]
    pub width: f64,
    #[serde(default = "default_card_height", alias = "h")]
    pub height: f64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub system: bool,
    #[serde(default = "default_card_z_index")]
    pub z_index: i32,
    #[serde(default)]
    pub group_id: Option<String>,
    /// Nested-group membership stack, outermost -> innermost (Stage 8b, Phase 3).
    /// `group_id` mirrors the innermost id for flat-group back-compat; empty for
    /// ungrouped cards. Disbanding the outer group pops the front, inner survives.
    #[serde(default)]
    pub group_ids: Vec<String>,
    #[serde(default)]
    pub abi_listen: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardWorkspace {
    pub id: String,
    pub name: String,
    #[serde(default = "default_camera")]
    pub camera: BoardCamera,
    pub cards: Vec<BoardCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardCamera {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardWorld {
    pub width: f64,
    pub height: f64,
    pub snap: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardState {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub density: u8,
    #[serde(default = "default_true")]
    pub global_streams_enabled: bool,
    #[serde(default = "default_world")]
    pub world: BoardWorld,
    pub active_workspace_id: String,
    pub workspaces: Vec<BoardWorkspace>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub app_name: &'static str,
    pub runtime: AppRuntimeInfo,
    pub density: u8,
    pub world: BoardWorld,
    pub cards: Vec<BoardCard>,
    pub board_state: BoardState,
    pub widget_bridge: WidgetBridgeSnapshot,
    pub servers: Vec<ServerBootstrap>,
    pub viewer: Option<ViewerBootstrap>,
}

/// The logged-in user's identity + permissions, best-effort resolved from the
/// request's JWT (`None` when unauthenticated or auth isn't required). Sands
/// never see raw JWTs — this flows down through the widget-bridge's per-card
/// meta (alongside cardState) so a sand's delete buttons can be shown/hidden
/// without a round trip, though the engine (`record:delete`/`record:delete_own`)
/// is the actual enforcement boundary, not this hint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerBootstrap {
    pub id: String,
    pub username: String,
    pub name: String,
    pub role: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRuntimeInfo {
    pub port: u16,
    pub version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerBootstrap {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub requires_auth: bool,
    pub authenticated: bool,
    pub session_state: Option<String>,
    pub username_hint: String,
    pub connected_at_unix: Option<u64>,
    pub last_error: String,
}

impl AppBootstrap {
    pub fn new(
        widget_bridge: WidgetBridgeSnapshot,
        board_state: BoardState,
        servers: Vec<ServerBootstrap>,
        runtime: AppRuntimeInfo,
        viewer: Option<ViewerBootstrap>,
    ) -> Self {
        let density = clamp_density(board_state.density);
        let cards = board_state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == board_state.active_workspace_id)
            .map(|workspace| workspace.cards.clone())
            .unwrap_or_default();

        Self {
            app_name: "Lince",
            runtime,
            density,
            world: board_state.world.clone(),
            cards,
            board_state,
            widget_bridge,
            servers,
            viewer,
        }
    }
}

impl Default for AppBootstrap {
    fn default() -> Self {
        Self::new(
            WidgetBridgeSnapshot::default(),
            default_board_state(),
            vec![],
            AppRuntimeInfo {
                port: 6174,
                version: env!("CARGO_PKG_VERSION"),
            },
            None,
        )
    }
}

pub fn default_board_state() -> BoardState {
    BoardState {
        schema_version: default_schema_version(),
        density: 4,
        global_streams_enabled: true,
        world: default_world(),
        active_workspace_id: "space-1".into(),
        workspaces: vec![
            BoardWorkspace {
                id: "space-1".into(),
                name: "Area 1".into(),
                camera: default_camera(),
                cards: seed_workspace_cards(true),
            },
            BoardWorkspace {
                id: "space-2".into(),
                name: "Area 2".into(),
                camera: default_camera(),
                cards: seed_workspace_cards(false),
            },
        ],
    }
}

pub const BOARD_STATE_SCHEMA_VERSION: u16 = 2;

fn default_schema_version() -> u16 {
    BOARD_STATE_SCHEMA_VERSION
}

fn default_true() -> bool {
    true
}

fn default_widget_state() -> Value {
    Value::Object(Map::new())
}

fn default_card_x() -> f64 {
    4_680.0
}

fn default_card_y() -> f64 {
    4_800.0
}

fn default_card_width() -> f64 {
    640.0
}

fn default_card_height() -> f64 {
    420.0
}

fn default_card_z_index() -> i32 {
    1
}

pub fn default_world() -> BoardWorld {
    BoardWorld {
        width: 10_000.0,
        height: 10_000.0,
        snap: 40.0,
    }
}

pub fn default_camera() -> BoardCamera {
    BoardCamera {
        // The seed workspace is composed for a 1920×1080 camera at 100%.
        // Its tutorial card is 1536×864 and centered in the world.
        x: -4_040.0,
        y: -4_460.0,
        scale: 1.0,
    }
}

pub const RECORD_PIN_ID: &str = "shell-record";

/// Screen coordinate parked past any real viewport so the pinned-card clamp
/// in `syncCardNode` (main.js) keeps this card against the right edge.
const PINNED_RIGHT: f64 = 99_999.0;

/// The Record sand, seeded pinned at the top-right corner. Starts
/// collapsed to an icon (`widgetState.recordExpanded` is absent/false) and
/// expands in place when it receives a `recordClicked`/`recordCreate` ABI
/// event; see `syncCardNode` in main.js for the icon<->full geometry.
pub fn record_pin_card() -> BoardCard {
    let mut card = package_card(
        RECORD_PIN_ID,
        "Record",
        "record.html",
        PINNED_RIGHT,
        0.0,
        340.0,
        520.0,
    );
    card.pinned = true;
    card.z_index = 50;
    card
}

fn seed_workspace_cards(include_seed_cards: bool) -> Vec<BoardCard> {
    let mut cards = vec![shell_card(
        "shell-edit",
        "Edit",
        "lince-shell-edit.html",
        0.0,
        0.0,
        280.0,
        620.0,
        92,
    )];

    cards.push(record_pin_card());

    if include_seed_cards {
        // Was the flat "Tutorial" sand (`lince-shell-tutorial.html`) until it
        // was rebuilt as the chaptered Instinct package (2026-08-01).
        let instinct = package_card(
            "seed-instinct",
            "Instinct",
            "instinct.html",
            4_232.0,
            4_568.0,
            1_536.0,
            864.0,
        );
        cards.push(instinct);
    }

    cards
}

fn shell_card(
    id: &str,
    title: &str,
    package_name: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    z_index: i32,
) -> BoardCard {
    let mut card = package_card(id, title, package_name, x, y, width, height);
    card.pinned = true;
    card.system = true;
    card.z_index = z_index;
    card
}

fn package_card(
    id: &str,
    title: &str,
    package_name: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> BoardCard {
    BoardCard {
        id: id.into(),
        kind: "package".into(),
        title: title.into(),
        description: String::new(),
        text: String::new(),
        html: String::new(),
        author: "Lince".into(),
        permissions: vec!["bridge_state".into(), "shell_board".into()],
        package_name: package_name.into(),
        requires_server: false,
        server_id: String::new(),
        streams_enabled: true,
        widget_state: default_widget_state(),
        x,
        y,
        width,
        height,
        pinned: false,
        system: false,
        z_index: default_card_z_index(),
        group_id: None,
        group_ids: Vec::new(),
        abi_listen: Vec::new(),
    }
}

fn clamp_density(level: u8) -> u8 {
    level.clamp(1, 7)
}
