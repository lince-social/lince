use crate::HtmlState;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures::stream::{self, Stream};
use persistence::write_coordinator::SqlParameter;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{FromRow, Pool, Sqlite};
use std::{
    collections::{BTreeSet, VecDeque},
    convert::Infallible,
    io::{Error, ErrorKind},
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast;
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};
use utils::auth::{AuthClaims, decode_jwt, issue_jwt, verify_password};
use utils::file_access::{
    FileAccessAction, FileAccessClaims, decode_file_access_token, issue_file_access_token,
};

const FILE_LINK_TTL_SECS: u64 = 300;
const MAX_FILE_UPLOAD_BYTES: usize = 1024 * 1024 * 1024;

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

#[derive(Serialize)]
struct MutationResponse {
    ok: bool,
    rows_affected: u64,
}

#[derive(Deserialize)]
struct FileListQuery {
    prefix: Option<String>,
    limit: Option<i32>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct FileKeyRequest {
    key: String,
}

#[derive(Serialize)]
struct FileLinkResponse {
    method: &'static str,
    url: String,
    expires_in: u64,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiTable {
    View,
    Record,
    Frequency,
    KarmaCondition,
    KarmaConsequence,
    Karma,
    Configuration,
    AppUser,
    Role,
}

#[derive(Debug, Clone, Copy)]
enum FieldKind {
    Text,
    NullableText,
    Integer,
    NullableInteger,
    Real,
    NullableReal,
    BooleanInteger,
}

#[derive(Debug, Clone, Copy)]
struct FieldSpec {
    name: &'static str,
    kind: FieldKind,
}

#[derive(Debug, Serialize, FromRow)]
struct ViewRow {
    id: i64,
    name: String,
    query: String,
}

#[derive(Debug, Serialize, FromRow)]
struct RecordRow {
    id: i64,
    quantity: f64,
    head: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
struct FrequencyRow {
    id: i64,
    quantity: f64,
    name: String,
    day_week: Option<f64>,
    months: f64,
    days: f64,
    seconds: f64,
    next_date: String,
    finish_date: Option<String>,
    catch_up_sum: i64,
}

#[derive(Debug, Serialize, FromRow)]
struct KarmaConditionRow {
    id: i64,
    quantity: i64,
    name: String,
    condition: String,
}

#[derive(Debug, Serialize, FromRow)]
struct KarmaConsequenceRow {
    id: i64,
    quantity: i64,
    name: String,
    consequence: String,
}

#[derive(Debug, Serialize, FromRow)]
struct KarmaRow {
    id: i64,
    quantity: i64,
    name: String,
    condition_id: i64,
    operator: String,
    consequence_id: i64,
}

#[derive(Debug, Serialize, FromRow)]
struct ConfigurationRow {
    id: i64,
    quantity: Option<i64>,
    name: String,
    language: Option<String>,
    timezone: Option<i64>,
    style: Option<String>,
    show_command_notifications: i64,
    command_notification_seconds: f64,
    delete_confirmation: i64,
    error_toast_seconds: f64,
    keybinding_mode: i64,
}

#[derive(Debug, Serialize, FromRow)]
struct PublicAppUserRow {
    id: i64,
    name: String,
    username: String,
    role_id: i64,
    role: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, FromRow)]
struct RoleRow {
    id: i64,
    name: String,
}

pub(crate) fn router() -> Router<Arc<HtmlState>> {
    Router::new()
        .route("/auth/login", post(login))
        .route(
            "/table/{table}",
            get(list_table_rows).post(create_table_row),
        )
        .route(
            "/table/{table}/{id}",
            get(get_table_row)
                .patch(update_table_row)
                .delete(delete_table_row),
        )
        .route("/files", get(list_files))
        .route("/files/upload-link", post(upload_link))
        .route("/files/download-link", post(download_link))
        .route("/files/delete-link", post(delete_link))
        .route(
            "/files/access/{token}",
            get(download_via_link)
                .put(upload_via_link)
                .delete(delete_via_link),
        )
        .route("/sse/view/{view_id}", get(view_sse))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        )
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
        user.role_id as u64,
        &user.role,
        Duration::from_secs(60 * 60 * 24),
    )
    .map_err(ApiError::internal)?;

    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer",
    }))
}

