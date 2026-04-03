use {
    crate::{
        application::state::AppState,
        infrastructure::{
            auth::{parse_cookie_header, session_cookie_name},
            organ_store::{Organ, organ_requires_auth},
        },
        presentation::http::api_error::{ApiResult, api_error},
    },
    ::application::{auth::AuthSubject, subscription::SseFrame},
    async_stream::stream,
    axum::{
        Json,
        body::to_bytes,
        extract::Request,
        extract::{Path, Query, State},
        http::{HeaderMap, HeaderValue, Method, StatusCode, header},
        response::{IntoResponse, Response},
    },
    serde::Deserialize,
    serde_json::{Value, json},
    std::{convert::Infallible, io::Error, io::ErrorKind},
    tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream},
    tokio_util::io::ReaderStream,
};

const MAX_PROXY_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct FilePathQuery {
    path: String,
}

pub async fn proxy_manas_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((server_id, view_id)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    let session_token = current_session_token(&headers);
    let server = load_organ(&state, &server_id).await?;
    if !organ_requires_auth(&server, state.local_auth_required) {
        return local_view_stream(&state, view_id).await;
    }
    let bearer_token = extract_manas_token(&state, &headers, &server_id).await?;
    let response = state
        .manas
        .open_view_stream(&server.base_url, &bearer_token, view_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        state
            .auth
            .expire_server_session(
                session_token.as_deref(),
                &server_id,
                "Sessao remota expirada. Conecte esse servidor novamente.",
            )
            .await;
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Sessao remota expirada. Conecte esse servidor novamente.",
        ));
    }

    if !response.status().is_success() {
        let status =
            StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let body = response.text().await.unwrap_or_default();
        return Err(api_error(
            status,
            if body.trim().is_empty() {
                format!("Stream remoto recusou a conexao com status {status}.")
            } else {
                body
            },
        ));
    }

    let stream = stream! {
        let mut response = response;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => yield Result::<_, std::io::Error>::Ok(chunk),
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!("manas proxy stream read failed: {error}");
                    yield Err(std::io::Error::other("Nao foi possivel ler o stream remoto da view."));
                    break;
                }
            }
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));

    Ok((headers, axum::body::Body::from_stream(stream)).into_response())
}

pub async fn proxy_manas_view_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((server_id, view_id)): Path<(String, u64)>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let session_token = current_session_token(&headers);
    let server = load_organ(&state, &server_id).await?;
    if !organ_requires_auth(&server, state.local_auth_required) {
        let snapshot = state
            .backend
            .read_view_snapshot(&local_host_subject(), view_id as u32)
            .await
            .map_err(map_backend_error)?;
        return Ok((StatusCode::OK, Json(snapshot)));
    }

    let bearer_token = extract_manas_token(&state, &headers, &server_id).await?;
    let response = state
        .manas
        .send_backend_request(
            &server.base_url,
            &bearer_token,
            Method::GET,
            &format!("/api/view/{view_id}/snapshot"),
            None,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    proxy_json_response(&state, session_token.as_deref(), &server_id, response).await
}

pub async fn proxy_manas_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Query(query): Query<FilePathQuery>,
) -> ApiResult<impl IntoResponse> {
    let session_token = current_session_token(&headers);
    let server = load_organ(&state, &server_id).await?;
    let path = normalize_file_key(&query.path)?;

    if !organ_requires_auth(&server, state.local_auth_required) {
        let downloaded = state
            .backend
            .download_file(&path)
            .await
            .map_err(map_backend_error)?;
        return Ok(downloaded_response(downloaded));
    }

    let bearer_token = extract_manas_token(&state, &headers, &server_id).await?;
    let link_response = state
        .manas
        .send_backend_request(
            &server.base_url,
            &bearer_token,
            Method::POST,
            "/api/files/download-link",
            Some(json!({ "key": path })),
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    if link_response.status().as_u16() == 401 {
        state
            .auth
            .expire_server_session(
                session_token.as_deref(),
                &server_id,
                "Sessao remota expirada. Conecte esse servidor novamente.",
            )
            .await;
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Sessao remota expirada. Conecte esse servidor novamente.",
        ));
    }

    if !link_response.status().is_success() {
        let status = StatusCode::from_u16(link_response.status().as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY);
        let body = link_response.text().await.unwrap_or_default();
        return Err(api_error(
            status,
            if body.trim().is_empty() {
                format!("Nao foi possivel gerar o link do arquivo com status {status}.")
            } else {
                body
            },
        ));
    }

    let link_payload = link_response
        .json::<Value>()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
    let Some(download_url) = link_payload.get("url").and_then(Value::as_str) else {
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            "Resposta invalida ao gerar o link do arquivo.",
        ));
    };

    let file_response = state
        .manas
        .send_backend_request(
            &server.base_url,
            &bearer_token,
            Method::GET,
            download_url,
            None,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    if file_response.status().as_u16() == 401 {
        state
            .auth
            .expire_server_session(
                session_token.as_deref(),
                &server_id,
                "Sessao remota expirada. Conecte esse servidor novamente.",
            )
            .await;
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Sessao remota expirada. Conecte esse servidor novamente.",
        ));
    }

    if !file_response.status().is_success() {
        let status = StatusCode::from_u16(file_response.status().as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY);
        let body = file_response.text().await.unwrap_or_default();
        return Err(api_error(
            status,
            if body.trim().is_empty() {
                format!("Nao foi possivel ler o arquivo com status {status}.")
            } else {
                body
            },
        ));
    }

    Ok(download_response(file_response).await)
}

