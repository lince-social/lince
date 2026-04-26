use crate::{
    application::backend_api::{FileLink, RecordQuantityBatchUpdateRequest},
    application::state::AppState,
    infrastructure::backend_api_store::{TableCreateSchemaResponse, TableListQuery},
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
use utoipa::{IntoParams, ToSchema};

const MAX_FILE_UPLOAD_BYTES: usize = 1024 * 1024 * 1024;

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    token: String,
    token_type: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct MutationResponse {
    ok: bool,
    rows_affected: u64,
    last_insert_rowid: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    row: Option<Value>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct FileListQuery {
    prefix: Option<String>,
    limit: Option<i32>,
    cursor: Option<String>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct TableSchemaQuery {
    preferred_table: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct FileKeyRequest {
    key: String,
}

#[derive(Serialize, ToSchema)]
pub struct FileLinkResponse {
    method: &'static str,
    url: String,
    expires_in: u64,
}

pub fn router() -> Router<AppState> {
    Router::<AppState>::new()
        .route("/auth/login", post(login))
        .route("/table/schema", get(list_table_create_schemas))
        .route(
            "/table/{table}",
            get(list_table_rows)
                .post(create_table_row)
                .patch(update_table_rows),
        )
        .route(
            "/table/{table}/{id}",
            get(get_table_row)
                .patch(update_table_row)
                .delete(delete_table_row),
        )
        .route(
            "/table/record/quantities",
            post(batch_update_record_quantities),
        )
        .route("/karma", get(list_karma_rows).post(create_karma_row))
        .route(
            "/karma/{id}",
            get(get_karma_row)
                .patch(update_karma_row)
                .delete(delete_karma_row),
        )
        .route("/karma/{id}/execute", post(execute_karma))
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
        .route("/view/{view_id}/snapshot", get(view_snapshot))
        .route("/sse/view/{view_id}", get(view_sse))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        )
}

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Bearer token issued", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = crate::presentation::http::api_error::ApiError)
    )
)]
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

#[utoipa::path(
    get,
    path = "/table/schema",
    tag = "table",
    params(TableSchemaQuery),
    responses(
        (status = 200, description = "Writable table schemas returned", body = TableCreateSchemaResponse),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn list_table_create_schemas(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TableSchemaQuery>,
) -> ApiResult<Json<TableCreateSchemaResponse>> {
    let _claims = authenticate_request(&state, &headers).await?;
    let response = state
        .backend
        .table_create_schema_response(query.preferred_table.as_deref());
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/table/{table_name}",
    tag = "table",
    params(
        ("table_name" = String, Path, description = "Table name"),
        TableListQuery
    ),
    responses(
        (status = 200, description = "Rows returned as JSON"),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn list_table_rows(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table_name): Path<String>,
    Query(query): Query<TableListQuery>,
) -> ApiResult<Json<Value>> {
    let claims = authenticate_request(&state, &headers).await?;
    let value = state
        .backend
        .list_table_rows_filtered(&claims, &table_name, &query)
        .await
        .map_err(map_backend_error)?;
    Ok(Json(value))
}

#[utoipa::path(
    get,
    path = "/table/{table_name}/{id}",
    tag = "table",
    params(
        ("table_name" = String, Path, description = "Table name"),
        ("id" = i64, Path, description = "Row identifier")
    ),
    responses(
        (status = 200, description = "Single row returned as JSON"),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "Row not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn get_table_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
) -> ApiResult<Json<Value>> {
    let claims = authenticate_request(&state, &headers).await?;
    let value = state
        .backend
        .get_table_row(&claims, &table_name, id)
        .await
        .map_err(map_backend_error)?;
    Ok(Json(value))
}

#[utoipa::path(
    post,
    path = "/table/{table_name}",
    tag = "table",
    params(("table_name" = String, Path, description = "Table name")),
    request_body = serde_json::Value,
    responses(
        (status = 201, description = "Row created", body = MutationResponse),
        (status = 400, description = "Invalid payload", body = crate::presentation::http::api_error::ApiError),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn create_table_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table_name): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult<(StatusCode, Json<MutationResponse>)> {
    let claims = authenticate_request(&state, &headers).await?;
    let object = payload_object(&payload)?;
    let outcome = state
        .backend
        .create_table_row(&claims, &table_name, object)
        .await
        .map_err(map_backend_error)?;
    let row = load_mutation_row(&state, &claims, &table_name, outcome.last_insert_rowid).await;

    Ok((
        StatusCode::CREATED,
        Json(MutationResponse {
            ok: true,
            rows_affected: outcome.rows_affected,
            last_insert_rowid: outcome.last_insert_rowid,
            row,
        }),
    ))
}

#[utoipa::path(
    patch,
    path = "/table/{table_name}",
    tag = "table",
    params(("table_name" = String, Path, description = "Table name")),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rows updated", body = MutationResponse),
        (status = 400, description = "Invalid payload", body = crate::presentation::http::api_error::ApiError),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn update_table_rows(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table_name): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    let objects = payload_object_array(&payload)?;
    let outcome = state
        .backend
        .update_table_rows(&claims, &table_name, &objects)
        .await
        .map_err(map_backend_error)?;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
        last_insert_rowid: outcome.last_insert_rowid,
        row: None,
    }))
}