async fn list_table_rows(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Path(table_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _claims = authenticate_request(&headers, &state)?;
    let table = parse_api_table(&table_name)?;
    let db = &*state.services.db;

    let value = match table {
        ApiTable::View => serialize_value(
            sqlx::query_as::<_, ViewRow>("SELECT id, name, query FROM view ORDER BY id")
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
        )?,
        ApiTable::Record => serialize_value(
            sqlx::query_as::<_, RecordRow>(
                "SELECT id, quantity, head, body FROM record ORDER BY id",
            )
            .fetch_all(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::Frequency => serialize_value(
            sqlx::query_as::<_, FrequencyRow>(
                "SELECT id, quantity, name, day_week, months, days, seconds, next_date, finish_date, catch_up_sum FROM frequency ORDER BY id",
            )
            .fetch_all(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::KarmaCondition => serialize_value(
            sqlx::query_as::<_, KarmaConditionRow>(
                "SELECT id, quantity, name, condition FROM karma_condition ORDER BY id",
            )
            .fetch_all(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::KarmaConsequence => serialize_value(
            sqlx::query_as::<_, KarmaConsequenceRow>(
                "SELECT id, quantity, name, consequence FROM karma_consequence ORDER BY id",
            )
            .fetch_all(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::Karma => serialize_value(
            sqlx::query_as::<_, KarmaRow>(
                "SELECT id, quantity, name, condition_id, operator, consequence_id FROM karma ORDER BY id",
            )
            .fetch_all(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::Configuration => serialize_value(
            sqlx::query_as::<_, ConfigurationRow>(
                "SELECT id, quantity, name, language, timezone, style, show_command_notifications, command_notification_seconds, delete_confirmation, error_toast_seconds, keybinding_mode FROM configuration ORDER BY id",
            )
            .fetch_all(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::AppUser => serialize_value(fetch_public_users(db).await?)?,
        ApiTable::Role => serialize_value(
            sqlx::query_as::<_, RoleRow>("SELECT id, name FROM role ORDER BY id")
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
        )?,
    };

    Ok(Json(value))
}

async fn get_table_row(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let _claims = authenticate_request(&headers, &state)?;
    let table = parse_api_table(&table_name)?;
    let db = &*state.services.db;

    let value = match table {
        ApiTable::View => serialize_value(
            sqlx::query_as::<_, ViewRow>("SELECT id, name, query FROM view WHERE id = ?")
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
        )?,
        ApiTable::Record => serialize_value(
            sqlx::query_as::<_, RecordRow>(
                "SELECT id, quantity, head, body FROM record WHERE id = ?",
            )
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::Frequency => serialize_value(
            sqlx::query_as::<_, FrequencyRow>(
                "SELECT id, quantity, name, day_week, months, days, seconds, next_date, finish_date, catch_up_sum FROM frequency WHERE id = ?",
            )
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::KarmaCondition => serialize_value(
            sqlx::query_as::<_, KarmaConditionRow>(
                "SELECT id, quantity, name, condition FROM karma_condition WHERE id = ?",
            )
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::KarmaConsequence => serialize_value(
            sqlx::query_as::<_, KarmaConsequenceRow>(
                "SELECT id, quantity, name, consequence FROM karma_consequence WHERE id = ?",
            )
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::Karma => serialize_value(
            sqlx::query_as::<_, KarmaRow>(
                "SELECT id, quantity, name, condition_id, operator, consequence_id FROM karma WHERE id = ?",
            )
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::Configuration => serialize_value(
            sqlx::query_as::<_, ConfigurationRow>(
                "SELECT id, quantity, name, language, timezone, style, show_command_notifications, command_notification_seconds, delete_confirmation, error_toast_seconds, keybinding_mode FROM configuration WHERE id = ?",
            )
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(map_sqlx_error)?,
        )?,
        ApiTable::AppUser => serialize_value(fetch_public_user_by_id(db, id).await?)?,
        ApiTable::Role => serialize_value(
            sqlx::query_as::<_, RoleRow>("SELECT id, name FROM role WHERE id = ?")
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
        )?,
    };

    Ok(Json(value))
}

async fn create_table_row(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Path(table_name): Path<String>,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<MutationResponse>), ApiError> {
    let claims = authenticate_request(&headers, &state)?;
    let table = parse_api_table(&table_name)?;
    let object = payload_object(&payload)?;

    let outcome = match table {
        ApiTable::View
        | ApiTable::Record
        | ApiTable::Frequency
        | ApiTable::KarmaCondition
        | ApiTable::KarmaConsequence
        | ApiTable::Karma
        | ApiTable::Configuration => {
            let (sql, params) = build_standard_insert(table, object)?;
            state
                .services
                .writer
                .execute_statement(sql, params)
                .await
                .map_err(ApiError::from_write)?
        }
        ApiTable::AppUser => {
            require_admin(&claims)?;
            let (sql, params) = build_app_user_insert(&state, object).await?;
            state
                .services
                .writer
                .execute_statement(sql, params)
                .await
                .map_err(ApiError::from_write)?
        }
        ApiTable::Role => {
            require_admin(&claims)?;
            let (sql, params) = build_role_insert(object)?;
            state
                .services
                .writer
                .execute_statement(sql, params)
                .await
                .map_err(ApiError::from_write)?
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(MutationResponse {
            ok: true,
            rows_affected: outcome.rows_affected,
        }),
    ))
}

async fn update_table_row(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
    Json(payload): Json<Value>,
) -> Result<Json<MutationResponse>, ApiError> {
    let claims = authenticate_request(&headers, &state)?;
    let table = parse_api_table(&table_name)?;
    let object = payload_object(&payload)?;

    let outcome = match table {
        ApiTable::View
        | ApiTable::Record
        | ApiTable::Frequency
        | ApiTable::KarmaCondition
        | ApiTable::KarmaConsequence
        | ApiTable::Karma
        | ApiTable::Configuration => {
            let (sql, params) = build_standard_update(table, id, object)?;
            state
                .services
                .writer
                .execute_statement(sql, params)
                .await
                .map_err(ApiError::from_write)?
        }
        ApiTable::AppUser => {
            ensure_self_or_admin(&claims, id)?;
            let (sql, params) = build_app_user_update(&state, &claims, id, object).await?;
            state
                .services
                .writer
                .execute_statement(sql, params)
                .await
                .map_err(ApiError::from_write)?
        }
        ApiTable::Role => {
            require_admin(&claims)?;
            let (sql, params) = build_role_update(id, object)?;
            state
                .services
                .writer
                .execute_statement(sql, params)
                .await
                .map_err(ApiError::from_write)?
        }
    };

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
    }))
}

async fn delete_table_row(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
) -> Result<Json<MutationResponse>, ApiError> {
    let claims = authenticate_request(&headers, &state)?;
    let table = parse_api_table(&table_name)?;

    match table {
        ApiTable::AppUser => ensure_self_or_admin(&claims, id)?,
        ApiTable::Role => require_admin(&claims)?,
        _ => {}
    }

    let outcome = state
        .services
        .writer
        .execute_statement(
            format!("DELETE FROM {} WHERE id = ?", table.as_table_name()),
            vec![persistence::write_coordinator::SqlParameter::Integer(id)],
        )
        .await
        .map_err(ApiError::from_write)?;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
    }))
}

async fn list_files(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Query(query): Query<FileListQuery>,
) -> Result<Json<persistence::storage::StorageList>, ApiError> {
    let _claims = authenticate_request(&headers, &state)?;
    let listing = state
        .services
        .storage
        .list_objects(
            query.prefix.as_deref(),
            query.limit.unwrap_or(100),
            query.cursor.as_deref(),
        )
        .await
        .map_err(ApiError::from_io)?;

    Ok(Json(listing))
}

async fn upload_link(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> Result<Json<FileLinkResponse>, ApiError> {
    let _claims = authenticate_request(&headers, &state)?;
    let key = validate_file_key(&request.key)?;
    Ok(Json(issue_file_link(
        state.jwt_secret.as_str(),
        &key,
        FileAccessAction::Upload,
    )?))
}

async fn download_link(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> Result<Json<FileLinkResponse>, ApiError> {
    let _claims = authenticate_request(&headers, &state)?;
    let key = validate_file_key(&request.key)?;
    Ok(Json(issue_file_link(
        state.jwt_secret.as_str(),
        &key,
        FileAccessAction::Download,
    )?))
}

async fn delete_link(
    State(state): State<Arc<HtmlState>>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> Result<Json<FileLinkResponse>, ApiError> {
    let _claims = authenticate_request(&headers, &state)?;
    let key = validate_file_key(&request.key)?;
    Ok(Json(issue_file_link(
        state.jwt_secret.as_str(),
        &key,
        FileAccessAction::Delete,
    )?))
}

async fn upload_via_link(
    State(state): State<Arc<HtmlState>>,
    Path(token): Path<String>,
    request: Request,
) -> Result<StatusCode, ApiError> {
    let claims = authenticate_file_access(&state, &token, FileAccessAction::Upload)?;
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = to_bytes(request.into_body(), MAX_FILE_UPLOAD_BYTES)
        .await
        .map_err(|error| ApiError::bad_request(Error::other(error)))?;

    state
        .services
        .storage
        .upload_object(&claims.key, body.to_vec(), content_type.as_deref())
        .await
        .map_err(ApiError::from_io)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn download_via_link(
    State(state): State<Arc<HtmlState>>,
    Path(token): Path<String>,
) -> Result<Response, ApiError> {
    let claims = authenticate_file_access(&state, &token, FileAccessAction::Download)?;
    let object = state
        .services
        .storage
        .download_object(&claims.key)
        .await
        .map_err(ApiError::from_io)?;

    let mut response = Response::new(Body::from_stream(ReaderStream::new(
        object.body.into_async_read(),
    )));
    if let Some(content_type) = object.content_type
        && let Ok(value) = HeaderValue::from_str(&content_type)
    {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    if let Some(content_length) = object.content_length
        && let Ok(value) = HeaderValue::from_str(&content_length.to_string())
    {
        response.headers_mut().insert(header::CONTENT_LENGTH, value);
    }
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        object.filename.replace('"', "_")
    )) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }

    Ok(response)
}

async fn delete_via_link(
    State(state): State<Arc<HtmlState>>,
    Path(token): Path<String>,
) -> Result<StatusCode, ApiError> {
    let claims = authenticate_file_access(&state, &token, FileAccessAction::Delete)?;
    state
        .services
        .storage
        .delete_object(&claims.key)
        .await
        .map_err(ApiError::from_io)?;
    Ok(StatusCode::NO_CONTENT)
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

const VIEW_FIELD_SPECS: [FieldSpec; 2] = [
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "query",
        kind: FieldKind::Text,
    },
];

const RECORD_FIELD_SPECS: [FieldSpec; 3] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "head",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "body",
        kind: FieldKind::NullableText,
    },
];

const FREQUENCY_FIELD_SPECS: [FieldSpec; 8] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "day_week",
        kind: FieldKind::NullableReal,
    },
    FieldSpec {
        name: "months",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "days",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "seconds",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "next_date",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "finish_date",
        kind: FieldKind::NullableText,
    },
];

const FREQUENCY_EXTRA_FIELD_SPECS: [FieldSpec; 1] = [FieldSpec {
    name: "catch_up_sum",
    kind: FieldKind::Integer,
}];

const KARMA_CONDITION_FIELD_SPECS: [FieldSpec; 3] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "condition",
        kind: FieldKind::Text,
    },
];

const KARMA_CONSEQUENCE_FIELD_SPECS: [FieldSpec; 3] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "consequence",
        kind: FieldKind::Text,
    },
];

const KARMA_FIELD_SPECS: [FieldSpec; 5] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "condition_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "operator",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "consequence_id",
        kind: FieldKind::Integer,
    },
];

const CONFIGURATION_FIELD_SPECS: [FieldSpec; 10] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::NullableInteger,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "language",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "timezone",
        kind: FieldKind::NullableInteger,
    },
    FieldSpec {
        name: "style",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "show_command_notifications",
        kind: FieldKind::BooleanInteger,
    },
    FieldSpec {
        name: "command_notification_seconds",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "delete_confirmation",
        kind: FieldKind::BooleanInteger,
    },
    FieldSpec {
        name: "error_toast_seconds",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "keybinding_mode",
        kind: FieldKind::Integer,
    },
];

const ROLE_FIELD_SPECS: [FieldSpec; 1] = [FieldSpec {
    name: "name",
    kind: FieldKind::Text,
}];

impl ApiTable {
    fn as_table_name(self) -> &'static str {
        match self {
            ApiTable::View => "view",
            ApiTable::Record => "record",
            ApiTable::Frequency => "frequency",
            ApiTable::KarmaCondition => "karma_condition",
            ApiTable::KarmaConsequence => "karma_consequence",
            ApiTable::Karma => "karma",
            ApiTable::Configuration => "configuration",
            ApiTable::AppUser => "app_user",
            ApiTable::Role => "role",
        }
    }

    fn standard_field_specs(self) -> Option<Vec<FieldSpec>> {
        match self {
            ApiTable::View => Some(VIEW_FIELD_SPECS.to_vec()),
            ApiTable::Record => Some(RECORD_FIELD_SPECS.to_vec()),
            ApiTable::Frequency => Some(
                FREQUENCY_FIELD_SPECS
                    .into_iter()
                    .chain(FREQUENCY_EXTRA_FIELD_SPECS)
                    .collect(),
            ),
            ApiTable::KarmaCondition => Some(KARMA_CONDITION_FIELD_SPECS.to_vec()),
            ApiTable::KarmaConsequence => Some(KARMA_CONSEQUENCE_FIELD_SPECS.to_vec()),
            ApiTable::Karma => Some(KARMA_FIELD_SPECS.to_vec()),
            ApiTable::Configuration => Some(CONFIGURATION_FIELD_SPECS.to_vec()),
            ApiTable::AppUser | ApiTable::Role => None,
        }
    }
}

fn parse_api_table(table_name: &str) -> Result<ApiTable, ApiError> {
    match table_name {
        "view" => Ok(ApiTable::View),
        "record" => Ok(ApiTable::Record),
        "frequency" => Ok(ApiTable::Frequency),
        "karma_condition" => Ok(ApiTable::KarmaCondition),
        "karma_consequence" => Ok(ApiTable::KarmaConsequence),
        "karma" => Ok(ApiTable::Karma),
        "configuration" => Ok(ApiTable::Configuration),
        "app_user" => Ok(ApiTable::AppUser),
        "role" => Ok(ApiTable::Role),
        _ => Err(ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            format!("Unsupported API table: {table_name}"),
        ))),
    }
}

