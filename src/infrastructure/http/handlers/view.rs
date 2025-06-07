use crate::{
    application::providers::view::{
        toggle_collection_id::provider_view_toggle_collection_id,
        toggle_view_id::provider_view_toggle_view_id,
    },
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::section::body::presentation_web_section_body,
};
use axum::extract::{Path, State};

pub async fn handler_view_toggle_view_id(
    State(services): State<InjectedServices>,
    Path((collection_id, view_id)): Path<(String, String)>,
) -> String {
    let _ = provider_view_toggle_view_id(collection_id, view_id).await;
    presentation_web_section_body(services).await
}

pub async fn handler_view_toggle_collection_id(
    State(services): State<InjectedServices>,
    Path(id): Path<String>,
) -> String {
    let _ = provider_view_toggle_collection_id(id).await;
    presentation_web_section_body(services).await
}
