pub mod colorscheme;

mod application;
mod domain;
mod infrastructure;
mod presentation;
mod sand;

use {
    crate::{
        application::{
            ai_builder::AiBuilderState, backend_api::BackendApiService, state::AppState,
        },
        infrastructure::{
            auth::AppAuth, board_state_store::BoardStateStore, manas::ManasGateway,
            organ_store::OrganStore, package_catalog_store::PackageCatalogStore,
            terminal_store::TerminalSessionStore, widget_bridge_store::WidgetBridgeStore,
        },
        presentation::http::router::build_router,
    },
    injection::cross_cutting::InjectedServices,
    std::{io::Error as IoError, net::SocketAddr, sync::Arc},
};

const DEFAULT_WEB_LISTEN_ADDR: &str = "127.0.0.1:6174";

pub async fn serve(
    services: InjectedServices,
    jwt_secret: String,
    local_auth_required: bool,
    listen_addr: Option<String>,
) -> Result<(), IoError> {
    let app_state = AppState {
        ai: AiBuilderState::new(),
        auth: AppAuth::new(),
        backend: BackendApiService::new(services.clone(), Arc::new(jwt_secret)),
        board_state: BoardStateStore::new().map_err(IoError::other)?,
        local_auth_required,
        manas: ManasGateway::new().map_err(IoError::other)?,
        organs: OrganStore::new(services.db.clone(), services.writer.clone()),
        packages: PackageCatalogStore::new().map_err(IoError::other)?,
        terminal: TerminalSessionStore::new(),
        widget_bridge: WidgetBridgeStore::new(),
    };

    let app = axum::Router::new().merge(build_router(app_state));

    let address = listen_addr
        .filter(|value| !value.trim().is_empty())
        .as_deref()
        .unwrap_or(DEFAULT_WEB_LISTEN_ADDR)
        .parse::<SocketAddr>()
        .map_err(|error| IoError::other(format!("Invalid listen addr: {error}")))?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(IoError::other)?;
    println!(
        "Web frontend listening at http://{}",
        listener.local_addr().map_err(IoError::other)?
    );
    axum::serve(listener, app).await.map_err(IoError::other)
}
