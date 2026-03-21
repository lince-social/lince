mod application;
mod domain;
mod infrastructure;
mod presentation;

use {
    crate::{
        application::{ai_builder::AiBuilderState, state::AppState},
        infrastructure::{
            auth::AppAuth, board_state_store::BoardStateStore, manas::ManasGateway,
            package_catalog_store::PackageCatalogStore, terminal_store::TerminalSessionStore,
            widget_bridge_store::WidgetBridgeStore,
        },
        presentation::http::router::build_router,
    },
    injection::cross_cutting::InjectedServices,
    std::{env, io::Error as IoError, net::SocketAddr},
};

const DEFAULT_WEB_LISTEN_ADDR: &str = "127.0.0.1:6174";

pub async fn serve(services: InjectedServices, jwt_secret: String) -> Result<(), IoError> {
    dotenvy::dotenv().ok();

    let app_state = AppState {
        ai: AiBuilderState::new(),
        auth: AppAuth::new(),
        board_state: BoardStateStore::new().map_err(IoError::other)?,
        manas: ManasGateway::new().map_err(IoError::other)?,
        packages: PackageCatalogStore::new().map_err(IoError::other)?,
        terminal: TerminalSessionStore::new(),
        widget_bridge: WidgetBridgeStore::new(),
    };

    let app = axum::Router::new()
        .merge(build_router(app_state))
        .nest("/consult", html::app(services, jwt_secret)?);

    let address = env::var("HTTP_LISTEN_ADDR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .as_deref()
        .unwrap_or(DEFAULT_WEB_LISTEN_ADDR)
        .parse::<SocketAddr>()
        .map_err(|error| IoError::other(format!("Invalid HTTP_LISTEN_ADDR: {error}")))?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(IoError::other)?;
    println!(
        "Web frontend listening at http://{}",
        listener.local_addr().map_err(IoError::other)?
    );
    axum::serve(listener, app).await.map_err(IoError::other)
}