#[utoipa::path(
    patch,
    path = "/table/{table_name}/{id}",
    tag = "table",
    params(
        ("table_name" = String, Path, description = "Table name"),
        ("id" = i64, Path, description = "Row identifier")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Row updated", body = MutationResponse),
        (status = 400, description = "Invalid payload", body = crate::presentation::http::api_error::ApiError),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "Row not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn update_table_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
    Json(payload): Json<Value>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    let object = payload_object(&payload)?;
    let outcome = state
        .backend
        .update_table_row(&claims, &table_name, id, object)
        .await
        .map_err(map_backend_error)?;
    let row = load_mutation_row(&state, &claims, &table_name, Some(id)).await;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
        last_insert_rowid: outcome.last_insert_rowid,
        row,
    }))
}

#[utoipa::path(
    delete,
    path = "/table/{table_name}/{id}",
    tag = "table",
    params(
        ("table_name" = String, Path, description = "Table name"),
        ("id" = i64, Path, description = "Row identifier")
    ),
    responses(
        (status = 200, description = "Row deleted", body = MutationResponse),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "Row not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn delete_table_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((table_name, id)): Path<(String, i64)>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    let outcome = state
        .backend
        .delete_table_row(&claims, &table_name, id)
        .await
        .map_err(map_backend_error)?;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
        last_insert_rowid: outcome.last_insert_rowid,
        row: None,
    }))
}

#[utoipa::path(
    post,
    path = "/table/record/quantities",
    tag = "table",
    responses(
        (status = 200, description = "Quantities updated", body = MutationResponse),
        (status = 400, description = "Invalid payload", body = crate::presentation::http::api_error::ApiError),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn batch_update_record_quantities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RecordQuantityBatchUpdateRequest>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    let outcome = state
        .backend
        .batch_update_record_quantities(&claims, request)
        .await
        .map_err(map_backend_error)?;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
        last_insert_rowid: outcome.last_insert_rowid,
        row: None,
    }))
}

