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
            kanban_streams::KanbanStreamService,
            karma_orchestra_widget::KarmaOrchestraWidgetService, state::AppState,
            trail_widget::TrailWidgetService, transfer_widget::TransferWidgetService,
            widget_runtime::WidgetRuntimeService,
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
    tokio::sync::oneshot,
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
    serve_with_bound_addr_sender(
        services,
        jwt_secret,
        local_auth_required,
        listen_addr,
        mode,
        None,
    )
    .await
}

pub async fn serve_with_bound_addr_sender(
    services: InjectedServices,
    jwt_secret: String,
    local_auth_required: bool,
    listen_addr: Option<String>,
    mode: HttpServeMode,
    bound_addr_sender: Option<oneshot::Sender<SocketAddr>>,
) -> Result<(), IoError> {
    let auth = AppAuth::with_shared_remote_tokens(services.remote_organ_auth.clone());
    let local_base_url = local_base_url_from_listen_addr(listen_addr.as_deref())?;
    let board_state = BoardStateStore::new().map_err(IoError::other)?;
    let organs = OrganStore::new(services.db.clone(), services.writer.clone());
    let backend = BackendApiService::new(services.clone(), Arc::new(jwt_secret));
    let manas = ManasGateway::new().map_err(IoError::other)?;
    let kanban_filters = KanbanFilterService::new(board_state.clone());
    let transfer_widget = TransferWidgetService::new(
        auth.clone(),
        board_state.clone(),
        local_auth_required,
        local_base_url,
        manas.clone(),
        organs.clone(),
        services.clone(),
    );
    transfer_widget.clone().spawn_sync_tasks();

    let app_state = AppState {
        ai: AiBuilderState::new(),
        auth: auth.clone(),
        services: services.clone(),
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
        karma_orchestra_widget: KarmaOrchestraWidgetService::new(
            auth.clone(),
            backend.clone(),
            board_state.clone(),
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
        transfer_widget,
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
    let local_addr = listener.local_addr().map_err(IoError::other)?;
    if let Some(sender) = bound_addr_sender {
        let _ = sender.send(local_addr);
    }
    status(format!("{label} listening at http://{local_addr}"));
    axum::serve(listener, app).await.map_err(IoError::other)
}

fn local_base_url_from_listen_addr(listen_addr: Option<&str>) -> Result<String, IoError> {
    let listen_addr = listen_addr.unwrap_or("127.0.0.1:6174");
    let address = listen_addr.parse::<SocketAddr>().map_err(|error| {
        IoError::other(format!("Invalid listen address `{listen_addr}`: {error}"))
    })?;
    let host = if address.ip().is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        address.ip().to_string()
    };
    Ok(format!("http://{host}:{}", address.port()))
}
