use {
    crate::{
        application::state::AppState,
        application::view_table_render::{
            ViewTableRenderContext, ViewTableRenderedHtml, render_error_payload,
            render_sync_payload,
        },
        infrastructure::{
            auth::{parse_cookie_header, session_cookie_name},
            organ_store::{Organ, organ_requires_auth},
        },
        presentation::http::api_error::{ApiResult, api_error},
    },
    ::application::{
        auth::AuthSubject,
        subscription::{SseFrame, SubscriptionHandle},
    },
    async_stream::stream,
    axum::{
        Json,
        body::to_bytes,
        extract::Request,
        extract::{Path, Query, State},
        http::{HeaderMap, HeaderValue, Method, StatusCode, header},
        response::{
            IntoResponse, Response,
            sse::{Event, KeepAlive, Sse},
        },
    },
    serde::Deserialize,
    serde_json::{Value, json},
    std::{convert::Infallible, io::Error, io::ErrorKind, time::Duration},
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

pub async fn proxy_manas_view_table_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((server_id, view_id)): Path<(String, u64)>,
) -> ApiResult<Response> {
    let session_token = current_session_token(&headers);
    let server = load_organ(&state, &server_id).await?;

    if !organ_requires_auth(&server, state.local_auth_required) {
        return local_view_table_stream(&state, &server_id, view_id).await;
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

    Ok(render_view_table_stream(
        ViewTableStreamSource::Remote { response },
        ViewTableRenderContext { server_id, view_id },
    ))
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

async fn local_view_table_stream(
    state: &AppState,
    server_id: &str,
    view_id: u64,
) -> ApiResult<Response> {
    let handle = state
        .backend
        .subscribe_view(local_host_subject(), view_id as u32)
        .await
        .map_err(map_backend_error)?;

    Ok(render_view_table_stream(
        ViewTableStreamSource::Local { handle },
        ViewTableRenderContext {
            server_id: server_id.to_string(),
            view_id,
        },
    ))
}

fn render_view_table_stream(
    source: ViewTableStreamSource,
    context: ViewTableRenderContext,
) -> Response {
    match source {
        ViewTableStreamSource::Local { mut handle } => {
            let stream = stream! {
                while let Some(frame) = handle.rx.recv().await {
                    for event in render_view_table_frame(context.clone(), frame) {
                        yield Ok::<_, Infallible>(event);
                    }
                }
            };

            Sse::new(stream)
                .keep_alive(
                    KeepAlive::new()
                        .interval(Duration::from_secs(15))
                        .text("heartbeat"),
                )
                .into_response()
        }
        ViewTableStreamSource::Remote { response } => {
            let stream = stream! {
                let mut response = response;
                let mut parser = SseParser::default();
                loop {
                    match response.chunk().await {
                        Ok(Some(chunk)) => {
                            for frame in parser.push_chunk(&chunk) {
                                for event in render_view_table_remote_frame(context.clone(), &frame) {
                                    yield Ok::<_, Infallible>(event);
                                }
                            }
                        }
                        Ok(None) => {
                            for frame in parser.finish() {
                                for event in render_view_table_remote_frame(context.clone(), &frame) {
                                    yield Ok::<_, Infallible>(event);
                                }
                            }
                            break;
                        }
                        Err(error) => {
                            for event in render_view_table_error_events(
                                context.clone(),
                                format!("Nao foi possivel ler o stream remoto da view: {error}"),
                            ) {
                                yield Ok::<_, Infallible>(event);
                            }
                            break;
                        }
                    }
                }
            };

            Sse::new(stream)
                .keep_alive(
                    KeepAlive::new()
                        .interval(Duration::from_secs(15))
                        .text("heartbeat"),
                )
                .into_response()
        }
    }
}

fn render_view_table_frame(context: ViewTableRenderContext, frame: SseFrame) -> Vec<Event> {
    match frame {
        SseFrame::Snapshot { payload } => match render_sync_payload(context.clone(), &payload) {
            Ok(rendered) => render_view_table_html_events(rendered.html),
            Err(error) => render_view_table_error_events(context, error.to_string()),
        },
        SseFrame::Error { payload } => {
            render_view_table_error_events(context, extract_error_message(&payload))
        }
    }
}

fn render_view_table_remote_frame(
    context: ViewTableRenderContext,
    frame: &DecodedSseEvent,
) -> Vec<Event> {
    match frame.event_name.as_str() {
        "snapshot" => match render_sync_payload(context.clone(), &frame.data) {
            Ok(rendered) => render_view_table_html_events(rendered.html),
            Err(error) => render_view_table_error_events(context, error.to_string()),
        },
        "error" => render_view_table_error_events(context, extract_error_message(&frame.data)),
        _ => Vec::new(),
    }
}

fn render_view_table_html_events(html: ViewTableRenderedHtml) -> Vec<Event> {
    vec![
        render_datastar_patch_event("#table-status", "outer", html.status_pill),
        render_datastar_patch_event("#table-details", "inner", html.details_panel),
        render_datastar_patch_event("#table-body", "inner", html.table_body),
    ]
}

fn render_view_table_error_events(context: ViewTableRenderContext, message: String) -> Vec<Event> {
    render_view_table_html_events(render_error_payload(context, &message))
}

fn render_datastar_patch_event(selector: &str, mode: &str, elements: String) -> Event {
    Event::default().event("datastar-patch-elements").data(
        json!({
            "selector": selector,
            "mode": mode,
            "namespace": "html",
            "elements": elements,
        })
        .to_string(),
    )
}

fn extract_error_message(payload: &str) -> String {
    serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| payload.to_string())
}

enum ViewTableStreamSource {
    Local { handle: SubscriptionHandle },
    Remote { response: reqwest::Response },
}

#[derive(Default)]
struct SseParser {
    buffer: String,
}

impl SseParser {
    fn push_chunk(&mut self, chunk: &[u8]) -> Vec<DecodedSseEvent> {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
        self.drain_events(false)
    }

    fn finish(&mut self) -> Vec<DecodedSseEvent> {
        self.drain_events(true)
    }

    fn drain_events(&mut self, flush_tail: bool) -> Vec<DecodedSseEvent> {
        let mut normalized = self.buffer.replace("\r\n", "\n");
        let mut events = Vec::new();

        while let Some(index) = normalized.find("\n\n") {
            let frame = normalized[..index].to_string();
            normalized.drain(..index + 2);
            if let Some(event) = parse_sse_frame(&frame) {
                events.push(event);
            }
        }

        if flush_tail {
            if let Some(event) = parse_sse_frame(normalized.trim_end_matches('\n')) {
                events.push(event);
            }
            self.buffer.clear();
        } else {
            self.buffer = normalized;
        }

        events
    }
}

#[derive(Debug, Clone)]
struct DecodedSseEvent {
    event_name: String,
    data: String,
}

fn parse_sse_frame(frame: &str) -> Option<DecodedSseEvent> {
    let mut event_name = None;
    let mut data_lines = Vec::new();

    for line in frame.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        return None;
    }

    Some(DecodedSseEvent {
        event_name: event_name
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "message".into()),
        data: data_lines.join("\n"),
    })
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
            let row = load_local_mutation_row(state, table_name, outcome.last_insert_rowid).await;
            serde_json::json!({
                "ok": true,
                "rows_affected": outcome.rows_affected,
                "last_insert_rowid": outcome.last_insert_rowid,
                "row": row,
            })
        }
        Method::PATCH => {
            let objects = payload_array_of_objects(body.as_ref())?;
            let outcome = state
                .backend
                .update_table_rows(&claims, table_name, &objects)
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
            let row = load_local_mutation_row(state, table_name, Some(id)).await;
            serde_json::json!({
                "ok": true,
                "rows_affected": outcome.rows_affected,
                "last_insert_rowid": outcome.last_insert_rowid,
                "row": row,
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

async fn load_local_mutation_row(
    state: &AppState,
    table_name: &str,
    row_id: Option<i64>,
) -> Option<Value> {
    let row_id = row_id?;
    state
        .backend
        .get_table_row(&local_host_subject(), table_name, row_id)
        .await
        .ok()
}

fn payload_array_of_objects(
    payload: Option<&Value>,
) -> ApiResult<Vec<serde_json::Map<String, Value>>> {
    let value = payload
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Expected a JSON array payload."))?;
    let rows = value
        .as_array()
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Expected a JSON array payload."))?;
    rows.iter()
        .map(|entry| {
            entry.as_object().cloned().ok_or_else(|| {
                api_error(
                    StatusCode::BAD_REQUEST,
                    "Expected every batch item to be a JSON object.",
                )
            })
        })
        .collect()
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
