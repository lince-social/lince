use {
    super::static_assets,
    crate::{
        HttpServeMode,
        application::state::AppState,
        domain::board::{AppBootstrap, ServerBootstrap},
        infrastructure::auth::{
            RemoteServerSessionSnapshot, RemoteServerSessionState, parse_cookie_header,
            session_cookie_header, session_cookie_name,
        },
        infrastructure::organ_store::organ_requires_auth,
        presentation::{
            http::api::{
                ai::{
                    ai_builder_status, download_draft, generate_widget, store_api_key,
                    update_draft_size,
                },
                backend::router as build_backend_router,
                board::hydrated_board_state,
                board::{export_workspace, get_board_state, import_workspace, put_board_state},
                integrations::{
                    proxy_manas_file, proxy_manas_table_collection, proxy_manas_table_item,
                    proxy_manas_view, proxy_manas_view_snapshot, proxy_manas_view_table_stream,
                },
                packages::{
                    delete_dna_publication, get_dna_catalog, get_local_package,
                    get_local_package_content, get_preview_package_content, install_dna_package,
                    install_package, list_local_packages, preview_dna_package, preview_package,
                    publish_dna_package, search_dna_packages,
                },
                servers::{
                    create_server, delete_server, list_servers, login_server, logout_server,
                    update_server,
                },
                terminal::{
                    connect_terminal_socket, create_terminal_session, delete_terminal_session,
                    get_terminal_output, post_terminal_input, post_terminal_resize,
                },
                trail::{get_trail_page, get_trail_stream},
                widget_bridge::{get_widget_bridge_state, post_widget_bridge_print},
                widgets::{get_widget_contract, get_widget_stream, post_widget_action},
            },
            pages::{render_ai_builder, render_app},
        },
    },
    axum::{
        Router,
        extract::{DefaultBodyLimit, State},
        http::{HeaderMap, StatusCode, header},
        response::{Html, IntoResponse},
        routing::{get, patch, post},
    },
    tower_http::services::ServeDir,
};

const HOST_API_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

pub fn build_router(state: AppState, mode: HttpServeMode) -> Router {
    if mode == HttpServeMode::ApiOnly {
        return Router::<AppState>::new()
            .route("/", get(api_only_index))
            .nest("/api", build_backend_router())
            .with_state(state);
    }

    let static_dir = crate::infrastructure::paths::static_dir();
    let protected_api = Router::<AppState>::new()
        .route("/ai/status", get(ai_builder_status))
        .route("/ai/key", post(store_api_key))
        .route("/ai/generate", post(generate_widget))
        .route("/ai/drafts/{draft_id}/size", post(update_draft_size))
        .route("/ai/drafts/{draft_id}/download", get(download_draft))
        .route("/board/state", get(get_board_state).put(put_board_state))
        .route("/board/workspaces/import", post(import_workspace))
        .route(
            "/board/workspaces/{workspace_id}/export",
            get(export_workspace),
        )
        .route(
            "/integrations/servers/{server_id}/views/{view_id}/stream",
            get(proxy_manas_view),
        )
        .route(
            "/integrations/servers/{server_id}/views/{view_id}/snapshot",
            get(proxy_manas_view_snapshot),
        )
        .route(
            "/integrations/servers/{server_id}/views/{view_id}/table/stream",
            get(proxy_manas_view_table_stream),
        )
        .route(
            "/integrations/servers/{server_id}/files",
            get(proxy_manas_file),
        )
        .route(
            "/integrations/servers/{server_id}/table/{table}",
            get(proxy_manas_table_collection)
                .post(proxy_manas_table_collection)
                .patch(proxy_manas_table_collection),
        )
        .route(
            "/integrations/servers/{server_id}/table/{table}/{id}",
            get(proxy_manas_table_item)
                .patch(proxy_manas_table_item)
                .delete(proxy_manas_table_item),
        )
        .route("/trail/{instance_id}", get(get_trail_page))
        .route("/trail/{instance_id}/stream", get(get_trail_stream))
        .route("/widgets/{instance_id}/trail", get(get_trail_page))
        .route("/widgets/{instance_id}/trail/stream", get(get_trail_stream))
        .route("/terminal/sessions", post(create_terminal_session))
        .route("/terminal/stream", get(connect_terminal_socket))
        .route(
            "/terminal/sessions/{session_id}/output",
            get(get_terminal_output),
        )
        .route(
            "/terminal/sessions/{session_id}/input",
            post(post_terminal_input),
        )
        .route(
            "/terminal/sessions/{session_id}/resize",
            post(post_terminal_resize),
        )
        .route(
            "/terminal/sessions/{session_id}",
            axum::routing::delete(delete_terminal_session),
        )
        .route("/widget-bridge/state", get(get_widget_bridge_state))
        .route(
            "/widget-bridge/actions/print",
            post(post_widget_bridge_print),
        )
        .route("/widgets/{instance_id}/contract", get(get_widget_contract))
        .route("/widgets/{instance_id}/stream", get(get_widget_stream))
        .route(
            "/widgets/{instance_id}/actions/{action}",
            post(post_widget_action),
        )
        .route("/packages/preview", post(preview_package))
        .route("/packages/install", post(install_package))
        .route("/packages/dna/publish", post(publish_dna_package))
        .route("/packages/local", get(list_local_packages))
        .route("/packages/local/{package_id}", get(get_local_package))
        .route(
            "/packages/local/by-filename/{filename}/content/{*asset_path}",
            get(get_local_package_content),
        )
        .route(
            "/packages/previews/{preview_id}/content/{*asset_path}",
            get(get_preview_package_content),
        )
        .route("/packages/dna/catalog", get(get_dna_catalog))
        .route("/packages/dna/search", get(search_dna_packages))
        .route(
            "/packages/dna/publications/{organ_id}/{record_id}/preview",
            get(preview_dna_package),
        )
        .route(
            "/packages/dna/publications/{organ_id}/{record_id}/install",
            post(install_dna_package),
        )
        .route(
            "/packages/dna/publications/{organ_id}/{record_id}",
            axum::routing::delete(delete_dna_publication),
        )
        .layer(DefaultBodyLimit::max(HOST_API_BODY_LIMIT_BYTES));

    let router = Router::<AppState>::new()
        .route("/", get(index))
        .route("/favicon.ico", get(static_assets::favicon))
        .route("/ai", get(ai_builder_page))
        .nest("/api", build_backend_router())
        .route("/host/servers", get(list_servers).post(create_server))
        .route(
            "/host/servers/{server_id}",
            patch(update_server).delete(delete_server),
        )
        .route(
            "/host/servers/{server_id}/session",
            post(login_server).delete(logout_server),
        )
        .nest("/host", protected_api)
        .with_state(state);

    if static_dir.exists() {
        router
            .nest_service("/static", ServeDir::new(&static_dir))
            .nest_service("/host/static", ServeDir::new(&static_dir))
    } else {
        router
            .route("/static/{*path}", get(static_assets::serve))
            .route("/host/static/{*path}", get(static_assets::serve))
    }
}

