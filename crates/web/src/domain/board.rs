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
        density: 4,
        global_streams_enabled: true,
        world: default_world(),
        active_workspace_id: "space-1".into(),
        workspaces: vec![
            BoardWorkspace {
                id: "space-1".into(),
                name: "Area 1".into(),
                camera: default_camera(),
                cards: vec![],
            },
            BoardWorkspace {
                id: "space-2".into(),
                name: "Area 2".into(),
                camera: default_camera(),
                cards: vec![],
            },
        ],
    }
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

fn clamp_density(level: u8) -> u8 {
    level.clamp(1, 7)
}
