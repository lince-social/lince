use {
    crate::{application::state::AppState, presentation::http::api_error::ApiResult},
    axum::{
        Json,
        extract::{Path, State},
        http::StatusCode,
    },
    injection::cross_cutting::AppNotification,
    serde::Serialize,
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