#[utoipa::path(
    get,
    path = "/karma",
    tag = "karma",
    params(TableListQuery),
    responses(
        (status = 200, description = "Karma rows returned as JSON"),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn list_karma_rows(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TableListQuery>,
) -> ApiResult<Json<Value>> {
    let claims = authenticate_request(&state, &headers).await?;
    let value = state
        .backend
        .list_table_rows_filtered(&claims, "karma", &query)
        .await
        .map_err(map_backend_error)?;
    Ok(Json(value))
}

#[utoipa::path(
    get,
    path = "/karma/{id}",
    tag = "karma",
    params(("id" = i64, Path, description = "Karma row identifier")),
    responses(
        (status = 200, description = "Karma row returned as JSON"),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "Row not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn get_karma_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let claims = authenticate_request(&state, &headers).await?;
    let value = state
        .backend
        .get_table_row(&claims, "karma", id)
        .await
        .map_err(map_backend_error)?;
    Ok(Json(value))
}

#[utoipa::path(
    post,
    path = "/karma",
    tag = "karma",
    request_body = serde_json::Value,
    responses(
        (status = 201, description = "Karma row created", body = MutationResponse),
        (status = 400, description = "Invalid payload", body = crate::presentation::http::api_error::ApiError),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn create_karma_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> ApiResult<(StatusCode, Json<MutationResponse>)> {
    let claims = authenticate_request(&state, &headers).await?;
    let object = payload_object(&payload)?;
    let outcome = state
        .backend
        .create_table_row(&claims, "karma", object)
        .await
        .map_err(map_backend_error)?;
    let row = load_mutation_row(&state, &claims, "karma", outcome.last_insert_rowid).await;

    Ok((
        StatusCode::CREATED,
        Json(MutationResponse {
            ok: true,
            rows_affected: outcome.rows_affected,
            last_insert_rowid: outcome.last_insert_rowid,
            row,
        }),
    ))
}

#[utoipa::path(
    patch,
    path = "/karma/{id}",
    tag = "karma",
    params(("id" = i64, Path, description = "Karma row identifier")),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Karma row updated", body = MutationResponse),
        (status = 400, description = "Invalid payload", body = crate::presentation::http::api_error::ApiError),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "Row not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn update_karma_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<Value>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    let object = payload_object(&payload)?;
    let outcome = state
        .backend
        .update_table_row(&claims, "karma", id, object)
        .await
        .map_err(map_backend_error)?;
    let row = load_mutation_row(&state, &claims, "karma", Some(id)).await;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
        last_insert_rowid: outcome.last_insert_rowid,
        row,
    }))
}

#[utoipa::path(
    delete,
    path = "/karma/{id}",
    tag = "karma",
    params(("id" = i64, Path, description = "Karma row identifier")),
    responses(
        (status = 200, description = "Karma row deleted", body = MutationResponse),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "Row not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn delete_karma_row(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    let outcome = state
        .backend
        .delete_table_row(&claims, "karma", id)
        .await
        .map_err(map_backend_error)?;

    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: outcome.rows_affected,
        last_insert_rowid: outcome.last_insert_rowid,
        row: None,
    }))
}

#[utoipa::path(
    post,
    path = "/karma/{id}/execute",
    tag = "karma",
    params(("id" = i64, Path, description = "Karma row identifier")),
    responses(
        (status = 200, description = "Karma rule executed", body = MutationResponse),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "Row not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn execute_karma(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Json<MutationResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    state
        .backend
        .execute_karma(&claims, id)
        .await
        .map_err(map_backend_error)?;
    Ok(Json(MutationResponse {
        ok: true,
        rows_affected: 0,
        last_insert_rowid: None,
        row: None,
    }))
}

