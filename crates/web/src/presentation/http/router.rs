use {
    crate::{
        application::state::AppState,
        domain::board::{AppBootstrap, ServerBootstrap},
        infrastructure::auth::{parse_cookie_header, session_cookie_header, session_cookie_name},
        infrastructure::organ_store::organ_requires_auth,
        presentation::{
            http::api::{
                ai::{
                    ai_builder_status, download_draft, generate_widget, store_api_key,
                    update_draft_size,
                },
                backend::router as build_backend_router,
                board::{export_workspace, get_board_state, import_workspace, put_board_state},
                integrations::{
                    proxy_manas_table_collection, proxy_manas_table_item, proxy_manas_view,
                },
                packages::{
                    get_local_package, install_package, list_local_packages, preview_package,
                },
                servers::{
                    create_server, delete_server, list_servers, login_server, logout_server,
                    update_server,
                },
                terminal::{
                    create_terminal_session, delete_terminal_session, get_terminal_output,
                    post_terminal_input,
                },
                widget_bridge::{get_widget_bridge_state, post_widget_bridge_print},
            },
            pages::{render_ai_builder, render_app},
        },
    },
    axum::{
        Router,
        extract::State,
        http::{HeaderMap, header},
        response::{Html, IntoResponse},
        routing::{get, patch, post},
    },
    tower_http::services::ServeDir,
};

pub fn build_router(state: AppState) -> Router {
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
            "/integrations/servers/{server_id}/table/{table}",
            get(proxy_manas_table_collection).post(proxy_manas_table_collection),
        )
        .route(
            "/integrations/servers/{server_id}/table/{table}/{id}",
            get(proxy_manas_table_item)
                .patch(proxy_manas_table_item)
                .delete(proxy_manas_table_item),
        )
        .route("/terminal/sessions", post(create_terminal_session))
        .route(
            "/terminal/sessions/{session_id}/output",
            get(get_terminal_output),
        )
        .route(
            "/terminal/sessions/{session_id}/input",
            post(post_terminal_input),
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
        .route("/packages/preview", post(preview_package))
        .route("/packages/install", post(install_package))
        .route("/packages/local", get(list_local_packages))
        .route("/packages/local/{package_id}", get(get_local_package));

    Router::<AppState>::new()
        .route("/", get(index))
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
        .nest_service("/static", ServeDir::new(static_dir))
        .with_state(state)
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
            let authenticated =
                !organ_requires_auth(&server, state.local_auth_required) || status.is_some();
            ServerBootstrap {
                id: server.id,
                name: server.name,
                base_url: server.base_url,
                authenticated,
                username_hint: status
                    .map(|value| value.username_hint.clone())
                    .unwrap_or_default(),
            }
        })
        .collect();
    let widget_bridge = state.widget_bridge.snapshot().await;
    let board_state = state.board_state.snapshot().await;

    AppBootstrap::new(widget_bridge, board_state, servers)
}
