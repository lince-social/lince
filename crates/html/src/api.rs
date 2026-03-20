use crate::HtmlState;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    convert::Infallible,
    io::Error,
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast;
use utils::auth::{AuthClaims, decode_jwt, issue_jwt, verify_password};

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    token_type: &'static str,
}

#[derive(Deserialize)]
struct SqlRequest {
    sql: String,
}

#[derive(Serialize)]
struct SqlResponse {
    ok: bool,
    rows_affected: u64,
    changed_tables: BTreeSet<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

pub(crate) fn router() -> Router<Arc<HtmlState>> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/sql", post(execute_sql))
        .route("/sse/view/{view_id}", get(view_sse))
}

async fn login(
    State(state): State<Arc<HtmlState>>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let user = state
        .services
        .repository
        .user
        .get_by_username(&request.username)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("Invalid username or password"))?;

    let is_valid =
        verify_password(&request.password, &user.password_hash).map_err(ApiError::internal)?;
    if !is_valid {
        return Err(ApiError::unauthorized("Invalid username or password"));
    }

    let token = issue_jwt(
        state.jwt_secret.as_str(),
        user.id as u64,
        &user.username,
        Duration::from_secs(60 * 60 * 24),
    )
    .map_err(ApiError::internal)?;

    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer",
    }))
}

async fn execute_sql(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Json(request): Json<SqlRequest>,
) -> Result<Json<SqlResponse>, ApiError> {
    let _claims = authenticate_request(&headers, &state)?;
    let outcome = state
        .services
        .writer
        .execute_sql(request.sql)
        .await
        .map_err(ApiError::bad_request)?;

    Ok(Json(SqlResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
        changed_tables: outcome.changed_tables,
    }))
}

async fn view_sse(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Path(view_id): Path<u32>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let _claims = authenticate_request(&headers, &state)?;
    let dependencies = application::view::view_dependencies(state.services.clone(), view_id)
        .await
        .map_err(ApiError::bad_request)?;
    let snapshot = application::view::view_snapshot(state.services.clone(), view_id)
        .await
        .map_err(ApiError::bad_request)?;
    let last_payload = serde_json::to_string(&snapshot)
        .map_err(|error| ApiError::internal(Error::other(error)))?;
    let invalidation_rx = state.services.writer.subscribe_invalidations();

    let stream = stream::unfold(
        ViewStreamState {
            services: state.services.clone(),
            view_id,
            dependencies,
            invalidation_rx,
            pending_events: VecDeque::from([snapshot_event(last_payload.clone())]),
            last_payload,
        },
        next_view_event,
    );

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

struct ViewStreamState {
    services: injection::cross_cutting::InjectedServices,
    view_id: u32,
    dependencies: BTreeSet<String>,
    invalidation_rx: broadcast::Receiver<persistence::write_coordinator::InvalidationEvent>,
    pending_events: VecDeque<Event>,
    last_payload: String,
}

async fn next_view_event(
    mut state: ViewStreamState,
) -> Option<(Result<Event, Infallible>, ViewStreamState)> {
    loop {
        if let Some(event) = state.pending_events.pop_front() {
            return Some((Ok(event), state));
        }

        match state.invalidation_rx.recv().await {
            Ok(event) => {
                if !event
                    .changed_tables
                    .iter()
                    .any(|table| state.dependencies.contains(table))
                {
                    continue;
                }

                match refresh_view_stream_state(&mut state).await {
                    Ok(Some(event)) => state.pending_events.push_back(event),
                    Ok(None) => {}
                    Err(error_event) => {
                        state.pending_events.push_back(error_event);
                        return Some((Ok(state.pending_events.pop_front().unwrap()), state));
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                match refresh_view_stream_state(&mut state).await {
                    Ok(Some(event)) => state.pending_events.push_back(event),
                    Ok(None) => {}
                    Err(error_event) => {
                        state.pending_events.push_back(error_event);
                        return Some((Ok(state.pending_events.pop_front().unwrap()), state));
                    }
                }
            }
            Err(broadcast::error::RecvError::Closed) => return None,
        }
    }
}

async fn refresh_view_stream_state(state: &mut ViewStreamState) -> Result<Option<Event>, Event> {
    let dependencies = application::view::view_dependencies(state.services.clone(), state.view_id)
        .await
        .map_err(error_event)?;
    let snapshot = application::view::view_snapshot(state.services.clone(), state.view_id)
        .await
        .map_err(error_event)?;
    let payload =
        serde_json::to_string(&snapshot).map_err(|error| error_event(Error::other(error)))?;

    state.dependencies = dependencies;
    if payload == state.last_payload {
        return Ok(None);
    }

    state.last_payload = payload.clone();
    Ok(Some(snapshot_event(payload)))
}

fn snapshot_event(payload: String) -> Event {
    Event::default().event("snapshot").data(payload)
}

fn error_event(error: Error) -> Event {
    let body = serde_json::to_string(&ErrorResponse {
        error: error.to_string(),
    })
    .unwrap_or_else(|_| "{\"error\":\"failed to serialize error\"}".to_string());

    Event::default().event("error").data(body)
}

fn authenticate_request(
    headers: &HeaderMap,
    state: &Arc<HtmlState>,
) -> Result<AuthClaims, ApiError> {
    let header_value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| ApiError::unauthorized("Missing Authorization header"))?;
    let header_str = header_value
        .to_str()
        .map_err(|_| ApiError::unauthorized("Invalid Authorization header"))?;
    let token = header_str
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::unauthorized("Expected Bearer token"))?;

    decode_jwt(state.jwt_secret.as_str(), token).map_err(ApiError::unauthorized_err)
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn unauthorized_err(error: Error) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: error.to_string(),
        }
    }

    fn bad_request(error: Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    fn internal(error: Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}