fn payload_object(payload: &Value) -> Result<&Map<String, Value>, ApiError> {
    payload.as_object().ok_or_else(|| {
        ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            "Expected a JSON object payload",
        ))
    })
}

fn serialize_value<T: Serialize>(value: T) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|error| ApiError::internal(Error::other(error)))
}

async fn fetch_public_users(db: &Pool<Sqlite>) -> Result<Vec<PublicAppUserRow>, ApiError> {
    sqlx::query_as::<_, PublicAppUserRow>(
        "
        SELECT
            u.id,
            u.name,
            u.username,
            u.role_id,
            r.name AS role,
            u.created_at,
            u.updated_at
        FROM app_user u
        JOIN role r ON r.id = u.role_id
        ORDER BY u.id
        ",
    )
    .fetch_all(db)
    .await
    .map_err(map_sqlx_error)
}

async fn fetch_public_user_by_id(db: &Pool<Sqlite>, id: i64) -> Result<PublicAppUserRow, ApiError> {
    sqlx::query_as::<_, PublicAppUserRow>(
        "
        SELECT
            u.id,
            u.name,
            u.username,
            u.role_id,
            r.name AS role,
            u.created_at,
            u.updated_at
        FROM app_user u
        JOIN role r ON r.id = u.role_id
        WHERE u.id = ?
        ",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(map_sqlx_error)
}

fn map_sqlx_error(error: sqlx::Error) -> ApiError {
    match error {
        sqlx::Error::RowNotFound => ApiError::not_found("Row not found"),
        other => ApiError::internal(Error::other(other)),
    }
}

fn require_admin(claims: &AuthClaims) -> Result<(), ApiError> {
    if claims.role == "admin" {
        Ok(())
    } else {
        Err(ApiError::forbidden("Admin role required"))
    }
}

fn ensure_self_or_admin(claims: &AuthClaims, id: i64) -> Result<(), ApiError> {
    if claims.role == "admin" || claims.sub as i64 == id {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "You may only modify your own user unless you are an admin",
        ))
    }
}

