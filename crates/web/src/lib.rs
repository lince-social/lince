pub mod colorscheme;

mod application;
mod domain;
mod infrastructure;
mod presentation;
pub mod sand;

pub use crate::domain::lince_package::{LincePackage, slugify};

use {
    crate::{
        application::{
            ai_builder::AiBuilderState, backend_api::BackendApiService,
            kanban_actions::KanbanActionService, kanban_filters::KanbanFilterService,
            kanban_streams::KanbanStreamService, state::AppState,
            trail_widget::TrailWidgetService, widget_runtime::WidgetRuntimeService,
        },
        infrastructure::{
            auth::AppAuth, board_state_store::BoardStateStore, manas::ManasGateway,
            organ_store::OrganStore, package_catalog_store::PackageCatalogStore,
            package_preview_store::PackagePreviewStore, terminal_store::TerminalSessionStore,
            widget_bridge_store::WidgetBridgeStore,
        },
        presentation::http::router::build_router,
    },
    injection::cross_cutting::InjectedServices,
    std::{
        io::{Error as IoError, ErrorKind},
        net::SocketAddr,
        sync::Arc,
    },
    utils::logging::status,
};

const DEFAULT_WEB_LISTEN_ADDR: &str = "127.0.0.1:6174";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpServeMode {
    FullUi,
    ApiOnly,
}

pub async fn serve(
    services: InjectedServices,
    jwt_secret: String,
    local_auth_required: bool,
    listen_addr: Option<String>,
    mode: HttpServeMode,
) -> Result<(), IoError> {
    let auth = AppAuth::new();
    let board_state = BoardStateStore::new().map_err(IoError::other)?;
    let organs = OrganStore::new(services.db.clone(), services.writer.clone());
    let backend = BackendApiService::new(services.clone(), Arc::new(jwt_secret));
    let manas = ManasGateway::new().map_err(IoError::other)?;
    let kanban_filters = KanbanFilterService::new(board_state.clone());
    let app_state = AppState {
        ai: AiBuilderState::new(),
        auth: auth.clone(),
        backend: backend.clone(),
        board_state: board_state.clone(),
        local_auth_required,
        manas: manas.clone(),
        organs: organs.clone(),
        packages: PackageCatalogStore::new().map_err(IoError::other)?,
        package_previews: PackagePreviewStore::new(),
        terminal: TerminalSessionStore::new(),
        widget_bridge: WidgetBridgeStore::new(),
        kanban_actions: KanbanActionService::new(
            auth.clone(),
            backend.clone(),
            board_state.clone(),
            local_auth_required,
            manas.clone(),
            organs.clone(),
        ),
        kanban_filters: kanban_filters.clone(),
        kanban_streams: KanbanStreamService::new(
            auth.clone(),
            backend.clone(),
            board_state.clone(),
            kanban_filters,
            local_auth_required,
            manas.clone(),
            organs.clone(),
        ),
        trail_widget: TrailWidgetService::new(
            auth.clone(),
            backend.clone(),
            board_state.clone(),
            local_auth_required,
            manas.clone(),
            organs.clone(),
        ),
        widget_runtime: WidgetRuntimeService::new(auth, board_state, local_auth_required, organs),
    };

    let app = axum::Router::new().merge(build_router(app_state, mode));

    let listen_addr = listen_addr
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_WEB_LISTEN_ADDR.to_string());
    let address = listen_addr.parse::<SocketAddr>().map_err(|error| {
        IoError::other(format!("Invalid listen address `{listen_addr}`: {error}"))
    })?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| {
            if error.kind() == ErrorKind::AddrInUse {
                IoError::new(
                    ErrorKind::AddrInUse,
                    format!("Address {address} is already in use"),
                )
            } else {
                IoError::new(error.kind(), format!("Failed to bind {address}: {error}"))
            }
        })?;
    let label = match mode {
        HttpServeMode::FullUi => "Web frontend",
        HttpServeMode::ApiOnly => "HTTP API",
    };
    status(format!(
        "{label} listening at http://{}",
        listener.local_addr().map_err(IoError::other)?
    ));
    axum::serve(listener, app).await.map_err(IoError::other)
}
