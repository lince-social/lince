//! The Cell surface (blueprint V L1/L3, VII.3): a minimal web host on the new
//! core. It serves exactly three things — the board shell, the sands, and the
//! one transport WebSocket every sand speaks (Protein reads, Actions writes,
//! ephemeral lanes). No SQL, no domain logic here: the surface only wires HTTP
//! to the engine and transport. Sands are static assets that talk Protein.

use std::sync::Arc;

use axum::extract::ws::WebSocket;
use axum::extract::{State, WebSocketUpgrade};
use axum::response::{Html, Response};
use axum::routing::get;
use axum::Router;
use engine::Engine;
use transport::LaneHub;

#[derive(Clone)]
pub struct Surface {
    pub engine: Arc<Engine>,
    pub hub: Arc<LaneHub>,
}

impl Surface {
    pub fn new(engine: Arc<Engine>) -> Surface {
        Surface { engine, hub: Arc::new(LaneHub::new()) }
    }
}

pub fn router(surface: Surface) -> Router {
    Router::new()
        .route("/", get(board))
        .route("/sand/focus", get(focus_sand))
        .route("/ws", get(ws_upgrade))
        .with_state(surface)
}

/// The board shell (L1): hosts sands. The pilot embeds the focus sand directly.
async fn board() -> Html<&'static str> {
    Html(include_str!("../assets/board.html"))
}

/// The focus-queue corner sand (blueprint Window 1b): a self-contained widget
/// that speaks only Protein + Actions over the transport.
async fn focus_sand() -> Html<&'static str> {
    Html(include_str!("../assets/focus.html"))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(surface): State<Surface>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(surface, socket))
}

async fn handle_socket(surface: Surface, socket: WebSocket) {
    // The pilot runs as the local Cell: subject = None (sees everything).
    // Auth/multi-user identity is a later layer that just sets a Some(subject).
    let connection_id = nucleus::new_uid("conn");
    transport::ws::serve(surface.engine, surface.hub, connection_id, None, socket).await;
}

/// Where the surface stores the Cell's database.
pub fn default_db_url() -> String {
    let dir = dirs::config_dir()
        .map(|d| d.join("lince"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    format!("sqlite://{}", dir.join("cell.db").display())
}
