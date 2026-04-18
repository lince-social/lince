use {
    crate::{
        application::state::AppState,
        infrastructure::terminal_store::{
            TerminalOutputChunk, TerminalResize, TerminalSessionSnapshot, TerminalSessionStore,
            TerminalStreamEvent,
        },
        presentation::http::api_error::{ApiResult, api_error},
    },
    axum::{
        Json,
        extract::{
            Path, Query, State,
            ws::{Message, WebSocket, WebSocketUpgrade},
        },
        http::StatusCode,
        response::IntoResponse,
    },
    futures::{Sink, SinkExt, StreamExt},
    serde::{Deserialize, Serialize},
    tokio::sync::broadcast,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInputRequest {
    pub input: Option<String>,
    pub input_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TerminalOutputQuery {
    pub cursor: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResizeRequest {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: Option<u16>,
    pub pixel_height: Option<u16>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSocketQuery {
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub pixel_width: Option<u16>,
    pub pixel_height: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum TerminalSocketCommand {
    Resize {
        cols: u16,
        rows: u16,
        pixel_width: Option<u16>,
        pixel_height: Option<u16>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum TerminalSocketFrame {
    Ready { session: TerminalSessionSnapshot },
    Snapshot { session: TerminalSessionSnapshot },
    Reset { session: TerminalSessionSnapshot },
    Closed { session: TerminalSessionSnapshot },
    Error { message: String },
}

pub async fn create_terminal_session(
    State(state): State<AppState>,
) -> ApiResult<Json<TerminalSessionSnapshot>> {
    let session = state
        .terminal
        .create_session()
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    Ok(Json(session))
}

pub async fn get_terminal_output(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<TerminalOutputQuery>,
) -> ApiResult<Json<TerminalOutputChunk>> {
    let output = state
        .terminal
        .read_output(&session_id, query.cursor.unwrap_or(0))
        .await
        .map_err(|message| api_error(StatusCode::NOT_FOUND, message))?;

    Ok(Json(output))
}

pub async fn post_terminal_input(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(payload): Json<TerminalInputRequest>,
) -> ApiResult<Json<TerminalSessionSnapshot>> {
    let session = if let Some(input_base64) = payload.input_base64.as_deref() {
        state
            .terminal
            .send_input_base64(&session_id, input_base64)
            .await
            .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?
    } else {
        state
            .terminal
            .send_input_text(&session_id, payload.input.as_deref().unwrap_or_default())
            .await
            .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
    };

    Ok(Json(session))
}

pub async fn post_terminal_resize(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(payload): Json<TerminalResizeRequest>,
) -> ApiResult<Json<TerminalSessionSnapshot>> {
    let session = state
        .terminal
        .resize(&session_id, resize_from_parts(
            payload.cols,
            payload.rows,
            payload.pixel_width,
            payload.pixel_height,
        ))
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    Ok(Json(session))
}

pub async fn delete_terminal_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    state
        .terminal
        .terminate(&session_id)
        .await
        .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn connect_terminal_socket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<TerminalSocketQuery>,
) -> impl IntoResponse {
    ws.max_frame_size(2 * 1024 * 1024)
        .max_message_size(2 * 1024 * 1024)
        .on_upgrade(move |socket| run_terminal_socket(socket, state.terminal.clone(), query))
}

async fn run_terminal_socket(
    mut socket: WebSocket,
    store: TerminalSessionStore,
    query: TerminalSocketQuery,
) {
    let session = match store.create_session_with_size(Some(query_resize(query))).await {
        Ok(session) => session,
        Err(message) => {
            let _ = send_socket_frame(&mut socket, TerminalSocketFrame::Error { message }).await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };

    let session_id = session.id.clone();
    let subscription = match store.subscribe_stream(&session_id).await {
        Ok(subscription) => subscription,
        Err(message) => {
            let _ = send_socket_frame(&mut socket, TerminalSocketFrame::Error { message }).await;
            let _ = store.terminate(&session_id).await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };

    let (mut sender, receiver) = socket.split();
    let session_snapshot = subscription.session.clone();

    if send_socket_frame(
        &mut sender,
        TerminalSocketFrame::Ready {
            session: session_snapshot,
        },
    )
    .await
    .is_err()
    {
        let _ = store.terminate(&session_id).await;
        return;
    }

    if !subscription.bytes.is_empty()
        && sender
            .send(Message::Binary(subscription.bytes.into()))
            .await
            .is_err()
    {
        let _ = store.terminate(&session_id).await;
        return;
    }

    let mut output_task = tokio::spawn(run_terminal_output_stream(
        sender,
        store.clone(),
        session_id.clone(),
        subscription.receiver,
        subscription.session.next_cursor,
    ));
    let mut input_task = tokio::spawn(run_terminal_input_stream(
        receiver,
        store.clone(),
        session_id.clone(),
    ));

    tokio::select! {
        _ = &mut output_task => {
            input_task.abort();
        }
        _ = &mut input_task => {
            output_task.abort();
        }
    }

    let _ = output_task.await;
    let _ = input_task.await;
    let _ = store.terminate(&session_id).await;
}

async fn run_terminal_output_stream(
    mut sender: futures::stream::SplitSink<WebSocket, Message>,
    store: TerminalSessionStore,
    session_id: String,
    mut receiver: broadcast::Receiver<TerminalStreamEvent>,
    mut cursor: usize,
) {
    loop {
        match receiver.recv().await {
            Ok(TerminalStreamEvent::Output) | Err(broadcast::error::RecvError::Lagged(_)) => {
                if stream_terminal_delta(&mut sender, &store, &session_id, &mut cursor)
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Ok(TerminalStreamEvent::Snapshot(session)) => {
                if send_socket_frame(&mut sender, TerminalSocketFrame::Snapshot { session })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Ok(TerminalStreamEvent::Closed(session)) => {
                if stream_terminal_delta(&mut sender, &store, &session_id, &mut cursor)
                    .await
                    .is_err()
                {
                    return;
                }
                let _ = send_socket_frame(&mut sender, TerminalSocketFrame::Closed { session }).await;
                return;
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn run_terminal_input_stream(
    mut receiver: futures::stream::SplitStream<WebSocket>,
    store: TerminalSessionStore,
    session_id: String,
) {
    while let Some(message) = receiver.next().await {
        let Ok(message) = message else {
            return;
        };

        match message {
            Message::Binary(bytes) => {
                if store.send_input_bytes(&session_id, &bytes).await.is_err() {
                    return;
                }
            }
            Message::Text(text) => {
                let Ok(command) = serde_json::from_str::<TerminalSocketCommand>(&text) else {
                    continue;
                };

                match command {
                    TerminalSocketCommand::Resize {
                        cols,
                        rows,
                        pixel_width,
                        pixel_height,
                    } => {
                        if store
                            .resize(
                                &session_id,
                                resize_from_parts(cols, rows, pixel_width, pixel_height),
                            )
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
            Message::Close(_) => return,
            Message::Ping(_) | Message::Pong(_) => {}
        }
    }
}

async fn stream_terminal_delta(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    store: &TerminalSessionStore,
    session_id: &str,
    cursor: &mut usize,
) -> Result<(), ()> {
    let chunk = store
        .read_output_bytes(session_id, *cursor)
        .await
        .map_err(|_| ())?;
    *cursor = chunk.session.next_cursor;

    if chunk.truncated {
        send_socket_frame(
            sender,
            TerminalSocketFrame::Reset {
                session: chunk.session.clone(),
            },
        )
        .await?;
    }

    if !chunk.bytes.is_empty() {
        sender
            .send(Message::Binary(chunk.bytes.into()))
            .await
            .map_err(|_| ())?;
    }

    Ok(())
}

async fn send_socket_frame<S>(
    sender: &mut S,
    frame: TerminalSocketFrame,
) -> Result<(), ()>
where
    S: Sink<Message> + Unpin,
{
    let payload = serde_json::to_string(&frame).map_err(|_| ())?;
    sender
        .send(Message::Text(payload.into()))
        .await
        .map_err(|_| ())
}

fn query_resize(query: TerminalSocketQuery) -> TerminalResize {
    resize_from_parts(query.cols.unwrap_or(80), query.rows.unwrap_or(24), query.pixel_width, query.pixel_height)
}

fn resize_from_parts(
    cols: u16,
    rows: u16,
    pixel_width: Option<u16>,
    pixel_height: Option<u16>,
) -> TerminalResize {
    TerminalResize {
        cols,
        rows,
        pixel_width: pixel_width.unwrap_or(0),
        pixel_height: pixel_height.unwrap_or(0),
    }
}