fn build_standard_insert(
    table: ApiTable,
    object: &Map<String, Value>,
) -> Result<(String, Vec<SqlParameter>), ApiError> {
    reject_common_forbidden_fields(object)?;
    let specs = table
        .standard_field_specs()
        .ok_or_else(|| ApiError::internal(Error::other("Missing field specs")))?;
    let fields = parse_fields(object, &specs)?;

    if fields.is_empty() {
        return Ok((
            format!("INSERT INTO {} DEFAULT VALUES", table.as_table_name()),
            vec![],
        ));
    }

    let columns = fields
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = vec!["?"; fields.len()].join(", ");
    let params = fields
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();

    Ok((
        format!(
            "INSERT INTO {} ({columns}) VALUES ({placeholders})",
            table.as_table_name()
        ),
        params,
    ))
}

fn build_standard_update(
    table: ApiTable,
    id: i64,
    object: &Map<String, Value>,
) -> Result<(String, Vec<SqlParameter>), ApiError> {
    reject_common_forbidden_fields(object)?;
    let specs = table
        .standard_field_specs()
        .ok_or_else(|| ApiError::internal(Error::other("Missing field specs")))?;
    let fields = parse_fields(object, &specs)?;
    if fields.is_empty() {
        return Err(ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            "At least one writable field is required",
        )));
    }

    let assignments = fields
        .iter()
        .map(|(name, _)| format!("{name} = ?"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut params = fields
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    params.push(SqlParameter::Integer(id));

    Ok((
        format!(
            "UPDATE {} SET {assignments} WHERE id = ?",
            table.as_table_name()
        ),
        params,
    ))
}

async fn build_app_user_insert(
    state: &Arc<HtmlState>,
    object: &Map<String, Value>,
) -> Result<(String, Vec<SqlParameter>), ApiError> {
    reject_user_forbidden_fields(object)?;

    let name = required_text_field(object, "name")?;
    let username = required_text_field(object, "username")?;
    let password = required_text_field(object, "password")?;
    let password_hash = utils::auth::hash_password(&password).map_err(ApiError::internal)?;

    let role_id = if let Some(value) = object.get("role_id") {
        let role_id = parse_i64_value("role_id", value)?;
        ensure_role_exists(&state.services.db, role_id).await?;
        role_id
    } else {
        role_id_by_name(&state.services.db, "lince").await?
    };

    reject_unknown_fields(object, &["name", "username", "password", "role_id"])?;

    Ok((
        "INSERT INTO app_user(name, username, password_hash, role_id) VALUES (?, ?, ?, ?)"
            .to_string(),
        vec![
            SqlParameter::Text(name),
            SqlParameter::Text(username),
            SqlParameter::Text(password_hash),
            SqlParameter::Integer(role_id),
        ],
    ))
}

async fn build_app_user_update(
    state: &Arc<HtmlState>,
    claims: &AuthClaims,
    id: i64,
    object: &Map<String, Value>,
) -> Result<(String, Vec<SqlParameter>), ApiError> {
    reject_user_forbidden_fields(object)?;
    reject_unknown_fields(object, &["name", "username", "password", "role_id"])?;

    let mut assignments = Vec::new();
    let mut params = Vec::new();

    if let Some(value) = object.get("name") {
        assignments.push("name = ?".to_string());
        params.push(parse_text_parameter("name", value)?);
    }
    if let Some(value) = object.get("username") {
        assignments.push("username = ?".to_string());
        params.push(parse_text_parameter("username", value)?);
    }
    if let Some(value) = object.get("password") {
        let password = parse_text_value("password", value)?;
        let password_hash = utils::auth::hash_password(&password).map_err(ApiError::internal)?;
        assignments.push("password_hash = ?".to_string());
        params.push(SqlParameter::Text(password_hash));
    }
    if let Some(value) = object.get("role_id") {
        require_admin(claims)?;
        let role_id = parse_i64_value("role_id", value)?;
        ensure_role_exists(&state.services.db, role_id).await?;
        assignments.push("role_id = ?".to_string());
        params.push(SqlParameter::Integer(role_id));
    }

    if assignments.is_empty() {
        return Err(ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            "At least one writable field is required",
        )));
    }

    assignments.push("updated_at = CURRENT_TIMESTAMP".to_string());
    params.push(SqlParameter::Integer(id));

    Ok((
        format!(
            "UPDATE app_user SET {} WHERE id = ?",
            assignments.join(", ")
        ),
        params,
    ))
}

