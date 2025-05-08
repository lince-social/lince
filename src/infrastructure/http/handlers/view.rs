use crate::{
    application::providers::view::{
        toggle_configuration_id::provider_view_toggle_configuration_id,
        toggle_view_id::provider_view_toggle_view_id,
    },
    presentation::web::section::body::presentation_web_section_body,
};
use axum::extract::Path;

pub async fn handler_view_toggle_view_id(
    Path((configuration_id, view_id)): Path<(String, String)>,
) -> String {
    let _ = provider_view_toggle_view_id(configuration_id, view_id).await;
    presentation_web_section_body().await
}

pub async fn handler_view_toggle_configuration_id(Path(id): Path<String>) -> String {
    let _ = provider_view_toggle_configuration_id(id).await;
    presentation_web_section_body().await
}
