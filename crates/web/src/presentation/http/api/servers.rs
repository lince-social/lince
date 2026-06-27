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
    utils::logging::{LogEntry, log},
    utoipa::ToSchema,
};

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfileResponse {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub trust_state: String,
    pub contact_discovery_enabled: bool,
    pub last_seen_at: Option<String>,
    pub last_transfer_polled_at: Option<String>,
    pub requires_auth: bool,
    pub authenticated: bool,
    pub session_state: Option<String>,
    pub username_hint: String,
    pub connected_at_unix: Option<u64>,
    pub last_error: String,
    pub sync_resources: Vec<String>,
    pub record_sync_mode: String,
    pub file_sync_enabled: bool,
    pub file_sync_path: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ServerLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertServerProfileRequest {
    pub name: String,
    pub base_url: String,
    pub trust_state: Option<String>,
    pub contact_discovery_enabled: Option<bool>,
    pub record_sync_mode: Option<String>,
    pub file_sync_enabled: Option<bool>,
    pub file_sync_path: Option<String>,
}

#[utoipa::path(
    get,
    path = "/organ",
    tag = "organ",
    responses(
        (status = 200, description = "List organ profiles", body = [ServerProfileResponse]),
        (status = 502, description = "Backend failure", body = crate::presentation::http::api_error::ApiError)
    )
)]
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

    let mut response = Vec::with_capacity(servers.len());
    for server in servers {
        let status = statuses.get(&server.id.to_string());
        let requires_auth = organ_requires_auth(&server, state.local_auth_required);
        let authenticated = !requires_auth || status.is_some_and(is_connected);
        let (sync_resources, record_sync_mode) = load_sync_policy(&state, server.id).await?;
        response.push(ServerProfileResponse {
            id: server.id,
            name: server.name,
            base_url: server.base_url,
            trust_state: server.trust_state,
            contact_discovery_enabled: server.contact_discovery_enabled != 0,
            last_seen_at: server.last_seen_at,
            last_transfer_polled_at: server.last_transfer_polled_at,
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
            sync_resources,
            record_sync_mode,
            file_sync_enabled: server.file_sync_enabled != 0,
            file_sync_path: server.file_sync_path,
        });
    }

    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/organ",
    tag = "organ",
    request_body = UpsertServerProfileRequest,
    responses(
        (status = 200, description = "Created organ profile", body = ServerProfileResponse),
        (status = 400, description = "Invalid profile", body = crate::presentation::http::api_error::ApiError)
    )
)]
pub async fn create_server(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpsertServerProfileRequest>,
) -> ApiResult<Json<ServerProfileResponse>> {
    let profile = state
        .organs
        .create(payload.name, payload.base_url)
        .await
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    if let Some(trust_state) = payload.trust_state.as_deref() {
        state
            .organs
            .set_trust_state(profile.id, trust_state)
            .await
            .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    }
    if let Some(enabled) = payload.contact_discovery_enabled {
        state
            .organs
            .set_contact_discovery_enabled(profile.id, enabled)
            .await
            .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    }
    let mode = payload.record_sync_mode.as_deref().unwrap_or("none");
    state
        .organs
        .set_record_sync_policy(profile.id, mode)
        .await
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    if payload.file_sync_enabled.is_some() || payload.file_sync_path.is_some() {
        state
            .organs
            .set_file_sync(
                profile.id,
                payload.file_sync_enabled.unwrap_or(false),
                payload.file_sync_path.as_deref(),
            )
            .await
            .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
        ::application::file_sync::configure_from_organs(state.services.clone())
            .await
            .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
        ::application::file_sync::sync_after_record_change(state.services.clone())
            .await
            .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
    }
    let profile = state
        .organs
        .get(profile.id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
        .unwrap_or(profile);

    Ok(Json(
        server_profile_response(&state, &headers, profile).await,
    ))
}

#[utoipa::path(
    post,
    path = "/organ/{server_id}/session",
    tag = "organ",
    params(("server_id" = String, Path, description = "Organ identifier")),
    request_body = ServerLoginRequest,
    responses(
        (status = 200, description = "Login successful", body = ServerProfileResponse),
        (status = 400, description = "Invalid login request", body = crate::presentation::http::api_error::ApiError),
        (status = 401, description = "Login rejected", body = crate::presentation::http::api_error::ApiError)
    )
)]
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
            server.id,
            username.to_string(),
            bearer_token.clone(),
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    let sync_state = state.clone();
    let sync_server_id = server.id;
    tokio::spawn(async move {
        if let Err(error) =
            super::sync::run_record_sync_for_organ(sync_state, sync_server_id, bearer_token).await
        {
            tracing::warn!(
                organ_id = sync_server_id,
                error = %error,
                "record sync: login-triggered sync failed"
            );
        }
    });
    state
        .services
        .notifications
        .dismiss(&format!("organ-login-required-{}", server.id));
    log(LogEntry::Info(format!(
        "record sync: spawning login-triggered sync organ_id={}",
        server.id
    )));

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
        .remove(&server.id.to_string());
    let (sync_resources, record_sync_mode) = load_sync_policy(&state, server.id)
        .await
        .unwrap_or_else(|_| (Vec::new(), "none".to_string()));

    Ok((
        response_headers,
        Json(ServerProfileResponse {
            id: server.id,
            name: server.name,
            base_url: server.base_url,
            trust_state: server.trust_state,
            contact_discovery_enabled: server.contact_discovery_enabled != 0,
            last_seen_at: server.last_seen_at,
            last_transfer_polled_at: server.last_transfer_polled_at,
            requires_auth: true,
            authenticated: true,
            session_state: Some("connected".to_string()),
            username_hint: username.to_string(),
            connected_at_unix: snapshot.and_then(|value| value.connected_at_unix),
            last_error: String::new(),
            sync_resources,
            record_sync_mode,
            file_sync_enabled: server.file_sync_enabled != 0,
            file_sync_path: server.file_sync_path,
        }),
    ))
}