fn build_role_insert(object: &Map<String, Value>) -> Result<(String, Vec<SqlParameter>), ApiError> {
    reject_common_forbidden_fields(object)?;
    let fields = parse_fields(object, &ROLE_FIELD_SPECS)?;
    if fields.is_empty() {
        return Err(ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            "At least one writable field is required",
        )));
    }

    let columns = fields
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = vec!["?"; fields.len()].join(", ");
    let params = fields
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();

    Ok((
        format!("INSERT INTO role ({columns}) VALUES ({placeholders})"),
        params,
    ))
}

fn build_role_update(
    id: i64,
    object: &Map<String, Value>,
) -> Result<(String, Vec<SqlParameter>), ApiError> {
    reject_common_forbidden_fields(object)?;
    let fields = parse_fields(object, &ROLE_FIELD_SPECS)?;
    if fields.is_empty() {
        return Err(ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            "At least one writable field is required",
        )));
    }

    let assignments = fields
        .iter()
        .map(|(name, _)| format!("{name} = ?"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut params = fields
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    params.push(SqlParameter::Integer(id));

    Ok((
        format!("UPDATE role SET {assignments} WHERE id = ?"),
        params,
    ))
}

fn reject_common_forbidden_fields(object: &Map<String, Value>) -> Result<(), ApiError> {
    if object.contains_key("id") {
        return Err(ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            "The id field is not writable",
        )));
    }

    Ok(())
}

