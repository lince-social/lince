use {
    crate::{
        application::state::AppState,
        infrastructure::{
            auth::{
                RemoteServerSessionSnapshot, RemoteServerSessionState, parse_cookie_header,
                session_cookie_header, session_cookie_name,
            },
            organ_store::{Organ, organ_requires_auth},
        },
        presentation::http::api_error::{ApiResult, api_error},
    },
    axum::{
        Json,
        extract::{Path, State},
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::IntoResponse,
    },
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfileResponse {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub requires_auth: bool,
    pub authenticated: bool,
    pub session_state: Option<String>,
    pub username_hint: String,
    pub connected_at_unix: Option<u64>,
    pub last_error: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpsertServerProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
}

pub async fn list_servers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<ServerProfileResponse>>> {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let statuses = state
        .auth
        .remote_server_snapshots(session_token.as_deref())
        .await;
    let servers = state
        .organs
        .list()
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    Ok(Json(
        servers
            .into_iter()
            .map(|server| {
                let status = statuses.get(&server.id);
                let requires_auth = organ_requires_auth(&server, state.local_auth_required);
                let authenticated = !requires_auth || status.is_some_and(is_connected);
                ServerProfileResponse {
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
            .collect(),
    ))
}

pub async fn create_server(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpsertServerProfileRequest>,
) -> ApiResult<Json<ServerProfileResponse>> {
    let profile = state
        .organs
        .upsert(Organ {
            id: payload.id.unwrap_or_default(),
            name: payload.name,
            base_url: payload.base_url,
        })
        .await
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;

    Ok(Json(
        server_profile_response(&state, &headers, profile).await,
    ))
}

pub async fn login_server(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(payload): Json<ServerLoginRequest>,
) -> ApiResult<impl IntoResponse> {
    let username = payload.username.trim();
    let password = payload.password.trim();
    if username.is_empty() || password.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Preencha login e senha do servidor.",
        ));
    }

    let server = load_organ(&state, &server_id).await?;
    let bearer_token = state
        .manas
        .login_with_credentials(&server.base_url, username, password)
        .await
        .map_err(|message| api_error(StatusCode::UNAUTHORIZED, message))?;

    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let (session_token, _created) = state.auth.ensure_session(session_token.as_deref()).await;
    state
        .auth
        .set_server_session(
            &session_token,
            server.id.clone(),
            username.to_string(),
            bearer_token,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie_header(&session_token)).map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha ao criar sessao local.",
            )
        })?,
    );
    let snapshot = state
        .auth
        .remote_server_snapshots(Some(&session_token))
        .await
        .remove(&server.id);

    Ok((
        response_headers,
        Json(ServerProfileResponse {
            id: server.id,
            name: server.name,
            base_url: server.base_url,
            requires_auth: true,
            authenticated: true,
            session_state: Some("connected".to_string()),
            username_hint: username.to_string(),
            connected_at_unix: snapshot.and_then(|value| value.connected_at_unix),
            last_error: String::new(),
        }),
    ))
}

pub async fn logout_server(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<StatusCode> {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    state
        .auth
        .clear_server_session(session_token.as_deref(), &server_id)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_server(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(payload): Json<UpsertServerProfileRequest>,
) -> ApiResult<Json<ServerProfileResponse>> {
    let profile = state
        .organs
        .upsert(Organ {
            id: server_id,
            name: payload.name,
            base_url: payload.base_url,
        })
        .await
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;

    Ok(Json(
        server_profile_response(&state, &headers, profile).await,
    ))
}

pub async fn delete_server(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<StatusCode> {
    let deleted = state
        .organs
        .delete(&server_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    if !deleted {
        return Err(api_error(StatusCode::NOT_FOUND, "Servidor nao encontrado."));
    }

    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    state
        .auth
        .clear_server_session(session_token.as_deref(), &server_id)
        .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn load_organ(state: &AppState, server_id: &str) -> ApiResult<Organ> {
    state
        .organs
        .get(server_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Servidor nao encontrado."))
}

async fn server_profile_response(
    state: &AppState,
    headers: &HeaderMap,
    profile: Organ,
) -> ServerProfileResponse {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let mut snapshot = state
        .auth
        .remote_server_snapshots(session_token.as_deref())
        .await
        .remove(&profile.id);
    let requires_auth = organ_requires_auth(&profile, state.local_auth_required);
    let authenticated = !requires_auth || snapshot.as_ref().is_some_and(is_connected);
    let username_hint = snapshot
        .as_ref()
        .map(|value| value.username_hint.clone())
        .unwrap_or_default();
    let connected_at_unix = snapshot.as_ref().and_then(|value| value.connected_at_unix);
    let last_error = snapshot
        .as_ref()
        .map(|value| value.last_error.clone())
        .unwrap_or_default();
    let session_state = snapshot
        .take()
        .map(|value| session_state_name(&value).to_string());

    ServerProfileResponse {
        id: profile.id,
        name: profile.name,
        base_url: profile.base_url,
        requires_auth,
        authenticated,
        session_state,
        username_hint,
        connected_at_unix,
        last_error,
    }
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