pub async fn proxy_manas_table_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Path((server_id, table_name)): Path<(String, String)>,
    request: Request,
) -> ApiResult<impl IntoResponse> {
    let session_token = current_session_token(&headers);
    let server = load_organ(&state, &server_id).await?;
    let body = request_json_body(request).await?;
    if !organ_requires_auth(&server, state.local_auth_required) {
        return local_table_collection(&state, method, &table_name, body).await;
    }
    let bearer_token = extract_manas_token(&state, &headers, &server_id).await?;
    let response = state
        .manas
        .send_table_request(
            &server.base_url,
            &bearer_token,
            method,
            &table_name,
            None,
            body,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    proxy_json_response(&state, session_token.as_deref(), &server_id, response).await
}

pub async fn proxy_manas_table_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Path((server_id, table_name, id)): Path<(String, String, i64)>,
    request: Request,
) -> ApiResult<impl IntoResponse> {
    let session_token = current_session_token(&headers);
    let server = load_organ(&state, &server_id).await?;
    let body = request_json_body(request).await?;
    if !organ_requires_auth(&server, state.local_auth_required) {
        return local_table_item(&state, method, &table_name, id, body).await;
    }
    let bearer_token = extract_manas_token(&state, &headers, &server_id).await?;
    let response = state
        .manas
        .send_table_request(
            &server.base_url,
            &bearer_token,
            method,
            &table_name,
            Some(id),
            body,
        )
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    proxy_json_response(&state, session_token.as_deref(), &server_id, response).await
}

fn current_session_token(headers: &HeaderMap) -> Option<String> {
    parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    )
}

fn normalize_file_key(raw_path: &str) -> ApiResult<String> {
    let path = raw_path.trim().trim_start_matches('/').to_string();
    if path.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Informe um path valido dentro do bucket.",
        ));
    }

    Ok(path)
}

fn downloaded_response(downloaded: persistence::storage::DownloadedObject) -> Response {
    let persistence::storage::DownloadedObject {
        body,
        content_type,
        content_length,
        filename,
    } = downloaded;
    let mut response = Response::new(axum::body::Body::from_stream(ReaderStream::new(
        body.into_async_read(),
    )));

    if let Some(content_type) = content_type.as_deref()
        && let Ok(value) = HeaderValue::from_str(content_type)
    {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    if let Some(content_length) = content_length
        && let Ok(value) = HeaderValue::from_str(&content_length.to_string())
    {
        response.headers_mut().insert(header::CONTENT_LENGTH, value);
    }
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename)) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }

    response
}

async fn download_response(downloaded: reqwest::Response) -> Response {
    let headers = downloaded.headers().clone();
    let mut downloaded = downloaded;
    let mut response = Response::new(axum::body::Body::from_stream(async_stream::stream! {
        loop {
            match downloaded.chunk().await {
                Ok(Some(chunk)) => yield Ok::<_, Error>(chunk),
                Ok(None) => break,
                Err(error) => {
                    yield Err(Error::other(error));
                    break;
                }
            }
        }
    }));

    if let Some(content_type) = headers.get(header::CONTENT_TYPE)
        && let Ok(value) = content_type.to_str()
        && let Ok(header_value) = HeaderValue::from_str(value)
    {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, header_value);
    }
    if let Some(content_length) = headers.get(header::CONTENT_LENGTH)
        && let Ok(value) = content_length.to_str()
        && let Ok(header_value) = HeaderValue::from_str(value)
    {
        response
            .headers_mut()
            .insert(header::CONTENT_LENGTH, header_value);
    }
    if let Some(content_disposition) = headers.get(header::CONTENT_DISPOSITION)
        && let Ok(value) = content_disposition.to_str()
        && let Ok(header_value) = HeaderValue::from_str(value)
    {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, header_value);
    }

    response
}