fn reject_user_forbidden_fields(object: &Map<String, Value>) -> Result<(), ApiError> {
    reject_common_forbidden_fields(object)?;
    for forbidden in ["password_hash", "role"] {
        if object.contains_key(forbidden) {
            return Err(ApiError::bad_request(Error::new(
                ErrorKind::InvalidInput,
                format!("The {forbidden} field is not writable"),
            )));
        }
    }
    Ok(())
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed_fields: &[&str],
) -> Result<(), ApiError> {
    for key in object.keys() {
        if !allowed_fields.iter().any(|allowed| allowed == key) {
            return Err(ApiError::bad_request(Error::new(
                ErrorKind::InvalidInput,
                format!("Unknown or non-writable field: {key}"),
            )));
        }
    }
    Ok(())
}

fn parse_fields(
    object: &Map<String, Value>,
    specs: &[FieldSpec],
) -> Result<Vec<(String, SqlParameter)>, ApiError> {
    let mut parsed = Vec::new();
    for (key, value) in object {
        if key == "id" {
            return Err(ApiError::bad_request(Error::new(
                ErrorKind::InvalidInput,
                "The id field is not writable",
            )));
        }

        let spec = specs.iter().find(|spec| spec.name == key).ok_or_else(|| {
            ApiError::bad_request(Error::new(
                ErrorKind::InvalidInput,
                format!("Unknown or non-writable field: {key}"),
            ))
        })?;
        parsed.push((key.clone(), parse_parameter(key, value, spec.kind)?));
    }

    parsed.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(parsed)
}

