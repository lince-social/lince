use {
    crate::{
        application::{
            kanban_actions::{
                CreateCommentRequest, CreateRecordRequest, CreateResourceRefRequest,
                DeleteCommentRequest, DeleteRecordRequest, DeleteResourceRefRequest,
                HeartbeatWorklogRequest, KanbanActionError, LoadRecordDetailRequest,
                MoveRecordRequest, StartWorklogRequest, StopWorklogRequest, UpdateCommentRequest,
                UpdateParentRelationRequest, UpdateRecordBodyRequest, UpdateRecordRequest,
            },
            kanban_filters::{KanbanFilterError, RawKanbanFilterRow, UpdateKanbanSettingsRequest},
            kanban_render::{
                KanbanRenderError, render_error_payload, render_mismatch_payload,
                render_sync_payload,
            },
            kanban_streams::{KanbanStreamError, PreparedKanbanStream},
            state::AppState,
            widget_runtime::WidgetRuntimeError,
        },
        infrastructure::auth::{parse_cookie_header, session_cookie_name},
        presentation::http::api_error::{ApiResult, api_error},
    },
    async_stream::stream,
    axum::{
        Json,
        extract::{Path, State},
        http::{HeaderMap, StatusCode, header},
        response::{
            IntoResponse, Response,
            sse::{Event, KeepAlive, Sse},
        },
    },
    serde::Deserialize,
    serde_json::{Value, json},
    std::{convert::Infallible, time::Duration},
};

pub async fn get_widget_contract(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
) -> ApiResult<Json<crate::application::widget_runtime::KanbanWidgetContract>> {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );

    let mut contract = state
        .widget_runtime
        .kanban_contract(session_token.as_deref(), &instance_id)
        .await
        .map_err(map_widget_runtime_error)?;
    let form_options = state
        .kanban_actions
        .load_form_options(session_token.as_deref(), &instance_id)
        .await
        .map_err(map_kanban_action_error)?;
    contract.form_options = Some(form_options);

    Ok(Json(contract))
}