async fn extract_manas_token(
    state: &AppState,
    headers: &HeaderMap,
    server_id: &str,
) -> ApiResult<String> {
    let session_token = current_session_token(headers);
    let Some(session) = state
        .auth
        .server_session(session_token.as_deref(), server_id)
        .await
    else {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Essa sessao local nao esta conectada a esse servidor.",
        ));
    };

    Ok(session.bearer_token)
}

async fn local_view_stream(state: &AppState, view_id: u64) -> ApiResult<Response> {
    let handle = state
        .backend
        .subscribe_view(local_host_subject(), view_id as u32)
        .await
        .map_err(map_backend_error)?;
    let stream = UnboundedReceiverStream::new(handle.rx).map(|frame| {
        Ok::<_, Infallible>(match frame {
            SseFrame::Snapshot { payload } => axum::response::sse::Event::default()
                .event("snapshot")
                .data(payload),
            SseFrame::Error { payload } => axum::response::sse::Event::default()
                .event("error")
                .data(payload),
        })
    });

    Ok(axum::response::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response())
}

async fn local_table_collection(
    state: &AppState,
    method: Method,
    table_name: &str,
    body: Option<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let claims = local_host_subject();
    let payload = match method {
        Method::GET => state
            .backend
            .list_table_rows(&claims, table_name)
            .await
            .map_err(map_backend_error)?,
        Method::POST => {
            let object = payload_object(body.as_ref())?;
            let outcome = state
                .backend
                .create_table_row(&claims, table_name, object)
                .await
                .map_err(map_backend_error)?;
            serde_json::json!({
                "ok": true,
                "rows_affected": outcome.rows_affected,
                "last_insert_rowid": outcome.last_insert_rowid,
            })
        }
        _ => {
            return Err(api_error(
                StatusCode::METHOD_NOT_ALLOWED,
                "Metodo nao suportado para esse recurso local.",
            ));
        }
    };
    let status = if method == Method::POST {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(payload)))
}

async fn local_table_item(
    state: &AppState,
    method: Method,
    table_name: &str,
    id: i64,
    body: Option<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let claims = local_host_subject();
    let payload = match method {
        Method::GET => state
            .backend
            .get_table_row(&claims, table_name, id)
            .await
            .map_err(map_backend_error)?,
        Method::PATCH => {
            let object = payload_object(body.as_ref())?;
            let outcome = state
                .backend
                .update_table_row(&claims, table_name, id, object)
                .await
                .map_err(map_backend_error)?;
            serde_json::json!({
                "ok": true,
                "rows_affected": outcome.rows_affected,
                "last_insert_rowid": outcome.last_insert_rowid,
            })
        }
        Method::DELETE => {
            let outcome = state
                .backend
                .delete_table_row(&claims, table_name, id)
                .await
                .map_err(map_backend_error)?;
            serde_json::json!({
                "ok": true,
                "rows_affected": outcome.rows_affected,
                "last_insert_rowid": outcome.last_insert_rowid,
            })
        }
        _ => {
            return Err(api_error(
                StatusCode::METHOD_NOT_ALLOWED,
                "Metodo nao suportado para esse recurso local.",
            ));
        }
    };
    Ok((StatusCode::OK, Json(payload)))
}

fn local_host_subject() -> AuthSubject {
    AuthSubject {
        user_id: 0,
        username: "local-host".into(),
        role_id: 0,
        role: "admin".into(),
    }
}

async fn request_json_body(request: Request) -> ApiResult<Option<Value>> {
    match *request.method() {
        Method::GET | Method::DELETE => Ok(None),
        _ => {
            let bytes = to_bytes(request.into_body(), MAX_PROXY_BODY_BYTES)
                .await
                .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))?;
            if bytes.is_empty() {
                return Ok(None);
            }

            serde_json::from_slice::<Value>(&bytes)
                .map(Some)
                .map_err(|error| api_error(StatusCode::BAD_REQUEST, error.to_string()))
        }
    }
}

async fn proxy_json_response(
    state: &AppState,
    session_token: Option<&str>,
    server_id: &str,
    response: reqwest::Response,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if status == StatusCode::UNAUTHORIZED {
        state
            .auth
            .expire_server_session(
                session_token,
                server_id,
                "Sessao remota expirada. Conecte esse servidor novamente.",
            )
            .await;
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
    Ok((status, Json(payload)))
}

fn payload_object(payload: Option<&Value>) -> ApiResult<&serde_json::Map<String, Value>> {
    payload
        .and_then(Value::as_object)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Expected a JSON object payload"))
}

async fn load_organ(state: &AppState, server_id: &str) -> ApiResult<Organ> {
    state
        .organs
        .get(server_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Servidor nao encontrado."))
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