fn parse_parameter(
    field_name: &str,
    value: &Value,
    kind: FieldKind,
) -> Result<SqlParameter, ApiError> {
    match kind {
        FieldKind::Text => parse_text_parameter(field_name, value),
        FieldKind::NullableText => {
            if value.is_null() {
                Ok(SqlParameter::Null)
            } else {
                parse_text_parameter(field_name, value)
            }
        }
        FieldKind::Integer => Ok(SqlParameter::Integer(parse_i64_value(field_name, value)?)),
        FieldKind::NullableInteger => {
            if value.is_null() {
                Ok(SqlParameter::Null)
            } else {
                Ok(SqlParameter::Integer(parse_i64_value(field_name, value)?))
            }
        }
        FieldKind::Real => Ok(SqlParameter::Real(parse_f64_value(field_name, value)?)),
        FieldKind::NullableReal => {
            if value.is_null() {
                Ok(SqlParameter::Null)
            } else {
                Ok(SqlParameter::Real(parse_f64_value(field_name, value)?))
            }
        }
        FieldKind::BooleanInteger => Ok(SqlParameter::Integer(parse_bool_i64_value(
            field_name, value,
        )?)),
    }
}

fn required_text_field(object: &Map<String, Value>, field_name: &str) -> Result<String, ApiError> {
    let value = object.get(field_name).ok_or_else(|| {
        ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            format!("Missing required field: {field_name}"),
        ))
    })?;
    parse_text_value(field_name, value)
}

