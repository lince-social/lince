use {
    crate::{
        application::state::AppState,
        infrastructure::auth::{parse_cookie_header, session_cookie_name},
        presentation::http::api_error::{ApiResult, api_error},
    },
    axum::{
        Json,
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::IntoResponse,
    },
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatusResponse {
    pub authenticated: bool,
    pub username_hint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub authenticated: bool,
    pub username_hint: String,
}

pub async fn get_auth_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<AuthStatusResponse>> {
    let token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let session = state.auth.session(token.as_deref()).await;

    Ok(Json(AuthStatusResponse {
        authenticated: session.is_some(),
        username_hint: session
            .map(|record| record.username_hint)
            .unwrap_or_else(|| state.auth.default_username_hint().to_string()),
    }))
}

pub async fn post_auth_login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    let username = payload.username.trim();
    let password = payload.password.trim();
    if username.is_empty() || password.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Preencha login e senha do servidor externo.",
        ));
    }

    let manas_token = state
        .manas
        .login_with_credentials(username, password)
        .await
        .map_err(|message| api_error(StatusCode::UNAUTHORIZED, message))?;
    let token = state
        .auth
        .create_authenticated_session(username.to_string(), manas_token)
        .await;
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax",
        session_cookie_name(),
        token
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "Falha ao criar sessao."))?,
    );

    Ok((
        headers,
        Json(LoginResponse {
            authenticated: true,
            username_hint: username.to_string(),
        }),
    ))
}

pub async fn post_auth_skip(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let token = state.auth.create_guest_session().await;
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax",
        session_cookie_name(),
        token
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "Falha ao criar sessao."))?,
    );

    Ok((
        headers,
        Json(LoginResponse {
            authenticated: true,
            username_hint: String::new(),
        }),
    ))
}

pub async fn post_auth_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    state.auth.destroy_session(token.as_deref()).await;

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "lince_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        ),
    );

    Ok((response_headers, StatusCode::NO_CONTENT))
}
