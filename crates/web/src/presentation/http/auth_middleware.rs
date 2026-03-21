use {
    crate::{
        application::state::AppState,
        infrastructure::auth::{parse_cookie_header, session_cookie_name},
    },
    axum::{
        body::Body,
        extract::State,
        http::{Request, StatusCode, header},
        middleware::Next,
        response::{IntoResponse, Response},
    },
};

pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let token = parse_cookie_header(
        request
            .headers()
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );

    if state.auth.is_authenticated(token.as_deref()).await {
        return next.run(request).await;
    }

    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({
            "error": "Sessao expirada ou ausente.",
        })),
    )
        .into_response()
}