#[utoipa::path(
    delete,
    path = "/organ/{server_id}/session",
    tag = "organ",
    params(("server_id" = String, Path, description = "Organ identifier")),
    responses(
        (status = 204, description = "Session cleared")
    )
)]
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

#[utoipa::path(
    patch,
    path = "/organ/{server_id}",
    tag = "organ",
    params(("server_id" = String, Path, description = "Organ identifier")),
    request_body = UpsertServerProfileRequest,
    responses(
        (status = 200, description = "Updated organ profile", body = ServerProfileResponse),
        (status = 400, description = "Invalid profile", body = crate::presentation::http::api_error::ApiError)
    )
)]
pub async fn update_server(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(payload): Json<UpsertServerProfileRequest>,
) -> ApiResult<Json<ServerProfileResponse>> {
    let _existing = load_organ(&state, &server_id).await?;
    let profile = state
        .organs
        .update(server_id, payload.name, payload.base_url)
        .await
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    if let Some(trust_state) = payload.trust_state.as_deref() {
        state
            .organs
            .set_trust_state(profile.id, trust_state)
            .await
            .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    }
    if let Some(enabled) = payload.contact_discovery_enabled {
        state
            .organs
            .set_contact_discovery_enabled(profile.id, enabled)
            .await
            .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    }
    if let Some(mode) = payload.record_sync_mode.as_deref() {
        state
            .organs
            .set_record_sync_policy(profile.id, mode)
            .await
            .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
        let session_token = parse_cookie_header(
            headers
                .get(header::COOKIE)
                .and_then(|value| value.to_str().ok()),
            session_cookie_name(),
        );
        if let Some(session) = state.auth.server_session(session_token.as_deref(), profile.id).await
        {
            let sync_state = state.clone();
            let sync_server_id = profile.id;
            tokio::spawn(async move {
                if let Err(error) = super::sync::run_record_sync_for_organ(
                    sync_state,
                    sync_server_id,
                    session.bearer_token,
                )
                .await
                {
                    tracing::warn!(
                        organ_id = sync_server_id,
                        error = %error,
                        "record sync: policy-triggered sync failed"
                    );
                }
            });
        }
    }
    if payload.file_sync_enabled.is_some() || payload.file_sync_path.is_some() {
        let existing = state
            .organs
            .get(profile.id)
            .await
            .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
            .unwrap_or(profile.clone());
        state
            .organs
            .set_file_sync(
                profile.id,
                payload
                    .file_sync_enabled
                    .unwrap_or(existing.file_sync_enabled != 0),
                payload
                    .file_sync_path
                    .as_deref()
                    .or(existing.file_sync_path.as_deref()),
            )
            .await
            .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
        ::application::file_sync::configure_from_organs(state.services.clone())
            .await
            .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
        ::application::file_sync::sync_after_record_change(state.services.clone())
            .await
            .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
    }
    let profile = state
        .organs
        .get(profile.id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
        .unwrap_or(profile);

    Ok(Json(
        server_profile_response(&state, &headers, profile).await,
    ))
}

#[utoipa::path(
    delete,
    path = "/organ/{server_id}",
    tag = "organ",
    params(("server_id" = String, Path, description = "Organ identifier")),
    responses(
        (status = 204, description = "Deleted organ"),
        (status = 404, description = "Organ not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
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
        .remove(&profile.id.to_string());
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
    let (sync_resources, record_sync_mode) = load_sync_policy(state, profile.id)
        .await
        .unwrap_or_else(|_| (Vec::new(), "none".to_string()));

    ServerProfileResponse {
        id: profile.id,
        name: profile.name,
        base_url: profile.base_url,
        trust_state: profile.trust_state,
        contact_discovery_enabled: profile.contact_discovery_enabled != 0,
        last_seen_at: profile.last_seen_at,
        last_transfer_polled_at: profile.last_transfer_polled_at,
        requires_auth,
        authenticated,
        session_state,
        username_hint,
        connected_at_unix,
        last_error,
        sync_resources,
        record_sync_mode,
        file_sync_enabled: profile.file_sync_enabled != 0,
        file_sync_path: profile.file_sync_path,
    }
}

pub(crate) async fn load_sync_policy(
    state: &AppState,
    organ_id: i64,
) -> ApiResult<(Vec<String>, String)> {
    let Some(policy) = state
        .organs
        .get_sync_policy(organ_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
    else {
        return Ok((Vec::new(), "none".to_string()));
    };
    let resources = serde_json::from_str::<Vec<String>>(&policy.sync_resources)
        .unwrap_or_default()
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    Ok((resources, policy.record_sync_mode))
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
