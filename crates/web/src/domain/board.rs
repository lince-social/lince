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
    #[serde(default)]
    pub view_id: Option<u32>,
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
        x: -4_200.0,
        y: -4_500.0,
        scale: 1.0,
    }
}

fn seed_workspace_cards(include_seed_cards: bool) -> Vec<BoardCard> {
    let mut cards = vec![
        shell_card(
            "shell-logo",
            "Logo",
            "lince-shell-logo.html",
            24.0,
            20.0,
            112.0,
            32.0,
            90,
        ),
        shell_card(
            "shell-operation",
            "Operation",
            "lince-shell-operation.html",
            920.0,
            14.0,
            420.0,
            40.0,
            91,
        ),
        shell_card(
            "shell-workspaces",
            "Workspaces",
            "lince-shell-workspaces.html",
            1360.0,
            14.0,
            72.0,
            40.0,
            92,
        ),
        shell_card(
            "shell-notifications",
            "Notifications",
            "lince-shell-notifications.html",
            1452.0,
            14.0,
            40.0,
            40.0,
            93,
        ),
        shell_card(
            "shell-edit",
            "Edit",
            "lince-shell-edit.html",
            1512.0,
            14.0,
            40.0,
            40.0,
            94,
        ),
        shell_card(
            "shell-zoom",
            "Zoom",
            "lince-shell-zoom.html",
            20.0,
            700.0,
            328.0,
            52.0,
            95,
        ),
    ];

    if include_seed_cards {
        cards.push(package_card(
            "seed-ai",
            "AI",
            "lince-shell-ai.html",
            4_240.0,
            4_460.0,
            460.0,
            280.0,
        ));
        cards.push(package_card(
            "seed-tutorial",
            "Tutorial",
            "lince-shell-tutorial.html",
            4_760.0,
            4_460.0,
            760.0,
            520.0,
        ));
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
        view_id: None,
        streams_enabled: true,
        widget_state: default_widget_state(),
        x,
        y,
        width,
        height,
        pinned: false,
        system: false,
        z_index: default_card_z_index(),
    }
}

fn clamp_density(level: u8) -> u8 {
    level.clamp(1, 7)
}
