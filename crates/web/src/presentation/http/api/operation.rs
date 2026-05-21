use {
    crate::{
        application::state::AppState,
        presentation::http::api_error::{ApiResult, api_error},
    },
    axum::{Json, extract::State, http::StatusCode},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRequest {
    pub operation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResponse {
    pub parsed_count: usize,
}

pub async fn post_operation(
    State(state): State<AppState>,
    Json(payload): Json<OperationRequest>,
) -> ApiResult<Json<OperationResponse>> {
    let operation = payload.operation.trim();
    if operation.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Operation cannot be empty.",
        ));
    }

    let parsed =
        ::application::operation::operation_execute(state.services.clone(), operation.to_string())
            .await
            .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;

    Ok(Json(OperationResponse {
        parsed_count: parsed.len(),
    }))
}
