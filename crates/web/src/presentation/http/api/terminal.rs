use {
    crate::{
        application::state::AppState,
        infrastructure::terminal_store::{TerminalOutputChunk, TerminalSessionSnapshot},
        presentation::http::api_error::{ApiResult, api_error},
    },
    axum::{
        Json,
        extract::{Path, Query, State},
        http::StatusCode,
    },
    serde::Deserialize,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInputRequest {
    pub input: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalOutputQuery {
    pub cursor: Option<usize>,
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
    let session = state
        .terminal
        .send_input(&session_id, &payload.input)
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