#[utoipa::path(
    get,
    path = "/files",
    tag = "files",
    params(FileListQuery),
    responses(
        (status = 200, description = "File listing returned"),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn list_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FileListQuery>,
) -> ApiResult<Json<persistence::storage::StorageList>> {
    let claims = authenticate_request(&state, &headers).await?;
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

#[utoipa::path(
    post,
    path = "/files/upload-link",
    tag = "files",
    request_body = FileKeyRequest,
    responses(
        (status = 200, description = "Upload link issued", body = FileLinkResponse),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn upload_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> ApiResult<Json<FileLinkResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    Ok(Json(map_file_link(
        state
            .backend
            .issue_file_link(&claims, &request.key, FileAccessAction::Upload)
            .map_err(map_backend_error)?,
    )))
}

#[utoipa::path(
    post,
    path = "/files/download-link",
    tag = "files",
    request_body = FileKeyRequest,
    responses(
        (status = 200, description = "Download link issued", body = FileLinkResponse),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn download_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> ApiResult<Json<FileLinkResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    Ok(Json(map_file_link(
        state
            .backend
            .issue_file_link(&claims, &request.key, FileAccessAction::Download)
            .map_err(map_backend_error)?,
    )))
}

#[utoipa::path(
    post,
    path = "/files/delete-link",
    tag = "files",
    request_body = FileKeyRequest,
    responses(
        (status = 200, description = "Delete link issued", body = FileLinkResponse),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn delete_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FileKeyRequest>,
) -> ApiResult<Json<FileLinkResponse>> {
    let claims = authenticate_request(&state, &headers).await?;
    Ok(Json(map_file_link(
        state
            .backend
            .issue_file_link(&claims, &request.key, FileAccessAction::Delete)
            .map_err(map_backend_error)?,
    )))
}

#[utoipa::path(
    put,
    path = "/files/access/{token}",
    tag = "files",
    params(("token" = String, Path, description = "Access token")),
    responses(
        (status = 204, description = "File uploaded"),
        (status = 400, description = "Invalid upload", body = crate::presentation::http::api_error::ApiError)
    )
)]
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

#[utoipa::path(
    get,
    path = "/files/access/{token}",
    tag = "files",
    params(("token" = String, Path, description = "Access token")),
    responses(
        (status = 200, description = "File downloaded"),
        (status = 404, description = "Token not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
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

#[utoipa::path(
    delete,
    path = "/files/access/{token}",
    tag = "files",
    params(("token" = String, Path, description = "Access token")),
    responses(
        (status = 204, description = "File link deleted"),
        (status = 404, description = "Token not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
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

#[utoipa::path(
    get,
    path = "/view/{view_id}/snapshot",
    tag = "view",
    params(("view_id" = u32, Path, description = "View identifier")),
    responses(
        (status = 200, description = "View snapshot returned as JSON"),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "View not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn view_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(view_id): Path<u32>,
) -> ApiResult<Json<Value>> {
    let claims = authenticate_request(&state, &headers).await?;
    let snapshot = state
        .backend
        .read_view_snapshot(&claims, view_id)
        .await
        .map_err(map_backend_error)?;
    Ok(Json(snapshot))
}

#[utoipa::path(
    get,
    path = "/sse/view/{view_id}",
    tag = "view",
    params(("view_id" = u32, Path, description = "View identifier")),
    responses(
        (status = 200, description = "Server-sent events stream"),
        (status = 401, description = "Missing or invalid authorization", body = crate::presentation::http::api_error::ApiError),
        (status = 404, description = "View not found", body = crate::presentation::http::api_error::ApiError)
    )
)]
async fn view_sse(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(view_id): Path<u32>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let claims = authenticate_request(&state, &headers).await?;
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

async fn authenticate_request(
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
        .await
        .map_err(|error| api_error(StatusCode::UNAUTHORIZED, error.to_string()))
}

async fn load_mutation_row(
    state: &AppState,
    claims: &::application::auth::AuthSubject,
    table_name: &str,
    row_id: Option<i64>,
) -> Option<Value> {
    let row_id = row_id?;
    state
        .backend
        .get_table_row(claims, table_name, row_id)
        .await
        .ok()
}

fn payload_object(payload: &Value) -> ApiResult<&Map<String, Value>> {
    payload
        .as_object()
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Expected a JSON object payload"))
}

fn payload_object_array(payload: &Value) -> ApiResult<Vec<Map<String, Value>>> {
    let rows = payload
        .as_array()
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Expected a JSON array payload"))?;
    rows.iter()
        .map(|value| {
            value.as_object().cloned().ok_or_else(|| {
                api_error(
                    StatusCode::BAD_REQUEST,
                    "Expected every batch item to be a JSON object",
                )
            })
        })
        .collect()
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
