use {
    crate::{
        application::state::AppState,
        application::trail_widget::{PreparedTrailStream, TrailWidgetError},
        infrastructure::auth::{parse_cookie_header, session_cookie_name},
        presentation::{
            http::api_error::{ApiResult, api_error},
            pages::render_trail_page,
        },
    },
    ::application::subscription::SseFrame,
    async_stream::stream,
    axum::{
        extract::{Path, State},
        http::{HeaderMap, StatusCode, header},
        response::{
            Html, IntoResponse, Response,
            sse::{Event, KeepAlive, Sse},
        },
    },
    serde_json::{Value, json},
    std::convert::Infallible,
};

pub async fn get_trail_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
) -> ApiResult<Html<String>> {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let (session_token, _created) = state.auth.ensure_session(session_token.as_deref()).await;
    let contract = state
        .trail_widget
        .contract(Some(session_token.as_str()), &instance_id)
        .await
        .map_err(map_trail_widget_error)?;

    Ok(Html(render_trail_page(&instance_id, &contract)))
}

pub async fn get_trail_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
) -> ApiResult<Response> {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    let (session_token, _created) = state.auth.ensure_session(session_token.as_deref()).await;
    let prepared = state
        .trail_widget
        .prepare_stream(Some(session_token.as_str()), &instance_id)
        .await
        .map_err(map_trail_widget_error)?;

    Ok(render_datastar_trail_stream(prepared))
}

fn render_datastar_trail_stream(prepared: PreparedTrailStream) -> Response {
    match prepared {
        PreparedTrailStream::Local { mut handle, .. } => {
            let stream = stream! {
                while let Some(frame) = handle.rx.recv().await {
                    yield Ok::<_, Infallible>(trail_datastar_event(frame));
                }
            };
            Sse::new(stream)
                .keep_alive(
                    KeepAlive::new()
                        .interval(std::time::Duration::from_secs(15))
                        .text("heartbeat"),
                )
                .into_response()
        }
        PreparedTrailStream::Remote { response, .. } => {
            let stream = stream! {
                let mut response = response;
                let mut parser = SseParser::default();
                loop {
                    match response.chunk().await {
                        Ok(Some(chunk)) => {
                            for frame in parser.push_chunk(&chunk) {
                                if let Some(event) = trail_datastar_remote_event(&frame) {
                                    yield Ok::<_, Infallible>(event);
                                }
                            }
                        }
                        Ok(None) => {
                            for frame in parser.finish() {
                                if let Some(event) = trail_datastar_remote_event(&frame) {
                                    yield Ok::<_, Infallible>(event);
                                }
                            }
                            break;
                        }
                        Err(error) => {
                            yield Ok::<_, Infallible>(trail_error_event(
                                format!("Nao foi possivel ler o stream remoto da trail view: {error}"),
                            ));
                            break;
                        }
                    }
                }
            };
            Sse::new(stream)
                .keep_alive(
                    KeepAlive::new()
                        .interval(std::time::Duration::from_secs(15))
                        .text("heartbeat"),
                )
                .into_response()
        }
    }
}

fn trail_datastar_event(frame: SseFrame) -> Event {
    match frame {
        SseFrame::Snapshot { payload } => trail_snapshot_event(&payload),
        SseFrame::Error { payload } => trail_error_event(extract_error_message(&payload)),
    }
}

fn trail_datastar_remote_event(frame: &DecodedSseEvent) -> Option<Event> {
    match frame.event_name.as_str() {
        "snapshot" => Some(trail_snapshot_event(&frame.data)),
        "error" => Some(trail_error_event(extract_error_message(&frame.data))),
        _ => None,
    }
}

fn trail_snapshot_event(payload: &str) -> Event {
    let snapshot = serde_json::from_str::<Value>(payload).unwrap_or_else(|_| json!({}));
    trail_patch_event(json!({
        "trail": {
            "binding": {
                "snapshot": snapshot,
            },
            "stream": {
                "status": "live",
                "error": null,
            }
        }
    }))
}

fn trail_error_event(message: String) -> Event {
    trail_patch_event(json!({
        "trail": {
            "stream": {
                "status": "error",
                "error": message,
            }
        }
    }))
}

fn trail_patch_event(patch: Value) -> Event {
    Event::default()
        .event("datastar-patch-signals")
        .data(patch.to_string())
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

fn map_trail_widget_error(
    error: TrailWidgetError,
) -> (
    StatusCode,
    axum::Json<crate::presentation::http::api_error::ApiError>,
) {
    match error {
        TrailWidgetError::NotFound(message) => api_error(StatusCode::NOT_FOUND, message),
        TrailWidgetError::Misconfigured(message)
        | TrailWidgetError::Disabled(message)
        | TrailWidgetError::Invalid(message) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
        }
        TrailWidgetError::Unauthorized(message) => api_error(StatusCode::UNAUTHORIZED, message),
        TrailWidgetError::Forbidden(message) => api_error(StatusCode::FORBIDDEN, message),
        TrailWidgetError::BadGateway(message) => api_error(StatusCode::BAD_GATEWAY, message),
        TrailWidgetError::Internal(message) => {
            api_error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
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