async fn api_only_index() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "Lince HTTP UI is disabled on this server. Use the /api routes instead.",
    )
}

async fn index(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let (session_token, _created) = state.auth.ensure_session(session_token.as_deref()).await;
    let bootstrap = build_bootstrap(&state, Some(session_token.as_str())).await;
    (
        [(header::SET_COOKIE, session_cookie_header(&session_token))],
        Html(render_app(&bootstrap)),
    )
}

async fn ai_builder_page(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let (session_token, _created) = state.auth.ensure_session(session_token.as_deref()).await;
    (
        [(header::SET_COOKIE, session_cookie_header(&session_token))],
        Html(render_ai_builder()),
    )
        .into_response()
}

async fn build_bootstrap(state: &AppState, session_token: Option<&str>) -> AppBootstrap {
    let server_statuses = state.auth.remote_server_snapshots(session_token).await;
    let servers = state
        .organs
        .list()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|server| {
            let status = server_statuses.get(&server.id);
            let requires_auth = organ_requires_auth(&server, state.local_auth_required);
            let authenticated = !requires_auth || status.is_some_and(is_connected);
            ServerBootstrap {
                id: server.id,
                name: server.name,
                base_url: server.base_url,
                requires_auth,
                authenticated,
                session_state: status.map(|value| session_state_name(value).to_string()),
                username_hint: status
                    .map(|value| value.username_hint.clone())
                    .unwrap_or_default(),
                connected_at_unix: status.and_then(|value| value.connected_at_unix),
                last_error: status
                    .map(|value| value.last_error.clone())
                    .unwrap_or_default(),
            }
        })
        .collect();
    let widget_bridge = state.widget_bridge.snapshot().await;
    let board_state = hydrated_board_state(state).await;

    AppBootstrap::new(widget_bridge, board_state, servers)
}

fn is_connected(session: &RemoteServerSessionSnapshot) -> bool {
    matches!(session.session_state, RemoteServerSessionState::Connected)
}

fn session_state_name(session: &RemoteServerSessionSnapshot) -> &'static str {
    match session.session_state {
        RemoteServerSessionState::Connected => "connected",
        RemoteServerSessionState::LoggedOut => "logged_out",
        RemoteServerSessionState::Expired => "expired",
    }
}
