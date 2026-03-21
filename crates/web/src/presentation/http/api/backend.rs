use crate::{
    application::backend_api::FileLink,
    application::state::AppState,
    presentation::http::api_error::{ApiResult, api_error},
};
use ::application::subscription::SseFrame;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    convert::Infallible,
    io::{Error, ErrorKind},
};
use tokio_stream::{Stream, StreamExt, wrappers::UnboundedReceiverStream};
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};
use utils::file_access::FileAccessAction;

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

pub fn router() -> Router<AppState> {
    Router::<AppState>::new()
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
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> ApiResult<Json<LoginResponse>> {
    let token = state
        .backend
        .login(&request.username, &request.password)
        .await
        .map_err(|error| api_error(StatusCode::UNAUTHORIZED, error.to_string()))?;

    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer",
    }))
}

async fn list_table_rows(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table_name): Path<String>,
) -> ApiResult<Json<Value>> {
    let claims = authenticate_request(&state, &headers)?;
    let value = state
        .backend
        .list_table_rows(&claims, &table_name)
        .await
        .map_err(map_backend_error)?;
    Ok(Json(value))
}

async fn get_table_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
) -> ApiResult<Json<Value>> {
    let claims = authenticate_request(&state, &headers)?;
    let value = state
        .backend
        .get_table_row(&claims, &table_name, id)
        .await
        .map_err(map_backend_error)?;
    Ok(Json(value))
}

async fn create_table_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table_name): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult<(StatusCode, Json<MutationResponse>)> {
    let claims = authenticate_request(&state, &headers)?;
    let object = payload_object(&payload)?;
    let outcome = state
        .backend
        .create_table_row(&claims, &table_name, object)
        .await
        .map_err(map_backend_error)?;

    Ok((
        StatusCode::CREATED,
        Json(MutationResponse {
            ok: true,
            rows_affected: outcome.rows_affected,
        }),
    ))
}

async fn update_table_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
    Json(payload): Json<Value>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers)?;
    let object = payload_object(&payload)?;
    let outcome = state
        .backend
        .update_table_row(&claims, &table_name, id, object)
        .await
        .map_err(map_backend_error)?;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
    }))
}

async fn delete_table_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers)?;
    let outcome = state
        .backend
        .delete_table_row(&claims, &table_name, id)
        .await
        .map_err(map_backend_error)?;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
    }))
}

async fn list_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FileListQuery>,
) -> ApiResult<Json<persistence::storage::StorageList>> {
    let claims = authenticate_request(&state, &headers)?;
    let listing = state
        .backend
        .list_files(
            &claims,
            query.prefix.as_deref(),
            query.limit.unwrap_or(100),
            query.cursor.as_deref(),
        )
        .await
        .map_err(map_backend_error)?;
    Ok(Json(listing))
}

async fn upload_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> ApiResult<Json<FileLinkResponse>> {
    let claims = authenticate_request(&state, &headers)?;
    Ok(Json(map_file_link(
        state
            .backend
            .issue_file_link(&claims, &request.key, FileAccessAction::Upload)
            .map_err(map_backend_error)?,
    )))
}

async fn download_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> ApiResult<Json<FileLinkResponse>> {
    let claims = authenticate_request(&state, &headers)?;
    Ok(Json(map_file_link(
        state
            .backend
            .issue_file_link(&claims, &request.key, FileAccessAction::Download)
            .map_err(map_backend_error)?,
    )))
}

async fn delete_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> ApiResult<Json<FileLinkResponse>> {
    let claims = authenticate_request(&state, &headers)?;
    Ok(Json(map_file_link(
        state
            .backend
            .issue_file_link(&claims, &request.key, FileAccessAction::Delete)
            .map_err(map_backend_error)?,
    )))
}

async fn upload_via_link(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> ApiResult<StatusCode> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(request.into_body(), MAX_FILE_UPLOAD_BYTES)
        .await
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
    state
        .backend
        .upload_via_link(&token, bytes.to_vec(), content_type.as_deref())
        .await
        .map_err(map_backend_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn download_via_link(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let downloaded = state
        .backend
        .download_via_link(&token)
        .await
        .map_err(map_backend_error)?;
    let mut response = axum::response::Response::new(Body::from_stream(ReaderStream::new(
        downloaded.body.into_async_read(),
    )));

    if let Some(content_type) = downloaded.content_type.as_deref()
        && let Ok(value) = HeaderValue::from_str(content_type)
    {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    if let Some(content_length) = downloaded.content_length
        && let Ok(value) = HeaderValue::from_str(&content_length.to_string())
    {
        response.headers_mut().insert(header::CONTENT_LENGTH, value);
    }
    if let Ok(value) =
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", downloaded.filename))
    {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }

    Ok(response)
}

async fn delete_via_link(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<StatusCode> {
    state
        .backend
        .delete_via_link(&token)
        .await
        .map_err(map_backend_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn view_sse(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(view_id): Path<u32>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let claims = authenticate_request(&state, &headers)?;
    let handle = state
        .backend
        .subscribe_view(claims, view_id)
        .await
        .map_err(map_backend_error)?;
    let stream = UnboundedReceiverStream::new(handle.rx).map(|frame| {
        Ok(match frame {
            SseFrame::Snapshot { payload } => Event::default().event("snapshot").data(payload),
            SseFrame::Error { payload } => Event::default().event("error").data(payload),
        })
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn authenticate_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<
    ::application::auth::AuthSubject,
    (
        StatusCode,
        Json<crate::presentation::http::api_error::ApiError>,
    ),
> {
    let header_value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "Missing Authorization header"))?;
    let header_str = header_value
        .to_str()
        .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "Invalid Authorization header"))?;
    state
        .backend
        .authenticate_authorization(header_str)
        .map_err(|error| api_error(StatusCode::UNAUTHORIZED, error.to_string()))
}

fn payload_object(payload: &Value) -> ApiResult<&Map<String, Value>> {
    payload
        .as_object()
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Expected a JSON object payload"))
}

fn map_file_link(link: FileLink) -> FileLinkResponse {
    FileLinkResponse {
        method: link.method,
        url: link.url,
        expires_in: link.expires_in,
    }
}

fn map_backend_error(
    error: Error,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    let status = match error.kind() {
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
        ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    api_error(status, error.to_string())
}
