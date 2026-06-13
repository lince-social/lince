use {
    crate::{
        application::state::AppState,
        presentation::http::api_error::{ApiResult, api_error},
    },
    axum::{
        Json,
        extract::{Path, State},
        http::StatusCode,
    },
    injection::cross_cutting::AppNotification,
    serde::Serialize,
    std::time::Duration,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsResponse {
    notifications: Vec<AppNotification>,
}

pub async fn list_notifications(
    State(state): State<AppState>,
) -> ApiResult<Json<NotificationsResponse>> {
    Ok(Json(NotificationsResponse {
        notifications: state.services.notifications.list(),
    }))
}

pub async fn dismiss_notification(
    State(state): State<AppState>,
    Path(notification_id): Path<String>,
) -> ApiResult<StatusCode> {
    state.services.notifications.dismiss(&notification_id);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn install_update(State(state): State<AppState>) -> ApiResult<StatusCode> {
    ::application::automatic_update::install_and_restart_later(
        state.services.clone(),
        Duration::from_millis(500),
    )
    .await
    .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
    Ok(StatusCode::ACCEPTED)
}
