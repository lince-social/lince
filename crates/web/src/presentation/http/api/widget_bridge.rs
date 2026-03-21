use {
    crate::{
        application::state::AppState,
        domain::widget_bridge::WidgetBridgeSnapshot,
        presentation::http::api_error::ApiResult,
    },
    axum::{Json, extract::State},
    serde::Deserialize,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintActionRequest {
    pub instance_id: String,
    pub label: Option<String>,
}

pub async fn get_widget_bridge_state(
    State(state): State<AppState>,
) -> ApiResult<Json<WidgetBridgeSnapshot>> {
    Ok(Json(state.widget_bridge.snapshot().await))
}

pub async fn post_widget_bridge_print(
    State(state): State<AppState>,
    Json(payload): Json<PrintActionRequest>,
) -> ApiResult<Json<WidgetBridgeSnapshot>> {
    let snapshot = state
        .widget_bridge
        .record_print(
            payload.instance_id,
            payload.label.unwrap_or_else(|| "print".into()),
        )
        .await;

    Ok(Json(snapshot))
}
