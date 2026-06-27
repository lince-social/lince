use {
    crate::{application::state::AppState, presentation::http::api_error::api_error},
    axum::{
        extract::{
            State,
            ws::{Message, WebSocket, WebSocketUpgrade},
        },
        http::{HeaderMap, StatusCode, header},
        response::IntoResponse,
    },
    futures::{SinkExt, StreamExt},
    serde::Serialize,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum RecordSyncSocketFrame {
    Hello {
        resource: &'static str,
        mode: &'static str,
    },
    Operation {
        operation: RecordSyncOperationFrame,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct RecordSyncOperationFrame {
    operation_uid: String,
    source_organ_id: i64,
    actor_user_id: Option<i64>,
    root_record_sync_uid: String,
    table_name: String,
    row_sync_uid: String,
    action: String,
    field_payload_json: String,
    operation_clock: String,
    source_operation_uid: Option<String>,
    created_at: String,
    applied_at: Option<String>,
    sent_at: Option<String>,
}

pub async fn connect_record_sync_socket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let claims = match state.backend.authenticate_authorization(auth_header).await {
        Ok(claims) => claims,
        Err(error) => {
            return api_error(StatusCode::UNAUTHORIZED, error.to_string()).into_response();
        }
    };
    if let Err(error) =
        claims.require_permission(::application::auth::PermissionKey::new("record", "read"))
    {
        return api_error(StatusCode::FORBIDDEN, error.to_string()).into_response();
    }

    ws.max_frame_size(2 * 1024 * 1024)
        .max_message_size(2 * 1024 * 1024)
        .on_upgrade(move |socket| run_record_sync_socket(socket, state))
        .into_response()
}

async fn run_record_sync_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    if send_socket_frame(
        &mut sender,
        RecordSyncSocketFrame::Hello {
            resource: "record",
            mode: "catch_up_read",
        },
    )
    .await
    .is_err()
    {
        return;
    }

    let operations = match load_recent_operations(&state).await {
        Ok(operations) => operations,
        Err(error) => {
            let _ = send_socket_frame(
                &mut sender,
                RecordSyncSocketFrame::Error {
                    message: error.to_string(),
                },
            )
            .await;
            return;
        }
    };

    for operation in operations {
        if send_socket_frame(
            &mut sender,
            RecordSyncSocketFrame::Operation { operation },
        )
        .await
        .is_err()
        {
            return;
        }
    }

    while let Some(message) = receiver.next().await {
        match message {
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(Message::Ping(bytes)) => {
                if sender.send(Message::Pong(bytes)).await.is_err() {
                    break;
                }
            }
            _ => {}
        }
    }
}

async fn load_recent_operations(
    state: &AppState,
) -> Result<Vec<RecordSyncOperationFrame>, sqlx::Error> {
    sqlx::query_as::<_, RecordSyncOperationFrame>(
        "SELECT
            operation_uid,
            source_organ_id,
            actor_user_id,
            root_record_sync_uid,
            table_name,
            row_sync_uid,
            action,
            field_payload_json,
            operation_clock,
            source_operation_uid,
            created_at,
            applied_at,
            sent_at
         FROM record_sync_operation
         ORDER BY operation_clock, source_organ_id, operation_uid
         LIMIT 500",
    )
    .fetch_all(&*state.services.db)
    .await
}

async fn send_socket_frame<S>(sender: &mut S, frame: RecordSyncSocketFrame) -> Result<(), ()>
where
    S: futures::Sink<Message> + Unpin,
{
    let payload = serde_json::to_string(&frame).map_err(|_| ())?;
    sender.send(Message::Text(payload.into())).await.map_err(|_| ())
}
