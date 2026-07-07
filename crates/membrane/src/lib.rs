//! The Cell surface (blueprint V L1/L3, VII.3): a minimal web host on the new
//! core. It serves exactly three things — the board shell, the sands, and the
//! one transport WebSocket every sand speaks (Protein reads, Actions writes,
//! ephemeral lanes). No SQL, no domain logic here: the surface only wires HTTP
//! to the engine and transport. Sands are static assets that talk Protein.

use std::sync::Arc;

use axum::extract::ws::WebSocket;
use axum::extract::{State, WebSocketUpgrade};
use axum::http::header::CONTENT_TYPE;
use axum::response::{Html, IntoResponse, Response};
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
    let mut router = Router::new()
        .route("/", get(board))
        .route("/legacy", get(legacy_board))
        .route("/board/board.js", get(board_js))
        .route("/board/board.css", get(board_css))
        .route("/board/bridge.js", get(bridge_js))
        .route("/board/frame.js", get(frame_js))
        // The six pure host-state chrome modules, REUSED verbatim from the web
        // crate (single source of truth; see docs/stage-8b §4). Reach-in
        // include_str! keeps them un-forked; a shared board-assets/ dir is the
        // clean follow-up.
        .route("/board/grid.js", get(mod_grid))
        .route("/board/store.js", get(mod_store))
        .route("/board/viewport.js", get(mod_viewport))
        .route("/board/interactions.js", get(mod_interactions))
        .route("/board/group-logic.js", get(mod_group_logic))
        .route("/board/LynxDS-components.js", get(mod_lynxds))
        .route("/sand/focus", get(focus_sand))
        .route("/sand/todo", get(todo_sand))
        .route("/sand/table", get(table_sand))
        .route("/sand/record-info", get(record_info_sand))
        .route("/sand/kanban", get(kanban_sand))
        .route("/sand/relations", get(relations_sand))
        .route("/ws", get(ws_upgrade));
    // Test-only sink for the headless board self-test, off unless LINCE_SELFTEST
    // is set (keeps it out of the shipped binary's surface).
    if std::env::var_os("LINCE_SELFTEST").is_some() {
        router = router.route("/selftest-result", get(selftest_result));
    }
    router.with_state(surface)
}

/// The board shell (L1): owns the one transport socket and hosts sands as
/// iframe cards over the re-pointed widget bridge (blueprint VII.4).
async fn board() -> Html<&'static str> {
    Html(include_str!("../assets/board/shell.html"))
}

/// The original pilot board (focus sand embedded directly). Kept for reference.
async fn legacy_board() -> Html<&'static str> {
    Html(include_str!("../assets/board.html"))
}

/// The membrane board bootstrap: the thin wiring layer that replaces web's
/// main.js (localStorage persistence + card iframes over the re-pointed bridge).
async fn board_js() -> impl IntoResponse {
    js(include_str!("../assets/board/board.js"))
}

async fn board_css() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/css; charset=utf-8")], include_str!("../assets/board/board.css"))
        .into_response()
}

/// The board bridge: parent-side of the re-pointed data plane (Protein +
/// Actions multiplexed onto one WebSocket).
async fn bridge_js() -> impl IntoResponse {
    js(include_str!("../assets/board/bridge.js"))
}

// The six reused pure chrome modules. `include_str!` reaches into the web crate's
// static dir so there is exactly one copy (no fork); the modules import each
// other by relative `./name.js`, which resolves because all are served under
// `/board/`.
async fn mod_grid() -> impl IntoResponse {
    js(include_str!("../../web/static/presentation/board/grid.js"))
}
async fn mod_store() -> impl IntoResponse {
    js(include_str!("../../web/static/presentation/board/store.js"))
}
async fn mod_viewport() -> impl IntoResponse {
    js(include_str!("../../web/static/presentation/board/viewport.js"))
}
async fn mod_interactions() -> impl IntoResponse {
    js(include_str!("../../web/static/presentation/board/interactions.js"))
}
async fn mod_group_logic() -> impl IntoResponse {
    js(include_str!("../../web/static/presentation/board/group-logic.js"))
}
async fn mod_lynxds() -> impl IntoResponse {
    js(include_str!("../../web/static/presentation/board/LynxDS-components.js"))
}

/// The frame bootstrap: exposes `window.LinceWidgetHost` inside each sand.
async fn frame_js() -> impl IntoResponse {
    js(include_str!("../assets/board/frame.js"))
}

/// The focus-queue corner sand (blueprint Window 1b): a self-contained widget
/// that speaks only Protein + Actions over the transport.
async fn focus_sand() -> Html<&'static str> {
    Html(include_str!("../assets/focus.html"))
}

/// The Todo sand: focus-queue reading over Protein, plus create/complete
/// Actions. This is the old todo widget's core workflow on the new contract.
async fn todo_sand() -> Html<&'static str> {
    Html(include_str!("../assets/sands/todo.html"))
}

/// The table sand: the reference port onto the re-pointed bridge
/// (`source: record` + create-record/set-quantity Actions).
async fn table_sand() -> Html<&'static str> {
    Html(include_str!("../assets/sands/table.html"))
}

/// The provenance sand (blueprint W-provenance, VII.1): every record with the
/// fact log that made it, via `include: facts` — reads-only.
async fn record_info_sand() -> Html<&'static str> {
    Html(include_str!("../assets/sands/record_info.html"))
}

/// The Kanban sand: legacy quantity lanes rebuilt over Protein + Actions.
async fn kanban_sand() -> Html<&'static str> {
    Html(include_str!("../assets/sands/kanban.html"))
}

/// The Relations sand: a lightweight link graph using the `before` relation.
async fn relations_sand() -> Html<&'static str> {
    Html(include_str!("../assets/sands/relations.html"))
}

fn js(body: &'static str) -> Response {
    ([(CONTENT_TYPE, "text/javascript; charset=utf-8")], body).into_response()
}

/// Real-time verification sink: the board self-test POSTs its result here so a
/// headless run can read it from the server log without `--dump-dom`'s virtual
/// clock racing real WebSocket delivery. Test-only.
async fn selftest_result(axum::extract::RawQuery(q): axum::extract::RawQuery) -> &'static str {
    eprintln!("[selftest-result] {}", q.unwrap_or_default());
    "ok"
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
    format!("sqlite://{}", dir.join("lince.db").display())
}
