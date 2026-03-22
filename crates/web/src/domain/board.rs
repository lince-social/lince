use crate::domain::widget_bridge::WidgetBridgeSnapshot;
use serde::{Deserialize, Serialize};

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
    pub server_id: String,
    #[serde(default)]
    pub view_id: Option<u32>,
    pub x: u8,
    pub y: u8,
    pub w: u8,
    pub h: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardWorkspace {
    pub id: String,
    pub name: String,
    pub cards: Vec<BoardCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardState {
    pub density: u8,
    pub active_workspace_id: String,
    pub workspaces: Vec<BoardWorkspace>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub app_name: &'static str,
    pub cols: u8,
    pub rows: u8,
    pub gap: u8,
    pub density: u8,
    pub cards: Vec<BoardCard>,
    pub board_state: BoardState,
    pub widget_bridge: WidgetBridgeSnapshot,
    pub servers: Vec<ServerBootstrap>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerBootstrap {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub authenticated: bool,
    pub username_hint: String,
}

impl AppBootstrap {
    pub fn new(
        widget_bridge: WidgetBridgeSnapshot,
        board_state: BoardState,
        servers: Vec<ServerBootstrap>,
    ) -> Self {
        let density = clamp_density(board_state.density);
        let (cols, rows, gap) = density_layout(density);
        let cards = board_state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == board_state.active_workspace_id)
            .map(|workspace| workspace.cards.clone())
            .unwrap_or_default();

        Self {
            app_name: "Lince",
            cols,
            rows,
            gap,
            density,
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
        )
    }
}

pub fn default_board_state() -> BoardState {
    BoardState {
        density: 4,
        active_workspace_id: "space-1".into(),
        workspaces: vec![
            BoardWorkspace {
                id: "space-1".into(),
                name: "Area 1".into(),
                cards: vec![],
            },
            BoardWorkspace {
                id: "space-2".into(),
                name: "Area 2".into(),
                cards: vec![],
            },
        ],
    }
}

fn clamp_density(level: u8) -> u8 {
    level.clamp(1, 7)
}

fn density_layout(level: u8) -> (u8, u8, u8) {
    match clamp_density(level) {
        1 => (10, 7, 18),
        2 => (12, 8, 16),
        3 => (14, 9, 14),
        4 => (16, 10, 12),
        5 => (18, 11, 10),
        6 => (20, 12, 9),
        _ => (22, 13, 8),
    }
}
