use crate::application::state::AppState;
use axum::{
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    response::Response,
};

pub async fn connect_transport_socket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: AppState, socket: WebSocket) {
    // Current web/Tauri runs as the local Cell. Subject-scoped auth lands when
    // the existing web auth model is mapped to Protein visibility subjects.
    let connection_id = nucleus::new_uid("conn");
    transport::ws::serve(
        state.cell_engine,
        state.cell_lanes,
        connection_id,
        None,
        socket,
    )
    .await;
}