fn parse_text_parameter(field_name: &str, value: &Value) -> Result<SqlParameter, ApiError> {
    Ok(SqlParameter::Text(parse_text_value(field_name, value)?))
}

fn parse_text_value(field_name: &str, value: &Value) -> Result<String, ApiError> {
    value.as_str().map(str::to_string).ok_or_else(|| {
        ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            format!("Expected string for field {field_name}"),
        ))
    })
}

fn parse_i64_value(field_name: &str, value: &Value) -> Result<i64, ApiError> {
    value.as_i64().ok_or_else(|| {
        ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            format!("Expected integer for field {field_name}"),
        ))
    })
}

fn parse_f64_value(field_name: &str, value: &Value) -> Result<f64, ApiError> {
    value.as_f64().ok_or_else(|| {
        ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            format!("Expected number for field {field_name}"),
        ))
    })
}

fn parse_bool_i64_value(field_name: &str, value: &Value) -> Result<i64, ApiError> {
    if let Some(boolean) = value.as_bool() {
        return Ok(i64::from(boolean));
    }

    let integer = parse_i64_value(field_name, value)?;
    if matches!(integer, 0 | 1) {
        Ok(integer)
    } else {
        Err(ApiError::bad_request(Error::new(
            ErrorKind::InvalidInput,
            format!("Expected boolean or 0/1 integer for field {field_name}"),
        )))
    }
}

async fn role_id_by_name(db: &Pool<Sqlite>, role_name: &str) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM role WHERE name = ? LIMIT 1")
        .bind(role_name)
        .fetch_one(db)
        .await
        .map_err(map_sqlx_error)
}

async fn ensure_role_exists(db: &Pool<Sqlite>, role_id: i64) -> Result<(), ApiError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM role WHERE id = ?")
        .bind(role_id)
        .fetch_one(db)
        .await
        .map_err(map_sqlx_error)?;

    if exists == 0 {
        return Err(ApiError::not_found("Role not found"));
    }

    Ok(())
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

fn authenticate_file_access(
    state: &Arc<HtmlState>,
    token: &str,
    expected_action: FileAccessAction,
) -> Result<FileAccessClaims, ApiError> {
    let claims = decode_file_access_token(state.jwt_secret.as_str(), token)
        .map_err(ApiError::unauthorized_err)?;
    if claims.action != expected_action {
        return Err(ApiError::unauthorized("File access token action mismatch"));
    }

    Ok(claims)
}

fn issue_file_link(
    secret: &str,
    key: &str,
    action: FileAccessAction,
) -> Result<FileLinkResponse, ApiError> {
    let token =
        issue_file_access_token(secret, key, action, Duration::from_secs(FILE_LINK_TTL_SECS))
            .map_err(ApiError::internal)?;

    Ok(FileLinkResponse {
        method: action.method(),
        url: format!("/api/files/access/{token}"),
        expires_in: FILE_LINK_TTL_SECS,
    })
}

fn validate_file_key(key: &str) -> Result<String, ApiError> {
    if key.trim().is_empty() {
        return Err(ApiError::bad_request(Error::new(
            std::io::ErrorKind::InvalidInput,
            "File key cannot be empty",
        )));
    }
    if key.starts_with('/') {
        return Err(ApiError::bad_request(Error::new(
            std::io::ErrorKind::InvalidInput,
            "File key cannot start with '/'",
        )));
    }

    Ok(key.to_string())
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

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn internal(error: Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }

    fn from_write(error: Error) -> Self {
        if error.to_string().contains("UNIQUE constraint failed") {
            return Self::conflict(error.to_string());
        }
        if matches!(error.kind(), ErrorKind::NotFound) {
            return Self::not_found(error.to_string());
        }

        Self::bad_request(error)
    }

    fn from_io(error: Error) -> Self {
        let status = match error.kind() {
            std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            std::io::ErrorKind::PermissionDenied => StatusCode::UNAUTHORIZED,
            std::io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Self {
            status,
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