pub async fn get_widget_stream(
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

    let prepared = state
        .kanban_streams
        .prepare_stream(session_token.as_deref(), &instance_id)
        .await
        .map_err(map_kanban_stream_error)?;

    let response = match prepared {
        PreparedKanbanStream::Local {
            mut handle,
            settings,
        } => {
            let stream = stream! {
                while let Some(frame) = handle.rx.recv().await {
                    let (event_name, data) = match frame {
                        ::application::subscription::SseFrame::Snapshot { payload } => {
                            ("snapshot", payload)
                        }
                        ::application::subscription::SseFrame::Error { payload } => {
                            ("error", payload)
                        }
                    };

                    for event in render_widget_stream_events(
                        event_name,
                        &data,
                        settings.show_parent_context,
                    ) {
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
        PreparedKanbanStream::Remote { response, settings } => {
            let stream = stream! {
                let mut response = response;
                let mut parser = SseEventParser::default();
                loop {
                    match response.chunk().await {
                        Ok(Some(chunk)) => {
                            for event in parser.push_chunk(&chunk) {
                                for rendered in render_widget_stream_events(
                                    &event.event_name,
                                    &event.data,
                                    settings.show_parent_context,
                                ) {
                                    yield Ok::<_, Infallible>(rendered);
                                }
                            }
                        }
                        Ok(None) => {
                            for event in parser.finish() {
                                for rendered in render_widget_stream_events(
                                    &event.event_name,
                                    &event.data,
                                    settings.show_parent_context,
                                ) {
                                    yield Ok::<_, Infallible>(rendered);
                                }
                            }
                            break;
                        }
                        Err(error) => {
                            for event in render_widget_stream_events(
                                "error",
                                &json!({ "error": format!("Nao foi possivel ler o stream remoto da view: {error}") }).to_string(),
                                settings.show_parent_context,
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
    };

    Ok(response)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetActionRequest {
    #[serde(default)]
    pub filters: Vec<RawKanbanFilterRow>,
}

pub async fn post_widget_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((instance_id, action)): Path<(String, String)>,
    Json(payload): Json<Value>,
) -> ApiResult<Json<Value>> {
    let session_token = parse_cookie_header(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        session_cookie_name(),
    );
    match action.as_str() {
        "apply-filters" => {
            let payload: WidgetActionRequest =
                serde_json::from_value(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_filters
                .apply_filters(&instance_id, payload.filters)
                .await
                .map_err(map_kanban_filter_error)?;
            Ok(Json(serde_json::json!({
                "ok": true,
                "action": action,
                "message": "Filters applied.",
                "record_id": Value::Null,
                "await_stream_refresh": true,
                "detail": {
                    "filters_version": outcome.filters_version
                }
            })))
        }
        "update-settings" => {
            let request = serde_json::from_value::<UpdateKanbanSettingsRequest>(payload).map_err(
                |error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                },
            )?;
            let settings = state
                .kanban_filters
                .update_settings(&instance_id, request)
                .await
                .map_err(map_kanban_filter_error)?;
            Ok(Json(serde_json::json!({
                "ok": true,
                "action": action,
                "message": "Settings updated.",
                "record_id": Value::Null,
                "await_stream_refresh": true,
                "detail": {
                    "settings": settings
                }
            })))
        }
        "create-record" => {
            let request =
                serde_json::from_value::<CreateRecordRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .create_record(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "update-record" => {
            let request =
                serde_json::from_value::<UpdateRecordRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .update_record(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "update-record-body" => {
            let request =
                serde_json::from_value::<UpdateRecordBodyRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .update_record_body(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "set-parent" => {
            let request = serde_json::from_value::<UpdateParentRelationRequest>(payload).map_err(
                |error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                },
            )?;
            let outcome = state
                .kanban_actions
                .set_parent_relation(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "move-record" => {
            let request =
                serde_json::from_value::<MoveRecordRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .move_record(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "delete-record" => {
            let request =
                serde_json::from_value::<DeleteRecordRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .delete_record(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "load-record-detail" => {
            let request =
                serde_json::from_value::<LoadRecordDetailRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let detail = state
                .kanban_actions
                .load_record_detail(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(detail))
        }
        "load-form-options" => {
            let detail = state
                .kanban_actions
                .load_form_options(session_token.as_deref(), &instance_id)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(detail))
        }
        "start-worklog" => {
            let request =
                serde_json::from_value::<StartWorklogRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .start_worklog(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "stop-worklog" => {
            let request =
                serde_json::from_value::<StopWorklogRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .stop_worklog(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "heartbeat-worklog" => {
            let request =
                serde_json::from_value::<HeartbeatWorklogRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .heartbeat_worklog(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "create-comment" => {
            let request =
                serde_json::from_value::<CreateCommentRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .create_comment(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "update-comment" => {
            let request =
                serde_json::from_value::<UpdateCommentRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .update_comment(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "delete-comment" => {
            let request =
                serde_json::from_value::<DeleteCommentRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .delete_comment(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "create-resource-ref" => {
            let request =
                serde_json::from_value::<CreateResourceRefRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .create_resource_ref(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        "delete-resource-ref" => {
            let request =
                serde_json::from_value::<DeleteResourceRefRequest>(payload).map_err(|error| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Payload invalido: {error}"),
                    )
                })?;
            let outcome = state
                .kanban_actions
                .delete_resource_ref(session_token.as_deref(), &instance_id, request)
                .await
                .map_err(map_kanban_action_error)?;
            Ok(Json(outcome.into_json_value()))
        }
        _ => Err(api_error(
            StatusCode::NOT_FOUND,
            "Acao de widget desconhecida.",
        )),
    }
}

fn map_widget_runtime_error(
    error: WidgetRuntimeError,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    match error {
        WidgetRuntimeError::NotFound(message) => api_error(StatusCode::NOT_FOUND, message),
        WidgetRuntimeError::Misconfigured(message) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
        }
        WidgetRuntimeError::Unsupported(message) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
        }
        WidgetRuntimeError::Internal(message) => {
            api_error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

fn map_kanban_filter_error(
    error: KanbanFilterError,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    match error {
        KanbanFilterError::NotFound(message) => api_error(StatusCode::NOT_FOUND, message),
        KanbanFilterError::Unsupported(message) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
        }
        KanbanFilterError::Invalid(message) => api_error(StatusCode::BAD_REQUEST, message),
        KanbanFilterError::Internal(message) => {
            api_error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

fn map_kanban_stream_error(
    error: KanbanStreamError,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    match error {
        KanbanStreamError::NotFound(message) => api_error(StatusCode::NOT_FOUND, message),
        KanbanStreamError::Misconfigured(message)
        | KanbanStreamError::Disabled(message)
        | KanbanStreamError::Invalid(message) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
        }
        KanbanStreamError::Unauthorized(message) => api_error(StatusCode::UNAUTHORIZED, message),
        KanbanStreamError::Forbidden(message) => api_error(StatusCode::FORBIDDEN, message),
        KanbanStreamError::BadGateway(message) => api_error(StatusCode::BAD_GATEWAY, message),
        KanbanStreamError::Internal(message) => {
            api_error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

fn map_kanban_action_error(
    error: KanbanActionError,
) -> (
    StatusCode,
    Json<crate::presentation::http::api_error::ApiError>,
) {
    match error {
        KanbanActionError::NotFound(message) => api_error(StatusCode::NOT_FOUND, message),
        KanbanActionError::Misconfigured(message) | KanbanActionError::Validation(message) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
        }
        KanbanActionError::Unauthorized(message) => api_error(StatusCode::UNAUTHORIZED, message),
        KanbanActionError::Forbidden(message) => api_error(StatusCode::FORBIDDEN, message),
        KanbanActionError::BadGateway(message) => api_error(StatusCode::BAD_GATEWAY, message),
        KanbanActionError::Internal(message) => {
            api_error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

impl crate::application::kanban_actions::KanbanActionOutcome {
    fn into_json_value(self) -> Value {
        serde_json::json!({
            "ok": true,
            "action": self.action,
            "message": self.message,
            "record_id": self.record_id,
            "await_stream_refresh": self.await_stream_refresh,
            "detail": self.detail,
        })
    }
}

#[derive(Default)]
struct SseEventParser {
    buffer: String,
}

impl SseEventParser {
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

fn render_widget_stream_events(
    event_name: &str,
    data: &str,
    show_parent_context: bool,
) -> Vec<Event> {
    let mut events = vec![Event::default().event(event_name).data(data.to_string())];

    match event_name {
        "snapshot" => match render_sync_payload(data, show_parent_context) {
            Ok(rendered) => {
                events.push(
                    Event::default().event("kanban-sync").data(
                        json!({
                            "ok": true,
                            "html": rendered.html,
                            "summary": rendered.summary,
                            "view": rendered.view,
                        })
                        .to_string(),
                    ),
                );
            }
            Err(KanbanRenderError::ShapeMismatch {
                expected_columns,
                received_columns,
            }) => {
                events.push(
                    Event::default()
                        .event("kanban-error")
                        .data(json!({
                            "ok": false,
                            "reason": "mismatch",
                            "message": "The Kanban stream did not satisfy the Record-centric contract.",
                            "html": render_mismatch_payload(&expected_columns, &received_columns),
                        }).to_string()),
                );
            }
            Err(KanbanRenderError::InvalidPayload(message)) => {
                events.push(
                    Event::default()
                        .event("kanban-error")
                        .data(json!({
                            "ok": false,
                            "reason": "invalid_payload",
                            "message": message,
                            "html": render_error_payload("The widget could not parse the SSE payload."),
                        }).to_string()),
                );
            }
        },
        "error" => {
            let message = serde_json::from_str::<Value>(data)
                .ok()
                .and_then(|value| {
                    value
                        .get("error")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "The backend stream reported an error.".into());
            events.push(
                Event::default().event("kanban-error").data(
                    json!({
                        "ok": false,
                        "reason": "stream_error",
                        "message": message,
                        "html": render_error_payload(&message),
                    })
                    .to_string(),
                ),
            );
        }
        _ => {}
    }

    events
}
