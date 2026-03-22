use {
    axum::{Json, http::StatusCode},
    serde::Serialize,
};

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

pub type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

pub fn api_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: message.into(),
        }),
    )
}

pub fn invalid_multipart(
    error: axum::extract::multipart::MultipartError,
) -> (StatusCode, Json<ApiError>) {
    tracing::warn!("multipart request failed: {error}");
    api_error(
        StatusCode::BAD_REQUEST,
        "Nao foi possivel ler o upload do widget HTML.",
    )
}
