pub mod configuration;
pub mod configuration_view;
pub mod record;
pub mod section;
// pub mod table;
pub mod tui;
pub mod view;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::io::{Error, ErrorKind};

pub fn error_into_response(error: Error) -> Response {
    let status = match error.kind() {
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
        ErrorKind::NotConnected => StatusCode::UNAUTHORIZED,
        ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (
        status,
        Json(json!({
            "message": error.to_string(),
        })),
    )
        .into_response()
}
