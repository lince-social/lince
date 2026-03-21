use {
    crate::{
        application::state::AppState,
        domain::board::{AppBootstrap, AuthBootstrap, default_board_state},
        presentation::{
            http::api::{
                ai::{
                    ai_builder_status, download_draft, generate_widget, store_api_key,
                    update_draft_size,
                },
                auth::{get_auth_status, post_auth_login, post_auth_logout, post_auth_skip},
                board::{export_workspace, get_board_state, import_workspace, put_board_state},
                integrations::proxy_manas_view,
                packages::{
                    get_local_package, install_package, list_local_packages, preview_package,
                },
                terminal::{
                    create_terminal_session, delete_terminal_session, get_terminal_output,
                    post_terminal_input,
                },
                widget_bridge::{get_widget_bridge_state, post_widget_bridge_print},
            },
            http::auth_middleware::require_auth,
            pages::{render_ai_builder, render_app},
        },
    },
    axum::{
        Router,
        extract::State,
        http::{HeaderMap, header},
        middleware,
        response::{Html, IntoResponse, Redirect},
        routing::{get, post},
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
            "/integrations/manas/views/{view_id}/stream",
            get(proxy_manas_view),
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
        .route("/packages/local/{package_id}", get(get_local_package))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::<AppState>::new()
        .route("/", get(index))
        .route("/ai", get(ai_builder_page))
        .route("/api/auth/status", get(get_auth_status))
        .route("/api/auth/login", post(post_auth_login))
        .route("/api/auth/skip", post(post_auth_skip))
        .route("/api/auth/logout", post(post_auth_logout))
        .nest("/api", protected_api)
        .nest_service("/static", ServeDir::new(static_dir))
        .with_state(state)
}

async fn index(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let bootstrap = build_bootstrap(&state, &headers).await;
    Html(render_app(&bootstrap))
}

async fn ai_builder_page(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let bootstrap = build_bootstrap(&state, &headers).await;

    if !bootstrap.auth.authenticated {
        return Redirect::to("/").into_response();
    }

    Html(render_ai_builder()).into_response()
}

async fn build_bootstrap(state: &AppState, headers: &HeaderMap) -> AppBootstrap {
    let token = crate::infrastructure::auth::parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        crate::infrastructure::auth::session_cookie_name(),
    );
    let session = state.auth.session(token.as_deref()).await;
    let authenticated = session.is_some();
    let widget_bridge = if authenticated {
        state.widget_bridge.snapshot().await
    } else {
        Default::default()
    };
    let board_state = if authenticated {
        state.board_state.snapshot().await
    } else {
        default_board_state()
    };

    AppBootstrap::new(
        widget_bridge,
        board_state,
        AuthBootstrap {
            authenticated,
            username_hint: session
                .map(|record| record.username_hint)
                .unwrap_or_else(|| state.auth.default_username_hint().to_string()),
        },
    )
}
